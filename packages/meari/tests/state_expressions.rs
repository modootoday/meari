use meari::{
    application::{Language, state_program::StateProgram},
    domain::{component_state::StateValue, state_program::StateEvent},
};

fn source() -> &'static str {
    r#"import {useState} from 'react';type Props={};export function Demo(props:Props){
    const [count,setCount]=useState(0);const [step,setStep]=useState(2);const [label,setLabel]=useState("A");const [flag,setFlag]=useState(false);const [previous,setPrevious]=useState(3);
    return <div>
    <button onClick={()=>setCount(count+1)}>Capture</button>
    <button onClick={()=>setCount(n=>n*2+step)}>Latest</button>
    <button onClick={()=>setStep(10)}>Step</button>
    <button onClick={()=>setLabel(label+":"+count)}>Label</button>
    <button onClick={()=>setFlag(!flag)}>Flag</button>
    <button onClick={()=>setLabel(flag ? "ok" : "bad"+(count/0))}>Conditional</button>
    <button onClick={()=>setCount(n=>n+previous)}>Shadow</button>
    <button onClick={()=>setCount(count/0)}>Fail</button>
    <button onClick={()=>setFlag(flag && count/0 > 0)}>Short circuit</button>
    <button onClick={()=>setFlag("🦀" < "\uE000")}>UTF16</button>
    </div>; }"#
}

fn component() -> meari::domain::Component {
    meari::parse(source(), Language::Tsx).unwrap()
}
fn event(program: &StateProgram, index: usize) -> StateEvent {
    program.project().unwrap().events[&format!("0/{index}")].clone()
}
fn count(program: &StateProgram) -> f64 {
    let StateValue::Number(value) = &program.snapshot().unwrap().values["count"] else {
        panic!()
    };
    value.as_f64().unwrap()
}

#[test]
fn held_captures_and_functional_updates_have_distinct_snapshot_semantics() {
    let mut program = StateProgram::mount(component(), "expressions".into(), 1).unwrap();
    let capture = event(&program, 0);
    let latest = event(&program, 1);
    program.dispatch(&capture).unwrap();
    program.dispatch(&capture).unwrap();
    assert_eq!(count(&program), 1.0);
    program.dispatch(&latest).unwrap();
    assert_eq!(count(&program), 4.0);
    program.dispatch(&event(&program, 2)).unwrap();
    program.dispatch(&latest).unwrap();
    assert_eq!(count(&program), 10.0); // held step was 2, latest previous was 4
    program.dispatch(&event(&program, 1)).unwrap();
    assert_eq!(count(&program), 30.0);
    let label = event(&program, 3);
    program.dispatch(&label).unwrap();
    program.dispatch(&label).unwrap();
    assert_eq!(
        program.snapshot().unwrap().values["label"],
        StateValue::String("A:30".into())
    );
    let flag = event(&program, 4);
    program.dispatch(&flag).unwrap();
    program.dispatch(&flag).unwrap();
    assert_eq!(
        program.snapshot().unwrap().values["flag"],
        StateValue::Bool(true)
    );
    program.dispatch(&event(&program, 5)).unwrap();
    assert_eq!(
        program.snapshot().unwrap().values["label"],
        StateValue::String("ok".into())
    );
}

#[test]
fn expressions_roundtrip_without_shadowing_free_state_and_fail_at_dispatch_only() {
    let component = component();
    for language in [Language::Rust, Language::Tsx] {
        let output = meari::emit(&component, language).unwrap();
        let restored = meari::parse(&output.code, language).unwrap();
        assert!(
            component.semantic_eq(&restored),
            "{language:?}: {}",
            output.code
        );
    }
    let mut program = StateProgram::mount(component, "expression-errors".into(), 1).unwrap();
    let snapshot = program.snapshot().unwrap();
    assert!(program.dispatch(&event(&program, 7)).is_err());
    assert_eq!(program.snapshot().unwrap(), snapshot);
    program.dispatch(&event(&program, 8)).unwrap(); // false && invalid RHS must not evaluate RHS
    assert_eq!(program.snapshot().unwrap(), snapshot);
    program.dispatch(&event(&program, 9)).unwrap();
    assert_eq!(
        program.snapshot().unwrap().values["flag"],
        StateValue::Bool(true)
    );
    program.dispatch(&event(&program, 6)).unwrap();
    assert_eq!(count(&program), 3.0);
}
