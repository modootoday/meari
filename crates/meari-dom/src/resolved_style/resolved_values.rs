use super::*;

impl BaseDocument {
    /// Compute the resolved value of a CSS property for the given node, as exposed
    /// by `getComputedStyle()`. Returns an empty string for unknown properties and
    /// for nodes without styles.
    ///
    /// Layout-dependent properties (`width`/`height`, margins, padding, insets,
    /// `transform-origin`, `grid-template-rows`/`columns`) resolve to *used* values, so [`resolve`](Self::resolve) should be called
    /// before this method to ensure layout is up to date.
    pub fn resolved_style_value(&self, node_id: NodeId, property_name: &str) -> String {
        let Some(node) = self.get_node(node_id) else {
            return String::new();
        };
        // The computed style of an element that is not in the document is empty
        if !node.flags.is_in_document() {
            return String::new();
        }
        // Elements inside a `display: none` subtree are skipped by the style
        // traversal, so their style has to be computed on demand.
        let undisplayed_styles;
        let stored_styles = node.primary_styles();
        let styles: &ComputedValues = match &stored_styles {
            Some(styles) => styles,
            None => match self.resolve_undisplayed_style(node_id) {
                Some(styles) => {
                    undisplayed_styles = styles;
                    &undisplayed_styles
                }
                None => return String::new(),
            },
        };

        let display = styles.clone_display();
        // Non-atomic inline elements are laid out as style spans within their
        // inline root's text layout rather than as boxes of their own, so their
        // layout-dependent properties resolve to computed (not used) values.
        let is_non_atomic_inline = display.outside() == DisplayOutside::Inline
            && display.inside() == DisplayInside::Flow
            && !node.flags.is_inline_root()
            && node.element_data().is_none_or(|data| {
                let tag = &data.name.local;
                !(is_replaced_element(tag)
                    || *tag == local_name!("input")
                    || *tag == local_name!("textarea")
                    || *tag == local_name!("button"))
            });
        let generates_box =
            node.flags.is_in_document() && !display.is_none() && stored_styles.is_some();
        let has_layout_box = generates_box && !is_non_atomic_inline;

        let Ok(property_id) = PropertyId::parse_enabled_for_all_content(property_name) else {
            return String::new();
        };
        // Logical longhands resolve as the physical longhand they map to
        let property_name = match property_id.as_shorthand() {
            Err(PropertyDeclarationId::Longhand(longhand)) => {
                longhand.to_physical(styles.writing_mode).name()
            }
            _ => property_name,
        };

        // Used lengths are serialized unzoomed, like computed lengths
        let zoom = styles.effective_zoom.value();
        let format_used_px = |px: f32| format_px(px / zoom);

        // Layout-dependent "used value" special cases
        match property_name {
            // Shorthands of longhands that resolve to used values
            "margin" | "margin-block" | "margin-inline" | "padding" | "padding-block"
            | "padding-inline" | "inset" | "inset-block" | "inset-inline"
                if has_layout_box =>
            {
                if let Ok(shorthand) = property_id.as_shorthand() {
                    let sides: Vec<String> = shorthand
                        .longhands()
                        .map(|longhand| self.resolved_style_value(node_id, longhand.name()))
                        .collect();
                    return serialize_sides(&sides);
                }
            }
            "grid-template-columns" | "grid-template-rows"
                if display.inside() == DisplayInside::Grid =>
            {
                if let Some(info) =
                    node.element_data()
                        .and_then(|data| match &data.detailed_layout_info {
                            taffy::DetailedLayoutInfo::Grid(info) => Some(info),
                            _ => None,
                        })
                {
                    return if property_name == "grid-template-columns" {
                        info.grid_template_columns(zoom)
                    } else {
                        info.grid_template_rows(zoom)
                    };
                }
            }
            // Browsers serialize the `grid-template` shorthand from the computed
            // track lists, except that a `none` track list on a grid container
            // is replaced by the used (implicit) track sizes.
            "grid-template" if display.inside() == DisplayInside::Grid => {
                let pos_styles = styles.get_position();
                let rows_are_none =
                    matches!(pos_styles.grid_template_rows, GridTemplateComponent::None);
                let columns_are_none = matches!(
                    pos_styles.grid_template_columns,
                    GridTemplateComponent::None
                );
                let info = node
                    .element_data()
                    .and_then(|data| match &data.detailed_layout_info {
                        taffy::DetailedLayoutInfo::Grid(info) => Some(info),
                        _ => None,
                    });
                if let Some(info) = info
                    && (rows_are_none || columns_are_none)
                    && matches!(pos_styles.grid_template_areas, GridTemplateAreas::None)
                {
                    let rows = if rows_are_none {
                        info.grid_template_rows(zoom)
                    } else {
                        pos_styles.grid_template_rows.to_css_string()
                    };
                    let columns = if columns_are_none {
                        info.grid_template_columns(zoom)
                    } else {
                        pos_styles.grid_template_columns.to_css_string()
                    };
                    return format!("{rows} / {columns}");
                }
            }
            "width" | "height" if has_layout_box => {
                // Used value: the layout size interpreted according to `box-sizing`
                // (border-box size for `border-box`, content-box size for `content-box`)
                let layout = node.final_layout();
                let border_box = styles.get_position().box_sizing == BoxSizing::BorderBox;
                let size = if property_name == "width" {
                    if border_box {
                        layout.size.width
                    } else {
                        layout.size.width
                            - layout.border.left
                            - layout.border.right
                            - layout.padding.left
                            - layout.padding.right
                    }
                } else if border_box {
                    layout.size.height
                } else {
                    layout.size.height
                        - layout.border.top
                        - layout.border.bottom
                        - layout.padding.top
                        - layout.padding.bottom
                };
                return format_used_px(size.max(0.0));
            }
            "margin-top" | "margin-right" | "margin-bottom" | "margin-left" if has_layout_box => {
                // Used value: the margin resolved by layout (percentages and
                // `auto` margins resolved to lengths)
                let layout = node.final_layout();
                let margin = match property_name {
                    "margin-top" => layout.margin.top,
                    "margin-right" => layout.margin.right,
                    "margin-bottom" => layout.margin.bottom,
                    "margin-left" => layout.margin.left,
                    _ => unreachable!(),
                };
                return format_used_px(margin);
            }
            "padding-top" | "padding-right" | "padding-bottom" | "padding-left"
                if has_layout_box =>
            {
                // Used value: the padding resolved by layout (percentages
                // resolved to lengths)
                let layout = self.physical_unrounded_geometry(node_id).1;
                let padding = match property_name {
                    "padding-top" => layout.padding.top,
                    "padding-right" => layout.padding.right,
                    "padding-bottom" => layout.padding.bottom,
                    "padding-left" => layout.padding.left,
                    _ => unreachable!(),
                };
                return format_used_px(padding);
            }
            "transform-origin" | "perspective-origin" if has_layout_box => {
                // Used value: percentages resolved against the border box
                let layout = self.physical_unrounded_geometry(node_id).1;
                let resolve = |value: &LengthPercentage, basis: f32| {
                    format_used_px(value.resolve(CSSPixelLength::new(basis)).px())
                };
                let box_styles = styles.get_box();
                return if property_name == "transform-origin" {
                    let origin = &box_styles.transform_origin;
                    let x = resolve(&origin.horizontal, layout.size.width);
                    let y = resolve(&origin.vertical, layout.size.height);
                    if origin.depth.px() == 0.0 {
                        format!("{x} {y}")
                    } else {
                        format!("{x} {y} {}", format_used_px(origin.depth.px()))
                    }
                } else {
                    let origin = &box_styles.perspective_origin;
                    let x = resolve(&origin.horizontal, layout.size.width);
                    let y = resolve(&origin.vertical, layout.size.height);
                    format!("{x} {y}")
                };
            }
            "top" | "right" | "bottom" | "left" if has_layout_box => {
                let position = styles.clone_position();
                let parent_layout = node
                    .containing_block()
                    .and_then(|id| self.get_node(id))
                    .map(|parent| *parent.final_layout());

                match position {
                    // Used value: the relative offset. An `auto` side resolves
                    // to the negation of the opposite side. When both sides
                    // are set the axis is overconstrained and each side
                    // resolves to its computed value instead.
                    Position::Relative => {
                        let (cb_width, cb_height) = parent_layout
                            .map(|pl| {
                                (
                                    pl.size.width
                                        - pl.border.left
                                        - pl.border.right
                                        - pl.padding.left
                                        - pl.padding.right,
                                    pl.size.height
                                        - pl.border.top
                                        - pl.border.bottom
                                        - pl.padding.top
                                        - pl.padding.bottom,
                                )
                            })
                            .unwrap_or((0.0, 0.0));
                        let pos_styles = styles.get_position();
                        let is_vertical = matches!(property_name, "top" | "bottom");
                        let basis = if is_vertical { cb_height } else { cb_width };
                        let (start, end) = if is_vertical {
                            (
                                resolve_inset(&pos_styles.top, basis),
                                resolve_inset(&pos_styles.bottom, basis),
                            )
                        } else {
                            (
                                resolve_inset(&pos_styles.left, basis),
                                resolve_inset(&pos_styles.right, basis),
                            )
                        };
                        let is_start = matches!(property_name, "top" | "left");
                        let used = match (start, end) {
                            (Some(start), Some(end)) => {
                                if is_start {
                                    start
                                } else {
                                    end
                                }
                            }
                            (Some(start), None) => {
                                if is_start {
                                    start
                                } else {
                                    -start
                                }
                            }
                            (None, Some(end)) => {
                                if is_start {
                                    -end
                                } else {
                                    end
                                }
                            }
                            (None, None) => 0.0,
                        };
                        return format_used_px(used);
                    }
                    // A specified (non-`auto`) inset resolves as-is (with
                    // percentages resolved against the containing block), even
                    // when overconstrained. An `auto` inset resolves to the
                    // used distance between the box's margin edge and the
                    // corresponding edge of the containing block's padding box.
                    Position::Absolute | Position::Fixed => {
                        if let Some(pl) = parent_layout {
                            let cb_width = pl.size.width - pl.border.left - pl.border.right;
                            let cb_height = pl.size.height - pl.border.top - pl.border.bottom;

                            let pos_styles = styles.get_position();
                            let (inset, basis) = match property_name {
                                "top" => (&pos_styles.top, cb_height),
                                "bottom" => (&pos_styles.bottom, cb_height),
                                "left" => (&pos_styles.left, cb_width),
                                "right" => (&pos_styles.right, cb_width),
                                _ => unreachable!(),
                            };
                            if let Some(value) = resolve_inset(inset, basis) {
                                return format_used_px(value);
                            }

                            let layout = node.final_layout();
                            let margin_box_top =
                                layout.location.y - layout.margin.top - pl.border.top;
                            let margin_box_left =
                                layout.location.x - layout.margin.left - pl.border.left;
                            let margin_box_width =
                                layout.margin.left + layout.size.width + layout.margin.right;
                            let margin_box_height =
                                layout.margin.top + layout.size.height + layout.margin.bottom;
                            let used = match property_name {
                                "top" => margin_box_top,
                                "bottom" => cb_height - margin_box_top - margin_box_height,
                                "left" => margin_box_left,
                                "right" => cb_width - margin_box_left - margin_box_width,
                                _ => unreachable!(),
                            };
                            return format_used_px(used);
                        }
                    }
                    // `static` and `sticky` boxes resolve to the computed value
                    _ => {}
                }
            }
            // `min-width: auto` / `min-height: auto` resolve to `auto` only for
            // boxes with a preferred aspect ratio and for flex and grid items;
            // otherwise (including when no box is generated) they resolve to `0px`
            "min-width" | "min-height" => {
                let pos_styles = styles.get_position();
                let is_auto = if property_name == "min-width" {
                    pos_styles.min_width.is_auto()
                } else {
                    pos_styles.min_height.is_auto()
                };
                let has_aspect_ratio =
                    !matches!(pos_styles.aspect_ratio.ratio, PreferredRatio::None);
                let preserves_auto =
                    generates_box && (has_aspect_ratio || self.is_flex_or_grid_item(node_id));
                if is_auto && !preserves_auto {
                    return format_px(0.0);
                }
            }
            "transform" if has_layout_box => {
                let transform = &styles.get_box().transform;
                if !transform.0.is_empty() {
                    // Used value: the transform list resolved to a matrix, with
                    // percentages resolved against the border box
                    let layout = node.final_layout();
                    let reference_box = euclid::Rect::new(
                        euclid::Point2D::new(CSSPixelLength::new(0.0), CSSPixelLength::new(0.0)),
                        euclid::Size2D::new(
                            CSSPixelLength::new(layout.size.width),
                            CSSPixelLength::new(layout.size.height),
                        ),
                    );
                    if let Ok((matrix, is_3d)) =
                        transform.to_transform_3d_matrix_f64(Some(&reference_box))
                    {
                        return format_matrix(&matrix, is_3d);
                    }
                }
            }
            _ => {}
        }

        // General case: serialize the stylo computed value
        let css = match property_id.as_shorthand() {
            // Serialize shorthands from the resolved values of their longhands
            Ok(shorthand) => serialize_resolved_shorthand(styles, shorthand),
            Err(declaration_id) => styles.computed_value_to_string(declaration_id),
        };
        canonicalize_computed_alignment(property_name, css)
    }

