//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::non_uniform_radii::NonUniformRoundedRectRadii;
use super::super::{Corner, CssBoxKind, Direction, Edge, get_corner_insets};
use super::*;
use kurbo::{Arc, BezPath, Insets, Point, Rect, Vec2};
use std::{f64::consts::FRAC_PI_2, f64::consts::PI};

impl CssBox {
    pub fn new(
        border_box: Rect,
        border: Insets,
        padding: Insets,
        outline_width: f64,
        mut border_radii: NonUniformRoundedRectRadii,
    ) -> Self {
        let padding_box = border_box - border;
        let content_box = padding_box - padding;
        let outline_box = border_box.inset(outline_width);

        // Correct the border radii if they are too big if two border radii would intersect, then we need to shrink
        // ALL border radii by the same factor such that they do not
        let top_overlap_factor =
            border_box.width() / (border_radii.top_left.x + border_radii.top_right.x);
        let bottom_overlap_factor =
            border_box.width() / (border_radii.bottom_left.x + border_radii.bottom_right.x);
        let left_overlap_factor =
            border_box.height() / (border_radii.top_left.y + border_radii.bottom_left.y);
        let right_overlap_factor =
            border_box.height() / (border_radii.top_right.y + border_radii.bottom_right.y);

        let min_factor = top_overlap_factor
            .min(bottom_overlap_factor)
            .min(left_overlap_factor)
            .min(right_overlap_factor)
            .min(1.0);
        if min_factor < 1.0 {
            border_radii *= min_factor
        }

        Self {
            padding_box,
            border_box,
            content_box,
            outline_box,
            outline_width,
            padding_width: padding,
            border_width: border,
            border_radii,
        }
    }

    /// Construct a BezPath representing one edge of a box's border.
    /// Takes into account border-radius and the possibility that the edges
    /// are different colors.
    ///
    /// Will construct the border by:
    /// - drawing an inner arc
    /// - jumping to an outer arc
    /// - jumping to the next outer arc (completing the edge with the previous)
    /// - drawing an inner arc
    pub fn border_edge_shape(&self, edge: Edge) -> BezPath {
        use {Corner::*, CssBoxKind::*, Direction::*, Edge::*};

        let mut path = BezPath::new();

        let (c0, c1) = match edge {
            Top => (TopLeft, TopRight),
            Right => (TopRight, BottomRight),
            Bottom => (BottomRight, BottomLeft),
            Left => (BottomLeft, TopLeft),
        };

        // 1. First corner
        // if the radius is bigger than the border, we need to draw the inner arc to fill in the gap
        if self.is_sharp(c0, BorderBox) {
            path.move_to(self.corner(c0, PaddingBox));
            path.line_to(self.corner(c0, BorderBox));
        } else {
            match self.corner_needs_infill(c0) {
                true => {
                    path.insert_arc(self.partial_corner_arc(c0, PaddingBox, edge, Anticlockwise))
                }
                false => path.move_to(self.corner(c0, PaddingBox)),
            }
            path.insert_arc(self.partial_corner_arc(c0, BorderBox, edge, Clockwise));
        }

        // 2. Second corner
        if self.is_sharp(c1, BorderBox) {
            path.line_to(self.corner(c1, BorderBox));
            path.line_to(self.corner(c1, PaddingBox));
        } else {
            path.insert_arc(self.partial_corner_arc(c1, BorderBox, edge, Clockwise));
            match self.corner_needs_infill(c1) {
                true => {
                    path.insert_arc(self.partial_corner_arc(c1, PaddingBox, edge, Anticlockwise))
                }
                false => path.line_to(self.corner(c1, PaddingBox)),
            }
        }

        path
    }

    /// Whether any corner of this box has a non-zero border radius.
    pub fn has_border_radius(&self) -> bool {
        let r = &self.border_radii;
        [r.top_left, r.top_right, r.bottom_right, r.bottom_left]
            .iter()
            .any(|radius| radius.x > 0.0 || radius.y > 0.0)
    }

