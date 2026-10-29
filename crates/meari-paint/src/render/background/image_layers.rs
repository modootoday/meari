//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
// Meari modification (2026-10-19): vector image tiling, clipping and checked budgets.
use super::super::{ElementCx, to_image_quality, to_peniko_image};
use crate::gradient::to_peniko_gradient;
use anyrender::PaintScene;
use blitz_dom::node::ImageData;
use kurbo::{self, Point, Rect, Shape, Size, Vec2};
use peniko::{self};
use style::values::{
    computed::{BackgroundRepeat, Gradient as StyloGradient},
    specified::background::BackgroundRepeatKeyword,
};

impl ElementCx<'_, '_> {
    #[cfg(feature = "svg")]
    pub(super) fn draw_svg_image_layer(
        &self,
        scene: &mut impl PaintScene,
        layer: &ImageLayerStyles,
    ) {
        let Some(bg_image) = layer.image_data else {
            return;
        };
        let ImageData::Svg(svg) = &bg_image.image else {
            return;
        };

        // A zero-sized `viewBox` disables rendering of the SVG
        if svg.intrinsic_dimensions.degenerate_view_box {
            return;
        }

        let (origin_rect, base_transform, clip_rect) = if self.layer_is_fixed(layer) {
            let (rect, transform) = self.fixed_positioning_area();
            (rect, transform, rect)
        } else {
            (
                self.box_rect(layer.origin),
                self.transform,
                self.box_rect(layer.clip),
            )
        };

        let frame_w = (origin_rect.width() / self.scale) as f32;
        let frame_h = (origin_rect.height() / self.scale) as f32;

        // Size the SVG per the CSS default sizing algorithm
        // (https://drafts.csswg.org/css-images/#default-sizing). An SVG image
        // may lack an intrinsic width, height, and/or aspect ratio, so each is
        // passed separately (usvg's resolved `Tree::size` is only used as the
        // source coordinate space of the rendered tree).
        let intrinsic_width = svg.intrinsic_width().filter(|w| w.is_finite() && *w > 0.0);
        let intrinsic_height = svg.intrinsic_height().filter(|h| h.is_finite() && *h > 0.0);
        let aspect_ratio = match (intrinsic_width, intrinsic_height) {
            (Some(w), Some(h)) => Some(w / h),
            _ => svg.viewbox_aspect_ratio(),
        }
        .filter(|r| r.is_finite() && *r > 0.0);

        let (bg_pos, bg_size) = compute_layer_position_and_size(
            layer,
            frame_w as f64,
            frame_h as f64,
            BackgroundSizeComputeMode::Intrinsic {
                width: intrinsic_width,
                height: intrinsic_height,
                ratio: aspect_ratio,
            },
        );

        if bg_size.width <= 0.0 || bg_size.height <= 0.0 {
            return;
        }
        if !bg_size.width.is_finite()
            || !bg_size.height.is_finite()
            || !bg_pos.x.is_finite()
            || !bg_pos.y.is_finite()
        {
            *self.context.failure.borrow_mut() = Some("SVG image layer geometry is non-finite");
            return;
        }

        let viewport = super::super::svg_viewport_affine(
            svg,
            (bg_size.width * self.scale) as f32,
            (bg_size.height * self.scale) as f32,
        );

        let BackgroundRepeat(repeat_x, repeat_y) = layer.repeat;
        let x = gradient_axis_tiling(
            *repeat_x,
            origin_rect.x0,
            origin_rect.width(),
            clip_rect.x0,
            clip_rect.width(),
            bg_pos.x * self.scale,
            bg_size.width * self.scale,
        );
        let y = gradient_axis_tiling(
            *repeat_y,
            origin_rect.y0,
            origin_rect.height(),
            clip_rect.y0,
            clip_rect.height(),
            bg_pos.y * self.scale,
            bg_size.height * self.scale,
        );
        let tiles = u64::from(x.count) * u64::from(y.count);
        let used = self.context.vector_tiles.get().saturating_add(tiles);
        if used > 65536 {
            *self.context.failure.borrow_mut() =
                Some("SVG image layers exceed the vector tile budget");
            return;
        }
        self.context.vector_tiles.set(used);
        for row in 0..y.count {
            for column in 0..x.count {
                let left = x.translate + f64::from(column) * x.stride;
                let top = y.translate + f64::from(row) * y.stride;
                let transform = base_transform * kurbo::Affine::translate((left, top)) * viewport;
                let tile_clip = Rect::new(
                    left,
                    top,
                    left + bg_size.width * self.scale,
                    top + bg_size.height * self.scale,
                )
                .to_path(0.1);
                self.context.layer_manager.maybe_with_layer(
                    scene,
                    true,
                    1.0,
                    base_transform,
                    &tile_clip,
                    None,
                    None,
                    |scene| {
                        anyrender_svg::render_svg_tree(scene, &svg.tree, transform);
                    },
                );
            }
        }
    }

