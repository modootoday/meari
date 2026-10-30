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
    /// The per-line-box fragment boxes of a non-atomic inline element, in CSS pixels relative
    /// to the border box of its inline root.
    ///
    /// Returns `None` for nodes that have their own layout box.
    pub fn inline_fragment_boxes(&self) -> Option<impl Iterator<Item = taffy::Rect<f32>> + '_> {
        use parley::PositionedLayoutItem;

        if !self.is_non_atomic_inline() {
            return None;
        }

        let inline_root = self.inline_root_ancestor()?;
        let inline_layout = inline_root.element_data()?.inline_layout_data.as_ref()?;
        let layout = &inline_layout.layout;
        let scale = layout.scale();

        // Walk up the DOM parent chain from `id` to check whether it is (or is
        // inside) the target node, stopping at the inline root.
        let inline_root_id = inline_root.id;
        let is_in_target = move |mut id: NodeId| -> bool {
            loop {
                if id == self.id {
                    return true;
                }
                if id == inline_root_id {
                    return false;
                }
                match self.with(id).parent {
                    Some(parent) => id = parent,
                    None => return false,
                }
            }
        };

        let root_layout = inline_root.unrounded_layout();
        let content_box_inset = root_layout.padding + root_layout.border;
        let origin_x = content_box_inset.left;
        let origin_y = content_box_inset.top + inline_layout.block_offset;

        fn union(acc: &mut Option<taffy::Rect<f32>>, left: f32, top: f32, right: f32, bottom: f32) {
            *acc = Some(match *acc {
                Some(rect) => taffy::Rect {
                    left: rect.left.min(left),
                    top: rect.top.min(top),
                    right: rect.right.max(right),
                    bottom: rect.bottom.max(bottom),
                },
                None => taffy::Rect {
                    left,
                    top,
                    right,
                    bottom,
                },
            });
        }

        // One rect per line box: the union of all of the target's fragments on that line
        Some(layout.lines().filter_map(move |line| {
            let line_metrics = line.metrics();
            let mut line_rect: Option<taffy::Rect<f32>> = None;

            for item in line.items() {
                match item {
                    PositionedLayoutItem::GlyphRun(glyph_run) => {
                        if !is_in_target(glyph_run.style().brush.id) {
                            continue;
                        }
                        let x0 = glyph_run.offset();
                        let x1 = x0 + glyph_run.advance();
                        // Use the line box's block extent rather than the
                        // run's font ascent/descent: fonts with small
                        // typographic metrics would otherwise produce rects
                        // that clip the rendered glyphs. This matches the
                        // geometry used for text selection highlights.
                        let y0 = line_metrics.block_min_coord;
                        let y1 = line_metrics.block_max_coord;
                        union(&mut line_rect, x0, y0, x1, y1);
                    }
                    PositionedLayoutItem::InlineBox(inline_box) => {
                        if !is_in_target(NodeId::from_u64(inline_box.id)) {
                            continue;
                        }
                        let x0 = inline_box.x;
                        let y0 = inline_box.y;
                        union(
                            &mut line_rect,
                            x0,
                            y0,
                            x0 + inline_box.width,
                            y0 + inline_box.height,
                        );
                    }
                }
            }

            line_rect.map(|rect| taffy::Rect {
                left: origin_x + rect.left / scale,
                top: origin_y + rect.top / scale,
                right: origin_x + rect.right / scale,
                bottom: origin_y + rect.bottom / scale,
            })
        }))
    }

    /// CSSOM View's `offsetLeft`/`offsetTop`: the offset of this node's border box from the
    /// padding edge of its [`offset_parent`](Self::offset_parent).
    pub fn offset_top_left(&self) -> crate::util::Point<f32> {
        let mut x = 0.0;
        let mut y = 0.0;
        let mut current = self;

        // Non-atomic inlines have no layout box of their own: start from the first
        // fragment box (relative to the inline root's border box) and continue the
        // walk from the inline root.
        if let Some(mut fragments) = self.inline_fragment_boxes() {
            if let Some(first) = fragments.next() {
                x += first.left;
                y += first.top;
            }
            let Some(inline_root) = self.inline_root_ancestor() else {
                return crate::util::Point { x, y };
            };
            if inline_root.is_offset_parent() && !inline_root.is_static_body() {
                let border = inline_root.final_layout().border;
                return crate::util::Point {
                    x: x - border.left,
                    y: y - border.top,
                };
            }
            current = inline_root;
        }

        loop {
            let layout = current.final_layout();
            x += layout.location.x;
            y += layout.location.y;

            let Some(parent_id) = current.containing_block() else {
                break;
            };
            let parent = self.with(parent_id);
            if parent.is_offset_parent() && !parent.is_static_body() {
                let border = parent.final_layout().border;
                x -= border.left;
                y -= border.top;
                break;
            }
            current = parent;
        }
        crate::util::Point { x, y }
    }

    /// CSSOM View's `clientWidth`: the width of the padding box (border box minus
    /// borders and scrollbar)
    pub fn client_width(&self) -> f32 {
        let layout = self.final_layout();
        layout.size.width - layout.border.left - layout.border.right - layout.scrollbar_size.width
    }

    /// CSSOM View's `clientHeight`: the height of the padding box (border box minus
    /// borders and scrollbar)
    pub fn client_height(&self) -> f32 {
        let layout = self.final_layout();
        layout.size.height - layout.border.top - layout.border.bottom - layout.scrollbar_size.height
    }

    /// CSSOM View's `scrollWidth`: the width of the node's content, including
    /// content not visible due to overflow
    pub fn scroll_width(&self) -> f32 {
        self.client_width()
            .max(self.final_layout().scrollable_overflow_rect.right)
    }

    /// CSSOM View's `scrollHeight`: the height of the node's content, including
    /// content not visible due to overflow
    pub fn scroll_height(&self) -> f32 {
        self.client_height()
            .max(self.final_layout().scrollable_overflow_rect.bottom)
    }

    /// Does the node generate any boxes? (e.g. `getClientRects()` returns an empty
    /// list for boxless nodes, such as `display: none`, `display: contents`, or
    /// detached elements)
    pub fn has_boxes(&self) -> bool {
        self.flags.is_in_document()
            && !self.display_style().is_some_and(|display| {
                matches!(
                    display.inside(),
                    style::values::specified::box_::DisplayInside::None
                        | style::values::specified::box_::DisplayInside::Contents
                )
            })
    }

    /// Creates a synthetic click event
    pub fn synthetic_click_event(&self, mods: Modifiers) -> DomEventData {
        DomEventData::Click(self.synthetic_click_event_data(mods))
    }

    pub fn synthetic_click_event_data(&self, mods: Modifiers) -> BlitzPointerEvent {
        let absolute_position = self.absolute_position(0.0, 0.0);
        let x = absolute_position.x + (self.final_layout().size.width / 2.0);
        let y = absolute_position.y + (self.final_layout().size.height / 2.0);

        BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords: PointerCoords {
                page_x: x,
                page_y: y,

                // TODO: should these be different?
                screen_x: x,
                screen_y: y,
                client_x: x,
                client_y: y,
            },
            mods,
            button: Default::default(),
            buttons: Default::default(),
            details: Default::default(),
            element: Default::default(),
            active_pointers: Default::default(),
        }
    }
}

/// It might be wrong to expose this since what does *equality* mean outside the dom?
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Node {}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // FIXME: update to reflect changes to fields
        f.debug_struct("NodeData")
            .field("parent", &self.parent)
            .field("id", &self.id)
            .field("is_inline_root", &self.flags.is_inline_root())
            .field("children", &self.children)
            .field("layout_children", &self.layout_children.borrow())
            // .field("style", &self.style)
            .field("node", &self.data)
            .field("stylo_element_data", &self.try_stylo_element_data())
            // .field("unrounded_layout", &self.unrounded_layout)
            // .field("final_layout", &self.final_layout)
            .finish()
    }
}
