//! Renderer-neutral composite widget policy. Actual OS event/a11y adapters are separate.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod controlled;
#[cfg(test)]
mod direction_tests;
mod reload;
pub use controlled::OpenRequest;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Family {
    Tabs,
    RadioGroup,
    Accordion,
    Dialog,
    AlertDialog,
    Popover,
    Select,
    Menu,
}

#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub path: String,
    pub value: String,
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Widget {
    pub root: String,
    pub family: Family,
    pub items: Vec<Item>,
    pub value: String,
    pub active: Option<String>,
    pub open: bool,
    pub controlled_open: bool,
    pub controlled_value: bool,
    pub radio_orientation: meari::domain::radio_group::Orientation,
    pub radio_direction: meari::domain::radio_group::Direction,
    pub radio_loop: bool,
    pub modal: bool,
    pub trigger: Option<String>,
    pub content: Option<String>,
    pub close_paths: Vec<String>,
    pub vertical: bool,
}

impl Widget {
    pub(crate) fn radio_model(&self, disabled: bool) -> meari::domain::radio_group::RadioGroup {
        use meari::domain::radio_group::{Item, RadioGroup};
        RadioGroup {
            items: self
                .items
                .iter()
                .map(|item| Item {
                    path: item.path.clone(),
                    value: item.value.clone(),
                    disabled: item.disabled,
                })
                .collect(),
            checked: self.value.clone(),
            disabled,
            orientation: self.radio_orientation,
            direction: self.radio_direction,
            looping: self.radio_loop,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Session {
    pub widgets: BTreeMap<String, Widget>,
    pub focus: Option<String>,
    pub changes: Vec<Change>,
    pub disabled_paths: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub root: String,
    pub value: String,
    pub open: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Activate {
        path: String,
    },
    Key {
        path: String,
        key: String,
        shift: bool,
    },
    DismissOutside {
        path: String,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum FocusMove {
    Next,
    Previous,
    First,
    Last,
}

pub trait Navigation {
    fn move_focus(
        &self,
        items: &[Item],
        active: Option<&str>,
        movement: FocusMove,
    ) -> Option<String>;
}

mod tree_queries;
pub use self::tree_queries::*;
mod dispatcher;