    pub(super) fn draw_raster_image_layer(
        &self,
        scene: &mut impl PaintScene,
        layer: &ImageLayerStyles,
    ) {
        let Some(bg_image) = layer.image_data else {
            return;
        };
        let ImageData::Raster(image_data) = &bg_image.image else {
            return;
        };

        let image_rendering = self.style.clone_image_rendering();
        let quality = to_image_quality(image_rendering);

        let (origin_rect, base_transform) = if self.layer_is_fixed(layer) {
            self.fixed_positioning_area()
        } else {
            (self.box_rect(layer.origin), self.transform)
        };

        let image_width = image_data.width as f64;
        let image_height = image_data.height as f64;

        let (bg_pos, bg_size) = compute_layer_position_and_size(
            layer,
            origin_rect.width() / self.scale,
            origin_rect.height() / self.scale,
            BackgroundSizeComputeMode::Size(image_width as f32, image_height as f32),
        );

        let bg_pos = (bg_pos.to_vec2() * self.scale).to_point();
        let bg_size = bg_size * self.scale;

        let x_ratio = bg_size.width / image_width;
        let y_ratio = bg_size.height / image_height;

        let BackgroundRepeat(repeat_x, repeat_y) = layer.repeat;

        let x = raster_axis_tiling(
            *repeat_x,
            origin_rect.x0,
            origin_rect.width(),
            bg_pos.x,
            bg_size.width,
            image_width,
            x_ratio,
        );
        let y = raster_axis_tiling(
            *repeat_y,
            origin_rect.y0,
            origin_rect.height(),
            bg_pos.y,
            bg_size.height,
            image_height,
            y_ratio,
        );

        let transform = base_transform
            .pre_scale_non_uniform(x_ratio, y_ratio)
            .then_translate(Vec2 {
                x: x.translate,
                y: y.translate,
            });
        let tile_rect = Rect::new(0.0, 0.0, x.rect_len, y.rect_len);

        for hc in 0..y.count {
            for wc in 0..x.count {
                let transform = transform.then_translate(Vec2 {
                    x: wc as f64 * x.stride,
                    y: hc as f64 * y.stride,
                });

                scene.fill(
                    peniko::Fill::NonZero,
                    transform,
                    to_peniko_image(image_data, quality).as_ref(),
                    None,
                    &tile_rect,
                );
            }
        }
    }

