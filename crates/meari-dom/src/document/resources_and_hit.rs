use super::*;

impl BaseDocument {
    pub fn load_resource(&mut self, res: ResourceLoadResponse) {
        self.pending_critical_resources.remove(&res.request_id);

        let resource = match res.result {
            Ok(resource) => resource,
            Err(err) => {
                if let Some(url) = res.resolved_url.as_ref() {
                    let waiting_nodes = self.pending_images.remove(url).unwrap_or_default();
                    #[cfg(feature = "tracing")]
                    tracing::warn!(
                        url = url.as_str(),
                        waiting_nodes = waiting_nodes.len(),
                        error = err.as_str(),
                        "Resource load failed"
                    );
                    #[cfg(not(feature = "tracing"))]
                    let _ = (waiting_nodes, err);
                } else {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(error = err.as_str(), "Resource load failed (no url)");
                    #[cfg(not(feature = "tracing"))]
                    let _ = err;
                }
                return;
            }
        };

        match resource {
            Resource::Css(css) => {
                let node_id = res.node_id.unwrap();
                self.add_stylesheet_for_node(css, node_id);
            }
            Resource::ImportedCss(import_rule, sheet) => {
                let mut guard = self.guard.write();
                import_rule.write_with(&mut guard).stylesheet =
                    style::stylesheets::import_rule::ImportSheet::Sheet(sheet);
                drop(guard);
                self.stylist
                    .force_stylesheet_origins_dirty(style::stylesheets::OriginSet::all());
            }
            Resource::Image(_kind, width, height, image_data) => {
                // Create the ImageData and cache it
                let image = ImageData::Raster(RasterImageData::new(width, height, image_data));

                let Some(url) = res.resolved_url.as_ref() else {
                    return;
                };

                self.apply_loaded_image(url, image);
            }
            #[cfg(feature = "svg")]
            Resource::Svg(_kind, svg) => {
                // Create the ImageData and cache it
                let image = ImageData::Svg(svg);

                let Some(url) = res.resolved_url.as_ref() else {
                    return;
                };

                self.apply_loaded_image(url, image);
            }
            Resource::DocumentSrc(html) => {
                let Some(node_id) = res.node_id else {
                    return;
                };
                self.apply_iframe_html(node_id, res.request_id, res.resolved_url, &html);
            }
            Resource::Font(bytes, overrides) => {
                let font = Blob::new(Arc::new(bytes));

                // Build a `FontInfoOverride` from the `@font-face` descriptors
                // captured during stylesheet parsing. Without this, parley
                // reads the family name from the TTF's own metadata, which
                // means CSS `font-family: 'Avenir Book'` won't match a font
                // file that internally identifies as `Avenir 45 Book`.
                let weight_override = overrides.weight.map(parley::fontique::FontWeight::new);
                let info_override = parley::fontique::FontInfoOverride {
                    family_name: overrides.family_name.as_deref(),
                    weight: weight_override,
                    style: overrides.style,
                    ..Default::default()
                };

                // TODO: Investigate eliminating double-box
                let mut global_font_ctx = self.font_ctx.lock().unwrap();
                global_font_ctx
                    .collection
                    .register_fonts(font.clone(), Some(info_override));

                #[cfg(feature = "parallel-construct")]
                {
                    rayon::broadcast(|_ctx| {
                        let mut font_ctx = self
                            .thread_font_contexts
                            .get_or(|| RefCell::new(Box::new(global_font_ctx.clone())))
                            .borrow_mut();
                        font_ctx
                            .collection
                            .register_fonts(font.clone(), Some(info_override));
                    });
                }
                drop(global_font_ctx);

                // TODO: see if we can only invalidate if resolved fonts may have changed
                self.invalidate_inline_contexts();
            }
            Resource::None => {
                // Do nothing
            }
        }
    }

    /// Cache a loaded image and apply it to all nodes waiting on it
    /// (`<img>` elements, `background-image` layers and `mask-image` layers).
    pub(super) fn apply_loaded_image(&mut self, url: &str, image: ImageData) {
        // Get all nodes waiting for this image
        let waiting_nodes = self.pending_images.remove(url).unwrap_or_default();

        #[cfg(feature = "tracing")]
        tracing::info!(
            "Image {url} loaded, applying to {} nodes",
            waiting_nodes.len()
        );

        // Cache the image
        self.image_cache.insert(url.to_string(), image.clone());

        // Apply to all waiting nodes
        for (node_id, image_type) in waiting_nodes {
            let Some(node) = self.get_node_mut(node_id) else {
                continue;
            };

            match image_type {
                ImageType::Image => {
                    let element = node.element_data_mut().unwrap();
                    #[cfg(feature = "custom-widget")]
                    if element.custom_widget_data().is_some() {
                        continue;
                    }
                    element.special_data = SpecialElementData::Image(Box::new(image.clone()));

                    // Clear layout cache
                    node.clear_layout_cache();
                    node.insert_damage(ALL_DAMAGE);
                }
                ImageType::Background(idx) | ImageType::Mask(idx) => {
                    let layer_image = node.element_data_mut().and_then(|el| {
                        let images = match image_type {
                            ImageType::Background(_) => &mut el.background_images,
                            ImageType::Mask(_) => &mut el.mask_images,
                            ImageType::Image => unreachable!(),
                        };
                        images.get_mut(idx)
                    });
                    if let Some(Some(layer_image)) = layer_image {
                        layer_image.status = Status::Ok;
                        layer_image.image = image.clone();
                    }
                }
            }
        }
    }

