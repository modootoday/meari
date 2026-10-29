use super::*;
use meari::{application::Language, domain::radio_group::Direction};

const GROUP: &str = "<RadioGroup orientation=\"horizontal\"><RadioGroupItem value=\"a\"/><RadioGroupItem value=\"b\"/><RadioGroupItem value=\"c\"/></RadioGroup>";

fn session(body: &str) -> Result<Session, String> {
    let source = format!(
        "import {{RadioGroup,RadioGroupItem}} from '@/components/ui/radio-group';type Props={{}};export function Directions(props:Props){{return {body};}}"
    );
    let component = meari::parse(&source, Language::Tsx).map_err(|error| error.message)?;
    Session::from_tree(&component.root)
}

#[test]
fn inherited_native_rtl_requests_match_portable_order_without_selecting() {
    let session = session(&format!(
        "<section dir=\"rtl\"><div>{GROUP}</div></section>"
    ))
    .unwrap();
    let widget = &session.widgets["0/0/0"];
    let model = widget.radio_model(false);
    assert_eq!(model.direction, Direction::RightToLeft);
    assert_eq!(
        model.key("0/0/0/0", "ArrowRight").unwrap().unwrap().value,
        "c"
    );
    assert!(widget.value.is_empty());
}

#[test]
fn native_nearest_override_and_sibling_direction_are_independent() {
    let explicit = GROUP.replace("orientation=", "dir=\"ltr\" orientation=");
    let session = session(&format!(
        "<section dir=\"RTL\"><div dir=\"ltr\">{GROUP}</div>{GROUP}{explicit}</section>"
    ))
    .unwrap();
    assert_eq!(
        session.widgets["0/0/0"].radio_direction,
        Direction::LeftToRight
    );
    assert_eq!(
        session.widgets["0/1"].radio_direction,
        Direction::RightToLeft
    );
    assert_eq!(
        session.widgets["0/2"].radio_direction,
        Direction::LeftToRight
    );
}

#[test]
fn native_auto_boundary_requires_resolution_and_explicit_group_recovers() {
    assert!(session(&format!("<section dir=\"auto\">{GROUP}</section>")).is_err());
    let explicit = GROUP.replace("orientation=", "dir=\"rtl\" orientation=");
    assert!(session(&format!("<section dir=\"auto\">{explicit}</section>")).is_ok());
    assert!(session(&GROUP.replace("orientation=", "dir=\"invalid\" orientation=")).is_err());
}
