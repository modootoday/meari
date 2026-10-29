//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::{Corner, CssBoxKind, Direction, Edge, add_insets, get_corner_insets};
use super::*;
use kurbo::{Arc, BezPath, Insets, PathEl, Point, Vec2};
use std::{f64::consts::FRAC_PI_2, f64::consts::PI};

impl CssBox {
    /// Get the arc for a half of a corner.
    /// This handles the case where adjacent border sides have different colors and thus
    /// the corner between the side changes color in the middle. We draw these as separate shapes
    /// and thus need to to get the arc up to the "middle point".
    ///
    /// The angle at which the color changes depends on the ratio of the border widths and radii of the corner
    pub(super) fn partial_corner_arc(
        &self,
        corner: Corner,
        css_box: CssBoxKind,
        edge: Edge,
        direction: Direction,
    ) -> Arc {
        use Corner::*;
        use CssBoxKind::*;
        use Edge::*;

        let (center, radii) = self.ellipse(corner, css_box);

        // We solve a tiny system of equations to find the start angle
        // This is fixed to a single coordinate system, so we need to adjust the start angle
        let theta = self.start_angle(corner, radii);

        // Sweep clockwise for outer arcs, counter clockwise for inner arcs
        let sweep_direction = match direction {
            Direction::Anticlockwise => -1.0,
            Direction::Clockwise => 1.0,
        };

        // Easier to reason about if we think about just offsetting the turns from the start
        let offset = match edge {
            Top => 0.0,
            Right => FRAC_PI_2,
            Bottom => PI,
            Left => PI + FRAC_PI_2,
        };

        // On left/right gets theta, on top/bottom gets pi/2 - theta
        let theta = match edge {
            Top | Bottom => FRAC_PI_2 - theta,
            Right | Left => theta,
        };

        // Depededning on the edge, we need to adjust the start angle
        // We still sweep the same, but the theta split is different since we're cutting in half
        // I imagine you could mnake this simpler using a bit more math
        let start = match (edge, corner, css_box) {
            // Top Edge
            (Top, TopLeft, PaddingBox) => 0.0,
            (Top, TopLeft, BorderBox) => -theta,
            (Top, TopRight, BorderBox) => 0.0,
            (Top, TopRight, PaddingBox) => theta,

            // Right Edge
            (Right, TopRight, PaddingBox) => 0.0,
            (Right, TopRight, BorderBox) => -theta,
            (Right, BottomRight, BorderBox) => 0.0,
            (Right, BottomRight, PaddingBox) => theta,

            // Bottom Edge
            (Bottom, BottomRight, PaddingBox) => 0.0,
            (Bottom, BottomRight, BorderBox) => -theta,
            (Bottom, BottomLeft, BorderBox) => 0.0,
            (Bottom, BottomLeft, PaddingBox) => theta,

            // Left Edge
            (Left, BottomLeft, PaddingBox) => 0.0,
            (Left, BottomLeft, BorderBox) => -theta,
            (Left, TopLeft, BorderBox) => 0.0,
            (Left, TopLeft, PaddingBox) => theta,

            _ => unreachable!("Invalid edge/corner combination"),
        };

        Arc::new(
            center,
            radii,
            // Note that we apply a fixed offset to get us in the unit circle coordinate system
            // vello chooses the x axis as the start of the arc, so we need to offset by 3pi/2
            start + offset + PI + FRAC_PI_2,
            theta * sweep_direction,
            0.0,
        )
    }

    /// Check if a corner is sharp (IE the absolute radius is 0)
    pub(super) fn is_sharp(&self, corner: Corner, side: CssBoxKind) -> bool {
        use Corner::*;
        use CssBoxKind::*;

        let corner_radii = match corner {
            TopLeft => self.border_radii.top_left,
            TopRight => self.border_radii.top_right,
            BottomLeft => self.border_radii.bottom_left,
            BottomRight => self.border_radii.bottom_right,
        };
        let is_sharp = (corner_radii.x == 0.0) | (corner_radii.y == 0.0);
        if is_sharp {
            return true;
        }

        let css_box: Insets = match side {
            OutlineBox => return false,
            BorderBox => return false,
            PaddingBox => self.border_width,
            ContentBox => add_insets(self.border_width, self.padding_width),
        };
        match corner {
            TopLeft => (corner_radii.x <= css_box.x0) | (corner_radii.y <= css_box.y0),
            TopRight => (corner_radii.x <= css_box.x1) | (corner_radii.y <= css_box.y0),
            BottomLeft => (corner_radii.x <= css_box.x0) | (corner_radii.y <= css_box.y1),
            BottomRight => (corner_radii.x <= css_box.x1) | (corner_radii.y <= css_box.y1),
        }
    }

