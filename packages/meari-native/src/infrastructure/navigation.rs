use crate::domain::widgets::{FocusMove, Item, Navigation};
use dioxus_shadcn_primitives::roving_focus::{self, RovingFocusItem, RovingFocusState};

pub struct PublishedNavigation;
impl Navigation for PublishedNavigation {
    fn move_focus(
        &self,
        items: &[Item],
        active: Option<&str>,
        movement: FocusMove,
    ) -> Option<String> {
        let items: Vec<_> = items
            .iter()
            .map(|i| RovingFocusItem {
                id: i.path.clone(),
                disabled: i.disabled,
            })
            .collect();
        let state = RovingFocusState {
            active_id: active.map(str::to_owned),
            ..Default::default()
        };
        let movement = match movement {
            FocusMove::Next => roving_focus::FocusMove::Next,
            FocusMove::Previous => roving_focus::FocusMove::Previous,
            FocusMove::First => roving_focus::FocusMove::First,
            FocusMove::Last => roving_focus::FocusMove::Last,
        };
        state.move_focus(&items, movement).map(str::to_owned)
    }
}
