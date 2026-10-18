use serde::{Deserialize, Serialize};

/// Layout viewport in CSS pixels, independent of monitor resolution or backend.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub dark: bool,
}

impl Viewport {
    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 || self.width > 8192 || self.height > 8192 {
            return Err("viewport dimensions must be in 1..=8192".into());
        }
        Ok(())
    }
}
