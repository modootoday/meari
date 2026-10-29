use crate::{application::Harness, domain::*};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct OwnedHost(Child);
impl Drop for OwnedHost {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

pub struct Session {
    pub root: PathBuf,
    host: OwnedHost,
    seq: u64,
}
impl Session {
    pub fn start(root: PathBuf, position: [i32; 2]) -> Result<Self> {
        ensure!(!root.exists(), "choose a new session directory");
        fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let executable = std::env::current_exe()?
            .parent()
            .context("executable parent")?
            .join("hmr-host.exe");
        ensure!(executable.is_file(), "build both hmr binaries first");
        let log = fs::File::create(root.join("host-stderr.txt"))?;
        let child = Command::new(executable)
            .arg(&root)
            .arg(position[0].to_string())
            .arg(position[1].to_string())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()?;
        Ok(Self {
            root,
            host: OwnedHost(child),
            seq: 0,
        })
    }
}

impl Harness for Session {
    fn request(&mut self, action: Action) -> Result<Receipt> {
        self.seq += 1;
        super::files::write_json(
            &self.root.join("request.json"),
            &Request {
                seq: self.seq,
                action,
            },
        )?;
        let started = Instant::now();
        loop {
            if let Ok(mut receipt) =
                super::files::read_json::<Receipt>(&self.root.join("receipt.json"))
                && receipt.seq == self.seq
            {
                ensure!(
                    receipt.telemetry.pid == self.host.0.id(),
                    "host PID changed"
                );
                super::platform::validate_virtual_window(&receipt.telemetry)?;
                receipt.request_receipt_ms = started.elapsed().as_millis();
                return Ok(receipt);
            }
            if let Some(exit) = self.host.0.try_wait()? {
                anyhow::bail!("owned host exited before receipt: {exit}");
            }
            ensure!(
                started.elapsed() < Duration::from_secs(30),
                "render receipt timeout at seq {}",
                self.seq
            );
            thread::sleep(Duration::from_millis(30));
        }
    }
    fn build(
        &mut self,
        generation: u32,
        schema: u32,
        wrong_abi: bool,
        syntax_error: bool,
    ) -> Result<Build> {
        super::compiler::build(&self.root, generation, schema, wrong_abi, syntax_error)
    }
    fn remove_module(&mut self, generation: u32) -> Result<()> {
        fs::remove_file(self.root.join(format!("generation-{generation}.dll")))
            .context("owned module file still locked")
    }
    fn wait_for_exit(&mut self) -> Result<()> {
        let start = Instant::now();
        loop {
            if let Some(exit) = self.host.0.try_wait()? {
                ensure!(exit.success(), "host failed to exit cleanly");
                return Ok(());
            }
            ensure!(
                start.elapsed() < Duration::from_secs(10),
                "owned host shutdown timeout"
            );
            thread::sleep(Duration::from_millis(30));
        }
    }
}
