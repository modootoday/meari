use super::*;

impl LayoutPassState<'_> {
    pub(super) fn compute_inline_layout_inner(
        &mut self,
        node_id: NodeId,
        inputs: taffy::tree::LayoutInput,
        block_ctx: &mut BlockContext<'_>,
    ) -> taffy::LayoutOutput {
        let scale = self.viewport.scale();
        let LayoutInput {
            known_dimensions,
            parent_size,
            available_space,
            sizing_mode,
            ..
        } = inputs;

        // Take inline layout to satisfy borrow checker
        let mut inline_layout = self.nodes[node_id]
            .data
            .downcast_element_mut()
            .unwrap()
            .take_inline_layout()
            .unwrap();

        let style = self.nodes[node_id].layout_style();

        // Note: both horizontal and vertical percentage padding/borders are resolved against the container's inline size (i.e. width).
        // This is not a bug, but is how CSS is specified (see: https://developer.mozilla.org/en-US/docs/Web/CSS/padding#values)
        let padding = style
            .padding()
            .resolve_or_zero(parent_size.width, resolve_calc_value);
        let border = style
            .border()
            .resolve_or_zero(parent_size.width, resolve_calc_value);
        let container_pb = padding + border;
        let pb_sum = container_pb.sum_axes();
        let box_sizing_adjustment = if style.box_sizing() == BoxSizing::ContentBox {
            pb_sum
        } else {
            Size::ZERO
        };

        // Scrollbar gutters are reserved when the `overflow` property is set to `Overflow::Scroll`.
        // However, the axis are switched (transposed) because a node that scrolls vertically needs
        // *horizontal* space to be reserved for a scrollbar
        let scrollbar_gutter = style.overflow().transpose().map(|overflow| match overflow {
            Overflow::Scroll => style.scrollbar_width(),
            _ => 0.0,
        });
        // TODO: make side configurable based on the `direction` property
        let mut content_box_inset = container_pb;
        content_box_inset.right += scrollbar_gutter.x;
        content_box_inset.bottom += scrollbar_gutter.y;

        let has_styles_preventing_being_collapsed_through = !style.is_block()
            || style.overflow().x.is_scroll_container()
            || style.overflow().y.is_scroll_container()
            || style.position().is_out_of_flow()
            || padding.top > 0.0
            || padding.bottom > 0.0
            || border.top > 0.0
            || border.bottom > 0.0;
        // || matches!(node_size.height, Some(h) if h > 0.0)
        // || matches!(node_min_size.height, Some(h) if h > 0.0)
        // || !inline_layout.text.is_empty();
        // || inline_layout.layout.inline_boxes().len() > 0;

        // Resolve node's preferred/min/max sizes (width/heights) against the available space (percentages resolve to pixel values)
        // For ContentSize mode, we pretend that the node has no size styles as these should be ignored.
        let (node_size, node_min_size, node_max_size, aspect_ratio) = match sizing_mode {
            SizingMode::ContentSize => {
                let node_size = known_dimensions;
                let node_min_size = Size::NONE;
                let node_max_size = Size::NONE;
                (node_size, node_min_size, node_max_size, None)
            }
            SizingMode::InherentSize => {
                let aspect_ratio = style.aspect_ratio();
                let style_size = style
                    .size()
                    .maybe_resolve(parent_size, resolve_calc_value)
                    .maybe_apply_aspect_ratio(aspect_ratio)
                    .maybe_add(box_sizing_adjustment);
                let style_min_size = style
                    .min_size()
                    .maybe_resolve(parent_size, resolve_calc_value)
                    .maybe_apply_aspect_ratio(aspect_ratio)
                    .maybe_add(box_sizing_adjustment);
                let style_max_size = style
                    .max_size()
                    .maybe_resolve(parent_size, resolve_calc_value)
                    .maybe_add(box_sizing_adjustment);

                let node_size =
                    known_dimensions.or(style_size.maybe_clamp(style_min_size, style_max_size));
                (node_size, style_min_size, style_max_size, aspect_ratio)
            }
        };

        let suppresses_baseline = style.contain().suppresses_baseline();
        drop(style);

        // Short circuit if inline context contains no text or inline boxes
        if !has_styles_preventing_being_collapsed_through
            && inline_layout.text.is_empty()
            && inline_layout.layout.inline_boxes().len() == 0
        {
            // Put layout back
            self.nodes[node_id]
                .data
                .downcast_element_mut()
                .unwrap()
                .inline_layout_data = Some(inline_layout);
            return LayoutOutput::from_outer_size(
                Size::ZERO.maybe_max(container_pb.sum_axes().map(Some)),
            );
        }

        // Meari: the available-space, inline-box sizing, content-width and inline-box placement
        // steps live in `inline_steps.rs` so this file meets the lint-structure length limit.
        let sizing = InlineSizing {
            inputs,
            known_dimensions,
            available_space,
            parent_size,
            node_size,
            node_min_size,
            node_max_size,
            content_box_inset,
            scale,
        };
        let (available_space, child_inputs) = self.inline_child_inputs(node_id, &sizing);
        #[cfg(feature = "floats")]
        let float_child_inputs = taffy::tree::LayoutInput {
            available_space: Size::MAX_CONTENT,
            ..child_inputs
        };

        // Update inline boxes
        self.size_inline_boxes(&mut inline_layout, inputs, child_inputs, scale);

        let text_indent = self.nodes[node_id]
            .primary_styles()
            .map(|s| s.clone_text_indent())
            .unwrap_or_else(GenericTextIndent::zero);
        let indent_options = IndentOptions {
            each_line: text_indent.each_line,
            hanging: text_indent.hanging,
        };
        // Percentage indents do not contribute to intrinsic widths.
        inline_layout.layout.set_text_indent(
            text_indent.length.resolve(CSSPixelLength::new(0.0)).px() * scale,
            indent_options,
        );

        let pbw = container_pb.horizontal_components().sum() * scale;
        let width = self.inline_content_width(
            &mut inline_layout,
            &sizing,
            available_space,
            child_inputs,
            pbw,
        );

        #[cfg(not(feature = "floats"))]
        let _ = block_ctx; // Suppress unused variable warning

        // Set block context width if this is a block context root
        #[cfg(feature = "floats")]
        let is_bfc_root = block_ctx.is_bfc_root();
        #[cfg(feature = "floats")]
        if is_bfc_root {
            block_ctx.set_width((width + pbw) / scale);
        }

        // Create sub-context to account for the inline layout's padding/border
        #[cfg(feature = "floats")]
        let outer_block_ctx = block_ctx;
        #[cfg(feature = "floats")]
        let mut block_ctx =
            outer_block_ctx.sub_context(container_pb.top, [container_pb.left, container_pb.right]);
        // block_ctx.apply_content_box_inset([container_pb.left, container_pb.right]);

        if inputs.run_mode == taffy::RunMode::ComputeSize
            && inputs.axis == RequestedAxis::Horizontal
        {
            // Put layout back
            self.nodes[node_id]
                .data
                .downcast_element_mut()
                .unwrap()
                .inline_layout_data = Some(inline_layout);

            let measured_size = inputs.known_dimensions.unwrap_or(taffy::Size {
                width: layout_unit_ceil(width / scale),
                // Height is ignored if RequestedAxis if Horizontal
                height: 0.0,
            });

            let clamped_size = inputs
                .known_dimensions
                .or(node_size)
                .unwrap_or(measured_size + content_box_inset.sum_axes())
                .maybe_clamp(node_min_size, node_max_size)
                .maybe_max(container_pb.sum_axes().map(Some));

            return LayoutOutput::from_outer_size(clamped_size);
        }

        let resolved_text_indent = text_indent
            .length
            .resolve(CSSPixelLength::new((width / scale).max(0.0)))
            .px();
        inline_layout
            .layout
            .set_text_indent(resolved_text_indent * scale, indent_options);

        #[cfg(not(feature = "floats"))]
        {
            inline_layout.layout.break_all_lines(Some(width));
        }

        // Out-of-flow candidates bubbled up from this container and its in-flow subtree.
        // These are laid out by the out-of-flow positioning pass (`compute_oof_layout`).
        let mut oof_candidates = OofCandidates::new();

        // Perform inline layout
        #[cfg(feature = "floats")]
        {
            let mut breaker = inline_layout.layout.break_lines();
            let initial_slot = block_ctx.find_content_slot(0.0, Clear::None, None);
            let mut has_active_floats = initial_slot.segment_id.is_some();
            let state = breaker.state_mut();
            state.set_layout_max_advance(width);
            state.set_line_max_advance(initial_slot.width * scale);
            state.set_line_x(initial_slot.x * scale);
            state.set_line_y((initial_slot.y * scale) as f64);

            // TODO: revert state and retry layout if a line doesn't fit
            //
            // Save initial state. Saved state is used to revert the layout to a previous state if needed
            // (e.g. to revert a line that doesn't fit in the space it was laid out into)
            //
            // let mut saved_state = breaker.state().clone();

            while let Some(yield_data) = breaker.break_next() {
                match yield_data {
                    YieldData::LineBreak(_line_break_data) => {
                        let state = breaker.state_mut();

                        if has_active_floats {
                            // TODO: revert state and retry layout if a line doesn't fit
                            // saved_state = state.clone();

                            let min_y = state.line_y() / scale as f64;
                            let next_slot =
                                block_ctx.find_content_slot(min_y as f32, Clear::None, None);
                            has_active_floats = next_slot.segment_id.is_some();

                            state.set_line_max_advance(next_slot.width * scale);
                            state.set_line_x(next_slot.x * scale);
                            state.set_line_y((next_slot.y * scale) as f64);
                        } else {
                            state.set_line_x(0.0);
                            state.set_line_max_advance(width);
                        }

                        continue;
                    }
                    YieldData::MaxHeightExceeded(_data) => {
                        // TODO
                        continue;
                    }
                    YieldData::InlineBoxBreak(box_break_data) => {
                        let state = breaker.state_mut();
                        let node_id = NodeId::from_u64(box_break_data.inline_box_id);

                        let (direction, clear, margin) = {
                            let style = self.child_layout_style(&self.nodes[node_id]);
                            // We can assume that the box is a float because we only set `break_on_box: true` for floats
                            let direction = match style.float() {
                                Float::Left => taffy::FloatDirection::Left,
                                Float::Right => taffy::FloatDirection::Right,
                                Float::None => unreachable!(),
                            };
                            (
                                direction,
                                style.clear(),
                                style
                                    .margin()
                                    .resolve_or_zero(inputs.parent_size, resolve_calc_value),
                            )
                        };

                        let margin_sum = margin.sum_axes();

                        let mut output = self.compute_child_layout(
                            crate::taffy_node_id(node_id),
                            float_child_inputs,
                        );
                        let min_y = state.line_y() as f32 / scale;

                        // Note: `pos` is content-box relative
                        let pos = block_ctx.place_floated_box(
                            output.size + margin_sum,
                            min_y,
                            direction,
                            clear,
                            false,
                        );

                        let min_y = state.line_y() / scale as f64; //.max(pos.y as f64);
                        let next_slot =
                            block_ctx.find_content_slot(min_y as f32, Clear::None, None);
                        has_active_floats = next_slot.segment_id.is_some();

                        state.set_line_max_advance(next_slot.width * scale);
                        state.set_line_x(next_slot.x * scale);
                        state.set_line_y((next_slot.y * scale) as f64);

                        let location = taffy::Point {
                            x: pos.x + margin.left + container_pb.left,
                            y: pos.y + margin.top + container_pb.top,
                        };
                        let layout = self.nodes[node_id].unrounded_layout_mut();
                        layout.size = output.size;
                        layout.location = location;
                        layout.margin = margin;

                        // Translate anchors from item-relative to container-relative
                        // coordinates and collect candidates bubbled from the float's subtree
                        if !output.oof_candidates.is_empty() {
                            output.oof_candidates.translate(location);
                            oof_candidates.append(&mut output.oof_candidates);
                        }

                        // dbg!(&layout.size);
                        // dbg!(&layout.location);

                        // Floats are out-of-flow and must not contribute to the line's height.
                        state.append_inline_box_to_line(
                            box_break_data.advance,
                            f32::NEG_INFINITY,
                            f32::NEG_INFINITY,
                        );

                        // if float.is_floated() {
                        //     println!("INLINE FLOATED BOX ({}) {:?}", ibox.id, float);
                        //     println!(
                        //         "w:{} h:{} x:{}, y:{}",
                        //         layout.size.width, layout.size.height, 0, 0
                        //     );
                        // }
                    }
                }
            }
            breaker.finish();
        }

        // Propagate the height consumed by floats placed within this container to the
        // enclosing block context (so that the BFC root can contain them)
        #[cfg(feature = "floats")]
        let float_height_contribution = block_ctx.floated_content_height_contribution();
        #[cfg(feature = "floats")]
        outer_block_ctx.add_child_floated_content_height_contribution(
            container_pb.top + float_height_contribution,
        );

        let (alignment, last_line_alignment) = self.nodes[node_id]
            .primary_styles()
            .map(|s| {
                (
                    stylo_to_parley::text_align(s.clone_text_align()),
                    stylo_to_parley::text_align_last(s.clone_text_align_last()),
                )
            })
            .unwrap_or((parley::layout::Alignment::Start, None));

        inline_layout.layout.align(
            alignment,
            AlignmentOptions {
                align_when_overflowing: false,
                last_line_alignment,
            },
        );

        // Parley lays out empty text as a single strut-height line (text-editor semantics),
        // but a line box containing no text, inline boxes or other in-flow content is a
        // zero-height line box in CSS (CSS2 §9.4.2).
        let has_inline_content =
            !inline_layout.text.is_empty() || inline_layout.layout.inline_boxes().len() > 0;

        let mut height = if has_inline_content {
            inline_layout.layout.height()
        } else {
            0.0
        };

        // A forced line break (e.g. `<br>` or a preserved newline) at the end of the inline
        // content ends the final line box without starting a new one. Parley still emits an
        // empty line after it (so that editors have a line to place the cursor on), so that
        // line is excluded from the measured height (and from the last baseline).
        let line_count = inline_layout.layout.len();
        let mut trailing_empty_line = None;
        if line_count >= 2
            && let (Some(prev_line), Some(last_line)) = (
                inline_layout.layout.get(line_count - 2),
                inline_layout.layout.get(line_count - 1),
            )
            && prev_line.break_reason() == BreakReason::Explicit
            && last_line.text_range().is_empty()
            && last_line.items().next().is_none()
        {
            height -= last_line.metrics().line_height;
            trailing_empty_line = Some(line_count - 1);
        }

        #[cfg(feature = "floats")]
        {
            if is_bfc_root {
                height = height.max(float_height_contribution * scale)
            };
        }

        // Note: `width` and `height` are content-box measurements of the inline content.
        // `known_dimensions` must not be substituted in here: those are border-box sizes, and
        // using them for the scrollable overflow rect (which adds padding below) would
        // double-count padding, incorrectly making the container's content overflow it.
        let measured_size = taffy::Size {
            width: width / scale,
            height: height / scale,
        };

        // Unbreakable content (e.g. `white-space: pre` text) may overflow the max advance the
        // lines were broken at. `width` is the advance the lines were broken at, so the actual
        // extent of the line boxes must be taken from the laid-out lines for `content_size`.
        let content_width = f32_max(width, inline_layout.layout.width()) / scale;

        let clamped_size = inputs
            .known_dimensions
            .or(node_size)
            .unwrap_or(measured_size + content_box_inset.sum_axes())
            .maybe_clamp(node_min_size, node_max_size);
        let final_size = Size {
            width: clamped_size.width,
            height: f32_max(
                clamped_size.height,
                aspect_ratio
                    .map(|ratio| clamped_size.width / ratio)
                    .unwrap_or(0.0),
            ),
        }
        .maybe_max(container_pb.sum_axes().map(Some));

        let container_direction = self.nodes[node_id].layout_style().direction();

        // `align-content` aligns the line boxes (as a single unit) within the content box in
        // the block axis. Floats are positioned relative to the formatting context and are not
        // moved. The offset is stored on the text layout for painting and hit-testing.
        let align_content = BlockContainerStyle::align_content(&self.nodes[node_id].layout_style());
        let block_offset = if align_content.keyword() == taffy::AlignContentKeyword::Normal {
            0.0
        } else {
            let free_space =
                final_size.height - content_box_inset.vertical_axis_sum() - measured_size.height;
            taffy::compute_block_align_content_offset(align_content, free_space)
        };
        inline_layout.block_offset = block_offset;
        let line_box_top = container_pb.top + block_offset;

        // Store sizes and positions of inline boxes
        let mut ibox_order: u32 = 0;
        let placement = InlineBoxPlacement {
            child_inputs,
            final_size,
            content_box_inset,
            container_pb,
            container_direction,
            line_box_top,
            scale,
        };
        let mut span_cb_flags = StyleFlags::empty();
        for line in inline_layout.layout.lines() {
            for item in line.items() {
                if let parley::layout::PositionedLayoutItem::InlineBox(ibox) = item {
                    let order = ibox_order;
                    ibox_order += 1;
                    if inputs.run_mode == RunMode::PerformLayout {
                        span_cb_flags |=
                            self.inline_span_cb_flags(node_id, NodeId::from_u64(ibox.id));
                    }
                    self.place_inline_box(
                        &ibox,
                        line.metrics(),
                        order,
                        &placement,
                        &mut oof_candidates,
                    );
                }
            }
        }

        // println!("INLINE LAYOUT FOR {:?}. max_advance: {:?}", node_id, max_advance);
        // dbg!(&inline_layout.text);
        // println!("Computed: w: {} h: {}", inline_layout.layout.width(), inline_layout.layout.height());
        // println!("known_dimensions: w: {:?} h: {:?}", inputs.known_dimensions.width, inputs.known_dimensions.height);
        // println!("\n");

        let line_baseline =
            |line: parley::Line<'_, _>| (line.metrics().baseline / scale) + line_box_top;
        let exports_baseline = has_inline_content && !suppresses_baseline;
        let first_baseline = exports_baseline
            .then(|| inline_layout.layout.lines().next().map(line_baseline))
            .flatten();
        let last_line_index = trailing_empty_line.unwrap_or(line_count).checked_sub(1);
        let last_baseline = exports_baseline
            .then(|| last_line_index.and_then(|i| inline_layout.layout.get(i)))
            .flatten()
            .map(line_baseline);

        if inputs.run_mode == RunMode::PerformLayout {
            inline_layout.span_cb_flags = span_cb_flags;
        }

        // Put layout back
        self.nodes[node_id]
            .data
            .downcast_element_mut()
            .unwrap()
            .inline_layout_data = Some(inline_layout);

        let oof_position_inset = taffy::Rect {
            left: border.left,
            right: border.right + scrollbar_gutter.x,
            top: border.top,
            bottom: border.bottom + scrollbar_gutter.y,
        };

        LayoutOutput {
            size: final_size,
            scrollable_overflow_rect: {
                let content_extent = taffy::Size {
                    width: content_width,
                    height: measured_size.height,
                } + padding.sum_axes();
                taffy::Rect {
                    left: 0.0,
                    right: content_extent.width,
                    top: 0.0,
                    bottom: content_extent.height + block_offset.max(0.0),
                }
            },
            baselines: taffy::Baselines {
                first: first_baseline,
                last: last_baseline,
            },
            top_margin: CollapsibleMarginSet::ZERO,
            bottom_margin: CollapsibleMarginSet::ZERO,
            margins_can_collapse_through: !has_styles_preventing_being_collapsed_through
                && final_size.height == 0.0
                && measured_size.height == 0.0,
            oof_candidates,
            oof_positioning_area: Some(OofPositioningArea {
                size: final_size - oof_position_inset.sum_axes(),
                offset: Point {
                    x: oof_position_inset.left,
                    y: oof_position_inset.top,
                },
            }),
        }
    }
}
