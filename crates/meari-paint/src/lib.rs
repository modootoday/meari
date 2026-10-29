//! Paint a [`blitz_dom::BaseDocument`] by pushing [`anyrender`] drawing commands into
//! an impl [`anyrender::PaintScene`].
//! Meari modification: checked SVG tile-budget failures (2026-10-19).

#![allow(clippy::collapsible_if)]

mod checked_recording;
mod color;
mod surface_observation;
pub use checked_recording::prepare_scene_checked;
pub use checked_recording::prepare_scene_observed;
pub use surface_observation::{Overlap, SurfacePaintNode, SurfacePaintTrace};
mod debug_overlay;
mod filters;
mod gradient;
mod kurbo_css;
mod layers;
mod render;
mod sizing;
mod text;

use std::collections::HashMap;

use anyrender::{PaintScene, Scene};
use blitz_dom::{BaseDocument, NodeId, util::Color};
use render::BlitzDomPainter;

const FONT_EMBOLDEN_ENABLED: bool = cfg!(any(
    feature = "font-embolden",
    all(feature = "apple-font-embolden", target_os = "macos"),
    all(feature = "apple-font-embolden", target_os = "ios"),
));

/// The default color for text selection highlights
const SELECTION_COLOR: Color = Color::from_rgb8(180, 213, 255);

/// Pre-computed `Scene`s for each CustomWidget, keyed by `(document id, node id)`
type CustomWidgetSceneMap = HashMap<(usize, NodeId), Scene>;

/// Paint a [`blitz_dom::BaseDocument`] by pushing drawing commands into
/// an impl [`anyrender::PaintScene`].
///
/// This function assumes that the styles and layout in the [`BaseDocument`] are already
/// resolved. Please ensure that this is the case before trying to paint.
///
/// The implementation of [`PaintScene`] is responsible for handling the commands that are pushed into it.
/// Generally this will involve executing them to draw a rasterized image/texture. But in some cases it may choose to
/// transform them to a vector format (e.g. SVG/PDF) or serialize them in raw form for later use.
pub fn paint_scene(
    scene: &mut impl PaintScene,
    doc: &mut BaseDocument,
    scale: f64,
    width: u32,
    height: u32,
    x_offset: u32,
    y_offset: u32,
) {
    let _ = paint_scene_checked(scene, doc, scale, width, height, x_offset, y_offset);
}

/// Meari checked entry point: reject an incomplete vector-tiled frame.
pub fn paint_scene_checked(
    scene: &mut impl PaintScene,
    doc: &mut BaseDocument,
    scale: f64,
    width: u32,
    height: u32,
    x_offset: u32,
    y_offset: u32,
) -> Result<(), &'static str> {
    paint_scene_observed_into(scene, doc, (scale, width, height, x_offset, y_offset), None)
}
fn paint_scene_observed_into(
    scene: &mut impl PaintScene,
    doc: &mut BaseDocument,
    settings: (f64, u32, u32, u32, u32),
    observer: Option<surface_observation::Shared>,
) -> Result<(), &'static str> {
    let (scale, width, height, x_offset, y_offset) = settings;
    // Run `.paint()` on every custom widget in the document (and all subdocuments) ahead of time.
    // This helps us avoid borrow-checker issues as we recurse down the tree (`.paint()` require `&mut self`).
    //
    // TODO: Take widget and sub-document visibility into account
    #[allow(unused_mut)]
    let mut custom_widget_scenes: CustomWidgetSceneMap = HashMap::new();
    #[cfg(feature = "custom-widget")]
    build_custom_widget_scenes(&mut custom_widget_scenes, doc, scene, scale);

    let mut generator = BlitzDomPainter::new(
        doc,
        scale,
        width,
        height,
        x_offset as f64,
        y_offset as f64,
        &custom_widget_scenes,
    );
    if let Some(observer) = observer {
        generator.observe(observer);
    }
    generator.paint_scene(scene);
    let failure = generator.failure.borrow_mut().take();
    if let Some(failure) = failure {
        return Err(failure);
    }

    // println!(
    //     "Rendered using {} clips (depth: {}) (wanted: {})",
    //     CLIPS_USED.load(atomic::Ordering::SeqCst),
    //     CLIP_DEPTH_USED.load(atomic::Ordering::SeqCst),
    //     CLIPS_WANTED.load(atomic::Ordering::SeqCst)
    // );
    Ok(())
}

#[cfg(feature = "custom-widget")]
fn build_custom_widget_scenes(
    custom_widget_scenes: &mut CustomWidgetSceneMap,
    doc: &mut BaseDocument,
    render_ctx: &mut impl anyrender::RenderContext,
    scale: f64,
) {
    let doc_id = doc.id();

    // Process scenes for every custom widget in the document
    let custom_widget_node_ids = doc.custom_widget_node_ids();
    for node_id in custom_widget_node_ids.into_iter() {
        if let Some(scene) = process_custom_widget_node(doc, render_ctx, node_id, scale) {
            custom_widget_scenes.insert((doc_id, node_id), scene);
        }
    }

    // Recurse into sub documents
    let sub_document_node_ids = doc.sub_document_node_ids();
    for node_id in sub_document_node_ids.into_iter() {
        if let Some(sub_doc) = doc.get_node_mut(node_id).and_then(|node| node.subdoc_mut()) {
            let mut inner = sub_doc.inner_mut();
            build_custom_widget_scenes(custom_widget_scenes, &mut inner, render_ctx, scale);
        }
    }
}

#[cfg(feature = "custom-widget")]
fn process_custom_widget_node(
    doc: &mut BaseDocument,
    render_ctx: &mut impl anyrender::RenderContext,
    node_id: NodeId,
    scale: f64,
) -> Option<Scene> {
    use blitz_dom::node::{CustomWidgetStatus, ProxyRenderContext};

    let node = doc.get_node_mut(node_id)?;
    let width = (node.final_layout().size.width as f64 * scale) as u32;
    let height = (node.final_layout().size.height as f64 * scale) as u32;

    if width == 0 || height == 0 {
        return None;
    }

    let style = (*node.stylo_element_data().primary_styles()?).clone();
    let element = node.data.downcast_element_mut()?;
    let widget_data = element.custom_widget_data_mut()?;

    let mut render_ctx = ProxyRenderContext {
        inner: render_ctx,
        resource_ids: &mut widget_data.active_resource_ids,
    };

    if widget_data.status == CustomWidgetStatus::Suspended {
        widget_data.widget.can_create_surfaces(&mut render_ctx);
        widget_data.status = CustomWidgetStatus::Active;
    }

    let widget_scene = widget_data
        .widget
        .paint(&mut render_ctx, &style, width, height, scale);

    Some(widget_scene)
}
