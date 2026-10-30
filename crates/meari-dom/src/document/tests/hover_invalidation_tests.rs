use super::*;
use crate::{Attribute, QualName, qual_name};
use blitz_traits::shell::ColorScheme;

/// Build `<html><body style="margin:0"><div style="width:300px;height:100px">
/// <span>text</span></div></body></html>` with a `div:hover` rule.
fn make_doc() -> (BaseDocument, NodeId, NodeId) {
    let mut doc = BaseDocument::new(DocumentConfig {
        viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
        ..Default::default()
    });
    let sheet = UserAgentStylesheet::parse(
        "div:hover { background-color: rgb(255, 0, 0); } div:hover span { color: rgb(0, 255, 0); }",
    );
    doc.add_user_agent_stylesheet(sheet);
    let root_id = doc.root_node().id;
    let style = |value: &str| Attribute {
        name: qual_name!("style"),
        value: value.to_string(),
    };

    let mut mutator = doc.mutate();
    let html = mutator.create_element(qual_name!("html"), vec![]);
    let body = mutator.create_element(qual_name!("body"), vec![style("margin:0")]);
    let div = mutator.create_element(qual_name!("div"), vec![style("width:300px;height:100px")]);
    let span = mutator.create_element(qual_name!("span"), vec![]);
    let text = mutator.create_text_node("some text");
    mutator.append_children(span, &[text]);
    mutator.append_children(div, &[span]);
    mutator.append_children(body, &[div]);
    mutator.append_children(html, &[body]);
    mutator.append_children(root_id, &[html]);
    drop(mutator);

    doc.resolve(0.0);
    (doc, div, span)
}

fn bg_color(doc: &BaseDocument, id: NodeId) -> String {
    format!(
        "{:?}",
        doc.nodes[id]
            .primary_styles()
            .unwrap()
            .get_background()
            .background_color
    )
}

fn text_color(doc: &BaseDocument, id: NodeId) -> String {
    format!(
        "{:?}",
        doc.nodes[id].primary_styles().unwrap().clone_color()
    )
}

#[test]
fn hover_styles_apply_and_clear() {
    let (mut doc, div, span) = make_doc();
    let initial_bg = bg_color(&doc, div);
    let initial_color = text_color(&doc, span);

    // Hover the div
    doc.set_hover_to(10.0, 10.0);
    assert!(doc.nodes[div].is_hovered());
    doc.resolve(0.0);
    let hovered_bg = bg_color(&doc, div);
    let hovered_color = text_color(&doc, span);
    assert_ne!(initial_bg, hovered_bg, "hover should change div background");
    assert_ne!(
        initial_color, hovered_color,
        "hover should change span color"
    );

    // Move the pointer off the div (below it, over the body)
    doc.set_hover_to(10.0, 200.0);
    assert!(!doc.nodes[div].is_hovered());
    doc.resolve(0.0);
    assert_eq!(
        bg_color(&doc, div),
        initial_bg,
        "unhover should restore div background"
    );
    assert_eq!(
        text_color(&doc, span),
        initial_color,
        "unhover should restore span color"
    );
}

