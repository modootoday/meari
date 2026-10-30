use super::*;

impl DocumentMutator<'_> {
    pub fn remove_and_drop_all_children(&mut self, node_id: NodeId) {
        let parent = &mut self.doc.nodes[node_id];
        let parent_is_in_doc = parent.flags.is_in_document();

        // TODO: make this fine grained / conditional based on ElementSelectorFlags
        if parent_is_in_doc {
            if let Some(mut data) = parent
                .try_stylo_element_data_mut()
                .and_then(|s| s.get_mut())
            {
                data.hint |= RestyleHint::restyle_subtree();
            }
            // Mark ancestors dirty so the style traversal visits this subtree.
            parent.mark_ancestors_dirty();
        }

        let children = mem::take(&mut parent.children);
        self.mutations_occurred |= parent_is_in_doc && !children.is_empty();
        for child_id in children {
            self.process_removed_subtree(child_id);
            let _ = self.doc.drop_node_ignoring_parent(child_id);
        }
        self.maybe_record_node(node_id);
    }

    // Tree mutation methods
    pub fn remove_node_if_unparented(&mut self, node_id: NodeId) {
        self.remove_node_if_unparented_with(node_id, &mut |_| {});
    }

    /// Like [`Self::remove_node_if_unparented`], but calls `on_drop` with the id of
    /// every dropped node (the node itself and all of its descendants).
    pub fn remove_node_if_unparented_with(
        &mut self,
        node_id: NodeId,
        on_drop: &mut dyn FnMut(NodeId),
    ) {
        if let Some(node) = self.doc.get_node(node_id) {
            if node.parent.is_none() {
                self.remove_and_drop_node_with(node_id, on_drop);
            }
        }
    }

    /// Remove all of the children from old_parent_id and append them to new_parent_id
    pub fn append_children(&mut self, parent_id: NodeId, child_ids: &[NodeId]) {
        self.add_children_to_parent(parent_id, child_ids, &|parent, child_ids| {
            parent.children.extend_from_slice(child_ids);
        });
    }

    pub fn insert_nodes_before(&mut self, anchor_node_id: NodeId, new_node_ids: &[NodeId]) {
        let parent_id = self.doc.nodes[anchor_node_id].parent.unwrap();
        self.add_children_to_parent(parent_id, new_node_ids, &|parent, child_ids| {
            let node_child_idx = parent.index_of_child(anchor_node_id).unwrap();
            parent
                .children
                .splice(node_child_idx..node_child_idx, child_ids.iter().copied());
        });
    }

    pub(super) fn add_children_to_parent(
        &mut self,
        parent_id: NodeId,
        child_ids: &[NodeId],
        insert_children_fn: &dyn Fn(&mut Node, &[NodeId]),
    ) {
        let new_parent_is_in_document = self.doc.nodes[parent_id].flags.is_in_document();
        self.mutations_occurred |= new_parent_is_in_document && !child_ids.is_empty();
        // Detach the children from their old parents *before* inserting them into
        // the new parent (matching DOM `insertBefore` semantics). If a child is
        // being moved within the same parent then detaching it after insertion
        // would remove both the old and the newly-inserted entries from the
        // parent's child list, and anchor indices would be computed against a
        // child list that still contains the moved nodes.
        for child_id in child_ids.iter().copied() {
            let child = &mut self.doc.nodes[child_id];
            let child_was_in_doc = child.flags.is_in_document();
            self.mutations_occurred |= child_was_in_doc;
            let Some(old_parent_id) = child.parent.take() else {
                continue;
            };

            let old_parent = &mut self.doc.nodes[old_parent_id];
            old_parent.insert_damage(ALL_DAMAGE);

            // TODO: make this fine grained / conditional based on ElementSelectorFlags
            if child_was_in_doc {
                if let Some(mut data) = old_parent
                    .try_stylo_element_data_mut()
                    .and_then(|s| s.get_mut())
                {
                    data.hint |= RestyleHint::restyle_subtree();
                }
                // Mark ancestors dirty so the style traversal visits this subtree.
                old_parent.mark_ancestors_dirty();
            }

            old_parent.children.retain(|id| *id != child_id);
            self.maybe_record_node(old_parent_id);
        }

        let new_parent = &mut self.doc.nodes[parent_id];
        new_parent.insert_damage(ALL_DAMAGE);

        // TODO: make this fine grained / conditional based on ElementSelectorFlags
        if new_parent_is_in_document {
            if let Some(mut data) = new_parent
                .try_stylo_element_data_mut()
                .and_then(|s| s.get_mut())
            {
                data.hint |= RestyleHint::restyle_subtree();
            }
            // Mark ancestors dirty so the style traversal visits this subtree.
            new_parent.mark_ancestors_dirty();
        }

        insert_children_fn(new_parent, child_ids);

        for child_id in child_ids.iter().copied() {
            let child = &mut self.doc.nodes[child_id];
            let child_was_in_doc = child.flags.is_in_document();
            child.parent = Some(parent_id);

            if new_parent_is_in_document && !child_was_in_doc {
                self.process_added_subtree(child_id);
            } else if !new_parent_is_in_document && child_was_in_doc {
                self.process_removed_subtree(child_id);
            }
        }

        self.maybe_record_node(parent_id);
    }

    // Tree mutation methods (that defer to other methods)
    pub fn insert_nodes_after(&mut self, anchor_node_id: NodeId, new_node_ids: &[NodeId]) {
        match self.next_sibling_id(anchor_node_id) {
            Some(id) => self.insert_nodes_before(id, new_node_ids),
            None => {
                let parent_id = self.parent_id(anchor_node_id).unwrap();
                self.append_children(parent_id, new_node_ids)
            }
        }
    }

    pub fn reparent_children(&mut self, old_parent_id: NodeId, new_parent_id: NodeId) {
        let child_ids = std::mem::take(&mut self.doc.nodes[old_parent_id].children);
        self.maybe_record_node(old_parent_id);
        self.append_children(new_parent_id, &child_ids);
    }

    pub fn replace_node_with(&mut self, anchor_node_id: NodeId, new_node_ids: &[NodeId]) {
        self.insert_nodes_before(anchor_node_id, new_node_ids);
        self.remove_node(anchor_node_id);
    }

    // === ParentNode / ChildNode mixin mutation methods ===
    //
    // These implement the DOM spec's mutation semantics for the ParentNode and
    // ChildNode mixins (`append`/`prepend`/`replaceChildren` and
    // `before`/`after`/`replaceWith`). Callers pass lists of node ids with
    // string arguments already converted to (detached) text nodes and
    // DocumentFragment arguments already expanded into their children (per the
    // DOM spec's "insert a node" steps).

    /// Detach (rather than drop) any already-parented nodes, so that references
    /// to them (and their descendants) remain valid
    pub(super) fn detach_all(&mut self, node_ids: &[NodeId]) {
        for node_id in node_ids {
            if self.node_has_parent(*node_id) {
                self.remove_node(*node_id);
            }
        }
    }

    /// The nearest sibling of `anchor_id` (in the direction given by `offset`:
    /// +1 = following, -1 = preceding) which is not in `excluded` — the spec's
    /// "viable next/previous sibling" for ChildNode mutation methods
    pub(super) fn viable_sibling(
        &self,
        anchor_id: NodeId,
        offset: isize,
        excluded: &[NodeId],
    ) -> Option<NodeId> {
        let node = self.doc.get_node(anchor_id)?;
        let parent = self.doc.get_node(node.parent?)?;
        let mut index = parent.index_of_child(anchor_id)?;
        loop {
            index = index.checked_add_signed(offset)?;
            let sibling_id = *parent.children.get(index)?;
            if !excluded.contains(&sibling_id) {
                return Some(sibling_id);
            }
        }
    }

    /// ParentNode's `prepend()`: insert the nodes at the start of `parent_id`'s
    /// children (ParentNode's `append()` is [`append_children`](Self::append_children))
    pub fn prepend_nodes(&mut self, parent_id: NodeId, node_ids: &[NodeId]) {
        self.detach_all(node_ids);
        match self.child_ids(parent_id).first().copied() {
            Some(first_child_id) => self.insert_nodes_before(first_child_id, node_ids),
            None => self.append_children(parent_id, node_ids),
        }
    }

    /// ParentNode's `replaceChildren()`: replace all of `parent_id`'s children
    /// with the given nodes. The existing children are detached rather than
    /// dropped, so that references to them remain valid.
    pub fn replace_children(&mut self, parent_id: NodeId, node_ids: &[NodeId]) {
        self.detach_all(node_ids);
        for child_id in self.child_ids(parent_id) {
            self.remove_node(child_id);
        }
        self.append_children(parent_id, node_ids);
    }

    /// ChildNode's `before()`: insert the nodes before `anchor_id`. The
    /// insertion point is after the nearest preceding sibling which isn't
    /// itself being inserted (or at the start of the parent if there is none).
    /// Does nothing if the anchor has no parent.
    pub fn before_node(&mut self, anchor_id: NodeId, node_ids: &[NodeId]) {
        let Some(parent_id) = self.parent_id(anchor_id) else {
            return;
        };
        let viable_prev = self.viable_sibling(anchor_id, -1, node_ids);
        self.detach_all(node_ids);
        match viable_prev {
            Some(prev_id) => self.insert_nodes_after(prev_id, node_ids),
            None => match self.child_ids(parent_id).first().copied() {
                Some(first_child_id) => self.insert_nodes_before(first_child_id, node_ids),
                None => self.append_children(parent_id, node_ids),
            },
        }
    }

    /// ChildNode's `after()`: insert the nodes after `anchor_id`. The insertion
    /// point is before the nearest following sibling which isn't itself being
    /// inserted (or at the end of the parent if there is none). Does nothing if
    /// the anchor has no parent.
    pub fn after_node(&mut self, anchor_id: NodeId, node_ids: &[NodeId]) {
        let Some(parent_id) = self.parent_id(anchor_id) else {
            return;
        };
        let viable_next = self.viable_sibling(anchor_id, 1, node_ids);
        self.detach_all(node_ids);
        match viable_next {
            Some(next_id) => self.insert_nodes_before(next_id, node_ids),
            None => self.append_children(parent_id, node_ids),
        }
    }

    /// ChildNode's `replaceWith()`: replace `anchor_id` with the nodes in its
    /// parent's child list. If the anchor is itself one of the inserted nodes,
    /// they are inserted at its old position (the spec's "viable next sibling"
    /// handling). The anchor is detached rather than dropped, so references to
    /// it remain valid. Does nothing if the anchor has no parent.
    pub fn replace_with_nodes(&mut self, anchor_id: NodeId, node_ids: &[NodeId]) {
        let Some(parent_id) = self.parent_id(anchor_id) else {
            return;
        };
        let viable_next = self.viable_sibling(anchor_id, 1, node_ids);
        self.detach_all(node_ids);
        if self.node_has_parent(anchor_id) {
            self.replace_node_with(anchor_id, node_ids);
        } else {
            match viable_next {
                Some(next_id) => self.insert_nodes_before(next_id, node_ids),
                None => self.append_children(parent_id, node_ids),
            }
        }
    }
}

