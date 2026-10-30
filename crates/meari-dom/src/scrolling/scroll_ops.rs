use super::*;

/// Cubic ease-in-out easing function, mapping a normalised time `t` in `[0, 1]`
/// to an eased progress value in `[0, 1]`. Used to give smooth scrolls a natural
/// acceleration/deceleration curve.
pub(super) fn ease_in_out_cubic(t: f64) -> f64 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let f = 2.0 * t - 2.0;
        1.0 + (f * f * f) / 2.0
    }
}

impl BaseDocument {
    /// Apply a scroll to the document, returning whether anything moved.
    ///
    /// This is the single scrolling primitive: user-initiated scrolls
    /// ([`BaseDocument::scroll_chain_by`]) and programmatic scrolls
    /// ([`BaseDocument::scroll_to`]) differ only in the [`ScrollRequest`] they build.
    pub(crate) fn scroll(
        &mut self,
        request: ScrollRequest,
        dispatch_event: &mut dyn FnMut(DomEvent),
    ) -> bool {
        if request.interrupt_animation
            && matches!(self.scroll_animation, ScrollAnimationState::ScrollTo(_))
        {
            self.scroll_animation = ScrollAnimationState::None;
        }

        let target = self.canonical_scroll_target(request.target);

        // Text inputs and sub-documents scroll their own content rather than an overflow
        // scrollport, so they only take part in the chained (user-initiated) path.
        if let (ScrollTarget::Node(node_id), ScrollAmount::By(delta), ScrollOverflow::Chain) =
            (target, request.amount, request.overflow)
        {
            if let Some(has_changed) =
                self.scroll_inner_content_by(node_id, delta, request, dispatch_event)
            {
                return has_changed;
            }
        }

        let include_hidden = request.source == ScrollSource::Programmatic;
        let (current, max) = self.scroll_state(target, include_hidden);
        let unclamped = match request.amount {
            ScrollAmount::To(to) => to,
            ScrollAmount::By(by) => Point {
                x: current.x + by.x,
                y: current.y + by.y,
            },
        };
        let end = Point {
            x: unclamped.x.clamp(0.0, max.x),
            y: unclamped.y.clamp(0.0, max.y),
        };

        if self.should_scroll_smoothly(target, request.behavior) {
            self.start_scroll_animation(target, end);
            return end != current;
        }

        let has_changed = self.write_scroll_offset(target, end, dispatch_event);

        // Transfer the delta the target could not consume to the next scroller in the chain.
        if request.overflow == ScrollOverflow::Chain {
            let remainder = Point {
                x: unclamped.x - end.x,
                y: unclamped.y - end.y,
            };
            if remainder != Point::ZERO {
                if let Some(next) = self.next_scroller_in_chain(target) {
                    let request = ScrollRequest {
                        target: next,
                        amount: ScrollAmount::By(remainder),
                        ..request
                    };
                    return has_changed | self.scroll(request, dispatch_event);
                }
            }
        }

        has_changed
    }

    /// Scroll a node which scrolls content of its own rather than an overflow scrollport
    /// (a text input or a sub-document), returning `None` if the node is not such a node.
    pub(super) fn scroll_inner_content_by(
        &mut self,
        node_id: NodeId,
        delta: Point<f64>,
        request: ScrollRequest,
        dispatch_event: &mut dyn FnMut(DomEvent),
    ) -> Option<bool> {
        let node = self.nodes.get_mut(node_id)?;

        if let Some(mut sub_doc) = node.subdoc_mut().map(|doc| doc.inner_mut()) {
            let target = sub_doc
                .get_hover_node_id()
                .map_or(ScrollTarget::Viewport, ScrollTarget::Node);
            // TODO: propagate the remaining scroll to the outer document
            let request = ScrollRequest {
                target,
                amount: ScrollAmount::By(delta),
                ..request
            };
            return Some(sub_doc.scroll(request, dispatch_event));
        }

        // Single-line inputs scroll their text horizontally, multi-line inputs vertically.
        node.element_data()?.text_input_data()?;
        let content_box_width = node.final_layout().content_box_width();
        let content_box_height = node.final_layout().content_box_height();
        let input = node
            .element_data_mut()
            .and_then(|el| el.text_input_data_mut())
            .unwrap();

        // `TextInputData::scroll_by` takes (and returns the unconsumed part of) a delta in
        // the opposite direction to a scroll offset.
        let mut remainder = delta;
        if input.is_multiline {
            remainder.y =
                -(input.scroll_by(-delta.y as f32, content_box_width, content_box_height) as f64);
        } else {
            remainder.x =
                -(input.scroll_by(-delta.x as f32, content_box_width, content_box_height) as f64);
        }

        let has_changed = remainder != delta;

        if remainder != Point::ZERO {
            if let Some(next) = self.next_scroller_in_chain(ScrollTarget::Node(node_id)) {
                let request = ScrollRequest {
                    target: next,
                    amount: ScrollAmount::By(remainder),
                    ..request
                };
                return Some(has_changed | self.scroll(request, dispatch_event));
            }
        }

        Some(has_changed)
    }