    /// Construct a new [`CssBox`] representing a "slice" of this box's border,
    /// running from `start_frac` to `end_frac` of the border width (measured as
    /// a fraction from the outer border-box edge inwards).
    ///
    /// The returned box's border region is exactly the requested slice, so
    /// [`CssBox::border_edge_shape`] can then be used to render it. This is used
    /// to draw the two lines of a `double` border.
    pub fn border_slice(&self, start_frac: f64, end_frac: f64) -> CssBox {
        use Corner::*;

        let scale_insets = |frac: f64| Insets {
            x0: self.border_width.x0 * frac,
            y0: self.border_width.y0 * frac,
            x1: self.border_width.x1 * frac,
            y1: self.border_width.y1 * frac,
        };

        let start_insets = scale_insets(start_frac);
        let slice_border = scale_insets(end_frac - start_frac);

        // Move the outer edge of the box inwards to the start of the slice.
        let slice_border_box = self.border_box - start_insets;

        // Border radii shrink as we move inwards through the border, matching the
        // model used when computing inner (padding/content) box radii.
        let reduce = |radius: Vec2, corner: Corner| {
            let inset = get_corner_insets(start_insets, corner);
            Vec2 {
                x: (radius.x - inset.x).max(0.0),
                y: (radius.y - inset.y).max(0.0),
            }
        };
        let slice_radii = NonUniformRoundedRectRadii {
            top_left: reduce(self.border_radii.top_left, TopLeft),
            top_right: reduce(self.border_radii.top_right, TopRight),
            bottom_right: reduce(self.border_radii.bottom_right, BottomRight),
            bottom_left: reduce(self.border_radii.bottom_left, BottomLeft),
        };

        CssBox::new(
            slice_border_box,
            slice_border,
            Insets::ZERO,
            0.0,
            slice_radii,
        )
    }

    /// Construct a bezpath drawing the outline
    pub fn outline(&self) -> BezPath {
        let mut path = BezPath::new();

        // TODO: this has been known to produce quirky outputs with hugely rounded edges
        self.shape(&mut path, CssBoxKind::OutlineBox, Direction::Clockwise);
        path.move_to(self.corner(Corner::TopLeft, CssBoxKind::BorderBox));

        self.shape(&mut path, CssBoxKind::BorderBox, Direction::Anticlockwise);
        path.move_to(self.corner(Corner::TopLeft, CssBoxKind::BorderBox));

        path
    }

    /// Construct a bezpath drawing the frame border
    pub fn border_box_path(&self) -> BezPath {
        let mut path = BezPath::new();
        self.shape(&mut path, CssBoxKind::BorderBox, Direction::Clockwise);
        path
    }

    /// Construct a bezpath drawing the frame padding
    pub fn padding_box_path(&self) -> BezPath {
        let mut path = BezPath::new();
        self.shape(&mut path, CssBoxKind::PaddingBox, Direction::Clockwise);
        path
    }

    /// Construct a bezpath drawing the frame content
    pub fn content_box_path(&self) -> BezPath {
        let mut path = BezPath::new();
        self.shape(&mut path, CssBoxKind::ContentBox, Direction::Clockwise);
        path
    }

    /// Whether the border box is a full ellipse (which includes a circle): every
    /// corner radius equals half the box in that axis, i.e. `border-radius: 50%`.
    /// Such a border is a single continuous curve with no straight edges; it is best
    /// drawn as a stroked ellipse rather than a filled two-contour annulus
    /// (see `draw_border`), which otherwise leaves seam notches on a thin ring where
    /// the quarter-arcs meet. Circles are already covered by the stroked-rounded-rect
    /// path (see [`Self::is_uniform_corner_border`]); this catches the true ellipses
    /// (unequal axes) that a rounded rect can't represent.
    pub fn is_elliptical_border(&self) -> bool {
        let (rx, ry) = (
            self.border_box.width() / 2.0,
            self.border_box.height() / 2.0,
        );
        let is_half = |c: Vec2| (c.x - rx).abs() < 0.01 && (c.y - ry).abs() < 0.01;
        let radii = &self.border_radii;
        is_half(radii.top_left)
            && is_half(radii.top_right)
            && is_half(radii.bottom_right)
            && is_half(radii.bottom_left)
    }

    /// Whether every corner has an equal x and y radius, i.e. each corner is a
    /// circular (not elliptical) arc. Corners may still differ from one another, so
    /// this also covers plain rectangles (radius 0) and ordinary rounded rectangles,
    /// not just circles. Such a border can be drawn as a single stroked
    /// `kurbo::RoundedRect` rather than the filled per-edge annulus in
    /// `draw_border` — simpler and faster, since it avoids building and filling a
    /// separate path per edge. (`draw_border` additionally requires uniform border
    /// width/color and each radius to be 0 or ≥ the border width for the stroke to
    /// reproduce the CSS shape exactly.)
    pub fn is_uniform_corner_border(&self) -> bool {
        let is_circular = |c: Vec2| (c.x - c.y).abs() < 0.01;
        let radii = &self.border_radii;
        is_circular(radii.top_left)
            && is_circular(radii.top_right)
            && is_circular(radii.bottom_right)
            && is_circular(radii.bottom_left)
    }

    pub(super) fn shape(&self, path: &mut BezPath, line: CssBoxKind, direction: Direction) {
        use Corner::*;

        let route = match direction {
            Direction::Clockwise => [TopLeft, TopRight, BottomRight, BottomLeft],
            Direction::Anticlockwise => [TopLeft, BottomLeft, BottomRight, TopRight],
        };

        for corner in route {
            if self.is_sharp(corner, line) {
                path.insert_point(self.corner(corner, line));
            } else {
                path.insert_arc(self.corner_arc(corner, line, direction));
            }
        }
    }

