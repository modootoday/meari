use super::*;

impl Node {
    pub fn insert_damage(&mut self, damage: RestyleDamage) {
        if let Some(stylo) = self.try_stylo_element_data_mut() {
            if let Some(mut data) = stylo.get_mut() {
                data.damage |= damage;
            }
        }
        if !damage.is_empty() {
            self.mark_damaged();
        }
    }

    pub fn remove_damage(&mut self, damage: RestyleDamage) {
        if let Some(stylo) = self.try_stylo_element_data_mut() {
            if let Some(mut data) = stylo.get_mut() {
                data.damage.remove(damage);
            }
        }
    }

    pub fn clear_damage_mut(&mut self) {
        if let Some(stylo) = self.try_stylo_element_data_mut() {
            if let Some(mut data) = stylo.get_mut() {
                data.damage = RestyleDamage::empty();
            }
        }
    }

    // State changes (hover/focus/active/disabled) do not set a restyle hint.
    // Invalidation is driven by element snapshots: the style traversal diffs the
    // snapshotted (pre-change) state against the current state and invalidates
    // only the elements matched by selectors that depend on the changed state
    // bits. Ancestors are marked dirty so the traversal reaches this node.
    pub fn hover(&mut self) {
        if let Some(data) = self.element_data_mut() {
            data.element_state.insert(ElementState::HOVER);
        }
        self.mark_ancestors_dirty();
    }

    pub fn unhover(&mut self) {
        if let Some(data) = self.element_data_mut() {
            data.element_state.remove(ElementState::HOVER);
        }
        self.mark_ancestors_dirty();
    }

    pub fn is_hovered(&self) -> bool {
        self.element_data()
            .is_some_and(|data| data.element_state.contains(ElementState::HOVER))
    }

    pub fn focus(&mut self, shell_provider: Arc<dyn ShellProvider>) {
        if let Some(data) = self.element_data_mut() {
            data.element_state
                .insert(ElementState::FOCUS | ElementState::FOCUSRING);
        }
        self.mark_ancestors_dirty();

        // If focussing a text input, enable IME and set IME area
        if self
            .element_data()
            .and_then(|elem| elem.text_input_data())
            .is_some()
        {
            shell_provider.set_ime_enabled(true);
            let mut pos = self.absolute_position(0.0, 0.0);
            pos.x += self.final_layout().content_box_x();
            pos.y += self.final_layout().content_box_y();
            let width = self.final_layout().content_box_width();
            let height = self.final_layout().content_box_height();
            shell_provider.set_ime_cursor_area(pos.x, pos.y, width, height);
        }
    }

    pub fn blur(&mut self, shell_provider: Arc<dyn ShellProvider>) {
        if let Some(data) = self.element_data_mut() {
            data.element_state
                .remove(ElementState::FOCUS | ElementState::FOCUSRING);
        }
        self.mark_ancestors_dirty();

        // If blurring a text input, disable IME
        if self
            .element_data()
            .and_then(|elem| elem.text_input_data())
            .is_some()
        {
            shell_provider.set_ime_enabled(false);
        }
    }

    pub fn is_focussed(&self) -> bool {
        self.element_data()
            .is_some_and(|data| data.element_state.contains(ElementState::FOCUS))
    }

    pub fn active(&mut self) {
        if let Some(data) = self.element_data_mut() {
            data.element_state.insert(ElementState::ACTIVE);
        }
        self.mark_ancestors_dirty();
    }

    pub fn unactive(&mut self) {
        if let Some(data) = self.element_data_mut() {
            data.element_state.remove(ElementState::ACTIVE);
        }
        self.mark_ancestors_dirty();
    }

    pub fn is_active(&self) -> bool {
        self.element_data()
            .is_some_and(|data| data.element_state.contains(ElementState::ACTIVE))
    }

    // Marks the node as disabled if it can be.
    // It does not disable any children which should be disabled as well (relevant for the `select` element).
    pub fn disable(&mut self) {
        if let Some(data) = self.element_data_mut() {
            if data.can_be_disabled() {
                data.element_state.insert(ElementState::DISABLED);
                data.element_state.remove(ElementState::ENABLED);
            }
        }
        self.mark_ancestors_dirty();
    }

    // Marks the node as enabled if it can be.
    // It does not enable any children which should be enabled as well (relevant for the `select` element).
    pub fn enable(&mut self) {
        if let Some(data) = self.element_data_mut() {
            if data.can_be_disabled() {
                data.element_state.insert(ElementState::ENABLED);
                data.element_state.remove(ElementState::DISABLED);
            }
        }
        self.mark_ancestors_dirty();
    }

