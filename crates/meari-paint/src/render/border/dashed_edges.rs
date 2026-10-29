//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::{
    kurbo_css::Edge,
    render::{ElementCx, PhysicalTracks},
};
use anyrender::PaintScene;
use blitz_dom::node::SpecialElementData;
use kurbo::{BezPath, Cap, Circle, Point, Rect, Stroke};
use peniko::{Color, Fill};
use style::{
    computed_values::border_collapse::T as BorderCollapse,
    values::computed::{BorderStyle, OutlineStyle},
};

impl ElementCx<'_, '_> {
    /// Draw a single `dashed` border edge.
    ///
    /// The dashes are produced by stroking the centre line of the border with a
    /// dash pattern (butt caps give them flat, square ends). Everything is clipped
    /// to the edge's region (the same trapezoid used by the solid path) so corners
    /// are mitred, adjacent edges of different colors don't overlap, and any
    /// border-radius is respected.
    pub(super) fn draw_dashed_border_edge(
        &self,
        scene: &mut impl PaintScene,
        edge: Edge,
        color: Color,
    ) {
        let thickness = self.edge_width(edge);
        if thickness <= 0.0 {
            return;
        }

        let (dash_ratio, gap_ratio) = dashed_ratios(thickness, self.scale);

        // Work out the centre line to stroke and the dash/gap lengths along it.
        let (centerline, dash, gap) = if self.frame.has_border_radius() {
            // Rounded corners: stroke the rounded centre line running through the
            // whole perimeter. Every edge uses the same centre line and pattern, so
            // dashes stay continuous and aligned as they wrap around each corner.
            // Dash and gap keep their ratio, sized so a whole number of periods fit
            // exactly around the perimeter (kurbo merges the dash across the seam).
            let mut centerline = self.frame.border_slice(0.0, 0.5).padding_box_path();
            centerline.close_path();
            let perimeter = centerline.perimeter(0.1);
            if perimeter <= 0.0 {
                return;
            }
            let period0 = (dash_ratio + gap_ratio) * thickness;
            let count = (perimeter / period0).round().max(1.0);
            let period = perimeter / count;
            let dash = period * dash_ratio / (dash_ratio + gap_ratio);
            (centerline, dash, period - dash)
        } else {
            // Square corners: stroke a straight line through the middle of the edge,
            // corner to corner. Dash and gap keep their ratio but are scaled so the
            // edge both starts and ends with a dash (covering the corners).
            let bb = self.frame.border_box;
            let half = thickness / 2.0;
            let (start, end, length) = match edge {
                Edge::Top => (
                    Point::new(bb.x0, bb.y0 + half),
                    Point::new(bb.x1, bb.y0 + half),
                    bb.width(),
                ),
                Edge::Bottom => (
                    Point::new(bb.x0, bb.y1 - half),
                    Point::new(bb.x1, bb.y1 - half),
                    bb.width(),
                ),
                Edge::Left => (
                    Point::new(bb.x0 + half, bb.y0),
                    Point::new(bb.x0 + half, bb.y1),
                    bb.height(),
                ),
                Edge::Right => (
                    Point::new(bb.x1 - half, bb.y0),
                    Point::new(bb.x1 - half, bb.y1),
                    bb.height(),
                ),
            };
            if length <= 0.0 {
                return;
            }
            let dash0 = dash_ratio * thickness;
            let gap0 = gap_ratio * thickness;
            // `count` dashes with `count - 1` gaps between them.
            let count = ((length + gap0) / (dash0 + gap0)).round().max(1.0);
            let r = dash_ratio / gap_ratio;
            let gap = length / (count * r + count - 1.0);

            let mut line = BezPath::new();
            line.move_to(start);
            line.line_to(end);
            (line, r * gap, gap)
        };

        let stroke = Stroke::new(thickness)
            .with_caps(Cap::Butt)
            .with_dashes(0.0, [dash, gap]);
        let clip = self.frame.border_edge_shape(edge);
        scene.push_clip_layer(Fill::NonZero, self.transform, &clip);
        scene.stroke(&stroke, self.transform, color, None, &centerline);
        scene.pop_layer();
    }

