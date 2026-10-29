use super::document::InteractiveDocument;
use crate::domain::widgets::Session;
use std::collections::BTreeMap;
#[cfg(test)]
mod tests;

enum FocusRequest {
    Enter(String),
    Return(Option<String>),
}
pub(super) struct ControlledFocusPlan {
    previous: Option<String>,
    requests: Vec<FocusRequest>,
}

impl InteractiveDocument {
    pub(super) fn prepare_controlled_focus(
        &self,
        next: &Session,
        old_paths: &BTreeMap<String, String>,
    ) -> ControlledFocusPlan {
        let previous = self
            .renderer
            .document
            .get_focussed_node_id()
            .and_then(|id| {
                self.elements()
                    .iter()
                    .find(|element| {
                        self.renderer.document.get_element_by_id(&element.id) == Some(id)
                    })
                    .map(|element| element.path.clone())
            });
        let mut requests = Vec::new();
        for widget in next
            .widgets
            .values()
            .filter(|widget| widget.controlled_open)
        {
            let Some(before) = old_paths
                .get(&widget.root)
                .and_then(|path| self.session.widgets.get(path))
            else {
                if widget.open
                    && widget.modal
                    && let Some(content) = &widget.content
                {
                    requests.push(FocusRequest::Enter(content.clone()));
                }
                continue;
            };
            if widget.open && widget.modal && (!before.open || !before.modal) {
                if let Some(content) = &widget.content {
                    requests.push(FocusRequest::Enter(content.clone()));
                }
            } else if !widget.open && before.open {
                let inside = previous.as_ref().zip(before.content.as_ref()).is_some_and(
                    |(path, content)| path == content || path.starts_with(&format!("{content}/")),
                );
                if before.modal || inside {
                    requests.push(FocusRequest::Return(widget.trigger.clone()));
                }
            }
        }
        ControlledFocusPlan { previous, requests }
    }

    pub(super) fn focus_initial_controlled_overlays(&mut self) -> Result<(), String> {
        let requests = self
            .session
            .widgets
            .values()
            .filter(|widget| widget.controlled_open && widget.open && widget.modal)
            .filter_map(|widget| widget.content.clone().map(FocusRequest::Enter))
            .collect();
        self.apply_controlled_focus(&ControlledFocusPlan {
            previous: None,
            requests,
        })
    }

    fn programmatic_focus_target(&self, path: &str) -> bool {
        self.admit_callback_target(path, None, false).is_ok()
            && self
                .path_id(path)
                .and_then(|id| self.renderer.document.get_element_by_id(id))
                .and_then(|id| self.renderer.document.get_node(id))
                .is_some_and(|node| node.is_focussable())
    }

    fn set_controlled_focus(&mut self, path: &str) -> Result<(), String> {
        if !self.programmatic_focus_target(path) {
            return Err("controlled focus target is unavailable".into());
        }
        let id = self
            .path_id(path)
            .and_then(|id| self.renderer.document.get_element_by_id(id))
            .ok_or("controlled focus identity disappeared")?;
        self.renderer.document.set_focus_to(id);
        if self.renderer.document.get_focussed_node_id() != Some(id) {
            return Err("controlled focus was not accepted".into());
        }
        self.session.focus = Some(path.into());
        Ok(())
    }

    pub(super) fn apply_controlled_focus(
        &mut self,
        plan: &ControlledFocusPlan,
    ) -> Result<(), String> {
        for request in &plan.requests {
            let target = match request {
                FocusRequest::Enter(content) => self
                    .focusable(Some(content))
                    .into_iter()
                    .next()
                    .or_else(|| {
                        self.programmatic_focus_target(content)
                            .then(|| content.clone())
                    })
                    .ok_or("controlled modal has no visible focus target")?,
                FocusRequest::Return(preferred) => {
                    let target = preferred
                        .as_ref()
                        .filter(|path| self.programmatic_focus_target(path))
                        .cloned()
                        .or_else(|| self.focusable(None).into_iter().next());
                    let Some(target) = target else {
                        self.renderer.document.clear_focus();
                        self.session.focus = None;
                        continue;
                    };
                    target
                }
            };
            self.set_controlled_focus(&target)?;
        }
        Ok(())
    }

    pub(super) fn restore_controlled_focus(
        &mut self,
        plan: &ControlledFocusPlan,
    ) -> Result<(), String> {
        if let Some(previous) = &plan.previous {
            self.set_controlled_focus(previous)
        } else {
            self.renderer.document.clear_focus();
            self.session.focus = None;
            Ok(())
        }
    }
}
