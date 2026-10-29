//! `{cond ? <A/> : <B/>}` shows exactly one branch and swaps on a state toggle.
use meari::application::Language;
use meari_native::{
    domain::{Viewport, widgets::Action},
    infrastructure::document::InteractiveDocument,
};

#[test]
fn ternary_swaps_branches_on_toggle() {
    let source = meari::parse(
        "import { useState } from 'react';type Props={};\
         export function Demo(props:Props){const [on,setOn]=useState(true);\
         return <div><button onClick={() => setOn(v => !v)}>t</button>\
         {on ? <p id=\"a\">On</p> : <p id=\"b\">Off</p>}</div>;}",
        Language::Tsx,
    )
    .unwrap();
    let mut document = InteractiveDocument::new(
        source,
        "",
        Viewport {
            width: 200,
            height: 100,
            dark: false,
        },
    )
    .unwrap();
    let hidden = |document: &InteractiveDocument, id: &str| {
        let dom = &document.renderer.document;
        dom.get_node(dom.get_element_by_id(id).unwrap())
            .unwrap()
            .attr(blitz_dom::local_name!("hidden"))
            .is_some()
    };
    assert!(!hidden(&document, "a") && hidden(&document, "b"));
    assert!(
        document
            .dispatch(Action::Activate { path: "0/0".into() })
            .unwrap()
    );
    assert!(hidden(&document, "a") && !hidden(&document, "b"));
}
