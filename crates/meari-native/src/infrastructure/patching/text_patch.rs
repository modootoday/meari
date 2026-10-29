use crate::application::markup::Element;
use blitz_dom::NodeId;
use blitz_html::HtmlDocument;

pub(in super::super) struct TextPatch {
    parent: NodeId,
    anchor: Option<NodeId>,
    node: Option<NodeId>,
    value: String,
}

pub(in super::super) fn prepare(
    document: &HtmlDocument,
    previous: &[Element],
    next: &[Element],
) -> Result<Vec<TextPatch>, String> {
    let mut updates = Vec::new();
    for element in next {
        let old = previous
            .iter()
            .find(|old| old.path == element.path)
            .ok_or("text element shape changed")?;
        if old.text_runs.len() != element.text_runs.len() {
            return Err("text run shape changed".into());
        }
        for (before, after) in old.text_runs.iter().zip(&element.text_runs) {
            if before.before != after.before {
                return Err("text anchor shape changed".into());
            }
            if before.text == after.text {
                continue;
            }
            let parent = document
                .get_element_by_id(&old.id)
                .ok_or("text parent missing")?;
            let anchor = after
                .before
                .as_ref()
                .map(|path| {
                    let element = previous
                        .iter()
                        .find(|element| element.path == *path)
                        .ok_or("text anchor missing")?;
                    document
                        .get_element_by_id(&element.id)
                        .ok_or("text anchor DOM identity missing")
                })
                .transpose()?;
            let children = &document
                .get_node(parent)
                .ok_or("text parent DOM missing")?
                .children;
            let index = match anchor {
                Some(anchor) => children
                    .iter()
                    .position(|child| *child == anchor)
                    .ok_or("text anchor has wrong parent")?,
                None => children.len(),
            };
            let node = index
                .checked_sub(1)
                .and_then(|index| children.get(index))
                .copied()
                .filter(|id| {
                    document
                        .get_node(*id)
                        .is_some_and(|node| node.is_text_node())
                });
            if node.is_none() && !before.text.is_empty() {
                return Err("owned text run DOM identity lost".into());
            }
            updates.push(TextPatch {
                parent,
                anchor,
                node,
                value: after.text.clone(),
            });
        }
    }
    Ok(updates)
}

pub(in super::super) fn apply(document: &mut HtmlDocument, updates: Vec<TextPatch>) {
    for update in updates {
        let mut mutation = document.mutate();
        if let Some(node) = update.node {
            mutation.set_node_text(node, &update.value);
        } else {
            let node = mutation.create_text_node(&update.value);
            if let Some(anchor) = update.anchor {
                mutation.insert_nodes_before(anchor, &[node]);
            } else {
                mutation.append_children(update.parent, &[node]);
            }
        }
    }
}
