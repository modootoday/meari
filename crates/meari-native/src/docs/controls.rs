//! Ordinary controls share one headless execution oracle across owned/upstream fixtures.
use crate::{
    application::Renderer,
    domain::widgets::Action,
    infrastructure::{artifacts, document::InteractiveDocument},
};
use meari::domain::{Node, NodeKind, Value};
use std::path::Path;

fn collect(node: &Node, path: String, out: &mut Vec<(String, bool)>) {
    if let NodeKind::Element {
        tag,
        attributes,
        children,
    } = &node.kind
    {
        let blocked = attributes.iter().any(|a| {
            matches!(a.name.as_str(), "disabled" | "readOnly") && a.value == Value::Bool(true)
        });
        if !blocked
            && matches!(
                tag.as_str(),
                "Button" | "button" | "Input" | "input" | "Checkbox" | "Switch"
            )
        {
            out.push((path.clone(), matches!(tag.as_str(), "Button" | "button")));
        }
        for (i, child) in children.iter().enumerate() {
            collect(child, format!("{path}/{i}"), out);
        }
    }
}

pub(super) fn verify(
    doc: &mut InteractiveDocument,
    directory: Option<&Path>,
) -> Result<(usize, usize), String> {
    let mut controls = Vec::new();
    collect(&doc.source.root, "0".into(), &mut controls);
    let (mut buttons, mut inputs) = (0, 0);
    let mut frames = Vec::new();
    for (path, button) in controls {
        if doc.session.is_control(&path)
            || doc.elements().iter().any(|e| {
                (path == e.path || path.starts_with(&format!("{}/", e.path)))
                    && (e.attributes.contains_key("disabled")
                        || e.attributes.contains_key("hidden")
                        || e.attributes.contains_key("inert")
                        || e.attributes.get("aria-hidden").is_some_and(|v| v == "true"))
            })
        {
            continue;
        }
        let Some(element) = doc.elements().iter().find(|e| e.path == path) else {
            return Err("missing ordinary control element".into());
        };
        if !doc
            .renderer
            .bounds(&element.id)
            .is_ok_and(|bounds| bounds.width > 0.0 && bounds.height > 0.0)
        {
            continue;
        }
        let file = element.attributes.contains_key("data-file-input");
        let toggle = element.attributes.contains_key("data-toggle");
        let action = if file {
            verify_files(doc, &path, directory)?;
            inputs += 1;
            serde_json::json!({"FileSelectionCancelClear":{"path":path,"adapter":"injected"}})
        } else if toggle {
            let before = element.attributes.get("aria-checked").cloned();
            doc.dispatch(Action::Activate { path: path.clone() })?;
            if doc
                .elements()
                .iter()
                .find(|e| e.path == path)
                .and_then(|e| e.attributes.get("aria-checked"))
                == before.as_ref()
            {
                return Err("toggle did not change state".into());
            }
            record(
                doc,
                directory,
                &format!("toggle-{}-changed", path.replace('/', "-")),
            )?;
            doc.dispatch(Action::Key {
                path: path.clone(),
                key: " ".into(),
                shift: false,
            })?;
            if doc
                .elements()
                .iter()
                .find(|e| e.path == path)
                .and_then(|e| e.attributes.get("aria-checked"))
                != before.as_ref()
            {
                return Err("Space did not restore toggle state".into());
            }
            buttons += 1;
            serde_json::json!({"ToggleClickSpace":{"path":path}})
        } else if button && doc.has_state_event(&path) {
            let before = doc.component_state()?;
            doc.dispatch(Action::Activate { path: path.clone() })?;
            let after = doc.component_state()?;
            if before.instance != after.instance || after.revision < before.revision {
                return Err("source state action lost its instance/revision".into());
            }
            buttons += 1;
            serde_json::json!({"StateActivate":{"path":path,"before":before,"after":after}})
        } else if button {
            let before = doc.activation_requests.len();
            doc.dispatch(Action::Activate { path: path.clone() })?;
            if doc.activation_requests.len() != before + 1
                || doc.activation_requests.last() != Some(&path)
            {
                return Err("button activation request missing".into());
            }
            buttons += 1;
            serde_json::json!({"Activate":{"path":path}})
        } else {
            doc.set_input_value(&path, "Kitchen sink input receipt")?;
            if doc.input_values.get(&path).map(String::as_str) != Some("Kitchen sink input receipt")
            {
                return Err("input value did not persist".into());
            }
            inputs += 1;
            serde_json::json!({"SetValue":{"path":path,"value":"Kitchen sink input receipt"}})
        };
        if let Some(directory) = directory {
            let capture =
                artifacts::capture(doc, directory, &format!("control-{:04}", frames.len()))?;
            frames.push(serde_json::json!({"action":action,"capture":capture}));
            artifacts::json(&directory.join("controls.json"), &frames)?;
        }
    }
    Ok((buttons, inputs))
}

