use meari::{
    application::{Language, assets::VerifiedAssets, source_modules::ModuleProgram},
    domain::{
        Value,
        assets::{AssetDescriptor, AssetManifest},
        build::{BuildPath, ContentHash, InputRole},
        component_state::StateValue,
    },
};
use meari_native::{
    application::Renderer,
    domain::{Viewport, widgets::Action},
    infrastructure::document::InteractiveDocument,
};
use std::collections::BTreeMap;

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
fn viewport() -> Viewport {
    Viewport {
        width: 160,
        height: 120,
        dark: false,
    }
}

#[test]
fn svg_backgrounds_repeat_position_space_round_and_clip_as_vectors() {
    let cases = [
        (
            "background-repeat:repeat;background-size:10px 10px",
            (65, 45),
            (75, 45),
        ),
        (
            "background-repeat:repeat-x;background-size:10px 10px",
            (65, 15),
            (15, 35),
        ),
        (
            "background-repeat:repeat-y;background-size:10px 10px",
            (15, 45),
            (45, 15),
        ),
        (
            "background-repeat:no-repeat;background-position:right bottom;background-size:10px 10px",
            (65, 45),
            (15, 15),
        ),
        (
            "background-repeat:no-repeat;background-position:center;background-size:10px 10px",
            (36, 26),
            (15, 15),
        ),
        (
            "background-repeat:space;background-size:16px 16px",
            (33, 12),
            (27, 12),
        ),
        (
            "background-repeat:round;background-size:17px 17px",
            (65, 45),
            (75, 45),
        ),
        (
            "background-repeat:round;background-size:200px 200px;background-position:center",
            (65, 45),
            (75, 45),
        ),
        (
            "background-repeat:repeat;background-size:10px 10px;padding:4px;border:4px solid black;background-clip:content-box",
            (20, 20),
            (15, 20),
        ),
    ];
    for (styles, painted, clear) in cases {
        let (assets, image, _) = assets();
        let component = meari::parse(
            "type Props={};export function Pattern(props:Props){return <main id=\"pattern\"/>;}",
            Language::Tsx,
        )
        .unwrap();
        let css = format!(
            "body{{margin:0}}main{{position:absolute;left:10px;top:10px;width:60px;height:40px;background-image:url('{image}');{styles}}}"
        );
        let mut document =
            InteractiveDocument::new_with_assets(component, &css, viewport(), assets).unwrap();
        let pixels = document.renderer.rasterize(&viewport()).unwrap();
        let red = |(x, y): (usize, usize)| {
            let at = (y * 160 + x) * 4;
            pixels[at] > 180 && pixels[at + 1] < 90 && pixels[at + 2] < 30 && pixels[at + 3] > 180
        };
        assert!(red(painted), "missing tile at {painted:?}: {styles}");
        assert!(
            !red(clear),
            "paint escaped layer/gap at {clear:?}: {styles}"
        );
        let node = document
            .renderer
            .document
            .get_element_by_id("pattern")
            .unwrap();
        let element = document
            .renderer
            .document
            .get_node(node)
            .unwrap()
            .element_data()
            .unwrap();
        assert!(matches!(
            &element.background_images[0].as_ref().unwrap().image,
            blitz_dom::node::ImageData::Svg(_)
        ));
    }
}

#[test]
fn excessive_svg_tiling_fails_instead_of_publishing_an_incomplete_frame() {
    let (assets, image, _) = assets();
    let component = meari::parse(
        "type Props={};export function Pattern(props:Props){return <main/>;}",
        Language::Tsx,
    )
    .unwrap();
    let css = format!(
        "main{{width:60px;height:40px;background-image:url('{image}');background-size:0.01px 0.01px}}"
    );
    let mut document =
        InteractiveDocument::new_with_assets(component, &css, viewport(), assets).unwrap();
    assert!(
        document
            .renderer
            .rasterize(&viewport())
            .unwrap_err()
            .contains("vector tile budget")
    );
    assert!(document.renderer.last_paint.is_none());
}
fn assets() -> (VerifiedAssets, String, String) {
    let image=b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\"><rect width=\"24\" height=\"24\" fill=\"#d12a00\"/></svg>";
    let mut files = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    let mut urls = Vec::new();
    for (name, bytes, role, media) in [
        (
            "image.svg",
            image.as_slice(),
            InputRole::Icon,
            "image/svg+xml",
        ),
        (
            "font.otf",
            blitz_dom::BULLET_FONT,
            InputRole::Font,
            "font/otf",
        ),
    ] {
        let hash = ContentHash::of(bytes);
        let descriptor = AssetDescriptor {
            role,
            content: hash.clone(),
            length: bytes.len() as u64,
            media_type: media.into(),
            license: None,
        };
        urls.push(format!("/{}", descriptor.output_path().unwrap().as_str()));
        files.insert(path(name), descriptor);
        blobs.insert(hash, bytes.to_vec());
    }
    (
        VerifiedAssets::new(
            AssetManifest {
                schema_version: 1,
                files,
            },
            blobs,
        )
        .unwrap(),
        urls.remove(0),
        urls.remove(0),
    )
}
fn css(font: &str) -> String {
    format!(
        "@font-face{{font-family:MeariBullet;src:url('{font}') format('opentype')}} html,body{{margin:0;background:white;font-family:MeariBullet}} img{{width:40px;height:40px;display:block}} button{{height:24px;display:block}}"
    )
}

