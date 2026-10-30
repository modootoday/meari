use super::*;

/*
-> Computed styles
-> Layout
-----> Needs to happen only when styles are computed
*/

// type DomRefCell<T> = RefCell<T>;

// pub struct DomData {
//     // ... we can probs just get away with using the html5ever types directly. basically just using the servo dom, but without the bindings
//     local_name: html5ever::LocalName,
//     tag_name: html5ever::QualName,
//     namespace: html5ever::Namespace,
//     prefix: DomRefCell<Option<html5ever::Prefix>>,
//     attrs: DomRefCell<Vec<Attr>>,
//     // attrs: DomRefCell<Vec<Dom<Attr>>>,
//     id_attribute: DomRefCell<Option<Atom>>,
//     is: DomRefCell<Option<LocalName>>,
//     // style_attribute: DomRefCell<Option<Arc<Locked<PropertyDeclarationBlock>>>>,
//     // attr_list: MutNullableDom<NamedNodeMap>,
//     // class_list: MutNullableDom<DOMTokenList>,
//     state: Cell<ElementState>,
// }

impl Node {
    /// The `order` sort key of this node within a flex/grid container's
    /// `layout_children`. The list is stable-sorted by this key when the
    /// container's children are constructed (`collect_layout_children`) and
    /// re-collected when a child's `order` changes (`REORDER_CHILDREN` damage).
    ///
    /// ::before/::after pseudos are flex/grid items and honor `order`. They
    /// sit first/last in the constructed list, and the sort is stable, so
    /// ties keep ::before first and ::after last. Out-of-flow children are
    /// not flex/grid items, so they key as 0 and keep source order.
    pub fn order(&self) -> i32 {
        self.primary_styles()
            .filter(|s| !matches!(s.get_box().position, Position::Absolute | Position::Fixed))
            .map(|s| s.clone_order())
            .unwrap_or(0)
    }

    pub fn z_index(&self) -> i32 {
        self.primary_styles()
            .map(|s| s.clone_z_index().integer_or(0))
            .unwrap_or(0)
    }

    // https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_positioned_layout/Stacking_context#features_creating_stacking_contexts
    pub fn is_stacking_context_root(&self, is_flex_or_grid_item: bool) -> bool {
        let Some(style) = self.primary_styles() else {
            return false;
        };

        let position = style.clone_position();
        let has_z_index = !style.clone_z_index().is_auto();

        if style.clone_opacity() != 1.0 {
            return true;
        }

        let position_based = match position {
            Position::Fixed | Position::Sticky => true,
            Position::Relative | Position::Absolute => has_z_index,
            Position::Static => has_z_index && is_flex_or_grid_item,
        };
        if position_based {
            return true;
        }

        let box_style = style.get_box();
        if !box_style.transform.0.is_empty()
            || !matches!(box_style.rotate, Rotate::None)
            || !matches!(box_style.scale, Scale::None)
            || !matches!(box_style.translate, Translate::None)
        {
            return true;
        }

        if self.applies_atomic_paint_effect() {
            return true;
        }

        if style.get_box().isolation == Isolation::Isolate {
            return true;
        }

        // TODO: mix-blend-mode
        // TODO: contain

        false
    }

    /// Whether this node's styles apply an atomic paint effect (opacity, filter,
    /// clip-path, mask) to its subtree.
    pub(crate) fn applies_atomic_paint_effect(&self) -> bool {
        use style::values::computed::basic_shape::ClipPath;
        use style::values::generics::image::GenericImage;

        let Some(style) = self.primary_styles() else {
            return false;
        };

        if style.clone_opacity() != 1.0 {
            return true;
        }

        let effects = style.get_effects();
        if !effects.filter.0.is_empty() {
            return true;
        }

        if !matches!(style.clone_clip_path(), ClipPath::None) {
            return true;
        }

        style
            .get_svg()
            .mask_image
            .0
            .iter()
            .any(|image| !matches!(image, GenericImage::None))
    }

