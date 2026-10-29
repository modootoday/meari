//! Shared radio navigation policy. Adapters guard requests and publish accepted props.
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Orientation {
    #[default]
    Unspecified,
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    #[default]
    LeftToRight,
    RightToLeft,
}

impl Direction {
    /// `None` preserves an unresolved HTML `auto` boundary until a host resolves it.
    pub fn inherit_html(value: Option<&str>, parent: Option<Self>) -> Option<Self> {
        match value {
            Some(value) if value.eq_ignore_ascii_case("ltr") => Some(Self::LeftToRight),
            Some(value) if value.eq_ignore_ascii_case("rtl") => Some(Self::RightToLeft),
            Some(value) if value.eq_ignore_ascii_case("auto") => None,
            _ => parent,
        }
    }
}

#[cfg(test)]
mod direction_tests {
    use super::Direction;

    #[test]
    fn html_direction_preserves_auto_boundaries_and_valid_explicit_overrides() {
        let rtl = Some(Direction::RightToLeft);
        assert_eq!(Direction::inherit_html(None, rtl), rtl);
        assert_eq!(Direction::inherit_html(Some("invalid"), rtl), rtl);
        assert_eq!(Direction::inherit_html(Some(""), rtl), rtl);
        assert_eq!(Direction::inherit_html(Some("AUTO"), rtl), None);
        assert_eq!(Direction::inherit_html(None, None), None);
        assert_eq!(
            Direction::inherit_html(Some("LTR"), None),
            Some(Direction::LeftToRight)
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub path: String,
    pub value: String,
    /// Includes adapter-computed visibility and inherited root/item disabled state.
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadioGroup {
    pub items: Vec<Item>,
    /// Empty means none; an unmatched external value also does not select an item.
    pub checked: String,
    pub disabled: bool,
    pub orientation: Orientation,
    pub direction: Direction,
    pub looping: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub focus: String,
    pub value: String,
    pub changes_checked: bool,
}

impl RadioGroup {
    pub fn handles_key(&self, key: &str) -> bool {
        match key {
            " " | "Space" | "Home" | "End" => true,
            "ArrowDown" | "ArrowUp" => self.orientation != Orientation::Horizontal,
            "ArrowLeft" | "ArrowRight" => self.orientation != Orientation::Vertical,
            _ => false,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.items.len() > 16384 || self.checked.len() > 4096 {
            return Err("radio group exceeds item/value budget".into());
        }
        let mut paths = BTreeSet::new();
        let mut values = BTreeSet::new();
        let mut bytes = self.checked.len();
        for item in &self.items {
            bytes = bytes
                .checked_add(item.path.len())
                .and_then(|bytes| bytes.checked_add(item.value.len()))
                .ok_or("radio group byte budget overflow")?;
            if item.path.is_empty()
                || item.path.len() > 4096
                || item.value.is_empty()
                || item.value.len() > 4096
                || bytes > 1024 * 1024
                || !paths.insert(&item.path)
                || !values.insert(&item.value)
            {
                return Err(
                    "radio group requires unique nonempty paths/values within byte budgets".into(),
                );
            }
        }
        Ok(())
    }

    /// Roving focus does not change checked state or request an application callback.
    pub fn tab_stop(&self, focused: Option<&str>) -> Result<Option<&str>, String> {
        self.validate()?;
        if self.disabled {
            return Ok(None);
        }
        Ok(focused
            .and_then(|path| {
                self.items
                    .iter()
                    .find(|item| !item.disabled && item.path == path)
            })
            .or_else(|| {
                self.items
                    .iter()
                    .find(|item| !item.disabled && item.value == self.checked)
            })
            .or_else(|| self.items.iter().find(|item| !item.disabled))
            .map(|item| item.path.as_str()))
    }

    fn request(&self, item: &Item) -> Request {
        Request {
            focus: item.path.clone(),
            value: item.value.clone(),
            changes_checked: item.value != self.checked,
        }
    }

    /// The returned request is inert: only a renderer/application commit changes props.
    pub fn activate(&self, path: &str) -> Result<Option<Request>, String> {
        self.validate()?;
        if self.disabled {
            return Ok(None);
        }
        Ok(self
            .items
            .iter()
            .find(|item| item.path == path && !item.disabled)
            .map(|item| self.request(item)))
    }

    pub fn key(&self, path: &str, key: &str) -> Result<Option<Request>, String> {
        self.validate()?;
        if self.disabled {
            return Ok(None);
        }
        let enabled = self
            .items
            .iter()
            .filter(|item| !item.disabled)
            .collect::<Vec<_>>();
        let Some(current) = enabled.iter().position(|item| item.path == path) else {
            return Ok(None);
        };
        let next = match key {
            " " | "Space" => current,
            "Home" => 0,
            "End" => enabled.len() - 1,
            "ArrowDown" | "ArrowUp" if self.orientation != Orientation::Horizontal => {
                let forward = key == "ArrowDown";
                self.adjacent(current, enabled.len(), forward)
            }
            "ArrowRight" | "ArrowLeft" if self.orientation != Orientation::Vertical => {
                let forward = (key == "ArrowRight") != (self.direction == Direction::RightToLeft);
                self.adjacent(current, enabled.len(), forward)
            }
            _ => return Ok(None),
        };
        Ok(Some(self.request(enabled[next])))
    }

    fn adjacent(&self, current: usize, len: usize, forward: bool) -> usize {
        if forward {
            if current + 1 < len {
                current + 1
            } else if self.looping {
                0
            } else {
                current
            }
        } else if current > 0 {
            current - 1
        } else if self.looping {
            len - 1
        } else {
            current
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group() -> RadioGroup {
        RadioGroup {
            items: ["a", "b", "c"]
                .into_iter()
                .map(|value| Item {
                    path: format!("0/{value}"),
                    value: value.into(),
                    disabled: value == "b",
                })
                .collect(),
            checked: String::new(),
            disabled: false,
            orientation: Orientation::Unspecified,
            direction: Direction::LeftToRight,
            looping: true,
        }
    }

    #[test]
    fn none_and_unmatched_values_keep_focus_separate_from_checked_state() {
        let mut group = group();
        assert_eq!(group.tab_stop(None).unwrap(), Some("0/a"));
        let request = group.key("0/a", "ArrowDown").unwrap().unwrap();
        assert_eq!(request.focus, "0/c");
        assert_eq!(request.value, "c");
        assert!(request.changes_checked);
        assert_eq!(group.checked, "");
        group.checked = "missing".into();
        assert_eq!(group.tab_stop(None).unwrap(), Some("0/a"));
        assert_eq!(group.checked, "missing");
        group.checked = "c".into();
        assert_eq!(group.tab_stop(None).unwrap(), Some("0/c"));
        assert_eq!(group.tab_stop(Some("0/a")).unwrap(), Some("0/a"));
        assert!(!group.activate("0/c").unwrap().unwrap().changes_checked);
    }

    #[test]
    fn orientation_rtl_looping_home_end_and_space_use_enabled_owned_items() {
        let mut group = group();
        assert_eq!(group.key("0/a", "End").unwrap().unwrap().value, "c");
        assert_eq!(group.key("0/c", "Home").unwrap().unwrap().value, "a");
        assert_eq!(group.key("0/c", "ArrowRight").unwrap().unwrap().value, "a");
        group.looping = false;
        assert_eq!(group.key("0/c", "ArrowRight").unwrap().unwrap().value, "c");
        group.orientation = Orientation::Horizontal;
        assert!(group.key("0/a", "ArrowDown").unwrap().is_none());
        group.direction = Direction::RightToLeft;
        assert_eq!(group.key("0/a", "ArrowLeft").unwrap().unwrap().value, "c");
        group.orientation = Orientation::Vertical;
        assert!(group.key("0/a", "ArrowLeft").unwrap().is_none());
        assert_eq!(group.key("0/a", "ArrowDown").unwrap().unwrap().value, "c");
        assert_eq!(group.key("0/a", " ").unwrap().unwrap().value, "a");
        for key in ["Enter", "Tab", "Escape"] {
            assert!(group.key("0/a", key).unwrap().is_none());
        }
        assert!(group.activate("0/foreign").unwrap().is_none());
        assert!(group.key("0/foreign", "Home").unwrap().is_none());
    }

    #[test]
    fn disabled_root_items_and_empty_groups_do_not_request_selection() {
        let mut group = group();
        assert!(group.activate("0/b").unwrap().is_none());
        assert!(group.key("0/b", "Home").unwrap().is_none());
        group.disabled = true;
        assert!(group.tab_stop(None).unwrap().is_none());
        assert!(group.activate("0/a").unwrap().is_none());
        assert!(group.key("0/a", "End").unwrap().is_none());
        group.disabled = false;
        group.items.clear();
        assert!(group.tab_stop(None).unwrap().is_none());
        assert!(group.key("0/a", "Home").unwrap().is_none());
    }

    #[test]
    fn duplicate_empty_and_oversized_item_values_are_rejected() {
        let mut group = group();
        group.items[1].value = "a".into();
        assert!(group.validate().is_err());
        group.items[1].value = String::new();
        assert!(group.validate().is_err());
        group.items[1].value = "x".repeat(4097);
        assert!(group.validate().is_err());
    }
}
