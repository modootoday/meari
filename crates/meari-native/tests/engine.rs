use meari::{
    application::Language,
    domain::design::{theme::Theme, utilities::COLORS},
};
use meari_native::{
    application::{Renderer, markup},
    domain::{
        Viewport,
        widgets::{Action, Session},
    },
    infrastructure::{
        document::InteractiveDocument, engine::NativeRenderer, tailwind::TailwindCompiler,
    },
};
use std::collections::BTreeMap;

fn source(body: &str) -> meari::domain::Component {
    meari::parse(
        &format!("type Props = {{}}; export function Example(props: Props) {{ return {body}; }}"),
        Language::Tsx,
    )
    .unwrap()
}
fn viewport() -> Viewport {
    Viewport {
        width: 800,
        height: 600,
        dark: false,
    }
}

#[test]
fn css_grid_calc_variables_selectors_and_raster_are_native() {
    let html = r#"<!doctype html><style>body{margin:0}.grid{display:grid;grid-template-columns:100px 1fr;gap:12px;width:400px;padding:16px;box-sizing:border-box;--paint:rgb(255,0,0)}.grid>[data-state=active]{background:var(--paint);height:40px}.grid>div:last-child{height:40px;width:calc(100% - 10px);background:rgb(0,0,255)}</style><div class="grid" id="grid"><div id="one" data-state="active"></div><div id="two"></div></div>"#;
    let mut engine = NativeRenderer::from_html(html);
    engine.layout(&viewport(), 0.0).unwrap();
    let first = engine.bounds("one").unwrap();
    let second = engine.bounds("two").unwrap();
    assert!(
        (first.x - 16.0).abs() < 0.5 && (first.width - 100.0).abs() < 0.5,
        "{first:?}"
    );
    assert!(
        (second.x - 128.0).abs() < 0.5 && (second.width - 246.0).abs() < 0.5,
        "{second:?}"
    );
    let pixels = engine.rasterize(&viewport()).unwrap();
    let at = |x: usize, y: usize| &pixels[(y * 800 + x) * 4..(y * 800 + x) * 4 + 4];
    assert_eq!(at(20, 20), [255, 0, 0, 255]);
    assert_eq!(at(140, 20), [0, 0, 255, 255]);
}

#[test]
fn tabs_native_dom_updates_selection_focus_and_accessibility() {
    let component = source(
        r#"<Tabs defaultValue="one"><TabsList><TabsTrigger value="one">One</TabsTrigger><TabsTrigger value="disabled" disabled={true}>Disabled</TabsTrigger><TabsTrigger value="three">Three</TabsTrigger></TabsList><TabsContent value="one">First</TabsContent><TabsContent value="three">Third</TabsContent></Tabs>"#,
    );
    let mut document =
        InteractiveDocument::new(component, "[hidden]{display:none!important}", viewport())
            .unwrap();
    let identity = document
        .renderer
        .document
        .get_element_by_id("bridge-0-0-2")
        .unwrap();
    assert!(
        document
            .dispatch(Action::Key {
                path: "0/0/0".into(),
                key: "ArrowRight".into(),
                shift: false
            })
            .unwrap()
    );
    assert_eq!(document.session.widgets["0"].value, "three");
    assert_eq!(document.session.focus.as_deref(), Some("0/0/2"));
    assert_eq!(
        document.renderer.document.get_element_by_id("bridge-0-0-2"),
        Some(identity)
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(identity)
    );
    let tree = document.renderer.document.build_accessibility_tree();
    let tab = tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == identity.as_u64())
        .unwrap();
    assert_eq!(format!("{:?}", tab.1.role()), "Tab");
    assert_eq!(tab.1.is_selected(), Some(true));
    assert!(
        !document
            .dispatch(Action::Activate {
                path: "0/0/1".into()
            })
            .unwrap()
    );
}