    /// Takes an (x, y) position (relative to the *parent's* top-left corner) and returns:
    ///    - None if the position is outside of this node's bounds
    ///    - Some(HitResult) if the position is within the node but doesn't match any children
    ///    - The result of recursively calling child.hit() on the the child element that is
    ///      positioned at that position if there is one.
    ///
    /// TODO: z-index
    /// (If multiple children are positioned at the position then a random one will be recursed into)
    pub fn hit(&self, x: f32, y: f32, scale: f64) -> Option<HitResult> {
        self.hit_inner(x, y, scale, &mut None)
    }

    /// [`hit`](Self::hit), also resolving the innermost overlay scrollbar
    /// thumb under the point into `scrollbar` during the same descent (so
    /// thumb hit-testing shares the exact coordinate handling — transforms
    /// included — of every other hit test).
    pub(crate) fn hit_inner(
        &self,
        x: f32,
        y: f32,
        scale: f64,
        scrollbar: &mut Option<crate::node::ScrollbarRef>,
    ) -> Option<HitResult> {
        use style::computed_values::pointer_events::T as PointerEvents;
        use style::computed_values::visibility::T as Visibility;

        // Don't hit on visbility:hidden elements
        if let Some(style) = self.primary_styles() {
            if matches!(
                style.clone_visibility(),
                Visibility::Hidden | Visibility::Collapse
            ) {
                return None;
            }
        }

        // pointer-events:none makes this element transparent to hits, but its
        // descendants are still tested (one may restore pointer-events:auto).
        let pointer_events_none = self
            .primary_styles()
            .is_some_and(|style| style.clone_pointer_events() == PointerEvents::None);

        let mut x = x - self.final_layout().location.x + self.scroll_offset().x as f32;
        let mut y = y - self.final_layout().location.y + self.scroll_offset().y as f32;

        if let Some(t) = self.transform().as_deref() {
            let p = t.inverse() * kurbo::Point::new(x as f64 * scale, y as f64 * scale);
            x = (p.x / scale) as f32;
            y = (p.y / scale) as f32;
        }

        let size = self.final_layout().size;
        let matches_self = !(x < 0.0
            || x > size.width + self.scroll_offset().x as f32
            || y < 0.0
            || y > size.height + self.scroll_offset().y as f32);

        let overflow_rect = self.final_layout().scrollable_overflow_rect;
        let matches_content = !(x < 0.0
            || x > overflow_rect.right + self.scroll_offset().x as f32
            || y < 0.0
            || y > overflow_rect.bottom + self.scroll_offset().y as f32);

        let matches_hoisted_content = match &self.stacking_context {
            Some(sc) => {
                // Both the point (after the scroll adjustment above) and the
                // bbox are in this node's unscrolled content coordinates.
                let bbox = sc.hoisted_content_bbox(self.tree(), self.id, scale);
                x >= bbox.left && x <= bbox.right && y >= bbox.top && y <= bbox.bottom
            }
            None => false,
        };

        // `scrollable_overflow` is stored in device (scaled) pixels, whereas the
        // coordinates here are in CSS pixels, so unscale it before comparing.
        let overflow = *self.scrollable_overflow();

        let matches_overflow = x >= (overflow.x0 / scale) as f32
            && x <= (overflow.x1 / scale) as f32
            && y >= (overflow.y0 / scale) as f32
            && y <= (overflow.y1 / scale) as f32;

        if !matches_self && !matches_content && !matches_hoisted_content && !matches_overflow {
            return None;
        }

        // Descendants overwrite, so the innermost scroll container's thumb
        // wins. Thumb coords are border-box relative (unscrolled).
        if matches_self
            && let Some(sb) = self.scrollbar_at_local(
                (x - self.scroll_offset().x as f32) as f64,
                (y - self.scroll_offset().y as f32) as f64,
            )
        {
            *scrollbar = Some(sb);
        }

        let content_box_offset = taffy::Point {
            x: self.final_layout().padding.left + self.final_layout().border.left,
            y: self.final_layout().padding.top + self.final_layout().border.top,
        };
        if self.flags.is_inline_root() {
            x -= content_box_offset.x;
            y -= content_box_offset.y;
        }

        // Positive z_index hoisted children
        if matches_hoisted_content {
            if let Some(hoisted) = &self.stacking_context {
                for hoisted_child in hoisted.pos_z_hoisted_children().rev() {
                    let pos = hoisted_child.position(self.tree(), self.id);
                    let x = x - pos.x;
                    let y = y - pos.y;
                    if let Some(hit) = self
                        .with(hoisted_child.node_id)
                        .hit_inner(x, y, scale, scrollbar)
                    {
                        return Some(hit);
                    }
                }
            }
        }

        // Call `.hit()` on each child in turn. If any return `Some` then return that value. Else return `Some(self.id).
        for child_id in self.paint_children.borrow().iter().flatten().rev() {
            let child = self.with(*child_id);
            let child_position = child.taffy_position();
            let mut child_x = x;
            let mut child_y = y;
            if child_position.is_out_of_flow() {
                // Out-of-flow children's layout location is relative to this node's
                // border box, so undo the inline-root content-box offset applied above
                if self.flags.is_inline_root() {
                    child_x += content_box_offset.x;
                    child_y += content_box_offset.y;
                }
                // Fixed-position children of the root element are positioned
                // against the viewport and do not scroll with it. (A fixed box
                // whose containing block is a transformed ancestor scrolls with
                // that ancestor like any other out-of-flow box.)
                if child_position == taffy::Position::Fixed && self.containing_block().is_none() {
                    child_x -= self.scroll_offset().x as f32;
                    child_y -= self.scroll_offset().y as f32;
                    let viewport_scroll = self.tree().viewport_scroll();
                    child_x -= viewport_scroll.x as f32;
                    child_y -= viewport_scroll.y as f32;
                }
            }
            if let Some(hit) = child.hit_inner(child_x, child_y, scale, scrollbar) {
                return Some(hit);
            }
        }

        // Negative z_index hoisted children
        if matches_hoisted_content {
            if let Some(hoisted) = &self.stacking_context {
                for hoisted_child in hoisted.neg_z_hoisted_children().rev() {
                    let pos = hoisted_child.position(self.tree(), self.id);
                    let x = x - pos.x;
                    let y = y - pos.y;
                    if let Some(hit) = self
                        .with(hoisted_child.node_id)
                        .hit_inner(x, y, scale, scrollbar)
                    {
                        return Some(hit);
                    }
                }
            }
        }

        // Inline children
        if self.flags.is_inline_root() {
            let element_data = &self.element_data().unwrap();
            if let Some(ild) = element_data.inline_layout_data.as_ref() {
                let layout = &ild.layout;
                let scale = layout.scale();
                let y = y - ild.block_offset;

                if let Some((cluster, _side)) =
                    Cluster::from_point_exact(layout, x * scale, y * scale)
                {
                    let node_id = cluster.style().brush.id;
                    let text_pointer_events_none = self
                        .with(node_id)
                        .primary_styles()
                        .is_some_and(|style| style.clone_pointer_events() == PointerEvents::None);
                    if !text_pointer_events_none {
                        return Some(HitResult {
                            node_id,
                            x,
                            y,
                            is_text: true,
                        });
                    }
                }
            }
        }

        // Self (this node)
        if matches_self && !pointer_events_none {
            return Some(HitResult {
                node_id: self.id,
                x,
                y,
                is_text: false,
            });
        }

        None
    }

