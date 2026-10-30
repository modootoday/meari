use super::*;

impl DragMode {
    pub(crate) fn take(&mut self) -> DragMode {
        std::mem::replace(self, DragMode::None)
    }
}

impl PanState {
    pub(super) fn update(&mut self, time_ms: u64, screen_x: f32, screen_y: f32) -> (f64, f64) {
        // Constrain panning to the axes permitted by the `touch-action` property. Positions are
        // still tracked on both axes so that deltas remain correct after a disallowed movement.
        let dx = if self.allow_x {
            (screen_x - self.last_x) as f64
        } else {
            0.0
        };
        let dy = if self.allow_y {
            (screen_y - self.last_y) as f64
        } else {
            0.0
        };
        self.last_x = screen_x;
        self.last_y = screen_y;

        self.samples.push_back(PanSample {
            time: time_ms,
            // TODO: account for scroll delta not applied due to clamping
            dx: dx as f32,
            dy: dy as f32,
        });

        // Remove samples older than 100ms
        if self.samples.len() > 50 && time_ms - self.samples.front().unwrap().time > 100 {
            let idx = self
                .samples
                .partition_point(|sample| time_ms - sample.time > 100);
            // FIXME: use truncate_front once stable
            for _ in 0..idx {
                self.samples.pop_front();
            }
        }

        (dx, dy)
    }

    pub(super) fn generate_fling(&self, time_ms: u64) -> Option<FlingState> {
        // Generate "fling"
        if let Some(last_sample) = self.samples.back()
            && time_ms - last_sample.time < 100
        {
            let idx = self
                .samples
                .partition_point(|sample| time_ms - sample.time > 100);

            // Compute pan_time. Will always be <= 100ms as we ignore samples older than that.
            let pan_start_time = self.samples[idx].time;
            let pan_time = (time_ms - pan_start_time) as f32;

            // Avoid division by 0
            if pan_time > 0.0 {
                let (pan_x, pan_y) = self
                    .samples
                    .iter()
                    .skip(idx)
                    .fold((0.0, 0.0), |(dx, dy), sample| {
                        (dx + sample.dx, dy + sample.dy)
                    });

                let x_velocity = if pan_x.abs() > pan_y.abs() {
                    pan_x / pan_time
                } else {
                    0.0
                };

                let y_velocity = if pan_y.abs() > pan_x.abs() {
                    pan_y / pan_time
                } else {
                    0.0
                };

                return Some(FlingState {
                    target: self.target,
                    last_seen_time: time_ms as f64,
                    x_velocity: x_velocity as f64 * 2.0,
                    y_velocity: y_velocity as f64 * 2.0,
                });
            }
        }

        None
    }
}

/// Compute which axes a touch pan gesture starting on `node_id` is allowed to scroll, according to
/// the `touch-action` property.
///
/// Per the Pointer Events spec the effective behaviour is the intersection of the `touch-action`
/// values of the target and its ancestors up to and including the nearest ancestor that implements
/// the pan (the nearest scroll container which can actually scroll on that axis), so the walk stops
/// per-axis at that scroller: `touch-action` values on elements *above* it do not restrict pans it
/// handles. `touch-action: none` blocks panning entirely, `pan-x`/`pan-y` restrict it to a single
/// axis, and `auto` / `manipulation` permit both.
pub(super) fn touch_action_pan_axes(doc: &BaseDocument, node_id: NodeId) -> (bool, bool) {
    let pan_x_flags = TouchAction::AUTO | TouchAction::MANIPULATION | TouchAction::PAN_X;
    let pan_y_flags = TouchAction::AUTO | TouchAction::MANIPULATION | TouchAction::PAN_Y;

    let mut allow_x = true;
    let mut allow_y = true;
    // Whether the nearest scroller for the axis has been reached (its own `touch-action` counts,
    // but its ancestors' do not).
    let mut done_x = false;
    let mut done_y = false;
    let mut current = Some(node_id);
    while let Some(id) = current {
        let node = &doc.nodes[id];
        if let Some(style) = node.primary_styles() {
            let touch_action = style.clone_touch_action();
            if !done_x {
                allow_x &= touch_action.intersects(pan_x_flags);
            }
            if !done_y {
                allow_y &= touch_action.intersects(pan_y_flags);
            }

            let scrolls_x = matches!(style.clone_overflow_x(), Overflow::Scroll | Overflow::Auto)
                && node.final_layout().scroll_width() > 0.0;
            let scrolls_y = matches!(style.clone_overflow_y(), Overflow::Scroll | Overflow::Auto)
                && node.final_layout().scroll_height() > 0.0;
            done_x |= scrolls_x;
            done_y |= scrolls_y;

            if (done_x || !allow_x) && (done_y || !allow_y) {
                break;
            }
        }
        current = node.parent;
    }

    (allow_x, allow_y)
}

