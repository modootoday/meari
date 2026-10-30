use super::*;

impl Drop for DocumentMutator<'_> {
    fn drop(&mut self) {
        self.flush(); // Defined at bottom of file
        if self.mutations_occurred {
            self.doc.shell_provider.request_redraw();
        }
    }
}

impl DocumentMutator<'_> {
    pub fn new<'doc>(doc: &'doc mut BaseDocument) -> DocumentMutator<'doc> {
        DocumentMutator {
            doc,
            eager_op_queue: Vec::new(),
            title_node: None,
            style_nodes: HashSet::new(),
            form_nodes: HashSet::new(),
            recompute_is_animating: false,
            mutations_occurred: false,
            #[cfg(feature = "autofocus")]
            node_to_autofocus: None,
        }
    }

    // Query methods

    pub fn node_has_parent(&self, node_id: NodeId) -> bool {
        self.doc.nodes[node_id].parent.is_some()
    }

    pub fn previous_sibling_id(&self, node_id: NodeId) -> Option<NodeId> {
        self.doc.nodes[node_id].backward(1).map(|node| node.id)
    }

    pub fn next_sibling_id(&self, node_id: NodeId) -> Option<NodeId> {
        self.doc.nodes[node_id].forward(1).map(|node| node.id)
    }

    pub fn parent_id(&self, node_id: NodeId) -> Option<NodeId> {
        self.doc.nodes[node_id].parent
    }

    pub fn last_child_id(&self, node_id: NodeId) -> Option<NodeId> {
        self.doc.nodes[node_id].children.last().copied()
    }

    pub fn child_ids(&self, node_id: NodeId) -> ThinVec<NodeId> {
        self.doc.nodes[node_id].children.clone()
    }

    pub fn element_name(&self, node_id: NodeId) -> Option<&QualName> {
        self.doc.nodes[node_id].element_data().map(|el| &el.name)
    }

    pub fn node_at_path(&self, start_node_id: NodeId, path: &[u8]) -> NodeId {
        let mut current = &self.doc.nodes[start_node_id];
        for i in path {
            let new_id = current.children[*i as usize];
            current = &self.doc.nodes[new_id];
        }
        current.id
    }

    // Node creation methods

    pub fn create_comment_node(&mut self, contents: &str) -> NodeId {
        self.doc.create_node(NodeData::Comment {
            contents: contents.to_string(),
        })
    }

    pub fn create_text_node(&mut self, text: &str) -> NodeId {
        self.doc.create_text_node(text)
    }

    pub fn create_element(&mut self, name: QualName, attrs: Vec<Attribute>) -> NodeId {
        let mut data = ElementData::new(name, attrs);
        data.flush_style_attribute(self.doc.guard(), &self.doc.url.url_extra_data());

        let id = self.doc.create_node(NodeData::Element(Box::new(data)));
        let node = self.doc.get_node_mut(id).unwrap();

        // Initialise style data
        *node.stylo_element_data_mut().ensure_init_mut() = style::data::ElementData {
            damage: ALL_DAMAGE,
            ..Default::default()
        };

        id
    }

    pub fn deep_clone_node(&mut self, node_id: NodeId) -> NodeId {
        self.doc.deep_clone_node(node_id)
    }

    /// Create (or return the existing) "template contents" fragment node for a
    /// `<template>` element. The contents node is detached from the tree (so it
    /// is inert: never styled or rendered) and is exposed to JavaScript as the
    /// template's `content` DocumentFragment.
    pub fn template_contents(&mut self, template_id: NodeId) -> NodeId {
        if let Some(contents_id) = self.try_template_contents(template_id) {
            return contents_id;
        }
        let name = QualName::new(
            None,
            markup5ever::ns!(html),
            markup5ever::LocalName::from("#document-fragment"),
        );
        let contents_id = self.create_element(name, Vec::new());
        if let Some(element) = self
            .doc
            .get_node_mut(template_id)
            .and_then(|node| node.element_data_mut())
        {
            element.template_contents = Some(contents_id);
        }
        contents_id
    }

    /// The "template contents" fragment node of a `<template>` element (if any)
    pub fn try_template_contents(&self, template_id: NodeId) -> Option<NodeId> {
        self.doc
            .get_node(template_id)
            .and_then(|node| node.element_data())
            .and_then(|element| element.template_contents)
    }

    // Node mutation methods

    pub fn set_node_text(&mut self, node_id: NodeId, value: &str) {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        let node = &mut self.doc.nodes[node_id];

        let text = match node.data {
            NodeData::Text(ref mut text) => text,
            // TODO: otherwise this is basically element.textContent which is a bit different - need to parse as html
            _ => return,
        };

        let changed = text.content != value;
        if changed {
            self.mutations_occurred |= node_is_in_document;
            text.content.clear();
            text.content.push_str(value);
            node.insert_damage(ALL_DAMAGE);
            // Mark ancestors dirty so the style traversal visits this subtree.
            // Without this, the traversal may skip nodes with pending damage.
            node.mark_ancestors_dirty();
            let parent_id = node.parent;

            // Also insert damage on the parent element, since text content changes
            // affect the parent's layout (text may wrap differently, change size, etc.)
            if let Some(parent_id) = parent_id {
                let parent = &mut self.doc.nodes[parent_id];
                parent.insert_damage(ALL_DAMAGE);
            }

            self.maybe_record_node(parent_id);
        }
    }

    pub fn append_text_to_node(
        &mut self,
        node_id: NodeId,
        text: &str,
    ) -> Result<(), AppendTextErr> {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        let node = &mut self.doc.nodes[node_id];
        node.insert_damage(ALL_DAMAGE);
        node.mark_ancestors_dirty();
        match node.text_data_mut() {
            Some(data) => {
                data.content += text;
                self.mutations_occurred |= node_is_in_document;
                Ok(())
            }
            None => Err(AppendTextErr::NotTextNode),
        }
    }

    pub fn add_attrs_if_missing(&mut self, node_id: NodeId, attrs: Vec<Attribute>) {
        let node = &mut self.doc.nodes[node_id];
        node.insert_damage(ALL_DAMAGE);
        let element_data = node.element_data_mut().expect("Not an element");

        let existing_names = element_data
            .attrs
            .iter()
            .map(|e| e.name.clone())
            .collect::<HashSet<_>>();

        for attr in attrs
            .into_iter()
            .filter(|attr| !existing_names.contains(&attr.name))
        {
            self.set_attribute(node_id, attr.name, &attr.value);
        }
    }

    pub fn set_attribute(&mut self, node_id: NodeId, name: QualName, value: &str) {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        if node_is_in_document {
            self.doc.snapshot_node(node_id);

            let node = &mut self.doc.nodes[node_id];
            if let Some(mut data) = node.try_stylo_element_data_mut().and_then(|s| s.get_mut()) {
                data.hint |= RestyleHint::restyle_subtree();
                data.damage.insert(ALL_DAMAGE);
            }
            node.mark_damaged();

            // TODO: make this fine grained / conditional based on ElementSelectorFlags
            let parent = node.parent;
            if let Some(parent_id) = parent {
                let parent = &mut self.doc.nodes[parent_id];
                if let Some(mut data) = parent
                    .try_stylo_element_data_mut()
                    .and_then(|s| s.get_mut())
                {
                    data.hint |= RestyleHint::restyle_subtree();
                }
            }

            // Mark ancestors dirty so the style traversal visits this subtree.
            // Without this, the traversal may skip nodes with pending RestyleHint/damage
            // because it uses dirty_descendants flags to determine which subtrees to visit.
            self.doc.nodes[node_id].mark_ancestors_dirty();
        }

        if name.local == local_name!("id") && node_is_in_document {
            if let Some(old_id) = self.doc.nodes[node_id]
                .element_data()
                .map(|element| element.id.clone())
            {
                if let Some(old_id) = old_id {
                    self.doc.remove_from_id_map(&old_id, node_id);
                }
                self.doc.add_to_id_map(value, node_id);
            }
        }

        let node = &mut self.doc.nodes[node_id];

        let NodeData::Element(ref mut element) = node.data else {
            return;
        };

        self.mutations_occurred |= node_is_in_document;
        // If element is a CustomWidget, then Ccall attribute_changed on it
        #[cfg(feature = "custom-widget")]
        if let SpecialElementData::CustomWidget(widget_data) = &mut element.special_data {
            let old_value = element.attrs.get(&name).as_ref().map(|attr| &*attr.value);
            widget_data
                .widget
                .attribute_changed(&name.local, old_value, Some(value));
        }

        element.attrs.set(name.clone(), value);

        // Focusability is cached on the element and comes from these
        // attributes, so it has to follow a change to one of them: a widget
        // that hands the focus around its own children - a menu, a grid -
        // sets their tabindex after creating them.
        if name.local == local_name!("tabindex")
            || name.local == local_name!("href")
            || name.local == local_name!("disabled")
        {
            element.flush_is_focussable();
        }

        if name.local == local_name!("href") {
            element.flush_link_state();
        }

        let tag = &element.name.local;
        let attr = &name.local;

        if *attr == local_name!("id") {
            element.id = Some(Atom::from(value))
        }

        if *attr == local_name!("value") {
            if let Some(input_data) = element.text_input_data_mut() {
                // Update text input value
                input_data.set_text(
                    &mut self.doc.font_ctx.lock().unwrap(),
                    &mut self.doc.layout_ctx,
                    value,
                );
            }
            return;
        }

        if *attr == local_name!("style") {
            element.flush_style_attribute(&self.doc.guard, &self.doc.url.url_extra_data());
            node.set_restyle_hint(RestyleHint::RESTYLE_STYLE_ATTRIBUTE);
            return;
        }

        if *attr == local_name!("disabled") && element.can_be_disabled() {
            node.disable();
            return;
        }

        // If node if not in the document, then don't apply any special behaviours
        // and simply set the attribute value
        if !node.flags.is_in_document() {
            return;
        }

        // Custom widgets replace the element's built-in tag-specific behaviour
        #[cfg(feature = "custom-widget")]
        if element.custom_widget_data().is_some() {
            return;
        }

        if (tag, attr) == tag_and_attr!("input", "checked") {
            set_input_checked_state(element, value.to_string());
        } else if (tag, attr) == tag_and_attr!("img", "src") {
            self.load_image(node_id);
        } else if (tag, attr) == tag_and_attr!("canvas", "src") {
            self.load_custom_paint_src(node_id);
        } else if (tag, attr) == tag_and_attr!("link", "href")
            || (tag, attr) == tag_and_attr!("link", "rel")
        {
            self.load_linked_stylesheet(node_id);
        } else if (tag, attr) == tag_and_attr!("iframe", "src")
            || (tag, attr) == tag_and_attr!("iframe", "srcdoc")
        {
            self.load_iframe(node_id);
        }
    }

    pub fn clear_attribute(&mut self, node_id: NodeId, name: QualName) {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        if node_is_in_document {
            self.doc.snapshot_node(node_id);

            let node = &mut self.doc.nodes[node_id];

            if let Some(mut data) = node.try_stylo_element_data_mut().and_then(|s| s.get_mut()) {
                data.hint |= RestyleHint::restyle_subtree();
                data.damage.insert(ALL_DAMAGE);
            }
            node.mark_damaged();

            // Mark ancestors dirty so the style traversal visits this subtree.
            // Without this, the traversal may skip nodes with pending RestyleHint/damage.
            node.mark_ancestors_dirty();
        }

        if name.local == local_name!("id") && node_is_in_document {
            if let Some(old_id) = self.doc.nodes[node_id]
                .element_data()
                .and_then(|element| element.id.clone())
            {
                self.doc.remove_from_id_map(&old_id, node_id);
            }
        }

        let node = &mut self.doc.nodes[node_id];

        let Some(element) = node.element_data_mut() else {
            return;
        };

        let removed_attr = element.attrs.remove(&name);
        let had_attr = removed_attr.is_some();
        if !had_attr {
            return;
        }
        self.mutations_occurred |= node_is_in_document;

        // If element is a CustomWidget, then call attribute_changed on it
        #[cfg(feature = "custom-widget")]
        if let SpecialElementData::CustomWidget(widget_data) = &mut element.special_data {
            let old_value = removed_attr.as_ref().map(|attr| &*attr.value);
            widget_data
                .widget
                .attribute_changed(&name.local, old_value, None);
        }

        if name.local == local_name!("id") {
            element.id = None;
        }

        // As in `set_attribute`: taking one of these away can make the element
        // unfocusable again.
        if name.local == local_name!("tabindex")
            || name.local == local_name!("href")
            || name.local == local_name!("disabled")
        {
            element.flush_is_focussable();
        }

        if name.local == local_name!("href") {
            element.flush_link_state();
        }

        // Update text input value
        if name.local == local_name!("value") {
            if let Some(input_data) = element.text_input_data_mut() {
                input_data.set_text(
                    &mut self.doc.font_ctx.lock().unwrap(),
                    &mut self.doc.layout_ctx,
                    "",
                );
            }
        }

        let tag = &element.name.local;
        let attr = &name.local;

        if *attr == local_name!("disabled") && element.can_be_disabled() {
            node.enable();
            return;
        }

        if *attr == local_name!("style") {
            element.flush_style_attribute(&self.doc.guard, &self.doc.url.url_extra_data());
            node.set_restyle_hint(RestyleHint::RESTYLE_STYLE_ATTRIBUTE);
        } else if (tag, attr) == tag_and_attr!("input", "checked") {
            element.set_checkbox_input_checked(false);
        } else if (tag, attr) == tag_and_attr!("canvas", "src") {
            self.recompute_is_animating = true;
        } else if (tag, attr) == tag_and_attr!("link", "href") {
            self.unload_stylesheet(node_id);
        } else if (tag, attr) == tag_and_attr!("iframe", "srcdoc") && node_is_in_document {
            // Fall back to loading from the `src` attribute (if any)
            self.load_iframe(node_id);
        }
    }

    pub fn set_style_property(&mut self, node_id: NodeId, name: &str, value: &str) {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        self.doc.set_style_property(node_id, name, value);
        self.mutations_occurred |= node_is_in_document;
    }

    pub fn remove_style_property(&mut self, node_id: NodeId, name: &str) {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        self.doc.remove_style_property(node_id, name);
        self.mutations_occurred |= node_is_in_document;
    }

    pub fn set_sub_document(&mut self, node_id: NodeId, sub_document: Box<dyn Document>) {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        self.doc.set_sub_document(node_id, sub_document);
        self.mutations_occurred |= node_is_in_document;
    }

    pub fn remove_sub_document(&mut self, node_id: NodeId) {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        self.doc.remove_sub_document(node_id);
        self.mutations_occurred |= node_is_in_document;
    }

    #[cfg(feature = "custom-widget")]
    pub fn set_custom_widget(&mut self, node_id: NodeId, widget: Box<dyn crate::Widget>) {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        self.doc.set_custom_widget(node_id, widget);
        self.mutations_occurred |= node_is_in_document;
    }

    #[cfg(feature = "custom-widget")]
    pub fn remove_custom_widget(&mut self, node_id: NodeId) {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        self.doc.remove_custom_widget(node_id);
        self.mutations_occurred |= node_is_in_document;
    }

    /// Remove the node from it's parent but don't drop it
    pub fn remove_node(&mut self, node_id: NodeId) {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        // Process the subtree *before* severing the parent link so that
        // interaction state referencing removed nodes can retarget to the
        // nearest surviving ancestor.
        self.process_removed_subtree(node_id);

        let node = &mut self.doc.nodes[node_id];

        // Update child_idx values
        if let Some(parent_id) = node.parent.take() {
            self.mutations_occurred |= node_is_in_document;
            let parent = &mut self.doc.nodes[parent_id];
            parent.insert_damage(ALL_DAMAGE);
            // Mark ancestors dirty so the style traversal visits this subtree.
            parent.mark_ancestors_dirty();
            parent.children.retain(|id| *id != node_id);
            self.maybe_record_node(parent_id);
        }
    }

    pub fn remove_and_drop_node(&mut self, node_id: NodeId) -> Option<Node> {
        self.remove_and_drop_node_with(node_id, &mut |_| {})
    }

    /// Like [`Self::remove_and_drop_node`], but calls `on_drop` with the id of
    /// every dropped node (the node itself and all of its descendants).
    pub fn remove_and_drop_node_with(
        &mut self,
        node_id: NodeId,
        on_drop: &mut dyn FnMut(NodeId),
    ) -> Option<Node> {
        let node_is_in_document = self.doc.nodes[node_id].flags.is_in_document();
        self.process_removed_subtree(node_id);

        let node = self.doc.drop_node_ignoring_parent_with(node_id, on_drop);
        self.mutations_occurred |= node_is_in_document;

        // Update child_idx values
        if let Some(parent_id) = node.as_ref().and_then(|node| node.parent) {
            let parent = &mut self.doc.nodes[parent_id];
            parent.insert_damage(ALL_DAMAGE);
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

            parent.children.retain(|id| *id != node_id);
            self.maybe_record_node(parent_id);
        }

        node
    }
}