    /// Find the inline root ancestor of this node (or self if this is an inline root).
    /// Returns None if no inline root ancestor exists.
    pub fn inline_root_ancestor(&self) -> Option<&Node> {
        let mut node = self;
        loop {
            if node.flags.is_inline_root() {
                return Some(node);
            }
            let id = node.layout_parent.get()?;
            node = self.with(id);
        }
    }

    /// Get the text byte offset at a given point, using coordinates already transformed
    /// to be relative to this inline root's content box.
    /// Returns Some(byte_offset) if the point hits text, None otherwise.
    pub fn text_offset_at_point(&self, x: f32, y: f32) -> Option<usize> {
        if !self.flags.is_inline_root() {
            return None;
        }

        let element_data = self.element_data()?;
        let inline_layout = element_data.inline_layout_data.as_ref()?;
        let layout = &inline_layout.layout;
        let scale = layout.scale();
        let y = y - inline_layout.block_offset;

        // Use Parley's cluster hit testing (from_point is more forgiving than from_point_exact)
        let (cluster, side) = Cluster::from_point(layout, x * scale, y * scale)?;

        // Determine byte offset based on which side of the cluster was clicked
        // For LTR text: left side = start of cluster, right side = end of cluster
        // For RTL text: left side = end of cluster, right side = start of cluster
        // Also, explicit line breaks should always use start to avoid cursor appearing on next line
        let is_leading = side == ClusterSide::Left;
        let offset = if cluster.is_rtl() {
            if is_leading {
                cluster.text_range().end
            } else {
                cluster.text_range().start
            }
        } else {
            // LTR text
            if is_leading || cluster.is_line_break() == Some(BreakReason::Explicit) {
                cluster.text_range().start
            } else {
                cluster.text_range().end
            }
        };

        Some(offset)
    }