    /// Construct a bezpath drawing the frame
    pub fn shadow_clip(&self, shadow_rect: Rect) -> BezPath {
        let mut path = BezPath::new();
        self.shadow_clip_shape(&mut path, shadow_rect);
        path
    }

    pub(super) fn shadow_clip_shape(&self, path: &mut BezPath, shadow_rect: Rect) {
        use Corner::*;

        for corner in [TopLeft, TopRight, BottomRight, BottomLeft] {
            path.insert_point(self.shadow_clip_corner(corner, shadow_rect));
        }

        if self.is_sharp(TopLeft, CssBoxKind::BorderBox) {
            path.move_to(self.corner(TopLeft, CssBoxKind::BorderBox));
        } else {
            const TOLERANCE: f64 = 0.1;
            let arc = self.corner_arc(TopLeft, CssBoxKind::BorderBox, Direction::Anticlockwise);
            let elements = arc.path_elements(TOLERANCE);
            path.extend(elements);
        }

        for corner in [/*TopLeft, */ BottomLeft, BottomRight, TopRight] {
            if self.is_sharp(corner, CssBoxKind::BorderBox) {
                path.insert_point(self.corner(corner, CssBoxKind::BorderBox));
            } else {
                path.insert_arc(self.corner_arc(
                    corner,
                    CssBoxKind::BorderBox,
                    Direction::Anticlockwise,
                ));
            }
        }
    }

    pub(super) fn corner(&self, corner: Corner, css_box: CssBoxKind) -> Point {
        let Rect { x0, y0, x1, y1 } = match css_box {
            CssBoxKind::OutlineBox => self.outline_box,
            CssBoxKind::BorderBox => self.border_box,
            CssBoxKind::PaddingBox => self.padding_box,
            CssBoxKind::ContentBox => self.content_box,
        };
        match corner {
            Corner::TopLeft => Point { x: x0, y: y0 },
            Corner::TopRight => Point { x: x1, y: y0 },
            Corner::BottomLeft => Point { x: x0, y: y1 },
            Corner::BottomRight => Point { x: x1, y: y1 },
        }
    }

    pub(super) fn shadow_clip_corner(&self, corner: Corner, shadow_rect: Rect) -> Point {
        let (x, y) = match corner {
            Corner::TopLeft => (shadow_rect.x0, shadow_rect.y0),
            Corner::TopRight => (shadow_rect.x1, shadow_rect.y0),
            Corner::BottomRight => (shadow_rect.x1, shadow_rect.y1),
            Corner::BottomLeft => (shadow_rect.x0, shadow_rect.y1),
        };

        Point { x, y }
    }

    /// Check if the corner width is smaller than the radius.
    /// If it is, we need to fill in the gap with an arc
    pub(super) fn corner_needs_infill(&self, corner: Corner) -> bool {
        match corner {
            Corner::TopLeft => {
                self.border_radii.top_left.x > self.border_width.x0
                    && self.border_radii.top_left.y > self.border_width.y0
            }
            Corner::TopRight => {
                self.border_radii.top_right.x > self.border_width.x1
                    && self.border_radii.top_right.y > self.border_width.y0
            }
            Corner::BottomRight => {
                self.border_radii.bottom_right.x > self.border_width.x1
                    && self.border_radii.bottom_right.y > self.border_width.y1
            }
            Corner::BottomLeft => {
                self.border_radii.bottom_left.x > self.border_width.x0
                    && self.border_radii.bottom_left.y > self.border_width.y1
            }
        }
    }

    /// Get the complete arc for a corner, skipping the need for splitting the arc into pieces
    pub(super) fn corner_arc(
        &self,
        corner: Corner,
        css_box: CssBoxKind,
        direction: Direction,
    ) -> Arc {
        let (center, radii) = self.ellipse(corner, css_box);

        // Sweep clockwise for outer arcs, counter clockwise for inner arcs
        let sweep_direction = match direction {
            Direction::Anticlockwise => -1.0,
            Direction::Clockwise => 1.0,
        };

        let offset = match corner {
            Corner::TopLeft => -FRAC_PI_2,
            Corner::TopRight => 0.0,
            Corner::BottomRight => FRAC_PI_2,
            Corner::BottomLeft => PI,
        };

        let offset = match direction {
            Direction::Clockwise => offset,
            Direction::Anticlockwise => offset + FRAC_PI_2,
        };

        Arc::new(
            center,
            radii,
            // Note that we apply a fixed offset to get us in the unit circle coordinate system
            // vello chooses the x axis as the start of the arc, so we need to offset by 3pi/2
            offset + PI + FRAC_PI_2,
            FRAC_PI_2 * sweep_direction,
            0.0,
        )
    }
}
