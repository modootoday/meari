//! Bounded metadata from the actual checked Scene traversal, never an OS presentation ACK.
use kurbo::{Affine, Rect, Shape};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
#[derive(Clone, Copy, Debug)]
pub(crate) enum PrimitiveKind {
    Glyph,
    Stroke,
    Shadow,
    Other,
}
static UNKNOWN_DIAGNOSTICS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub(crate) type Shared = Rc<RefCell<Collector>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlap {
    Clear,
    Overlap,
    Unknown,
}
#[derive(Clone, Debug)]
pub struct SurfacePaintNode {
    pub node: u64,
    pub generation: u64,
    pub physical_content_rect: Option<Rect>,
    pub rectangular_clip: Option<Rect>,
    pub painted: bool,
    pub unsupported: bool,
    pub later_overlap: Overlap,
    pub focusable: Option<bool>,
    pub paint_ordinal: u64,
}
#[derive(Clone, Debug)]
pub struct SurfacePaintTrace {
    pub document_id: usize,
    pub focused_node: Option<u64>,
    pub targets: BTreeMap<u64, SurfacePaintNode>,
    pub skipped_layer: bool,
    /// Draw primitives and external-surface markers, not the Scene command count.
    pub draw_events: u64,
}
#[derive(Clone, Copy)]
struct Clip {
    rect: Option<Rect>,
    unsupported: bool,
}
pub(crate) struct Collector {
    trace: SurfacePaintTrace,
    clips: Vec<Clip>,
    nodes: Vec<(u64, bool)>,
    failed: bool,
    diagnostics: u8,
}
impl Collector {
    pub fn new(
        document_id: usize,
        focused_node: Option<u64>,
        targets: &[(u64, u64)],
        viewport: Rect,
    ) -> Self {
        let mut entries = BTreeMap::new();
        for &(node, generation) in targets.iter().take(64) {
            entries.insert(
                node,
                SurfacePaintNode {
                    node,
                    generation,
                    physical_content_rect: None,
                    rectangular_clip: None,
                    painted: false,
                    unsupported: false,
                    later_overlap: Overlap::Clear,
                    focusable: None,
                    paint_ordinal: 0,
                },
            );
        }
        Self {
            trace: SurfacePaintTrace {
                document_id,
                focused_node,
                targets: entries,
                skipped_layer: false,
                draw_events: 0,
            },
            clips: vec![Clip {
                rect: Some(viewport),
                unsupported: false,
            }],
            nodes: Vec::new(),
            failed: targets.len() > 64,
            diagnostics: 0,
        }
    }
    pub fn enter(shared: &Shared, node: u64) -> NodeGuard {
        let mut collector = shared.borrow_mut();
        let pushed = collector.nodes.len() < 256;
        if pushed {
            let inherited = collector.nodes.last().is_some_and(|(_, bad)| *bad);
            collector.nodes.push((node, inherited));
        } else {
            collector.failed = true;
        }
        NodeGuard {
            shared: shared.clone(),
            pushed,
        }
    }
    pub fn unsupported(&mut self) {
        if let Some((_, unsupported)) = self.nodes.last_mut() {
            *unsupported = true;
        }
    }
    pub fn abort(&mut self) {
        self.failed = true;
    }
    pub fn layer_skipped(&mut self) {
        self.trace.skipped_layer = true;
    }
    pub fn push(&mut self, transform: Affine, shape: &impl Shape, unsupported: bool) {
        if self.clips.len() >= 2048 {
            self.failed = true;
            return;
        }
        let parent = *self.clips.last().expect("viewport clip");
        let rectangular = axis_aligned(transform)
            .then(|| exact_rect(shape))
            .flatten()
            .map(|rect| transform.transform_rect_bbox(rect));
        self.clips.push(Clip {
            rect: parent.rect.zip(rectangular).map(|(a, b)| a.intersect(b)),
            unsupported: parent.unsupported || unsupported || rectangular.is_none(),
        });
    }
    pub fn pop(&mut self) {
        if self.clips.len() > 1 {
            self.clips.pop();
        } else {
            self.failed = true;
        }
    }
    pub fn target(&mut self, node: u64, transform: Affine, content: Rect, focusable: bool) {
        let inherited = self.nodes.iter().any(|(_, bad)| *bad);
        let clip = *self.clips.last().expect("viewport clip");
        let rectangle = transform.transform_rect_bbox(content);
        if [rectangle.x0, rectangle.y0, rectangle.x1, rectangle.y1]
            .into_iter()
            .any(|value| !value.is_finite())
        {
            self.failed = true;
            return;
        }
        let potentially_painted = rectangle.area() > 0.0
            && clip
                .rect
                .is_none_or(|clip| clip.intersect(rectangle).area() > 0.0);
        if self.trace.targets.contains_key(&node) && potentially_painted {
            self.draw(Some(rectangle));
        }
        if let Some(target) = self.trace.targets.get_mut(&node) {
            let rect = rectangle;
            target.physical_content_rect = Some(rect);
            target.rectangular_clip = clip.rect.map(|clip| clip.intersect(rect));
            target.unsupported = clip.unsupported || inherited || !axis_aligned(transform);
            target.focusable = Some(focusable);
            target.painted = potentially_painted;
            target.paint_ordinal = self.trace.draw_events;
        }
    }
    pub fn draw(&mut self, extent: Option<Rect>) {
        self.draw_kind(extent, PrimitiveKind::Other);
    }
    pub fn draw_kind(&mut self, extent: Option<Rect>, primitive: PrimitiveKind) {
        self.trace.draw_events = match self.trace.draw_events.checked_add(1) {
            Some(count) if count <= 100_000 => count,
            _ => {
                self.failed = true;
                return;
            }
        };
        let owner = self.nodes.last().map(|(node, _)| *node);
        let uncertain = self.nodes.last().is_some_and(|(_, bad)| *bad)
            || self.clips.last().is_some_and(|clip| clip.unsupported);
        // A real pure rectangular clip bounds every draw until it is popped, including
        // primitives without measured ink bounds. It proves separation only, not overlap.
        let unknown_bound = if extent.is_none() || uncertain {
            self.clips
                .iter()
                .rev()
                .filter(|clip| !clip.unsupported)
                .find_map(|clip| clip.rect)
                .filter(|rect| {
                    [rect.x0, rect.y0, rect.x1, rect.y1]
                        .into_iter()
                        .all(f64::is_finite)
                        && rect.width() >= 0.0
                        && rect.height() >= 0.0
                })
        } else {
            None
        };
        let extent = if uncertain {
            None
        } else {
            extent.filter(|rect| {
                [rect.x0, rect.y0, rect.x1, rect.y1]
                    .into_iter()
                    .all(f64::is_finite)
            })
        };
        let current_clip = self.clips.last().map(|clip| (clip.rect, clip.unsupported));
        let mut diagnostics = self.diagnostics;
        for target in self
            .trace
            .targets
            .values_mut()
            .filter(|target| target.painted && Some(target.node) != owner)
        {
            match (extent, target.rectangular_clip) {
                (None, Some(clip))
                    if unknown_bound.is_some_and(|bound| bound.intersect(clip).area() <= 0.0) => {}
                (Some(extent), Some(clip)) if extent.intersect(clip).area() <= 0.0 => {}
                (Some(_), Some(_)) => target.later_overlap = Overlap::Overlap,
                _ if target.later_overlap != Overlap::Overlap => {
                    if diagnostics < 8 {
                        use std::{io::Write, sync::atomic::Ordering};
                        if UNKNOWN_DIAGNOSTICS
                            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                                (count < 16).then_some(count + 1)
                            })
                            .is_ok()
                        {
                            let _ = writeln!(
                                std::io::stderr().lock(),
                                "meari-paint-unknown-blocker kind={primitive:?} owner={owner:?} target={} clip={current_clip:?} bound={unknown_bound:?} uncertain={uncertain}",
                                target.node
                            );
                        }
                        diagnostics += 1;
                    }
                    target.later_overlap = Overlap::Unknown
                }
                _ => {}
            }
        }
        self.diagnostics = diagnostics;
    }
    pub fn finish(&self) -> Option<SurfacePaintTrace> {
        if self.failed || !self.nodes.is_empty() || self.clips.len() != 1 {
            return None;
        }
        let mut trace = self.trace.clone();
        if trace.skipped_layer {
            for target in trace.targets.values_mut() {
                target.unsupported = true;
                target.rectangular_clip = None;
            }
        }
        Some(trace)
    }
}
pub(crate) fn axis_aligned(transform: Affine) -> bool {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    [a, b, c, d, e, f].into_iter().all(f64::is_finite) && a > 0.0 && d > 0.0 && b == 0.0 && c == 0.0
}
fn exact_rect(shape: &impl Shape) -> Option<Rect> {
    if let Some(rect) = shape.as_rect() {
        return (rect.width() >= 0.0 && rect.height() >= 0.0).then_some(rect);
    }
    use kurbo::PathEl;
    let path = shape.path_elements(0.1).take(6).collect::<Vec<_>>();
    // Filled clip subpaths close implicitly; CssBox emits four sharp corners without Z.
    let points = match path.as_slice() {
        [
            PathEl::MoveTo(a),
            PathEl::LineTo(b),
            PathEl::LineTo(c),
            PathEl::LineTo(d),
        ]
        | [
            PathEl::MoveTo(a),
            PathEl::LineTo(b),
            PathEl::LineTo(c),
            PathEl::LineTo(d),
            PathEl::ClosePath,
        ] => [*a, *b, *c, *d],
        _ => return None,
    };
    if points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return None;
    }
    for i in 0..4 {
        for j in 0..i {
            if points[i] == points[j] {
                return None;
            }
        }
    }
    let rect = shape.bounding_box();
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    for index in 0..4 {
        let p = points[index];
        let q = points[(index + 1) % 4];
        if (p.x != rect.x0 && p.x != rect.x1)
            || (p.y != rect.y0 && p.y != rect.y1)
            || (p.x == q.x) == (p.y == q.y)
        {
            return None;
        }
    }
    Some(rect)
}
pub(crate) struct NodeGuard {
    shared: Shared,
    pushed: bool,
}
impl Drop for NodeGuard {
    fn drop(&mut self) {
        if self.pushed {
            self.shared.borrow_mut().nodes.pop();
        }
    }
}
#[cfg(test)]
#[path = "surface_observation/tests.rs"]
mod tests;