impl<'doc> DocumentMutator<'doc> {
    pub fn flush(&mut self) {
        if self.recompute_is_animating {
            self.doc.has_canvas = self.doc.compute_has_canvas();
        }

        if let Some(id) = self.title_node {
            let title = self.doc.nodes[id].text_content();
            self.doc.shell_provider.set_window_title(title);
        }

        // Add/Update inline stylesheets (<style> elements)
        for id in self.style_nodes.drain() {
            self.doc.process_style_element(id);
        }

        for id in self.form_nodes.drain() {
            self.doc.reset_form_owner(id);
        }

        #[cfg(feature = "autofocus")]
        if let Some(node_id) = self.node_to_autofocus.take() {
            if self.doc.get_node(node_id).is_some() {
                self.doc.set_focus_to(node_id);
            }
        }
    }

    pub fn set_inner_html(&mut self, node_id: NodeId, html: &str) {
        self.remove_and_drop_all_children(node_id);
        self.doc
            .html_parser_provider
            .clone()
            .parse_inner_html(self, node_id, html);
    }

    pub(super) fn flush_eager_ops(&mut self) {
        let mut ops = mem::take(&mut self.eager_op_queue);
        for op in ops.drain(0..) {
            match op {
                SpecialOp::LoadImage(node_id) => self.load_image(node_id),
                SpecialOp::LoadIframe(node_id) => self.load_iframe(node_id),
                SpecialOp::LoadStylesheet(node_id) => self.load_linked_stylesheet(node_id),
                SpecialOp::UnloadStylesheet(node_id) => self.unload_stylesheet(node_id),
                SpecialOp::LoadCustomPaintSource(node_id) => self.load_custom_paint_src(node_id),
                SpecialOp::ProcessButtonInput(node_id) => self.process_button_input(node_id),
                SpecialOp::UnloadSubDocument(node_id) => self.remove_sub_document(node_id),
                #[cfg(feature = "custom-widget")]
                SpecialOp::UnloadCustomWidget(node_id) => self.remove_custom_widget(node_id),
            }
        }

        // Queue is empty, but put Vec back anyway so allocation can be reused.
        self.eager_op_queue = ops;
    }

