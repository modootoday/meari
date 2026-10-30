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
    /// Ink row/column spans agree within [`LAYOUT_SLACK_PX`]: any difference is edge
    /// (anti-aliasing) only, not placement or size.
    pub layout_equal: bool,
}

/// Per-edge slack for [`Diff::layout_equal`] (glyph AA can grow an ink span by one pixel).
pub const LAYOUT_SLACK_PX: usize = 1;
/// Channel distance from the top-left background pixel that counts as ink.
const INK_THRESHOLD: u8 = 48;

/// Contiguous runs of `true` as inclusive `(start, end)` index pairs.
fn runs(flags: &[bool]) -> Spans {
    let mut out = Vec::new();
    let mut start = None;
    for (i, &on) in flags.iter().enumerate() {
        match (on, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, i - 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, flags.len() - 1));
    }
    out
}

/// Inclusive `(start, end)` index runs.
type Spans = Vec<(usize, usize)>;

/// Ink row spans and column spans of an RGBA image `width` pixels wide.
fn ink_spans(rgba: &[u8], width: usize) -> (Spans, Spans) {
    let pixels = rgba.as_chunks::<4>().0;
    let height = pixels.len() / width.max(1);
    let background = pixels.first().copied().unwrap_or([255; 4]);
    let (mut rows, mut cols) = (vec![false; height], vec![false; width]);
    for (i, px) in pixels.iter().enumerate() {
        let ink = px
            .iter()
            .zip(background)
            .any(|(x, y)| x.abs_diff(y) > INK_THRESHOLD);
        if ink {
            rows[i / width] = true;
            cols[i % width] = true;
        }
    }
    (runs(&rows), runs(&cols))
}

fn spans_close(a: &[(usize, usize)], b: &[(usize, usize)]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.0.abs_diff(y.0) <= LAYOUT_SLACK_PX && x.1.abs_diff(y.1) <= LAYOUT_SLACK_PX
        })
}

/// Whether two same-size RGBA images place their ink identically up to AA edge slack.
pub fn layout_matches(a: &[u8], b: &[u8], width: usize) -> bool {
    let (ra, ca) = ink_spans(a, width);
    let (rb, cb) = ink_spans(b, width);
    spans_close(&ra, &rb) && spans_close(&ca, &cb)
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
        layout_equal: layout_matches(a, b, crate::WIDTH as usize),
    })
}

pub fn write_png(path: &Path, rgba: &[u8]) -> Result<(), String> {
    write_png_sized(path, rgba, crate::WIDTH, crate::HEIGHT)
}

pub fn write_png_sized(path: &Path, rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
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

#[cfg(test)]
mod layout_tests {
    use super::layout_matches;

    fn image(width: usize, height: usize, ink: &[(usize, usize)]) -> Vec<u8> {
        let mut rgba = vec![255u8; width * height * 4];
        for &(x, y) in ink {
            rgba[(y * width + x) * 4..][..3].copy_from_slice(&[0, 0, 0]);
        }
        rgba
    }

    #[test]
    fn one_pixel_aa_growth_is_layout_equal() {
        let a = image(8, 8, &[(2, 2), (3, 2), (2, 3)]);
        let b = image(8, 8, &[(2, 2), (3, 2), (4, 2), (2, 3), (2, 4)]);
        assert!(layout_matches(&a, &b, 8));
    }

    #[test]
    fn moved_or_missing_ink_is_a_layout_difference() {
        let a = image(8, 8, &[(1, 1)]);
        assert!(!layout_matches(&a, &image(8, 8, &[(4, 4)]), 8));
        assert!(!layout_matches(&a, &image(8, 8, &[(1, 1), (5, 5)]), 8));
        assert!(!layout_matches(&a, &image(8, 8, &[]), 8));
    }
}