    /// Snapshot the node's pre-mutation state (element state and attributes) ahead of
    /// an attribute mutation, so that the next style traversal can diff selector matches
    /// then-vs-now and invalidate the affected elements.
    pub fn snapshot_node(&mut self, node_id: NodeId) {
        self.snapshot_node_impl(node_id, true)
    }

    /// Snapshot only the node's pre-change [`ElementState`] ahead of a state change
    /// (hover/focus/active/etc). Cheaper than [`Self::snapshot_node`] as it does not
    /// copy attributes or trigger attribute/class/id invalidation work.
    pub fn snapshot_node_state_only(&mut self, node_id: NodeId) {
        self.snapshot_node_impl(node_id, false)
    }

    pub(super) fn snapshot_node_impl(&mut self, node_id: NodeId, capture_attrs: bool) {
        let node = &mut self.nodes[node_id];

        // Do not snapshot nodes that have never been styled. A snapshot records an element's
        // pre-mutation state so a restyle can diff selector matches then-vs-now. An element
        // that has never been styled has no "then" to diff against. Snapshotting it anyway
        // makes Stylo's invalidation unwrap its (absent) primary style and panic.
        let has_been_styled = node.primary_styles().is_some();
        if !has_been_styled {
            return;
        }

        let opaque_node_id = TNode::opaque(&&*node);
        node.set_has_snapshot(true);
        node.snapshot_handled()
            .store(false, std::sync::atomic::Ordering::SeqCst);

        // A snapshot records the element's state/attributes as they were *before the first
        // mutation* since the last style flush (matching Gecko's ServoElementSnapshot
        // semantics): state is captured at most once, and attributes are captured at most
        // once, but a state-only snapshot is upgraded to also capture attributes if an
        // attribute mutation follows.
        let needs_attrs = capture_attrs
            && self
                .snapshots
                .get_mut(&opaque_node_id)
                .is_none_or(|snapshot| snapshot.attrs.is_none());

        let (attrs, changed_attrs) = if needs_attrs {
            let node = &self.nodes[node_id];
            let attrs: Option<Vec<_>> = node.attrs().map(|attrs| {
                attrs
                    .iter()
                    .map(|attr| {
                        let ident = AttrIdentifier {
                            local_name: GenericAtomIdent(attr.name.local.clone()),
                            name: GenericAtomIdent(attr.name.local.clone()),
                            namespace: GenericAtomIdent(attr.name.ns.clone()),
                            prefix: None,
                        };

                        let value = if attr.name.local == local_name!("id") {
                            AttrValue::Atom(Atom::from(&*attr.value))
                        } else if attr.name.local == local_name!("class") {
                            let classes = attr
                                .value
                                .split_ascii_whitespace()
                                .map(Atom::from)
                                .collect();
                            AttrValue::TokenList(OnceLock::from(attr.value.clone()), classes)
                        } else {
                            AttrValue::String(attr.value.clone())
                        };

                        (ident, value)
                    })
                    .collect()
            });

            let changed_attrs: Vec<_> = attrs
                .as_ref()
                .map(|attrs| attrs.iter().map(|attr| attr.0.name.clone()).collect())
                .unwrap_or_default();

            (attrs, changed_attrs)
        } else {
            (None, Vec::new())
        };

        if let Some(snapshot) = self.snapshots.get_mut(&opaque_node_id) {
            // The existing snapshot's state is preserved: it records the state before
            // the *first* change since the last style flush.
            if needs_attrs {
                snapshot.attrs = attrs;
                snapshot.changed_attrs = changed_attrs;
                snapshot.class_changed = true;
                snapshot.id_changed = true;
                snapshot.other_attributes_changed = true;
            }
        } else {
            self.snapshots.insert(
                opaque_node_id,
                ServoElementSnapshot {
                    state: Some(*self.nodes[node_id].element_state()),
                    attrs,
                    changed_attrs,
                    class_changed: needs_attrs,
                    id_changed: needs_attrs,
                    other_attributes_changed: needs_attrs,
                },
            );
        }
    }

