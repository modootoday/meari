//! Mount-only `useEffect(() => { setX(..) }, [])` commits its setter updates before the
//! first rendered frame, as React's post-mount re-render does; anything else is rejected.
use meari::{application::Language, domain::component_state::StateValue};

fn source(effect: &str) -> String {
    format!(
        "import {{ useEffect, useState }} from 'react';type Props={{}};\
         export function Demo(props:Props){{const [n,setN]=useState(0);\
         const [on,setOn]=useState(false);{effect}return <p data-on={{on}}>{{n}}</p>;}}"
    )
}

#[test]
fn mount_effect_setters_fold_into_the_committed_state_in_order() {
    let component = meari::parse(
        &source("useEffect(() => { setN(1); setN(n => n + 2); setOn(v => !v); }, []);"),
        Language::Tsx,
    )
    .unwrap();
    let value = |name: &str| {
        component
            .states
            .iter()
            .find(|state| state.name == name)
            .unwrap()
            .initial
            .clone()
    };
    assert_eq!(
        value("n"),
        StateValue::Number(serde_json::Number::from_f64(3.0).unwrap())
    );
    assert_eq!(value("on"), StateValue::Bool(true));
}

#[test]
fn effects_that_could_observe_more_than_mount_are_rejected() {
    for effect in [
        "useEffect(() => { setN(1); }, [n]);",
        "useEffect(() => { setN(1); });",
        "useEffect(() => { setN('x'); }, []);",
        "useEffect(() => { console.log(n); }, []);",
        "useEffect(() => { setN(1); return () => setN(0); }, []);",
        "useEffect(async () => { setN(1); }, []);",
        "useEffect(() => setN(1), []);",
        "useEffect(() => {}, []);",
        "",
    ] {
        assert!(
            meari::parse(&source(effect), Language::Tsx).is_err(),
            "accepted: {effect:?}"
        );
    }
}
