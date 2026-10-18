use meari::domain::design::{
    stylesheet::{StylesheetMode, StylesheetProject},
    theme::Theme,
};
use meari_native::{
    application::{self, Renderer, StylesheetProjectCompiler, StylesheetRenderRequest},
    infrastructure::{
        document::InteractiveDocument, stylesheet::DirectCssCompiler, tailwind::TailwindCompiler,
    },
};

const RAW: &str = include_str!("../fixtures/custom-css/raw.json");
const TAILWIND: &str = include_str!("../fixtures/custom-css/tailwind.json");

fn assert_render(
    request: &StylesheetRenderRequest,
    css: &str,
    id: &str,
    width: f32,
    height: f32,
    color: [u8; 4],
) {
    let mut document = InteractiveDocument::new(
        meari::parse(&request.source, request.language).unwrap(),
        css,
        request.viewport.clone(),
    )
    .unwrap();
    let rect = document.renderer.bounds(id).unwrap();
    assert!(
        (rect.width - width).abs() < 0.5 && (rect.height - height).abs() < 0.5,
        "{id}: {rect:?}"
    );
    let pixels = document.renderer.rasterize(&request.viewport).unwrap();
    let index = (((rect.y + height - 5.0) as usize) * request.viewport.width as usize
        + (rect.x + width - 5.0) as usize)
        * 4;
    assert_eq!(&pixels[index..index + 4], color, "{id}");
}

#[test]
fn custom_css_file_imports_variables_nesting_media_and_originals_survive() {
    let mut request: StylesheetRenderRequest = serde_json::from_str(RAW).unwrap();
    let (_, compiled) = application::prepare_stylesheet(&request, &DirectCssCompiler).unwrap();
    assert_eq!(compiled.original, request.stylesheet);
    assert_eq!(compiled.source_sha256.len(), 3);
    assert!(!compiled.native_full_acceptance);
    assert_render(
        &request,
        &compiled.css,
        "panel",
        240.0,
        64.0,
        [12, 34, 56, 255],
    );
    request.viewport.width = 400;
    assert_render(
        &request,
        &compiled.css,
        "panel",
        120.0,
        64.0,
        [12, 34, 56, 255],
    );
    let encoded = serde_json::to_string(&compiled.original).unwrap();
    assert_eq!(
        serde_json::from_str::<StylesheetProject>(&encoded).unwrap(),
        request.stylesheet
    );
}

#[test]
fn css_strings_comments_and_escaped_directives_have_correct_boundaries() {
    let safe = StylesheetProject::inline(
        StylesheetMode::Css,
        r#"/* @plugin "x"; */ .x::after{content:'@config "x"; </style><script>bad</script>'}"#,
    );
    assert!(
        DirectCssCompiler
            .compile_project(&safe, "", &Theme::default())
            .is_ok()
    );
    for source in [
        r#"@p\6c ugin "evil";"#,
        r#"@config "x";"#,
        r#"@import url("../outside.css");"#,
        r#"@import "https://example.com/style.css";"#,
        r#"@theme{--color-a:red}"#,
    ] {
        let project = StylesheetProject::inline(StylesheetMode::Css, source);
        assert!(
            DirectCssCompiler
                .compile_project(&project, "", &Theme::default())
                .is_err(),
            "{source}"
        );
    }
}

#[test]
fn missing_cyclic_conditional_dependencies_are_explicit_errors() {
    let mut project = StylesheetProject::inline(StylesheetMode::Css, "@import './other.css';");
    assert!(
        DirectCssCompiler
            .compile_project(&project, "", &Theme::default())
            .unwrap_err()
            .contains("missing")
    );
    project
        .files
        .insert("other.css".into(), "@import './styles.css';".into());
    assert!(
        DirectCssCompiler
            .compile_project(&project, "", &Theme::default())
            .unwrap_err()
            .contains("cycle")
    );
    project
        .files
        .insert("other.css".into(), ".x{width:20px}".into());
    project.files.insert(
        "styles.css".into(),
        "@import './other.css' layer(components);".into(),
    );
    assert!(
        DirectCssCompiler
            .compile_project(&project, "", &Theme::default())
            .unwrap_err()
            .contains("conditional")
    );
}

