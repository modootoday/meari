pub mod abi;
mod summary;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct State {
    pub story: String,
    pub variant: String,
    pub counter: i64,
    pub input: String,
    pub scroll: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            story: "Button".into(),
            variant: "default".into(),
            counter: 0,
            input: "initial".into(),
            scroll: 0.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    SetState(State),
    Load { filename: String },
    BuildFailed { diagnostic: String },
    Capture,
    Stop,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub seq: u64,
    pub action: Action,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rendered {
    pub generation: u32,
    pub heading: String,
    pub button: String,
    pub computed: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WindowTelemetry {
    pub pid: u32,
    pub hwnd: u64,
    pub bounds: [i32; 4],
    pub monitor: [i32; 4],
    pub primary: bool,
    pub working_set_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub seq: u64,
    #[serde(default)]
    pub request_receipt_ms: u128,
    pub status: String,
    pub state: State,
    pub rendered: Option<Rendered>,
    pub module_sha256: Option<String>,
    pub diagnostic: Option<String>,
    pub telemetry: WindowTelemetry,
    pub screenshot: Option<String>,
    pub screenshot_sha256: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Build {
    pub generation: u32,
    pub elapsed_ms: u128,
    pub source_sha256: String,
    pub success: bool,
    pub stderr: String,
}

#[derive(Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub scope: String,
    pub compatible_code_swaps: u32,
    pub old_modules_unlocked: u32,
    pub owned_host_exited: bool,
    pub final_module_unlocked: bool,
    pub builds: Vec<Build>,
    pub receipts: Vec<Receipt>,
}