/// Mimics BBC's headline hover pattern: `a:link:hover .headline` with the
/// headline several levels below the anchor.
#[test]
fn ancestor_hover_with_link_state_updates_descendant() {
    let mut doc = BaseDocument::new(DocumentConfig {
        viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
        ..Default::default()
    });
    let sheet = UserAgentStylesheet::parse(
        ".promo:link:hover .headline, .promo:visited:hover .headline { color: rgb(184, 0, 0); text-decoration-line: underline; }",
    );
    doc.add_user_agent_stylesheet(sheet);
    let root_id = doc.root_node().id;
    let attr = |name: QualName, value: &str| Attribute {
        name,
        value: value.to_string(),
    };

    let mut mutator = doc.mutate();
    let html = mutator.create_element(qual_name!("html"), vec![]);
    let body = mutator.create_element(
        qual_name!("body"),
        vec![attr(qual_name!("style"), "margin:0")],
    );
    let a = mutator.create_element(
        qual_name!("a"),
        vec![
            attr(qual_name!("href"), "https://example.com"),
            attr(qual_name!("class"), "promo"),
            attr(
                qual_name!("style"),
                "display:block;width:300px;height:100px",
            ),
        ],
    );
    let p = mutator.create_element(qual_name!("p"), vec![]);
    let span = mutator.create_element(
        qual_name!("span"),
        vec![attr(qual_name!("class"), "headline")],
    );
    let text = mutator.create_text_node("Headline text");
    mutator.append_children(span, &[text]);
    mutator.append_children(p, &[span]);
    mutator.append_children(a, &[p]);
    mutator.append_children(body, &[a]);
    mutator.append_children(html, &[body]);
    mutator.append_children(root_id, &[html]);
    drop(mutator);

    doc.resolve(0.0);
    let initial_color = text_color(&doc, span);

    doc.set_hover_to(10.0, 10.0);
    assert!(doc.nodes[a].is_hovered());
    doc.resolve(0.0);
    let hovered_color = text_color(&doc, span);
    assert_ne!(
        initial_color, hovered_color,
        "hovering the anchor should change the headline color"
    );

    doc.set_hover_to(10.0, 200.0);
    assert!(!doc.nodes[a].is_hovered());
    doc.resolve(0.0);
    assert_eq!(
        text_color(&doc, span),
        initial_color,
        "unhovering the anchor should restore the headline color"
    );
}

/// Toggling a checkbox must invalidate `:checked`-dependent styles.
#[test]
fn checkbox_toggle_updates_checked_styles() {
    let mut doc = BaseDocument::new(DocumentConfig {
        viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
        ..Default::default()
    });
    let sheet = UserAgentStylesheet::parse(
        "input:checked { color: rgb(184, 0, 0); } input:checked + label { color: rgb(0, 184, 0); }",
    );
    doc.add_user_agent_stylesheet(sheet);
    let root_id = doc.root_node().id;
    let attr = |name: QualName, value: &str| Attribute {
        name,
        value: value.to_string(),
    };

    let mut mutator = doc.mutate();
    let html = mutator.create_element(qual_name!("html"), vec![]);
    let body = mutator.create_element(
        qual_name!("body"),
        vec![attr(qual_name!("style"), "margin:0")],
    );
    let input = mutator.create_element(
        qual_name!("input"),
        vec![attr(qual_name!("type"), "checkbox")],
    );
    let label = mutator.create_element(qual_name!("label"), vec![]);
    let text = mutator.create_text_node("label text");
    mutator.append_children(label, &[text]);
    mutator.append_children(body, &[input, label]);
    mutator.append_children(html, &[body]);
    mutator.append_children(root_id, &[html]);
    drop(mutator);

    doc.resolve(0.0);
    let initial_input_color = text_color(&doc, input);
    let initial_label_color = text_color(&doc, label);

    // Toggle the checkbox on (as the click handler does)
    doc.snapshot_node_and(input, ElementState::CHECKED, |node| {
        if let Some(el) = node.element_data_mut() {
            BaseDocument::toggle_checkbox(el);
        }
        node.mark_ancestors_dirty();
    });
    doc.resolve(0.0);
    assert_ne!(
        text_color(&doc, input),
        initial_input_color,
        "checking should change the input color"
    );
    assert_ne!(
        text_color(&doc, label),
        initial_label_color,
        "checking should change the sibling label color"
    );

    // Toggle the checkbox back off
    doc.snapshot_node_and(input, ElementState::CHECKED, |node| {
        if let Some(el) = node.element_data_mut() {
            BaseDocument::toggle_checkbox(el);
        }
        node.mark_ancestors_dirty();
    });
    doc.resolve(0.0);
    assert_eq!(
        text_color(&doc, input),
        initial_input_color,
        "unchecking should restore the input color"
    );
    assert_eq!(
        text_color(&doc, label),
        initial_label_color,
        "unchecking should restore the sibling label color"
    );
}

