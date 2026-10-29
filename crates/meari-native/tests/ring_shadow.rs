//! A spread-only box-shadow (Tailwind `ring-*`) paints a crisp solid band in its exact color.
use meari_native::{
    application::Renderer, domain::Viewport, infrastructure::engine::NativeRenderer,
};

const VIEWPORT: Viewport = Viewport {
    width: 100,
    height: 60,
    dark: false,
};

#[test]
fn spread_only_shadow_is_a_solid_band_in_the_shadow_color() {
    let mut engine = NativeRenderer::from_html(
        "<html><head><style>body{margin:0}#r{margin:20px;width:40px;height:20px;\
         border-radius:4px;box-shadow:0 0 0 3px oklch(68.5% .169 237.323)}</style></head>\
         <body><div id=r></div></body></html>",
    );
    engine.layout(&VIEWPORT, 0.0).unwrap();
    let pixels = engine.rasterize(&VIEWPORT).unwrap();
    let at = |x: usize, y: usize| -> [u8; 4] {
        let i = (y * VIEWPORT.width as usize + x) * 4;
        pixels[i..i + 4].try_into().unwrap()
    };
    // Mid-band on the left and top edges is the exact shadow color; outside stays transparent.
    // oklch(68.5% .169 237.323) is Tailwind sky-500, sRGB ≈ rgb(0, 166, 244).
    for [r, g, b, a] in [at(18, 30), at(40, 18)] {
        assert!(
            r < 8 && (160..=172).contains(&g) && b > 236 && a == 255,
            "{r},{g},{b},{a}"
        );
    }
    assert_eq!(at(15, 30), [0, 0, 0, 0]);
    assert_eq!(at(40, 30), [0, 0, 0, 0], "inside is not shadowed");
}
