//! A keyed `.map` over a string-list state renders its items and re-expands after a setter.
use meari::application::Language;
use meari_native::{
    domain::{Viewport, widgets::Action},
    infrastructure::document::InteractiveDocument,
};

#[test]
fn state_list_renders_and_re_expands_after_set() {
    let source = meari::parse(
        "import { useState } from 'react';type Props={};\
         export function Demo(props:Props){const [items,setItems]=useState([\"A\",\"B\"]);\
         return <div><button onClick={() => setItems([\"A\",\"B\",\"C\"])}>add</button>\
         <ul id=\"list\">{items.map(item => <li key={item}>{item}</li>)}</ul></div>;}",
        Language::Tsx,
    )
    .unwrap();
    let mut document = InteractiveDocument::new(
        source,
        "",
        Viewport {
            width: 200,
            height: 200,
            dark: false,
        },
    )
    .unwrap();
    let text = |document: &InteractiveDocument| {
        let dom = &document.renderer.document;
        dom.get_node(dom.get_element_by_id("list").unwrap())
            .unwrap()
            .text_content()
    };
    assert_eq!(text(&document), "AB");
    assert!(
        document
            .dispatch(Action::Activate { path: "0/0".into() })
            .unwrap()
    );
    assert_eq!(text(&document), "ABC");
}