    /// The `(center, radii)` of the ellipse that a given corner traces along the
    /// given box edge.
    ///
    /// The radii are returned as an axis-aligned `(x, y)` pair, matching the CSS
    /// `border-radius` horizontal/vertical radii. We deliberately do *not* return
    /// a [`kurbo::Ellipse`]: `Ellipse::radii()` canonicalises the ellipse via an
    /// SVD, which swaps the axes and introduces a `π/2` rotation whenever the
    /// vertical radius exceeds the horizontal one (`ry > rx`). Callers here build
    /// [`kurbo::Arc`]s with a fixed `x_rotation` of `0`, so that swap would draw
    /// the corner with its axes transposed. Returning the raw radii avoids the
    /// round-trip entirely.
    pub(super) fn ellipse(&self, corner: Corner, side: CssBoxKind) -> (Point, Vec2) {
        use {Corner::*, CssBoxKind::*};
        let CssBox {
            border_box,
            padding_width,
            border_width,
            border_radii,
            ..
        } = self;

        let corner_radii = match corner {
            TopLeft => border_radii.top_left,
            TopRight => border_radii.top_right,
            BottomLeft => border_radii.bottom_left,
            BottomRight => border_radii.bottom_right,
        };

        let center = match corner {
            TopLeft => corner_radii,
            TopRight => Vec2 {
                x: border_box.width() - corner_radii.x,
                y: corner_radii.y,
            },
            BottomLeft => Vec2 {
                x: corner_radii.x,
                y: border_box.height() - corner_radii.y,
            },
            BottomRight => Vec2 {
                x: border_box.width() - corner_radii.x,
                y: border_box.height() - corner_radii.y,
            },
        };

        let radii: Vec2 = match side {
            BorderBox => corner_radii,
            OutlineBox => corner_radii + Vec2::new(self.outline_width, self.outline_width),
            PaddingBox => corner_radii - get_corner_insets(*border_width, corner),
            ContentBox => {
                corner_radii - get_corner_insets(add_insets(*border_width, *padding_width), corner)
            }
        };

        (border_box.origin() + center, radii)
    }

    pub(super) fn start_angle(&self, corner: Corner, radii: Vec2) -> f64 {
        let corner_insets = get_corner_insets(self.border_width, corner);
        start_angle(corner_insets.y, corner_insets.x, radii)
    }
}

impl BuildBezpath for BezPath {
    /// Vello uses an inner tolerance for creating segments
    /// We're just reusing the value here
    const TOLERANCE: f64 = 0.1;

    fn insert_arc(&mut self, arc: Arc) {
        let mut elements = arc.path_elements(Self::TOLERANCE);
        match elements.next().unwrap() {
            PathEl::MoveTo(a) if !self.elements().is_empty() => self.push(PathEl::LineTo(a)),
            el => self.push(el),
        }
        self.extend(elements)
    }

    fn insert_point(&mut self, point: Point) {
        if self.elements().is_empty() {
            self.push(PathEl::MoveTo(point));
        } else {
            self.push(PathEl::LineTo(point));
        }
    }
}

/// Get the start angle of the arc based on the border width and the radii
pub(super) fn start_angle(bt_width: f64, br_width: f64, radii: Vec2) -> f64 {
    // slope of the border intersection split
    let w = bt_width / br_width;
    let x = radii.y / (w * radii.x);

    /*
    Any point on an ellipse is given by:
    x = a cos(t)
    y = b sin(t)

    The equation of the border intersect is:
    y = w (x - a) + b

    where w is the ratio of the width to the height of the border
    and b is the y intercept of the border
    and x is the x intercept of the border

    This formula is the result of solving the system of equations:
    x = a cos(t)
    y = b sin(t)
    y = w (x - a) + b

    b/(w*a) = (cos(t) - 1)/(sin(t) - 1)

    Substituting s = tan(t/2) turns this into the quadratic

        (k - 2) s² - 2k s + k = 0     where k = b/(w*a) = x

    whose relevant root can be written (after rationalising to remove the
    catastrophic cancellation / removable singularity the naive quadratic
    formula has at k == 2) as:

        s = √k / (√k + √2)

    This form is well behaved for all k >= 0 (in particular around k == 2,
    which occurs for perfectly ordinary elliptical corners, e.g. a 80px/30px
    radius with 40px/10px border widths), always yielding t in [0, π/2).
    */

    use std::f64::consts::SQRT_2;
    let sqrt_x = x.sqrt();
    let s = sqrt_x / (sqrt_x + SQRT_2);
    s.atan() * 2.0
}
