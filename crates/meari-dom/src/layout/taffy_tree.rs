use super::*;

impl LayoutPassState<'_> {
    pub(super) fn dispatch_child_layout(
        &mut self,
        node_id: NodeId,
        inputs: taffy::tree::LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> taffy::tree::LayoutOutput {
        let orthogonal_percent_basis = self.orthogonal_percent_basis_of(dom_node_id(node_id));
        let node = &mut self.nodes[dom_node_id(node_id)];

        let font_styles = node.primary_styles().map(|style| {
            use style::values::computed::font::LineHeight;

            let font_size = style.clone_font_size().used_size().px();
            let line_height = match style.clone_line_height() {
                LineHeight::Normal => font_size * 1.2,
                LineHeight::Number(num) => font_size * num.0,
                LineHeight::Length(value) => value.0.px(),
            };

            (font_size, line_height)
        });
        let font_size = font_styles.map(|s| s.0);
        let resolved_line_height = font_styles.map(|s| s.1);

        match &mut node.data {
            NodeData::Text(data) => {
                // With the new "inline context" architecture all text nodes should be wrapped in an "inline layout context"
                // and should therefore never be measured individually.
                #[cfg(feature = "tracing")]
                tracing::error!(
                    node_id = ?dom_node_id(node_id),
                    data = ?data,
                    "Tried to lay out text node individually",
                );

                #[cfg(not(feature = "tracing"))]
                let _ = data;

                taffy::LayoutOutput::HIDDEN
                // unreachable!();

                // compute_leaf_layout(inputs, &node.style, |known_dimensions, available_space| {
                //     let context = TextContext {
                //         text_content: &data.content.trim(),
                //         writing_mode: WritingMode::Horizontal,
                //     };
                //     let font_metrics = FontMetrics {
                //         char_width: 8.0,
                //         char_height: 16.0,
                //     };
                //     text_measure_function(
                //         known_dimensions,
                //         available_space,
                //         &context,
                //         &font_metrics,
                //     )
                // })
            }
            NodeData::Element(element_data) | NodeData::AnonymousBlock(element_data) => {
                // TODO: deduplicate with single-line text input
                if *element_data.name.local == *"textarea" {
                    let rows = element_data
                        .attr(local_name!("rows"))
                        .and_then(|val| val.parse::<f32>().ok())
                        .unwrap_or(2.0);

                    let cols = element_data
                        .attr(local_name!("cols"))
                        .and_then(|val| val.parse::<f32>().ok());

                    let output = compute_leaf_layout(
                        inputs,
                        &node.layout_style(),
                        resolve_calc_value,
                        |_known_size, _available_space| taffy::Size {
                            width: cols
                                .map(|cols| cols * font_size.unwrap_or(16.0) * 0.6)
                                .unwrap_or(300.0),
                            height: resolved_line_height.unwrap_or(16.0) * rows,
                        },
                    );
                    if inputs.run_mode == RunMode::PerformLayout {
                        let pb = {
                            let style = node.layout_style();
                            style
                                .padding()
                                .resolve_or_zero(inputs.parent_size.width, resolve_calc_value)
                                + style
                                    .border()
                                    .resolve_or_zero(inputs.parent_size.width, resolve_calc_value)
                        };
                        let content_width = (output.size.width - pb.horizontal_axis_sum()).max(0.0);
                        let doc = &mut *self.doc;
                        let scale = doc.viewport.scale();
                        let node = &mut doc.nodes[dom_node_id(node_id)];
                        if let Some(input) = node
                            .data
                            .downcast_element_mut()
                            .and_then(|el| el.text_input_data_mut())
                        {
                            input.editor.set_width(Some(content_width * scale));
                            input.editor.refresh_layout(
                                &mut doc.font_ctx.lock().unwrap(),
                                &mut doc.layout_ctx,
                            );
                        }
                    }
                    return output;
                }

                if *element_data.name.local == *"input" {
                    match element_data.attr(local_name!("type")) {
                        // if the input type is hidden, hide it
                        Some("hidden") => {
                            return taffy::LayoutOutput::HIDDEN;
                        }
                        Some("checkbox") => {
                            return compute_leaf_layout(
                                inputs,
                                &node.layout_style(),
                                resolve_calc_value,
                                |_known_size, _available_space| {
                                    let size = node.layout_style().size();
                                    let width = size.width.resolve_or_zero(
                                        inputs.parent_size.width,
                                        resolve_calc_value,
                                    );
                                    let height = size.height.resolve_or_zero(
                                        inputs.parent_size.height,
                                        resolve_calc_value,
                                    );
                                    let min_size = width.min(height);
                                    taffy::Size {
                                        width: min_size,
                                        height: min_size,
                                    }
                                },
                            );
                        }
                        None | Some("text" | "password" | "email" | "tel" | "url" | "search") => {
                            return compute_leaf_layout(
                                inputs,
                                &node.layout_style(),
                                resolve_calc_value,
                                |_known_size, _available_space| taffy::Size {
                                    width: match inputs.available_space.width {
                                        AvailableSpace::Definite(limit) => limit.min(300.0),
                                        AvailableSpace::MinContent => 0.0,
                                        AvailableSpace::MaxContent => 300.0,
                                    },
                                    height: resolved_line_height.unwrap_or(16.0),
                                },
                            );
                        }
                        _ => {}
                    }
                }

                if is_replaced_element(&element_data.name.local) {
                    // Width/height attributes are presentational hints mapped to the CSS
                    // width/height properties by
                    // `synthesize_presentational_hints_for_legacy_attributes`, so they
                    // are already part of the style. They are only read here for the
                    // elements whose attributes determine their *intrinsic* size
                    // (canvas, and custom widgets on canvas tags).
                    let attr_size = taffy::Size {
                        width: element_data
                            .attr(local_name!("width"))
                            .and_then(|val| val.parse::<f32>().ok()),
                        height: element_data
                            .attr(local_name!("height"))
                            .and_then(|val| val.parse::<f32>().ok()),
                    };

                    // Get the element's intrinsic dimensions and default object size
                    let (intrinsic_sizes, default_object_size) = match &element_data.special_data {
                        SpecialElementData::Image(image_data) => match &**image_data {
                            ImageData::Raster(image) => {
                                let (width, height) = (image.width as f32, image.height as f32);
                                (
                                    IntrinsicSizes {
                                        width: Some(width),
                                        height: Some(height),
                                        ratio: Some(width / height),
                                    },
                                    DEFAULT_OBJECT_SIZE,
                                )
                            }
                            #[cfg(feature = "svg")]
                            ImageData::Svg(svg) => {
                                let mut width = svg.intrinsic_width();
                                let mut height = svg.intrinsic_height();
                                // An SVG with no declared dimensions and no viewBox has no
                                // intrinsic dimensions per CSS, but usvg still resolves a
                                // concrete size; use it in place of the default object size.
                                if width.is_none()
                                    && height.is_none()
                                    && svg.viewbox_aspect_ratio().is_none()
                                {
                                    let size = svg.tree.size();
                                    width = Some(size.width());
                                    height = Some(size.height());
                                }
                                (
                                    IntrinsicSizes {
                                        width,
                                        height,
                                        ratio: Some(svg.aspect_ratio()),
                                    },
                                    DEFAULT_OBJECT_SIZE,
                                )
                            }
                            ImageData::None => (IntrinsicSizes::default(), taffy::Size::ZERO),
                        },
                        SpecialElementData::Canvas(_)
                        | SpecialElementData::SubDocument(_)
                        | SpecialElementData::None => {
                            tag_intrinsic_sizes(&element_data.name.local, attr_size)
                        }
                        #[cfg(feature = "custom-widget")]
                        SpecialElementData::CustomWidget(widget_data) => {
                            let (fallback, default_object_size) =
                                tag_intrinsic_sizes(&element_data.name.local, attr_size);
                            // A canvas's content attributes determine its intrinsic size,
                            // overriding the widget-reported one; the widget-reported size
                            // in turn overrides the tag's fallback.
                            let attr_intrinsic =
                                if *element_data.name.local == local_name!("canvas") {
                                    attr_size
                                } else {
                                    taffy::Size::NONE
                                };
                            let attr_ratio = match (attr_intrinsic.width, attr_intrinsic.height) {
                                (Some(w), Some(h)) => Some(w / h),
                                _ => None,
                            };
                            let widget_sizes = widget_data.widget.intrinsic_sizes();
                            (
                                IntrinsicSizes {
                                    width: attr_intrinsic
                                        .width
                                        .or(widget_sizes.width)
                                        .or(fallback.width),
                                    height: attr_intrinsic
                                        .height
                                        .or(widget_sizes.height)
                                        .or(fallback.height),
                                    ratio: attr_ratio.or(widget_sizes.ratio).or(fallback.ratio),
                                },
                                default_object_size,
                            )
                        }
                        _ => unreachable!(),
                    };

                    #[cfg(feature = "writing-mode")]
                    let (intrinsic_sizes, default_object_size) =
                        if node.writing_mode().is_vertical() {
                            // Intrinsic sizes are physical; the element's algorithm runs in its own writing mode
                            (
                                crate::layout::replaced::IntrinsicSizes {
                                    width: intrinsic_sizes.height,
                                    height: intrinsic_sizes.width,
                                    ratio: intrinsic_sizes.ratio.map(|ratio| 1.0 / ratio),
                                },
                                default_object_size.transpose(),
                            )
                        } else {
                            (intrinsic_sizes, default_object_size)
                        };

                    let replaced_context = ReplacedContext {
                        intrinsic_sizes,
                        default_object_size,
                    };

                    let mut style = node.layout_style();
                    style.set_percent_basis(orthogonal_percent_basis);
                    return compute_replaced_layout(
                        inputs,
                        &style,
                        resolve_calc_value,
                        &replaced_context,
                    );
                }

                if node.flags.is_table_root() {
                    // The space available to an absolutely positioned table never exceeds
                    // the size of its containing block (less the table's margins), even
                    // when negative insets would otherwise add to it
                    // (https://drafts.csswg.org/css-tables-3/#abspos).
                    let mut inputs = inputs;
                    if self.nodes[dom_node_id(node_id)].is_out_of_flow() {
                        let margin = self.nodes[dom_node_id(node_id)]
                            .layout_style()
                            .margin()
                            .resolve_or_zero(inputs.parent_size.width, resolve_calc_value);
                        let max_available_space = inputs
                            .parent_size
                            .maybe_sub(margin.sum_axes())
                            .maybe_max(taffy::Size::ZERO);
                        inputs.available_space =
                            inputs.available_space.maybe_min(max_available_space);
                    }

                    let SpecialElementData::TableRoot(context) = &self.nodes[dom_node_id(node_id)]
                        .data
                        .downcast_element()
                        .unwrap()
                        .special_data
                    else {
                        panic!("Node marked as table root but doesn't have TableContext");
                    };
                    let context = Arc::clone(context);

                    let mut table_wrapper = TableTreeWrapper {
                        doc: self,
                        ctx: context,
                    };
                    let mut output = compute_grid_layout(&mut table_wrapper, node_id, inputs);

                    // Absolutely positioned children take no part in the table grid. Their
                    // static position is the top-left of the table's content box.
                    let context = table_wrapper.ctx;
                    if inputs.run_mode == RunMode::PerformLayout && !context.oof_children.is_empty()
                    {
                        let style = &context.style;
                        let parent_width = inputs.parent_size.width;
                        let padding = style
                            .padding
                            .resolve_or_zero(parent_width, resolve_calc_value);
                        let border = style
                            .border
                            .resolve_or_zero(parent_width, resolve_calc_value);
                        let static_position = taffy::Point {
                            x: padding.left + border.left,
                            y: padding.top + border.top,
                        }
                        .map(|pos| AxisStaticPosition::from_edge(pos, AxisStaticEdge::Start));
                        let in_flow_count = context.cells.len();
                        for (idx, (child_id, position)) in context.oof_children.iter().enumerate() {
                            output.oof_candidates.push(OofCandidate {
                                node: taffy_node_id(*child_id),
                                order: (in_flow_count + idx) as u32,
                                position: *position,
                                static_position,
                            });
                        }
                    }

                    // HACK: Cap scrollable overflow at node size to prevent scrolling
                    output.scrollable_overflow_rect.left = 0.0;
                    output.scrollable_overflow_rect.top = 0.0;
                    output.scrollable_overflow_rect.right =
                        output.scrollable_overflow_rect.right.min(output.size.width);
                    output.scrollable_overflow_rect.bottom = output
                        .scrollable_overflow_rect
                        .bottom
                        .min(output.size.height);

                    return output;
                }

                if node.flags.is_inline_root() {
                    return self.compute_inline_layout(dom_node_id(node_id), inputs, block_ctx);
                }

                // The default CSS file will set
                match node.taffy_display() {
                    Display::Block => compute_block_layout(self, node_id, inputs, block_ctx),
                    Display::FlowRoot => compute_block_layout(self, node_id, inputs, None),
                    Display::Flex => {
                        #[cfg(feature = "writing-mode")]
                        {
                            self.current_align_axis_is_inline = node.is_column_flex_container();
                        }
                        compute_flexbox_layout(self, node_id, inputs)
                    }
                    Display::Grid => compute_grid_layout(self, node_id, inputs),
                    Display::None => taffy::LayoutOutput::HIDDEN,
                }
            }
            NodeData::Document(_) => compute_block_layout(self, node_id, inputs, None),

            _ => taffy::LayoutOutput::HIDDEN,
        }
    }
}

