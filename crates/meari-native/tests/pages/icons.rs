use meari::{
    application::Language,
    domain::design::icons::{IconSize, IconSpec, IconStroke, IconToken, IconTone},
};
use meari_native::{
    application::{icons, markup},
    domain::{Viewport, widgets::Session},
    infrastructure::{accessibility, document::InteractiveDocument},
};

fn component(body: &str) -> meari::domain::Component {
    meari::parse(
        &format!("type Props={{}};export function Demo(props:Props){{return {body};}}"),
        Language::Tsx,
    )
    .unwrap()
}

#[test]
fn every_registered_icon_size_tone_and_stroke_uses_one_asset_registry() {
    for token in IconToken::ALL {
        assert_eq!(IconToken::parse(token.name()), Some(token));
        for size in [IconSize::Xs, IconSize::Sm, IconSize::Md, IconSize::Lg] {
            for tone in [
                IconTone::Inherit,
                IconTone::Foreground,
                IconTone::Muted,
                IconTone::Primary,
                IconTone::Destructive,
            ] {
                for stroke in [IconStroke::Thin, IconStroke::Regular, IconStroke::Bold] {
                    let result = icons::render(IconSpec {
                        token,
                        size,
                        tone,
                        stroke,
                    });
                    if token == IconToken::ToggleThumb && stroke != IconStroke::Regular {
                        assert!(result.is_err());
                        continue;
                    }
                    let svg = result.unwrap();
                    assert!(svg.contains(&format!("width=\"{}\"", size.pixels())));
                    assert!(svg.contains(size.class()) && svg.contains(tone.class()));
                    assert!(
                        svg.contains("aria-hidden=\"true\"") && svg.contains("focusable=\"false\"")
                    );
                    if token == IconToken::SelectionCheck {
                        assert!(svg.contains(&format!("stroke-width=\"{}\"", stroke.width())));
                    }
                }
            }
        }
    }
    assert!(IconToken::parse("../../arbitrary.svg").is_none());
}

#[test]
fn icon_component_roundtrips_tokens_and_separates_semantic_and_decorative_accessibility() {
    let source = component(
        r#"<div><Icon id="decorative" name="selection-check" size="md" tone="primary" stroke="bold"/><Icon id="semantic" name="toggle-thumb" size="lg" tone="destructive" decorative={false} label="Selected"/></div>"#,
    );
    for language in [Language::Rust, Language::Tsx] {
        let out = meari::emit(&source, language).unwrap();
        assert!(source.semantic_eq(&meari::parse(&out.code, language).unwrap()));
    }
    let markup = markup::build(&source, &Session::from_tree(&source.root).unwrap()).unwrap();
    assert!(markup.classes.contains("size-5") && markup.classes.contains("text-destructive"));
    let doc = InteractiveDocument::new(
        source,
        "",
        Viewport {
            width: 400,
            height: 300,
            dark: false,
        },
    )
    .unwrap();
    let tree = accessibility::tree(&doc);
    let node = |name: &str| {
        let id = doc
            .renderer
            .document
            .get_element_by_id(name)
            .unwrap()
            .as_u64();
        &tree.nodes.iter().find(|(key, _)| key.0 == id).unwrap().1
    };
    assert!(node("decorative").is_hidden());
    assert_eq!(node("semantic").role(), accesskit::Role::Image);
    assert_eq!(node("semantic").label(), Some("Selected"));
    assert!(!node("semantic").supports_action(accesskit::Action::Click));
    for (id, node) in &tree.nodes {
        if let Some(dom) = doc
            .renderer
            .document
            .get_node(blitz_dom::NodeId::from_u64(id.0))
            && dom
                .element_data()
                .is_some_and(|e| e.name.local == blitz_dom::local_name!("svg"))
        {
            assert!(node.is_hidden());
        }
    }
}

#[test]
fn invalid_tokens_labels_and_unimplemented_icon_props_are_diagnosed() {
    for body in [
        r#"<Icon/>"#,
        r#"<Icon name="unknown"/>"#,
        r#"<Icon name="selection-check" size="huge"/>"#,
        r#"<Icon name="selection-check" decorative={false}/>"#,
        r#"<Icon name="selection-check" label="Conflicting"/>"#,
        r#"<Icon name="toggle-thumb" stroke="bold"/>"#,
        r#"<Icon name="selection-check" tone="pink"/>"#,
        r#"<Icon name="selection-check" hidden/>"#,
    ] {
        let source = component(body);
        assert!(
            markup::build(&source, &Session::from_tree(&source.root).unwrap()).is_err(),
            "{body}"
        );
    }
}

#[test]
#[ignore = "requires pinned BRIDGE_TAILWIND executable"]
fn icon_size_tokens_produce_native_geometry_with_the_official_compiler() {
    use meari::domain::design::{theme::Theme, utilities::COLORS};
    use meari_native::{
        application::{self, Renderer},
        infrastructure::tailwind::TailwindCompiler,
    };
    let compiler = TailwindCompiler::windows_x64(std::path::Path::new(
        &std::env::var_os("BRIDGE_TAILWIND").unwrap(),
    ))
    .unwrap();
    let mut theme = Theme::default();
    for name in COLORS {
        theme.light.insert((*name).into(), [24, 24, 27, 255]);
        theme.dark.insert((*name).into(), [250, 250, 250, 255]);
    }
    let source = component(
        r#"<div className="flex gap-4"><Icon id="xs" name="selection-check" size="xs" stroke="thin"/><Icon id="sm" name="selection-check" size="sm"/><Icon id="md" name="selection-check" size="md" stroke="bold"/><Icon id="lg" name="toggle-thumb" size="lg"/></div>"#,
    );
    let css = application::stylesheet(&source, &theme, &compiler).unwrap();
    for dark in [false, true] {
        let mut doc = InteractiveDocument::new(
            source.clone(),
            &css,
            Viewport {
                width: 400,
                height: 300,
                dark,
            },
        )
        .unwrap();
        for (id, size) in [
            ("xs", IconSize::Xs),
            ("sm", IconSize::Sm),
            ("md", IconSize::Md),
            ("lg", IconSize::Lg),
        ] {
            let bounds = doc.renderer.bounds(id).unwrap();
            assert!(
                (bounds.width - f32::from(size.pixels())).abs() < 0.1,
                "{id}: {bounds:?}"
            );
            assert!(
                (bounds.height - f32::from(size.pixels())).abs() < 0.1,
                "{id}: {bounds:?}"
            );
        }
        assert!(!doc.renderer.rasterize(&doc.viewport).unwrap().is_empty());
    }
}