pub(crate) fn handle_pointermove<F: FnMut(DomEvent)>(
    doc: &mut BaseDocument,
    target: NodeId,
    event: &BlitzPointerEvent,
    mut dispatch_event: F,
) -> bool {
    let x = event.page_x();
    let y = event.page_y();
    let buttons = event.buttons;

    let mut changed = doc.set_hover_to(x, y);

    // Check if we've moved enough to be considered a selection drag (2px threshold)
    if buttons != MouseEventButtons::None && doc.drag_mode == DragMode::None {
        let dx = x - doc.mousedown_position.x;
        let dy = y - doc.mousedown_position.y;
        if dx.abs() > 2.0 || dy.abs() > 2.0 {
            match event.id {
                BlitzPointerId::Mouse | BlitzPointerId::Pen => {
                    if let Some(mousedown_node_id) = doc.mousedown_node_id {
                        let node = &doc.nodes[mousedown_node_id];
                        if let Some(style) = node.primary_styles() {
                            let user_select = style.clone_user_select();
                            if user_select == UserSelect::None {
                                // Do nothing. Continue with rest of function
                            } else if user_select == UserSelect::Auto {
                                if let Some(parent) = node.parent {
                                    let node = &doc.nodes[parent];
                                    if let Some(style) = node.primary_styles() {
                                        let user_select = style.clone_user_select();
                                        if user_select == UserSelect::None {
                                            // Do nothing. Continue with rest of function
                                        } else {
                                            doc.drag_mode = DragMode::Selecting;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                BlitzPointerId::Finger(_) => {
                    let (allow_x, allow_y) = touch_action_pan_axes(doc, target);
                    // If `touch-action` forbids panning on both axes (e.g. `touch-action: none`)
                    // there is nothing to scroll, so don't enter the panning drag mode.
                    if allow_x || allow_y {
                        doc.drag_mode = DragMode::Panning(PanState {
                            target,
                            last_x: event.screen_x(),
                            last_y: event.screen_y(),
                            allow_x,
                            allow_y,
                            samples: VecDeque::with_capacity(200),
                        });
                    }
                }
            }
        }
    }

    if let DragMode::Panning(state) = &mut doc.drag_mode {
        let time_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let target = state.target;
        let (dx, dy) = state.update(time_ms, event.screen_x(), event.screen_y());

        let has_changed = doc.scroll_chain_by(Some(target), dx, dy, &mut dispatch_event);
        return has_changed;
    }

    if let DragMode::ScrollbarDrag(state) = &mut doc.drag_mode {
        let ScrollbarRef { node_id, axis } = state.scrollbar;
        let pos = match axis {
            AbsoluteAxis::Horizontal => x,
            AbsoluteAxis::Vertical => y,
        };
        let delta_px = (pos - state.last_pos) as f64;
        state.last_pos = pos;

        // Thumb px -> content px. scroll_by uses wheel-delta semantics
        // (positive delta decreases the offset), so negate.
        let ratio = doc.nodes[node_id].scrollbar_drag_ratio(axis);
        let (dx, dy) = match axis {
            AbsoluteAxis::Horizontal => (-delta_px * ratio, 0.0),
            AbsoluteAxis::Vertical => (0.0, -delta_px * ratio),
        };
        let has_changed = doc.scroll_chain_by(Some(node_id), dx, dy, &mut dispatch_event);
        return has_changed;
    }

    let Some(hit) = doc.hit(x, y) else {
        return changed;
    };

    if changed {
        dispatch_event(DomEvent::new(
            hit.node_id,
            DomEventData::MouseEnter(event.clone()),
        ));
    }

    // `target` is the event's canonicalized target (never a layout-generated
    // node), while the hit may be an anonymous block (e.g. bare text wrapped
    // in an anonymous box). Compare against the hit's canonical DOM ancestor
    // so selection drags keep working over anonymous blocks.
    if doc.nearest_non_anonymous_ancestor(hit.node_id) != Some(target) {
        return changed;
    }

    let node = &mut doc.nodes[target];
    let Some(el) = node.data.downcast_element_mut() else {
        // Handle text selection extension for non-element nodes
        if buttons != MouseEventButtons::None
            && doc.drag_mode == DragMode::Selecting
            && doc.extend_text_selection_to_point(x, y)
        {
            changed = true;
        }
        return changed;
    };

    let disabled = el.attr(local_name!("disabled")).is_some();
    if disabled {
        return changed;
    }

    let final_layout = el.layout_data().final_layout;
    if let SpecialElementData::TextInput(ref mut text_input_data) = el.special_data {
        if buttons == MouseEventButtons::None {
            return changed;
        }

        let mut content_box_offset = taffy::Point {
            x: final_layout.padding.left + final_layout.border.left,
            y: final_layout.padding.top + final_layout.border.top,
        };
        if !text_input_data.is_multiline {
            let layout = text_input_data.editor.try_layout().unwrap();
            let content_box_height = final_layout.content_box_height();
            let input_height = layout.height() / layout.scale();
            let y_offset = ((content_box_height - input_height) / 2.0).max(0.0);

            content_box_offset.y += y_offset;
        }

        // Account for the input's scroll offset (stored in CSS pixels, scaled here to device
        // pixels) when mapping the pointer location into the text content's coordinate space.
        let scroll_offset = text_input_data.scroll_offset as f64 * doc.viewport.scale_f64();
        let (scroll_x, scroll_y) = if text_input_data.is_multiline {
            (0.0, scroll_offset)
        } else {
            (scroll_offset, 0.0)
        };

        let x = (hit.x - content_box_offset.x) as f64 * doc.viewport.scale_f64() + scroll_x;
        let y = (hit.y - content_box_offset.y) as f64 * doc.viewport.scale_f64() + scroll_y;

        text_input_data
            .editor
            .driver(&mut doc.font_ctx.lock().unwrap(), &mut doc.layout_ctx)
            .extend_selection_to_point(x as f32, y as f32);

        changed = true;
    } else if event.is_mouse()
        && buttons != MouseEventButtons::None
        && doc.drag_mode == DragMode::Selecting
        && doc.extend_text_selection_to_point(x, y)
    {
        changed = true;
    }

    changed
}

pub(crate) fn handle_pointerdown(
    doc: &mut BaseDocument,
    _target: NodeId,
    x: f32,
    y: f32,
    button: MouseEventButton,
    mods: Modifiers,
    dispatch_event: &mut dyn FnMut(DomEvent),
) {
    // Compute click count using the previous mousedown position (before updating)
    // This handles both double-click detection and text input word/line selection
    // TODO: For text inputs, only increment click count if click maps to the same/similar caret position
    doc.click_count = if doc
        .last_mousedown_time
        .map(|t| t.elapsed() < Duration::from_millis(500))
        .unwrap_or(false)
        && (doc.mousedown_position.x - x).abs() <= 2.0
        && (doc.mousedown_position.y - y).abs() <= 2.0
    {
        doc.click_count + 1
    } else {
        1
    };

    // Update mousedown tracking for next click and selection drag detection
    doc.last_mousedown_time = Some(Instant::now());
    doc.mousedown_position = taffy::Point { x, y };
    doc.drag_mode = DragMode::None;
    doc.scroll_animation = ScrollAnimationState::None;

    let Some(hit) = doc.hit(x, y) else {
        // Clear text selection when clicking outside any element
        doc.clear_text_selection();
        return;
    };

    // Scrollbar thumb drags take precedence over content interactions and
    // are not dispatched to the page (matching native scrollbars). A
    // faded-out thumb doesn't capture: the click goes to the content.
    if button == MouseEventButton::Main {
        if let (_, Some(scrollbar)) = doc.hit_with_scrollbar(x, y)
            && doc.scrollbar_opacity(scrollbar.node_id) > 0.0
        {
            doc.drag_mode = DragMode::ScrollbarDrag(ScrollbarDragState {
                scrollbar,
                last_pos: match scrollbar.axis {
                    AbsoluteAxis::Horizontal => x,
                    AbsoluteAxis::Vertical => y,
                },
            });
            doc.shell_provider.request_redraw();
            return;
        }
    }

    // Use hit.node_id for determining the actual clicked element.
    // This may differ from `target` for anonymous blocks (which are layout children
    // but not DOM children), so we use the hit result for text selection.
    let actual_target = hit.node_id;

    // Check what kind of element we're dealing with and extract needed info
    enum ClickTarget {
        TextInput {
            content_box_offset: taffy::Point<f32>,
            /// Scroll offset of the input along each axis, in scaled (device) pixels.
            scroll: taffy::Point<f64>,
        },
        Disabled,
        SelectableText,
    }

    let click_target = {
        let node = &doc.nodes[actual_target];
        match node.data.downcast_element() {
            Some(el) if el.has_attr(local_name!("disabled")) => ClickTarget::Disabled,
            Some(el) => {
                if let SpecialElementData::TextInput(ref text_input_data) = el.special_data {
                    let mut content_box_offset = taffy::Point {
                        x: node.final_layout().padding.left + node.final_layout().border.left,
                        y: node.final_layout().padding.top + node.final_layout().border.top,
                    };
                    if !text_input_data.is_multiline {
                        let layout = text_input_data.editor.try_layout().unwrap();
                        let content_box_height = node.final_layout().content_box_height();
                        let input_height = layout.height() / layout.scale();
                        let y_offset = ((content_box_height - input_height) / 2.0).max(0.0);
                        content_box_offset.y += y_offset;
                    }
                    // `scroll_offset` is stored in CSS pixels; scale it to device pixels to
                    // match the editor's coordinate space.
                    let scroll_offset =
                        text_input_data.scroll_offset as f64 * doc.viewport.scale_f64();
                    let scroll = if text_input_data.is_multiline {
                        taffy::Point {
                            x: 0.0,
                            y: scroll_offset,
                        }
                    } else {
                        taffy::Point {
                            x: scroll_offset,
                            y: 0.0,
                        }
                    };
                    ClickTarget::TextInput {
                        content_box_offset,
                        scroll,
                    }
                } else {
                    ClickTarget::SelectableText
                }
            }
            None => ClickTarget::SelectableText,
        }
    };

    match click_target {
        ClickTarget::Disabled => (),
        ClickTarget::SelectableText => {
            // Handle text selection for non-input elements
            if let Some((inline_root_id, byte_offset)) = doc.find_text_position(x, y) {
                doc.set_text_selection(inline_root_id, byte_offset, inline_root_id, byte_offset);
                doc.shell_provider.request_redraw();
            } else {
                doc.clear_text_selection();
            }
        }
        ClickTarget::TextInput {
            content_box_offset,
            scroll,
        } => {
            // Clear general text selection when focusing a text input
            doc.clear_text_selection();

            let tx = (hit.x - content_box_offset.x) as f64 * doc.viewport.scale_f64() + scroll.x;
            let ty = (hit.y - content_box_offset.y) as f64 * doc.viewport.scale_f64() + scroll.y;

            // Now get mutable access to the text input
            let click_count = doc.click_count;
            let node = &mut doc.nodes[actual_target];
            let el = node.data.downcast_element_mut().unwrap();
            if let SpecialElementData::TextInput(ref mut text_input_data) = el.special_data {
                let mut font_ctx = doc.font_ctx.lock().unwrap();
                let mut driver = text_input_data
                    .editor
                    .driver(&mut font_ctx, &mut doc.layout_ctx);

                match click_count {
                    1 => {
                        if mods.shift() {
                            driver.shift_click_extension(tx as f32, ty as f32);
                        } else {
                            driver.move_to_point(tx as f32, ty as f32);
                        }
                    }
                    2 => driver.select_word_at_point(tx as f32, ty as f32),
                    _ => driver.select_hard_line_at_point(tx as f32, ty as f32),
                }

                drop(font_ctx);
            }

            generate_focus_events(
                doc,
                &mut |doc| {
                    doc.set_focus_to(hit.node_id);
                },
                dispatch_event,
            );
        }
    }
}
