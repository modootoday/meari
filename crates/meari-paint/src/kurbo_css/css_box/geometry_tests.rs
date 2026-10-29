//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::Edge;
use super::super::non_uniform_radii::NonUniformRoundedRectRadii;
use super::corner_arcs::start_angle;
use super::*;
use kurbo::{BezPath, Insets, PathEl, Point, Rect, Vec2};

#[cfg(test)]
mod tests {
    use super::*;

    /// `start_angle` must return the angle `t` at which the border colour split
    /// line crosses the corner ellipse, i.e. the solution of
    /// `(cos t - 1) / (sin t - 1) == k` where `k = radii.y / (w * radii.x)`.
    fn assert_solves(bt: f64, br: f64, radii: Vec2) {
        let t = start_angle(bt, br, radii);
        assert!(t.is_finite(), "start_angle returned {t} for {radii:?}");
        assert!(
            (0.0..=std::f64::consts::FRAC_PI_2).contains(&t),
            "t={t} out of range"
        );
        let w = bt / br;
        let k = radii.y / (w * radii.x);
        let lhs = (t.cos() - 1.0) / (t.sin() - 1.0);
        assert!(
            (lhs - k).abs() < 1e-9,
            "t={t} does not solve k={k} (got {lhs})"
        );
    }

    /// Regression test for elliptical corners where the vertical radius exceeds
    /// the horizontal one (`ry > rx`). `kurbo::Ellipse::radii()` canonicalises
    /// such an ellipse by swapping its axes and adding a `π/2` rotation; when
    /// that rotation was dropped the corner arcs were drawn transposed, skewing
    /// the whole box. The straight portions of each edge must stay axis aligned:
    /// the top/bottom edges horizontal and the left/right edges vertical.
    #[test]
    fn edges_stay_axis_aligned_for_tall_corners() {
        let b = CssBox::new(
            Rect::new(0.0, 0.0, 400.0, 200.0),
            Insets::uniform(10.0),
            Insets::ZERO,
            0.0,
            NonUniformRoundedRectRadii {
                top_left: Vec2::new(60.0, 20.0),
                top_right: Vec2::new(20.0, 50.0), // ry > rx
                bottom_right: Vec2::new(50.0, 10.0),
                bottom_left: Vec2::new(30.0, 40.0), // ry > rx
            },
        );

        // The outer border box corner y (top) / x (right) etc. that the straight
        // part of each edge should run along.
        let checks = [
            (Edge::Top, 0.0),      // outer top edge at y == 0
            (Edge::Bottom, 200.0), // outer bottom edge at y == 200
            (Edge::Left, 0.0),     // outer left edge at x == 0
            (Edge::Right, 400.0),  // outer right edge at x == 400
        ];
        // Collect every point (endpoints and Bézier control points) of a path.
        // A cubic Bézier lies within the convex hull of its control points, so
        // checking these is enough to prove the whole path stays in the box.
        let points = |path: &BezPath| -> Vec<Point> {
            path.elements()
                .iter()
                .flat_map(|el| match *el {
                    PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
                    PathEl::QuadTo(a, b) => vec![a, b],
                    PathEl::CurveTo(a, b, c) => vec![a, b, c],
                    PathEl::ClosePath => vec![],
                })
                .collect()
        };

        for (edge, expected) in checks {
            let path = b.border_edge_shape(edge);
            let pts = points(&path);
            // The transposed-axis bug pushed points well outside the border box.
            for p in &pts {
                assert!(
                    (-0.01..=400.01).contains(&p.x) && (-0.01..=200.01).contains(&p.y),
                    "{edge:?}: point {p:?} escaped the border box"
                );
            }
            // And the outer straight run must actually reach the box edge.
            let reaches = pts.iter().any(|p| match edge {
                Edge::Top | Edge::Bottom => (p.y - expected).abs() < 0.01,
                Edge::Left | Edge::Right => (p.x - expected).abs() < 0.01,
            });
            assert!(
                reaches,
                "{edge:?}: no point reached the outer edge {expected}"
            );
        }
    }

    #[test]
    fn should_solve_properly() {
        // 0.643501
        assert!((start_angle(4.0, 1.0, Vec2 { x: 1.0, y: 2.0 }) - 0.643501).abs() < 1e-5);
    }

