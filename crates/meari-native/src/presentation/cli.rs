use crate::{
    application,
    domain::{Viewport, widgets::Action},
    infrastructure::{artifacts, document::InteractiveDocument, tailwind::TailwindCompiler},
};
use meari::{
    application::Language,
    domain::design::{theme::Theme, utilities::COLORS},
};
use std::{collections::BTreeMap, env, fs, path::Path, process::ExitCode};

fn showcase() -> Result<(meari::domain::Component, Theme), String> {
    let component = meari::parse(include_str!("../../fixtures/showcase.tsx"), Language::Tsx)
        .map_err(|e| e.message)?;
    let mut light: BTreeMap<String, [u8; 4]> = COLORS
        .iter()
        .map(|n| ((*n).into(), [244, 244, 245, 255]))
        .collect();
    for (name, color) in [
        ("background", [255, 255, 255, 255]),
        ("foreground", [24, 24, 27, 255]),
        ("card", [255, 255, 255, 255]),
        ("card-foreground", [24, 24, 27, 255]),
        ("primary", [24, 24, 27, 255]),
        ("primary-foreground", [250, 250, 250, 255]),
        ("muted-foreground", [113, 113, 122, 255]),
        ("border", [228, 228, 231, 255]),
        ("input", [228, 228, 231, 255]),
        ("popover", [255, 255, 255, 255]),
        ("popover-foreground", [24, 24, 27, 255]),
    ] {
        light.insert(name.into(), color);
    }
    let dark = COLORS
        .iter()
        .map(|n| ((*n).into(), [24, 24, 27, 255]))
        .collect();
    Ok((
        component,
        Theme {
            light,
            dark,
            ..Default::default()
        },
    ))
}

fn execute() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("styles") {
        return super::styles::execute(&args);
    }
    if args.as_slice() == ["--help"] {
        println!(
            "meari-native verify TAILWIND_EXE NEW_OUTPUT_DIRECTORY\nmeari-native show TAILWIND_EXE [SECONDS] (requires --features window)\nmeari-native styles REQUEST.json NEW_OUTPUT_DIRECTORY [TAILWIND_EXE]\nOfficial pinned compiler; native Rust layout/paint; no WebView. verify uses library-owned showcase fixtures, not product visual acceptance."
        );
        return Ok(());
    }
    #[cfg(windows)]
    if matches!(
        args.first().map(String::as_str),
        Some("probe" | "probe-tabs")
    ) && args.len() == 3
    {
        let pid = args[1].parse::<u32>().map_err(|e| e.to_string())?;
        let result = if args[0] == "probe-tabs" {
            crate::infrastructure::uia_probe::verify_tabs(pid)
        } else {
            crate::infrastructure::uia_probe::verify(pid)
        };
        let report = match &result {
            Ok(report) => report.clone(),
            Err(error) => serde_json::json!({"pid":pid,"success":false,"error":error}),
        };
        artifacts::json(Path::new(&args[2]), &report)?;
        println!("{report}");
        return result.map(|_| ());
    }
    if args.len() < 2 {
        return Err("use --help".into());
    }
    let compiler = TailwindCompiler::pinned(Path::new(&args[1]))?;
    let (source, theme) = showcase()?;
    let css = application::stylesheet(&source, &theme, &compiler)?;
    let viewport = Viewport {
        width: 800,
        height: 760,
        dark: false,
    };
    let mut document = InteractiveDocument::new(source, &css, viewport)?;
    match args[0].as_str() {
        "verify" if args.len() == 3 => {
            let directory = Path::new(&args[2]);
            fs::create_dir(directory).map_err(|e| e.to_string())?;
            fs::write(directory.join("compiled.css"), &css).map_err(|e| e.to_string())?;
            let mut receipts = vec![artifacts::capture(&mut document, directory, "initial")?];
            let path = |document: &InteractiveDocument, id: &str| {
                document
                    .elements()
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| e.path.clone())
                    .ok_or_else(|| format!("missing fixture node: {id}"))
            };
            let name = path(&document, "name")?;
            document.set_input_value(&name, "State survives native patches")?;
            let details = path(&document, "tab-overview")?;
            document.dispatch(Action::Key {
                path: details,
                key: "ArrowRight".into(),
                shift: false,
            })?;
            receipts.push(artifacts::capture(
                &mut document,
                directory,
                "tabs-details",
            )?);
            let accordion = path(&document, "accordion-trigger")?;
            document.dispatch(Action::Activate { path: accordion })?;
            let select = path(&document, "select-trigger")?;
            document.dispatch(Action::Activate { path: select })?;
            receipts.push(artifacts::capture(&mut document, directory, "select-open")?);
            let quality = path(&document, "mode-quality")?;
            document.dispatch(Action::Activate { path: quality })?;
            let trigger = path(&document, "dialog-trigger")?;
            document.dispatch(Action::Activate {
                path: trigger.clone(),
            })?;
            receipts.push(artifacts::capture(&mut document, directory, "dialog-open")?);
            let close = path(&document, "dialog-close")?;
            document.dispatch(Action::Key {
                path: close,
                key: "Tab".into(),
                shift: false,
            })?;
            let focused = document.session.focus.clone().ok_or("modal has no focus")?;
            document.dispatch(Action::Key {
                path: focused,
                key: "Escape".into(),
                shift: false,
            })?;
            if document.session.focus.as_deref() != Some(&trigger) {
                return Err("focus return verification failed".into());
            }
            if document.input_values.get(&name).map(String::as_str)
                != Some("State survives native patches")
            {
                return Err("input state lost".into());
            }
            receipts.push(artifacts::capture(&mut document, directory, "recovered")?);
            let report = serde_json::json!({"schema_version":1,"engine":"blitz-0.3.0-beta.4/stylo/taffy/parley/vello_cpu","tailwind_version":"4.3.3","tailwind_sha256":crate::infrastructure::tailwind::WINDOWS_X64_SHA256,"source":"library-owned showcase","native_window_test":false,"whole_catalog_parity":false,"receipts":receipts});
            artifacts::json(&directory.join("report.json"), &report)?;
            println!(
                "{}",
                serde_json::json!({"success":true,"report":directory.join("report.json"),"captures":5})
            );
            Ok(())
        }
        "show" if args.len() <= 3 => {
            let seconds = args
                .get(2)
                .map(|s| s.parse::<u64>().map_err(|e| e.to_string()))
                .transpose()?;
            #[cfg(feature = "window")]
            {
                crate::infrastructure::window::show(document, seconds)
            }
            #[cfg(not(feature = "window"))]
            {
                let _ = (document, seconds);
                Err("show requires the window feature".into())
            }
        }
        _ => Err("invalid command; use --help".into()),
    }
}

pub fn run() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!(
                "{}",
                serde_json::json!({"code":"native-renderer","message":message})
            );
            ExitCode::FAILURE
        }
    }
}
