//! Meari: steps of `compute_inline_layout_inner` moved out of `inline.rs` verbatim so each
//! file stays within the lint-structure length limit. Behaviour is unchanged.

use super::*;
use crate::node::TextLayout;
use parley::layout::{LineMetrics, PositionedInlineBox};

/// Values of `compute_inline_layout_inner` shared by the sizing steps.
pub(super) struct InlineSizing {
    pub(super) inputs: taffy::tree::LayoutInput,
    pub(super) known_dimensions: Size<Option<f32>>,
    pub(super) available_space: Size<AvailableSpace>,
    pub(super) parent_size: Size<Option<f32>>,
    pub(super) node_size: Size<Option<f32>>,
    pub(super) node_min_size: Size<Option<f32>>,
    pub(super) node_max_size: Size<Option<f32>>,
    pub(super) content_box_inset: taffy::Rect<f32>,
    pub(super) scale: f32,
}

/// Values of `compute_inline_layout_inner` shared by inline-box placement.
pub(super) struct InlineBoxPlacement {
    pub(super) child_inputs: taffy::tree::LayoutInput,
    pub(super) final_size: Size<f32>,
    pub(super) content_box_inset: taffy::Rect<f32>,
    pub(super) container_pb: taffy::Rect<f32>,
    pub(super) container_direction: Direction,
    pub(super) line_box_top: f32,
    pub(super) scale: f32,
}

