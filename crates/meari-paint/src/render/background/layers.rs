//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
// Meari modification (2026-10-19): vector image tiling, clipping and checked budgets.
use super::super::ElementCx;
use crate::color::{Color, ToColorColor};
use anyrender::PaintScene;
use blitz_dom::NodeId;
use blitz_dom::node::SpecialElementData;
use kurbo::{self, Affine, BezPath, Rect, Size};
use peniko::{self, Fill};
use style::{
    computed_values::border_collapse::T as BorderCollapse,
    properties::generated::longhands::{
        background_attachment::single_value::computed_value::T as StyloBackgroundAttachment,
        background_clip::single_value::computed_value::T as StyloBackgroundClip,
        background_origin::single_value::computed_value::T as StyloBackgroundOrigin,
        mask_origin::single_value::computed_value::T as StyloMaskOrigin,
    },
    values::generics::image::GenericImage,
};
#[cfg(feature = "tracing")]
use tracing::warn;

// Also covers MaskClip as the type is the same
impl From<StyloBackgroundClip> for BoxModelBox {
    fn from(value: StyloBackgroundClip) -> Self {
        match value {
            StyloBackgroundClip::BorderBox => Self::BorderBox,
            StyloBackgroundClip::PaddingBox => Self::PaddingBox,
            StyloBackgroundClip::ContentBox => Self::ContentBox,

            // TODO: support BorderArea
            StyloBackgroundClip::BorderArea => Self::BorderBox,
        }
    }
}

impl From<StyloBackgroundOrigin> for BoxModelBox {
    fn from(value: StyloBackgroundOrigin) -> Self {
        match value {
            StyloBackgroundOrigin::BorderBox => Self::BorderBox,
            StyloBackgroundOrigin::PaddingBox => Self::PaddingBox,
            StyloBackgroundOrigin::ContentBox => Self::ContentBox,
        }
    }
}

impl From<StyloMaskOrigin> for BoxModelBox {
    fn from(value: StyloMaskOrigin) -> Self {
        match value {
            StyloMaskOrigin::BorderBox => Self::BorderBox,
            StyloMaskOrigin::PaddingBox => Self::PaddingBox,
            StyloMaskOrigin::ContentBox => Self::ContentBox,
        }
    }
}

impl ElementCx<'_, '_> {
    pub(in super::super) fn draw_background(&self, scene: &mut impl PaintScene) {
        let bg_styles = &self.style.get_background();
        let image_data = &self.element.background_images;
        let layer_count = bg_styles.background_image.0.len();

        // The background color is clipped by the clip of the last layer in the list
        let background_clip: BoxModelBox =
            (*get_cyclic(&bg_styles.background_clip.0, layer_count - 1)).into();
        let background_clip_path = self.box_path(background_clip);

        // Draw background color (if any)
        self.draw_solid_bg(scene, &background_clip_path);

        for idx in (0..layer_count).rev() {
            let layer = ImageLayerStyles::from_background(bg_styles, image_data, idx);
            if layer_paints_nothing(&layer) {
                continue;
            }
            let background_clip_path = self.box_path(layer.clip);

            self.context.layer_manager.maybe_with_layer(
                scene,
                true,
                1.0,
                self.transform,
                &background_clip_path,
                None,
                None,
                |scene| {
                    self.draw_image_layer(scene, &layer);
                },
            );
        }
    }

    /// The path of the given CSS box model box for this element
    pub(in super::super) fn box_path(&self, css_box: BoxModelBox) -> BezPath {
        match css_box {
            BoxModelBox::BorderBox => self.frame.border_box_path(),
            BoxModelBox::PaddingBox => self.frame.padding_box_path(),
            BoxModelBox::ContentBox => self.frame.content_box_path(),
        }
    }

    /// The rect of the given CSS box model box for this element
    pub(super) fn box_rect(&self, css_box: BoxModelBox) -> Rect {
        match css_box {
            BoxModelBox::BorderBox => self.frame.border_box,
            BoxModelBox::PaddingBox => self.frame.padding_box,
            BoxModelBox::ContentBox => self.frame.content_box,
        }
    }

    /// Draw a single layer of a CSS image layer list (`background-image` or `mask-image`)
    pub(in super::super) fn draw_image_layer(
        &self,
        scene: &mut impl PaintScene,
        layer: &ImageLayerStyles,
    ) {
        match layer.stylo_image {
            GenericImage::None => {
                // Do nothing
            }
            GenericImage::Gradient(gradient) => self.draw_gradient_layer(scene, gradient, layer),
            GenericImage::Url(_) => {
                self.draw_raster_image_layer(scene, layer);
                #[cfg(feature = "svg")]
                self.draw_svg_image_layer(scene, layer);
            }
            GenericImage::LightDark(_) => {
                #[cfg(feature = "tracing")]
                warn!("Implement image layer drawing for ImageLightDark")
            }
            GenericImage::PaintWorklet(_) => {
                #[cfg(feature = "tracing")]
                warn!("Implement image layer drawing for Image::PaintWorklet")
            }
            GenericImage::CrossFade(_) => {
                #[cfg(feature = "tracing")]
                warn!("Implement image layer drawing for Image::CrossFade")
            }
            GenericImage::Image(_) => {
                #[cfg(feature = "tracing")]
                warn!("Implement image layer drawing for Image::Image")
            }
            GenericImage::ImageSet(_) => {
                #[cfg(feature = "tracing")]
                warn!("Implement image layer drawing for Image::ImageSet")
            }
        }
    }

