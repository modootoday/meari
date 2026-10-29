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

fn list_document(update_add: &str, update_remove: &str) -> InteractiveDocument {
    let source = meari::parse(
        &format!(
            "import {{ useState }} from 'react';type Props={{}};\
             export function Demo(props:Props){{const [items,setItems]=useState([\"A\",\"B\"]);\
             return <div><button onClick={{() => {update_add}}}>add</button>\
             <button onClick={{() => {update_remove}}}>remove</button>\
             <ul id=\"list\">{{items.map(item => <li key={{item}}>{{item}}</li>)}}</ul></div>;}}"
        ),
        Language::Tsx,
    )
    .unwrap();
    InteractiveDocument::new(
        source,
        "",
        Viewport {
            width: 200,
            height: 200,
            dark: false,
        },
    )
    .unwrap()
}

fn list_text(document: &InteractiveDocument) -> String {
    let dom = &document.renderer.document;
    dom.get_node(dom.get_element_by_id("list").unwrap())
        .unwrap()
        .text_content()
}

#[test]
fn spread_append_and_filter_remove_update_the_live_list() {
    for (add, remove) in [
        (
            "setItems([...items, \"C\"])",
            "setItems(items.filter(item => item !== \"A\"))",
        ),
        (
            "setItems(xs => [...xs, \"C\"])",
            "setItems(xs => xs.filter(x => \"A\" !== x))",
        ),
    ] {
        let mut document = list_document(add, remove);
        let click = |document: &mut InteractiveDocument, path: &str| {
            assert!(
                document
                    .dispatch(Action::Activate { path: path.into() })
                    .unwrap()
            );
        };
        click(&mut document, "0/0");
        assert_eq!(list_text(&document), "ABC", "{add}");
        // A second append would duplicate the key "C"; keyed lists reject it, unchanged.
        assert!(
            document
                .dispatch(Action::Activate { path: "0/0".into() })
                .is_err()
        );
        assert_eq!(list_text(&document), "ABC");
        click(&mut document, "0/1");
        assert_eq!(list_text(&document), "BC", "{remove}");
    }
}

#[test]
fn list_updates_over_another_state_or_computed_items_are_rejected() {
    for bad in [
        "import { useState } from 'react';type Props={};export function D(props:Props){const [items,setItems]=useState([\"A\"]);const [b,setB]=useState([\"Z\"]);return <div><button onClick={() => setItems([...b, \"C\"])}>x</button>{items.map(item => <li key={item}>{item}</li>)}{b.map(item => <li key={item}>{item}</li>)}</div>;}",
        "import { useState } from 'react';type Props={};export function D(props:Props){const [items,setItems]=useState([\"A\"]);return <div><button onClick={() => setItems(items.filter(item => item.length > 1))}>x</button>{items.map(item => <li key={item}>{item}</li>)}</div>;}",
    ] {
        assert!(meari::parse(bad, Language::Tsx).is_err(), "{bad}");
    }
}