impl TraversePartialTree for LayoutPassState<'_> {
    type ChildIter<'a>
        = RefCellChildIter<'a>
    where
        Self: 'a;

    fn child_ids(&self, node_id: NodeId) -> Self::ChildIter<'_> {
        let layout_children = self.node_from_id(node_id).layout_children.borrow(); //.unwrap().as_ref();
        RefCellChildIter::new(Ref::map(layout_children, |children| {
            children.as_ref().map(|c| c.as_slice()).unwrap_or(&[])
        }))
    }

    fn child_count(&self, node_id: NodeId) -> usize {
        self.node_from_id(node_id)
            .layout_children
            .borrow()
            .as_ref()
            .map(|c| c.len())
            .unwrap_or(0)
    }

    fn get_child_id(&self, node_id: NodeId, index: usize) -> NodeId {
        taffy_node_id(
            self.node_from_id(node_id)
                .layout_children
                .borrow()
                .as_ref()
                .unwrap()[index],
        )
    }
}

impl TraverseTree for LayoutPassState<'_> {}

impl LayoutPartialTree for LayoutPassState<'_> {
    type CoreContainerStyle<'a>
        = TaffyStyloStyle<ComputedStyleRef<'a>>
    where
        Self: 'a;

    type CustomIdent = Atom;

    fn get_core_container_style(&self, node_id: NodeId) -> Self::CoreContainerStyle<'_> {
        #[cfg(feature = "writing-mode")]
        {
            // Container styles are read by the node's own algorithm (or, for the containing block
            // of an out-of-flow box, by that box's algorithm) and are expressed in the node's writing mode.
            let dom_id = dom_node_id(node_id);
            let mut style = self.nodes[dom_id].layout_style_in(self.layout_wm_of(dom_id));
            style.set_percent_basis(self.orthogonal_percent_basis_of(dom_id));
            style
        }
        #[cfg(not(feature = "writing-mode"))]
        {
            self.node_from_id(node_id).layout_style()
        }
    }

    fn set_unrounded_layout(&mut self, node_id: NodeId, layout: &Layout) {
        *self.node_from_id_mut(node_id).unrounded_layout_mut() = *layout;
    }

    fn resolve_calc_value(&self, calc_ptr: *const (), parent_size: f32) -> f32 {
        resolve_calc_value(calc_ptr, parent_size)
    }

    #[inline(always)]
    fn compute_child_layout(
        &mut self,
        node_id: NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            tree.compute_child_layout_internal(node_id, inputs, None)
        })
    }
}

