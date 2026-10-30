use super::*;

bitflags! {
    #[derive(Clone, Copy, PartialEq)]
    pub struct NodeFlags: u32 {
        /// Whether the node is the root node of an Inline Formatting Context
        const IS_INLINE_ROOT = 0b00000001;
        /// Whether the node is the root node of an Table formatting context
        const IS_TABLE_ROOT = 0b00000010;
        /// Whether the node is "in the document" (~= has a parent and isn't a template node)
        const IS_IN_DOCUMENT = 0b00000100;
    }

}

impl NodeFlags {
    #[inline(always)]
    pub fn is_inline_root(&self) -> bool {
        self.contains(Self::IS_INLINE_ROOT)
    }

    #[inline(always)]
    pub fn is_table_root(&self) -> bool {
        self.contains(Self::IS_TABLE_ROOT)
    }

    #[inline(always)]
    pub fn is_in_document(&self) -> bool {
        self.contains(Self::IS_IN_DOCUMENT)
    }

    #[inline(always)]
    pub fn reset_construction_flags(&mut self) {
        self.remove(Self::IS_INLINE_ROOT);
        self.remove(Self::IS_TABLE_ROOT);
    }
}

unsafe impl Send for Node {}

unsafe impl Sync for Node {}

universal_accessors! {
    stylo_element_data / stylo_element_data_mut: StyloData,
    transform / transform_mut: Option<Box<Affine>>,
    display_constructed_as / display_constructed_as_mut: StyloDisplay,
    // The document node is styled/snapshotted like an element, so it also
    // carries these:
    element_state / element_state_mut: ElementState,
    snapshot_handled / snapshot_handled_mut: AtomicBool,
    // `apply_selector_flags` deposits `for_parent()` flags on the parent node,
    // and the parent of the root <html> element is the document -- so the
    // document has to be able to hold selector flags too.
    selector_flags / selector_flags_mut: Cell<ElementSelectorFlags>,
}

impl Node {
    /// This node's layout output state, or a shared default if layout has
    /// never written to this node.
    ///
    /// Panics for node kinds which do not participate in layout (text and
    /// comment nodes).
    #[inline]
    pub fn layout_data(&self) -> &LayoutData {
        match &self.data {
            NodeData::Element(data) | NodeData::AnonymousBlock(data) => data.layout_data(),
            NodeData::Document(data) => data.layout_data(),
            _ => panic!("`layout_data` is not available on this node kind"),
        }
    }

    /// Mutable access to this node's layout output state, allocating it if it
    /// does not yet exist.
    ///
    /// Panics for node kinds which do not participate in layout (text and
    /// comment nodes).
    #[inline]
    pub fn layout_data_mut(&mut self) -> &mut LayoutData {
        match &mut self.data {
            NodeData::Element(data) | NodeData::AnonymousBlock(data) => data.layout_data_mut(),
            NodeData::Document(data) => data.layout_data_mut(),
            _ => panic!("`layout_data` is not available on this node kind"),
        }
    }

    /// Mutable access to this node's layout output state, if it has been
    /// allocated. Never allocates. Returns `None` for node kinds which do not
    /// participate in layout.
    #[inline]
    pub fn try_layout_data_mut(&mut self) -> Option<&mut LayoutData> {
        match &mut self.data {
            NodeData::Element(data) | NodeData::AnonymousBlock(data) => {
                data.layout_data.as_deref_mut()
            }
            NodeData::Document(data) => data.layout_data.as_deref_mut(),
            _ => None,
        }
    }

    /// Clear this node's taffy layout cache without allocating `LayoutData`
    /// for nodes that have never been laid out.
    #[inline]
    pub fn clear_layout_cache(&mut self) {
        if let Some(layout_data) = self.try_layout_data_mut() {
            layout_data.cache.clear();
        }
    }

    /// Clear this node's taffy layout cache and any cached inline
    /// `content_widths`, forcing a full relayout of it on the next pass.
    pub fn invalidate_layout_cache(&mut self) {
        self.clear_layout_cache();
        if let Some(inline_layout) = self
            .data
            .downcast_element_mut()
            .and_then(|el| el.inline_layout_data.as_mut())
        {
            inline_layout.content_widths = None;
        }
    }

    #[inline]
    pub fn cache(&self) -> &Cache {
        &self.layout_data().cache
    }

    #[inline]
    pub fn cache_mut(&mut self) -> &mut Cache {
        &mut self.layout_data_mut().cache
    }

