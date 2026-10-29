use crate::{application::markup, domain::widgets::Session};
fn build(body: &str) -> Result<markup::Markup, String> {
    let component = meari::parse(&format!("type Props={{}};export function Panel(props:Props){{return <Dialog open={{false}}><DialogTrigger asChild>{body}</DialogTrigger><DialogContent aria-label=\"test\"/></Dialog>;}}"),meari::application::Language::Tsx).unwrap();
    let session = Session::from_tree(&component.root)?;
    markup::build(&component, &session)
}
#[test]
fn button_has_single_metadata_entry_and_preserves_child_descendant_paths() {
    let output = build("<Button id=\"owned-child\" className=\"child\" aria-label=\"open\">before<span>icon</span>after</Button>").unwrap();
    let button = output
        .elements
        .iter()
        .find(|element| element.path == "0/0")
        .unwrap();
    assert_eq!(button.aliases, ["0/0/0"]);
    assert_eq!(button.id, "owned-child");
    assert_eq!(button.dom_tag, "button");
    assert_eq!(output.html.matches("<button").count(), 1);
    assert_eq!(button.text_runs[0].before.as_deref(), Some("0/0/0/1"));
    assert!(
        output
            .elements
            .iter()
            .any(|element| element.path == "0/0/0/1")
    );
    assert_eq!(button.attributes["aria-controls"], "bridge-0-1");
    assert!(button.attributes["class"].contains("child"));
    assert!(!button.attributes.contains_key("asChild"));
}
#[test]
fn invalid_child_count_kind_and_nested_composition_are_explicit() {
    for body in [
        "",
        "text",
        "<Button/><Button/>",
        "<Input/>",
        "<Button asChild><Button/></Button>",
    ] {
        assert!(build(body).is_err(), "{body}");
    }
}
#[test]
fn child_disabled_is_an_admission_boundary_for_canonical_control() {
    let component = meari::parse("type Props={};export function Panel(props:Props){return <Dialog><DialogTrigger asChild><Button disabled>open</Button></DialogTrigger><DialogContent aria-label=\"test\"/></Dialog>;}",meari::application::Language::Tsx).unwrap();
    let session = Session::from_tree(&component.root).unwrap();
    assert!(session.disabled_paths.contains("0/0"));
    let output = markup::build(&component, &session).unwrap();
    assert!(
        output
            .elements
            .iter()
            .find(|element| element.path == "0/0")
            .unwrap()
            .attributes
            .contains_key("disabled")
    );
}
#[test]
fn source_child_handlers_are_rejected_before_projection_can_strip_them() {
    let component = meari::parse("type Props={click:()=>void};export function Panel(props:Props){return <Dialog><DialogTrigger asChild><Button onClick={props.click}>open</Button></DialogTrigger><DialogContent aria-label=\"test\"/></Dialog>;}",meari::application::Language::Tsx).unwrap();
    assert!(
        crate::application::state_program::validate_callbacks(&component, true)
            .unwrap_err()
            .contains("atomic composed event admission")
    );
}
