//! `text-overflow: ellipsis` painting for single overflowing lines.
//!
//! Meari modification (2026-10-30): Blitz lays out `truncate` text without an ellipsis;
//! Chrome drops whole glyphs from the line end until U+2026 fits inside the content box
//! and draws the ellipsis in the run's own font. We mirror that at paint time.

/// The ellipsis glyph id and its advance (layout units) in `font` at `font_size`.
pub(crate) fn ellipsis_glyph(
    font: &parley::FontData,
    font_size: f32,
    normalized_coords: &[i16],
) -> Option<(u32, f32)> {
    use skrifa::MetadataProvider as _;
    use skrifa::instance::{LocationRef, Size};
    use skrifa::raw::types::F2Dot14;
    let font_ref = skrifa::FontRef::from_index(font.data.as_ref(), font.index).ok()?;
    let gid = font_ref.charmap().map('\u{2026}')?;
    let coords: Vec<F2Dot14> = normalized_coords
        .iter()
        .map(|c| F2Dot14::from_bits(*c))
        .collect();
    let advance = font_ref
        .glyph_metrics(Size::new(font_size), LocationRef::new(&coords))
        .advance_width(gid)?;
    Some((gid.to_u32(), advance))
}

/// Inline-axis limit for `text-overflow: ellipsis` on `node`, in layout (device) units:
/// `Some(content width)` when the box clips its inline overflow and ends with an ellipsis.
pub(crate) fn ellipsis_limit(node: &blitz_dom::Node, scale: f64) -> Option<f64> {
    use style::values::computed::Overflow;
    use style::values::specified::text::TextOverflowSide;
    let style = node.primary_styles()?;
    if matches!(style.get_box().overflow_x, Overflow::Visible) {
        return None;
    }
    let text_overflow = &style.get_text().text_overflow;
    if !matches!(text_overflow.second, TextOverflowSide::Ellipsis) {
        return None;
    }
    let layout = node.final_layout();
    let width = layout.size.width
        - layout.padding.left
        - layout.padding.right
        - layout.border.left
        - layout.border.right;
    Some(width as f64 * scale)
}