#[test]
fn select_and_modal_state_return_focus_and_trap_tab() {
    let component = source(
        r#"<div><Select defaultValue="one"><SelectTrigger><SelectValue/></SelectTrigger><SelectContent><SelectItem value="one">One</SelectItem><SelectItem value="two">Two</SelectItem></SelectContent></Select><Dialog><DialogTrigger>Open</DialogTrigger><DialogContent><DialogTitle>Settings</DialogTitle><Input id="edit" defaultValue="keep"/><DialogClose>Close</DialogClose></DialogContent></Dialog></div>"#,
    );
    let mut document =
        InteractiveDocument::new(component, "[hidden]{display:none!important}", viewport())
            .unwrap();
    let input = document
        .renderer
        .document
        .get_element_by_id("edit")
        .unwrap();
    document
        .dispatch(Action::Activate {
            path: "0/0/0".into(),
        })
        .unwrap();
    document
        .dispatch(Action::Key {
            path: "0/0/1/0".into(),
            key: "ArrowDown".into(),
            shift: false,
        })
        .unwrap();
    document
        .dispatch(Action::Key {
            path: "0/0/1/1".into(),
            key: "Enter".into(),
            shift: false,
        })
        .unwrap();
    assert_eq!(document.session.widgets["0/0"].value, "two");
    assert!(!document.session.widgets["0/0"].open);
    assert_eq!(document.session.focus.as_deref(), Some("0/0/0"));
    document
        .dispatch(Action::Activate {
            path: "0/1/0".into(),
        })
        .unwrap();
    assert!(document.session.widgets["0/1"].open);
    document
        .dispatch(Action::Key {
            path: "0/1/1/2".into(),
            key: "Tab".into(),
            shift: false,
        })
        .unwrap();
    assert_eq!(document.session.focus.as_deref(), Some("0/1/1/1"));
    document
        .dispatch(Action::Key {
            path: "0/1/1/1".into(),
            key: "Escape".into(),
            shift: false,
        })
        .unwrap();
    assert!(!document.session.widgets["0/1"].open);
    assert_eq!(document.session.focus.as_deref(), Some("0/1/0"));
    assert_eq!(
        document.renderer.document.get_element_by_id("edit"),
        Some(input)
    );
}

#[test]
fn registry_imports_roundtrip_without_inventing_native_capability() {
    let component = source(
        "<Dialog><DialogTrigger>Open</DialogTrigger><DialogContent><DialogTitle>Name</DialogTitle></DialogContent></Dialog>",
    );
    let output = meari::emit(&component, Language::Tsx).unwrap();
    assert!(output.code.contains("@/components/ui/dialog"));
    assert!(component.semantic_eq(&meari::parse(&output.code, Language::Tsx).unwrap()));
    assert!(!output.design_system.native_syntax_supported); // bounded typed-plan backend remains distinct
}

#[test]
#[ignore = "requires explicitly pinned official Tailwind standalone compiler"]
fn official_tailwind_output_runs_through_native_css_and_paint() {
    let path = std::env::var_os("BRIDGE_TAILWIND").expect("BRIDGE_TAILWIND required");
    let compiler = TailwindCompiler::windows_x64(std::path::Path::new(&path)).unwrap();
    let colors: BTreeMap<_, _> = COLORS
        .iter()
        .map(|n| ((*n).into(), [30, 40, 50, 255]))
        .collect();
    let theme = Theme {
        light: colors.clone(),
        dark: colors,
        ..Default::default()
    };
    let component = source(
        r#"<div id="grid" className="grid grid-cols-[100px_1fr] gap-3 w-[400px] p-4"><div id="one" className="h-10 bg-[#ff0000]"/><div id="two" className="h-10 bg-[#0000ff] md:w-[calc(100%-10px)]"/></div>"#,
    );
    let session = Session::from_tree(&component.root).unwrap();
    let markup = markup::build(&component, &session).unwrap();
    let css = compiler.compile(&markup.classes, &theme).unwrap();
    assert!(css.contains("grid-template-columns:100px 1fr"));
    let mut doc = InteractiveDocument::new(component, &css, viewport()).unwrap();
    let first = doc.renderer.bounds("one").unwrap();
    assert!((first.width - 100.0).abs() < 0.5, "{first:?}");
    let rgba = doc.renderer.rasterize(&viewport()).unwrap();
    assert_eq!(
        &rgba[(20 * 800 + 20) * 4..(20 * 800 + 20) * 4 + 4],
        &[255, 0, 0, 255]
    );
}

#[test]
#[ignore = "requires explicitly pinned official Tailwind standalone compiler"]
fn first_showcase_capture_contains_real_content_without_a_state_change() {
    let path = std::env::var_os("BRIDGE_TAILWIND").expect("BRIDGE_TAILWIND required");
    let compiler = TailwindCompiler::windows_x64(std::path::Path::new(&path)).unwrap();
    let mut colors: BTreeMap<String, [u8; 4]> = COLORS
        .iter()
        .map(|n| ((*n).into(), [255, 255, 255, 255]))
        .collect();
    colors.insert("foreground".into(), [24, 24, 27, 255]);
    let theme = Theme {
        light: colors.clone(),
        dark: colors,
        ..Default::default()
    };
    let component = meari::parse(include_str!("../fixtures/showcase.tsx"), Language::Tsx).unwrap();
    let css = meari_native::application::stylesheet(&component, &theme, &compiler).unwrap();
    let mut doc = InteractiveDocument::new(component, &css, viewport()).unwrap();
    let pixels = doc.renderer.rasterize(&viewport()).unwrap();
    let dark_text = pixels[..800 * 90 * 4]
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] < 100 && p[1] < 100 && p[2] < 100 && p[3] > 200)
        .count();
    assert!(
        dark_text > 100,
        "first capture is blank or missing title text: {dark_text}"
    );
}
