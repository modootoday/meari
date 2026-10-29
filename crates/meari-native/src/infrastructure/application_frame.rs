use super::document::InteractiveDocument;
use crate::application::Renderer;
use meari::{application::app_bundle::AppBuildStamp, domain::build::ContentHash};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ApplicationFrameReceipt {
    pub stamp: AppBuildStamp,
    pub definition_revision: u64,
    pub source_schema: ContentHash,
    pub state: meari::application::component_state::StateCommit,
    pub width: u32,
    pub height: u32,
    pub rgba_bytes: usize,
    pub frame_hash: ContentHash,
}

impl InteractiveDocument {
    /// Actual offscreen rasterization; this does not acknowledge an OS-presented window frame.
    pub fn render_application_frame(&mut self) -> Result<ApplicationFrameReceipt, String> {
        if self.source_input_poisoned {
            return Err("source input document requires remount after failed recovery".into());
        }
        let stamp = self
            .application_stamp()
            .ok_or("document has no application build identity")?
            .clone();
        let definition_revision = self
            .state_program
            .as_ref()
            .ok_or("application state disappeared")?
            .definition_revision();
        if definition_revision != self.application_definition_revision {
            return Err("application source changed outside its build commit; resynchronize before rendering a receipt".into());
        }
        let state = self.component_state()?;
        let source_schema = self.source_schema.fingerprint()?;
        let viewport = self.viewport.clone();
        let pixels = self.renderer.rasterize(&viewport)?;
        let expected = (viewport.width as usize)
            .checked_mul(viewport.height as usize)
            .and_then(|size| size.checked_mul(4))
            .ok_or("application frame size overflow")?;
        if pixels.len() != expected {
            return Err("application raster byte count differs from viewport".into());
        }
        Ok(ApplicationFrameReceipt {
            stamp,
            definition_revision,
            source_schema,
            state,
            width: viewport.width,
            height: viewport.height,
            rgba_bytes: pixels.len(),
            frame_hash: ContentHash::of(&pixels),
        })
    }
}
