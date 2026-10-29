//! Side-by-side contact sheet: panels laid out left to right with an 8px grey gutter.
use std::path::Path;

pub fn write(path: &Path, panels: &[&[u8]]) -> Result<(), String> {
    const GUTTER: u32 = 8;
    let (w, h) = (crate::WIDTH, crate::HEIGHT);
    let count = panels.len() as u32;
    let width = w * count + GUTTER * (count - 1);
    let mut image = vec![160u8; (width * h * 4) as usize];
    for (index, panel) in panels.iter().enumerate() {
        let x0 = index as u32 * (w + GUTTER);
        for y in 0..h {
            let src = (y * w * 4) as usize;
            let dst = ((y * width + x0) * 4) as usize;
            image[dst..dst + (w * 4) as usize].copy_from_slice(&panel[src..src + (w * 4) as usize]);
        }
    }
    crate::compare::write_png_sized(path, &image, width, h)
}