    /// Draw a single `dotted` border edge.
    ///
    /// Dots are filled circles (diameter == border thickness). Unlike dashes they
    /// can't be produced by stroking, because kurbo doesn't emit zero-length dashes
    /// (so a round-capped dash pattern would render nothing); drawing them
    /// explicitly also lets us anchor a dot in each square corner. Everything is
    /// clipped to the edge's region, as for the other styles.
    pub(super) fn draw_dotted_border_edge(
        &self,
        scene: &mut impl PaintScene,
        edge: Edge,
        color: Color,
    ) {
        let thickness = self.edge_width(edge);
        if thickness <= 0.0 {
            return;
        }
        let radius = thickness / 2.0;

        let mut path = BezPath::new();
        if self.frame.has_border_radius() {
            // Rounded corners: dots spaced evenly around the rounded centre line so
            // the ring wraps seamlessly around the corners.
            let mut centerline = self.frame.border_slice(0.0, 0.5).padding_box_path();
            centerline.close_path();
            let perimeter = centerline.perimeter(0.1);
            if perimeter <= 0.0 {
                return;
            }
            let count = (perimeter / (2.0 * thickness)).round().max(1.0);
            let spacing = perimeter / count;
            for center in sample_points_along_path(&centerline, spacing, count as usize) {
                path.extend(Circle::new(center, radius).path_elements(0.1));
            }
        } else {
            // Square corners: a dot is anchored in each corner (both ends of the
            // edge, inset by the radius so it sits snugly in the corner) and the
            // rest are spread evenly between them.
            let bb = self.frame.border_box;
            let length = match edge {
                Edge::Top | Edge::Bottom => bb.width(),
                Edge::Left | Edge::Right => bb.height(),
            };
            if length <= 0.0 {
                return;
            }
            let dot_center = |along: f64| -> Point {
                match edge {
                    Edge::Top => Point::new(bb.x0 + along, bb.y0 + radius),
                    Edge::Bottom => Point::new(bb.x0 + along, bb.y1 - radius),
                    Edge::Left => Point::new(bb.x0 + radius, bb.y0 + along),
                    Edge::Right => Point::new(bb.x1 - radius, bb.y0 + along),
                }
            };

            let span = length - thickness;
            if span <= 0.0 {
                // Edge too short to fit two dots; place a single centred dot.
                let center = dot_center(length / 2.0);
                path.extend(Circle::new(center, radius).path_elements(0.1));
            } else {
                // Aim for a centre-to-centre spacing of about two dot diameters,
                // then adjust it so the first and last dots land in the corners.
                let gaps = (span / (2.0 * thickness)).round().max(1.0) as usize;
                let spacing = span / gaps as f64;
                for i in 0..=gaps {
                    let center = dot_center(radius + i as f64 * spacing);
                    path.extend(Circle::new(center, radius).path_elements(0.1));
                }
            }
        }

        let clip = self.frame.border_edge_shape(edge);
        scene.push_clip_layer(Fill::NonZero, self.transform, &clip);
        scene.fill(Fill::NonZero, self.transform, color, None, &path);
        scene.pop_layer();
    }