    pub(super) fn process_added_subtree(&mut self, node_id: NodeId) {
        self.doc.iter_subtree_mut(node_id, |node_id, doc| {
            let node = &mut doc.nodes[node_id];
            node.flags.set(NodeFlags::IS_IN_DOCUMENT, true);
            node.insert_damage(ALL_DAMAGE);

            // If the node has an "id" attribute, store it in the ID map.
            if let Some(id_attr) = node.attr(local_name!("id")).map(ToString::to_string) {
                doc.add_to_id_map(&id_attr, node_id);
            }

            let node = &mut doc.nodes[node_id];
            let NodeData::Element(ref mut element) = node.data else {
                return;
            };

            // Custom widgets replace the element's built-in tag-specific behaviour
            #[cfg(feature = "custom-widget")]
            let has_custom_widget = element.custom_widget_data().is_some();
            #[cfg(not(feature = "custom-widget"))]
            let has_custom_widget = false;

            // Custom post-processing by element tag name
            let tag = element.name.local.as_ref();
            match tag {
                "title" => self.title_node = Some(node_id),
                "style" => {
                    self.style_nodes.insert(node_id);
                }
                _ if has_custom_widget => {}
                "link" => self.eager_op_queue.push(SpecialOp::LoadStylesheet(node_id)),
                "img" => self.eager_op_queue.push(SpecialOp::LoadImage(node_id)),
                "iframe" => self.eager_op_queue.push(SpecialOp::LoadIframe(node_id)),
                "canvas" => self
                    .eager_op_queue
                    .push(SpecialOp::LoadCustomPaintSource(node_id)),
                "button" | "fieldset" | "input" | "select" | "textarea" | "object" | "output" => {
                    self.eager_op_queue
                        .push(SpecialOp::ProcessButtonInput(node_id));
                    self.form_nodes.insert(node_id);
                }
                _ => {}
            }

            #[cfg(feature = "autofocus")]
            if node.is_focussable() {
                if let NodeData::Element(ref element) = node.data {
                    // `autofocus` is a boolean attribute, so presence alone enables it
                    // (`<input autofocus>`). "false" is still ignored, because Dioxus
                    // writes `autofocus: false` as the string "false".
                    if let Some(value) = element.attr(local_name!("autofocus")) {
                        if value != "false" {
                            self.node_to_autofocus = Some(node_id);
                        }
                    }
                }
            }
        });

        self.flush_eager_ops();
    }

