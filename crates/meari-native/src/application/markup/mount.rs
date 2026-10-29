//! Presence metadata is separate from visibility and widget open state.
use meari::domain::{Node, NodeKind, Value};
use std::collections::BTreeSet;

pub(super) fn collect(root: &Node) -> Result<BTreeSet<String>, String> {
    fn visit(
        node: &Node,
        path: String,
        inherited: bool,
        out: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        let NodeKind::Element {
            tag,
            attributes,
            children,
        } = &node.kind
        else {
            return Ok(());
        };
        let local = attributes
            .iter()
            .find(|attribute| attribute.name == "forceMount");
        if local.is_some()
            && !matches!(
                tag.as_str(),
                "DialogPortal" | "DialogContent" | "DialogOverlay"
            )
        {
            return Err(format!(
                "native {tag}.forceMount requires its semantic adapter"
            ));
        }
        let forced = match local.map(|attribute| &attribute.value) {
            Some(Value::Bool(value)) => *value,
            None => inherited,
            _ => return Err(format!("native {tag}.forceMount requires bound bool")),
        };
        if forced && matches!(tag.as_str(), "DialogContent" | "DialogOverlay") {
            out.insert(path.clone());
        }
        let inherited = if tag == "DialogPortal" {
            forced
        } else if tag == "Dialog" {
            false
        } else {
            inherited
        };
        for (index, child) in children.iter().enumerate() {
            visit(child, format!("{path}/{index}"), inherited, out)?;
        }
        Ok(())
    }
    let mut result = BTreeSet::new();
    visit(root, "0".into(), false, &mut result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::{application::markup, domain::widgets::Session};

    fn markup(source: &str) -> Result<markup::Markup, String> {
        let component = meari::parse(source, meari::application::Language::Tsx).unwrap();
        let session = Session::from_tree(&component.root)?;
        markup::build(&component, &session)
    }

    #[test]
    fn retained_closed_content_obeys_explicit_visibility_and_portal_inheritance() {
        let output = markup("type Props={};export function Panel(props:Props){return <Dialog open={false} modal={false}><DialogTrigger>open</DialogTrigger><DialogPortal forceMount><DialogContent aria-label=\"inspector\"><Input defaultValue=\"draft\"/></DialogContent></DialogPortal></Dialog>;}").unwrap();
        let content = &output
            .elements
            .iter()
            .find(|element| element.path == "0/1/0")
            .unwrap()
            .attributes;
        assert!(!content.contains_key("hidden"));
        assert!(!content.contains_key("aria-modal"));
        assert!(!content.contains_key("forceMount"));
        assert_eq!(
            content.get("data-state").map(String::as_str),
            Some("closed")
        );
        assert!(
            output
                .elements
                .iter()
                .any(|element| element.path == "0/1/0/0" && element.preserve_dirty_value)
        );
        let hidden = markup("type Props={};export function Panel(props:Props){return <Dialog open={false}><DialogTrigger>open</DialogTrigger><DialogContent forceMount hidden aria-label=\"inspector\"/></Dialog>;}").unwrap();
        assert!(
            hidden
                .elements
                .iter()
                .find(|element| element.path == "0/1")
                .unwrap()
                .attributes
                .contains_key("hidden")
        );
    }

    #[test]
    fn invalid_mount_targets_and_types_are_not_generic_attributes() {
        for source in [
            "type Props={};export function Panel(props:Props){return <Button forceMount/>;}",
            "type Props={};export function Panel(props:Props){return <Dialog><DialogTrigger>open</DialogTrigger><DialogContent forceMount=\"true\" aria-label=\"inspector\"/></Dialog>;}",
        ] {
            assert!(markup(source).err().unwrap().contains("forceMount"));
        }
    }

    #[test]
    fn nested_nonmodal_does_not_remove_outer_modal_background_boundary() {
        let output = markup("type Props={};export function Panel(props:Props){return <div><Button>background</Button><Dialog open={true}><DialogTrigger>outer</DialogTrigger><DialogContent aria-label=\"outer\"><Dialog open={true} modal={false}><DialogTrigger>inner</DialogTrigger><DialogContent aria-label=\"inner\"/></Dialog></DialogContent></Dialog></div>;}").unwrap();
        let background = &output
            .elements
            .iter()
            .find(|element| element.path == "0/0")
            .unwrap()
            .attributes;
        assert!(background.contains_key("inert"));
        assert_eq!(
            background.get("aria-hidden").map(String::as_str),
            Some("true")
        );
        let inner = &output
            .elements
            .iter()
            .find(|element| element.path == "0/1/1/0/1")
            .unwrap()
            .attributes;
        assert!(!inner.contains_key("inert"));
        assert!(!inner.contains_key("aria-modal"));
    }
}
