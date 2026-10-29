//! AccessKit projection/action adapter fills the published shell's unimplemented action seam.
use crate::{domain::widgets, infrastructure::document::InteractiveDocument};
use accesskit::{Action, ActionData, ActionRequest, TreeUpdate};

pub fn tree(document: &InteractiveDocument) -> TreeUpdate {
    let mut update = document.renderer.document.build_accessibility_tree();
    if let Some((_, root)) = update.nodes.iter_mut().find(|(id, _)| id.0 == u64::MAX) {
        root.set_label("Meari Native");
    }
    if let Some(info) = &mut update.tree {
        info.toolkit_name = Some("Meari Native".into());
        info.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
    }
    let available: std::collections::BTreeSet<_> =
        update.nodes.iter().map(|(id, _)| id.0).collect();
    for (id, node) in &mut update.nodes {
        if document
            .renderer
            .document
            .get_node(blitz_dom::NodeId::from_u64(id.0))
            .is_some_and(|dom| dom.attr(blitz_dom::LocalName::from("aria-hidden")) == Some("true"))
        {
            node.set_hidden();
        }
    }
    for element in document.elements() {
        let Some(id) = document.renderer.document.get_element_by_id(&element.id) else {
            continue;
        };
        let Some(dom) = document.renderer.document.get_node(id) else {
            continue;
        };
        let Some((_, node)) = update
            .nodes
            .iter_mut()
            .find(|(key, _)| key.0 == id.as_u64())
        else {
            continue;
        };
        let disabled = element.attributes.contains_key("disabled")
            || element
                .attributes
                .get("aria-disabled")
                .is_some_and(|v| v == "true");
        let hidden = document.elements().iter().any(|ancestor| {
            (element.path == ancestor.path
                || element.path.starts_with(&format!("{}/", ancestor.path)))
                && ancestor
                    .attributes
                    .get("aria-hidden")
                    .is_some_and(|v| v == "true")
        });
        if hidden {
            node.set_hidden();
            continue;
        }
        let current = element.attributes.get("aria-current").map(String::as_str);
        let current = match current {
            None | Some("") => None,
            Some(value)
                if value.eq_ignore_ascii_case("false")
                    || value.eq_ignore_ascii_case("undefined") =>
            {
                None
            }
            Some(value) if value.eq_ignore_ascii_case("page") => Some(accesskit::AriaCurrent::Page),
            Some(value) if value.eq_ignore_ascii_case("step") => Some(accesskit::AriaCurrent::Step),
            Some(value) if value.eq_ignore_ascii_case("location") => {
                Some(accesskit::AriaCurrent::Location)
            }
            Some(value) if value.eq_ignore_ascii_case("date") => Some(accesskit::AriaCurrent::Date),
            Some(value) if value.eq_ignore_ascii_case("time") => Some(accesskit::AriaCurrent::Time),
            Some(_) => Some(accesskit::AriaCurrent::True),
        };
        if let Some(current) = current {
            node.set_aria_current(current);
        } else {
            node.clear_aria_current();
        }
        let relation = |name: &str| -> Vec<accesskit::NodeId> {
            element
                .attributes
                .get(name)
                .into_iter()
                .flat_map(|value| value.split_whitespace())
                .filter_map(|id| document.renderer.document.get_element_by_id(id))
                .filter(|id| available.contains(&id.as_u64()))
                .map(|id| accesskit::NodeId(id.as_u64()))
                .collect()
        };
        let labels = relation("aria-labelledby");
        let explicit_labels = !labels.is_empty();
        if explicit_labels {
            node.clear_label();
            node.set_labelled_by(labels);
        } else if let Some(label) = element
            .attributes
            .get("aria-label")
            .filter(|label| !label.trim().is_empty())
        {
            node.set_label(label.clone());
        } else if dom
            .element_data()
            .is_some_and(|data| data.name.local == blitz_dom::local_name!("img"))
            && let Some(alt) = element.attributes.get("alt")
        {
            if alt.is_empty()
                && !["role", "tabindex", "aria-describedby", "aria-labelledby"]
                    .iter()
                    .any(|name| element.attributes.contains_key(*name))
            {
                node.set_hidden();
                continue;
            }
            node.set_label(alt.clone());
        }
        let descriptions = relation("aria-describedby");
        if !descriptions.is_empty() {
            node.set_described_by(descriptions);
        }
        match element.attributes.get("role").map(String::as_str) {
            Some("img") => node.set_role(accesskit::Role::Image),
            Some("checkbox") | Some("switch") | Some("radio") => {
                node.set_role(if element.attributes["role"] == "radio" {
                    accesskit::Role::RadioButton
                } else if element.attributes["role"] == "switch" {
                    accesskit::Role::Switch
                } else {
                    accesskit::Role::CheckBox
                });
                node.set_toggled(
                    if element
                        .attributes
                        .get("aria-checked")
                        .is_some_and(|v| v == "true")
                    {
                        accesskit::Toggled::True
                    } else {
                        accesskit::Toggled::False
                    },
                );
            }
            Some("radiogroup") => node.set_role(accesskit::Role::RadioGroup),
            Some("group") => node.set_role(accesskit::Role::Group),
            Some("alert") => {
                node.set_role(accesskit::Role::Alert);
                node.set_live(accesskit::Live::Assertive);
            }
            _ => {}
        }
        if element
            .attributes
            .get("aria-invalid")
            .is_some_and(|value| value == "true")
        {
            node.set_invalid(accesskit::Invalid::True);
        }
        if disabled {
            node.set_disabled();
        }
        let readonly = element.attributes.contains_key("readonly");
        if readonly {
            node.set_read_only();
        }
        if dom.is_focussable() && !disabled {
            node.add_action(Action::Focus);
        }
        let composite_action = document.session.owner(&element.path).is_some_and(|w| {
            w.trigger.as_ref() == Some(&element.path)
                || w.close_paths.contains(&element.path)
                || w.items.iter().any(|i| i.path == element.path)
        });
        let button = dom
            .element_data()
            .is_some_and(|element| element.name.local == blitz_dom::local_name!("button"));
        let disclosure = document.disclosure_open(&element.path);
        if let Some(open) = disclosure {
            node.set_role(accesskit::Role::DisclosureTriangle);
            node.set_expanded(open);
            node.set_label(dom.text_content());
            if !disabled {
                node.add_action(Action::Click);
                node.add_action(Action::Expand);
                node.add_action(Action::Collapse);
            }
        }
        let labelled_control = element.attributes.contains_key("data-toggle")
            || element.attributes.contains_key("data-file-input");
        if element.attributes.get("role").map(String::as_str) == Some("radio")
            && !explicit_labels
            && let Some(label) = document
                .elements()
                .iter()
                .find(|candidate| candidate.attributes.get("for") == Some(&element.id))
            && let Some(text) = document
                .renderer
                .document
                .get_element_by_id(&label.id)
                .and_then(|id| document.renderer.document.get_node(id))
                .map(|dom| dom.text_content())
        {
            node.set_label(text);
        }
        if (composite_action || button)
            && !labelled_control
            && node.label().is_none()
            && !explicit_labels
        {
            node.set_label(dom.text_content());
        }
        if (composite_action || button) && !disabled {
            node.add_action(Action::Focus);
            node.add_action(Action::Click);
            if element.attributes.contains_key("aria-expanded") {
                node.add_action(Action::Expand);
                node.add_action(Action::Collapse);
            }
        }
        if dom
            .element_data()
            .is_some_and(|e| e.text_input_data().is_some())
            || labelled_control
        {
            if node.label().is_none()
                && !explicit_labels
                && let Some(label) = document
                    .elements()
                    .iter()
                    .find(|e| e.attributes.get("for") == Some(&element.id))
                && let Some(text) = document
                    .renderer
                    .document
                    .get_element_by_id(&label.id)
                    .and_then(|id| document.renderer.document.get_node(id))
                    .map(|node| node.text_content())
            {
                node.set_label(text);
            }
            if let Some(value) = document
                .input_values
                .get(&element.path)
                .or_else(|| element.attributes.get("value"))
            {
                node.set_value(value.clone());
            }
            if !disabled && !readonly && !labelled_control {
                node.add_action(Action::SetValue);
            }
            if let Some(files) = document.file_values.get(&element.path) {
                node.set_value(
                    files
                        .iter()
                        .map(|f| f.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
        }
    }
    update
}

pub fn act(document: &mut InteractiveDocument, request: &ActionRequest) -> Result<(), String> {
    let path = document
        .elements()
        .iter()
        .find(|e| {
            document
                .renderer
                .document
                .get_element_by_id(&e.id)
                .is_some_and(|id| id.as_u64() == request.target_node.0)
        })
        .map(|e| e.path.clone())
        .ok_or("accessibility target no longer exists")?;
    let supported = tree(document)
        .nodes
        .iter()
        .find(|(id, _)| *id == request.target_node)
        .is_some_and(|(_, node)| node.supports_action(request.action));
    if !supported {
        return Err("accessibility action is not supported by this target".into());
    }
    match request.action {
        Action::Expand | Action::Collapse => {
            let open = if let Some(open) = document.disclosure_open(&path) {
                open
            } else {
                document
                    .session
                    .owner(&path)
                    .ok_or("expand target has no widget")?
                    .open
            };
            if open != (request.action == Action::Expand) {
                document.dispatch(widgets::Action::Activate { path })?;
            }
        }
        Action::Click => {
            document.dispatch(widgets::Action::Activate { path })?;
        }
        Action::Focus => {
            document.focus_path(&path)?;
        }
        Action::SetValue => {
            let Some(ActionData::Value(value)) = &request.data else {
                return Err("SetValue requires a value payload".into());
            };
            document.set_input_value(&path, value)?;
        }
        _ => return Err("accessibility action adapter is not implemented".into()),
    }
    Ok(())
}
