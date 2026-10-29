use super::{Action, Family, Session};

/// A requested value, never a committed widget state or a dispatch capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    pub root: String,
    pub target: String,
    pub open: bool,
}

impl Session {
    pub fn requested_open(&self, action: &Action) -> Option<OpenRequest> {
        let (target, key) = match action {
            Action::Activate { path } => (path, None),
            Action::Key { path, key, .. } => (path, Some(key.as_str())),
            Action::DismissOutside { path } => {
                let widget = self.top_overlay()?;
                if !widget.controlled_open || widget.family != Family::Dialog {
                    return None;
                }
                let inside = widget
                    .content
                    .iter()
                    .chain(widget.trigger.iter())
                    .any(|scope| path == scope || path.starts_with(&format!("{scope}/")));
                return (!inside).then(|| OpenRequest {
                    root: widget.root.clone(),
                    target: path.clone(),
                    open: false,
                });
            }
        };
        let widget = self.owner(target)?;
        if !widget.controlled_open
            || ![Family::Dialog, Family::AlertDialog].contains(&widget.family)
            || self.disabled_paths.contains(target)
        {
            return None;
        }
        let requested = if key == Some("Escape") && widget.open && widget.family == Family::Dialog {
            Some(false)
        } else if key.is_none() || matches!(key, Some("Enter" | " ")) {
            if widget.trigger.as_ref() == Some(target) {
                Some(!widget.open)
            } else if widget.open && widget.close_paths.contains(target) {
                Some(false)
            } else {
                None
            }
        } else {
            None
        };
        requested.map(|open| OpenRequest {
            root: widget.root.clone(),
            target: target.clone(),
            open,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::widgets::{FocusMove, Item, Navigation};
    struct NoNavigation;
    impl Navigation for NoNavigation {
        fn move_focus(
            &self,
            _items: &[Item],
            _active: Option<&str>,
            _movement: FocusMove,
        ) -> Option<String> {
            panic!("dialog actions must not use roving navigation");
        }
    }

    fn session(open: &str, modal: bool) -> Session {
        let source = format!(
            "type Props={{}};export function Panel(props:Props){{return <Dialog {open} modal={{{modal}}}><DialogTrigger>open</DialogTrigger><DialogContent aria-label=\"inspector\"><DialogClose>close</DialogClose></DialogContent></Dialog>;}}"
        );
        let component = meari::parse(&source, meari::application::Language::Tsx).unwrap();
        Session::from_tree(&component.root).unwrap()
    }

    #[test]
    fn controlled_false_wins_default_and_request_does_not_commit_or_focus() {
        let mut session = session("open={false} defaultOpen={true}", true);
        let action = Action::Activate { path: "0/0".into() };
        assert!(session.requested_open(&action).unwrap().open);
        assert!(!session.widgets["0"].open);
        assert!(session.focus.is_none());
        assert!(session.changes.is_empty());
        assert!(
            session
                .dispatch(action, &NoNavigation, &["0/1/0".into()])
                .is_err()
        );
        assert!(!session.widgets["0"].open);
        assert!(session.focus.is_none());
    }

    #[test]
    fn accepted_props_override_old_open_but_uncontrolled_migration_keeps_live_state() {
        let before = session("open={false}", true);
        let after = before
            .migrate_retained(session("open={true}", true))
            .unwrap();
        assert!(after.widgets["0"].open);
        assert!(
            before
                .migrate_retained(session("defaultOpen={false}", true))
                .is_err()
        );
        let mut uncontrolled = session("defaultOpen={true}", true);
        uncontrolled
            .dispatch(
                Action::Activate {
                    path: "0/1/0".into(),
                },
                &NoNavigation,
                &[],
            )
            .unwrap();
        let retained = uncontrolled
            .migrate_retained(session("defaultOpen={true}", true))
            .unwrap();
        assert!(!retained.widgets["0"].open);
    }

    #[test]
    fn nonmodal_tab_is_not_trapped_and_escape_is_only_a_request() {
        let mut session = session("open={true}", false);
        session.focus = Some("0/0".into());
        assert!(
            !session
                .dispatch(
                    Action::Key {
                        path: "0/0".into(),
                        key: "Tab".into(),
                        shift: false
                    },
                    &NoNavigation,
                    &["0/1/0".into()]
                )
                .unwrap()
        );
        assert_eq!(session.focus.as_deref(), Some("0/0"));
        let request = session
            .requested_open(&Action::Key {
                path: "0/1/0".into(),
                key: "Escape".into(),
                shift: false,
            })
            .unwrap();
        assert!(!request.open);
        assert!(session.widgets["0"].open);
    }
}