    /// The scroller which unconsumed scroll is transferred to: the node's parent, or the
    /// viewport once the chain runs out of nodes.
    pub(super) fn next_scroller_in_chain(&self, target: ScrollTarget) -> Option<ScrollTarget> {
        match target {
            ScrollTarget::Viewport => None,
            ScrollTarget::Node(node_id) => Some(
                self.nodes
                    .get(node_id)
                    .and_then(|node| node.parent)
                    .map_or(ScrollTarget::Viewport, ScrollTarget::Node),
            ),
        }
    }

    /// Resolve a scroll target to the scroller which actually moves: the root element scrolls
    /// the viewport, per the CSS overflow propagation rules.
    pub(super) fn canonical_scroll_target(&self, target: ScrollTarget) -> ScrollTarget {
        match target {
            ScrollTarget::Node(node_id)
                if self.try_root_element().is_some_and(|el| el.id == node_id) =>
            {
                ScrollTarget::Viewport
            }
            target => target,
        }
    }

    /// Write a scroll target's offset (which must already be clamped to its scrollable
    /// range), dispatching a `scroll` event and returning whether the offset changed.
    pub(crate) fn write_scroll_offset(
        &mut self,
        target: ScrollTarget,
        offset: Point<f64>,
        dispatch_event: &mut dyn FnMut(DomEvent),
    ) -> bool {
        match self.canonical_scroll_target(target) {
            ScrollTarget::Viewport => {
                let initial = self.viewport_scroll();
                self.set_viewport_scroll(offset);
                if offset == initial {
                    return false;
                }

                if let Some(root) = self.try_root_element() {
                    let root_id = root.id;
                    let layout = *root.final_layout();
                    let scale = self.viewport.scale() as f64;
                    let event = BlitzScrollEvent {
                        scroll_top: offset.y,
                        scroll_left: offset.x,
                        scroll_width: layout.size.width.max(layout.scrollable_overflow_rect.right)
                            as i32,
                        scroll_height: layout
                            .size
                            .height
                            .max(layout.scrollable_overflow_rect.bottom)
                            as i32,
                        client_width: (self.viewport.window_size.0 as f64 / scale) as i32,
                        client_height: (self.viewport.window_size.1 as f64 / scale) as i32,
                    };
                    dispatch_event(DomEvent::new(root_id, DomEventData::Scroll(event)));
                }

                self.shell_provider.request_redraw();
                true
            }
            ScrollTarget::Node(node_id) => {
                let Some(node) = self.nodes.get_mut(node_id) else {
                    return false;
                };

                let initial = *node.scroll_offset();
                *node.scroll_offset_mut() = offset;
                if offset == initial {
                    return false;
                }

                let layout = *node.final_layout();
                self.nodes.bump_geometry_generation();
                let event = BlitzScrollEvent {
                    scroll_top: offset.y,
                    scroll_left: offset.x,
                    scroll_width: layout.scroll_width() as i32,
                    scroll_height: layout.scroll_height() as i32,
                    client_width: layout.size.width as i32,
                    client_height: layout.size.height as i32,
                };
                dispatch_event(DomEvent::new(node_id, DomEventData::Scroll(event)));

                self.show_scrollbars(node_id);
                self.shell_provider.request_redraw();
                true
            }
        }
    }

    pub fn scroll_node_by<F: FnMut(DomEvent)>(
        &mut self,
        node_id: NodeId,
        x: f64,
        y: f64,
        dispatch_event: F,
    ) {
        self.scroll_node_by_has_changed(node_id, x, y, dispatch_event);
    }

