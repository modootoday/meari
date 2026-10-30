use super::*;

impl BaseDocument {
    /// CSSOM View's `offsetLeft`/`offsetTop`/`offsetWidth`/`offsetHeight` as a single rect:
    /// the node's border box, positioned relative to the padding edge of its `offsetParent`
    /// (or to the initial containing block when the `offsetParent` is a non-positioned
    /// `body`, or there is none).
    ///
    /// Nodes with their own layout box use that box directly. Non-atomic inline elements
    /// have no layout box: they use the bounding box of their per-line-box fragments, and
    /// their `offsetParent` is resolved from the containing inline root.
    pub fn offset_rect(&self, node_id: NodeId) -> Option<BoundingRect> {
        let node = self.get_node(node_id)?;

        // Nodes with their own layout box: use it directly
        let Some(rects) = self.inline_fragment_rects(node_id) else {
            let pos = node.offset_top_left();
            let size = node.unrounded_layout().size;
            #[cfg(feature = "writing-mode")]
            let size = match node.containing_block() {
                Some(parent) if self.nodes[parent].layout_data().writing_mode.is_vertical() => {
                    size.transpose()
                }
                _ => size,
            };
            return Some(BoundingRect {
                x: pos.x as f64,
                y: pos.y as f64,
                width: size.width as f64,
                height: size.height as f64,
            });
        };

        // Union the per-line-box fragments into a single bounding box
        let bounds = rects.reduce(BoundingRect::union)?;

        // Fragment rects are viewport-relative; convert to document-relative
        let scroll = self.viewport_scroll();
        let mut x = bounds.x + scroll.x;
        let mut y = bounds.y + scroll.y;

        // Resolve the offsetParent from the inline root (the root itself if it is positioned)
        let inline_root = node.inline_root_ancestor()?;
        let offset_parent = if inline_root.is_offset_parent() {
            Some(inline_root)
        } else {
            inline_root.offset_parent()
        };
        // Make the position relative to the offsetParent's padding edge
        if let Some(parent) = offset_parent.filter(|parent| !parent.is_static_body()) {
            let (parent_pos, parent_layout) = self.physical_unrounded_geometry(parent.id);
            let border = parent_layout.border;
            x -= (parent_pos.x + border.left) as f64;
            y -= (parent_pos.y + border.top) as f64;
        }

        Some(BoundingRect {
            x,
            y,
            width: bounds.width,
            height: bounds.height,
        })
    }

    /// Computes the sizes and positions of the `Node`'s box fragments relative to the
    /// viewport (CSSOM `getClientRects()` semantics). Nodes with their own layout box
    /// return a single rect. Non-atomic inline elements (which are laid out as style
    /// spans within an inline root's text layout) return one rect per line box.
    pub fn node_client_rects(&self, node_id: NodeId) -> Vec<BoundingRect> {
        match self.inline_fragment_rects(node_id) {
            Some(rects) => rects.collect(),
            None => self.get_client_bounding_rect(node_id).into_iter().collect(),
        }
    }

    /// Computes per-line-box fragment rects for a non-atomic inline element by walking
    /// the containing inline root's text layout. Returns `None` for nodes that have
    /// their own layout box (which should use `get_client_bounding_rect` instead).
    pub fn inline_fragment_rects(
        &self,
        node_id: NodeId,
    ) -> Option<impl Iterator<Item = BoundingRect> + '_> {
        let node = self.get_node(node_id)?;

        // Only non-atomic inline elements lack their own layout box: they are
        // flattened into the containing inline root's text layout as style spans.
        let fragments = node.inline_fragment_boxes()?;
        let inline_root = node.inline_root_ancestor()?;

        // Fragment boxes are relative to the inline root's border box.
        let (root_pos, _) = self.physical_unrounded_geometry(inline_root.id);
        let origin_x = root_pos.x as f64 - self.viewport_scroll().x;
        let origin_y = root_pos.y as f64 - self.viewport_scroll().y;

