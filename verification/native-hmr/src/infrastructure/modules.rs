use crate::domain::{
    Rendered,
    abi::{self, ContractFn, Input, RenderFn},
};
use anyhow::{Result, ensure};
use libloading::Library;
use std::path::Path;

pub struct Module {
    _library: Library,
    render: RenderFn,
    pub sha256: String,
}

impl Module {
    pub fn load(path: &Path) -> Result<Self> {
        let sha256 = super::files::hash(path)?;
        // SAFETY: path is an owned, locally compiled verification fixture. Symbols are
        // fixed C ABI and the library remains owned until all frame data is copied.
        unsafe {
            let library = Library::new(path)?;
            let contract = library.get::<ContractFn>(b"ui_contract\0")?;
            ensure!(abi::compatible(contract()), "abi_schema_mismatch");
            let render = *library.get::<RenderFn>(b"ui_render\0")?;
            Ok(Self {
                _library: library,
                render,
                sha256,
            })
        }
    }
    pub fn frame(&self, counter: i64) -> Result<Rendered> {
        // SAFETY: render belongs to self's live library and returns borrowed static
        // UTF-8 buffers. Copies finish in this call; no plugin pointer escapes.
        unsafe {
            let frame = (self.render)(Input { counter });
            ensure!(
                frame.heading_len <= 4096
                    && frame.button_len <= 4096
                    && !frame.heading.is_null()
                    && !frame.button.is_null(),
                "invalid frame buffers"
            );
            let heading =
                std::str::from_utf8(std::slice::from_raw_parts(frame.heading, frame.heading_len))?
                    .to_owned();
            let button =
                std::str::from_utf8(std::slice::from_raw_parts(frame.button, frame.button_len))?
                    .to_owned();
            Ok(Rendered {
                generation: frame.generation,
                heading,
                button,
                computed: frame.computed,
            })
        }
    }
}
