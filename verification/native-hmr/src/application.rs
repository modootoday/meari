use crate::domain::*;
use anyhow::{Context, Result, ensure};

pub trait Harness {
    fn request(&mut self, action: Action) -> Result<Receipt>;
    fn build(
        &mut self,
        generation: u32,
        schema: u32,
        wrong_abi: bool,
        syntax_error: bool,
    ) -> Result<Build>;
    fn remove_module(&mut self, generation: u32) -> Result<()>;
    fn wait_for_exit(&mut self) -> Result<()>;
}

struct Recorder<'a> {
    port: &'a mut dyn Harness,
    receipts: Vec<Receipt>,
}
impl Recorder<'_> {
    fn request(&mut self, action: Action) -> Result<Receipt> {
        let receipt = self.port.request(action)?;
        if let Some(first) = self.receipts.first() {
            ensure!(
                receipt.telemetry.pid == first.telemetry.pid
                    && receipt.telemetry.hwnd == first.telemetry.hwnd,
                "PID/HWND changed"
            );
        }
        self.receipts.push(receipt.clone());
        Ok(receipt)
    }
    fn load(&mut self, generation: u32) -> Result<Receipt> {
        self.request(Action::Load {
            filename: format!("generation-{generation}.dll"),
        })
    }
}

pub fn verify(port: &mut dyn Harness) -> Result<Report> {
    let mut session = Recorder {
        port,
        receipts: vec![],
    };
    let mut builds = Vec::new();
    let state = State {
        story: "Tabs".into(),
        variant: "selected".into(),
        counter: 7,
        input: "host-owned typed text survives code changes".into(),
        scroll: 77.0,
    };
    let baseline = session.request(Action::SetState(state))?;
    let expected = baseline.state;
    let mut old_modules_unlocked = 0;
    let mut previous_frame_hash = None;
    for generation in 1..=21 {
        let build = session.port.build(generation, 1, false, false)?;
        ensure!(build.success, "Rust UI build failed: {}", build.stderr);
        builds.push(build);
        let receipt = session.load(generation)?;
        ensure!(
            receipt.status == "reloaded",
            "reload failed: {:?}",
            receipt.diagnostic
        );
        ensure!(
            receipt.state == expected,
            "compatible state changed at {generation}: {:?}",
            receipt.state
        );
        let rendered = receipt.rendered.as_ref().context("rendered UI missing")?;
        ensure!(
            rendered.generation == generation
                && rendered.computed == 7 * (i64::from(generation) + 1),
            "compiled Rust body not applied"
        );
        ensure!(
            receipt.screenshot.is_some() && receipt.screenshot_sha256.is_some(),
            "no actual rendered screenshot"
        );
        if let Some(previous) = &previous_frame_hash {
            ensure!(
                receipt.screenshot_sha256.as_ref() != Some(previous),
                "rendered screenshot unchanged"
            );
        }
        previous_frame_hash = receipt.screenshot_sha256;
        if generation > 1 {
            session.port.remove_module(generation - 1)?;
            old_modules_unlocked += 1;
        }
    }
    let broken = session.port.build(22, 1, false, true)?;
    ensure!(!broken.success, "compile error was not detected");
    let receipt = session.request(Action::BuildFailed {
        diagnostic: "intentional Rust syntax error; last good generation retained".into(),
    })?;
    ensure!(
        receipt.status == "build_failed_last_good"
            && receipt
                .rendered
                .as_ref()
                .is_some_and(|r| r.generation == 21)
            && receipt.state == expected,
        "compile failure lost scene/state"
    );
    builds.push(broken);
    for (generation, schema, wrong_abi) in [(23, 2, false), (24, 1, true)] {
        let build = session.port.build(generation, schema, wrong_abi, false)?;
        ensure!(build.success, "mismatch fixture failed to compile");
        builds.push(build);
        let receipt = session.load(generation)?;
        ensure!(
            receipt.status == "reload_rejected"
                && receipt
                    .rendered
                    .as_ref()
                    .is_some_and(|r| r.generation == 21)
                && receipt.state == expected,
            "incompatible module was applied"
        );
        session.port.remove_module(generation)?;
    }
    let build = session.port.build(25, 1, false, false)?;
    ensure!(build.success, "recovery build failed");
    builds.push(build);
    let recovered = session.load(25)?;
    ensure!(
        recovered.status == "reloaded"
            && recovered
                .rendered
                .as_ref()
                .is_some_and(|r| r.generation == 25 && r.computed == 182)
            && recovered.state == expected,
        "recovery failed"
    );
    session.port.remove_module(21)?;
    session.request(Action::Stop)?;
    session.port.wait_for_exit()?;
    session.port.remove_module(25)?;
    Ok(Report {
        schema_version: 1,
        scope: "native verification fixture; not completed workbench or product visual acceptance; controls set through host API, not OS input".into(),
        compatible_code_swaps: 20,
        old_modules_unlocked,
        owned_host_exited: true,
        final_module_unlocked: true,
        builds,
        receipts: session.receipts,
    })
}
