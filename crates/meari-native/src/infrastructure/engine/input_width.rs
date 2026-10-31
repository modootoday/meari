//! Chromium's intrinsic inline size for single-line text inputs (`size=20`).
//!
//! Blitz sizes an auto-width text input as `min(available, 300px)`. Chromium uses
//! `ceil(avgCharWidth × 20) + (maxCharWidth − avgCharWidth)` from the primary font's OS/2 and
//! head tables (`LayoutTextControl` / `TextControlSingleLineLayoutAlgorithm`). We compute that
//! width and expose it to a user-agent rule through a custom property, so every author `width`
//! (any origin above user agent) still wins exactly as on the Web.
use blitz_dom::NodeId;
use blitz_html::HtmlDocument;
use meari_platform::{AvgCharWidthSource, PlatformHost as _};
use parley::style::{FontFamily, FontFamilyName, GenericFamily};
use parley::{FontContext, LayoutContext, PositionedLayoutItem, StyleProperty};
use std::borrow::Cow;

pub(super) const PROPERTY: &str = "--meari-input-intrinsic-width";
/// Only the auto `width` of single-line text-like inputs takes the computed intrinsic size.
/// Chromium's text control refuses to flex-shrink below that size while its width is the
/// intrinsic one, so the automatic minimum follows it (published only in that case).
pub(super) const CSS: &str = "input:is(:not([type]),[type=text],[type=search],[type=email],\
[type=password],[type=url],[type=tel],[type=number]){width:var(--meari-input-intrinsic-width);\
min-width:var(--meari-input-intrinsic-min-width)}";
const MIN_PROPERTY: &str = "--meari-input-intrinsic-min-width";
/// HTML `size` default for text inputs.
const SIZE: f32 = 20.0;

/// Chromium's average and maximum char width for a font face at `size` px.
///
/// `source` is the host's avgCharWidth origin: OS/2 `xAvgCharWidth`, or (Skia DirectWrite on
/// Windows, which reports none) the advance of `x`; on Apple also `max = max(avg, ascent)`.
pub(super) fn char_widths(
    data: &[u8],
    index: u32,
    size: f32,
    zero: f32,
    source: AvgCharWidthSource,
) -> Option<(f32, f32)> {
    let face = ttf_parser::Face::parse(data, index).ok()?;
    let scale = size / f32::from(face.units_per_em());
    let avg = match source {
        AvgCharWidthSource::Os2XAvgCharWidth => {
            let os2 = face
                .raw_face()
                .table(ttf_parser::Tag::from_bytes(b"OS/2"))?;
            f32::from(i16::from_be_bytes([*os2.get(2)?, *os2.get(3)?])) // xAvgCharWidth
        }
        AvgCharWidthSource::XGlyphAdvance | AvgCharWidthSource::XGlyphAdvanceAscentMax => {
            let x = face.glyph_index('x')?;
            f32::from(face.glyph_hor_advance(x)?)
        }
    };
    let avg = avg * scale;
    // Fonts whose avgCharWidth matches CJK full-width glyphs fall back to the '0' advance.
    if avg <= 0.0 || (zero > 0.0 && avg > zero * 1.7) {
        return Some((zero, 0.0));
    }
    let max = if source == AvgCharWidthSource::XGlyphAdvanceAscentMax {
        // Blink on Apple: max(avg, ascent), CoreText ascent = hhea ascender (crbug.com/420901).
        (f32::from(face.tables().hhea.ascender) * scale).max(avg)
    } else {
        let bbox = face.global_bounding_box();
        (f32::from(bbox.x_max) - f32::from(bbox.x_min)) * scale
    };
    Some((avg.max(avg.round()), max.round()))
}

/// Content-box inline size Chromium gives a `size=20` input.
pub(super) fn intrinsic(avg: f32, max: f32) -> f32 {
    let base = (avg * SIZE * 64.0).ceil() / 64.0; // LayoutUnit::FromFloatCeil
    // The text control then snaps up to whole pixels (measured against Chrome on 15
    // family/size pairs: Noto Sans, Arimo, Tinos, DejaVu Sans, Pretendard (former bundled face, removed) at 13/16/17.5px).
    (if max > 0.0 { base + max - avg } else { base }).ceil()
}

fn primary(fonts: &mut FontContext, style: &style::properties::ComputedValues) -> Option<f32> {
    use style::values::computed::font::{GenericFontFamily, SingleFontFamily};
    let font = style.get_font();
    let families = font
        .font_family
        .families
        .iter()
        .map(|family| match family {
            SingleFontFamily::FamilyName(name) => {
                FontFamilyName::Named(Cow::Owned(name.name.to_string()))
            }
            SingleFontFamily::Generic(generic) => FontFamilyName::Generic(match generic {
                GenericFontFamily::Serif => GenericFamily::Serif,
                GenericFontFamily::Monospace => GenericFamily::Monospace,
                GenericFontFamily::Cursive => GenericFamily::Cursive,
                GenericFontFamily::Fantasy => GenericFamily::Fantasy,
                GenericFontFamily::SystemUi => GenericFamily::SystemUi,
                _ => GenericFamily::SansSerif,
            }),
        })
        .collect::<Vec<_>>();
    let size = font.font_size.computed_size().px();
    let mut layouts = LayoutContext::<()>::new();
    let mut builder = layouts.ranged_builder(fonts, "0", 1.0, true);
    builder.push_default(StyleProperty::FontFamily(FontFamily::List(Cow::Owned(
        families,
    ))));
    builder.push_default(StyleProperty::FontSize(size));
    builder.push_default(StyleProperty::FontWeight(parley::FontWeight::new(
        font.font_weight.value(),
    )));
    let mut layout = builder.build("0");
    layout.break_all_lines(None);
    let line = layout.lines().next()?;
    let run = line.items().find_map(|item| match item {
        PositionedLayoutItem::GlyphRun(run) => Some(run),
        PositionedLayoutItem::InlineBox(_) => None,
    })?;
    let zero = run.advance();
    let face = run.run().font();
    let (avg, max) = char_widths(
        face.data.data(),
        face.index,
        run.run().font_size(),
        zero,
        meari_platform::current().avg_char_width_source(),
    )?;
    Some(intrinsic(avg, max))
}

