use super::Session;

impl Session {
    /// Preserve live widget state only when navigation and interaction identities still match.
    pub fn migrate_retained(&self, mut next: Self) -> Result<Self, String> {
        if !self.widgets.keys().eq(next.widgets.keys()) {
            return Err("widget roots changed; remount is required".into());
        }
        if !self.widgets.is_empty() && self.disabled_paths != next.disabled_paths {
            return Err("widget disabled controls changed; migration is required".into());
        }
        for (path, before) in &self.widgets {
            let after = next
                .widgets
                .get_mut(path)
                .ok_or("widget root disappeared")?;
            if before.family != after.family
                || before.root != after.root
                || before.trigger != after.trigger
                || before.content != after.content
                || before.close_paths != after.close_paths
                || before.vertical != after.vertical
                || before.items.len() != after.items.len()
                || before
                    .items
                    .iter()
                    .zip(&after.items)
                    .any(|(before, after)| {
                        before.path != after.path
                            || before.value != after.value
                            || before.disabled != after.disabled
                    })
            {
                return Err("widget interaction layout changed; remount is required".into());
            }
            if before.controlled_open != after.controlled_open {
                return Err("controlled open mode changed; remount is required".into());
            }
            if before.controlled_value != after.controlled_value
                || before.radio_orientation != after.radio_orientation
                || before.radio_direction != after.radio_direction
                || before.radio_loop != after.radio_loop
            {
                return Err("radio selection contract changed; remount is required".into());
            }
            if !after.controlled_value {
                after.value = before.value.clone();
            }
            if after.family == super::Family::RadioGroup {
                after.active = self
                    .focus
                    .as_ref()
                    .filter(|path| before.items.iter().any(|item| item.path == **path))
                    .cloned()
                    .or(after
                        .radio_model(next.disabled_paths.contains(path))
                        .tab_stop(None)?
                        .map(str::to_owned));
            } else {
                after.active = before.active.clone();
            }
            if !after.controlled_open {
                after.open = before.open;
            }
        }
        next.focus = self.focus.clone();
        next.changes = self.changes.clone();
        Ok(next)
    }
}
