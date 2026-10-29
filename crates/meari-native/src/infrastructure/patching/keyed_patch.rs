//! Prepare a retained DOM tree from the same native markup adapter before mutating the live tree.
use crate::application::markup::{Element, Markup};
use blitz_dom::{Attribute, DocumentMutator, LocalName, NodeData, NodeId, QualName};
use blitz_html::HtmlDocument;
use std::collections::{BTreeMap, BTreeSet};

enum Data {
    Element(QualName, Vec<Attribute>),
    Text(String),
    Comment(String),
}
pub(in super::super) struct Prepared {
    existing: Option<NodeId>,
    data: Data,
    children: Vec<Prepared>,
    clear: Vec<QualName>,
}

pub(in super::super) fn prepare(
    document: &HtmlDocument,
    old: &[Element],
    next: &Markup,
    dirty_inputs: &BTreeMap<String, String>,
) -> Result<Prepared, String> {
    for element in &next.elements {
        if let Some(previous) = old.iter().find(|previous| previous.id == element.id)
            && !previous.same_composition(element)
        {
            return Err("native child composition changed; remount is required".into());
        }
    }
    let template = HtmlDocument::from_html(&next.html, Default::default());
    let root = next
        .elements
        .iter()
        .find(|element| element.path == "0")
        .ok_or("native root metadata missing")?;
    let template_root = template
        .get_element_by_id(&root.id)
        .ok_or("prepared native root missing")?;
    let existing = document
        .get_element_by_id(&root.id)
        .ok_or("retained native root missing")?;
    let known = old
        .iter()
        .map(|element| element.id.as_str())
        .collect::<BTreeSet<_>>();
    let overrides = next
        .elements
        .iter()
        .map(|element| (element.id.as_str(), element))
        .collect::<BTreeMap<_, _>>();
    struct Context<'a> {
        document: &'a HtmlDocument,
        template: &'a HtmlDocument,
        known: BTreeSet<&'a str>,
        overrides: BTreeMap<&'a str, &'a Element>,
        dirty_inputs: &'a BTreeMap<String, String>,
        seen: BTreeSet<NodeId>,
        count: usize,
    }
    fn visit(
        context: &mut Context<'_>,
        source: NodeId,
        positional: Option<NodeId>,
        depth: usize,
    ) -> Result<Prepared, String> {
        context.count += 1;
        if context.count > 65536 || depth > 128 {
            return Err("native keyed tree exceeds budget".into());
        }
        let node = context
            .template
            .get_node(source)
            .ok_or("prepared native node missing")?;
        let id = node.attr(LocalName::from("id"));
        let existing = if let Some(id) = id {
            if context.known.contains(id) {
                Some(
                    context
                        .document
                        .get_element_by_id(id)
                        .ok_or("owned native identity disappeared")?,
                )
            } else {
                None
            }
        } else {
            positional
        };
        if let Some(existing) = existing {
            if !context.seen.insert(existing) {
                return Err("native retained identity aliases another node".into());
            }
            let old = context
                .document
                .get_node(existing)
                .ok_or("retained native node missing")?;
            let compatible = match (&old.data, &node.data) {
                (NodeData::Element(old), NodeData::Element(new)) => {
                    old.name == new.name
                        && old
                            .attrs
                            .iter()
                            .find(|attr| attr.name.local == blitz_dom::local_name!("type"))
                            .map(|attr| &attr.value)
                            == new
                                .attrs
                                .iter()
                                .find(|attr| attr.name.local == blitz_dom::local_name!("type"))
                                .map(|attr| &attr.value)
                }
                (NodeData::Text(_), NodeData::Text(_))
                | (NodeData::Comment { .. }, NodeData::Comment { .. }) => true,
                _ => false,
            };
            if !compatible {
                return Err("retained native node/control kind changed".into());
            }
        }
        let data = match &node.data {
            NodeData::Element(element) => {
                let mut attrs = element.attrs.iter().cloned().collect::<Vec<_>>();
                if let Some(metadata) = id.and_then(|id| context.overrides.get(id)) {
                    attrs = metadata
                        .attributes
                        .iter()
                        .map(|(name, value)| Attribute {
                            name: super::super::document::name(name),
                            value: value.clone(),
                        })
                        .collect();
                    if metadata.preserve_dirty_value
                        && let Some(value) = context.dirty_inputs.get(&metadata.path)
                    {
                        attrs.retain(|attribute| {
                            attribute.name.local != blitz_dom::local_name!("value")
                        });
                        attrs.push(Attribute {
                            name: super::super::document::name("value"),
                            value: value.clone(),
                        });
                    }
                }
                Data::Element(element.name.clone(), attrs)
            }
            NodeData::Text(text) => Data::Text(text.content.clone()),
            NodeData::Comment { contents } => Data::Comment(contents.clone()),
            _ => return Err("unsupported prepared native node kind".into()),
        };
        let old_children = existing
            .and_then(|id| context.document.get_node(id))
            .map(|node| node.children.to_vec())
            .unwrap_or_default();
        let source_children = node.children.to_vec();
        let anonymous = old_children
            .iter()
            .copied()
            .filter(|id| {
                context
                    .document
                    .get_node(*id)
                    .is_some_and(|node| node.attr(LocalName::from("id")).is_none())
            })
            .collect::<Vec<_>>();
        let mut anonymous = anonymous.into_iter();
        let mut children = Vec::with_capacity(source_children.len());
        for child in source_children {
            let positional = if context
                .template
                .get_node(child)
                .is_some_and(|node| node.attr(LocalName::from("id")).is_none())
            {
                anonymous.next()
            } else {
                None
            };
            children.push(visit(context, child, positional, depth + 1)?);
        }
        let clear = match (
            &data,
            existing
                .and_then(|id| context.document.get_node(id))
                .and_then(|node| node.element_data()),
        ) {
            (Data::Element(_, attrs), Some(old)) => old
                .attrs
                .iter()
                .filter(|attribute| !attrs.iter().any(|next| next.name == attribute.name))
                .map(|attribute| attribute.name.clone())
                .collect(),
            _ => Vec::new(),
        };
        let data = match data {
            Data::Element(name, attrs) if existing.is_some() => {
                let old = context
                    .document
                    .get_node(existing.unwrap())
                    .and_then(|node| node.element_data())
                    .ok_or("retained native element missing")?;
                Data::Element(
                    name,
                    attrs
                        .into_iter()
                        .filter(|attribute| !old.attrs.iter().any(|old| old == attribute))
                        .collect(),
                )
            }
            other => other,
        };
        Ok(Prepared {
            existing,
            data,
            children,
            clear,
        })
    }
    let mut context = Context {
        document,
        template: &template,
        known,
        overrides,
        dirty_inputs,
        seen: BTreeSet::new(),
        count: 0,
    };
    let prepared = visit(&mut context, template_root, Some(existing), 0)?;
    if prepared.existing != Some(existing) {
        return Err("native root identity cannot be replaced".into());
    }
    Ok(prepared)
}