    /// Whether the node's box is a flex or grid item, i.e. its nearest ancestor
    /// that generates a box (skipping `display: contents` ancestors) is a flex
    /// or grid container.
    pub(super) fn is_flex_or_grid_item(&self, node_id: NodeId) -> bool {
        let mut parent_id = self.get_node(node_id).and_then(|node| node.parent);
        while let Some(parent) = parent_id.and_then(|id| self.get_node(id)) {
            let Some(parent_styles) = parent.primary_styles() else {
                return false;
            };
            let display = parent_styles.clone_display();
            if display.is_contents() {
                parent_id = parent.parent;
                continue;
            }
            return matches!(display.inside(), DisplayInside::Flex | DisplayInside::Grid);
        }
        false
    }
}

/// Serialize the values of a two-sided (start, end) or four-sided (top, right,
/// bottom, left) shorthand, omitting sides that repeat the opposite side.
pub(super) fn serialize_sides(sides: &[String]) -> String {
    let len = match sides {
        [start, end] if start == end => 1,
        [top, right, bottom, left] if right == left => {
            if top != bottom {
                3
            } else if top != right {
                2
            } else {
                1
            }
        }
        _ => sides.len(),
    };
    sides[..len].join(" ")
}

/// Serialize the resolved value of a shorthand property by resolving each of
/// its longhands and re-serializing them as the shorthand (as
/// `getComputedStyle()` does for shorthands). Returns an empty string if the
/// longhand values cannot be represented as the shorthand.
pub(super) fn serialize_resolved_shorthand(
    styles: &ComputedValues,
    shorthand: ShorthandId,
) -> String {
    // `all` cannot be serialized from longhands
    if shorthand == ShorthandId::All {
        return String::new();
    }

    let declarations: Vec<PropertyDeclaration> = shorthand
        .longhands()
        .map(|longhand| {
            let mut context = resolved::Context {
                style: styles,
                for_property: PropertyId::NonCustom(longhand.into()),
                current_longhand: Some(longhand),
            };
            styles.computed_or_resolved_declaration(longhand, Some(&mut context))
        })
        .collect();
    let declaration_refs: Vec<&PropertyDeclaration> = declarations.iter().collect();

    let mut css = CssStringWriter::new();
    let _ = shorthand.longhands_to_css(&declaration_refs, &mut css);
    css
}

/// Canonicalize the computed value of an alignment property the way css-align-3
/// now requires: `flex-start`/`flex-end` compute to `flow-start`/`flow-end`
/// (e.g. `align-self: flex-start` resolves to `flow-start`, `safe flex-end` to
/// `safe flow-end`). Stylo does not yet do this, so patch the serialized value
/// here until it does (see https://github.com/servo/stylo).
pub(super) fn canonicalize_computed_alignment(property_name: &str, value: String) -> String {
    const ALIGNMENT_PROPERTIES: [&str; 9] = [
        "align-content",
        "align-items",
        "align-self",
        "justify-content",
        "justify-items",
        "justify-self",
        "place-content",
        "place-items",
        "place-self",
    ];
    if !ALIGNMENT_PROPERTIES.contains(&property_name) || !value.contains("flex-") {
        return value;
    }
    value
        .replace("flex-start", "flow-start")
        .replace("flex-end", "flow-end")
}
