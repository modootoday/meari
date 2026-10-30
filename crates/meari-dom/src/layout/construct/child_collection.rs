use super::*;

pub(super) const DUMMY_NAME: QualName = qual_name!("div", html);

impl LayoutChildren {
    /// Append a single layout child.
    pub(super) fn push(&mut self, child_id: NodeId, doc: &mut BaseDocument) {
        self.maybe_push_anon_block(doc);
        self.children.push(child_id);
    }

    /// Append all layout children in `slice`.
    pub(super) fn extend(&mut self, slice: &[NodeId], doc: &mut BaseDocument) {
        self.maybe_push_anon_block(doc);
        self.children.extend_from_slice(slice);
    }

    pub(super) fn maybe_push_anon_block(&mut self, doc: &mut BaseDocument) {
        fn block_is_only_whitespace(doc: &BaseDocument, node_id: NodeId) -> bool {
            for child_id in doc.nodes[node_id].children.iter().copied() {
                let child = &doc.nodes[child_id];
                if !child.is_collapsible_whitespace_node() {
                    return false;
                }
            }

            true
        }

        // If anonymous block node only contains whitespace then delete it
        if let Some(anon_id) = self.anonymous_block_id {
            if block_is_only_whitespace(doc, anon_id) {
                // Remove by identity, not pop(): hoisted display:contents
                // children may have been pushed after the anon block.
                if let Some(pos) = self.children.iter().rposition(|id| *id == anon_id) {
                    self.children.remove(pos);
                }
                self.anonymous_blocks.retain(|id| *id != anon_id);
                doc.remove_node_from_tree(anon_id);
            }
        }

        self.anonymous_block_id = None;
    }

    pub(super) fn push_wrapped(
        &mut self,
        container_node_id: NodeId,
        child_id: NodeId,
        doc: &mut BaseDocument,
    ) {
        if self.anonymous_block_id.is_none() {
            self.create_anonymous_block(container_node_id, doc);
        }
        doc.nodes[self.anonymous_block_id.unwrap()]
            .children
            .push(child_id);
    }

    pub(super) fn create_anonymous_block(
        &mut self,
        container_node_id: NodeId,
        doc: &mut BaseDocument,
    ) {
        use style::selector_parser::PseudoElement;

        const NAME: QualName = QualName {
            prefix: None,
            ns: ns!(html),
            local: local_name!("div"),
        };
        let node_id = doc.create_node(NodeData::AnonymousBlock(Box::new(ElementData::new(
            NAME,
            Vec::new(),
        ))));

        // Set style data
        let parent_style = doc.nodes[container_node_id].primary_styles().unwrap();
        let read_guard = doc.guard.read();
        let guards = StylesheetGuards::same(&read_guard);
        let style = doc.stylist.style_for_anonymous::<&Node>(
            &guards,
            &PseudoElement::ServoAnonymousBox,
            &parent_style,
        );
        let mut stylo_element_data = StyloElementData {
            damage: ALL_DAMAGE,
            ..Default::default()
        };
        drop(parent_style);

        stylo_element_data.styles.primary = Some(style);
        stylo_element_data.set_restyled();

        *doc.nodes[node_id]
            .stylo_element_data_mut()
            .ensure_init_mut() = stylo_element_data;

        if doc.nodes[container_node_id]
            .flags
            .contains(NodeFlags::IS_IN_DOCUMENT)
        {
            doc.nodes[node_id].flags.insert(NodeFlags::IS_IN_DOCUMENT);
        }
        doc.nodes[node_id].parent = Some(container_node_id);
        doc.nodes[node_id]
            .layout_parent
            .set(Some(container_node_id));

        self.children.push(node_id);
        self.anonymous_block_id = Some(node_id);
        self.anonymous_blocks.push(node_id);
    }
}

pub(super) fn push_children_and_pseudos(layout_children: &mut ThinVec<NodeId>, node: &Node) {
    if let Some(before) = node.before() {
        layout_children.push(before);
    }
    layout_children.extend(node.children.iter().copied().filter(|child_id| {
        let child_node = node.with(*child_id);
        child_node.data.kind() != NodeKind::Comment
    }));
    if let Some(after) = node.after() {
        layout_children.push(after);
    }
}

pub(super) fn block_item_needs_wrap(
    child_node_kind: NodeKind,
    display_outside: DisplayOutside,
) -> bool {
    child_node_kind == NodeKind::Text || display_outside == DisplayOutside::Inline
}

/// Used for flex/grid containers, and for flow containers whose in-flow
/// children are all out-of-flow (where inline elements are pushed raw, but
/// bare text still cannot be laid out directly).
pub(super) fn text_item_needs_wrap(
    child_node_kind: NodeKind,
    _display_outside: DisplayOutside,
) -> bool {
    child_node_kind == NodeKind::Text
}

/// Push a single hoisted child, recursing through display:contents nodes and
/// wrapping text/inline children per the ancestor container's `WrapContext`.
pub(super) fn push_hoisted_child(
    doc: &mut BaseDocument,
    child_id: NodeId,
    out: &mut LayoutChildren,
    wrap: Option<WrapContext>,
) {
    let child = &doc.nodes[child_id];
    let child_display = child.display_style().unwrap_or(Display::inline());
    if matches!(child_display.inside(), DisplayInside::Contents) {
        collect_layout_children_with_wrap(doc, child_id, out, wrap);
        return;
    }
    if let Some(wrap_ctx) = wrap {
        let child_node_kind = child.data.kind();
        let display_outside = if child.is_or_contains_block() {
            DisplayOutside::Block
        } else {
            child_display.outside()
        };
        if (wrap_ctx.needs_wrap)(child_node_kind, display_outside) {
            out.push_wrapped(wrap_ctx.container_node_id, child_id, doc);
            return;
        }
    }
    out.push(child_id, doc);
}