impl Prepared {
    pub(in super::super) fn apply(self, mutation: &mut DocumentMutator<'_>) -> NodeId {
        let node = match self.data {
            Data::Element(name, attrs) => {
                if let Some(node) = self.existing {
                    let old = mutation.element_name(node).cloned();
                    debug_assert_eq!(old.as_ref(), Some(&name));
                    for name in self.clear {
                        mutation.clear_attribute(node, name);
                    }
                    for attribute in &attrs {
                        mutation.set_attribute(node, attribute.name.clone(), &attribute.value);
                    }
                    node
                } else {
                    mutation.create_element(name, attrs)
                }
            }
            Data::Text(text) => {
                if let Some(node) = self.existing {
                    mutation.set_node_text(node, &text);
                    node
                } else {
                    mutation.create_text_node(&text)
                }
            }
            Data::Comment(text) => self
                .existing
                .unwrap_or_else(|| mutation.create_comment_node(&text)),
        };
        let children = self
            .children
            .into_iter()
            .map(|child| child.apply(mutation))
            .collect::<Vec<_>>();
        let old = mutation.child_ids(node);
        let retained = children.iter().copied().collect::<BTreeSet<_>>();
        for child in old {
            if !retained.contains(&child) {
                mutation.remove_and_drop_node(child);
            }
        }
        if !children.is_empty() {
            mutation.append_children(node, &children);
        }
        node
    }
}
