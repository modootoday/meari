//! Apply a fully prepared native keyed tree while retaining owned DOM identities.
use super::document::InteractiveDocument;
use crate::application::markup::{Element, Markup};
use std::collections::BTreeMap;

pub(super) fn remap<T: Clone>(
    values: &BTreeMap<String, T>,
    old: &[Element],
    next: &[Element],
) -> BTreeMap<String, T> {
    let paths = old
        .iter()
        .map(|element| (element.path.as_str(), element.id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let next_paths = next
        .iter()
        .map(|element| (element.id.as_str(), element.path.as_str()))
        .collect::<BTreeMap<_, _>>();
    values
        .iter()
        .filter_map(|(path, value)| {
            let id = paths.get(path.as_str())?;
            Some((next_paths.get(id)?.to_string(), value.clone()))
        })
        .collect()
}

impl InteractiveDocument {
    pub(super) fn patch_keyed(&mut self, next: Markup) -> Result<(), String> {
        self.renderer
            .validate_asset_references(next.elements.iter().flat_map(|element| {
                ["src", "poster"]
                    .into_iter()
                    .filter_map(|name| element.attributes.get(name).map(String::as_str))
            }))?;
        let mut inputs = self.input_values.clone();
        for element in &next.elements {
            if !inputs.contains_key(&element.path) {
                continue;
            }
            if !element.preserve_dirty_value {
                if let Some(value) = element.attributes.get("value") {
                    inputs.insert(element.path.clone(), value.clone());
                }
            } else if self
                .elements
                .iter()
                .any(|old| old.id == element.id && !old.preserve_dirty_value)
            {
                inputs.remove(&element.path);
            }
        }
        let prepared =
            super::keyed_patch::prepare(&self.renderer.document, &self.elements, &next, &inputs)?;
        // prepare() completed every fallible tree/control validation before this mutation.
        prepared.apply(&mut self.renderer.document.mutate());
        self.input_values = inputs;
        for element in &next.elements {
            if !element.preserve_dirty_value
                && let Some(value) = element.attributes.get("value")
            {
                let node = self
                    .renderer
                    .document
                    .get_element_by_id(&element.id)
                    .expect("prepared native control ID");
                if self
                    .renderer
                    .document
                    .get_node(node)
                    .and_then(|node| node.element_data())
                    .is_some_and(|data| data.text_input_data().is_some())
                {
                    self.renderer.document.mutate().set_attribute(
                        node,
                        super::document::name("value"),
                        value,
                    );
                }
            }
        }
        self.elements = next.elements;
        let viewport = self.renderer.document.get_viewport();
        let (width, height) = viewport.logical_size();
        self.viewport.width = width.round().max(1.0) as u32;
        self.viewport.height = height.round().max(1.0) as u32;
        self.renderer.document.resolve(0.0);
        Ok(())
    }
}

#[cfg(test)]
mod dirty_value_tests {
    use super::*;
    use crate::domain::Viewport;

    fn component(control: &str) -> meari::domain::Component {
        meari::parse(
            &format!("type Props={{}};export function Editor(props:Props){{return <div data-meari-repeat=\"stable\">{control}</div>;}}"),
            meari::application::Language::Tsx,
        ).unwrap()
    }

    fn text(document: &InteractiveDocument) -> String {
        document
            .renderer
            .document
            .get_node(
                document
                    .renderer
                    .document
                    .get_element_by_id(document.path_id("0/0").unwrap())
                    .unwrap(),
            )
            .unwrap()
            .element_data()
            .unwrap()
            .text_input_data()
            .unwrap()
            .editor
            .raw_text()
            .to_string()
    }

    #[test]
    fn retained_plain_default_and_textarea_keep_dirty_while_controlled_value_wins() {
        for (before, after, initial, expected) in [
            ("<input/>", "<input aria-label=\"next\"/>", "", "dirty"),
            (
                "<Input defaultValue=\"initial\"/>",
                "<Input defaultValue=\"next\"/>",
                "initial",
                "dirty",
            ),
            (
                "<textarea/>",
                "<textarea aria-label=\"next\"/>",
                "",
                "dirty",
            ),
            (
                "<input value=\"initial\"/>",
                "<input value=\"next\"/>",
                "initial",
                "next",
            ),
        ] {
            let mut document = InteractiveDocument::new(
                component(before),
                "input,textarea{width:120px;height:30px}",
                Viewport {
                    width: 640,
                    height: 480,
                    dark: false,
                },
            )
            .unwrap();
            assert_eq!(text(&document), initial, "{before}");
            let node = document
                .renderer
                .document
                .get_element_by_id(document.path_id("0/0").unwrap())
                .unwrap();
            document.set_input_value("0/0", "dirty").unwrap();
            assert_eq!(text(&document), "dirty");
            document
                .commit_component_projection(component(after))
                .unwrap();
            assert_eq!(text(&document), expected, "{after}");
            assert_eq!(document.input_values["0/0"], expected);
            assert_eq!(
                document
                    .renderer
                    .document
                    .get_element_by_id(document.path_id("0/0").unwrap()),
                Some(node)
            );
        }
    }

    #[test]
    fn retained_nondirty_initial_values_and_raw_control_transitions_do_not_restore_old_edits() {
        for control in [
            "<input/>",
            "<Input defaultValue=\"initial\"/>",
            "<textarea/>",
        ] {
            let mut document = InteractiveDocument::new(
                component(control),
                "",
                Viewport {
                    width: 640,
                    height: 480,
                    dark: false,
                },
            )
            .unwrap();
            let initial = text(&document);
            document
                .commit_component_projection(component(control))
                .unwrap();
            assert_eq!(text(&document), initial);
            assert!(document.input_values.is_empty());
        }
        let mut document = InteractiveDocument::new(
            component("<input/>"),
            "",
            Viewport {
                width: 640,
                height: 480,
                dark: false,
            },
        )
        .unwrap();
        document.set_input_value("0/0", "old dirty").unwrap();
        document
            .commit_component_projection(component("<input value=\"host\"/>"))
            .unwrap();
        assert_eq!(text(&document), "host");
        assert_eq!(document.input_values["0/0"], "host");
        document
            .commit_component_projection(component("<input/>"))
            .unwrap();
        assert_eq!(text(&document), "");
        assert!(!document.input_values.contains_key("0/0"));
        document
            .commit_component_projection(component("<input/>"))
            .unwrap();
        assert_eq!(text(&document), "");
    }
}
