use super::*;

impl BaseDocument {
    /// Whether any overlay scrollbars are awaiting or animating their
    /// fade-out (so frames must keep rendering until they finish).
    pub(super) fn scrollbars_animating(&self) -> bool {
        use crate::node::scrollbar::{FADE_DELAY, FADE_DURATION};
        self.scrollbar_activity
            .values()
            .any(|last| last.elapsed() < FADE_DELAY + FADE_DURATION)
    }

    /// [`hit`](Self::hit), also resolving the innermost overlay scrollbar
    /// thumb under the point (shares the traversal, so it costs nothing
    /// extra).
    pub(crate) fn hit_with_scrollbar(
        &self,
        x: f32,
        y: f32,
    ) -> (Option<HitResult>, Option<crate::node::ScrollbarRef>) {
        if TDocument::as_node(&self.root_node())
            .first_element_child()
            .is_none()
        {
            #[cfg(feature = "tracing")]
            tracing::warn!("No DOM - not resolving hit test");
            return (None, None);
        }
        let mut scrollbar = None;
        let hit = self
            .root_element()
            .hit_inner(x, y, self.viewport().scale_f64(), &mut scrollbar);
        (hit, scrollbar)
    }

    pub fn set_hover_to(&mut self, x: f32, y: f32) -> bool {
        // Record the pointer position in client (unscrolled) coordinates so
        // that `refresh_hover` can re-resolve hover state after layout or
        // scroll changes.
        self.last_client_pointer_position = Some(taffy::Point {
            x: x - self.viewport_scroll().x as f32,
            y: y - self.viewport_scroll().y as f32,
        });

        let (hit, hovered_scrollbar) = self.hit_with_scrollbar(x, y);
        // A faded-out thumb is not interactive: pointer moves never fade
        // overlay scrollbars back in (only scrolling shows them).
        let hovered_scrollbar =
            hovered_scrollbar.filter(|scrollbar| self.scrollbar_opacity(scrollbar.node_id) > 0.0);
        // Scrollbar-thumb hover is part of hover state: track it here so a
        // pointer crossing a thumb restyles it even when the hit node (the
        // content under the overlay thumb) is unchanged.
        let scrollbar_changed = hovered_scrollbar != self.hovered_scrollbar;
        if scrollbar_changed {
            // Entering a thumb restores full opacity mid-fade; leaving one
            // restarts the fade-out delay.
            for scrollbar in [self.hovered_scrollbar, hovered_scrollbar]
                .into_iter()
                .flatten()
            {
                self.show_scrollbars(scrollbar.node_id);
            }
        }
        self.hovered_scrollbar = hovered_scrollbar;

        // Store both the precise layout node that was hit (transient: used for
        // cursor/style queries) and its canonical DOM target (persistent: must
        // not reference layout-generated nodes, whose ids die on box-tree
        // reconstruction).
        let hit_node_id = hit.map(|hit| hit.node_id);
        let hover_node_id = hit_node_id.and_then(|id| self.nearest_non_anonymous_ancestor(id));
        let new_is_text = hit.map(|hit| hit.is_text).unwrap_or(false);

        self.hover_hit_node_id = hit_node_id;
        self.hover_node_is_text = new_is_text;

        // Return early if the new node is the same as the already-hovered node
        if hover_node_id == self.hover_node_id {
            return scrollbar_changed;
        }

        let old_node_path = self.maybe_node_element_ancestors(self.hover_node_id);
        let new_node_path = self.maybe_node_element_ancestors(hover_node_id);
        let same_count = old_node_path
            .iter()
            .zip(&new_node_path)
            .take_while(|(o, n)| o == n)
            .count();
        for &id in old_node_path.iter().skip(same_count) {
            self.snapshot_node_and(id, ElementState::HOVER, |node| node.unhover());
        }
        for &id in new_node_path.iter().skip(same_count) {
            self.snapshot_node_and(id, ElementState::HOVER, |node| node.hover());
        }

        self.hover_node_id = hover_node_id;

        // Request redraw
        self.shell_provider.request_redraw();

        true
    }

    pub fn clear_hover(&mut self) -> bool {
        // The pointer is no longer over the document, so stop re-resolving
        // hover state against it.
        self.last_client_pointer_position = None;
        self.hover_hit_node_id = None;

        let Some(hover_node_id) = self.hover_node_id else {
            return false;
        };

        let old_node_path = self.maybe_node_element_ancestors(Some(hover_node_id));
        for &id in old_node_path.iter() {
            self.snapshot_node_and(id, ElementState::HOVER, |node| node.unhover());
        }

        self.hover_node_id = None;
        self.hover_node_is_text = false;

        // Request redraw
        self.shell_provider.request_redraw();

        true
    }

