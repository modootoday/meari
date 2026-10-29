//! Record checked vector paint without silently discarding custom resources.
use crate::surface_observation::{Collector, PrimitiveKind, Shared, SurfacePaintTrace};
use anyrender::{
    Filter, Glyph, NormalizedCoord, Paint, PaintRef, PaintScene, RenderContext, Scene,
};
use kurbo::{Affine, Rect, Shape, Stroke, Vec2};
use peniko::{BlendMode, Color, Fill, FontData, StyleRef};
use std::sync::Arc;

pub fn prepare_scene_observed(
    document: &mut blitz_dom::BaseDocument,
    scale: f64,
    bounds: (u32, u32),
    offset: (u32, u32),
    targets: &[(u64, u64)],
) -> Result<(Scene, Option<SurfacePaintTrace>), &'static str> {
    if document.has_pending_critical_resources() {
        return Err("surface observation is waiting for critical resources");
    }
    let observer = std::rc::Rc::new(std::cell::RefCell::new(Collector::new(
        document.id(),
        document.get_focussed_node_id().map(|node| node.as_u64()),
        targets,
        Rect::new(
            f64::from(offset.0),
            f64::from(offset.1),
            f64::from(offset.0) + f64::from(bounds.0),
            f64::from(offset.1) + f64::from(bounds.1),
        ),
    )));
    let mut recording = CheckedRecording {
        observer: Some(observer.clone()),
        ..CheckedRecording::default()
    };
    crate::paint_scene_observed_into(
        &mut recording,
        document,
        (scale, bounds.0, bounds.1, offset.0, offset.1),
        Some(observer.clone()),
    )?;
    let scene = recording.finish()?;
    let trace = observer.borrow().finish();
    Ok((scene, trace))
}

pub fn prepare_scene_checked(
    document: &mut blitz_dom::BaseDocument,
    scale: f64,
    width: u32,
    height: u32,
    x_offset: u32,
    y_offset: u32,
) -> Result<Scene, &'static str> {
    let mut recording = CheckedRecording::default();
    crate::paint_scene_checked(
        &mut recording,
        document,
        scale,
        width,
        height,
        x_offset,
        y_offset,
    )?;
    recording.finish()
}

#[derive(Default)]
pub(crate) struct CheckedRecording {
    observer: Option<Shared>,
    scene: Scene,
    failure: Option<&'static str>,
}
impl CheckedRecording {
    fn accepts(&mut self, brush: &PaintRef<'_>) -> bool {
        if matches!(brush, Paint::Custom(_) | Paint::Resource(_)) {
            self.failure =
                Some("recorded custom/resource paint requires a scoped resource adapter");
            false
        } else {
            true
        }
    }
    pub fn finish(self) -> Result<Scene, &'static str> {
        match self.failure {
            Some(error) => Err(error),
            None => Ok(self.scene),
        }
    }
}
impl RenderContext for CheckedRecording {}
impl PaintScene for CheckedRecording {
    fn reset(&mut self) {
        if let Some(observer) = &self.observer {
            observer.borrow_mut().abort();
        }
        self.scene.reset();
        self.failure = None;
    }
    fn push_layer(
        &mut self,
        fill: Fill,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
        filter: Option<Arc<Filter>>,
        backdrop_filter: Option<Arc<Filter>>,
    ) {
        if let Some(observer) = &self.observer {
            observer.borrow_mut().push(transform, clip, true);
        }
        self.scene
            .push_layer(fill, blend, alpha, transform, clip, filter, backdrop_filter);
    }
    fn push_clip_layer(&mut self, fill: Fill, transform: Affine, clip: &impl Shape) {
        if let Some(observer) = &self.observer {
            observer.borrow_mut().push(transform, clip, false);
        }
        self.scene.push_clip_layer(fill, transform, clip);
    }
    fn pop_layer(&mut self) {
        if let Some(observer) = &self.observer {
            observer.borrow_mut().pop();
        }
        self.scene.pop_layer();
    }
    fn stroke<'a>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<PaintRef<'a>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        let brush = brush.into();
        if self.accepts(&brush) {
            if let Some(observer) = &self.observer {
                observer.borrow_mut().draw_kind(None, PrimitiveKind::Stroke);
            }
            self.scene
                .stroke(style, transform, brush, brush_transform, shape);
        }
    }
    fn fill<'a>(
        &mut self,
        style: Fill,
        transform: Affine,
        brush: impl Into<PaintRef<'a>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        let brush = brush.into();
        if self.accepts(&brush) {
            if let Some(observer) = &self.observer {
                observer
                    .borrow_mut()
                    .draw(Some(transform.transform_rect_bbox(shape.bounding_box())));
            }
            self.scene
                .fill(style, transform, brush, brush_transform, shape);
        }
    }
    fn draw_glyphs<'a, 's: 'a>(
        &'s mut self,
        font: &'a FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'a [NormalizedCoord],
        embolden: Vec2,
        style: impl Into<StyleRef<'a>>,
        brush: impl Into<PaintRef<'a>>,
        brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: impl Iterator<Item = Glyph> + Clone,
    ) {
        let brush = brush.into();
        if self.accepts(&brush) {
            if let Some(observer) = &self.observer {
                observer.borrow_mut().draw_kind(None, PrimitiveKind::Glyph);
            }
            self.scene.draw_glyphs(
                font,
                font_size,
                hint,
                normalized_coords,
                embolden,
                style,
                brush,
                brush_alpha,
                transform,
                glyph_transform,
                glyphs,
            );
        }
    }
    fn draw_box_shadow(
        &mut self,
        transform: Affine,
        rect: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
    ) {
        if let Some(observer) = &self.observer {
            observer.borrow_mut().draw_kind(None, PrimitiveKind::Shadow);
        }
        self.scene
            .draw_box_shadow(transform, rect, brush, radius, std_dev);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borrowed_custom_paint_produces_an_error_instead_of_a_transparent_success() {
        let custom = "custom backend brush".to_owned();
        let mut recorder = CheckedRecording::default();
        let brush: PaintRef<'_> = Paint::Custom(&custom as &(dyn std::any::Any + Send + Sync));
        recorder.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            brush,
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        assert!(recorder.finish().is_err());
    }
    #[test]
    fn ordinary_commands_record_exactly_and_reset_discards_an_aborted_recording() {
        let mut recorder = CheckedRecording::default();
        let custom = 7u32;
        let brush: PaintRef<'_> = Paint::Custom(&custom as &(dyn std::any::Any + Send + Sync));
        recorder.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            brush,
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        recorder.reset();
        recorder.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::BLACK,
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        assert_eq!(recorder.finish().unwrap().commands.len(), 1);
    }
}