        Some(fragments.map(move |rect| BoundingRect {
            x: snap_to_layout_unit(origin_x + rect.left as f64),
            y: snap_to_layout_unit(origin_y + rect.top as f64),
            width: snap_to_layout_unit((rect.right - rect.left) as f64),
            height: snap_to_layout_unit((rect.bottom - rect.top) as f64),
        }))
    }

    /// The first element in tree order with the given tag name. The root element and
    /// its children are checked first as a fast path before a full tree search, making
    /// this suitable for the `documentElement`/`head`/`body` document accessors.
    pub fn find_element_by_tag_name(&self, tag: &LocalName) -> Option<&Node> {
        let root = self.try_root_element()?;
        if root.data.is_element_with_tag_name(tag) {
            return Some(root);
        }
        root.children
            .iter()
            .copied()
            .find(|child_id| {
                self.get_node(*child_id)
                    .is_some_and(|child| child.data.is_element_with_tag_name(tag))
            })
            .or_else(|| {
                TreeTraverser::new(self)
                    .find(|node_id| self.nodes[*node_id].data.is_element_with_tag_name(tag))
            })
            .map(|node_id| &self.nodes[node_id])
    }

    /// The document's `body` element, as for `document.body`
    pub fn find_body_node(&self) -> Option<&Node> {
        self.find_element_by_tag_name(&local_name!("body"))
    }

    /// The document's `head` element, as for `document.head`
    pub fn find_head_node(&self) -> Option<&Node> {
        self.find_element_by_tag_name(&local_name!("head"))
    }

    /// The document's `title` element
    pub fn find_title_node(&self) -> Option<&Node> {
        self.find_element_by_tag_name(&local_name!("title"))
    }

    pub fn with_text_input(
        &mut self,
        node_id: NodeId,
        cb: impl FnOnce(PlainEditorDriver<TextBrush>),
    ) {
        let Some(node) = self.nodes.get_mut(node_id) else {
            return;
        };

        if let Some(text_input) = node
            .element_data_mut()
            .and_then(|el| el.text_input_data_mut())
        {
            let mut font_ctx = self.font_ctx.lock().unwrap();
            let layout_ctx = &mut self.layout_ctx;
            let driver = text_input.editor.driver(&mut font_ctx, layout_ctx);
            cb(driver)
        }
    }

    /// Recompute the scroll offset of the text input at `node_id` (if any) so that its caret
    /// remains visible within the input's content box.
    pub(crate) fn clamp_text_input_scroll(&mut self, node_id: NodeId) {
        let Some(node) = self.nodes.get_mut(node_id) else {
            return;
        };

        let content_box_width = node.final_layout().content_box_width();
        let content_box_height = node.final_layout().content_box_height();

        if let Some(text_input) = node
            .element_data_mut()
            .and_then(|el| el.text_input_data_mut())
        {
            text_input.clamp_scroll_offset(content_box_width, content_box_height);
        }
    }

    pub(crate) fn compute_has_canvas(&self) -> bool {
        TreeTraverser::new(self).any(|node_id| {
            let node = &self.nodes[node_id];
            let Some(element) = node.element_data() else {
                return false;
            };
            if element.name.local == local_name!("canvas") && element.has_attr(local_name!("src")) {
                return true;
            }

            false
        })
    }

    // Text selection methods

    /// Find the text position (inline_root_id, byte_offset) at a given point.
    /// Uses hit() for proper coordinate transformation, then finds the inline root
    /// and byte offset.
    pub fn find_text_position(&self, x: f32, y: f32) -> Option<(NodeId, usize)> {
        let hit = self.hit(x, y)?;
        let hit_node = self.get_node(hit.node_id)?;
        let inline_root = hit_node.inline_root_ancestor()?;
        let byte_offset = inline_root.text_offset_at_point(hit.x, hit.y)?;
        Some((inline_root.id, byte_offset))
    }

    /// Set the text selection range (creates a new selection from anchor to focus)
    pub fn set_text_selection(
        &mut self,
        anchor_node: NodeId,
        anchor_offset: usize,
        focus_node: NodeId,
        focus_offset: usize,
    ) {
        self.text_selection =
            TextSelection::new(anchor_node, anchor_offset, focus_node, focus_offset);

        // For anonymous blocks, switch to storing parent+sibling_index (stable reference)
        if let (Some(parent), Some(idx)) = self.anonymous_block_location(anchor_node) {
            self.text_selection
                .anchor
                .set_anonymous(parent, idx, anchor_offset);
        }
        if let (Some(parent), Some(idx)) = self.anonymous_block_location(focus_node) {
            self.text_selection
                .focus
                .set_anonymous(parent, idx, focus_offset);
        }
    }

    /// Get the parent ID and sibling index for a node if it's an anonymous block.
    /// Returns (None, None) for non-anonymous blocks.
    pub(super) fn anonymous_block_location(
        &self,
        node_id: NodeId,
    ) -> (Option<NodeId>, Option<usize>) {
        let Some(node) = self.get_node(node_id) else {
            return (None, None);
        };

        if !node.is_anonymous() {
            return (None, None);
        }

        let Some(parent_id) = node.parent else {
            return (None, None);
        };

        let Some(parent) = self.get_node(parent_id) else {
            return (Some(parent_id), None);
        };

        let layout_children = parent.layout_children.borrow();
        let Some(children) = layout_children.as_ref() else {
            return (Some(parent_id), None);
        };

        // Find the index of this anonymous block among siblings
        let mut anon_index = 0;
        for &child_id in children.iter() {
            if child_id == node_id {
                return (Some(parent_id), Some(anon_index));
            }
            if self.get_node(child_id).is_some_and(|n| n.is_anonymous()) {
                anon_index += 1;
            }
        }

        (Some(parent_id), None)
    }

    /// Clear the text selection
    pub fn clear_text_selection(&mut self) {
        self.text_selection.clear();
    }

    /// Update the selection focus point (used during mouse drag to extend selection).
    pub fn update_selection_focus(&mut self, focus_node: NodeId, focus_offset: usize) {
        // For anonymous blocks, store parent+sibling_index; otherwise store node directly
        if let (Some(parent), Some(idx)) = self.anonymous_block_location(focus_node) {
            self.text_selection
                .focus
                .set_anonymous(parent, idx, focus_offset);
        } else {
            self.text_selection.set_focus(focus_node, focus_offset);
        }
    }

    /// Extend text selection to the given point. Returns true if selection was updated.
    /// This is a convenience method that combines find_text_position and update_selection_focus.
    pub fn extend_text_selection_to_point(&mut self, x: f32, y: f32) -> bool {
        if !self.text_selection.anchor.is_some() {
            return false;
        }

        if let Some((node, offset)) = self.find_text_position(x, y) {
            self.update_selection_focus(node, offset);
            self.shell_provider.request_redraw();
            true
        } else {
            false
        }
    }

    /// Find the Nth anonymous block under a parent.
    pub(super) fn find_anonymous_block_by_index(
        &self,
        parent_id: NodeId,
        target_index: usize,
    ) -> Option<NodeId> {
        let parent = self.get_node(parent_id)?;
        let layout_children = parent.layout_children.borrow();
        let children = layout_children.as_ref()?;

        children
            .iter()
            .filter(|&&child_id| self.get_node(child_id).is_some_and(|n| n.is_anonymous()))
            .nth(target_index)
            .copied()
    }

    /// Check if there is an active (non-empty) text selection
    pub fn has_text_selection(&self) -> bool {
        self.text_selection.is_active()
    }

    /// Get the selected text content, supporting selection across multiple inline roots.
    pub fn get_selected_text(&self) -> Option<String> {
        let ranges = self.get_text_selection_ranges();
        if ranges.is_empty() {
            return None;
        }

        let mut result = String::new();
        for (node_id, start, end) in &ranges {
            let node = self.get_node(*node_id)?;
            let element_data = node.element_data()?;
            let inline_layout = element_data.inline_layout_data.as_ref()?;

            if *end > inline_layout.text.len() {
                continue;
            }

            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(&inline_layout.text[*start..*end]);
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Get all selection ranges as Vec<(node_id, start_offset, end_offset)>.
    /// Returns empty vec if no selection.
    pub fn get_text_selection_ranges(&self) -> Vec<(NodeId, usize, usize)> {
        let lookup = |parent_id, idx| self.find_anonymous_block_by_index(parent_id, idx);

        let anchor_node = match self.text_selection.anchor.resolve_node_id(lookup) {
            Some(id) => id,
            None => return Vec::new(),
        };
        let focus_node = match self.text_selection.focus.resolve_node_id(lookup) {
            Some(id) => id,
            None => return Vec::new(),
        };

        // Guard against stale selection endpoints: nodes may have been removed from
        // the document (e.g. by script) since the selection was made.
        let node_is_in_doc = |node_id: NodeId| {
            self.nodes
                .get(node_id)
                .is_some_and(|node| node.flags.is_in_document())
        };
        if !node_is_in_doc(anchor_node) || !node_is_in_doc(focus_node) {
            return Vec::new();
        }

        // Single node selection
        if anchor_node == focus_node {
            let start = self
                .text_selection
                .anchor
                .offset
                .min(self.text_selection.focus.offset);
            let end = self
                .text_selection
                .anchor
                .offset
                .max(self.text_selection.focus.offset);

            if start == end {
                return Vec::new();
            }
            return vec![(anchor_node, start, end)];
        }

        // Multi-node selection: collect all inline roots between anchor and focus
        let inline_roots = self.collect_inline_roots_in_range(anchor_node, focus_node);
        if inline_roots.is_empty() {
            return Vec::new();
        }

        // Determine document order using the collected inline_roots order
        // (inline_roots is already in document order from first to last)
        let first_in_roots = inline_roots[0];

        let (first_node, first_offset, last_node, last_offset) =
            if first_in_roots == anchor_node || (first_in_roots != focus_node) {
                // anchor is first (or neither endpoint is in roots, which shouldn't happen)
                (
                    anchor_node,
                    self.text_selection.anchor.offset,
                    focus_node,
                    self.text_selection.focus.offset,
                )
            } else {
                // focus is first
                (
                    focus_node,
                    self.text_selection.focus.offset,
                    anchor_node,
                    self.text_selection.anchor.offset,
                )
            };

        let mut ranges = Vec::with_capacity(inline_roots.len());

        for &node_id in &inline_roots {
            let Some(node) = self.get_node(node_id) else {
                continue;
            };
            let Some(element_data) = node.element_data() else {
                continue;
            };
            let Some(inline_layout) = element_data.inline_layout_data.as_ref() else {
                continue;
            };

            let text_len = inline_layout.text.len();

            if node_id == first_node && node_id == last_node {
                let start = first_offset.min(last_offset);
                let end = first_offset.max(last_offset);
                if start < end && end <= text_len {
                    ranges.push((node_id, start, end));
                }
            } else if node_id == first_node {
                if first_offset < text_len {
                    ranges.push((node_id, first_offset, text_len));
                }
            } else if node_id == last_node {
                if last_offset > 0 && last_offset <= text_len {
                    ranges.push((node_id, 0, last_offset));
                }
            } else if text_len > 0 {
                ranges.push((node_id, 0, text_len));
            }
        }

        ranges
    }
}

impl BoundingRect {
    /// The smallest rect containing both `self` and `other`.
    pub fn union(self, other: BoundingRect) -> BoundingRect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        BoundingRect {
            x,
            y,
            width: (self.x + self.width).max(other.x + other.width) - x,
            height: (self.y + self.height).max(other.y + other.height) - y,
        }
    }
}

/// Snap a CSSOM geometry value to a 1/64px grid (the precision of Blink's `LayoutUnit`).
/// Layout values are accumulated in `f32`, so e.g. seven `55/7`-wide flex items would
/// otherwise end at `55.0000005` and appear to overflow their 55px container.
pub(super) fn snap_to_layout_unit(value: f64) -> f64 {
    (value * 64.0).round() / 64.0
}

impl AsRef<BaseDocument> for BaseDocument {
    fn as_ref(&self) -> &BaseDocument {
        self
    }
}

impl AsMut<BaseDocument> for BaseDocument {
    fn as_mut(&mut self) -> &mut BaseDocument {
        self
    }
}
