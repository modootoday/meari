//! `<progress>` painted like Chromium (no native widget in Blitz).
//!
//! Chromium lays it out as a 10em × 1em inline-block lowered by 0.2em. Unstyled, it paints
//! its Linux control: an 8px track (`#efefef`, half-transparent grey edge, radius 4px) with a
//! `#0075ff` fill. Once the author styles its border or background — Tailwind's preflight
//! `border:0 solid` always does — Chromium drops the native appearance and paints the
//! fallback bar: grey `::-webkit-progress-bar` with a green `::-webkit-progress-value` over the
//! content box. Both are reproduced; the fill width is `--meari-progress`. An indeterminate
//! bar (no `value`) shows its empty bar, without Chromium's animation. Known limit: in the
//! native look the track is the border box centred by 0.25em margins, so an author margin on
//! an otherwise unstyled `<progress>` moves the track by its own height.
use blitz_dom::NodeId;
use blitz_html::HtmlDocument;

const PROPERTY: &str = "--meari-progress";
const LOOK: &str = "data-meari-progress";
pub(super) const CSS: &str = "progress{display:inline-block;box-sizing:border-box;width:10em;\
height:1em;vertical-align:-.2em}\
progress[data-meari-progress=native]{height:.5em;margin:.25em 0;\
border:1px solid rgba(118,118,118,.5);border-radius:.25em;\
background:linear-gradient(#0075ff,#0075ff) 0 0/var(--meari-progress,0%) 100% no-repeat \
border-box,#efefef}\
progress[data-meari-progress=fallback]{background-image:linear-gradient(green,green),\
linear-gradient(gray,gray);background-size:var(--meari-progress,0%) 100%,100% 100%;\
background-repeat:no-repeat;background-origin:content-box;background-clip:content-box}";

/// Whether Chromium keeps the native appearance: no author border or background. In the
/// native look our own rule's border and background are recognised by their exact values.
fn native(style: &style::properties::ComputedValues, current: Option<&str>) -> bool {
    use style::values::computed::BorderStyle;
    let border = style.get_border();
    let color = style.resolve_color(&style.get_background().background_color);
    let edge = style.resolve_color(&border.border_top_color);
    let rgba = |c: &style::color::AbsoluteColor| {
        c.to_color_space(style::color::ColorSpace::Srgb)
            .raw_components()
            .map(|v| (v * 255.0).round() as i32)
    };
    if current == Some("native") {
        border.border_top_style == BorderStyle::Solid
            && rgba(&edge) == [118, 118, 118, 128]
            && rgba(&color) == [239, 239, 239, 255]
    } else {
        [
            border.border_top_style,
            border.border_right_style,
            border.border_bottom_style,
            border.border_left_style,
        ]
        .iter()
        .all(|side| *side == BorderStyle::None)
            && color.alpha == 0.0
    }
}

/// Filled fraction per HTML: value clamped to [0, max]; max defaults to 1; no value → none.
pub(super) fn fraction(value: Option<&str>, max: Option<&str>) -> f64 {
    let number = |text: Option<&str>| text.and_then(|text| text.trim().parse::<f64>().ok());
    let Some(value) = number(value).filter(|value| value.is_finite()) else {
        return 0.0;
    };
    let max = number(max)
        .filter(|max| max.is_finite() && *max > 0.0)
        .unwrap_or(1.0);
    (value / max).clamp(0.0, 1.0)
}

fn attr<'a>(doc: &'a HtmlDocument, id: NodeId, name: &str) -> Option<&'a str> {
    doc.get_node(id)?.attr(blitz_dom::LocalName::from(name))
}

/// Publishes each progress element's fill; true when any changed.
pub(super) fn refresh(
    doc: &mut HtmlDocument,
    published: &mut std::collections::HashMap<NodeId, String>,
) -> bool {
    let mut queue = vec![doc.root_element().id];
    let mut ids = Vec::new();
    while let Some(id) = queue.pop() {
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        queue.extend(node.children.iter().copied());
        if node
            .element_data()
            .is_some_and(|e| AsRef::<str>::as_ref(&e.name.local) == "progress")
        {
            ids.push(id);
        }
    }
    let mut changed = false;
    for id in ids {
        let current = attr(doc, id, LOOK).map(str::to_owned);
        let look = doc
            .get_node(id)
            .and_then(|node| node.primary_styles())
            .map(|style| {
                if native(&style, current.as_deref()) {
                    "native"
                } else {
                    "fallback"
                }
            });
        if let Some(look) = look
            && current.as_deref() != Some(look)
        {
            doc.mutate().set_attribute(
                id,
                blitz_dom::QualName::new(
                    None,
                    blitz_dom::Namespace::from(""),
                    blitz_dom::LocalName::from(LOOK),
                ),
                look,
            );
            changed = true;
        }
        let fill = fraction(attr(doc, id, "value"), attr(doc, id, "max"));
        let value = format!("{}%", fill * 100.0);
        if published.get(&id) != Some(&value) {
            doc.set_style_property(id, PROPERTY, &value);
            published.insert(id, value);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::fraction;

    #[test]
    fn html_progress_value_rules() {
        assert_eq!(fraction(Some("40"), Some("100")), 0.4);
        assert_eq!(fraction(Some("0.5"), None), 0.5);
        assert_eq!(fraction(Some("7"), Some("5")), 1.0);
        assert_eq!(fraction(Some("-1"), Some("5")), 0.0);
        assert_eq!(fraction(Some("2"), Some("0")), 1.0);
        assert_eq!(fraction(None, Some("5")), 0.0);
        assert_eq!(fraction(Some("x"), None), 0.0);
    }
}
