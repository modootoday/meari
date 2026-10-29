//! A mounted native document shows the state committed by a mount-only effect.
use meari::application::Language;
use meari_native::{domain::Viewport, infrastructure::document::InteractiveDocument};

#[test]
fn native_document_renders_the_mount_effect_state() {
    let source = meari::parse(
        "import { useEffect, useState } from 'react';type Props={};\
         export function Demo(props:Props){const [n,setN]=useState(0);\
         useEffect(() => { setN(n => n + 41); setN(n => n + 1); }, []);\
         return <p id=\"out\">{n}</p>;}",
        Language::Tsx,
    )
    .unwrap();
    let document = InteractiveDocument::new(
        source,
        "",
        Viewport {
            width: 200,
            height: 100,
            dark: false,
        },
    )
    .unwrap();
    let dom = &document.renderer.document;
    let node = dom.get_node(dom.get_element_by_id("out").unwrap()).unwrap();
    assert_eq!(node.text_content(), "42");
}
