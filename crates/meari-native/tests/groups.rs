use meari::{application::Language, parse};
use meari_native::{
    domain::{Viewport, widgets::Action},
    infrastructure::{accessibility, document::InteractiveDocument},
};

fn document(body: &str) -> InteractiveDocument {
    let source = parse(
        &format!("type Props={{}};export function Demo(props:Props){{return {body};}}"),
        Language::Tsx,
    )
    .unwrap();
    InteractiveDocument::new(
        source,
        "",
        Viewport {
            width: 600,
            height: 400,
            dark: false,
        },
    )
    .unwrap()
}

#[test]
fn button_group_has_a_name_and_real_button_actions_without_toolbar_key_semantics() {
    let mut doc = document(
        "<ButtonGroup aria-label=\"Actions\"><Button id=\"save\">Save</Button><Button id=\"blocked\" disabled>Blocked</Button></ButtonGroup>",
    );
    assert_eq!(doc.elements()[0].attributes["role"], "group");
    let tree = accessibility::tree(&doc);
    let group = doc
        .renderer
        .document
        .get_element_by_id(&doc.elements()[0].id)
        .unwrap();
    let group = &tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == group.as_u64())
        .unwrap()
        .1;
    assert_eq!(group.role(), accesskit::Role::Group);
    assert_eq!(group.label(), Some("Actions"));
    let target = doc.renderer.document.get_element_by_id("save").unwrap();
    let node = &tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == target.as_u64())
        .unwrap()
        .1;
    assert_eq!(node.label(), Some("Save"));
    assert!(node.supports_action(accesskit::Action::Click));
    let path = doc
        .elements()
        .iter()
        .find(|e| e.id == "save")
        .unwrap()
        .path
        .clone();
    assert!(
        doc.dispatch(Action::Activate { path: path.clone() })
            .unwrap()
    );
    assert_eq!(doc.activation_requests.last(), Some(&path));
    let blocked = doc
        .elements()
        .iter()
        .find(|e| e.id == "blocked")
        .unwrap()
        .path
        .clone();
    assert!(!doc.dispatch(Action::Activate { path: blocked }).unwrap());
}

#[test]
fn field_labels_editable_values_and_errors_reach_the_accessibility_tree() {
    let mut doc = document(
        "<FieldSet><FieldLegend>Account</FieldLegend><Field><FieldLabel htmlFor=\"email\">Email</FieldLabel><Input id=\"email\" aria-describedby=\"help error\" aria-invalid=\"true\"/><FieldDescription id=\"help\">Required</FieldDescription><FieldError id=\"error\">Invalid email</FieldError></Field></FieldSet>",
    );
    let tree = accessibility::tree(&doc);
    let input = doc.renderer.document.get_element_by_id("email").unwrap();
    let node = &tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == input.as_u64())
        .unwrap()
        .1;
    assert_eq!(node.label(), Some("Email"));
    assert_eq!(node.invalid(), Some(accesskit::Invalid::True));
    assert_eq!(node.described_by().len(), 2);
    assert!(node.supports_action(accesskit::Action::SetValue));
    accessibility::act(
        &mut doc,
        &accesskit::ActionRequest {
            action: accesskit::Action::SetValue,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(input.as_u64()),
            data: Some(accesskit::ActionData::Value("test@example.com".into())),
        },
    )
    .unwrap();
    assert_eq!(
        doc.input_values.values().next().unwrap(),
        "test@example.com"
    );
    let error = doc.elements().iter().find(|e| e.id == "error").unwrap();
    assert_eq!(error.attributes["role"], "alert");
    let id = doc.renderer.document.get_element_by_id("error").unwrap();
    let tree = accessibility::tree(&doc);
    let node = &tree
        .nodes
        .iter()
        .find(|(key, _)| key.0 == id.as_u64())
        .unwrap()
        .1;
    assert_eq!(node.role(), accesskit::Role::Alert);
    assert_eq!(node.live(), Some(accesskit::Live::Assertive));
}

#[test]
fn explicit_labelled_by_takes_precedence_over_aria_label_and_field_label() {
    let doc = document(
        "<Field><FieldTitle id=\"preferred\">Preferred</FieldTitle><FieldLabel htmlFor=\"input\">Fallback</FieldLabel><Input id=\"input\" aria-labelledby=\"preferred\" aria-label=\"Ignored\"/></Field>",
    );
    let tree = accessibility::tree(&doc);
    let id = doc.renderer.document.get_element_by_id("input").unwrap();
    let label = doc
        .renderer
        .document
        .get_element_by_id("preferred")
        .unwrap();
    let node = &tree
        .nodes
        .iter()
        .find(|(key, _)| key.0 == id.as_u64())
        .unwrap()
        .1;
    assert_eq!(node.labelled_by(), &[accesskit::NodeId(label.as_u64())]);
    assert_eq!(node.label(), None);
}

#[test]
fn invalid_group_options_and_unsupported_slot_and_fieldset_behavior_are_diagnosed() {
    for body in [
        "<Input type=\"file\" value=\"forbidden\"/>",
        "<Field size=\"sm\"/>",
        "<ButtonGroup orientation=\"diagonal\"/>",
        "<ButtonGroup orientation={true}/>",
        "<ButtonGroupText asChild><button>Child</button></ButtonGroupText>",
        "<FieldSet disabled><Input/></FieldSet>",
        "<FieldLegend variant=\"unknown\"/>",
    ] {
        let source = parse(
            &format!("type Props={{}};export function Demo(props:Props){{return {body};}}"),
            Language::Tsx,
        )
        .unwrap();
        assert!(
            InteractiveDocument::new(
                source,
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

#[test]
#[ignore = "requires pinned BRIDGE_TAILWIND executable"]
fn official_tailwind_renders_button_group_horizontal_and_vertical_geometry() {
    use meari_native::{
        application::{self, Renderer},
        infrastructure::tailwind::TailwindCompiler,
    };
    let compiler = TailwindCompiler::windows_x64(std::path::Path::new(
        &std::env::var_os("BRIDGE_TAILWIND").unwrap(),
    ))
    .unwrap();
    let mut theme = meari::domain::design::theme::Theme::default();
    for name in meari::domain::design::utilities::COLORS {
        theme.light.insert((*name).into(), [244, 244, 245, 255]);
        theme.dark.insert((*name).into(), [24, 24, 27, 255]);
    }
    for orientation in ["horizontal", "vertical"] {
        let source=parse(&format!("type Props={{}};export function Demo(props:Props){{return <ButtonGroup orientation=\"{orientation}\" aria-label=\"Actions\"><Button id=\"a\">First</Button><Button id=\"b\">Second</Button></ButtonGroup>;}}"),Language::Tsx).unwrap();
        let css = application::stylesheet(&source, &theme, &compiler).unwrap();
        let doc = InteractiveDocument::new(
            source,
            &css,
            Viewport {
                width: 600,
                height: 400,
                dark: false,
            },
        )
        .unwrap();
        let a = doc.renderer.bounds("a").unwrap();
        let b = doc.renderer.bounds("b").unwrap();
        assert!(a.width > 20.0 && a.height > 20.0);
        if orientation == "horizontal" {
            assert!(b.x >= a.x + a.width - 0.1);
            assert!((b.y - a.y).abs() < 0.1);
        } else {
            assert!(b.y >= a.y + a.height - 0.1);
            assert!((b.x - a.x).abs() < 0.1);
        }
    }
}
