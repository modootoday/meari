//! `<progress>` follows Chrome: a 10em×1em inline box drawn as the native rounded blue bar
//! when unstyled, and as the green-on-gray fallback once author borders or backgrounds apply.
use meari_native::{
    application::Renderer, domain::Viewport, infrastructure::engine::NativeRenderer,
};

const VIEWPORT: Viewport = Viewport {
    width: 240,
    height: 60,
    dark: false,
};

fn render(reset: &str) -> (NativeRenderer, Vec<u8>) {
    let html = format!(
        "<html><head><style>body{{margin:0;padding:16px;font-size:16px}}{reset}</style></head>\
         <body><progress id=p value=3 max=4></progress></body></html>"
    );
    let mut engine = NativeRenderer::from_html(&html);
    engine.layout(&VIEWPORT, 0.0).unwrap();
    let pixels = engine.rasterize(&VIEWPORT).unwrap();
    (engine, pixels)
}

fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    let index = (y * VIEWPORT.width as usize + x) * 4;
    bytes[index..index + 4].try_into().unwrap()
}

#[test]
fn unstyled_progress_uses_the_native_blue_bar() {
    let (engine, pixels) = render("");
    let p = engine.bounds("p").unwrap();
    assert_eq!((p.width, p.height), (160.0, 8.0), "{p:?}");
    let mid = (p.y + 4.0) as usize;
    assert_eq!(pixel(&pixels, 16 + 60, mid), [0, 117, 255, 255]);
    assert_eq!(pixel(&pixels, 16 + 140, mid), [239, 239, 239, 255]);
}

#[test]
fn author_border_switches_to_the_fallback_look_with_the_value_fraction() {
    let (engine, pixels) = render("*{border:0 solid}");
    let p = engine.bounds("p").unwrap();
    assert_eq!((p.width, p.height), (160.0, 16.0), "{p:?}");
    let mid = (p.y + 8.0) as usize;
    let fill = pixel(&pixels, 16 + 110, mid);
    let track = pixel(&pixels, 16 + 130, mid);
    assert!(
        fill[1] > fill[0] && fill[1] > fill[2],
        "green fill {fill:?}"
    );
    assert!(
        track[0] == track[1] && track[1] == track[2],
        "gray track {track:?}"
    );
}
