//! `.map` over state is admitted only for dense, non-empty string-array states.
use meari::application::Language;

fn source(initial: &str, set: &str) -> String {
    format!(
        "import {{ useState }} from 'react';type Props={{}};\
         export function Demo(props:Props){{const [items,setItems]=useState({initial});\
         return <ul><button onClick={{() => setItems({set})}}>x</button>\
         {{items.map(item => <li key={{item}}>{{item}}</li>)}}</ul>;}}"
    )
}

#[test]
fn string_list_state_is_accepted() {
    meari::parse(&source("[\"a\",\"b\"]", "[\"a\"]"), Language::Tsx).unwrap();
}

#[test]
fn unsupported_list_states_are_rejected() {
    for (initial, set) in [
        ("[]", "[\"a\"]"),
        ("[1,2]", "[1]"),
        ("[\"a\"]", "[...items, \"b\"]"),
    ] {
        assert!(
            meari::parse(&source(initial, set), Language::Tsx).is_err(),
            "accepted: {initial} / {set}"
        );
    }
}