    /// Returns whether any style rule depends on any of the given [`ElementState`] bits.
    /// If not, changing those bits cannot affect styling and snapshotting can be skipped.
    pub fn style_depends_on_state(&self, state: ElementState) -> bool {
        self.stylist.iter_origins().any(|(data, _)| {
            data.has_state_dependency(state) || data.has_nth_of_state_dependency(state)
        })
    }

    /// Apply a state change (hover/focus/active/etc affecting the given [`ElementState`]
    /// bits) to a node, taking a state-only snapshot beforehand if any style rule
    /// depends on those bits.
    pub fn snapshot_node_and(
        &mut self,
        node_id: NodeId,
        state: ElementState,
        cb: impl FnOnce(&mut Node),
    ) {
        if self.style_depends_on_state(state) {
            self.snapshot_node_state_only(node_id);
        }
        cb(&mut self.nodes[node_id]);
    }

    // Takes (x, y) co-ordinates (relative to the )
    pub fn hit(&self, x: f32, y: f32) -> Option<HitResult> {
        self.hit_with_scrollbar(x, y).0
    }

    /// The topmost element at viewport coordinates (x, y), or `None` if the point
    /// is outside the viewport. Anonymous boxes and text nodes are resolved to
    /// their nearest element; a point over the background hits the root element.
    ///
    /// This implements the hit-testing semantics of `document.elementFromPoint()`.
    /// Hit testing consults layout, so [`resolve`](Self::resolve) should be called
    /// before this method to ensure layout is up to date.
    pub fn element_from_point(&self, x: f32, y: f32) -> Option<NodeId> {
        let viewport = self.viewport();
        let scale = viewport.scale();
        let (viewport_width, viewport_height) = (
            viewport.window_size.0 as f32 / scale,
            viewport.window_size.1 as f32 / scale,
        );
        if x < 0.0 || y < 0.0 || x > viewport_width || y > viewport_height {
            return None;
        }

        let Some(hit) = self.hit(x, y) else {
            // The point is within the viewport but over no box: the root element
            // (which covers the viewport in an HTML document) is the hit target
            return self.try_root_element().map(|root| root.id);
        };

        // Resolve anonymous boxes / pseudo-elements, then text nodes, to elements
        let mut node_id = self.nearest_non_anonymous_ancestor(hit.node_id)?;
        loop {
            let node = self.get_node(node_id)?;
            if node.is_element() {
                return Some(node_id);
            }
            node_id = node.parent?;
        }
    }

    /// All elements at viewport coordinates (x, y), from topmost to bottommost,
    /// as for `document.elementsFromPoint()`.
    ///
    /// The spec's paint-order list is approximated with the hit element followed
    /// by its ancestor elements (which is correct for non-overlapping content).
    /// As with [`element_from_point`](Self::element_from_point), layout should be
    /// resolved before calling this method.
    pub fn elements_from_point(&self, x: f32, y: f32) -> Vec<NodeId> {
        let mut element_ids = Vec::new();
        let mut current = self.element_from_point(x, y);
        while let Some(node_id) = current {
            element_ids.push(node_id);
            current = self
                .get_node(node_id)
                .and_then(|node| node.parent)
                .filter(|parent_id| {
                    self.get_node(*parent_id)
                        .is_some_and(|node| node.is_element())
                });
        }
        element_ids
    }

    /// Walk up the tree to the nearest DOM node whose id is stable across
    /// box-tree reconstruction, so canonicalized interaction state never goes
    /// stale.
    ///
    /// Layout-generated nodes (anonymous blocks and `::before`/`::after`
    /// pseudo-elements, both stored as anonymous blocks) get new ids on every
    /// reconstruction, so we skip any anonymous node *and* a non-anonymous node
    /// whose parent is anonymous (the pseudo's text content). The first
    /// non-anonymous node with a non-anonymous parent is a real DOM node; the
    /// root element's `Document` parent guarantees termination.
    ///
    /// Returns `None` if `node_id` (or an ancestor) no longer exists.
    pub fn nearest_non_anonymous_ancestor(&self, node_id: NodeId) -> Option<NodeId> {
        // Recurse up the tree keeping a window of the current node and its
        // parent, advancing one step per iteration so each node is looked up
        // exactly once.
        let mut node = self.get_node(node_id)?;
        loop {
            let parent = match node.parent {
                Some(parent_id) => self.get_node(parent_id)?,
                None => return Some(node.id),
            };
            if !node.is_anonymous() && !parent.is_anonymous() {
                return Some(node.id);
            }
            node = parent;
        }
    }

    pub fn focus_next_node(&mut self) -> Option<NodeId> {
        let focussed_node_id = self.get_focussed_node_id()?;
        let id = self.next_node(&self.nodes[focussed_node_id], |node| node.is_focussable())?;
        self.set_focus_to(id);
        Some(id)
    }