    #[inline]
    pub fn unrounded_layout(&self) -> &Layout {
        &self.layout_data().unrounded_layout
    }

    #[inline]
    pub fn unrounded_layout_mut(&mut self) -> &mut Layout {
        &mut self.layout_data_mut().unrounded_layout
    }

    #[inline]
    pub fn final_layout(&self) -> &Layout {
        &self.layout_data().final_layout
    }

    #[inline]
    pub fn final_layout_mut(&mut self) -> &mut Layout {
        &mut self.layout_data_mut().final_layout
    }

    #[inline]
    pub fn scroll_offset(&self) -> &crate::Point<f64> {
        &self.layout_data().scroll_offset
    }

    #[inline]
    pub fn scroll_offset_mut(&mut self) -> &mut crate::Point<f64> {
        &mut self.layout_data_mut().scroll_offset
    }

    #[inline]
    pub fn scrollable_overflow(&self) -> &KurboRect {
        &self.layout_data().scrollable_overflow
    }

    #[inline]
    pub fn scrollable_overflow_mut(&mut self) -> &mut KurboRect {
        &mut self.layout_data_mut().scrollable_overflow
    }

    /// Style data from stylo, if this node kind carries it (element or document
    /// nodes). Returns `None` for text/comment nodes.
    #[inline]
    pub fn try_stylo_element_data(&self) -> Option<&StyloData> {
        match &self.data {
            NodeData::Element(data) | NodeData::AnonymousBlock(data) => {
                Some(&data.stylo_element_data)
            }
            NodeData::Document(data) => Some(&data.stylo_element_data),
            _ => None,
        }
    }

    #[inline]
    pub fn try_stylo_element_data_mut(&mut self) -> Option<&mut StyloData> {
        match &mut self.data {
            NodeData::Element(data) | NodeData::AnonymousBlock(data) => {
                Some(&mut data.stylo_element_data)
            }
            NodeData::Document(data) => Some(&mut data.stylo_element_data),
            _ => None,
        }
    }

    /// The `dirty_descendants` flag, if this node kind carries it (element or
    /// document nodes). Returns `None` for text/comment nodes.
    #[inline]
    pub(super) fn dirty_descendants_flag(&self) -> Option<&AtomicBool> {
        match &self.data {
            NodeData::Element(data) | NodeData::AnonymousBlock(data) => {
                Some(&data.dirty_descendants)
            }
            NodeData::Document(data) => Some(&data.dirty_descendants),
            _ => None,
        }
    }

    /// The `damaged_descendants` flag, if this node kind carries it (element or
    /// document nodes). Returns `None` for text/comment nodes.
    #[inline]
    pub(super) fn damaged_descendants_flag(&self) -> Option<&AtomicBool> {
        match &self.data {
            NodeData::Element(data) | NodeData::AnonymousBlock(data) => {
                Some(&data.damaged_descendants)
            }
            NodeData::Document(data) => Some(&data.damaged_descendants),
            _ => None,
        }
    }

    /// The document's shared style lock. Only available on element and
    /// document nodes.
    #[inline]
    pub fn guard(&self) -> &SharedRwLock {
        let guard = match &self.data {
            NodeData::Element(data) | NodeData::AnonymousBlock(data) => data.guard.as_ref(),
            NodeData::Document(data) => data.guard.as_ref(),
            _ => None,
        };
        guard.expect("`guard` is not available on this node kind")
    }

    #[inline]
    pub fn has_snapshot(&self) -> bool {
        match &self.data {
            NodeData::Element(data) | NodeData::AnonymousBlock(data) => data.has_snapshot,
            NodeData::Document(data) => data.has_snapshot,
            _ => false,
        }
    }

    #[inline]
    pub fn set_has_snapshot(&mut self, value: bool) {
        match &mut self.data {
            NodeData::Element(data) | NodeData::AnonymousBlock(data) => data.has_snapshot = value,
            NodeData::Document(data) => data.has_snapshot = value,
            _ => {}
        }
    }

    #[inline]
    pub fn before(&self) -> Option<NodeId> {
        self.element_data().and_then(|data| data.before)
    }

    #[inline]
    pub fn after(&self) -> Option<NodeId> {
        self.element_data().and_then(|data| data.after)
    }
}

