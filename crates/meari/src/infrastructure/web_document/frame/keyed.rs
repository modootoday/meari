//! Prepare detached insertions and stable-key matches before changing the live Web frame.
use super::{DomNode, Frame, error};
use crate::application::{repeat::IDENTITY_ATTRIBUTE, source_modules::child_placement};
use crate::domain::Span;
use std::collections::{BTreeMap, BTreeSet};
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, HtmlElement, Node};

pub(super) fn contains(node: &DomNode) -> bool {
    matches!(node, DomNode::Element { attributes, children, .. } if attributes.contains_key(IDENTITY_ATTRIBUTE) || children.iter().any(contains))
}
fn key(node: &DomNode) -> Option<&str> {
    match node {
        DomNode::Element { attributes, .. } => {
            attributes.get(IDENTITY_ATTRIBUTE).map(String::as_str)
        }
        _ => None,
    }
}
fn shallow(node: &DomNode) -> DomNode {
    match node {
        DomNode::Text(value) => DomNode::Text(value.clone()),
        DomNode::Element {
            tag, attributes, ..
        } => DomNode::Element {
            tag: tag.clone(),
            attributes: attributes.clone(),
            children: Vec::new(),
        },
    }
}

enum Prepared {
    New(Frame, String),
    Retained {
        node: Node,
        path: String,
        old: Box<DomNode>,
        next: Box<DomNode>,
        old_children: Vec<Node>,
        removed: Vec<Node>,
        children: Vec<Prepared>,
    },
}
impl Prepared {
    fn node(&self) -> &Node {
        match self {
            Self::New(frame, _) => &frame.root,
            Self::Retained { node, .. } => node,
        }
    }
    fn collect(&self, nodes: &mut BTreeMap<String, Node>) {
        match self {
            Self::New(frame, path) => {
                for (suffix, node) in &frame.nodes {
                    nodes.insert(format!("{path}{}", &suffix[1..]), node.clone());
                }
            }
            Self::Retained {
                node,
                path,
                children,
                ..
            } => {
                nodes.insert(path.clone(), node.clone());
                for child in children {
                    child.collect(nodes);
                }
            }
        }
    }
    fn apply(&self) -> Result<(), String> {
        let Self::Retained {
            node,
            old,
            next,
            removed,
            children,
            ..
        } = self
        else {
            return Ok(());
        };
        attributes(node, old, next)?;
        for child in children {
            child.apply()?;
        }
        for child in removed {
            node.remove_child(child).map_err(error)?;
        }
        order(
            node,
            &children
                .iter()
                .map(|child| child.node().clone())
                .collect::<Vec<_>>(),
        )
    }
    fn restore(&self) -> Result<(), String> {
        let Self::Retained {
            node,
            old,
            next,
            old_children,
            children,
            ..
        } = self
        else {
            return Ok(());
        };
        attributes(node, next, old)?;
        for child in children {
            if let Self::New(frame, _) = child
                && frame
                    .root
                    .parent_node()
                    .is_some_and(|parent| parent.is_same_node(Some(node)))
            {
                node.remove_child(&frame.root).map_err(error)?;
            }
        }
        order(node, old_children)?;
        for child in children {
            child.restore()?;
        }
        Ok(())
    }
}

fn attributes(node: &Node, old: &DomNode, next: &DomNode) -> Result<(), String> {
    match (old, next) {
        (DomNode::Text(_), DomNode::Text(value)) => node.set_node_value(Some(value)),
        (
            DomNode::Element {
                attributes: old, ..
            },
            DomNode::Element {
                attributes: next, ..
            },
        ) => {
            let element = node
                .dyn_ref::<Element>()
                .ok_or("retained Web element changed")?;
            for name in old.keys().filter(|name| !next.contains_key(*name)) {
                element.remove_attribute(name).map_err(error)?;
            }
            for (name, value) in next {
                if old.get(name) != Some(value) {
                    element.set_attribute(name, value).map_err(error)?;
                }
            }
        }
        _ => return Err("retained Web shape changed".into()),
    }
    Ok(())
}
fn order(parent: &Node, children: &[Node]) -> Result<(), String> {
    let mut anchor: Option<&Node> = None;
    for child in children.iter().rev() {
        let same_parent = child
            .parent_node()
            .is_some_and(|node| node.is_same_node(Some(parent)));
        let same_next = match (child.next_sibling(), anchor) {
            (None, None) => true,
            (Some(next), Some(anchor)) => next.is_same_node(Some(anchor)),
            _ => false,
        };
        if !same_parent || !same_next {
            parent.insert_before(child, anchor).map_err(error)?;
        }
        anchor = Some(child);
    }
    Ok(())
}

