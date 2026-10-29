//! Controller loop: read single-word commands from stdin and switch fixtures.
use crate::catalog::Catalog;
use crate::cli::{Options, USAGE};
use crate::session::Session;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub fn run(options: Options) -> Result<(), String> {
    let mut catalog = Catalog::load()?;
    if let Some(start) = &options.start {
        catalog.select(start)?;
    }
    if options.list_only {
        println!("{}", catalog.listing());
        return Ok(());
    }
    std::fs::create_dir_all(&options.out).map_err(|e| e.to_string())?;
    let out = std::fs::canonicalize(&options.out).map_err(|e| e.to_string())?;
    let mut session = Session::start(&out, options.chrome.clone())?;
    let show = |catalog: &Catalog, session: &mut Session| -> Result<(), String> {
        let prepared = catalog.prepare(&out, &options.tailwind)?;
        println!("[{}/{}]", catalog.index, catalog.len() - 1);
        session.show(&prepared)
    };
    show(&catalog, &mut session)?;
    println!("{USAGE}");
    let lines = stdin_lines();
    let deadline = options
        .seconds
        .map(|s| Instant::now() + Duration::from_secs(s));
    loop {
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Ok(());
        }
        let command = match lines.recv_timeout(Duration::from_millis(500)) {
            Ok(line) => line,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            // stdin closed (not a terminal): keep the windows until --seconds or a signal.
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        };
        let outcome = match command.trim() {
            "" => continue,
            "q" | "quit" => return Ok(()),
            "l" | "list" => {
                println!("{}", catalog.listing());
                continue;
            }
            "n" | "next" => catalog.step(true),
            "p" | "prev" => catalog.step(false),
            "r" | "reload" => Ok(()),
            key => catalog.select(key),
        }
        .and_then(|()| show(&catalog, &mut session));
        if let Err(error) = outcome {
            eprintln!("{error}");
        }
        for label in session.exited() {
            eprintln!("{label} window exited; press r to reopen");
        }
    }
}

fn stdin_lines() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}
