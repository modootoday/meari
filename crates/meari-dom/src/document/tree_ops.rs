use super::*;

impl BaseDocument {
    #[cfg(feature = "custom-widget")]
    pub fn set_custom_widget(&mut self, node_id: NodeId, widget: Box<dyn crate::Widget>) {
        self.nodes[node_id]
            .element_data_mut()
            .unwrap()
            .set_custom_widget(widget);
        self.custom_widget_nodes.insert(node_id);
    }

    #[cfg(feature = "custom-widget")]
    pub fn remove_custom_widget(&mut self, node_id: NodeId) {
        let resources_to_deallocate = self.nodes[node_id]
            .element_data_mut()
            .unwrap()
            .remove_custom_widget();
        self.pending_resource_deallocations
            .extend_from_slice(&resources_to_deallocate);
        self.custom_widget_nodes.remove(&node_id);
    }

    pub fn root_node(&self) -> &Node {
        &self.nodes[self.root_node_id]
    }

    pub fn root_node_mut(&mut self) -> &mut Node {
        &mut self.nodes[self.root_node_id]
    }

    pub fn try_root_element(&self) -> Option<&Node> {
        TDocument::as_node(&self.root_node()).first_element_child()
    }

    pub fn root_element(&self) -> &Node {
        TDocument::as_node(&self.root_node())
            .first_element_child()
            .unwrap()
            .as_element()
            .unwrap()
    }

    pub fn create_node(&mut self, node_data: NodeData) -> NodeId {
        let tree_ptr = self.nodes.as_mut() as *mut NodeTree;
        let guard = self.guard.clone();

        let id = self
            .nodes
            .insert_with_key(|id| Node::new(tree_ptr, id, guard, node_data));

        // Mark the new node as changed.
        self.changed_nodes.insert(id);
        id
    }

    /// Remove a node from the node tree, clearing any interaction state
    /// (hover/active/focus/mousedown/selection/drag/scrollbar) that references
    /// it so that stale NodeIds are never dereferenced after the slot is freed.
    pub(crate) fn remove_node_from_tree(&mut self, node_id: NodeId) -> Option<Node> {
        self.clear_interaction_state_for_removed_node(node_id);
        self.nodes.remove(node_id)
    }

    /// The nearest element ancestor of `node_id` that is still in the
    /// document. Used to retarget hover/active state when the node they
    /// reference is removed. Tolerates already-removed ancestors (subtree
    /// teardown proceeds root-first) by giving up and returning `None`.
    pub(super) fn nearest_surviving_element_ancestor(&self, node_id: NodeId) -> Option<NodeId> {
        let mut current = self.get_node(node_id)?.parent;
        while let Some(id) = current {
            let node = self.get_node(id)?;
            if node.is_element() && node.flags.is_in_document() {
                return Some(id);
            }
            current = node.parent;
        }
        None
    }