    /// Re-resolve hover state against the current layout using the last known
    /// pointer position.
    ///
    /// TODO: synthesizing pointerenter/pointerleave DOM events for
    /// hover changes caused by layout shifts.
    pub fn refresh_hover(&mut self) -> bool {
        let Some(pos) = self.last_client_pointer_position else {
            return false;
        };
        let x = pos.x + self.viewport_scroll().x as f32;
        let y = pos.y + self.viewport_scroll().y as f32;
        let prev_hover = self.hover_state();
        let changed = self.set_hover_to(x, y);
        if self.hover_state() != prev_hover {
            self.update_cursor();
        }
        changed
    }

    /// The hover state that determines the cursor: the hovered node, the precise hit
    /// node and whether the hit is text
    pub(crate) fn hover_state(&self) -> (Option<NodeId>, Option<NodeId>, bool) {
        (
            self.hover_node_id,
            self.hover_hit_node_id,
            self.hover_node_is_text,
        )
    }

    /// Send the current cursor to the shell.
    ///
    /// Hover updates don't send the cursor themselves: the cursor of a node hosting a
    /// sub-document is only known once the sub-document has handled the event.
    pub(crate) fn update_cursor(&self) {
        self.shell_provider.set_cursor(self.get_cursor());
    }

    pub fn get_hover_node_id(&self) -> Option<NodeId> {
        self.hover_node_id
    }

    pub fn get_mousedown_node_id(&self) -> Option<NodeId> {
        self.mousedown_node_id
    }

    pub fn set_viewport(&mut self, viewport: Viewport) {
        let changes = DeviceChanges::from_viewports(&self.viewport, &viewport);
        self.viewport = viewport;
        self.queue_device_changes(changes);
    }

    /// Returns the current CSS media type used to evaluate `@media` rules.
    pub fn media_type(&self) -> &MediaType {
        &self.media_type
    }

    /// Sets the CSS media type used to evaluate `@media` rules (e.g. `screen` or `print`)
    /// and queues a stylist device rebuild so updated rules apply on the next restyle.
    pub fn set_media_type(&mut self, media_type: MediaType) {
        if self.media_type == media_type {
            return;
        }
        self.media_type = media_type;
        self.queue_device_changes(DeviceChanges::MEDIA_TYPE);
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn viewport_mut(&mut self) -> ViewportMut<'_> {
        ViewportMut::new(self)
    }

    pub fn zoom_by(&mut self, increment: f32) {
        *self.viewport_mut().zoom_mut() += increment;
    }

    pub fn zoom_to(&mut self, zoom: f32) {
        *self.viewport_mut().zoom_mut() = zoom;
    }

    pub fn get_viewport(&self) -> Viewport {
        self.viewport.clone()
    }

    /// Returns whether incremental layout is currently enabled for this document.
    ///
    /// Both modes run the same style → damage → box construction → layout
    /// pipeline. In incremental mode only nodes carrying `RestyleDamage` (from
    /// mutations or restyles) are reconstructed and have their layout caches
    /// cleared; in non-incremental mode every node is marked damaged at the
    /// start of each `resolve`, so everything is rebuilt each frame.
    pub fn incremental_layout(&self) -> bool {
        self.incremental_layout
    }

    /// Enables or disables incremental layout for this document.
    ///
    /// Disabling it does not switch to a separate code path: all nodes are
    /// simply treated as damaged on every `resolve` (see [`Self::incremental_layout`]).
    pub fn set_incremental_layout(&mut self, enabled: bool) {
        self.incremental_layout = enabled;
    }

    pub fn devtools(&self) -> &DevtoolSettings {
        &self.devtool_settings
    }

    pub fn devtools_mut(&mut self) -> &mut DevtoolSettings {
        &mut self.devtool_settings
    }

    pub fn subdoc(&self, node_id: NodeId) -> Option<&dyn Document> {
        self.get_node(node_id)
            .and_then(|node| node.element_data())
            .and_then(|el| el.sub_doc_data())
    }

    pub fn subdoc_mut(&mut self, node_id: NodeId) -> Option<&mut dyn Document> {
        self.get_node_mut(node_id)
            .and_then(|node| node.element_data_mut())
            .and_then(|el| el.sub_doc_data_mut())
    }