fn text_inputs(doc: &HtmlDocument) -> Vec<NodeId> {
    let mut queue = vec![doc.root_element().id];
    let mut ids = Vec::new();
    while let Some(id) = queue.pop() {
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        queue.extend(node.children.iter().copied());
        if node
            .element_data()
            .and_then(|e| e.text_input_data())
            .is_some_and(|data| !data.is_multiline)
        {
            ids.push(id);
        }
    }
    ids
}

/// Publishes each text input's border-box or content-box intrinsic width; true when any changed.
pub(super) fn refresh(
    doc: &mut HtmlDocument,
    fonts: &mut FontContext,
    published: &mut std::collections::HashMap<(NodeId, bool), String>,
) -> bool {
    let mut changed = false;
    for id in text_inputs(doc) {
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        let Some(style) = node.primary_styles() else {
            continue;
        };
        let Some(content) = primary(fonts, &style) else {
            continue;
        };
        let edges = if style.get_position().box_sizing
            == style::computed_values::box_sizing::T::BorderBox
        {
            let padding = style.get_padding();
            let border = style.get_border();
            let px = |value: &style::values::computed::LengthPercentage| {
                value.to_length().map_or(0.0, |length| length.px())
            };
            px(&padding.padding_left.0)
                + px(&padding.padding_right.0)
                + border.border_left_width.0.to_f32_px()
                + border.border_right_width.0.to_f32_px()
        } else {
            0.0
        };
        let total = content + edges;
        let value = format!("{total}px");
        // Automatic minimum only while the used width is still the intrinsic one.
        let intrinsic_width = matches!(&style.get_position().width,
            style::values::generics::length::GenericSize::LengthPercentage(width)
                if width.0.to_length().is_some_and(|length| (length.px() - total).abs() < 0.01));
        let auto_min = matches!(
            style.get_position().min_width,
            style::values::generics::length::GenericSize::Auto
        ) || published.contains_key(&(id, true));
        let min = (intrinsic_width && auto_min).then(|| value.clone());
        drop(style);
        if let Some(min) = min.clone()
            && published.get(&(id, true)) != Some(&min)
        {
            doc.set_style_property(id, MIN_PROPERTY, &min);
            published.insert((id, true), min);
            changed = true;
        } else if min.is_none() && published.remove(&(id, true)).is_some() {
            doc.remove_style_property(id, MIN_PROPERTY);
            changed = true;
        }
        let current = published.get(&(id, false)) == Some(&value);
        if !current {
            doc.set_style_property(id, PROPERTY, &value);
            published.insert((id, false), value);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_size_twenty_formula() {
        // ceil(avg × 20) to a 1/64 LayoutUnit, plus (max − avg) when the max width is known.
        assert_eq!(intrinsic(7.0, 0.0), 140.0);
        assert_eq!(intrinsic(7.0, 20.0), 153.0);
        // Noto Sans 16px: avg 9.12, max 55 → 228.28 → Chrome 229.
        assert_eq!(intrinsic(9.12, 55.0), 229.0);
        // Arimo 17.5px: avg 10.177, max 58 → 251.37 → Chrome 252.
        assert_eq!(intrinsic(10.177, 58.0), 252.0);
    }

    #[test]
    fn directwrite_hosts_take_the_x_advance_as_avg_char_width() {
        // Chrome on Windows (Skia DirectWrite, no fAvgCharWidth) takes the advance of `x` as
        // avg: Noto Sans 16px x = 529/1000 em = 8.464, max = round(3421/1000 em) = 55 → 216.
        let avg = 529.0 * 16.0 / 1000.0_f32;
        assert_eq!(intrinsic(avg.max(avg.round()), 55.0), 216.0);
    }

    #[test]
    fn apple_hosts_take_the_ascent_as_max_char_width() {
        // Chrome on macOS: avg = `x` advance 8.464, max = round(max(avg, hhea ascent
        // 1069/1000 em = 17.104)) = 17 → 178 (measured on the macOS runner: size=20 → 178,
        // size=10 → 94, size=2 → 26, size=1 → 17).
        let avg = 529.0 * 16.0 / 1000.0_f32;
        let max = (1069.0 * 16.0 / 1000.0_f32).max(avg).round();
        assert_eq!(intrinsic(avg.max(avg.round()), max), 178.0);
    }
}
