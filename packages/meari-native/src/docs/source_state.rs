use crate::{
    domain::widgets::Action,
    infrastructure::{accessibility, artifacts, document::InteractiveDocument},
};
use std::path::Path;

pub(super) fn verify(
    doc: &mut InteractiveDocument,
    directory: Option<&Path>,
) -> Result<usize, String> {
    let path = |id: &str| {
        doc.elements()
            .iter()
            .find(|element| element.id == id)
            .map(|element| element.path.clone())
            .ok_or_else(|| format!("source-state fixture target missing: {id}"))
    };
    let increment = path("increment")?;
    let rename = path("rename")?;
    let toggle = path("toggle")?;
    let input = path("name")?;
    let units = doc
        .renderer
        .document
        .get_element_by_id("units")
        .ok_or("source-state units missing")?;
    let input_node = doc
        .renderer
        .document
        .get_element_by_id("name")
        .ok_or("source-state input missing")?;
    doc.set_input_value(&input, "State oracle retained input")?;
    doc.focus_path(&input)?;
    let mut frames = Vec::new();
    for (index, action) in [
        Action::Activate {
            path: increment.clone(),
        },
        Action::Key {
            path: increment.clone(),
            key: "Enter".into(),
            shift: false,
        },
        Action::Activate { path: rename },
        Action::Activate {
            path: toggle.clone(),
        },
        Action::Activate {
            path: increment.clone(),
        },
        Action::Activate { path: toggle },
    ]
    .into_iter()
    .enumerate()
    {
        let serialized = serde_json::to_value(&action).map_err(|error| error.to_string())?;
        let changed = doc.dispatch(action)?;
        if changed == (index == 4) {
            return Err("source-state disabled/activation oracle mismatch".into());
        }
        let snapshot = doc.component_state()?;
        let state: serde_json::Value =
            serde_json::to_value(&snapshot).map_err(|error| error.to_string())?;
        let expected_count = if index == 0 { 1.0 } else { 2.0 };
        if state["values"]["count"]["value"].as_f64() != Some(expected_count) {
            return Err("source-state functional updater count mismatch".into());
        }
        let status = doc
            .renderer
            .document
            .get_element_by_id("status")
            .ok_or("state status missing")?;
        let expected = format!(
            "Count {} units{}",
            expected_count as u32,
            if index >= 2 { " ready🦀" } else { "" }
        );
        if doc
            .renderer
            .document
            .get_node(status)
            .ok_or("state status DOM missing")?
            .text_content()
            != expected
        {
            return Err("source-state mixed text projection mismatch".into());
        }
        if index == 3
            && !accessibility::tree(doc)
                .nodes
                .iter()
                .any(|(_, node)| node.is_disabled())
        {
            return Err("state disabled action absent from accessibility tree".into());
        }
        let capture = directory
            .map(|directory| {
                artifacts::capture(doc, directory, &format!("source-state-{index:04}"))
            })
            .transpose()?;
        frames.push(serde_json::json!({"action":serialized,"changed":changed,"state":snapshot,"capture":capture}));
    }
    if doc.renderer.document.get_element_by_id("units") != Some(units)
        || doc.renderer.document.get_element_by_id("name") != Some(input_node)
        || doc.input_values.get(&input).map(String::as_str) != Some("State oracle retained input")
        || doc.session.focus.as_deref() != Some(input.as_str())
    {
        return Err("state render replaced unrelated DOM/input/focus".into());
    }
    if doc
        .renderer
        .document
        .get_node(input_node)
        .and_then(|node| node.element_data())
        .and_then(|element| element.text_input_data())
        .is_none_or(|input| input.editor.text() != "State oracle retained input")
    {
        return Err("state defaultValue update overwrote the actual native editor value".into());
    }
    if let Some(directory) = directory {
        artifacts::json(&directory.join("source-state-actions.json"), &frames)?;
    }
    Ok(frames.len())
}