    pub fn is_animating(&self) -> bool {
        #[cfg(feature = "custom-widget")]
        let custom_widget_is_animating = self.custom_widget_nodes.iter().any(|&node_id| {
            self.nodes[node_id]
                .element_data()
                .and_then(|el| el.custom_widget_data())
                .is_some_and(|data| data.widget.requires_redraw())
        });
        #[cfg(not(feature = "custom-widget"))]
        let custom_widget_is_animating = false;

        self.has_canvas
            | self.has_active_animations
            | self.subdoc_is_animating
            | custom_widget_is_animating
            | (self.scroll_animation != ScrollAnimationState::None)
            | self.scrollbars_animating()
    }

    /// Record pending [`DeviceChanges`] and request a redraw so that they are
    /// flushed (via [`Self::flush_pending_device_changes`]) on the next resolve.
    pub(crate) fn queue_device_changes(&mut self, changes: DeviceChanges) {
        if changes.is_empty() {
            return;
        }
        self.pending_device_changes |= changes;
        self.shell_provider.request_redraw();
    }

    /// Apply any pending device changes to the stylist, coalescing all changes
    /// since the last flush into a single device rebuild.
    pub(crate) fn flush_pending_device_changes(&mut self) {
        let changes = std::mem::take(&mut self.pending_device_changes);
        if changes.is_empty() {
            return;
        }

        self.set_stylist_device(make_device(
            &self.viewport,
            self.media_type.clone(),
            self.font_ctx.clone(),
        ));
        self.scroll_viewport_by(0.0, 0.0); // Clamp scroll offset

        // Text is shaped at a specific scale factor, so cached inline layouts
        // must be invalidated when the scale changes.
        if changes.contains(DeviceChanges::SCALE) {
            self.invalidate_inline_contexts();
        }

        // Color-scheme changes affect values that are resolved at cascade time
        // (`light-dark()`, system colors) without necessarily flipping any
        // media query result, so conservatively recascade the whole tree.
        if changes.contains(DeviceChanges::COLOR_SCHEME) {
            if let Some(root_id) = self.try_root_element().map(|el| el.id) {
                self.nodes[root_id].set_restyle_hint(RestyleHint::recascade_subtree());
            }
        }
    }

    /// Update the device and reset the stylist to process the new size
    pub fn set_stylist_device(&mut self, device: Device) {
        // Seed the new device with the root element's current style and font-relative
        // unit state (used to resolve rem/rlh/rex/rch/rcap/ric units). Stylo only
        // updates this state when the root element's style *changes* during a restyle,
        // so a freshly-built device would otherwise resolve these units against the
        // default font-size (16px) until the root's font-size next changes.
        let root_styles = self
            .try_root_element()
            .and_then(|root| root.primary_styles());
        if let Some(root_style) = root_styles.as_deref() {
            device.set_root_style(root_style);

            let font = root_style.get_font();
            let font_size = font.clone_font_size().computed_size();
            device.set_root_font_size(root_style.effective_zoom.unzoom(font_size.px()));

            let line_height = device
                .calc_line_height(font, root_style.writing_mode, None)
                .0;
            device.set_root_line_height(root_style.effective_zoom.unzoom(line_height.px()));
        }
        drop(root_styles);

        let old_device = self.stylist.device();
        let viewport_size_changed = old_device.au_viewport_size() != device.au_viewport_size();
        let styles_use_viewport_units = old_device.used_viewport_size();

        let origins = {
            let guard = &self.guard;
            let guards = StylesheetGuards {
                author: &guard.read(),
                ua_or_user: &guard.read(),
            };
            self.stylist.set_device(device, &guards)
        };

        // Only fully invalidate element styles when the media query results of
        // some origin actually changed. `force_stylesheet_origins_dirty` fully
        // invalidates styles even when `origins` is empty, which would force a
        // full restyle on every viewport resize.
        if !origins.is_empty() {
            self.stylist.force_stylesheet_origins_dirty(origins);
        }

        // Styles that resolve viewport units (vw/vh/etc) still need recomputing
        // when the viewport size changes, even if no media query results
        // changed. Invalidate just the elements whose styles use viewport
        // units (tracked via per-element computed value flags).
        if viewport_size_changed && styles_use_viewport_units {
            if self
                .stylist
                .get_custom_property_initial_values_flags()
                .intersects(ComputedValueFlags::USES_VIEWPORT_UNITS)
            {
                self.stylist.rebuild_initial_values_for_custom_properties();
            }
            if let Some(root) = self.try_root_element() {
                style::invalidation::viewport_units::invalidate(root);
            }
        }
    }

    pub fn stylist_device(&mut self) -> &Device {
        self.flush_pending_device_changes();
        self.stylist.device()
    }