    /// Paint the backgrounds of the table's column groups, columns, row groups and rows
    /// (in that order, bottom to top: CSS 2.1 §17.5.1).
    ///
    /// These backgrounds are only painted behind the table's cells (not in the
    /// `border-spacing` between them, nor where a row has no cell), so they are
    /// painted cell by cell.
    pub(in super::super) fn draw_table_row_backgrounds(&self, scene: &mut impl PaintScene) {
        let SpecialElementData::TableRoot(table) = &self.element.special_data else {
            return;
        };

        let background_color = |node_id: NodeId| -> Option<Color> {
            let node = self.context.dom.get_node(node_id)?;
            let style = node.primary_styles()?;
            let current_color = style.clone_color();
            let color = style
                .get_background()
                .background_color
                .resolve_to_absolute(&current_color)
                .as_srgb_color();
            (color != Color::TRANSPARENT).then_some(color)
        };

        // Resolve the (group, track) background colors once per track rather than once per cell
        let column_colors: Vec<[Option<Color>; 2]> = table
            .columns
            .iter()
            .map(|col| {
                [
                    col.group.and_then(background_color),
                    background_color(col.node_id),
                ]
            })
            .collect();
        let row_colors: Vec<[Option<Color>; 2]> = table
            .rows
            .iter()
            .map(|row| {
                [
                    row.group.and_then(background_color),
                    background_color(row.node_id),
                ]
            })
            .collect();
        let has_color =
            |colors: &[[Option<Color>; 2]]| colors.iter().flatten().any(Option::is_some);
        if !has_color(&column_colors) && !has_color(&row_colors) {
            return;
        }

        let cell_rect = |cell_id: NodeId| -> Option<Rect> {
            let layout = self.context.dom.get_node(cell_id)?.final_layout();
            Some(Rect::new(
                layout.location.x as f64,
                layout.location.y as f64,
                (layout.location.x + layout.size.width) as f64,
                (layout.location.y + layout.size.height) as f64,
            ))
        };

        // In the collapsed borders model the borders are laid out as gutters between the
        // cells, but a cell's border box (and so its background) extends to the middle of
        // the borders around it. The gutter is split on a whole pixel so that the
        // backgrounds of adjacent cells tile exactly.
        let collapsed = table.border_collapse == BorderCollapse::Collapse;
        let (gutter, bounds) = if collapsed {
            let gap = table.style.gap.map(|gap| gap.into_raw().value() as f64);
            let bounds = table
                .cells
                .iter()
                .filter_map(|cell| cell_rect(cell.node_id))
                .reduce(|a, b| a.union(b));
            (Size::new(gap.width, gap.height), bounds)
        } else {
            (Size::ZERO, None)
        };
        let before = Size::new((gutter.width / 2.0).ceil(), (gutter.height / 2.0).ceil());
        let after = gutter - before;

        for cell in &table.cells {
            let no_colors = [None, None];
            let column = column_colors
                .get(cell.column as usize)
                .unwrap_or(&no_colors);
            let row = row_colors.get(cell.row as usize).unwrap_or(&no_colors);
            let mut colors = column.iter().chain(row).flatten().peekable();
            if colors.peek().is_none() {
                continue;
            }
            let Some(mut rect) = cell_rect(cell.node_id) else {
                continue;
            };
            if let Some(bounds) = bounds {
                rect = Rect::new(
                    rect.x0 - before.width,
                    rect.y0 - before.height,
                    rect.x1 + after.width,
                    rect.y1 + after.height,
                )
                .intersect(bounds);
            }

            let shape = rect.scale_from_origin(self.scale);
            for color in colors {
                scene.fill(Fill::NonZero, self.transform, *color, None, &shape);
            }
        }
    }

    pub(super) fn draw_solid_bg(&self, scene: &mut impl PaintScene, shape: &BezPath) {
        let current_color = self.style.clone_color();
        let background_color = &self.style.get_background().background_color;
        let bg_color = background_color
            .resolve_to_absolute(&current_color)
            .as_srgb_color();

        if bg_color != Color::TRANSPARENT {
            // Fill the color
            scene.fill(Fill::NonZero, self.transform, bg_color, None, shape);
        }
    }

    /// Whether the layer is positioned against the viewport
    /// (`background-attachment: fixed`) rather than the element's origin box.
    /// `fixed` behaves as `scroll` on elements affected by a CSS transform
    /// (the transformed element acts as the layer's containing block).
    pub(super) fn layer_is_fixed(&self, layer: &ImageLayerStyles) -> bool {
        layer.attachment == StyloBackgroundAttachment::Fixed && !self.is_transformed()
    }

    /// The background positioning area and the transform from its coordinate
    /// space to the scene for a fixed layer: the viewport, unaffected by any
    /// scrolling.
    pub(super) fn fixed_positioning_area(&self) -> (Rect, Affine) {
        let viewport_rect = Rect::new(
            0.0,
            0.0,
            self.context.width as f64,
            self.context.height as f64,
        );
        let transform = Affine::translate((self.context.initial_x, self.context.initial_y));
        (viewport_rect, transform)
    }

    /// Whether this element or any of its ancestors has a CSS transform
    ///
    /// TODO: this misses transformed elements whose resolved transform is the
    /// identity (e.g. `transform: translate(0)`), and elements with
    /// `will-change: transform`, both of which should also degrade `fixed`
    /// to `scroll` (see WPT css/css-transforms/transform-fixed-bg-005/008)
    pub(super) fn is_transformed(&self) -> bool {
        let mut current = Some(self.node.id);
        while let Some(node) = current.and_then(|id| self.context.dom.get_node(id)) {
            if node.transform().is_some() {
                return true;
            }
            current = node.parent;
        }
        false
    }
}
