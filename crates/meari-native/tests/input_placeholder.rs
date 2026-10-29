//! Native `::placeholder`: an empty input paints its `placeholder` attribute in the
//! projected `::placeholder` color, and a value hides it.
use meari::application::Language;
use meari_native::{
    application::Renderer, domain::Viewport, infrastructure::document::InteractiveDocument,
};

const VIEWPORT: Viewport = Viewport {
    width: 300,
    height: 60,
    dark: false,
};
const CSS: &str = "body{margin:0}input{position:absolute;left:0;top:0;width:280px;height:40px;\
border:0;font-size:20px;color:#000}input::placeholder{color:#ff0000}";

fn red_pixels(input: &str) -> usize {
    let source = meari::parse(
        &format!("type Props={{}};export function S(props:Props){{return <main>{input}</main>}}"),
        Language::Tsx,
    )
    .unwrap();
    let mut doc = InteractiveDocument::new(source, CSS, VIEWPORT).unwrap();
    let rgba = doc.renderer.rasterize(&VIEWPORT).unwrap();
    rgba.as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 200 && p[1] < 80 && p[2] < 80)
        .count()
}

#[test]
fn empty_input_paints_placeholder_color() {
    assert!(red_pixels("<input placeholder=\"hello\" />") > 20);
}

#[test]
fn value_or_missing_attribute_hides_placeholder() {
    assert_eq!(
        red_pixels("<input placeholder=\"hello\" defaultValue=\"x\" />"),
        0
    );
    assert_eq!(red_pixels("<input />"), 0);
}
