//! Ternary rendering accepts only two JSX element branches.
use meari::application::Language;

fn source(body: &str) -> String {
    format!(
        "import {{ useState }} from 'react';type Props={{}};\
         export function Demo(props:Props){{const [on,setOn]=useState(true);\
         return <div><button onClick={{() => setOn(v => !v)}}>t</button>{body}</div>;}}"
    )
}

#[test]
fn ternary_with_element_branches_is_accepted() {
    meari::parse(&source("{on ? <p>a</p> : (<p>b</p>)}"), Language::Tsx).unwrap();
}

#[test]
fn ternary_with_non_element_branches_is_rejected() {
    for body in [
        "{on ? 'a' : <p>b</p>}",
        "{on ? <p>a</p> : null}",
        "{on ? <p hidden>a</p> : <p>b</p>}",
    ] {
        assert!(
            meari::parse(&source(body), Language::Tsx).is_err(),
            "accepted: {body}"
        );
    }
}