    /// Scroll a node by given x and y
    /// Will bubble scrolling up to parent node once it can no longer scroll further
    /// If we're already at the root node, bubbles scrolling up to the viewport
    pub fn scroll_node_by_has_changed<F: FnMut(DomEvent)>(
        &mut self,
        node_id: NodeId,
        x: f64,
        y: f64,
        mut dispatch_event: F,
    ) -> bool {
        self.scroll(
            ScrollRequest {
                target: ScrollTarget::Node(node_id),
                // A user-facing scroll delta moves the content, i.e. the opposite direction
                // to the scroll offset.
                amount: ScrollAmount::By(Point { x: -x, y: -y }),
                overflow: ScrollOverflow::Chain,
                source: ScrollSource::User,
                behavior: ScrollBehavior::Instant,
                interrupt_animation: false,
            },
            &mut dispatch_event,
        )
    }

    pub fn scroll_viewport_by(&mut self, x: f64, y: f64) {
        self.scroll_viewport_by_has_changed(x, y);
    }

    /// Scroll the viewport by the given values
    pub fn scroll_viewport_by_has_changed(&mut self, x: f64, y: f64) -> bool {
        self.scroll(
            ScrollRequest {
                target: ScrollTarget::Viewport,
                amount: ScrollAmount::By(Point { x: -x, y: -y }),
                overflow: ScrollOverflow::Clamp,
                source: ScrollSource::User,
                behavior: ScrollBehavior::Instant,
                interrupt_animation: false,
            },
            &mut |_| {},
        )
    }

    pub(crate) fn scroll_chain_by(
        &mut self,
        anchor_node_id: Option<NodeId>,
        scroll_x: f64,
        scroll_y: f64,
        dispatch_event: &mut dyn FnMut(DomEvent),
    ) -> bool {
        self.scroll(
            ScrollRequest {
                target: anchor_node_id.map_or(ScrollTarget::Viewport, ScrollTarget::Node),
                amount: ScrollAmount::By(Point {
                    x: -scroll_x,
                    y: -scroll_y,
                }),
                overflow: ScrollOverflow::Chain,
                source: ScrollSource::User,
                behavior: ScrollBehavior::Instant,
                // A user-initiated scroll aborts any smooth scroll in progress, so that the
                // two do not fight over the scroll offset for the rest of the animation.
                interrupt_animation: true,
            },
            dispatch_event,
        )
    }

    /// Duration (in milliseconds) of an animated scroll.
    pub(super) const SMOOTH_SCROLL_DURATION_MS: f64 = 300.0;

    /// Returns the current scroll offset and the maximum scroll offset (the minimum is
    /// always `0`) for the given scroll target. `include_hidden` controls whether
    /// `overflow: hidden` axes count as scrollable (they do for programmatic scrolls,
    /// which apply to any scrolling box, but not for user scrolls).
    pub(crate) fn scroll_state(
        &self,
        target: ScrollTarget,
        include_hidden: bool,
    ) -> (Point<f64>, Point<f64>) {
        match self.canonical_scroll_target(target) {
            ScrollTarget::Viewport => {
                // The viewport scrolls the root element's scrollable overflow, which includes
                // both the root element itself and any content which overflows it (e.g. when
                // the root element has a fixed height but its content is taller). A document
                // without a root element has no scrollable content.
                let (content_width, content_height) = match self.try_root_element() {
                    Some(root) => {
                        let layout = root.final_layout();
                        (
                            layout.size.width.max(layout.scrollable_overflow_rect.right) as f64,
                            layout
                                .size
                                .height
                                .max(layout.scrollable_overflow_rect.bottom)
                                as f64,
                        )
                    }
                    None => (0.0, 0.0),
                };
                let scale = self.viewport.scale() as f64;
                let window_width = self.viewport.window_size.0 as f64 / scale;
                let window_height = self.viewport.window_size.1 as f64 / scale;
                let max = Point {
                    x: (content_width - window_width).max(0.0),
                    y: (content_height - window_height).max(0.0),
                };
                (self.viewport_scroll(), max)
            }
            ScrollTarget::Node(node_id) => {
                let Some(node) = self.nodes.get(node_id) else {
                    return (Point::ZERO, Point::ZERO);
                };

                // An axis with a non-scrolling overflow value has no scrollable range, even
                // when its content overflows.
                let scrollable = |overflow| match overflow {
                    Overflow::Scroll | Overflow::Auto => true,
                    Overflow::Hidden => include_hidden,
                    _ => false,
                };
                let (can_x_scroll, can_y_scroll) = node
                    .primary_styles()
                    .map(|styles| {
                        (
                            scrollable(styles.clone_overflow_x()),
                            scrollable(styles.clone_overflow_y()),
                        )
                    })
                    .unwrap_or((false, false));
                let max = Point {
                    x: match can_x_scroll {
                        true => node.final_layout().scroll_width() as f64,
                        false => 0.0,
                    },
                    y: match can_y_scroll {
                        true => node.final_layout().scroll_height() as f64,
                        false => 0.0,
                    },
                };
                (*node.scroll_offset(), max)
            }
        }
    }

