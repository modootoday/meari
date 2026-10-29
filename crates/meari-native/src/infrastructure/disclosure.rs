//! Bridge summary activation to the published DOM engine's disclosure behavior.
use super::document::InteractiveDocument;
use crate::domain::widgets::Action;
use blitz_dom::{LocalName, NodeId};

impl InteractiveDocument {
    pub(super) fn disclosure_open(&self, path: &str) -> Option<bool> {
        let document = &self.renderer.document;
        let target = document.get_element_by_id(self.path_id(path)?)?;
        let node = document.get_node(target)?;
        if !node
            .element_data()
            .is_some_and(|data| data.name.local == blitz_dom::local_name!("summary"))
        {
            return None;
        }
        let details = document.get_node(node.parent?)?;
        if !details
            .element_data()
            .is_some_and(|data| data.name.local == blitz_dom::local_name!("details"))
        {
            return None;
        }
        let first = details.children.iter().copied().find(|id| {
            document.get_node(*id).is_some_and(|node| {
                node.element_data()
                    .is_some_and(|data| data.name.local == blitz_dom::local_name!("summary"))
            })
        });
        (first == Some(target)).then(|| details.attr(LocalName::from("open")).is_some())
    }
    pub(super) fn dispatch_disclosure(&mut self, action: &Action) -> Result<Option<bool>, String> {
        let path = match action {
            Action::Activate { path } => path,
            Action::Key { path, key, .. } if key == "Enter" || key == " " => path,
            _ => return Ok(None),
        };
        let Some(target) = self
            .path_id(path)
            .and_then(|id| self.renderer.document.get_element_by_id(id))
        else {
            return Ok(None);
        };
        let document = &self.renderer.document;
        let Some(node) = document.get_node(target) else {
            return Ok(None);
        };
        if !node
            .element_data()
            .is_some_and(|data| data.name.local == blitz_dom::local_name!("summary"))
        {
            return Ok(None);
        }
        let Some(parent) = node.parent else {
            return Ok(None);
        };
        let Some(details) = document.get_node(parent) else {
            return Ok(None);
        };
        if !details
            .element_data()
            .is_some_and(|data| data.name.local == blitz_dom::local_name!("details"))
        {
            return Ok(None);
        }
        let first = details.children.iter().copied().find(|id| {
            document.get_node(*id).is_some_and(|node| {
                node.element_data()
                    .is_some_and(|data| data.name.local == blitz_dom::local_name!("summary"))
            })
        });
        if first != Some(target) {
            return Ok(Some(false));
        }
        let mut ancestor: Option<NodeId> = Some(target);
        while let Some(id) = ancestor {
            let node = document.get_node(id).ok_or("disclosure ancestor missing")?;
            if ["hidden", "inert", "disabled"]
                .iter()
                .any(|name| node.attr(LocalName::from(*name)).is_some())
                || node.attr(LocalName::from("aria-disabled")) == Some("true")
            {
                return Ok(Some(false));
            }
            ancestor = node.parent;
        }
        let id = details
            .attr(LocalName::from("id"))
            .ok_or("owned details ID missing")?
            .to_string();
        let open = details.attr(LocalName::from("open")).is_none();
        self.renderer.document.toggle_details_open(parent);
        self.disclosure_values.insert(id.clone(), open);
        if let Some(element) = self.elements_mut_for_disclosure(&id) {
            if open {
                element.attributes.insert("open".into(), String::new());
            } else {
                element.attributes.remove("open");
            }
        }
        self.renderer.document.resolve(0.0);
        Ok(Some(true))
    }
}