impl LayoutContainingBlock for LayoutPassState<'_> {
    type OofItemStyle<'a>
        = TaffyStyloStyle<ComputedStyleRef<'a>>
    where
        Self: 'a;

    fn get_oof_item_style(&self, node_id: NodeId) -> Self::OofItemStyle<'_> {
        self.child_layout_style(self.node_from_id(node_id))
    }

    fn clear_hoisted_children(&mut self, node_id: NodeId) {
        let containing_block = dom_node_id(node_id);
        let mut hoisted = self.node_from_id(node_id).hoisted_children.borrow_mut();
        for &hoisted_id in hoisted.iter() {
            // A box which has since been claimed by another containing block
            // must keep pointing at that one.
            if let Some(node) = self.nodes.get(hoisted_id) {
                if node.oof_containing_block.get() == Some(containing_block) {
                    node.oof_containing_block.set(None);
                }
            }
        }
        hoisted.clear();
    }

    fn add_hoisted_children(&mut self, node_id: NodeId, hoisted: &[NodeId]) {
        let containing_block = dom_node_id(node_id);
        let node = self.node_from_id(node_id);
        node.hoisted_children
            .borrow_mut()
            .extend(hoisted.iter().copied().map(dom_node_id));
        for &hoisted_id in hoisted {
            self.node_from_id(hoisted_id)
                .oof_containing_block
                .set(Some(containing_block));
        }
    }

    fn get_detailed_layout_info(&self, node_id: NodeId) -> &DetailedLayoutInfo<Atom> {
        self.node_from_id(node_id)
            .element_data()
            .map(|element| &element.detailed_layout_info)
            .unwrap_or(&DetailedLayoutInfo::None)
    }
}