    pub(super) fn process_removed_subtree(&mut self, node_id: NodeId) {
        self.doc.iter_subtree_mut(node_id, |node_id, doc| {
            doc.nodes[node_id]
                .flags
                .set(NodeFlags::IS_IN_DOCUMENT, false);

            // Clear any interaction state that references this node, running
            // the usual teardown steps (unhover/unactive the surviving
            // ancestor chain, IME disable on blur of a focused input).
            doc.clear_interaction_state_for_removed_node(node_id);

            let node = &mut doc.nodes[node_id];

            // Clear the text selection if one of its endpoints references this node.
            // This prevents stale selection endpoint references.
            if doc.text_selection.anchor.node_or_parent == Some(node_id)
                || doc.text_selection.focus.node_or_parent == Some(node_id)
            {
                doc.text_selection.clear();
            }

            // Remove any snapshot for this node to prevent stale snapshot references
            // during style invalidation.
            if node.has_snapshot() {
                let opaque_id = style::dom::TNode::opaque(&&*node);
                doc.snapshots.remove(&opaque_id);
                node.set_has_snapshot(false);
            }

            // If the node has an "id" attribute remove it from the ID map.
            if let Some(id_attr) = node.attr(local_name!("id")).map(ToString::to_string) {
                doc.remove_from_id_map(&id_attr, node_id);
            }

            let node = &mut doc.nodes[node_id];
            let NodeData::Element(ref mut element) = node.data else {
                return;
            };

            match &element.special_data {
                SpecialElementData::SubDocument(_) => {
                    self.eager_op_queue
                        .push(SpecialOp::UnloadSubDocument(node_id));
                }
                #[cfg(feature = "custom-widget")]
                SpecialElementData::CustomWidget(_) => {
                    self.eager_op_queue
                        .push(SpecialOp::UnloadCustomWidget(node_id));
                }
                SpecialElementData::Stylesheet(_) => self
                    .eager_op_queue
                    .push(SpecialOp::UnloadStylesheet(node_id)),
                SpecialElementData::Image(_) => {}
                SpecialElementData::Canvas(_) => {
                    self.recompute_is_animating = true;
                }
                SpecialElementData::TableRoot(_) => {}
                SpecialElementData::TextInput(_) => {}
                SpecialElementData::CheckboxInput(_) => {}
                #[cfg(feature = "file-input")]
                SpecialElementData::FileInput(_) => {}
                SpecialElementData::None => {}
            }
        });

        self.flush_eager_ops();
    }

