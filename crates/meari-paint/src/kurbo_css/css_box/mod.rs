use kurbo::{Arc, Insets, Point, Rect, Shape as _};

use super::non_uniform_radii::NonUniformRoundedRectRadii;

/// There are several nested boxes at play here:
/// We have 4 boxes, 4 corners, and clockwise/anticlockwise for a total of 16 different options
///
/// ```a
///    *--------------------------------------------------------*  <--- CssBox::OutlineBox
///    |                         Outline                        |
///    |    *----------------------------------------------*    |  <--- CssBox::BorderBox
///    |    |                    Border                    |    |
///    |    |    *------------------------------------*    |    |  <--- CssBox::PaddingBox
///    |    |    |               Padding              |    |    |
///    |    |    |    *--------------------------*    |    |    |  <--- CssBox::ContentBox
///    |    |    |    |          Content         |    |    |    |
///    |    |    |    |                          |    |    |    |
///    |    |    |    |                          |    |    |    |
///    |    |    |    |                          |    |    |    |
///    |    |    |    *--------------------------*    |    |    |
///    |    |    |                                    |    |    |
///    |    |    *------------------------------------*    |    |
///    |    |                                              |    |
///    |    *----------------------------------------------*    |
///    |                                                        |
///    *--------------------------------------------------------*
/// ```
///
#[derive(Debug, Clone)]
pub struct CssBox {
    pub border_box: Rect,
    pub padding_box: Rect,
    pub content_box: Rect,
    pub outline_box: Rect,

    pub padding_width: Insets,
    pub border_width: Insets,
    pub outline_width: f64,

    pub border_radii: NonUniformRoundedRectRadii,
}

/// Makes it easier to insert objects into a bezpath without having to do checks
/// Mostly because I consider the vello api slightly defficient
trait BuildBezpath {
    const TOLERANCE: f64;
    fn insert_arc(&mut self, arc: Arc);
    fn insert_point(&mut self, point: Point);
}

mod corner_arcs;
#[cfg(test)]
mod geometry_tests;
mod paths;