    /// Clear any interaction state (hover/active/focus/mousedown/selection/
    /// drag/scrollbar) that references `node_id`, which is being removed from
    /// the document, running the usual teardown steps. `node_id` must still be
    /// present in the slab.
    ///
    /// This matches browser semantics (WebKit `hoveredElementDidDetach` /
    /// `elementInActiveChainDidDetach`, Blink `HoveredElementDetached` /
    /// `ActiveChainNodeDetached`):
    /// - Hover and active retarget to the nearest surviving element ancestor
    ///   as a *transient bridge*: the HOVER/ACTIVE element-state bits along
    ///   the surviving chain stay lit (no one-frame gap in `:hover`/`:active`
    ///   styling), and the subsequent hover diff can unset exactly the right
    ///   bits. Hover is then re-resolved against the pointer position by
    ///   [`Self::refresh_hover`] at the end of the next resolve pass (the
    ///   analogue of WebKit's "fake mouse move"), which corrects the bridge
    ///   value — including cases where the removed node overflowed its
    ///   ancestor's box, so the ancestor was never truly under the pointer.
    /// - Focus resets to the body (encoded as `None`), running blur
    ///   side-effects (clearing focus element state and disabling IME for
    ///   text inputs).
    pub(crate) fn clear_interaction_state_for_removed_node(&mut self, node_id: NodeId) {
        if !self.nodes.contains_key(node_id) {
            return;
        }

        if self.hover_node_id == Some(node_id) {
            self.hover_node_id = self.nearest_surviving_element_ancestor(node_id);
            self.hover_node_is_text = false;
        }
        if self.hover_hit_node_id == Some(node_id) {
            self.hover_hit_node_id = None;
        }
        if self.active_node_id == Some(node_id) {
            self.active_node_id = self.nearest_surviving_element_ancestor(node_id);
        }
        if self.focus_node_id == Some(node_id) {
            let shell_provider = self.shell_provider.clone();
            self.nodes[node_id].blur(shell_provider);
            self.focus_node_id = None;
        }
        if self.mousedown_node_id == Some(node_id) {
            self.mousedown_node_id = None;
        }
        if self.text_selection.anchor.node_or_parent == Some(node_id)
            || self.text_selection.focus.node_or_parent == Some(node_id)
        {
            self.text_selection.clear();
        }
        if self
            .hovered_scrollbar
            .is_some_and(|scrollbar| scrollbar.node_id == node_id)
        {
            self.hovered_scrollbar = None;
        }
        let drag_references_node = match &self.drag_mode {
            DragMode::Panning(state) => state.target == node_id,
            DragMode::ScrollbarDrag(state) => state.scrollbar.node_id == node_id,
            DragMode::Selecting | DragMode::None => false,
        };
        if drag_references_node {
            self.drag_mode = DragMode::None;
        }
        self.scrollbar_activity.remove(&node_id);
    }

    pub(crate) fn drop_node_ignoring_parent(&mut self, node_id: NodeId) -> Option<Node> {
        self.drop_node_ignoring_parent_with(node_id, &mut |_| {})
    }

    /// Like [`Self::drop_node_ignoring_parent`], but calls `on_drop` with the id of
    /// every dropped node (the node itself and all of its descendants).
    pub(crate) fn drop_node_ignoring_parent_with(
        &mut self,
        node_id: NodeId,
        on_drop: &mut dyn FnMut(NodeId),
    ) -> Option<Node> {
        let mut node = self.remove_node_from_tree(node_id);
        if let Some(node) = &mut node {
            on_drop(node_id);
            if let Some(before) = node.before() {
                self.drop_node_ignoring_parent_with(before, on_drop);
            }
            if let Some(after) = node.after() {
                self.drop_node_ignoring_parent_with(after, on_drop);
            }

            for &child in &node.children {
                self.drop_node_ignoring_parent_with(child, on_drop);
            }

            // Anonymous blocks live only in the slab, so deallocate the ones this
            // node owns rather than leaking them.
            for &anon_id in &node.anonymous_blocks {
                self.deallocate_anonymous_block(anon_id);
            }
        }
        node
    }

    /// Deallocate an anonymous block created in a previous construction
    /// round, along with any anonymous blocks nested within it.
    pub(crate) fn deallocate_anonymous_block(&mut self, anon_id: NodeId) {
        // The block may already have been removed from the slab (e.g. a
        // whitespace-only anonymous block dropped during construction).
        if !self.nodes.contains_key(anon_id) {
            return;
        }

        // Free any anonymous blocks that this block owns before removing it.
        let nested = std::mem::take(&mut self.nodes[anon_id].anonymous_blocks);
        for nested_id in nested {
            self.deallocate_anonymous_block(nested_id);
        }

        self.remove_node_from_tree(anon_id);
    }

    /// Whether the document has been mutated
    pub fn has_changes(&self) -> bool {
        self.changed_nodes.is_empty()
    }

    pub fn create_text_node(&mut self, text: &str) -> NodeId {
        let content = text.to_string();
        let data = NodeData::Text(TextNodeData::new(content));
        self.create_node(data)
    }