    pub(super) fn maybe_record_node(&mut self, node_id: impl Into<Option<NodeId>>) {
        let Some(node_id) = node_id.into() else {
            return;
        };

        let Some(tag_name) = self.doc.nodes[node_id]
            .data
            .downcast_element()
            .map(|elem| &elem.name.local)
        else {
            return;
        };

        match tag_name.as_ref() {
            "title" => self.title_node = Some(node_id),
            "style" => {
                self.style_nodes.insert(node_id);
            }
            _ => {}
        }
    }

    pub(super) fn load_linked_stylesheet(&mut self, target_id: NodeId) {
        let node = &self.doc.nodes[target_id];

        let mut is_in_head = false;
        let mut parent_id = node.parent;
        while let Some(id) = parent_id
            && !is_in_head
        {
            let parent = &self.doc.nodes[id];
            is_in_head |= parent.data.is_element_with_tag_name(&local_name!("head"));
            parent_id = parent.parent;
        }

        let rel_attr = node.attr(local_name!("rel"));
        let href_attr = node.attr(local_name!("href"));

        let (Some(rels), Some(href)) = (rel_attr, href_attr) else {
            return;
        };
        if !rels.split_ascii_whitespace().any(|rel| rel == "stylesheet") {
            return;
        }

        let url = self.doc.resolve_url(href);
        let handler = ResourceHandler::new(
            self.doc.tx.clone(),
            self.doc.id(),
            Some(node.id),
            self.doc.shell_provider.clone(),
            StylesheetHandler {
                source_url: url.clone(),
                guard: self.doc.guard.clone(),
                net_provider: self.doc.net_provider.clone(),
                abort_signal: self.doc.abort_signal.clone(),
            },
        );

        if is_in_head && !self.doc.net_provider.is_noop() {
            self.doc
                .pending_critical_resources
                .insert(handler.request_id());
        }

        self.doc.net_provider.fetch(
            self.doc.id(),
            self.doc.build_request(url),
            Box::new(handler),
        );
    }
}
