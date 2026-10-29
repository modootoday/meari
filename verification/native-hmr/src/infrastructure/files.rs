use anyhow::{Result, ensure};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub fn hash(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
pub fn module_path(root: &Path, filename: &str) -> Result<std::path::PathBuf> {
    ensure!(
        filename.starts_with("generation-")
            && filename.ends_with(".dll")
            && filename
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.'),
        "invalid module filename"
    );
    let path = root.join(filename).canonicalize()?;
    ensure!(path.parent() == Some(root), "module outside session root");
    Ok(path)
}
pub fn screenshot(path: &Path, image: &eframe::egui::ColorImage) -> Result<()> {
    let mut encoder = png::Encoder::new(
        fs::File::create(path)?,
        image.size[0] as u32,
        image.size[1] as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
    encoder.write_header()?.write_image_data(&bytes)?;
    Ok(())
}