impl Frame {
    pub(super) fn patch_keyed(
        &mut self,
        next: DomNode,
        parent: &Element,
    ) -> Result<(), (String, bool)> {
        self.preflight(&self.plan, parent)
            .map_err(|error| (error, false))?;
        let document = parent
            .owner_document()
            .ok_or_else(|| ("Web owner document missing".into(), false))?;
        struct Context<'a> {
            frame: &'a Frame,
            document: &'a Document,
            budget: usize,
        }
        fn prepare(
            context: &mut Context<'_>,
            old: &DomNode,
            next: &DomNode,
            old_path: &str,
            path: String,
            depth: usize,
        ) -> Result<Prepared, String> {
            context.budget += 1;
            if context.budget > 65536 || depth > 128 {
                return Err("Web keyed tree exceeds node/depth budget".into());
            }
            let frame = context.frame;
            let document = context.document;
            let node = frame
                .nodes
                .get(old_path)
                .ok_or("retained Web identity missing")?
                .clone();
            let mut prepared = Vec::new();
            let mut old_nodes = Vec::new();
            let mut removed = Vec::new();
            match (old, next) {
                (DomNode::Text(_), DomNode::Text(_)) => {}
                (
                    DomNode::Element {
                        tag,
                        attributes,
                        children,
                    },
                    DomNode::Element {
                        tag: next_tag,
                        attributes: next_attributes,
                        children: next_children,
                    },
                ) if tag == next_tag => {
                    for identity in ["id", "type", "role"] {
                        if attributes.get(identity) != next_attributes.get(identity) {
                            return Err("retained Web control identity changed".into());
                        }
                    }
                    let before = children.iter().map(key).collect::<Vec<_>>();
                    let after = next_children.iter().map(key).collect::<Vec<_>>();
                    let matches = child_placement::plan(&before, &after, Span::default())
                        .map_err(|error| error.message)?;
                    old_nodes = (0..children.len())
                        .map(|index| {
                            frame
                                .nodes
                                .get(&format!("{old_path}/{index}"))
                                .cloned()
                                .ok_or("Web child identity missing")
                        })
                        .collect::<Result<_, _>>()?;
                    let retained = matches.iter().flatten().copied().collect::<BTreeSet<_>>();
                    removed = old_nodes
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| !retained.contains(index))
                        .map(|(_, node)| node.clone())
                        .collect();
                    for (index, (child, previous)) in next_children.iter().zip(matches).enumerate()
                    {
                        let next_path = format!("{path}/{index}");
                        prepared.push(if let Some(previous) = previous {
                            prepare(
                                context,
                                &children[previous],
                                child,
                                &format!("{old_path}/{previous}"),
                                next_path,
                                depth + 1,
                            )?
                        } else {
                            Prepared::New(Frame::build(document, child.clone())?, next_path)
                        });
                    }
                }
                _ => return Err("retained Web node shape changed".into()),
            }
            Ok(Prepared::Retained {
                node,
                path,
                old: Box::new(shallow(old)),
                next: Box::new(shallow(next)),
                old_children: old_nodes,
                removed,
                children: prepared,
            })
        }
        let prepared = prepare(
            &mut Context {
                frame: self,
                document: &document,
                budget: 0,
            },
            &self.plan,
            &next,
            "0",
            "0".into(),
            0,
        )
        .map_err(|error| (error, false))?;
        let active = document.active_element();
        let mut nodes = BTreeMap::new();
        prepared.collect(&mut nodes);
        if let Err(error) = prepared.apply() {
            return Err((error, prepared.restore().is_err()));
        }
        let restore_focus = active
            .filter(|active| {
                nodes
                    .values()
                    .any(|node| node.is_same_node(Some(active.as_ref())))
            })
            .filter(|active| {
                !document
                    .active_element()
                    .is_some_and(|current| current.is_same_node(Some(active.as_ref())))
            })
            .and_then(|element| element.dyn_into::<HtmlElement>().ok());
        if let Some(element) = restore_focus
            && let Err(error) = element.focus()
        {
            return Err((super::error(error), prepared.restore().is_err()));
        }
        self.nodes = nodes;
        self.plan = next;
        Ok(())
    }
}