fn record(
    doc: &mut InteractiveDocument,
    directory: Option<&Path>,
    name: &str,
) -> Result<(), String> {
    if let Some(directory) = directory {
        let capture = artifacts::capture(doc, directory, name)?;
        artifacts::json(&directory.join(format!("{name}.json")), &capture)?;
    }
    Ok(())
}

fn verify_files(
    doc: &mut InteractiveDocument,
    path: &str,
    directory: Option<&Path>,
) -> Result<(), String> {
    use crate::application::file_picker::{FilePicker, FileRequest, SelectedFile};
    struct Injected(Option<Vec<SelectedFile>>);
    impl FilePicker for Injected {
        fn select(&mut self, _: &FileRequest) -> Result<Option<Vec<SelectedFile>>, String> {
            Ok(self.0.take())
        }
    }
    let file = SelectedFile {
        path: "kitchen-sink-picture.png".into(),
        name: "kitchen-sink-picture.png".into(),
    };
    doc.dispatch(Action::Activate { path: path.into() })?;
    if doc.file_requests.last().map(String::as_str) != Some(path) {
        return Err("file picker request missing".into());
    }
    doc.process_file_requests(&mut Injected(Some(vec![file.clone()])))?;
    if doc.file_values.get(path) != Some(&vec![file.clone()]) {
        return Err("selected file not stored".into());
    }
    record(
        doc,
        directory,
        &format!("file-{}-selected", path.replace('/', "-")),
    )?;
    doc.choose_files(path, &mut Injected(None))?;
    if doc.file_values.get(path) != Some(&vec![file]) {
        return Err("cancel lost previous selection".into());
    }
    record(
        doc,
        directory,
        &format!("file-{}-cancelled", path.replace('/', "-")),
    )?;
    doc.choose_files(path, &mut Injected(Some(Vec::new())))?;
    if !doc.file_values.get(path).is_some_and(Vec::is_empty) {
        return Err("clear failed".into());
    }
    record(
        doc,
        directory,
        &format!("file-{}-cleared", path.replace('/', "-")),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn hidden_and_disabled_controls_are_not_counted_as_exercised() {
        let source=meari::parse("type Props={};export function Demo(props:Props){return <div><div hidden><Button>Hidden</Button><Input id=\"hidden-input\"/></div><Button>Visible</Button><Input id=\"visible-input\"/><Input readOnly/><Button disabled>Disabled</Button><Button className=\"hidden\">CSS hidden</Button><Input className=\"hidden\"/></div>;}",meari::application::Language::Tsx).unwrap();
        let mut doc = crate::infrastructure::document::InteractiveDocument::new(
            source,
            "[hidden],.hidden{display:none!important}",
            crate::domain::Viewport {
                width: 400,
                height: 300,
                dark: false,
            },
        )
        .unwrap();
        assert_eq!(super::verify(&mut doc, None).unwrap(), (1, 1));
        assert_eq!(doc.activation_requests, vec!["0/1"]);
        assert_eq!(doc.input_values.len(), 1);
        assert_eq!(doc.input_values["0/2"], "Kitchen sink input receipt");
    }
}