    pub fn deep_clone_node(&mut self, node_id: NodeId) -> NodeId {
        // Load existing node
        let node = &self.nodes[node_id];
        let mut data = node.data.clone();

        match &mut data {
            NodeData::Element(elem) | NodeData::AnonymousBlock(elem) => {
                if let Some(arc) = elem.style_attribute.as_mut() {
                    let read_guard = self.guard().read();
                    let block = arc.read_with(&read_guard);
                    *arc = ServoArc::new(self.guard().wrap(block.clone()));
                }
            }
            _ => {}
        }

        let children = node.children.clone();

        // Create new node
        let new_node_id = self.create_node(data);

        // Recursively clone children
        let new_children: ThinVec<NodeId> = children
            .into_iter()
            .map(|child_id| self.deep_clone_node(child_id))
            .collect();
        for &child_id in &new_children {
            self.nodes[child_id].parent = Some(new_node_id);
        }
        self.nodes[new_node_id].children = new_children;

        new_node_id
    }

    pub(crate) fn remove_and_drop_pe(&mut self, node_id: NodeId) -> Option<Node> {
        fn remove_pe_ignoring_parent(doc: &mut BaseDocument, node_id: NodeId) -> Option<Node> {
            let mut node = doc.remove_node_from_tree(node_id);
            if let Some(node) = &mut node {
                for &child in &node.children {
                    remove_pe_ignoring_parent(doc, child);
                }
                for &anon_id in &node.anonymous_blocks {
                    doc.deallocate_anonymous_block(anon_id);
                }
            }
            node
        }

        let node = remove_pe_ignoring_parent(self, node_id);

        // Update child_idx values
        if let Some(parent_id) = node.as_ref().and_then(|node| node.parent) {
            let parent = &mut self.nodes[parent_id];
            parent.children.retain(|id| *id != node_id);
        }

        node
    }

    pub(crate) fn resolve_url(&self, raw: &str) -> url::Url {
        self.url.resolve_relative(raw).unwrap_or_else(|| {
            panic!(
                "to be able to resolve {raw} with the base_url: {:?}",
                *self.url
            )
        })
    }

    pub fn print_tree(&self) {
        crate::util::walk_tree(0, self.root_node());
    }

    pub fn print_subtree(&self, node_id: NodeId) {
        crate::util::walk_tree(0, &self.nodes[node_id]);
    }

    pub fn reload_resource_by_href(&mut self, href_to_reload: &str) {
        for &node_id in self.nodes_to_stylesheet.keys() {
            let node = &self.nodes[node_id];
            let Some(element) = node.element_data() else {
                continue;
            };

            if element.name.local == local_name!("link") {
                if let Some(href) = element.attr(local_name!("href")) {
                    // println!("Node {node_id} {href} {href_to_reload} {} {}", resolved_href.as_str(), resolved_href.as_str() == url_to_reload);
                    if href == href_to_reload {
                        let resolved_href = self.resolve_url(href);
                        self.net_provider.fetch(
                            self.id(),
                            self.build_request(resolved_href.clone()),
                            ResourceHandler::boxed(
                                self.tx.clone(),
                                self.id,
                                Some(node_id),
                                self.shell_provider.clone(),
                                StylesheetHandler {
                                    source_url: resolved_href,
                                    guard: self.guard.clone(),
                                    net_provider: self.net_provider.clone(),
                                    abort_signal: self.abort_signal.clone(),
                                },
                            ),
                        );
                    }
                }
            }
        }
    }

    pub fn process_style_element(&mut self, target_id: NodeId) {
        let css = self.nodes[target_id].text_content();
        let css = html_escape::decode_html_entities(&css);
        let sheet = self.make_stylesheet(&css, Origin::Author);
        self.add_stylesheet_for_node(sheet, target_id);
    }

    pub fn remove_user_agent_stylesheet(&mut self, sheet: &UserAgentStylesheet) {
        if self.useragent_stylesheets().any(|s| *s == sheet.0) {
            self.stylist
                .remove_stylesheet(sheet.0.clone(), &self.guard.read());
        }
    }

    /// The document's base URL
    pub fn url(&self) -> &url::Url {
        &self.url
    }

    /// Iterate over the author stylesheets (from `<style>` and `<link>` nodes)
    /// currently associated with this document
    pub fn author_stylesheets(&self) -> impl Iterator<Item = &DocumentStyleSheet> {
        self.nodes_to_stylesheet.values()
    }

