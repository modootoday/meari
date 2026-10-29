use crate::{
    domain::*,
    infrastructure::{files, modules::Module, platform},
};
use anyhow::{Result, ensure};
use eframe::egui;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Host {
    root: PathBuf,
    module: Option<Module>,
    state: State,
    last: Option<Rendered>,
    last_seq: u64,
    pending: Option<Receipt>,
    capture_sent: bool,
    status: String,
    diagnostic: Option<String>,
    started: Instant,
}

impl Host {
    fn receipt(&self, seq: u64) -> Receipt {
        Receipt {
            seq,
            request_receipt_ms: 0,
            status: self.status.clone(),
            state: self.state.clone(),
            rendered: self.last.clone(),
            module_sha256: self.module.as_ref().map(|m| m.sha256.clone()),
            diagnostic: self.diagnostic.clone(),
            telemetry: WindowTelemetry::default(),
            screenshot: None,
            screenshot_sha256: None,
        }
    }
    fn execute(&mut self, request: Request, ctx: &egui::Context) -> Result<bool> {
        self.last_seq = request.seq;
        self.diagnostic = None;
        match request.action {
            Action::SetState(state) => {
                self.state = state;
                self.status = "state_updated".into();
            }
            Action::Load { filename } => {
                match files::module_path(&self.root, &filename).and_then(|path| Module::load(&path))
                {
                    Ok(module) => {
                        self.module = Some(module);
                        self.status = "reloaded".into();
                    }
                    Err(error) => {
                        self.status = "reload_rejected".into();
                        self.diagnostic = Some(error.to_string());
                    }
                }
            }
            Action::BuildFailed { diagnostic } => {
                self.status = "build_failed_last_good".into();
                self.diagnostic = Some(diagnostic);
            }
            Action::Capture => self.status = "captured".into(),
            Action::Stop => {
                let mut receipt = self.receipt(request.seq);
                receipt.status = "stopped".into();
                receipt.telemetry = platform::telemetry()?;
                files::write_json(&self.root.join("receipt.json"), &receipt)?;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn complete_capture(&mut self, ctx: &egui::Context) -> Result<()> {
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot {
                user_data, image, ..
            } = event
                && let Some(mut receipt) = self.pending.take()
            {
                if user_data
                    .data
                    .as_ref()
                    .and_then(|d| d.downcast_ref::<u64>())
                    != Some(&receipt.seq)
                {
                    self.pending = Some(receipt);
                    continue;
                }
                let filename = format!("frame-{}.png", receipt.seq);
                let path = self.root.join(&filename);
                files::screenshot(&path, &image)?;
                receipt.screenshot = Some(filename);
                receipt.screenshot_sha256 = Some(files::hash(&path)?);
                receipt.telemetry = platform::telemetry()?;
                files::write_json(
                    &self.root.join("window-observation.json"),
                    &receipt.telemetry,
                )?;
                platform::validate_virtual_window(&receipt.telemetry)?;
                files::write_json(&self.root.join("receipt.json"), &receipt)?;
                self.capture_sent = false;
            }
        }
        Ok(())
    }
}

impl eframe::App for Host {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.started.elapsed() > Duration::from_secs(600) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Err(error) = self.complete_capture(&ctx) {
            eprintln!("capture failed: {error:#}");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let mut capture = None;
        if self.pending.is_none()
            && let Ok(request) = files::read_json::<Request>(&self.root.join("request.json"))
            && request.seq > self.last_seq
        {
            let seq = request.seq;
            match self.execute(request, &ctx) {
                Ok(true) => capture = Some(seq),
                Ok(false) => {}
                Err(error) => {
                    self.status = "request_error".into();
                    self.diagnostic = Some(error.to_string());
                    capture = Some(seq);
                }
            }
        }
        if let Some(module) = &self.module {
            match module.frame(self.state.counter) {
                Ok(frame) => self.last = Some(frame),
                Err(error) => {
                    self.status = "render_error".into();
                    self.diagnostic = Some(error.to_string());
                }
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Rust code HMR verification host");
            ui.label("Owned verification fixture. No product visual acceptance claim.");
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Story");
                egui::ComboBox::from_id_salt("story")
                    .selected_text(&self.state.story)
                    .show_ui(ui, |ui| {
                        for story in ["Button", "Input", "Tabs"] {
                            ui.selectable_value(&mut self.state.story, story.into(), story);
                        }
                    });
                ui.label("Variant");
                egui::ComboBox::from_id_salt("variant")
                    .selected_text(&self.state.variant)
                    .show_ui(ui, |ui| {
                        for variant in ["default", "selected", "disabled"] {
                            ui.selectable_value(&mut self.state.variant, variant.into(), variant);
                        }
                    });
                ui.add(egui::DragValue::new(&mut self.state.counter));
            });
            ui.text_edit_singleline(&mut self.state.input);
            ui.label(format!("Status: {}", self.status));
            if let Some(diagnostic) = &self.diagnostic {
                ui.colored_label(egui::Color32::LIGHT_RED, diagnostic);
            }
            if let Some(frame) = &self.last {
                ui.heading(&frame.heading);
                ui.label(format!("Compiled function result: {}", frame.computed));
                if ui.button(&frame.button).clicked() {
                    self.state.counter += 1;
                }
            }
            ui.separator();
            let area = egui::ScrollArea::vertical()
                .id_salt("preserved_scroll")
                .max_height(400.0)
                .vertical_scroll_offset(self.state.scroll)
                .show(ui, |ui| {
                    for row in 0..100 {
                        ui.label(format!("Stable host-owned row {row}"));
                    }
                });
            self.state.scroll = area.state.offset.y;
        });
        if let Some(seq) = capture {
            self.pending = Some(self.receipt(seq));
        }
        if let Some(receipt) = &self.pending
            && !self.capture_sent
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                receipt.seq,
            )));
            self.capture_sent = true;
        }
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

pub fn run_host(root: PathBuf, position: [i32; 2]) -> Result<()> {
    ensure!(root.is_dir(), "session directory missing");
    let root = root.canonicalize()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_position([position[0] as f32, position[1] as f32])
            .with_inner_size([1280.0, 900.0])
            .with_active(false),
        ..Default::default()
    };
    eframe::run_native(
        "Rust HMR Verification",
        options,
        Box::new(move |_| {
            Ok(Box::new(Host {
                root,
                module: None,
                state: State::default(),
                last: None,
                last_seq: 0,
                pending: None,
                capture_sent: false,
                status: "ready".into(),
                diagnostic: None,
                started: Instant::now(),
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}