impl Node {
    pub(crate) fn new(
        tree: *mut crate::NodeTree,
        id: NodeId,
        guard: SharedRwLock,
        mut data: NodeData,
    ) -> Self {
        // Store a handle to the document's shared style lock on the node data.
        // Both element and document nodes are styled by stylo and so need it.
        match &mut data {
            NodeData::Element(data) | NodeData::AnonymousBlock(data) => {
                data.guard = Some(guard);
            }
            NodeData::Document(data) => data.guard = Some(guard),
            _ => {}
        }

        Self {
            tree,

            id,
            parent: None,
            children: ThinVec::new(),
            layout_parent: Cell::new(None),
            oof_containing_block: Cell::new(None),
            layout_children: RefCell::new(None),
            hoisted_children: RefCell::new(ThinVec::new()),
            anonymous_blocks: ThinVec::new(),
            paint_children: RefCell::new(None),
            stacking_context: None,
            sc_contribution_cache: RefCell::new(ThinVec::new()),

            flags: NodeFlags::empty(),
            data,
        }
    }

    pub fn set_transform(&mut self, scale: f32) -> Option<Affine> {
        let transform = self.primary_styles().and_then(|s| {
            let size = self.final_layout().size;
            let reference_box = Rect::new(
                Point2D::new(CSSPixelLength::new(0.0), CSSPixelLength::new(0.0)),
                Size2D::new(
                    CSSPixelLength::new(size.width),
                    CSSPixelLength::new(size.height),
                ),
            );
            // Resolve the transform in CSS pixels, then convert it to device-pixel space
            // (S * T * S^-1): translation components are scaled, linear components are not.
            crate::resolve_2d_transform(s.get_box(), reference_box).map(|t| {
                let scale = scale as f64;
                let [m11, m12, m21, m22, m41, m42] = t.as_coeffs();
                Affine::new([m11, m12, m21, m22, m41 * scale, m42 * scale])
            })
        });

        let slot = self.transform_mut();
        match (slot.as_deref_mut(), transform) {
            (Some(existing), Some(new)) => *existing = new,
            (None, Some(new)) => *slot = Some(Box::new(new)),
            (_, None) => *slot = None,
        }
        transform
    }

    pub fn pe_by_index(&self, index: usize) -> Option<NodeId> {
        match index {
            0 => self.after(),
            1 => self.before(),
            _ => panic!("Invalid pseudo element index"),
        }
    }

    pub fn set_pe_by_index(&mut self, index: usize, value: Option<NodeId>) {
        let Some(data) = self.element_data_mut() else {
            return;
        };
        match index {
            0 => data.after = value,
            1 => data.before = value,
            _ => panic!("Invalid pseudo element index"),
        }
    }

    pub(crate) fn display_style(&self) -> Option<StyloDisplay> {
        let display = self.primary_styles().as_ref()?.clone_display();
        Some(
            if self.element_data().is_some_and(|el| el.is_button_input()) {
                stylo_taffy::convert::button_input_display(display)
            } else {
                display
            },
        )
    }

    pub fn is_or_contains_block(&self) -> bool {
        let style = self.primary_styles();
        let style = style.as_ref();

        // Ignore out-of-flow items
        let position = style
            .map(|s| s.clone_position())
            .unwrap_or(Position::Relative);
        let is_in_flow = matches!(
            position,
            Position::Static | Position::Relative | Position::Sticky
        );
        if !is_in_flow {
            return false;
        }
        // Floated boxes do not break up the inline flow: they participate in the
        // inline formatting context as out-of-flow inline boxes
        let is_floating = style
            .map(|s| s.clone_float().is_floating())
            .unwrap_or(false);
        if is_floating {
            return false;
        }
        let display = self.display_style().unwrap_or(StyloDisplay::inline());
        match display.outside() {
            DisplayOutside::None => false,
            DisplayOutside::Block => true,
            _ => {
                if display.inside() == DisplayInside::Flow {
                    self.children
                        .iter()
                        .copied()
                        .any(|child_id| self.tree()[child_id].is_or_contains_block())
                } else {
                    false
                }
            }
        }
    }

    pub fn is_whitespace_node(&self) -> bool {
        match &self.data {
            NodeData::Text(data) => data.content.chars().all(|c| c.is_ascii_whitespace()),
            _ => false,
        }
    }