    /// Start a smooth (animated) scroll towards the given absolute scroll offset. The
    /// animation is advanced each frame in [`BaseDocument::resolve_scroll_animation`].
    pub(super) fn start_scroll_animation(&mut self, target: ScrollTarget, end: Point<f64>) {
        let start = self.scroll_state(target, true).0;

        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64 as f64;

        self.scroll_animation = ScrollAnimationState::ScrollTo(ScrollToState {
            target,
            start,
            end,
            start_time,
            duration: Self::SMOOTH_SCROLL_DURATION_MS,
        });

        // Ensure the frame loop runs so the animation is driven to completion.
        self.shell_provider.request_redraw();
    }

    pub(super) fn should_scroll_smoothly(
        &self,
        target: ScrollTarget,
        behavior: ScrollBehavior,
    ) -> bool {
        match behavior {
            ScrollBehavior::Auto => {
                let styled_node = match target {
                    ScrollTarget::Node(node_id) => self.nodes.get(node_id),
                    ScrollTarget::Viewport => self.try_root_element(),
                };
                styled_node.is_some_and(|node| {
                    node.primary_styles().is_some_and(|style| {
                        style.clone_scroll_behavior()
                            == style::computed_values::scroll_behavior::T::Smooth
                    })
                })
            }
            ScrollBehavior::Instant => false,
            ScrollBehavior::Smooth => true,
        }
    }

    /// Scroll an element to the given absolute scroll offset in CSS pixels.
    ///
    /// Unlike a user-initiated scroll, a programmatic scroll targets exactly one scroller:
    /// scroll the element cannot consume is discarded rather than transferred to an ancestor.
    pub fn scroll_to(&mut self, node_id: NodeId, x: f64, y: f64, behavior: ScrollBehavior) {
        self.scroll_programmatically(node_id, ScrollAmount::To(Point { x, y }), behavior);
    }

    /// Scroll an element by the given relative offset in CSS pixels.
    pub fn scroll_by(&mut self, node_id: NodeId, x: f64, y: f64, behavior: ScrollBehavior) {
        self.scroll_programmatically(node_id, ScrollAmount::By(Point { x, y }), behavior);
    }

    pub(super) fn scroll_programmatically(
        &mut self,
        node_id: NodeId,
        amount: ScrollAmount,
        behavior: ScrollBehavior,
    ) {
        if self.nodes.get(node_id).is_none() {
            return;
        }

        // TODO: dispatch `scroll` events for programmatic scrolls.
        self.scroll(
            ScrollRequest {
                target: ScrollTarget::Node(node_id),
                amount,
                overflow: ScrollOverflow::Clamp,
                source: ScrollSource::Programmatic,
                behavior,
                interrupt_animation: true,
            },
            &mut |_| {},
        );
    }

    pub(super) fn aligned_scroll_offset(
        current: f64,
        viewport_size: f64,
        target_start: f64,
        target_size: f64,
        position: ScrollLogicalPosition,
    ) -> f64 {
        let target_end = target_start + target_size;
        match position {
            ScrollLogicalPosition::Start => target_start,
            ScrollLogicalPosition::Center => target_start - (viewport_size - target_size) / 2.0,
            ScrollLogicalPosition::End => target_end - viewport_size,
            ScrollLogicalPosition::Nearest => {
                let viewport_end = current + viewport_size;
                if (target_start >= current && target_end <= viewport_end)
                    || (target_start <= current && target_end >= viewport_end)
                {
                    current
                } else {
                    let start_offset = target_start;
                    let end_offset = target_end - viewport_size;
                    if (start_offset - current).abs() < (end_offset - current).abs() {
                        start_offset
                    } else {
                        end_offset
                    }
                }
            }
        }
    }
}