    pub(crate) fn draw_table_borders(&self, scene: &mut impl PaintScene) {
        let SpecialElementData::TableRoot(table) = &self.element.special_data else {
            return;
        };
        // Borders are only handled at the table level when BorderCollapse::Collapse
        if table.border_collapse != BorderCollapse::Collapse {
            return;
        }

        let Some(grid_info) = &mut *table.computed_grid_info.borrow_mut() else {
            return;
        };
        let Some(border_style) = table.border_style.as_deref() else {
            return;
        };

        let outer_border_style = self.style.get_border();

        let cols = PhysicalTracks::from_tracks(&grid_info.columns);
        let rows = PhysicalTracks::from_tracks(&grid_info.rows);

        // TODO: support different colors for different borders
        let current_color = self.style.clone_color();
        let border_color = border_style
            .border_top_color
            .resolve_to_absolute(&current_color)
            .as_srgb_color();

        // No need to draw transparent borders (as they won't be visible anyway)
        if border_color == Color::TRANSPARENT {
            return;
        }

        // Border widths are not adjusted for border-style in computed styles,
        // so a border with `none`/`hidden` style must be treated as zero-width.
        if border_style.border_top_style.none_or_hidden() {
            return;
        }

        // The borders are laid out as the gutters between the tracks, and as the border
        // of the table around them. Track positions are relative to the table's border box.
        let border_width = table.style.gap.map(|gap| gap.into_raw().value() as f64);
        let left = cols.origin() as f64 - border_width.width;
        let right = (cols.origin() + cols.span()) as f64 + border_width.width;
        let top = rows.origin() as f64 - border_width.height;
        let bottom = (rows.origin() + rows.span()) as f64 + border_width.height;

        let mut fill = |x0: f64, y0: f64, x1: f64, y1: f64| {
            let shape = Rect::new(x0, y0, x1, y1).scale_from_origin(self.scale);
            scene.fill(Fill::NonZero, self.transform, border_color, None, &shape);
        };

        // Horizontal borders: the outer edges, then the gutters between adjacent row tracks.
        // The vertical borders are drawn between them so that no corner is painted twice.
        if outer_border_style.border_top_style != BorderStyle::Hidden {
            fill(left, top, right, top + border_width.height);
        }
        if outer_border_style.border_bottom_style != BorderStyle::Hidden {
            fill(left, bottom - border_width.height, right, bottom);
        }
        for (prev, next) in rows.iter().zip(rows.iter().skip(1)) {
            fill(left, prev.end as f64, right, next.start as f64);
        }

        // Vertical borders, one segment per row
        for row in rows.iter() {
            let (y0, y1) = (row.start as f64, row.end as f64);
            if outer_border_style.border_left_style != BorderStyle::Hidden {
                fill(left, y0, left + border_width.width, y1);
            }
            if outer_border_style.border_right_style != BorderStyle::Hidden {
                fill(right - border_width.width, y0, right, y1);
            }
            for (prev, next) in cols.iter().zip(cols.iter().skip(1)) {
                fill(prev.end as f64, y0, next.start as f64, y1);
            }
        }
    }

    /// ❌ dotted - Defines a dotted border
    /// ❌ dashed - Defines a dashed border
    /// ✅ solid - Defines a solid border
    /// ❌ double - Defines a double border
    /// ❌ groove - Defines a 3D grooved border. The effect depends on the border-color value
    /// ❌ ridge - Defines a 3D ridged border. The effect depends on the border-color value
    /// ❌ inset - Defines a 3D inset border. The effect depends on the border-color value
    /// ❌ outset - Defines a 3D outset border. The effect depends on the border-color value
    /// ✅ none - Defines no border
    /// ✅ hidden - Defines a hidden border
    pub(crate) fn draw_outline(&self, scene: &mut impl PaintScene) {
        let outline = self.style.get_outline();

        let current_color = self.style.clone_color();
        let color = outline
            .outline_color
            .resolve_to_absolute(&current_color)
            .as_srgb_color();

        let style = match outline.outline_style {
            OutlineStyle::Auto => return,
            OutlineStyle::BorderStyle(style) => style,
        };

        let path = match style {
            BorderStyle::None | BorderStyle::Hidden => return,
            BorderStyle::Solid => self.frame.outline(),

            // TODO: Implement other border styles
            BorderStyle::Inset
            | BorderStyle::Groove
            | BorderStyle::Outset
            | BorderStyle::Ridge
            | BorderStyle::Dotted
            | BorderStyle::Dashed
            | BorderStyle::Double => self.frame.outline(),
        };

        scene.fill(Fill::NonZero, self.transform, color, None, &path);
    }
}
