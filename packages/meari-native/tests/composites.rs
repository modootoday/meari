use meari::{application::Language, domain::Component};
use meari_native::{
    domain::{Viewport, widgets::Action},
    infrastructure::{accessibility, document::InteractiveDocument},
};

fn source(body: &str) -> Component {
    meari::parse(
        &format!("type Props = {{}};export function Demo(props:Props){{return {body};}}"),
        Language::Tsx,
    )
    .unwrap()
}
fn document(body: &str) -> InteractiveDocument {
    InteractiveDocument::new(
        source(body),
        "[hidden]{display:none!important}",
        Viewport {
            width: 600,
            height: 400,
            dark: false,
        },
    )
    .unwrap()
}

#[test]
fn accordion_toggle_relations_and_single_selection_are_native() {
    let mut doc = document(
        "<Accordion><AccordionItem value=\"one\"><AccordionTrigger id=\"trigger\">One</AccordionTrigger><AccordionContent id=\"panel\">Content</AccordionContent></AccordionItem></Accordion>",
    );
    let trigger = doc.elements().iter().find(|e| e.id == "trigger").unwrap();
    assert_eq!(trigger.attributes["aria-controls"], "panel");
    let path = trigger.path.clone();
    doc.dispatch(Action::Activate { path: path.clone() })
        .unwrap();
    assert_eq!(doc.session.widgets["0"].value, "one");
    assert_eq!(
        doc.elements()
            .iter()
            .find(|e| e.id == "trigger")
            .unwrap()
            .attributes["aria-expanded"],
        "true"
    );
    doc.dispatch(Action::Activate { path }).unwrap();
    assert_eq!(doc.session.widgets["0"].value, "");
}

#[test]
fn alert_dialog_requires_explicit_cancel_and_modal_tree_hides_background() {
    let mut doc = document(
        "<div><Input id=\"background\"/><AlertDialog><AlertDialogTrigger>Delete</AlertDialogTrigger><AlertDialogContent><AlertDialogTitle>Confirm deletion</AlertDialogTitle><AlertDialogCancel>Cancel</AlertDialogCancel></AlertDialogContent></AlertDialog></div>",
    );
    doc.dispatch(Action::Activate {
        path: "0/1/0".into(),
    })
    .unwrap();
    assert!(
        !doc.dispatch(Action::Key {
            path: "0/1/1/1".into(),
            key: "Escape".into(),
            shift: false
        })
        .unwrap()
    );
    assert!(
        !doc.dispatch(Action::DismissOutside { path: "0/0".into() })
            .unwrap()
    );
    let id = doc
        .renderer
        .document
        .get_element_by_id("background")
        .unwrap();
    let tree = accessibility::tree(&doc);
    let background = &tree
        .nodes
        .iter()
        .find(|(node, _)| node.0 == id.as_u64())
        .unwrap()
        .1;
    assert!(background.is_hidden());
    assert!(!background.supports_action(accesskit::Action::Focus));
    doc.dispatch(Action::Activate {
        path: "0/1/1/1".into(),
    })
    .unwrap();
    assert!(!doc.session.widgets["0/1"].open);
    assert_eq!(doc.session.focus.as_deref(), Some("0/1/0"));
}

#[test]
fn disabled_accordion_items_and_overlay_triggers_cannot_activate() {
    let mut doc = document(
        "<div><Accordion><AccordionItem value=\"one\" disabled><AccordionTrigger id=\"disabled\">One</AccordionTrigger><AccordionContent>Content</AccordionContent></AccordionItem><AccordionItem value=\"two\"><AccordionTrigger id=\"enabled\">Two</AccordionTrigger><AccordionContent>Second</AccordionContent></AccordionItem></Accordion><Dialog><DialogTrigger id=\"dialog-disabled\" disabled>Open</DialogTrigger><DialogContent><DialogTitle>Title</DialogTitle></DialogContent></Dialog></div>",
    );
    for id in ["disabled", "dialog-disabled"] {
        let element = doc.elements().iter().find(|e| e.id == id).unwrap().clone();
        assert!(element.attributes.contains_key("disabled"));
        let before = serde_json::to_value(&doc.session).unwrap();
        assert!(
            !doc.dispatch(Action::Activate {
                path: element.path.clone()
            })
            .unwrap()
        );
        assert!(
            !doc.dispatch(Action::Key {
                path: element.path,
                key: "Enter".into(),
                shift: false
            })
            .unwrap()
        );
        assert_eq!(before, serde_json::to_value(&doc.session).unwrap());
    }
    let enabled = doc
        .elements()
        .iter()
        .find(|e| e.id == "enabled")
        .unwrap()
        .path
        .clone();
    doc.dispatch(Action::Activate { path: enabled }).unwrap();
    assert_eq!(doc.session.widgets["0/0"].value, "two");
    let tree = accessibility::tree(&doc);
    let id = doc.renderer.document.get_element_by_id("disabled").unwrap();
    let node = tree
        .nodes
        .iter()
        .find(|(key, _)| key.0 == id.as_u64())
        .unwrap();
    assert!(node.1.is_disabled());
    assert_eq!(node.1.label(), Some("One"));
    assert!(!node.1.supports_action(accesskit::Action::Click));
}

#[test]
fn dropdown_roving_focus_and_popover_outside_dismiss_share_the_session() {
    let mut doc = document(
        "<div><DropdownMenu><DropdownMenuTrigger>Menu</DropdownMenuTrigger><DropdownMenuContent><DropdownMenuItem value=\"one\">One</DropdownMenuItem><DropdownMenuItem value=\"blocked\" disabled={true}>Blocked</DropdownMenuItem><DropdownMenuItem value=\"three\">Three</DropdownMenuItem></DropdownMenuContent></DropdownMenu><Popover><PopoverTrigger>Popup</PopoverTrigger><PopoverContent aria-label=\"Popup\">Body</PopoverContent></Popover></div>",
    );
    doc.dispatch(Action::Activate {
        path: "0/0/0".into(),
    })
    .unwrap();
    doc.dispatch(Action::Key {
        path: "0/0/1/0".into(),
        key: "ArrowDown".into(),
        shift: false,
    })
    .unwrap();
    assert_eq!(doc.session.focus.as_deref(), Some("0/0/1/2"));
    doc.dispatch(Action::Key {
        path: "0/0/1/2".into(),
        key: "Enter".into(),
        shift: false,
    })
    .unwrap();
    assert!(!doc.session.widgets["0/0"].open);
    assert_eq!(doc.session.widgets["0/0"].value, "three");
    doc.dispatch(Action::Activate {
        path: "0/1/0".into(),
    })
    .unwrap();
    doc.dispatch(Action::DismissOutside {
        path: "0/0/0".into(),
    })
    .unwrap();
    assert!(!doc.session.widgets["0/1"].open);
    assert_eq!(doc.session.focus.as_deref(), Some("0/1/0"));
}

#[test]
fn invalid_composition_and_unimplemented_widgets_do_not_render_as_empty_divs() {
    for body in [
        "<TabsTrigger value=\"one\">Orphan</TabsTrigger>",
        "<Accordion type=\"multiple\"/>",
        "<Dialog><DialogTrigger>Open</DialogTrigger><DialogContent>Unnamed</DialogContent></Dialog>",
        "<Tabs defaultValue={true}/>",
        "<Calendar/>",
        "<Dialog><TabsTrigger>Wrong root</TabsTrigger></Dialog>",
    ] {
        assert!(
            InteractiveDocument::new(
                source(body),
                "",
                Viewport {
                    width: 400,
                    height: 300,
                    dark: false
                }
            )
            .is_err(),
            "{body}"
        );
    }
}