#[test]
fn native_declared_svg_and_font_are_delivered_and_image_pixels_are_painted() {
    let (assets, image, font) = assets();
    let source = format!(
        "type Props={{}};export function Demo(props:Props){{return <main><img id=\"logo\" alt=\"Declared SVG\" src=\"{image}\"/><p>•</p><img id=\"decorative\" alt=\"\" src=\"{image}\"/></main>;}}"
    );
    let component = meari::parse(&source, Language::Tsx).unwrap();
    assert!(InteractiveDocument::new(component.clone(), &css(&font), viewport()).is_err());
    let mut document =
        InteractiveDocument::new_with_assets(component, &css(&font), viewport(), assets).unwrap();
    let pixels = document.renderer.rasterize(&viewport()).unwrap();
    assert_eq!(pixels.len(), 160 * 120 * 4);
    let offset = (10 * 160 + 10) * 4;
    assert!(
        pixels[offset] > 180 && pixels[offset + 1] < 90 && pixels[offset + 2] < 30,
        "SVG pixel {:?}",
        &pixels[offset..offset + 4]
    );
    let report = document.renderer.asset_report().unwrap().unwrap();
    assert_eq!(report.rejected, 0);
    assert!(report.delivered.contains_key(image.trim_start_matches('/')));
    assert!(report.delivered.contains_key(font.trim_start_matches('/')));
    assert!(!document.renderer.document.has_pending_critical_resources());
    let tree = meari_native::infrastructure::accessibility::tree(&document);
    assert!(
        tree.nodes
            .iter()
            .any(|(_, node)| node.role() == accesskit::Role::Image
                && node.label() == Some("Declared SVG"))
    );
    let decorative = document
        .renderer
        .document
        .get_element_by_id("decorative")
        .unwrap()
        .as_u64();
    assert!(
        tree.nodes
            .iter()
            .find(|(id, _)| id.0 == decorative)
            .unwrap()
            .1
            .is_hidden()
    );
}

#[test]
fn module_events_and_props_retain_assets_while_missing_updates_do_not_commit() {
    let (assets, image, font) = assets();
    let sources=BTreeMap::from([
        (path("view.tsx"),"import {Counter} from './counter';type Props={caption:string;logo:string};export function View(props:Props){return <main><Counter caption={props.caption}/><img id=\"logo\" src={props.logo}/></main>; }".into()),
        (path("counter.tsx"),"import {useState} from 'react';type Props={caption:string};export function Counter(props:Props){const [count,setCount]=useState(0);return <button onClick={()=>setCount(previous=>previous+1)}>{props.caption}{count}</button>; }".into()),
    ]);
    let graph = meari::parse_module_graph(&path("view.tsx"), &sources).unwrap();
    let props = |caption: &str, logo: &str| {
        BTreeMap::from([
            ("caption".into(), Value::String(caption.into())),
            ("logo".into(), Value::String(logo.into())),
        ])
    };
    let program =
        ModuleProgram::mount(graph, props("Before", &image), "asset-module".into(), 1).unwrap();
    let mut document = InteractiveDocument::from_module_program_with_assets(
        program,
        &css(&font),
        viewport(),
        assets,
    )
    .unwrap();
    let image_node = document
        .renderer
        .document
        .get_element_by_id("logo")
        .unwrap();
    let before_report = document.renderer.asset_report().unwrap().unwrap();
    document
        .dispatch(Action::Activate { path: "0/0".into() })
        .unwrap();
    document
        .update_module_props(props("After", &image))
        .unwrap();
    assert_eq!(
        document.module_projection().unwrap().instances["root@0/0"].values["count"],
        StateValue::Number(serde_json::Number::from_f64(1.0).unwrap())
    );
    let before = document.module_projection().unwrap().instances.clone();
    let before_component = document.module_projection().unwrap().component.clone();
    let before_labels = meari_native::infrastructure::accessibility::tree(&document)
        .nodes
        .iter()
        .map(|(id, node)| (id.0, node.label().map(str::to_owned)))
        .collect::<BTreeMap<_, _>>();
    assert!(
        document
            .update_module_props(props("Uncommitted", "/assets/missing.svg"))
            .is_err()
    );
    assert_eq!(document.module_projection().unwrap().instances, before);
    assert_eq!(
        document.module_projection().unwrap().component,
        before_component
    );
    assert!(
        document
            .replace_stylesheet("img{background-image:url('/assets/missing.svg')}")
            .is_err()
    );
    document.renderer.rasterize(&viewport()).unwrap();
    assert_eq!(
        document.renderer.document.get_element_by_id("logo"),
        Some(image_node)
    );
    let after_report = document.renderer.asset_report().unwrap().unwrap();
    assert_eq!(before_report.document, after_report.document);
    assert_eq!(after_report.rejected, 0);
    let after_labels = meari_native::infrastructure::accessibility::tree(&document)
        .nodes
        .iter()
        .map(|(id, node)| (id.0, node.label().map(str::to_owned)))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(before_labels, after_labels);
}