    pub(super) fn draw_gradient_layer(
        &self,
        scene: &mut impl PaintScene,
        gradient: &StyloGradient,
        layer: &ImageLayerStyles,
    ) {
        // For a fixed layer the positioning area (the viewport) already covers
        // everything visible, so it also serves as the clip rect (no extension
        // towards the clip box is needed).
        let (origin_rect, base_transform, clip_rect) = if self.layer_is_fixed(layer) {
            let (viewport_rect, transform) = self.fixed_positioning_area();
            (viewport_rect, transform, viewport_rect)
        } else {
            (
                self.box_rect(layer.origin),
                self.transform,
                self.box_rect(layer.clip),
            )
        };

        let (bg_pos, bg_size) = compute_layer_position_and_size(
            layer,
            origin_rect.width() / self.scale,
            origin_rect.height() / self.scale,
            BackgroundSizeComputeMode::Auto,
        );

        let bg_pos = (bg_pos.to_vec2() * self.scale).to_point();
        let bg_size = bg_size * self.scale;

        let BackgroundRepeat(repeat_x, repeat_y) = layer.repeat;

        let x = gradient_axis_tiling(
            *repeat_x,
            origin_rect.x0,
            origin_rect.width(),
            clip_rect.x0,
            clip_rect.width(),
            bg_pos.x,
            bg_size.width,
        );
        let y = gradient_axis_tiling(
            *repeat_y,
            origin_rect.y0,
            origin_rect.height(),
            clip_rect.y0,
            clip_rect.height(),
            bg_pos.y,
            bg_size.height,
        );

        // FIXME: https://wpt.live/css/css-backgrounds/background-size/background-size-near-zero-gradient.html
        if x.count as u64 * y.count as u64 > 500 {
            return;
        }

        let tile_rect = Rect::new(0.0, 0.0, x.rect_len, y.rect_len);
        let bounding_box = self.frame.border_box.bounding_box();
        let current_color = self.style.clone_color();

        let (gradient, gradient_transform) = to_peniko_gradient(
            gradient,
            tile_rect,
            bounding_box,
            self.scale,
            &current_color,
        );
        let brush = anyrender::Paint::Gradient(&gradient);

        let transform = base_transform.then_translate(Vec2 {
            x: x.translate,
            y: y.translate,
        });

        for hc in 0..y.count {
            for wc in 0..x.count {
                let transform = transform.then_translate(Vec2 {
                    x: wc as f64 * x.stride,
                    y: hc as f64 * y.stride,
                });

                scene.fill(
                    peniko::Fill::NonZero,
                    transform,
                    brush.clone(),
                    gradient_transform,
                    &tile_rect,
                );
            }
        }
    }
}

pub(super) fn compute_layer_position_and_size(
    layer: &ImageLayerStyles,
    container_w: f64,
    container_h: f64,
    size_mode: BackgroundSizeComputeMode,
) -> (Point, Size) {
    use BackgroundRepeatKeyword::*;

    let bg_size = compute_layer_size(layer, container_w as f32, container_h as f32, size_mode);

    let BackgroundRepeat(repeat_x, repeat_y) = layer.repeat;

    let bg_size = if matches!(repeat_x, Round) && matches!(repeat_y, Round) {
        let count = (container_w / bg_size.width).round().max(1.0);
        let width = container_w / count;

        let count = (container_h / bg_size.height).round().max(1.0);
        let height = container_h / count;

        Size::new(width, height)
    } else if matches!(repeat_x, Round) {
        let count = (container_w / bg_size.width).round().max(1.0);
        let width = container_w / count;
        Size::new(width, bg_size.height)
    } else if matches!(repeat_y, Round) {
        let count = (container_h / bg_size.height).round().max(1.0);
        let height = container_h / count;
        Size::new(bg_size.width, height)
    } else {
        bg_size
    };

    let bg_pos = compute_layer_position(
        layer,
        (container_w - bg_size.width) as f32,
        (container_h - bg_size.height) as f32,
    );
    (bg_pos, bg_size)
}

#[inline]
pub(super) fn compute_layer_position(layer: &ImageLayerStyles, width: f32, height: f32) -> Point {
    use style::values::computed::Length;

    let bg_pos_x = layer.position_x.resolve(Length::new(width)).px() as f64;
    let bg_pos_y = layer.position_y.resolve(Length::new(height)).px() as f64;

    Point::new(bg_pos_x, bg_pos_y)
}
