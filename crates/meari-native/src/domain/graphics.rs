//! Backend-neutral generation and asset identity. No native handles cross this boundary.
use meari::domain::design::icons::IconSpec;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameTicket {
    pub context_id: String,
    pub context_generation: u64,
    pub scene_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IconCacheKey {
    pub context_id: String,
    pub context_generation: u64,
    pub asset_revision: String,
    pub style_revision: String,
    pub spec: IconSpec,
    pub scale_milli: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct FrameReceipt {
    pub context_id: String,
    pub backend: String,
    pub context_generation: u64,
    pub scene_generation: u64,
    pub width: u32,
    pub height: u32,
    pub rgba_sha256: String,
    pub gpu_backed: bool,
    pub driver: Driver,
}

#[derive(Clone, Debug, Serialize)]
pub struct Driver {
    pub vendor: String,
    pub renderer: String,
    pub version: String,
}