    /// Regression test: when `k == radii.y / (w * radii.x)` is exactly 2 the
    /// old closed form evaluated `0 / 0` and produced `NaN`, corrupting the
    /// corner arc. This happens for ordinary elliptical corners such as an
    /// 80px/30px radius with 40px/10px border widths (inner/padding ellipse
    /// radii 40/20, widths 30/0 ... => k == 2).
    #[test]
    fn handles_k_equal_two() {
        // k = radii.y / (w * radii.x) = 40 / ((10/40) * 80) = 2.0
        assert_solves(10.0, 40.0, Vec2 { x: 80.0, y: 40.0 });
    }

    #[test]
    fn solves_a_range_of_elliptical_corners() {
        for &(bt, br) in &[(1.0, 1.0), (1.0, 4.0), (4.0, 1.0), (3.0, 7.0)] {
            for &(rx, ry) in &[(80.0, 30.0), (30.0, 80.0), (60.0, 60.0), (120.0, 20.0)] {
                assert_solves(bt, br, Vec2 { x: rx, y: ry });
            }
        }
    }
}

#[test]
fn detects_elliptical_border() {
    let corners = |x: f64, y: f64| NonUniformRoundedRectRadii {
        top_left: Vec2::new(x, y),
        top_right: Vec2::new(x, y),
        bottom_right: Vec2::new(x, y),
        bottom_left: Vec2::new(x, y),
    };
    let css_box = |w: f64, h: f64, radii: NonUniformRoundedRectRadii| {
        CssBox::new(
            Rect::new(0.0, 0.0, w, h),
            Insets::uniform(1.0),
            Insets::ZERO,
            0.0,
            radii,
        )
    };

    // Circle: square box, every radius == half the side.
    assert!(css_box(44.0, 44.0, corners(22.0, 22.0)).is_elliptical_border());
    // Ellipse: non-square box, radii == half each axis.
    assert!(css_box(120.0, 64.0, corners(60.0, 32.0)).is_elliptical_border());
    // Rounded rectangle: radius smaller than half → has straight edges.
    assert!(!css_box(44.0, 44.0, corners(10.0, 10.0)).is_elliptical_border());
    // Sharp rectangle: no rounding.
    assert!(!css_box(44.0, 44.0, corners(0.0, 0.0)).is_elliptical_border());
}

#[test]
fn detects_uniform_corner_border() {
    let corners = |x: f64, y: f64| NonUniformRoundedRectRadii {
        top_left: Vec2::new(x, y),
        top_right: Vec2::new(x, y),
        bottom_right: Vec2::new(x, y),
        bottom_left: Vec2::new(x, y),
    };
    let css_box = |w: f64, h: f64, radii: NonUniformRoundedRectRadii| {
        CssBox::new(
            Rect::new(0.0, 0.0, w, h),
            Insets::uniform(1.0),
            Insets::ZERO,
            0.0,
            radii,
        )
    };

    // Sharp rectangle: no rounding, still trivially "uniform" (0 == 0).
    assert!(css_box(44.0, 44.0, corners(0.0, 0.0)).is_uniform_corner_border());
    // Ordinary rounded rectangle: each corner is a circular arc.
    assert!(css_box(44.0, 44.0, corners(10.0, 10.0)).is_uniform_corner_border());
    // Circle: square box, every radius == half the side.
    assert!(css_box(44.0, 44.0, corners(22.0, 22.0)).is_uniform_corner_border());
    // Ellipse: non-square box, radii == half each axis → corners aren't circular.
    assert!(!css_box(120.0, 64.0, corners(60.0, 32.0)).is_uniform_corner_border());
    // Per-corner elliptical radius (rx != ry) anywhere disqualifies the box, even
    // if it isn't a full border-radius: 50% ellipse.
    let mixed = NonUniformRoundedRectRadii {
        top_left: Vec2::new(10.0, 5.0),
        ..corners(10.0, 10.0)
    };
    assert!(!css_box(44.0, 44.0, mixed).is_uniform_corner_border());

    // Corners may differ in radius from each other, as long as each is circular.
    let differing = NonUniformRoundedRectRadii {
        top_left: Vec2::new(10.0, 10.0),
        top_right: Vec2::new(5.0, 5.0),
        bottom_right: Vec2::new(8.0, 8.0),
        bottom_left: Vec2::new(2.0, 2.0),
    };
    assert!(css_box(44.0, 44.0, differing).is_uniform_corner_border());
}
