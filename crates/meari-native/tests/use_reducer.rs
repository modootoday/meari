//! `useReducer` with string and `{ type }` actions dispatches through the lowered reducer.
use meari::application::Language;
use meari_native::{
    domain::{Viewport, widgets::Action},
    infrastructure::document::InteractiveDocument,
};

fn run(source: &str, clicks: &[&str]) -> String {
    let source = meari::parse(source, Language::Tsx).unwrap();
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
    for path in clicks {
        assert!(
            document
                .dispatch(Action::Activate {
                    path: (*path).into()
                })
                .unwrap()
        );
    }
    let dom = &document.renderer.document;
    dom.get_node(dom.get_element_by_id("n").unwrap())
        .unwrap()
        .text_content()
}

#[test]
fn string_action_ternary_reducer_increments() {
    let source = r#"import { useReducer } from "react";type Props={};
function reducer(state: number, action: string) { return action === "inc" ? state + 1 : state; }
export function D(props:Props){const [n, dispatch] = useReducer(reducer, 0);
return <div><button onClick={() => dispatch("inc")}>+</button><p id="n">{n}</p></div>;}"#;
    assert_eq!(run(source, &[]), "0");
    assert_eq!(run(source, &["0/0", "0/0"]), "2");
}

#[test]
fn switch_on_action_type_with_fallthrough_and_default() {
    let source = r#"import { useReducer } from "react";type Props={};
function reducer(state, action) {
  switch (action.type) {
    case "incremented": { return state + 1; }
    case "decremented": return state - 1;
    case "reset":
    case "zero": return 0;
    default: throw Error("Unknown action: " + action.type);
  }
}
export function D(props:Props){const [n, dispatch] = useReducer(reducer, 5);
return <div><button onClick={() => dispatch({ type: "incremented" })}>+</button><button onClick={() => dispatch({ type: "decremented" })}>-</button><button onClick={() => dispatch({ type: "reset" })}>0</button><p id="n">{n}</p></div>;}"#;
    assert_eq!(run(source, &["0/0", "0/0", "0/1"]), "6");
    assert_eq!(run(source, &["0/0", "0/2"]), "0");
}

#[test]
fn if_chain_reducer_and_rejections() {
    let source = r#"import { useReducer } from "react";type Props={};
function reducer(state, action) { if (action === "on") return "yes"; if (action === "off") { return "no"; } return state; }
export function D(props:Props){const [n, dispatch] = useReducer(reducer, "no");
return <div><button onClick={() => dispatch("on")}>on</button><p id="n">{n}</p></div>;}"#;
    assert_eq!(run(source, &["0/0"]), "yes");
    for bad in [
        // payload actions read a free variable
        r#"import { useReducer } from "react";type Props={};function reducer(state, action){ return state + action.by; } export function D(props:Props){const [n, dispatch] = useReducer(reducer, 0); return <button onClick={() => dispatch({ type: "x" })}>{n}</button>;}"#,
        // an unknown action reaches the throw
        r#"import { useReducer } from "react";type Props={};function reducer(state, action){ switch(action){ case "a": return 1; default: throw Error("x"); } } export function D(props:Props){const [n, dispatch] = useReducer(reducer, 0); return <button onClick={() => dispatch("b")}>{n}</button>;}"#,
        // reducer function without useReducer import
        r#"type Props={};function reducer(state, action){ return state; } export function D(props:Props){ return <p>x</p>;}"#,
    ] {
        assert!(meari::parse(bad, Language::Tsx).is_err(), "{bad}");
    }
}