    /// Iterate over the user-agent stylesheets currently associated with this document
    pub fn useragent_stylesheets(&self) -> impl Iterator<Item = &DocumentStyleSheet> {
        (0..self.stylist.sheet_count(Origin::UserAgent))
            .filter_map(|i| self.stylist.sheet_at(Origin::UserAgent, i))
    }

    /// Append a user-agent stylesheet. Adding a sheet that is already present is a no-op.
    pub fn add_user_agent_stylesheet(&mut self, sheet: UserAgentStylesheet) {
        if self.useragent_stylesheets().any(|s| *s == sheet.0) {
            return;
        }
        self.stylist.append_stylesheet(sheet.0, &self.guard.read());
    }

    pub fn make_stylesheet(&self, css: impl AsRef<str>, origin: Origin) -> DocumentStyleSheet {
        let data = Stylesheet::from_str(
            css.as_ref(),
            self.url.url_extra_data(),
            origin,
            ServoArc::new(self.guard.wrap(MediaList::empty())),
            self.guard.clone(),
            Some(&StylesheetLoader {
                tx: self.tx.clone(),
                doc_id: self.id,
                net_provider: self.net_provider.clone(),
                shell_provider: self.shell_provider.clone(),
                abort_signal: self.abort_signal.clone(),
                import_depth: 0,
            }),
            None,
            QuirksMode::NoQuirks,
            AllowImportRules::Yes,
        );

        DocumentStyleSheet(ServoArc::new(data))
    }

    pub fn upsert_stylesheet_for_node(&mut self, node_id: NodeId) {
        let raw_styles = self.nodes[node_id].text_content();
        let sheet = self.make_stylesheet(raw_styles, Origin::Author);
        self.add_stylesheet_for_node(sheet, node_id);
    }

    pub fn add_stylesheet_for_node(&mut self, stylesheet: DocumentStyleSheet, node_id: NodeId) {
        let old = self.nodes_to_stylesheet.insert(node_id, stylesheet.clone());
        self.stylesheet_generation += 1;

        if let Some(old) = old {
            self.stylist.remove_stylesheet(old, &self.guard.read())
        }

        // Fetch @font-face fonts
        crate::net::fetch_font_face(
            self.tx.clone(),
            self.id,
            Some(node_id),
            &stylesheet.0,
            &self.net_provider,
            &self.shell_provider,
            &self.guard.read(),
            self.abort_signal.as_ref(),
        );

        // Store data on element
        let element = &mut self.nodes[node_id].element_data_mut().unwrap();
        element.special_data = SpecialElementData::Stylesheet(stylesheet.clone());

        // TODO: Nodes could potentially get reused so ordering by node_id might be wrong.
        let insertion_point = self
            .nodes_to_stylesheet
            .range((Bound::Excluded(node_id), Bound::Unbounded))
            .next()
            .map(|(_, sheet)| sheet);

        if let Some(insertion_point) = insertion_point {
            self.stylist.insert_stylesheet_before(
                stylesheet,
                insertion_point.clone(),
                &self.guard.read(),
            )
        } else {
            self.stylist
                .append_stylesheet(stylesheet, &self.guard.read())
        }
    }

    pub fn handle_messages(&mut self) {
        // Remove event Reciever from the Document so that we can process events
        // without holding a borrow to the Document
        let rx = self.rx.take().unwrap();

        while let Ok(msg) = rx.try_recv() {
            self.handle_message(msg);
        }

        // Put Reciever back
        self.rx = Some(rx);
    }

    pub fn handle_message(&mut self, msg: DocumentEvent) {
        match msg {
            DocumentEvent::ResourceLoad(resource) => self.load_resource(resource),
            DocumentEvent::NavigateIframe { node_id, url } => self.navigate_iframe(node_id, url),
        }
    }

    /// Whether the Document has pending requests for "critical" resources (that should block rendering)
    pub fn has_pending_critical_resources(&self) -> bool {
        !self.pending_critical_resources.is_empty()
    }
}
