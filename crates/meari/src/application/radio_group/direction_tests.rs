use super::*;
use crate::application::{Language, dom};

fn frame(body: &str) -> Result<dom::DomNode, String> {
    let source = format!(
        "import {{RadioGroup,RadioGroupItem}} from '@/components/ui/radio-group';type Props={{}};export function Directions(props:Props){{return {body};}}"
    );
    dom::prepare(&crate::parse(&source, Language::Tsx).map_err(|error| error.message)?)
}

const GROUP: &str = "<RadioGroup orientation=\"horizontal\"><RadioGroupItem value=\"a\"/><RadioGroupItem value=\"b\"/><RadioGroupItem value=\"c\"/></RadioGroup>";

#[test]
fn inherited_rtl_changes_portable_keyboard_requests_without_selecting() {
    let frame = frame(&format!(
        "<section dir=\"rtl\"><div>{GROUP}</div></section>"
    ))
    .unwrap();
    let groups = plan::groups(&frame).unwrap();
    let group = &groups["0/0/0"];
    assert_eq!(group.model.direction, Direction::RightToLeft);
    assert_eq!(
        group
            .model
            .key("0/0/0/0", "ArrowRight")
            .unwrap()
            .unwrap()
            .value,
        "c"
    );
    assert!(group.model.checked.is_empty());
}

#[test]
fn nearest_direction_overrides_do_not_leak_into_siblings() {
    let explicit = GROUP.replace("orientation=", "dir=\"ltr\" orientation=");
    let frame = frame(&format!(
        "<section dir=\"RTL\"><div dir=\"ltr\">{GROUP}</div>{GROUP}{explicit}</section>"
    ))
    .unwrap();
    let groups = plan::groups(&frame).unwrap();
    assert_eq!(groups["0/0/0"].model.direction, Direction::LeftToRight);
    assert_eq!(groups["0/1"].model.direction, Direction::RightToLeft);
    assert_eq!(groups["0/2"].model.direction, Direction::LeftToRight);
}

#[test]
fn unresolved_auto_is_not_silently_assigned_ltr_and_explicit_override_recovers() {
    assert!(frame(&format!("<section dir=\"auto\">{GROUP}</section>")).is_err());
    let explicit = GROUP.replace("orientation=", "dir=\"rtl\" orientation=");
    assert!(frame(&format!("<section dir=\"auto\">{explicit}</section>")).is_ok());
    assert!(frame(&GROUP.replace("orientation=", "dir=\"invalid\" orientation=")).is_err());
}
