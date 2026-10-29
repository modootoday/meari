//! Pixel comparison and PNG I/O (RGBA8, sRGB, same WIDTH x HEIGHT).
use std::{fs::File, io::BufWriter, path::Path};

#[derive(Clone, Copy, Debug)]
pub struct Tolerance {
    /// Max per-channel absolute difference for a pixel to count as equal.
    pub channel: u8,
    /// Max ratio of differing pixels for a fixture to pass.
    pub max_ratio: f64,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            channel: 16,
            max_ratio: 0.005,
        }
    }
}

pub struct Diff {
    pub differing: usize,
    pub total: usize,
    pub max_channel: u8,
    pub image: Vec<u8>,
}

impl Diff {
    pub fn ratio(&self) -> f64 {
        self.differing as f64 / self.total as f64
    }
}

pub fn diff(a: &[u8], b: &[u8], tolerance: Tolerance) -> Result<Diff, String> {
    if a.len() != b.len() {
        return Err(format!("size mismatch {} vs {}", a.len(), b.len()));
    }
    let mut image = Vec::with_capacity(a.len());
    let (mut differing, mut max_channel) = (0, 0u8);
    for (pa, pb) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()) {
        let delta = pa
            .iter()
            .zip(pb)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        max_channel = max_channel.max(delta);
        if delta > tolerance.channel {
            differing += 1;
            image.extend_from_slice(&[255, 0, 255, 255]);
        } else {
            let gray = (pa[0] as u16 + pa[1] as u16 + pa[2] as u16) / 3 / 3 + 170;
            image.extend_from_slice(&[gray as u8, gray as u8, gray as u8, 255]);
        }
    }
    Ok(Diff {
        differing,
        total: a.len() / 4,
        max_channel,
        image,
    })
}

pub fn write_png(path: &Path, rgba: &[u8]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), crate::WIDTH, crate::HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())
}

pub fn read_png(path: &Path) -> Result<Vec<u8>, String> {
    let decoder = png::Decoder::new(std::io::BufReader::new(
        File::open(path).map_err(|e| e.to_string())?,
    ));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("png too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.width != crate::WIDTH || info.height != crate::HEIGHT {
        return Err(format!("web screenshot is {}x{}", info.width, info.height));
    }
    let data = &buf[..info.buffer_size()];
    Ok(match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => return Err(format!("unsupported png color {other:?}")),
    })
}