#[test]
fn live_css_replacement_preserves_dom_input_focus_and_replaces_old_sheet() {
    let component = meari::parse("type Props = {}; export function LiveCss(props: Props) { return <main><Input id=\"field\"/><div id=\"box\" className=\"box\"/></main>; }", meari::application::Language::Tsx).unwrap();
    let mut document = InteractiveDocument::new(
        component,
        ".box{width:100px;height:40px;border:4px solid red}",
        meari_native::domain::Viewport {
            width: 800,
            height: 300,
            dark: false,
        },
    )
    .unwrap();
    let field = document
        .renderer
        .document
        .get_element_by_id("field")
        .unwrap();
    document
        .set_input_value("0/0", "Retained custom input")
        .unwrap();
    document.focus_path("0/0").unwrap();
    for _ in 0..20 {
        document.replace_stylesheet(r#".box{width:200px;height:60px;background:rgb(12,34,56)}.box::after{content:"&amp; </style><div id='injected'>"}"#).unwrap();
        let rect = document.renderer.bounds("box").unwrap();
        assert!(
            (rect.width - 200.0).abs() < 0.5 && (rect.height - 60.0).abs() < 0.5,
            "{rect:?}"
        );
        assert_eq!(document.renderer.document.author_stylesheets().count(), 1);
        assert_eq!(
            document.renderer.document.get_element_by_id("field"),
            Some(field)
        );
        assert_eq!(
            document.renderer.document.get_focussed_node_id(),
            Some(field)
        );
        assert_eq!(document.input_values["0/0"], "Retained custom input");
        assert!(
            document
                .renderer
                .document
                .get_element_by_id("injected")
                .is_none()
        );
    }
}

#[test]
#[ignore = "requires pinned BRIDGE_TAILWIND executable"]
fn official_custom_theme_utilities_variants_apply_reference_and_imports_render() {
    let executable = std::env::var_os("BRIDGE_TAILWIND").expect("BRIDGE_TAILWIND");
    let compiler = TailwindCompiler::windows_x64(std::path::Path::new(&executable)).unwrap();
    let mut request: StylesheetRenderRequest = serde_json::from_str(TAILWIND).unwrap();
    let (_, compiled) = application::prepare_stylesheet(&request, &compiler).unwrap();
    assert_eq!(compiled.original, request.stylesheet);
    assert!(!compiled.css.contains("reference-must-not-emit"));
    assert!(compiled.css.contains("border-brand"));
    assert_render(
        &request,
        &compiled.css,
        "themed",
        120.0,
        48.0,
        [12, 34, 56, 255],
    );
    assert_render(
        &request,
        &compiled.css,
        "applied",
        80.0,
        40.0,
        [70, 80, 90, 255],
    );
    assert_render(
        &request,
        &compiled.css,
        "responsive",
        160.0,
        32.0,
        [12, 34, 56, 255],
    );
    request.viewport.width = 400;
    assert_render(
        &request,
        &compiled.css,
        "responsive",
        64.0,
        32.0,
        [12, 34, 56, 255],
    );
    request.stylesheet.bridge_theme = true;
    for name in meari::domain::design::utilities::COLORS {
        request
            .theme
            .light
            .insert((*name).into(), [240, 240, 240, 255]);
        request.theme.dark.insert((*name).into(), [24, 24, 24, 255]);
    }
    request.source="type Props = {}; export function Override(props: Props) { return <div id=\"override\" className=\"bg-primary w-20 h-10\"/>; }".into();
    request
        .stylesheet
        .files
        .get_mut("tokens.css")
        .unwrap()
        .push_str("@theme {--color-primary:rgb(100,110,120);--spacing:2px;}");
    let (_, override_css) = application::prepare_stylesheet(&request, &compiler).unwrap();
    assert_render(
        &request,
        &override_css.css,
        "override",
        40.0,
        20.0,
        [100, 110, 120, 255],
    );
    for bad in [
        "@source '../outside';",
        "@import 'tailwindcss' source('../outside');",
        "@plugin 'x';",
        "@import 'tailwindcss/../../outside.css';",
    ] {
        let project = StylesheetProject::inline(StylesheetMode::Tailwind, bad);
        assert!(
            compiler
                .compile_project(&project, "", &Theme::default())
                .is_err(),
            "{bad}"
        );
    }
}