    pub fn subdoc(&self) -> Option<&dyn Document> {
        self.element_data().and_then(|el| el.sub_doc_data())
    }

    pub fn subdoc_mut(&mut self) -> Option<&mut dyn Document> {
        self.element_data_mut().and_then(|el| el.sub_doc_data_mut())
    }

    pub fn text_input_v_centering_offset(&self, scale: f64) -> f64 {
        // For single-line inputs, add an offset to vertically center the text input layout
        // within the content box of it's node.
        if let Some(input_data) = self
            .data
            .downcast_element()
            .and_then(|el| el.text_input_data())
        {
            if !input_data.is_multiline {
                let content_box_height = self.final_layout().content_box_height();
                let input_height = input_data.editor.try_layout().unwrap().height() / scale as f32;
                let y_offset = ((content_box_height - input_height) / 2.0).max(0.0);

                return y_offset as f64;
            }
        }

        0.0
    }
}

impl NodeData {
    pub fn downcast_element(&self) -> Option<&ElementData> {
        match self {
            Self::Element(data) => Some(data),
            Self::AnonymousBlock(data) => Some(data),
            _ => None,
        }
    }

    pub fn downcast_element_mut(&mut self) -> Option<&mut ElementData> {
        match self {
            Self::Element(data) => Some(data),
            Self::AnonymousBlock(data) => Some(data),
            _ => None,
        }
    }

    pub fn is_element_with_tag_name(&self, name: &impl PartialEq<LocalName>) -> bool {
        let Some(elem) = self.downcast_element() else {
            return false;
        };
        *name == elem.name.local
    }

    pub fn attrs(&self) -> Option<&[Attribute]> {
        Some(&self.downcast_element()?.attrs)
    }

    pub fn attr(&self, name: impl PartialEq<LocalName>) -> Option<&str> {
        self.downcast_element()?.attr(name)
    }

    pub fn has_attr(&self, name: impl PartialEq<LocalName>) -> bool {
        self.downcast_element()
            .is_some_and(|elem| elem.has_attr(name))
    }

    pub fn kind(&self) -> NodeKind {
        match self {
            NodeData::Document(_) => NodeKind::Document,
            NodeData::Element(_) => NodeKind::Element,
            NodeData::AnonymousBlock(_) => NodeKind::AnonymousBlock,
            NodeData::Text(_) => NodeKind::Text,
            NodeData::Comment { .. } => NodeKind::Comment,
        }
    }
}

