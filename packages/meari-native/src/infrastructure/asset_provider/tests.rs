use super::*;
use blitz_traits::net::{AbortController, Url};
use meari::domain::{
    assets::{AssetDescriptor, AssetManifest},
    build::{ContentHash, InputRole},
};
use std::sync::mpsc::{Sender, channel};

const SVG:&[u8]=b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\"><path d=\"M0 0L24 24\"/></svg>";
fn bundle(bytes: &[u8], media_type: &str) -> (VerifiedAssets, String) {
    let hash = ContentHash::of(bytes);
    let role = if media_type.starts_with("font/") {
        InputRole::Font
    } else if media_type == "image/svg+xml" {
        InputRole::Icon
    } else {
        InputRole::Image
    };
    let descriptor = AssetDescriptor {
        role,
        content: hash.clone(),
        length: bytes.len() as u64,
        media_type: media_type.into(),
        license: None,
    };
    let url = format!("{BASE_URL}{}", descriptor.output_path().unwrap().as_str());
    let manifest = AssetManifest {
        schema_version: 1,
        files: BTreeMap::from([(
            "source/asset.bin".to_owned().try_into().unwrap(),
            descriptor,
        )]),
    };
    (
        VerifiedAssets::new(manifest, BTreeMap::from([(hash, bytes.to_vec())])).unwrap(),
        url,
    )
}
struct Capture(Sender<(String, Vec<u8>)>);
impl NetHandler for Capture {
    fn bytes(self: Box<Self>, url: String, bytes: Bytes) {
        self.0.send((url, bytes.to_vec())).unwrap();
    }
}
fn fetch(provider: &AssetProvider, document: usize, request: Request) -> (String, Vec<u8>) {
    let (tx, rx) = channel();
    provider.fetch(document, request, Box::new(Capture(tx)));
    rx.recv().unwrap()
}

#[test]
fn declared_delivery_keeps_query_aliases_and_rejects_foreign_scope_or_fallback() {
    let (assets, url) = bundle(SVG, "image/svg+xml");
    let provider = AssetProvider::new(assets).unwrap();
    let query = format!("{url}?revision=2");
    assert_eq!(
        fetch(&provider, 17, Request::get(Url::parse(&query).unwrap())),
        (query, SVG.to_vec())
    );
    provider.ensure_success().unwrap();
    assert_eq!(
        provider.report().unwrap().delivered.values().sum::<u64>(),
        1
    );
    assert!(
        fetch(&provider, 18, Request::get(Url::parse(&url).unwrap()))
            .1
            .is_empty()
    );
    for invalid in [
        "https://example.test/assets/missing.svg",
        "file:///not-declared.svg",
        "https://meari.invalid/assets/missing.svg",
    ] {
        assert!(
            fetch(&provider, 17, Request::get(Url::parse(invalid).unwrap()))
                .1
                .is_empty()
        );
    }
    assert_eq!(provider.report().unwrap().rejected, 4);
    assert!(provider.ensure_success().is_err());
}

#[test]
fn abort_method_and_fragment_failures_complete_the_handler_without_success() {
    let (assets, url) = bundle(SVG, "image/svg+xml");
    let provider = AssetProvider::new(assets).unwrap();
    let abort = AbortController::default();
    let request = Request::get(Url::parse(&url).unwrap()).signal(abort.signal.clone());
    abort.abort();
    assert!(fetch(&provider, 17, request).1.is_empty());
    let mut post = Request::get(Url::parse(&url).unwrap());
    post.method = Method::POST;
    assert!(fetch(&provider, 17, post).1.is_empty());
    assert!(
        fetch(
            &provider,
            17,
            Request::get(Url::parse(&format!("{url}#part")).unwrap())
        )
        .1
        .is_empty()
    );
    assert!(provider.report().unwrap().delivered.is_empty());
    assert_eq!(provider.report().unwrap().rejected, 3);
}

#[test]
fn media_preflight_rejects_corrupt_fonts_and_external_svg_references() {
    let (assets, _) = bundle(b"not a font", "font/ttf");
    assert!(AssetProvider::new(assets).is_err());
    let (assets, _) = bundle(blitz_dom::BULLET_FONT, "font/otf");
    assert!(AssetProvider::new(assets).is_ok());
    for source in [
        "<svg><image href='not-declared.png'/></svg>",
        "<svg><image href='#local-file-name'/></svg>",
        "<svg><feImage href='data:image/png;base64,AA=='/></svg>",
        "<svg><path fill=\"url('file.svg#paint')\"/></svg>",
        "<svg><style>@import 'not-declared.css';</style></svg>",
        "<svg onload='unregistered()'/>",
        "<svg><foreignObject/></svg>",
        "<svg><script/></svg>",
        "<not-svg/>",
        "not XML",
    ] {
        let (assets, _) = bundle(source.as_bytes(), "image/svg+xml");
        assert!(AssetProvider::new(assets).is_err(), "{source}");
    }
}

#[test]
fn local_svg_paint_servers_decode_without_becoming_file_requests() {
    let svg=b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\"><defs><linearGradient id=\"paint\"><stop offset=\"0\" stop-color=\"red\"/></linearGradient></defs><rect width=\"24\" height=\"24\" fill=\"url(#paint)\"/></svg>";
    let (assets, url) = bundle(svg, "image/svg+xml");
    let provider = AssetProvider::new(assets).unwrap();
    assert_eq!(
        fetch(&provider, 1, Request::get(Url::parse(&url).unwrap())).1,
        svg
    );
    assert_eq!(provider.report().unwrap().requests, 1);
}
