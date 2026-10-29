//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::domain::widgets::{self, Session};
use meari::domain::{Component, Node, NodeKind};
use std::collections::{BTreeMap, BTreeSet};

pub fn build(component: &Component, session: &Session) -> Result<Markup, String> {
    fn identifiers(
        node: &Node,
        path: String,
        identity: String,
        ids: &mut BTreeMap<String, String>,
        seen: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        let composed = composition::prepare(node, &path)?;
        let node = composed
            .as_ref()
            .map(|composed| &composed.node)
            .unwrap_or(node);
        let content_path = composed
            .as_ref()
            .map(|composed| composed.child_path.as_str())
            .unwrap_or(&path);
        if let NodeKind::Element { children, .. } = &node.kind {
            let identity = widgets::string(node, meari::application::repeat::IDENTITY_ATTRIBUTE)
                .map(|key| format!("key-{key}"))
                .unwrap_or(identity);
            let id = widgets::string(node, "id")
                .unwrap_or_else(|| format!("bridge-{}", identity.replace('/', "-")));
            if id.trim().is_empty() || !seen.insert(id.clone()) {
                return Err(format!("empty or duplicate DOM id: {id}"));
            }
            ids.insert(path.clone(), id.clone());
            if let Some(composed) = &composed {
                ids.insert(composed.child_path.clone(), id);
            }
            let mut unkeyed = 0usize;
            for (index, child) in children.iter().enumerate() {
                let keyed = widgets::string(child, meari::application::repeat::IDENTITY_ATTRIBUTE)
                    .is_some();
                let child_identity = if keyed {
                    // A keyed root replaces this provisional identity in identifiers().
                    identity.clone()
                } else {
                    let next = format!("{identity}/{unkeyed}");
                    unkeyed += 1;
                    next
                };
                identifiers(
                    child,
                    format!("{content_path}/{index}"),
                    child_identity,
                    ids,
                    seen,
                )?;
            }
        }
        Ok(())
    }
    let mut ids = BTreeMap::new();
    identifiers(
        &component.root,
        "0".into(),
        "0".into(),
        &mut ids,
        &mut BTreeSet::new(),
    )?;
    let mut markup = Markup {
        html: String::new(),
        classes: String::new(),
        elements: Vec::new(),
        panels: BTreeMap::new(),
        force_mounted: mount::collect(&component.root)?,
    };
    fn panels(
        node: &Node,
        path: String,
        item: Option<String>,
        session: &Session,
        out: &mut BTreeMap<(String, String), String>,
    ) {
        let NodeKind::Element { tag, children, .. } = &node.kind else {
            return;
        };
        let item = if tag == "AccordionItem" {
            widgets::string(node, "value")
        } else {
            item
        };
        if ["TabsContent", "AccordionContent"].contains(&tag.as_str())
            && let Some(value) = widgets::string(node, "value").or_else(|| item.clone())
            && let Some(widget) = session.owner(&path)
        {
            out.insert((widget.root.clone(), value), path.clone());
        }
        for (index, child) in children.iter().enumerate() {
            panels(child, format!("{path}/{index}"), item.clone(), session, out);
        }
    }
    panels(
        &component.root,
        "0".into(),
        None,
        session,
        &mut markup.panels,
    );
    visit(
        &component.root,
        "0".into(),
        None,
        session,
        &ids,
        &mut markup,
    )?;
    Ok(markup)
}