impl TextNodeData {
    pub fn new(content: String) -> Self {
        Self { content }
    }
}

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
    pub fn tree(&self) -> &crate::NodeTree {
        unsafe { &*self.tree }
    }

    #[track_caller]
    pub fn with(&self, id: NodeId) -> &Node {
        self.tree().get(id).unwrap()
    }

    pub fn print_tree(&self, level: usize) {
        println!(
            "{} {} {:?} {} {:?}",
            "  ".repeat(level),
            self.id,
            self.parent,
            self.node_debug_str().replace('\n', ""),
            self.children
        );
        // println!("{} {:?}", "  ".repeat(level), self.children);
        for child_id in self.children.iter() {
            let child = self.with(*child_id);
            child.print_tree(level + 1)
        }
    }

    // Get the index of the current node in the parents child list
    pub fn index_of_child(&self, child_id: NodeId) -> Option<usize> {
        self.children.iter().position(|id| *id == child_id)
    }

    // Get the index of the current node in the parents child list
    pub fn child_index(&self) -> Option<usize> {
        self.tree()[self.parent?]
            .children
            .iter()
            .position(|id| *id == self.id)
    }

    // Get the nth node in the parents child list
    pub fn forward(&self, n: usize) -> Option<&Node> {
        let child_idx = self.child_index().unwrap_or(0);
        self.tree()[self.parent?]
            .children
            .get(child_idx + n)
            .map(|id| self.with(*id))
    }

    pub fn backward(&self, n: usize) -> Option<&Node> {
        let child_idx = self.child_index().unwrap_or(0);
        if child_idx < n {
            return None;
        }

        self.tree()[self.parent?]
            .children
            .get(child_idx - n)
            .map(|id| self.with(*id))
    }

    pub fn is_element(&self) -> bool {
        matches!(self.data, NodeData::Element { .. })
    }

    pub fn is_anonymous(&self) -> bool {
        matches!(self.data, NodeData::AnonymousBlock { .. })
    }

    pub fn is_text_node(&self) -> bool {
        matches!(self.data, NodeData::Text { .. })
    }

    pub fn element_data(&self) -> Option<&ElementData> {
        match self.data {
            NodeData::Element(ref data) => Some(data),
            NodeData::AnonymousBlock(ref data) => Some(data),
            _ => None,
        }
    }

    pub fn element_data_mut(&mut self) -> Option<&mut ElementData> {
        match self.data {
            NodeData::Element(ref mut data) => Some(data),
            NodeData::AnonymousBlock(ref mut data) => Some(data),
            _ => None,
        }
    }

    pub fn text_data(&self) -> Option<&TextNodeData> {
        match self.data {
            NodeData::Text(ref data) => Some(data),
            _ => None,
        }
    }

    pub fn text_data_mut(&mut self) -> Option<&mut TextNodeData> {
        match self.data {
            NodeData::Text(ref mut data) => Some(data),
            _ => None,
        }
    }

    pub fn node_debug_str(&self) -> String {
        let mut s = String::new();

        match &self.data {
            NodeData::Document(_) => write!(s, "DOCUMENT"),
            // NodeData::Doctype { name, .. } => write!(s, "DOCTYPE {name}"),
            NodeData::Text(data) => {
                let bytes = data.content.as_bytes();
                write!(
                    s,
                    "TEXT {}",
                    std::str::from_utf8(bytes.split_at(10.min(bytes.len())).0)
                        .unwrap_or("INVALID UTF8")
                )
            }
            NodeData::Comment { .. } => write!(s, "COMMENT"),
            NodeData::AnonymousBlock(_) => write!(s, "AnonymousBlock"),
            NodeData::Element(data) => {
                let name = &data.name;
                let class = self.attr(local_name!("class")).unwrap_or("");
                let id = self.attr(local_name!("id")).unwrap_or("");
                let display = self.display_constructed_as().to_css_string();
                write!(s, "<{}", name.local).unwrap();
                if !id.is_empty() {
                    write!(s, " #{id}").unwrap();
                }
                if !class.is_empty() {
                    if class.contains(' ') {
                        write!(s, " class=\"{class}\"").unwrap()
                    } else {
                        write!(s, " .{class}").unwrap()
                    }
                }
                write!(s, "> ({display})")
            } // NodeData::ProcessingInstruction { .. } => write!(s, "ProcessingInstruction"),
        }
        .unwrap();
        s
    }

    pub fn attrs(&self) -> Option<&[Attribute]> {
        Some(&self.element_data()?.attrs)
    }

    pub fn attr(&self, name: LocalName) -> Option<&str> {
        let attr = self.attrs()?.iter().find(|id| id.name.local == name)?;
        Some(&attr.value)
    }

    pub fn primary_styles(&self) -> Option<impl Deref<Target = ServoArc<ComputedValues>>> {
        self.try_stylo_element_data()
            .and_then(|stylo| stylo.primary_styles())
    }

    /// A lazy Taffy style backed by this node's primary stylo style.
    ///
    /// Panics if the node has no computed styles: layout always runs after
    /// styling, and the only unstyled nodes (text, comments, descendants of
    /// `display: none`) are never queried by Taffy.
    pub fn layout_style(&self) -> stylo_taffy::TaffyStyloStyle<ComputedStyleRef<'_>> {
        let styles = self
            .try_stylo_element_data()
            .and_then(|stylo| stylo.computed_styles())
            .expect("layout_style() called on a node without computed styles");

        let mut flags = stylo_taffy::StyleFlags::empty();
        if let Some(el) = self.data.downcast_element() {
            if crate::layout::replaced::is_replaced_element(&el.name.local) {
                flags |= stylo_taffy::StyleFlags::IS_REPLACED;
            }
            if el.name.local == local_name!("button") {
                flags |= stylo_taffy::StyleFlags::IS_BUTTON;
            }
            if el.is_button_input() {
                flags |= stylo_taffy::StyleFlags::IS_BUTTON_INPUT;
            }
            if let Some(inline_layout) = el.inline_layout_data.as_deref() {
                flags |= inline_layout.span_cb_flags;
            }
        }

        #[cfg(feature = "writing-mode")]
        {
            // Own-layout_wm by default: the node's own algorithm reads its style in its own axes
            let layout_wm = styles.writing_mode;
            stylo_taffy::TaffyStyloStyle::new_in(styles, flags, layout_wm)
        }
        #[cfg(not(feature = "writing-mode"))]
        {
            stylo_taffy::TaffyStyloStyle::new(styles, flags)
        }
    }

    /// The node's style for Taffy, expressed in the axes of the writing mode `layout_wm` (the writing
    /// mode of the box laying this node out). Used for the child-style getters.
    #[cfg(feature = "writing-mode")]
    pub fn layout_style_in(
        &self,
        layout_wm: stylo_taffy::WritingMode,
    ) -> stylo_taffy::TaffyStyloStyle<ComputedStyleRef<'_>> {
        let mut style = self.layout_style();
        style.layout_wm = layout_wm;
        style
    }

    /// Whether the node has any `contain` value, which blocks `writing-mode` propagation from
    /// `<body>` to the root (css-writing-modes-3 §8).
    #[cfg(feature = "writing-mode")]
    pub(crate) fn has_containment(&self) -> bool {
        self.primary_styles()
            .is_some_and(|style| !style.clone_contain().is_empty())
    }

    /// Whether the node is a flex container with a column `flex-direction`, i.e. aligns its
    /// children's `align-self` in its inline axis.
    #[cfg(feature = "writing-mode")]
    pub(crate) fn is_column_flex_container(&self) -> bool {
        self.primary_styles().is_some_and(|s| {
            s.clone_display().inside() == DisplayInside::Flex
                && !self.element_data().is_some_and(|el| el.is_button_input())
                && matches!(
                    s.get_position().flex_direction,
                    FlexDirection::Column | FlexDirection::ColumnReverse
                )
        })
    }

    /// The node's computed `writing-mode` (horizontal-tb for nodes without styles)
    #[cfg(feature = "writing-mode")]
    pub fn writing_mode(&self) -> stylo_taffy::WritingMode {
        self.primary_styles()
            .map(|s| s.writing_mode)
            .unwrap_or(stylo_taffy::WritingMode::empty())
    }

    /// The node's `display` as a [`taffy::Display`]. Returns [`taffy::Display::Block`]
    /// for nodes without computed styles (e.g. text nodes).
    pub fn taffy_display(&self) -> taffy::Display {
        self.primary_styles()
            .map(|style| {
                let display = style.clone_display();
                if self.element_data().is_some_and(|el| el.is_button_input())
                    && !matches!(
                        display.inside(),
                        DisplayInside::None | DisplayInside::Contents
                    )
                {
                    taffy::Display::FlowRoot
                } else {
                    stylo_taffy::convert::display(display)
                }
            })
            .unwrap_or(taffy::Display::Block)
    }

    /// The node's `position` as a [`taffy::Position`]. Returns [`taffy::Position::Static`]
    /// for nodes without computed styles (e.g. text nodes).
    pub fn taffy_position(&self) -> taffy::Position {
        self.primary_styles()
            .map(|s| stylo_taffy::convert::position(s.get_box().position))
            .unwrap_or(taffy::Position::Static)
    }

    /// Whether the node is an out-of-flow box that Taffy positions from its containing
    /// block's hoisted child list rather than from its parent (`display: none` boxes
    /// generate no box and are never hoisted).
    pub fn is_out_of_flow(&self) -> bool {
        self.primary_styles().is_some_and(|s| {
            let box_style = s.get_box();
            stylo_taffy::convert::position(box_style.position).is_out_of_flow()
                && stylo_taffy::convert::display(box_style.display) != taffy::Display::None
        })
    }

    pub fn text_content(&self) -> String {
        let mut out = String::new();
        self.write_text_content(&mut out);
        out
    }

    pub(super) fn write_text_content(&self, out: &mut String) {
        match &self.data {
            NodeData::Text(data) => {
                out.push_str(&data.content);
            }
            NodeData::Element(..) | NodeData::AnonymousBlock(..) => {
                for child_id in self.children.iter() {
                    self.with(*child_id).write_text_content(out);
                }
            }
            _ => {}
        }
    }

    pub fn flush_style_attribute(&mut self, url_extra_data: &UrlExtraData) {
        if let NodeData::Element(ref mut elem_data) = self.data {
            if let Some(guard) = elem_data.guard.clone() {
                elem_data.flush_style_attribute(&guard, url_extra_data);
            }
        }
    }
}