    /// Whether this node is a non-atomic inline element: one that has no layout box of its
    /// own because it is flattened into the containing inline root's text layout as a style
    /// span.
    pub fn is_non_atomic_inline(&self) -> bool {
        let Some(element) = self.element_data() else {
            return false;
        };
        if self.flags.is_inline_root()
            || crate::layout::replaced::is_inline_box_element(&element.name.local)
        {
            return false;
        }
        self.primary_styles().is_some_and(|styles| {
            let display = styles.clone_display();
            display.outside() == DisplayOutside::Inline && display.inside() == DisplayInside::Flow
        })
    }

    /// The node whose box this node's `Layout.location` is relative to: the
    /// `oof_containing_block` for a hoisted out-of-flow box, otherwise the
    /// `layout_parent`.
    pub fn containing_block(&self) -> Option<NodeId> {
        self.oof_containing_block
            .get()
            .or_else(|| self.layout_parent.get())
    }

    /// Computes the Document-relative coordinates of the `Node`
    pub fn absolute_position(&self, x: f32, y: f32) -> crate::util::Point<f32> {
        let x = x + self.final_layout().location.x - self.scroll_offset().x as f32;
        let y = y + self.final_layout().location.y - self.scroll_offset().y as f32;

        // Recurse up the positioning hierarchy
        self.containing_block()
            .map(|i| self.with(i).absolute_position(x, y))
            .unwrap_or(crate::util::Point { x, y })
    }

    /// Whether this node can act as an [`offset_parent`](Self::offset_parent): a positioned
    /// element, or one of the elements that always qualify (`body`, `td`, `th`).
    pub(crate) fn is_offset_parent(&self) -> bool {
        let Some(styles) = self.primary_styles() else {
            return false;
        };
        if styles.get_box().position != Position::Static {
            return true;
        }
        self.data.is_element_with_tag_name(&local_name!("body"))
            || self.data.is_element_with_tag_name(&local_name!("td"))
            || self.data.is_element_with_tag_name(&local_name!("th"))
    }

    /// Whether this node is a non-positioned `body` element. When such an element is the
    /// `offsetParent`, `offsetLeft`/`offsetTop` are measured from the initial containing
    /// block origin rather than from the `body`'s padding edge.
    pub(crate) fn is_static_body(&self) -> bool {
        self.data.is_element_with_tag_name(&local_name!("body"))
            && self
                .primary_styles()
                .is_some_and(|styles| styles.get_box().position == Position::Static)
    }

    /// The nearest layout ancestor that [is an offset parent](Self::is_offset_parent), as in
    /// CSSOM View's `offsetParent`.
    pub fn offset_parent(&self) -> Option<&Node> {
        let mut node = self;
        loop {
            node = self.with(node.containing_block()?);
            if node.is_offset_parent() {
                return Some(node);
            }
        }
    }
}