    pub fn get_cursor(&self) -> Option<CursorIcon> {
        // Prefer the precise hit node: `cursor` and `user-select` may be set on
        // a pseudo-element or resolved on an anonymous box, and text hits carry
        // is_text via the hit node. Fall back to the canonical hover node if
        // the hit node has been removed (it is transient across resolves).
        let node_id = self
            .hover_hit_node_id
            .filter(|&id| self.nodes.contains_key(id))
            .or(self.get_hover_node_id())?;
        let node = &self.nodes[node_id];

        if let Some(subdoc) = node.subdoc().map(|doc| doc.inner())
            && subdoc.get_hover_node_id().is_some()
        {
            return subdoc.get_cursor();
        }

        let style = node.primary_styles()?;
        let user_select = style.clone_user_select();
        let keyword = style.clone_cursor().keyword;

        // Return cursor from style if it is non-auto
        if keyword != CursorKind::Auto {
            return stylo_to_cursor_icon(keyword);
        }

        // Return text cursor for text inputs
        if node
            .element_data()
            .is_some_and(|e| e.text_input_data().is_some())
        {
            return Some(CursorIcon::Text);
        }

        // Use "pointer" cursor if any ancestor is a link
        let mut maybe_node = Some(node);
        while let Some(node) = maybe_node {
            if node.is_link() {
                return Some(CursorIcon::Pointer);
            }

            maybe_node = node.layout_parent.get().map(|node_id| node.with(node_id));
        }

        // Return text cursor for text nodes
        if self.hover_node_is_text {
            return Some(match user_select {
                UserSelect::Text | UserSelect::All | UserSelect::Auto | UserSelect::Contain => {
                    CursorIcon::Text
                }
                UserSelect::None => CursorIcon::Default,
            });
        }

        // Else fallback to default cursor
        Some(CursorIcon::Default)
    }

    pub fn viewport_scroll(&self) -> crate::Point<f64> {
        self.nodes.viewport_scroll()
    }

    pub fn set_viewport_scroll(&mut self, scroll: crate::Point<f64>) {
        self.nodes.set_viewport_scroll(scroll);
    }

    /// Find the node targeted by a URL fragment (the `#...` part of a URL).
    ///
    /// Per the HTML spec, this is the element whose `id` matches the fragment, falling
    /// back to the first `<a>` element whose `name` attribute matches.
    pub fn get_fragment_target(&self, fragment: &str) -> Option<NodeId> {
        if let Some(node_id) = self.get_element_by_id(fragment) {
            return Some(node_id);
        }

        // Fall back to a named anchor: `<a name="...">`
        self.nodes.iter().find_map(|(id, node)| {
            let el = node.element_data()?;
            (el.name.local == local_name!("a") && el.attr(local_name!("name")) == Some(fragment))
                .then_some(id)
        })
    }

    /// `node_id`'s document-relative position and unrounded (sub-pixel) layout in physical
    /// coordinates, for CSSOM geometry APIs such as `getBoundingClientRect`.
    #[cfg(not(feature = "writing-mode"))]
    pub(crate) fn physical_unrounded_geometry(
        &self,
        node_id: NodeId,
    ) -> (crate::util::Point<f32>, taffy::Layout) {
        let mut pos = crate::util::Point { x: 0.0, y: 0.0 };
        let mut current = Some(node_id);
        while let Some(id) = current {
            let node = &self.nodes[id];
            pos.x += node.unrounded_layout().location.x - node.scroll_offset().x as f32;
            pos.y += node.unrounded_layout().location.y - node.scroll_offset().y as f32;
            current = node.containing_block();
        }
        (pos, *self.nodes[node_id].unrounded_layout())
    }

    /// Computes the size and position of the `Node` relative to the viewport
    pub fn get_client_bounding_rect(&self, node_id: NodeId) -> Option<BoundingRect> {
        // Non-atomic inline elements have no layout box of their own: return
        // the union of their per-line-box fragment rects.
        if let Some(rects) = self.inline_fragment_rects(node_id) {
            return rects.reduce(BoundingRect::union);
        }

        self.get_node(node_id)?;
        let (pos, layout) = self.physical_unrounded_geometry(node_id);
        let size = layout.size;

        Some(BoundingRect {
            x: snap_to_layout_unit(pos.x as f64 - self.viewport_scroll().x),
            y: snap_to_layout_unit(pos.y as f64 - self.viewport_scroll().y),
            width: snap_to_layout_unit(size.width as f64),
            height: snap_to_layout_unit(size.height as f64),
        })
    }
}