    /// Whether this is an empty or whitespace-only text node whose whitespace may be collapsed
    /// away under the `white-space-collapse` it inherits from its parent.
    pub fn is_collapsible_whitespace_node(&self) -> bool {
        let NodeData::Text(data) = &self.data else {
            return false;
        };
        if data.content.is_empty() {
            return true;
        }
        if !data.content.chars().all(|c| c.is_ascii_whitespace()) {
            return false;
        }
        let white_space_collapse = self
            .parent
            .and_then(|parent_id| self.with(parent_id).primary_styles())
            .map(|style| style.clone_white_space_collapse());
        match white_space_collapse {
            Some(WhiteSpaceCollapse::Preserve | WhiteSpaceCollapse::BreakSpaces) => false,
            Some(WhiteSpaceCollapse::PreserveBreaks) => !data.content.contains('\n'),
            Some(WhiteSpaceCollapse::Collapse) | None => true,
        }
    }

    pub fn is_focussable(&self) -> bool {
        self.data
            .downcast_element()
            .map(|el| el.is_focussable)
            .unwrap_or(false)
    }

    pub fn set_restyle_hint(&mut self, hint: RestyleHint) {
        if let Some(stylo_element_data) = self.try_stylo_element_data_mut() {
            if let Some(mut element_data) = stylo_element_data.get_mut() {
                element_data.hint.insert(hint);
            }
        }
        // Mark all ancestors as having dirty descendants so the style traversal
        // will visit this node's subtree
        self.mark_ancestors_dirty();
    }

    /// Returns whether this node has any descendants that need restyling.
    pub fn has_dirty_descendants(&self) -> bool {
        self.dirty_descendants_flag()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    /// Sets the dirty_descendants flag on this node.
    pub fn set_dirty_descendants(&self) {
        if let Some(flag) = self.dirty_descendants_flag() {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Clears the dirty_descendants flag on this node.
    pub fn unset_dirty_descendants(&self) {
        if let Some(flag) = self.dirty_descendants_flag() {
            flag.store(false, Ordering::Relaxed);
        }
    }

    /// Marks all ancestors of this node as having dirty descendants.
    /// This propagates the dirty flag up the tree so that the style traversal
    /// knows to visit the subtree containing this node.
    pub fn mark_ancestors_dirty(&self) {
        let mut current_id = self.parent;
        while let Some(parent_id) = current_id {
            let parent = &self.tree()[parent_id];
            // If this ancestor already has dirty_descendants set, we can stop
            // because all further ancestors must also have it set
            if let Some(flag) = parent.dirty_descendants_flag() {
                if flag.swap(true, Ordering::Relaxed) {
                    break;
                }
            }
            current_id = parent.parent;
        }
    }

    /// Returns whether this node or any of its descendants may carry damage.
    pub fn has_damaged_descendants(&self) -> bool {
        self.damaged_descendants_flag()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    /// Clears the damaged_descendants flag on this node.
    pub fn unset_damaged_descendants(&self) {
        if let Some(flag) = self.damaged_descendants_flag() {
            flag.store(false, Ordering::Relaxed);
        }
    }

    /// Marks this node and all of its ancestors as (potentially) carrying
    /// damage, so that the damage propagation pass visits this node's subtree.
    ///
    /// The invariant is: if a node carries damage (or needs damage-phase
    /// processing such as pseudo-element style syncing), then it and all of
    /// its ancestors have `damaged_descendants` set.
    pub fn mark_damaged(&self) {
        if let Some(flag) = self.damaged_descendants_flag() {
            if flag.swap(true, Ordering::Relaxed) {
                return;
            }
        }
        let mut current_id = self.parent;
        while let Some(parent_id) = current_id {
            let parent = &self.tree()[parent_id];
            // If this ancestor already has damaged_descendants set, we can stop
            // because all further ancestors must also have it set
            if let Some(flag) = parent.damaged_descendants_flag() {
                if flag.swap(true, Ordering::Relaxed) {
                    break;
                }
            }
            current_id = parent.parent;
        }
    }

    // pub fn damage_mut(&mut self) -> Option<&mut RestyleDamage> {
    //     self.stylo_element_data
    //         .get_mut()
    //         .map(|mut data: ElementDataMut<'a>| &'a mut data.damage)
    // }

    pub fn damage(&self) -> Option<RestyleDamage> {
        self.try_stylo_element_data()
            .and_then(|stylo| stylo.get().map(|data| data.damage))
    }

    pub fn set_damage(&mut self, damage: RestyleDamage) {
        if let Some(stylo) = self.try_stylo_element_data_mut() {
            if let Some(mut data) = stylo.get_mut() {
                data.damage = damage;
            }
        }
    }
}