/// A repaint-only style change to an existing `::before` pseudo-element
/// (no box construction damage) must be flushed to the pseudo-element's
/// anonymous node during the style traversal.
#[test]
fn hover_updates_existing_pseudo_element_style() {
    let mut doc = BaseDocument::new(DocumentConfig {
        viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
        ..Default::default()
    });
    let sheet = UserAgentStylesheet::parse(
        "div::before { content: \"x\"; color: rgb(1, 2, 3); } \
         div:hover::before { color: rgb(0, 0, 255); }",
    );
    doc.add_user_agent_stylesheet(sheet);
    let root_id = doc.root_node().id;
    let style = |value: &str| Attribute {
        name: qual_name!("style"),
        value: value.to_string(),
    };

    let mut mutator = doc.mutate();
    let html = mutator.create_element(qual_name!("html"), vec![]);
    let body = mutator.create_element(qual_name!("body"), vec![style("margin:0")]);
    let div = mutator.create_element(
        qual_name!("div"),
        vec![style("display:block;width:300px;height:100px")],
    );
    let text = mutator.create_text_node("some text");
    mutator.append_children(div, &[text]);
    mutator.append_children(body, &[div]);
    mutator.append_children(html, &[body]);
    mutator.append_children(root_id, &[html]);
    drop(mutator);

    doc.resolve(0.0);
    let before = doc.nodes[div].before().expect("::before node should exist");
    let initial_color = text_color(&doc, before);

    doc.set_hover_to(10.0, 10.0);
    assert!(doc.nodes[div].is_hovered());
    doc.resolve(0.0);
    assert_ne!(
        text_color(&doc, before),
        initial_color,
        "hover should change the ::before color"
    );

    doc.set_hover_to(10.0, 200.0);
    assert!(!doc.nodes[div].is_hovered());
    doc.resolve(0.0);
    assert_eq!(
        text_color(&doc, before),
        initial_color,
        "unhover should restore the ::before color"
    );
}

/// Background-image layers must be flushed to the node's dedicated image
/// storage when its style changes (queued during the style traversal),
/// now that image flushing no longer runs on every node in the flush pass.
#[test]
fn hover_updates_background_image_layers() {
    let mut doc = BaseDocument::new(DocumentConfig {
        viewport: Some(Viewport::new(400, 300, 1.0, ColorScheme::Light)),
        ..Default::default()
    });
    let sheet = UserAgentStylesheet::parse(
        "div { background-image: url(\"https://example.com/a.png\"); } \
         div:hover { background-image: url(\"https://example.com/b.png\"); }",
    );
    doc.add_user_agent_stylesheet(sheet);
    let root_id = doc.root_node().id;
    let style = |value: &str| Attribute {
        name: qual_name!("style"),
        value: value.to_string(),
    };

    let mut mutator = doc.mutate();
    let html = mutator.create_element(qual_name!("html"), vec![]);
    let body = mutator.create_element(qual_name!("body"), vec![style("margin:0")]);
    let div = mutator.create_element(
        qual_name!("div"),
        vec![style("display:block;width:300px;height:100px")],
    );
    mutator.append_children(body, &[div]);
    mutator.append_children(html, &[body]);
    mutator.append_children(root_id, &[html]);
    drop(mutator);

    let background_image_url = |doc: &BaseDocument, id: NodeId| -> Option<String> {
        let elem = doc.nodes[id].data.downcast_element().unwrap();
        elem.background_images
            .first()
            .and_then(|img| img.as_ref())
            .map(|img| img.url.as_str().to_string())
    };

    doc.resolve(0.0);
    assert_eq!(
        background_image_url(&doc, div).as_deref(),
        Some("https://example.com/a.png"),
        "initial resolve should flush the background image"
    );

    doc.set_hover_to(10.0, 10.0);
    assert!(doc.nodes[div].is_hovered());
    doc.resolve(0.0);
    assert_eq!(
        background_image_url(&doc, div).as_deref(),
        Some("https://example.com/b.png"),
        "hover should flush the changed background image"
    );

    doc.set_hover_to(10.0, 200.0);
    assert!(!doc.nodes[div].is_hovered());
    doc.resolve(0.0);
    assert_eq!(
        background_image_url(&doc, div).as_deref(),
        Some("https://example.com/a.png"),
        "unhover should flush the restored background image"
    );
}
