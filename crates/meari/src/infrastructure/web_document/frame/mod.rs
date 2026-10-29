use crate::application::dom::DomNode;
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, Node};
mod keyed;

pub(super) fn error(value: wasm_bindgen::JsValue) -> String {
    format!("Web DOM operation failed: {value:?}")
}

pub(super) struct Frame {
    pub plan: DomNode,
    pub root: Node,
    pub nodes: BTreeMap<String, Node>,
}

impl Frame {
    pub fn sync_text_controls(
        &self,
        paths: &std::collections::BTreeSet<String>,
    ) -> Result<(), String> {
        for path in paths {
            let mut plan = &self.plan;
            for index in path.split('/').skip(1) {
                let index: usize = index.parse().map_err(|_| "invalid Web control path")?;
                plan = match plan {
                    DomNode::Element { children, .. } => {
                        children.get(index).ok_or("Web control plan missing")?
                    }
                    _ => return Err("Web control path crosses text".into()),
                };
            }
            let value = match plan {
                DomNode::Element { attributes, .. } => attributes
                    .get("value")
                    .map(String::as_str)
                    .unwrap_or_default(),
                _ => return Err("Web callback control is not an element".into()),
            };
            let node = self.nodes.get(path).ok_or("Web callback control missing")?;
            if let Some(input) = node.dyn_ref::<web_sys::HtmlInputElement>() {
                input.set_value(value);
            } else if let Some(input) = node.dyn_ref::<web_sys::HtmlTextAreaElement>() {
                input.set_value(value);
            } else {
                return Err("Web callback control kind changed".into());
            }
        }
        if let Some(parent) = self
            .root
            .parent_node()
            .and_then(|node| node.dyn_into::<Element>().ok())
        {
            super::range_parts::sync_frame(self, &parent)?;
        }
        Ok(())
    }
    pub fn build(document: &Document, plan: DomNode) -> Result<Self, String> {
        fn create(
            document: &Document,
            plan: &DomNode,
            path: String,
            nodes: &mut BTreeMap<String, Node>,
        ) -> Result<Node, String> {
            let node: Node = match plan {
                DomNode::Text(value) => document.create_text_node(value).into(),
                DomNode::Element {
                    tag,
                    attributes,
                    children,
                } => {
                    let element = document.create_element(tag).map_err(error)?;
                    for (name, value) in attributes {
                        element.set_attribute(name, value).map_err(error)?;
                    }
                    for (index, child) in children.iter().enumerate() {
                        element
                            .append_child(&create(
                                document,
                                child,
                                format!("{path}/{index}"),
                                nodes,
                            )?)
                            .map_err(error)?;
                    }
                    element.into()
                }
            };
            nodes.insert(path, node.clone());
            Ok(node)
        }
        let mut nodes = BTreeMap::new();
        let root = create(document, &plan, "0".into(), &mut nodes)?;
        Ok(Self { plan, root, nodes })
    }

    pub(super) fn preflight(&self, next: &DomNode, parent: &Element) -> Result<(), String> {
        fn visit(frame: &Frame, old: &DomNode, new: &DomNode, path: &str) -> Result<(), String> {
            let node = frame.nodes.get(path).ok_or("missing Web node identity")?;
            match (old, new) {
                (DomNode::Text(value), DomNode::Text(_))
                    if node.node_type() == Node::TEXT_NODE
                        && node.node_value().as_ref() == Some(value) => {}
                (
                    DomNode::Element {
                        tag,
                        attributes,
                        children,
                    },
                    DomNode::Element {
                        tag: next_tag,
                        children: next_children,
                        ..
                    },
                ) if tag == next_tag && children.len() == next_children.len() => {
                    let element = node
                        .dyn_ref::<Element>()
                        .ok_or("Web element identity changed")?;
                    let live_disclosure = tag == "details";
                    let observed_count = element
                        .get_attribute_names()
                        .iter()
                        .filter(|name| {
                            !live_disclosure || name.as_string().as_deref() != Some("open")
                        })
                        .count();
                    let expected_count = attributes
                        .keys()
                        .filter(|name| !live_disclosure || name.as_str() != "open")
                        .count();
                    if element.local_name() != *tag
                        || observed_count != expected_count
                        || attributes.iter().any(|(name, value)| {
                            !(live_disclosure && name == "open")
                                && element.get_attribute(name).as_ref() != Some(value)
                        })
                        || node.child_nodes().length() as usize != children.len()
                    {
                        return Err("Web DOM was modified outside its frame".into());
                    }
                    for (index, (child, next)) in children.iter().zip(next_children).enumerate() {
                        let child_path = format!("{path}/{index}");
                        let observed = node
                            .child_nodes()
                            .item(index as u32)
                            .ok_or("Web child disappeared")?;
                        if !observed.is_same_node(frame.nodes.get(&child_path)) {
                            return Err("Web child identity changed".into());
                        }
                        visit(frame, child, next, &child_path)?;
                    }
                }
                _ => {
                    return Err(
                        "Web patch requires unchanged node shape and an intact frame".into(),
                    );
                }
            }
            Ok(())
        }
        if !self
            .root
            .parent_node()
            .is_some_and(|node| node.is_same_node(Some(parent.as_ref())))
        {
            return Err("Web root was detached or replaced".into());
        }
        visit(self, &self.plan, next, "0")
    }

    fn apply(&self, previous: &DomNode, next: &DomNode, path: &str) -> Result<(), String> {
        let node = self.nodes.get(path).ok_or("missing Web patch node")?;
        match (previous, next) {
            (DomNode::Text(_), DomNode::Text(value)) => node.set_node_value(Some(value)),
            (
                DomNode::Element {
                    attributes: old,
                    children: old_children,
                    ..
                },
                DomNode::Element {
                    attributes,
                    children,
                    ..
                },
            ) => {
                let element = node
                    .dyn_ref::<Element>()
                    .ok_or("Web patch element changed")?;
                for name in old.keys().filter(|name| !attributes.contains_key(*name)) {
                    element.remove_attribute(name).map_err(error)?;
                }
                for (name, value) in attributes {
                    if old.get(name) != Some(value) {
                        element.set_attribute(name, value).map_err(error)?;
                    }
                }
                for (index, (old, new)) in old_children.iter().zip(children).enumerate() {
                    self.apply(old, new, &format!("{path}/{index}"))?;
                }
            }
            _ => return Err("Web patch shape changed".into()),
        }
        Ok(())
    }

    pub fn patch(&mut self, mut next: DomNode, parent: &Element) -> Result<(), (String, bool)> {
        let focused = parent
            .owner_document()
            .and_then(|document| document.active_element())
            .and_then(|active| {
                self.nodes
                    .iter()
                    .find(|(_, node)| node.is_same_node(Some(active.as_ref())))
                    .map(|(path, _)| path.clone())
            });
        crate::application::radio_group::plan::retain(&self.plan, &mut next, focused.as_deref())
            .map_err(|error| (error, false))?;
        self.patch_interaction(next, parent)?;
        super::range_parts::sync_frame(self, parent).map_err(|error| (error, true))
    }

    pub fn patch_interaction(
        &mut self,
        next: DomNode,
        parent: &Element,
    ) -> Result<(), (String, bool)> {
        if keyed::contains(&self.plan) || keyed::contains(&next) {
            return self.patch_keyed(next, parent);
        }
        self.preflight(&next, parent)
            .map_err(|error| (error, false))?;
        if let Err(error) = self.apply(&self.plan, &next, "0") {
            let failed = self.apply(&next, &self.plan, "0").is_err();
            return Err((error, failed));
        }
        self.plan = next;
        Ok(())
    }
}
