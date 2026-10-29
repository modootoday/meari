//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::{ComponentReference, ModuleDiagnostic};
use super::*;
use crate::domain::node_slots::{NodeSlot, SlotReference};
use crate::{
    application::state_program::StateProjection,
    domain::{
        Node, NodeKind, Span, Value, build::ContentHash, source_modules::SourceComponentModule,
    },
};
use std::{collections::BTreeMap, rc::Rc};

impl ProjectionContext<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn node(
        &mut self,
        mut node: Node,
        module: &Rc<SourceComponentModule>,
        reference: &ComponentReference,
        instance: &str,
        local: &str,
        placement: &str,
        output: &str,
        projection: &Rc<StateProjection>,
        depth: usize,
    ) -> Result<Vec<Node>, ModuleDiagnostic> {
        self.nodes += 1;
        if self.nodes > 100000 || depth > 128 {
            return Err(failure(
                &reference.module,
                node.span,
                "module projection node/depth budget exceeded",
            ));
        }
        if let NodeKind::Text(Value::NodeSlot(slot)) = &node.kind {
            match slot {
                NodeSlot::Empty => return Ok(Vec::new()),
                NodeSlot::Content(nodes) => {
                    let captured = CapturedSlot {
                        nodes: nodes
                            .iter()
                            .enumerate()
                            .map(|(index, node)| (node.clone(), format!("{local}/slot/{index}")))
                            .collect(),
                        module: module.clone(),
                        reference: reference.clone(),
                        instance: instance.into(),
                        projection: projection.clone(),
                    };
                    return self.expand_slot(captured, placement, output, depth + 1);
                }
                NodeSlot::Captured(slot) => {
                    if slot.owner != self.slot_owner {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "foreign node slot owner",
                        ));
                    }
                    let captured = self.slots.remove(slot).ok_or_else(|| {
                        failure(
                            &reference.module,
                            node.span,
                            "stale or multiply consumed node slot",
                        )
                    })?;
                    return self.expand_slot(captured, placement, output, depth + 1);
                }
                NodeSlot::Owned(slot) => {
                    if slot.owner != self.slot_owner
                        || (!self.active.contains(&slot.nonce)
                            && slot.generation != self.generation)
                    {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "foreign or retired node slot lease",
                        ));
                    }
                    if self.owned.len() >= 16384 {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "active node slot budget exceeded",
                        ));
                    }
                    if !self.owned.insert(slot.nonce) {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "owned node slot cannot be inserted more than once",
                        ));
                    }
                    let key = ContentHash::of(slot.key.as_bytes());
                    let parent = self.render_stack.last().cloned().ok_or_else(|| {
                        failure(&reference.module, node.span, "slot render parent missing")
                    })?;
                    let child = if slot.movable {
                        let container = placement.rsplit_once('/').map_or("", |(parent, _)| parent);
                        let child = format!("{parent}@movable-slot:{container}/{}", key.as_str());
                        if !self.movable.insert(child.clone()) {
                            return Err(failure(
                                &reference.module,
                                node.span,
                                "movable slot key is duplicated within its source parent",
                            ));
                        }
                        child
                    } else {
                        format!("{parent}@slot:{placement}/{}", key.as_str())
                    };
                    self.calls.insert(
                        child.clone(),
                        ModuleOrigin {
                            module: reference.module.clone(),
                            instance: instance.into(),
                            source_path: local.into(),
                            span: node.span,
                        },
                    );
                    let mut rendered = self.component(
                        &slot.reference,
                        &slot.props,
                        &child,
                        output,
                        Some(&parent),
                        depth + 1,
                    )?;
                    if let NodeKind::Element { attributes, .. } = &mut rendered.kind {
                        if attributes.iter().any(|attribute| {
                            attribute.name == crate::application::repeat::IDENTITY_ATTRIBUTE
                        }) {
                            return Err(failure(
                                &reference.module,
                                node.span,
                                "owned node root identity attribute is reserved",
                            ));
                        }
                        let identity = ContentHash::of(
                            &serde_json::to_vec(&(&child, &slot.reference)).map_err(|error| {
                                failure(&reference.module, node.span, error.to_string())
                            })?,
                        );
                        attributes.push(crate::domain::Attribute {
                            name: crate::application::repeat::IDENTITY_ATTRIBUTE.into(),
                            value: Value::String(identity.as_str().into()),
                        });
                    }
                    return Ok(vec![rendered]);
                }
            }
        }
        let origin = ModuleOrigin {
            module: reference.module.clone(),
            instance: instance.into(),
            source_path: local.into(),
            span: node.span,
        };
        if let NodeKind::Element {
            tag,
            attributes,
            children,
        } = &mut node.kind
        {
            if let Some(import) = module.imports.get(tag) {
                if projection.events.contains_key(local) || projection.inputs.contains_key(local) {
                    return Err(failure(
                        &reference.module,
                        node.span,
                        "source component callback props require a typed callback contract",
                    ));
                }
                let mut props: BTreeMap<String, Value> = attributes
                    .iter()
                    .filter(|attribute| {
                        attribute.name != crate::application::repeat::IDENTITY_ATTRIBUTE
                    })
                    .map(|attribute| (attribute.name.clone(), attribute.value.clone()))
                    .collect();
                for (name, value) in &mut props {
                    if let Value::NodeSlot(NodeSlot::Content(nodes)) = value {
                        let nodes = nodes
                            .iter()
                            .enumerate()
                            .map(|(index, node)| {
                                (node.clone(), format!("{local}/slot:{name}/{index}"))
                            })
                            .collect();
                        *value = self.capture_slot(
                            format!("{instance}/{local}:{name}"),
                            nodes,
                            module,
                            reference,
                            instance,
                            projection,
                        )?;
                    }
                }
                if !children.is_empty() {
                    if props.contains_key("children") {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "children supplied both as attribute and nested nodes",
                        ));
                    }
                    let definition = self
                        .graph
                        .definition(&import.target)
                        .map_err(|error| failure(&reference.module, node.span, error))?;
                    if !definition.component.props.iter().any(|prop| {
                        prop.name == "children" && prop.kind == crate::domain::PropType::Node
                    }) {
                        return Err(failure(
                            &reference.module,
                            node.span,
                            "source component children require a typed slot contract",
                        ));
                    }
                    let nodes = children
                        .iter()
                        .enumerate()
                        .map(|(index, node)| (node.clone(), format!("{local}/{index}")))
                        .collect();
                    let value = self.capture_slot(
                        format!("{instance}/{local}:children"),
                        nodes,
                        module,
                        reference,
                        instance,
                        projection,
                    )?;
                    props.insert("children".into(), value);
                }
                let stable = attributes.iter().find_map(|attribute| {
                    if attribute.name == crate::application::repeat::IDENTITY_ATTRIBUTE
                        && let Value::String(identity) = &attribute.value
                    {
                        return Some(identity.as_str());
                    }
                    None
                });
                let parent = self.render_stack.last().cloned().ok_or_else(|| {
                    failure(
                        &reference.module,
                        node.span,
                        "component render parent missing",
                    )
                })?;
                let child = format!("{parent}@{}", stable.unwrap_or(placement));
                self.calls.insert(child.clone(), origin);
                let mut result = self.component(
                    &import.target,
                    &props,
                    &child,
                    output,
                    Some(&parent),
                    depth + 1,
                )?;
                if let Some(stable) = stable
                    && let NodeKind::Element { attributes, .. } = &mut result.kind
                {
                    attributes.push(crate::domain::Attribute {
                        name: crate::application::repeat::IDENTITY_ATTRIBUTE.into(),
                        value: Value::String(stable.into()),
                    });
                }
                return Ok(vec![result]);
            }
            for (index, child) in std::mem::take(children).into_iter().enumerate() {
                let expanded = self.node(
                    child,
                    module,
                    reference,
                    instance,
                    &format!("{local}/{index}"),
                    &format!("{placement}/{index}"),
                    &format!("{output}/{}", children.len()),
                    projection,
                    depth + 1,
                )?;
                children.extend(expanded);
            }
        }
        if let Some(event) = projection.events.get(local) {
            let handler = if let Some(pass) = &self.hooks {
                let MountedState::Hook(handle) = &self
                    .next
                    .get(instance)
                    .ok_or_else(|| {
                        failure(&reference.module, node.span, "hook event instance missing")
                    })?
                    .state
                else {
                    return Err(failure(
                        &reference.module,
                        node.span,
                        "hook event owner missing",
                    ));
                };
                Some(
                    pass.prepared
                        .handler(handle, event.clone())
                        .map_err(|diagnostic| ModuleDiagnostic {
                            file: reference.module.clone(),
                            diagnostic,
                        })?,
                )
            } else {
                None
            };
            self.events.insert(
                output.into(),
                EventBinding {
                    instance: instance.into(),
                    event: event.clone(),
                    handler,
                },
            );
        }
        if let Some(binding) = projection.inputs.get(local) {
            self.inputs.insert(
                output.into(),
                InputBinding {
                    instance: instance.into(),
                    binding: binding.clone(),
                },
            );
        }
        if let Some(binding) = projection.callbacks.get(local) {
            self.callbacks.insert(output.into(), binding.clone());
        }
        let commit_local =
            crate::application::host_callbacks::DocumentCallbacks::value_commit_key(local);
        if let Some(binding) = projection.callbacks.get(&commit_local) {
            self.callbacks.insert(
                crate::application::host_callbacks::DocumentCallbacks::value_commit_key(output),
                binding.clone(),
            );
        }
        self.origins.insert(output.into(), origin);
        Ok(vec![node])
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn capture_slot(
        &mut self,
        identity: String,
        nodes: Vec<(Node, String)>,
        module: &Rc<SourceComponentModule>,
        reference: &ComponentReference,
        instance: &str,
        projection: &Rc<StateProjection>,
    ) -> Result<Value, ModuleDiagnostic> {
        if self.slots.len() >= 16384 {
            return Err(failure(
                &reference.module,
                Span::default(),
                "node slot capture budget exceeded",
            ));
        }
        let slot = SlotReference {
            owner: self.slot_owner,
            identity,
        };
        let captured = CapturedSlot {
            nodes,
            module: module.clone(),
            reference: reference.clone(),
            instance: instance.into(),
            projection: projection.clone(),
        };
        if self.slots.insert(slot.clone(), captured).is_some() {
            return Err(failure(
                &reference.module,
                Span::default(),
                "duplicate node slot source identity",
            ));
        }
        Ok(Value::NodeSlot(NodeSlot::Captured(slot)))
    }

    pub(super) fn expand_slot(
        &mut self,
        captured: CapturedSlot,
        placement: &str,
        output: &str,
        depth: usize,
    ) -> Result<Vec<Node>, ModuleDiagnostic> {
        let mut expanded = Vec::new();
        for (index, (node, local)) in captured.nodes.into_iter().enumerate() {
            let path = if expanded.is_empty() {
                output.to_owned()
            } else {
                let (parent, index) = output.rsplit_once('/').ok_or_else(|| {
                    failure(
                        &captured.reference.module,
                        node.span,
                        "multiple slot roots require a containing element",
                    )
                })?;
                let index = index.parse::<usize>().map_err(|_| {
                    failure(
                        &captured.reference.module,
                        node.span,
                        "invalid slot placement",
                    )
                })?;
                format!("{parent}/{}", index + expanded.len())
            };
            expanded.extend(self.node(
                node,
                &captured.module,
                &captured.reference,
                &captured.instance,
                &local,
                &format!("{placement}/slot/{index}"),
                &path,
                &captured.projection,
                depth,
            )?);
        }
        Ok(expanded)
    }
}