impl LayoutPassState<'_> {
    pub(super) fn inline_child_inputs(
        &self,
        node_id: NodeId,
        sizing: &InlineSizing,
    ) -> (Size<AvailableSpace>, taffy::tree::LayoutInput) {
        let InlineSizing {
            inputs,
            known_dimensions,
            available_space,
            parent_size,
            node_size,
            node_min_size,
            node_max_size,
            content_box_inset,
            ..
        } = *sizing;
        // Compute available space
        let available_space = Size {
            width: known_dimensions
                .width
                .map(AvailableSpace::from)
                .unwrap_or(available_space.width)
                .maybe_set(known_dimensions.width)
                .maybe_set(node_size.width)
                .map_definite_value(|size| {
                    (size.maybe_clamp(node_min_size.width, node_max_size.width)
                        - content_box_inset.horizontal_axis_sum())
                    .max(0.0)
                }),
            height: known_dimensions
                .height
                .map(AvailableSpace::from)
                .unwrap_or(available_space.height)
                .maybe_set(known_dimensions.height)
                .maybe_set(node_size.height)
                .map_definite_value(|size| {
                    (size.maybe_clamp(node_min_size.height, node_max_size.height)
                        - content_box_inset.vertical_axis_sum())
                    .max(0.0)
                }),
        };

        // Compute size of inline boxes
        let child_inputs = taffy::tree::LayoutInput {
            known_dimensions: Size::NONE,
            available_space,
            sizing_mode: SizingMode::InherentSize,
            parent_size: Size {
                width: available_space.width.into_option(),
                // Anonymous blocks do not establish the containing block for percentages.
                height: if self.nodes[node_id].is_anonymous() {
                    parent_size.height
                } else {
                    available_space.height.into_option()
                },
            },
            // Atomic inlines (e.g. inline-block) establish independent formatting
            // contexts: their margins never collapse with their children's margins.
            vertical_margins_are_collapsible: taffy::Line::FALSE,
            ..inputs
        };
        (available_space, child_inputs)
    }

    pub(super) fn size_inline_boxes(
        &mut self,
        inline_layout: &mut TextLayout,
        inputs: taffy::tree::LayoutInput,
        child_inputs: taffy::tree::LayoutInput,
        scale: f32,
    ) {
        // Update inline boxes
        for ibox in inline_layout.layout.inline_boxes_mut() {
            let style = self.child_layout_style(&self.nodes[NodeId::from_u64(ibox.id)]);
            let margin = style
                .margin()
                .resolve_or_zero(inputs.parent_size, resolve_calc_value);

            #[cfg(feature = "floats")]
            let is_floated = style.float().is_floated();
            #[cfg(not(feature = "floats"))]
            let is_floated = false;

            let is_out_of_flow = style.position().is_out_of_flow();
            // `baseline-source: auto` selects the last baseline for inline-blocks and the first
            // for other atomic inlines. A block-axis scroll-container inline-block (except buttons)
            // instead uses its bottom margin edge, but only with `auto` (css-align-3 §9.1).
            // `overflow: clip` is not a scroll container. Exported scroll-container baselines
            // are clamped to the border box.
            // A layout-contained box is treated as having no baseline (css-contain-1 §3.3).
            let overflow = style.overflow();
            let box_style = style.style.get_box();
            let is_flow = matches!(
                style.effective_display().inside(),
                DisplayInside::Flow | DisplayInside::FlowRoot
            );
            let baseline_source = box_style.clone_baseline_source();
            let uses_last_baseline = match baseline_source {
                BaselineSource::Auto => is_flow,
                BaselineSource::First => false,
                BaselineSource::Last => true,
            };
            let is_scroll_container = !matches!(overflow.y, Overflow::Visible | Overflow::Clip);
            let is_block_axis_scroll_container = is_flow
                && is_scroll_container
                && !style
                    .flags
                    .intersects(StyleFlags::IS_BUTTON | StyleFlags::IS_BUTTON_INPUT);
            let contain_layout = box_style.clone_contain().contains(Contain::LAYOUT);
            let exports_baseline = !(baseline_source == BaselineSource::Auto
                && is_block_axis_scroll_container)
                && !contain_layout;
            let box_inputs = inline_box_inputs(style.size().width, margin, child_inputs);
            drop(style);

            if is_out_of_flow || is_floated {
                ibox.width = 0.0;
                ibox.height = 0.0;
                ibox.baseline = None;
            } else {
                let output = self.compute_child_layout(taffy::NodeId::from(ibox.id), box_inputs);
                ibox.width = (margin.left + margin.right + output.size.width) * scale;
                ibox.baseline = if exports_baseline {
                    let baseline = if uses_last_baseline {
                        output.baselines.last.or(output.baselines.first)
                    } else {
                        output.baselines.first.or(output.baselines.last)
                    };
                    baseline.map(|baseline| {
                        let baseline = if is_scroll_container {
                            baseline.clamp(0.0, output.size.height)
                        } else {
                            baseline
                        };
                        (margin.top + baseline) * scale
                    })
                } else {
                    None
                };
                // Vertical margins adjust the space the box reserves in the line. A box with a
                // baseline splits that space into ascent (`margin.top + baseline`) and descent
                // (`margin.bottom + height - baseline`), either of which may be negative. A box
                // without a baseline sits on the baseline and cannot reserve negative space.
                // Kept finite: huge author lengths can sum to infinity, which is taller than
                // the line breaker's `f32::MAX` height limit, and it then yields
                // `MaxHeightExceeded` for this box forever without advancing.
                let margin_box_height = margin.top + margin.bottom + output.size.height;
                ibox.height = if ibox.baseline.is_some() {
                    (margin_box_height * scale).min(f32::MAX)
                } else {
                    (margin_box_height.max(0.0) * scale).min(f32::MAX)
                };
            }
        }
    }

    pub(super) fn inline_content_width(
        &mut self,
        inline_layout: &mut TextLayout,
        sizing: &InlineSizing,
        available_space: Size<AvailableSpace>,
        child_inputs: taffy::tree::LayoutInput,
        pbw: f32,
    ) -> f32 {
        let InlineSizing {
            inputs,
            known_dimensions,
            node_size,
            node_min_size,
            node_max_size,
            scale,
            ..
        } = *sizing;
        #[cfg(not(feature = "floats"))]
        let _ = (inputs, child_inputs); // Only read when sizing floats.
        known_dimensions
            .width
            .map(|w| (w * scale) - pbw)
            .unwrap_or_else(|| {
                // TODO: Cache content widths.
                //
                // This is a little tricky as the size of the inline boxes may depend on whether we are sizing under
                // and a min-content or max-content constraint. So if we want to compute both widths in one pass then
                // we need to store both a min-content and max-content size on each box.
                let content_sizes = inline_layout.layout.calculate_content_widths();
                let min_content_width = content_sizes.min;
                let max_content_width = content_sizes.max;

                #[cfg(feature = "floats")]
                let float_width = match available_space.width {
                    AvailableSpace::Definite(_) => 0.0,
                    AvailableSpace::MinContent => {
                        let mut width: f32 = 0.0;
                        for ibox in inline_layout.layout.inline_boxes_mut() {
                            let (is_floated, margin) = {
                                let style =
                                    self.child_layout_style(&self.nodes[NodeId::from_u64(ibox.id)]);
                                (
                                    style.float().is_floated(),
                                    style
                                        .margin()
                                        .resolve_or_zero(inputs.parent_size, resolve_calc_value),
                                )
                            };

                            if is_floated {
                                let output = self.compute_child_layout(
                                    taffy::NodeId::from(ibox.id),
                                    subtract_margins(child_inputs, margin),
                                );
                                width = width.max(output.size.width + margin.left + margin.right);
                            }
                        }

                        width * scale
                    }
                    AvailableSpace::MaxContent => {
                        // When computing a max-content size the available width is effectively
                        // infinite, so floats never wrap onto a new "band" due to a lack of
                        // horizontal space. They only move below preceding floats when the `clear`
                        // property forces them to.
                        //
                        // Floats that share a band sit side-by-side and so their widths sum, whereas
                        // floats pushed onto a new band (via `clear`) stack vertically and so we
                        // take the maximum extent across bands rather than summing.
                        let mut left_band: f32 = 0.0;
                        let mut right_band: f32 = 0.0;
                        let mut width: f32 = 0.0;
                        for ibox in inline_layout.layout.inline_boxes_mut() {
                            let (float, clear, margin) = {
                                let style =
                                    self.child_layout_style(&self.nodes[NodeId::from_u64(ibox.id)]);
                                (
                                    style.float(),
                                    style.clear(),
                                    style
                                        .margin()
                                        .resolve_or_zero(inputs.parent_size, resolve_calc_value),
                                )
                            };

                            if float.is_floated() {
                                if matches!(clear, Clear::Left | Clear::Both) {
                                    left_band = 0.0;
                                }
                                if matches!(clear, Clear::Right | Clear::Both) {
                                    right_band = 0.0;
                                }

                                let output = self.compute_child_layout(
                                    taffy::NodeId::from(ibox.id),
                                    subtract_margins(child_inputs, margin),
                                );
                                let box_width = output.size.width + margin.left + margin.right;

                                match float {
                                    Float::Left => left_band += box_width,
                                    Float::Right => right_band += box_width,
                                    Float::None => {}
                                }
                                width = width.max(left_band + right_band);
                            }
                        }

                        width * scale
                    }
                };

                #[cfg(not(feature = "floats"))]
                let float_width = 0.0;

                let computed_width = match available_space.width {
                    AvailableSpace::MinContent => min_content_width.max(float_width),
                    AvailableSpace::MaxContent => max_content_width + float_width,
                    AvailableSpace::Definite(limit) => (limit * scale)
                        .min(max_content_width + float_width)
                        .max(min_content_width),
                };
                // Meari: Chrome snaps text content widths up to a 1/64px LayoutUnit, not a
                // whole pixel (upstream `.ceil()`), so right-aligned runs keep their position.
                let computed_width = layout_unit_ceil(computed_width / scale) * scale;

                let style_width = node_size.width.map(|w| w * scale);
                let min_width = node_min_size.width.map(|w| w * scale);
                let max_width = node_max_size.width.map(|w| w * scale);

                (style_width)
                    .unwrap_or(computed_width + pbw)
                    .max(computed_width)
                    .maybe_clamp(min_width, max_width)
                    - pbw
            })
    }

    pub(super) fn place_inline_box(
        &mut self,
        ibox: &PositionedInlineBox,
        line_metrics: &LineMetrics,
        order: u32,
        placement: &InlineBoxPlacement,
        oof_candidates: &mut OofCandidates,
    ) {
        let InlineBoxPlacement {
            child_inputs,
            final_size,
            content_box_inset,
            container_pb,
            container_direction,
            line_box_top,
            scale,
        } = *placement;
        let node = &self.nodes[NodeId::from_u64(ibox.id)];
        let style = self.child_layout_style(node);
        let padding = style
            .padding()
            .resolve_or_zero(child_inputs.parent_size, resolve_calc_value);
        let border = style
            .border()
            .resolve_or_zero(child_inputs.parent_size, resolve_calc_value);
        let margin = style
            .margin()
            .resolve_or_zero(child_inputs.parent_size, resolve_calc_value);

        #[cfg(feature = "floats")]
        let is_floated = style.float() != Float::None;
        #[cfg(not(feature = "floats"))]
        let is_floated = false;

        let position = style.position();
        let is_absolute = position.is_out_of_flow();
        let item_direction = style.direction();
        // Inline formatting contexts have no `justify-items`/`align-items` for an
        // `auto` self-alignment to defer to, so it behaves as `normal`.
        let justify_self = OofItemStyle::justify_self(&style).unwrap_or(taffy::AlignItems::NORMAL);
        let align_self = OofItemStyle::align_self(&style).unwrap_or(taffy::AlignItems::NORMAL);

        // The static position of an absolutely positioned box depends on the
        // display its hypothetical box would have had (the display specified
        // before position:absolute blockified it): inline-level boxes sit at
        // their position within the line, while block-level boxes start at the
        // content-box left edge of their containing block.
        let is_inline_level =
            style.style.get_box().original_display.outside() == DisplayOutside::Inline;

        // Resolve relative inset offsets against the containing block
        // (the content box of the inline container).
        let container_content_size = final_size - content_box_inset.sum_axes();
        let inset_style = style.inset();
        let inset = taffy::Rect {
            left: inset_style
                .left
                .maybe_resolve(container_content_size.width, resolve_calc_value),
            right: inset_style
                .right
                .maybe_resolve(container_content_size.width, resolve_calc_value),
            top: inset_style
                .top
                .maybe_resolve(container_content_size.height, resolve_calc_value),
            bottom: inset_style
                .bottom
                .maybe_resolve(container_content_size.height, resolve_calc_value),
        };
        let box_inputs = inline_box_inputs(style.size().width, margin, child_inputs);
        drop(style);

        if is_absolute {
            // The static-position rectangle
            // (https://www.w3.org/TR/css-position-3/#staticpos-rect):
            // - An inline-level box's rectangle is zero-width at its position
            //   within the line (`ibox.y` is the baseline as out-of-flow boxes are
            //   zero-sized) and spans the line box in the block axis.
            // - A block-level box's rectangle spans the containing block's content
            //   box in the inline axis and is zero-height below the line box.
            let line_top = (line_metrics.block_min_coord / scale) + line_box_top;
            let line_bottom = (line_metrics.block_max_coord / scale) + line_box_top;
            let (inline_area, block_area) = if is_inline_level {
                let x = (ibox.x / scale) + container_pb.left;
                (
                    taffy::Line { start: x, end: x },
                    taffy::Line {
                        start: line_top,
                        end: line_bottom,
                    },
                )
            } else {
                (
                    taffy::Line {
                        start: container_pb.left,
                        end: final_size.width - container_pb.right,
                    },
                    taffy::Line {
                        start: line_bottom,
                        end: line_bottom,
                    },
                )
            };

            oof_candidates.push(OofCandidate {
                node: taffy::NodeId::from(ibox.id),
                order,
                position,
                static_position: taffy::Point {
                    x: AxisStaticPosition::from_alignment(
                        justify_self.resolve_self_relative(
                            item_direction,
                            container_direction,
                            true,
                        ),
                        inline_area,
                        container_direction.is_rtl(),
                    ),
                    y: AxisStaticPosition::from_alignment(
                        align_self.resolve_self_relative(
                            item_direction,
                            container_direction,
                            false,
                        ),
                        block_area,
                        false,
                    ),
                },
            });
        } else if is_floated {
            let layout = self.nodes[NodeId::from_u64(ibox.id)].unrounded_layout_mut();
            layout.padding = padding; //.map(|p| p / scale);
            layout.border = border; //.map(|p| p / scale);
            layout.margin = margin;
        } else {
            // Re-measure the box to get its border-box size (this hits the layout
            // cache). The size cannot be recovered from `ibox` dimensions as the
            // space reserved in the line is clamped to be non-negative.
            let mut output = self.compute_child_layout(taffy::NodeId::from(ibox.id), box_inputs);
            let size = output.size;
            let node = &mut self.nodes[NodeId::from_u64(ibox.id)];

            let is_relative = position == taffy::Position::Relative;
            let inset_offset = if is_relative {
                taffy::Point {
                    x: if container_direction == Direction::Rtl {
                        inset.right.map(|x| -x).or(inset.left).unwrap_or(0.0)
                    } else {
                        inset.left.or(inset.right.map(|x| -x)).unwrap_or(0.0)
                    },
                    y: inset.top.or(inset.bottom.map(|x| -x)).unwrap_or(0.0),
                }
            } else {
                taffy::Point::ZERO
            };

            let layout = node.unrounded_layout_mut();
            layout.size = size;
            layout.scrollable_overflow_rect = output.scrollable_overflow_rect;
            layout.location.x = (ibox.x / scale) + margin.left + container_pb.left + inset_offset.x;
            // Parley positions the margin box; offset to the border box even
            // when a negative top margin makes it extend above the margin box.
            layout.location.y = (ibox.y / scale) + margin.top + line_box_top + inset_offset.y;
            layout.padding = padding; //.map(|p| p / scale);
            layout.border = border; //.map(|p| p / scale);
            layout.margin = margin;

            // Translate anchors from item-relative to container-relative
            // coordinates and collect candidates bubbled from the box's subtree
            if !output.oof_candidates.is_empty() {
                let location = layout.location;
                output.oof_candidates.translate(location);
                oof_candidates.append(&mut output.oof_candidates);
            }
        }
    }
}