    /// Move focus to the previous focussable node in the document
    pub fn focus_prev_node(&mut self) -> Option<NodeId> {
        let focussed_node_id = self.get_focussed_node_id()?;
        let id = self.prev_node(&self.nodes[focussed_node_id], |node| node.is_focussable())?;
        self.set_focus_to(id);
        Some(id)
    }

    /// Clear the focussed node
    pub fn clear_focus(&mut self) {
        if let Some(id) = self.focus_node_id {
            let shell_provider = self.shell_provider.clone();
            self.snapshot_node_and(id, ElementState::FOCUS | ElementState::FOCUSRING, |node| {
                node.blur(shell_provider)
            });
            self.focus_node_id = None;
        }
    }

    pub fn set_mousedown_node_id(&mut self, node_id: Option<NodeId>) {
        self.mousedown_node_id = node_id.and_then(|id| self.nearest_non_anonymous_ancestor(id));
    }

    pub fn set_focus_to(&mut self, focus_node_id: NodeId) -> bool {
        let Some(focus_node_id) = self.nearest_non_anonymous_ancestor(focus_node_id) else {
            return false;
        };
        if Some(focus_node_id) == self.focus_node_id {
            return false;
        }

        #[cfg(feature = "tracing")]
        tracing::info!("Focussed node {focus_node_id}");

        let shell_provider = self.shell_provider.clone();

        // Remove focus from the old node
        if let Some(id) = self.focus_node_id {
            self.snapshot_node_and(id, ElementState::FOCUS | ElementState::FOCUSRING, |node| {
                node.blur(shell_provider.clone())
            });
        }

        // Focus the new node
        self.snapshot_node_and(
            focus_node_id,
            ElementState::FOCUS | ElementState::FOCUSRING,
            |node| node.focus(shell_provider),
        );

        self.focus_node_id = Some(focus_node_id);

        true
    }

    pub fn active_node(&mut self) -> bool {
        let Some(hover_node_id) = self.get_hover_node_id() else {
            return false;
        };

        if let Some(active_node_id) = self.active_node_id {
            if active_node_id == hover_node_id {
                return true;
            }
            self.unactive_node();
        }

        // hover_node_id is canonicalized when stored, so this always holds.
        debug_assert!(
            self.get_node(hover_node_id)
                .is_some_and(|node| !node.is_anonymous()),
            "interaction state must reference DOM nodes, not layout-generated nodes"
        );
        let active_node_id = Some(hover_node_id);

        let node_path = self.maybe_node_element_ancestors(active_node_id);
        for &id in node_path.iter() {
            self.snapshot_node_and(id, ElementState::ACTIVE, |node| node.active());
        }

        self.active_node_id = active_node_id;

        true
    }

    pub fn unactive_node(&mut self) -> bool {
        let Some(active_node_id) = self.active_node_id.take() else {
            return false;
        };

        let node_path = self.maybe_node_element_ancestors(Some(active_node_id));
        for &id in node_path.iter() {
            self.snapshot_node_and(id, ElementState::ACTIVE, |node| node.unactive());
        }

        true
    }

    /// The scrollbar thumb currently under the pointer, if any.
    pub fn hovered_scrollbar(&self) -> Option<crate::node::ScrollbarRef> {
        self.hovered_scrollbar
    }

    /// The scrollbar thumb currently being dragged, if any.
    pub fn scrollbar_drag_target(&self) -> Option<crate::node::ScrollbarRef> {
        match &self.drag_mode {
            DragMode::ScrollbarDrag(state) => Some(state.scrollbar),
            _ => None,
        }
    }

    /// The current opacity of `node_id`'s overlay scrollbars. They show at
    /// full opacity on scroll and fade out after a delay (Chromium's overlay
    /// timings); the pointer resting on a thumb, or dragging it, holds them
    /// visible.
    pub fn scrollbar_opacity(&self, node_id: NodeId) -> f32 {
        let interacting = |scrollbar: &crate::node::ScrollbarRef| scrollbar.node_id == node_id;
        if self.hovered_scrollbar.as_ref().is_some_and(interacting)
            || self
                .scrollbar_drag_target()
                .as_ref()
                .is_some_and(interacting)
        {
            return 1.0;
        }
        self.scrollbar_activity.get(&node_id).map_or(0.0, |last| {
            crate::node::scrollbar::opacity_at(last.elapsed())
        })
    }

    /// Show `node_id`'s overlay scrollbars at full opacity and restart their
    /// fade-out delay.
    pub(crate) fn show_scrollbars(&mut self, node_id: NodeId) {
        if cfg!(feature = "scrollbars") {
            self.scrollbar_activity.insert(node_id, Instant::now());
        }
    }
}
