use super::*;
use crate::{Attribute, qual_name};
use blitz_traits::shell::ColorScheme;

/// Build `<html><body style="margin:0"><div style="width:300px">some text
/// <div style="height:50px"></div></div></body></html>` manually (the HTML
/// parser lives in blitz-html, which would be a circular dev-dependency).
/// The bare text next to a block sibling gets wrapped in an anonymous
/// block, which becomes the inline root: text hits report the anonymous
/// block as the hit node.
fn make_doc() -> (BaseDocument, NodeId) {
    let mut doc = BaseDocument::new(DocumentConfig {
        viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
        ..Default::default()
    });
    let root_id = doc.root_node().id;
    let style = |value: &str| Attribute {
        name: qual_name!("style"),
        value: value.to_string(),
    };

    let mut mutator = doc.mutate();
    let html = mutator.create_element(qual_name!("html"), vec![]);
    let body = mutator.create_element(qual_name!("body"), vec![style("margin:0")]);
    let container = mutator.create_element(qual_name!("div"), vec![style("width:300px")]);
    let text = mutator.create_text_node("some text");
    let block = mutator.create_element(qual_name!("div"), vec![style("height:50px")]);
    mutator.append_children(container, &[text, block]);
    mutator.append_children(body, &[container]);
    mutator.append_children(html, &[body]);
    mutator.append_children(root_id, &[html]);
    drop(mutator);

    doc.resolve(0.0);
    (doc, container)
}

/// Whether text laid out with a real (non-zero-metric) font. Without the
/// `system-fonts` feature text measures 0x0 and text hits are impossible,
/// making these tests vacuous.
fn text_has_size(doc: &BaseDocument, container: NodeId) -> bool {
    doc.nodes[container].final_layout().size.height > 50.0
}

/// Regression test: hovering bare text wrapped in an anonymous block must
/// report a text cursor. The hit node for such text is the anonymous
/// inline root itself, while the *stored* hover target is canonicalized to
/// the containing element — the cursor must be derived from the precise
/// hit node, not the canonical target.
#[test]
fn hovering_text_in_anonymous_block_reports_text_cursor() {
    let (mut doc, container) = make_doc();
    if !text_has_size(&doc, container) {
        eprintln!("skipping: no usable font (text measures 0x0)");
        return;
    }

    doc.set_hover_to(5.0, 8.0);
    assert!(doc.hover_node_is_text, "expected a text hit");
    let hit_id = doc.hover_hit_node_id.expect("expected a hit node");
    assert!(
        doc.nodes[hit_id].is_anonymous(),
        "expected the hit node to be the anonymous inline root"
    );
    assert_eq!(
        doc.get_hover_node_id(),
        Some(container),
        "expected the stored hover target to be the containing element"
    );
    assert_eq!(doc.get_cursor(), Some(CursorIcon::Text));
}

/// Hovering the empty region of the anonymous block (right of the text) is
/// not a text hit: default cursor, same canonical hover target.
#[test]
fn hovering_anonymous_block_whitespace_reports_default_cursor() {
    let (mut doc, container) = make_doc();
    if !text_has_size(&doc, container) {
        eprintln!("skipping: no usable font (text measures 0x0)");
        return;
    }

    doc.set_hover_to(250.0, 8.0);
    assert!(!doc.hover_node_is_text);
    assert_eq!(doc.get_hover_node_id(), Some(container));
    assert_eq!(doc.get_cursor(), Some(CursorIcon::Default));
}