/// Push the container's children (and ::before/::after pseudos) as layout
/// children, hoisting transparently through display:contents nodes and
/// filtering out comments and whitespace.
pub(super) fn push_hoisted_children_and_pseudos(
    doc: &mut BaseDocument,
    container_node_id: NodeId,
    out: &mut LayoutChildren,
    wrap: Option<WrapContext>,
) {
    if let Some(before) = doc.nodes[container_node_id].before() {
        push_hoisted_child(doc, before, out, wrap);
    }
    // Take children array from node to avoid borrow checker issues.
    let children = std::mem::take(&mut doc.nodes[container_node_id].children);
    for child_id in children.iter().copied() {
        let child = &doc.nodes[child_id];
        if child.data.kind() == NodeKind::Comment || child.is_collapsible_whitespace_node() {
            continue;
        }
        push_hoisted_child(doc, child_id, out, wrap);
    }
    doc.nodes[container_node_id].children = children;
    if let Some(after) = doc.nodes[container_node_id].after() {
        push_hoisted_child(doc, after, out, wrap);
    }
}

pub(super) fn push_non_whitespace_children_and_pseudos(
    layout_children: &mut ThinVec<NodeId>,
    node: &Node,
) {
    if let Some(before) = node.before() {
        layout_children.push(before);
    }
    layout_children.extend(node.children.iter().copied().filter(|child_id| {
        let child_node = node.with(*child_id);
        !child_node.is_whitespace_node() && child_node.data.kind() != NodeKind::Comment
    }));
    if let Some(after) = node.after() {
        layout_children.push(after);
    }
}

impl Default for FlowClassification {
    fn default() -> Self {
        Self {
            all_block: true,
            all_inline: true,
            all_out_of_flow: true,
            has_contents: false,
        }
    }
}

/// Classify a flow container's children (including its ::before/::after
/// pseudo-elements) for inline-vs-block layout, recursing transparently
/// through display:contents nodes (whose children participate in the
/// container's formatting context).
pub(super) fn classify_flow_children(
    doc: &BaseDocument,
    container_node_id: NodeId,
    classification: &mut FlowClassification,
) {
    let node = &doc.nodes[container_node_id];
    // ::before/::after pseudos with display:contents are transparent for box
    // generation, so their children (e.g. generated text) participate in the
    // container's formatting context and vote in the classification. Pseudos
    // with any other display value generate their own box, which every
    // construction arm already pushes explicitly, so they cast no vote.
    let pseudo_ids = node
        .before()
        .into_iter()
        .chain(node.after())
        .filter(|pe_id| {
            let display = doc.nodes[*pe_id]
                .display_style()
                .unwrap_or(Display::inline());
            matches!(display.inside(), DisplayInside::Contents)
        });
    let child_ids = node.children.iter().copied().chain(pseudo_ids);
    for child_id in child_ids {
        let child = &doc.nodes[child_id];

        // Comment nodes generate no boxes and must not affect the
        // inline-vs-block classification: an unstyled comment would
        // default to display:inline below and force an inline
        // formatting context on the container, swallowing element
        // siblings into the inline layout (zero-sizing any
        // out-of-flow ones).
        if child.data.kind() == NodeKind::Comment {
            continue;
        }

        // Unwraps on Text and SVG nodes
        let style = child.primary_styles();
        let style = style.as_ref();
        let display = child.display_style().unwrap_or(Display::inline());
        if matches!(display.inside(), DisplayInside::Contents) {
            // Transparent for box generation: the contents node casts
            // no vote itself — its children decide.
            classification.has_contents = true;
            classify_flow_children(doc, child_id, classification);
        } else if matches!(display.inside(), DisplayInside::None) {
            // display:none children generate no boxes and cast no vote.
            continue;
        } else {
            let position = style
                .map(|s| s.clone_position())
                .unwrap_or(PositionProperty::Static);
            let float = style.map(|s| s.clone_float()).unwrap_or(Float::None);

            // Ignore nodes whose whitespace is entirely collapsed away
            if child.is_collapsible_whitespace_node() {
                continue;
            }

            // display:none children generate no boxes and cast no vote
            if matches!(display.outside(), DisplayOutside::None) {
                continue;
            }

            let is_in_flow = matches!(
                position,
                PositionProperty::Static | PositionProperty::Relative | PositionProperty::Sticky
            ) && matches!(float, Float::None);

            if !is_in_flow {
                continue;
            }

            classification.all_out_of_flow = false;
            match display.outside() {
                DisplayOutside::None => {}
                DisplayOutside::Block
                | DisplayOutside::TableCaption
                | DisplayOutside::InternalTable => classification.all_inline = false,
                DisplayOutside::Inline => {
                    classification.all_block = false;

                    // We need the "complex" tree fixing when an inline contains a block
                    if child.is_or_contains_block() {
                        classification.all_inline = false;
                    }
                }
            }
        }
    }
}

pub(crate) fn collect_layout_children(
    doc: &mut BaseDocument,
    container_node_id: NodeId,
    out: &mut LayoutChildren,
) {
    collect_layout_children_with_wrap(doc, container_node_id, out, None)
}
