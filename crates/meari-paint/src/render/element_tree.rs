//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::filters::convert_filters;
use anyrender::PaintScene;
use blitz_dom::node::{NodeData, TextNodeData};
use blitz_dom::{NodeId, local_name};
use kurbo::{self, Affine, Point, Rect, Shape, Size, Vec2};
use std::sync::Arc;
use style::{
    dom::TElement,
    properties::generated::longhands::visibility::computed_value::T as StyloVisibility,
    values::{
        computed::{Contain, Overflow},
        specified::box_::{DisplayInside, DisplayOutside},
    },
};

impl<'dom, 'a> BlitzDomPainter<'dom, 'a> {
    /// Renders a node, but is guaranteed that the node is an element
    /// This is because the font_size is calculated from layout resolution and all text is rendered directly here, instead
    /// of a separate text stroking phase.
    ///
    /// In Blitz, text styling gets its attributes from its container element/resolved styles
    /// In other libraries, text gets its attributes from a `text` element - this is not how HTML works.
    ///
    /// Approaching rendering this way guarantees we have all the styles we need when rendering text with not having
    /// to traverse back to the parent for its styles, or needing to pass down styles
    pub(super) fn render_element(
        &self,
        scene: &mut impl PaintScene,
        node_id: NodeId,
        parent_style_transform: Affine,
        clip_rect: Rect,
    ) {
        let _surface_scope = self.surface_observer.as_ref().map(|observer| {
            crate::surface_observation::Collector::enter(observer, node_id.as_u64())
        });
        let node = &self.dom.as_ref().tree()[node_id];

        // Early return if the element is hidden
        if matches!(node.taffy_display(), taffy::Display::None) {
            return;
        }

        // Only draw elements with a style
        let Some(styles) = node.primary_styles() else {
            return;
        };

        // Hide inputs with type=hidden
        // Implemented here rather than using the style engine for performance reasons
        if node.local_name() == "input" && node.attr(local_name!("type")) == Some("hidden") {
            return;
        }

        // Hide elements with a visibility style other than visible
        if styles.get_inherited_box().visibility != StyloVisibility::Visible {
            return;
        }

        let effects = styles.get_effects();
        let opacity = effects.opacity;
        if opacity == 0.0 {
            return;
        }
        let has_opacity = opacity < 1.0;

        // TODO: account for overflow_x vs overflow_y
        let overflow_x = styles.get_box().overflow_x;
        let overflow_y = styles.get_box().overflow_y;
        if let Some(observer) = &self.surface_observer {
            if overflow_x != overflow_y
                || opacity != 1.0
                || !effects.filter.0.is_empty()
                || !effects.backdrop_filter.0.is_empty()
            {
                observer.borrow_mut().unsupported();
            }
            let box_style = styles.get_box();
            let requested = !box_style.transform.0.is_empty()
                || !matches!(box_style.rotate, style::values::computed::Rotate::None)
                || !matches!(
                    box_style.scale,
                    style::values::generics::transform::Scale::None
                )
                || !matches!(
                    box_style.translate,
                    style::values::generics::transform::Translate::None
                );
            if requested && node.transform().is_none() {
                observer.borrow_mut().unsupported();
            }
        }
        // `contain: paint` (and stronger values like `strict`/`content`) clips the element's
        // contents to its padding box. Paint containment does not apply to non-atomic inlines
        // or internal table boxes other than table-cell.
        let contain_paint = styles.get_box().clone_contain().contains(Contain::PAINT) && {
            let display = styles.clone_display();
            let is_internal_table_box_other_than_cell = display.outside()
                == DisplayOutside::InternalTable
                && display.inside() != DisplayInside::TableCell;
            !display.is_inline_flow() && !is_internal_table_box_other_than_cell
        };
        let is_image = node
            .element_data()
            .and_then(|e| e.raster_image_data())
            .is_some();
        let is_sub_doc = node
            .element_data()
            .and_then(|el| el.sub_doc_data())
            .is_some();
        let is_text_input = node
            .element_data()
            .and_then(|el| el.text_input_data())
            .is_some();
        // The root element's overflow is propagated to the viewport (which is clipped by the
        // window/surface bounds), so the root element must not clip its own overflow.
        let is_root_element = self.root_element_id == Some(node_id);
        let should_clip = !is_root_element
            && (is_image
                || is_sub_doc
                || is_text_input
                || contain_paint
                || !matches!(overflow_x, Overflow::Visible)
                || !matches!(overflow_y, Overflow::Visible));

        // Apply padding/border offset to inline root
        let taffy::Layout {
            size,
            border,
            padding,
            location,
            ..
        } = *node.final_layout();
        let box_position = Vec2::new(location.x as f64, location.y as f64) * self.scale;
        let box_size = Size::new(size.width as f64, size.height as f64);
        let border_box = Rect::from_origin_size(box_position.to_point(), box_size);
        let scaled_pb = (padding + border).map(f64::from);
        let content_position = kurbo::Point {
            x: scaled_pb.left,
            y: scaled_pb.top,
        };
        let content_box_size = kurbo::Size {
            width: (size.width as f64 - scaled_pb.left - scaled_pb.right) * self.scale,
            height: (size.height as f64 - scaled_pb.top - scaled_pb.bottom) * self.scale,
        };

        // Don't render things that are out of view
        let overflow = *node.scrollable_overflow();
        let transform = parent_style_transform
            * Affine::translate(box_position)
            * node.transform().as_deref().copied().unwrap_or_default();

        let screen_transform = Affine::translate(Vec2 {
            x: -self.initial_x,
            y: -self.initial_y,
        }) * transform;
        let screen_bbox = screen_transform.transform_rect_bbox(overflow.union(border_box));

        // Cull elements that fall entirely outside the current clip rectangle. In addition to
        // the viewport, `clip_rect` is narrowed by any ancestor scrollport (see below), so this
        // also culls elements scrolled out of view inside a clipping/scrolling container.
        if screen_bbox.x1 < clip_rect.x0
            || screen_bbox.x0 > clip_rect.x1
            || screen_bbox.y1 < clip_rect.y0
            || screen_bbox.y0 > clip_rect.y1
        {
            return;
        }

        // Cull elements whose transformed geometry is not representable in f32 (e.g.
        // `transform: scale(99e99)`): the renderer works in f32 and flattening such paths
        // can attempt to allocate an unbounded number of line segments.
        let f32_max = f64::from(f32::MAX);
        if !screen_bbox.is_finite()
            || [
                screen_bbox.x0,
                screen_bbox.y0,
                screen_bbox.x1,
                screen_bbox.y1,
            ]
            .iter()
            .any(|c| c.abs() > f32_max)
        {
            return;
        }

        // Optimise zero-area (/very small area) clips by not rendering at all
        let clip_area = content_box_size.width * content_box_size.height;
        let overflow_area =
            node.scrollable_overflow().width() * node.scrollable_overflow().height();
        if should_clip && clip_area < 0.01 && overflow_area < 0.01 {
            return;
        }

        #[cfg(feature = "custom-widget")]
        let custom_widget_scene = self.custom_widget_scenes.get(&(self.dom.id(), node_id));
        #[cfg(not(feature = "custom-widget"))]
        let custom_widget_scene = None;

        // Apply CSS transform property (where transforms are 2d)

        let mut cx = self.element_cx(node, *node.final_layout(), transform, custom_widget_scene);
        if let Some(observer) = &self.surface_observer {
            if is_sub_doc || custom_widget_scene.is_some() {
                observer.borrow_mut().unsupported();
            }
        }

        // If this element clips its overflow it establishes a scrollport: narrow the clip
        // rectangle passed to descendants to the visible (clipped) region so that content
        // scrolled out of view is culled rather than drawn and clipped away. The box used
        // here matches the clip applied to the content below.
        let child_clip_rect = if should_clip {
            let clip_box = if is_text_input {
                cx.frame.content_box_path()
            } else {
                cx.frame.padding_box_path()
            };
            clip_rect.intersect(screen_transform.transform_rect_bbox(clip_box.bounding_box()))
        } else {
            clip_rect
        };

        // Compute clip-path (if any) and wrap all rendering in a clip layer
        let clip_path_shape = cx.clip_path_shape();
        if let Some(observer) = &self.surface_observer {
            let requested = !matches!(
                styles.clone_clip_path(),
                style::values::computed::basic_shape::ClipPath::None
            );
            if (requested && clip_path_shape.is_none()) || cx.has_css_mask() {
                observer.borrow_mut().unsupported();
            }
        }
        let has_clip_path = clip_path_shape.is_some();
        let default_clip = cx.frame.border_box_path();
        let mut clip_path_for_layer = clip_path_shape.unwrap_or(default_clip);
        clip_path_for_layer.apply_affine(Affine::scale(self.scale));

        // CSS 2 `clip` property (absolutely positioned elements only). Applies to the whole of
        // the element's rendering (including outlines and shadows), so it is the outermost layer.
        let css_clip_rect = cx.css_clip_rect();
        if css_clip_rect.is_some_and(|rect| rect.is_zero_area()) {
            return;
        }
        let css_clip_layer_pushed = self.layer_manager.maybe_push_layer(
            scene,
            css_clip_rect.is_some(),
            1.0,
            cx.transform,
            &css_clip_rect.unwrap_or(Rect::ZERO),
            None,
            None,
        );

        cx.draw_outline(scene);
        cx.draw_outset_box_shadow(scene);

        // clip-path clip ayer
        self.layer_manager.maybe_with_layer(
            scene,
            has_clip_path,
            1.0,
            cx.transform,
            &clip_path_for_layer,
            None,
            None,
            |scene| {
                // If the element has a CSS `mask`, then push an isolation layer for the
                // masked content. The mask is applied when the layer is popped below.
                let mask_layer_pushed = cx.maybe_push_css_mask_layer(scene);
                // `cx.transform` is mutated to apply scroll offsets while drawing content.
                // Save it so that the mask can be drawn untransformed by scroll offsets.
                let unscrolled_transform = cx.transform;

                let filter = convert_filters(&effects.filter.0).map(Arc::new);
                let backdrop_filter = convert_filters(&effects.backdrop_filter.0).map(Arc::new);

                // Adjust effect layer clip by filter expansion area
                //
                // Returns a rectangle centered at the origin representing how much the filter
                // expands the processing region in each direction. The rect coordinates are:
                // - x0: negative left expansion
                // - y0: negative top expansion
                // - x1: positive right expansion
                // - y1: positive bottom expansion
                let filter_expansion_area = filter
                    .as_ref()
                    .map(|f| f.expansion_rect())
                    .unwrap_or(Rect::ZERO);

                let mut effect_layer_clip = cx.frame.border_box_path().bounding_box();
                effect_layer_clip.x0 += filter_expansion_area.x0;
                effect_layer_clip.y0 += filter_expansion_area.y0;
                effect_layer_clip.x1 += filter_expansion_area.x1;
                effect_layer_clip.y1 += filter_expansion_area.y1;

                // Opacity/Filter layer if box has opacity or a filter.
                // Clipped to border-box as it needs to include the background and borders.
                self.layer_manager.maybe_with_layer(
                    scene,
                    has_opacity || filter.is_some() || backdrop_filter.is_some(),
                    opacity,
                    cx.transform,
                    &effect_layer_clip,
                    filter,
                    backdrop_filter,
                    |scene| {
                        cx.draw_background(scene);
                        cx.draw_inset_box_shadow(scene);
                        cx.draw_table_row_backgrounds(scene);
                        cx.draw_table_borders(scene);
                        cx.draw_border(scene);
                        cx.stroke_devtools(scene);

                        // TODO: allow layers with opacity to be unclipped (overflow: visible)
                        let clip = if is_text_input {
                            &cx.frame.content_box_path()
                        } else {
                            &cx.frame.padding_box_path()
                        };

                        // Clip layer if box requires clipping. Opacity set to 1.0
                        self.layer_manager.maybe_with_layer(
                            scene,
                            should_clip,
                            1.0, // opacity
                            cx.transform,
                            clip,
                            None,
                            None,
                            |scene| {
                                if let Some(observer) = &self.surface_observer {
                                    if node.local_name() == "video" {
                                        observer.borrow_mut().target(
                                            node.id.as_u64(),
                                            cx.transform,
                                            cx.frame.content_box,
                                            node.is_focussable(),
                                        );
                                    }
                                }
                                // Now that background has been drawn, offset pos and cx in order to draw our contents scrolled
                                let content_position = Point {
                                    x: content_position.x - node.scroll_offset().x,
                                    y: content_position.y - node.scroll_offset().y,
                                };

                                cx.transform = cx.transform.then_translate(Vec2 {
                                    x: -node.scroll_offset().x * self.scale,
                                    y: -node.scroll_offset().y * self.scale,
                                });
                                cx.draw_image(scene);
                                #[cfg(feature = "svg")]
                                cx.draw_svg(scene);
                                #[cfg(feature = "custom-widget")]
                                cx.draw_custom_widget(scene);
                                cx.draw_sub_document(scene);
                                cx.draw_input(scene);
                                cx.draw_text_input_text(scene, content_position);
                                cx.draw_inline_layout(scene, content_position);
                                cx.draw_marker(scene, content_position);
                                cx.draw_children(scene, cx.transform, child_clip_rect);
                            },
                        );

                        // Overlay scrollbars, drawn unscrolled above the
                        // clipped content.
                        #[cfg(feature = "scrollbars")]
                        {
                            cx.transform = unscrolled_transform;
                            cx.draw_scrollbars(scene);
                        }
                    },
                );

                // Apply the CSS `mask` (if any) to the content drawn above
                cx.transform = unscrolled_transform;
                cx.maybe_pop_css_mask_layer(scene, mask_layer_pushed);
            },
        );

        self.layer_manager
            .maybe_pop_layer(scene, css_clip_layer_pushed);
    }

    pub(super) fn render_node(
        &self,
        scene: &mut impl PaintScene,
        node_id: NodeId,
        parent_style_transform: Affine,
        clip_rect: Rect,
    ) {
        let node = &self.dom.as_ref().tree()[node_id];

        match &node.data {
            NodeData::Element(_) | NodeData::AnonymousBlock(_) => {
                self.render_element(scene, node_id, parent_style_transform, clip_rect)
            }
            NodeData::Text(TextNodeData { .. }) => {
                // Text nodes should never be rendered directly
                // (they should always be rendered as part of an inline layout)
                // unreachable!()
            }
            NodeData::Document(_) => {}
            // NodeData::Doctype => {}
            NodeData::Comment { .. } => {} // NodeData::ProcessingInstruction { .. } => {}
        }
    }
}
