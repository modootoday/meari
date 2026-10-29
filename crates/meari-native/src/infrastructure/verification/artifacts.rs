use crate::{
    application::Renderer,
    infrastructure::{accessibility, document::InteractiveDocument},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub fn json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub fn capture(
    document: &mut InteractiveDocument,
    directory: &Path,
    name: &str,
) -> Result<serde_json::Value, String> {
    let viewport = document.viewport.clone();
    let bytes = document.renderer.rasterize(&viewport)?;
    let image =
        fs::File::create(directory.join(format!("{name}.png"))).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(image, viewport.width, viewport.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&bytes).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    let boxes: Vec<_> = document
        .elements()
        .iter()
        .filter_map(|element| document.renderer.bounds(&element.id).ok())
        .collect();
    let tree = accessibility::tree(document);
    json(&directory.join(format!("{name}.a11y.json")), &tree)?;
    json(&directory.join(format!("{name}.layout.json")), &boxes)?;
    Ok(
        serde_json::json!({"name":name,"rgba_sha256":format!("{:x}",Sha256::digest(bytes)),"paint":document.renderer.last_paint,"backend":if document.renderer.last_paint.is_some(){"skia-opengl-vector"}else{"vello-cpu"},"accessibility_nodes":tree.nodes.len(),"session":document.session,"input_values":document.input_values,"file_values":document.file_values,"checked_values":document.checked_values,"viewport":viewport}),
    )
}
