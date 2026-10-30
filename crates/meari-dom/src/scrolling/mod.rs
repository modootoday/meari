//! Scrolling: user-initiated (interactive) and programmatic scrolling of nodes and the
//! viewport, and the scroll animations (smooth scrolls and flings) which drive them.

use blitz_traits::events::{BlitzScrollEvent, DomEvent, DomEventData};
use blitz_traits::node_id::NodeId;
use style::values::computed::Overflow;
use web_time::{SystemTime, UNIX_EPOCH};

use crate::BaseDocument;
use crate::util::Point;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollBehavior {
    #[default]
    Auto,
    Instant,
    Smooth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollLogicalPosition {
    Start,
    Center,
    End,
    Nearest,
}

/// What a scroll applies to.
///
/// Per the CSS overflow propagation rules the root element has no scrolling mechanism of its
/// own (its overflow is applied to the viewport), so [`ScrollTarget::Node`] holding the root
/// element is equivalent to [`ScrollTarget::Viewport`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollTarget {
    Node(NodeId),
    Viewport,
}

/// How far to scroll.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ScrollAmount {
    /// An absolute scroll offset.
    To(Point<f64>),
    /// A delta to apply to the current scroll offset.
    By(Point<f64>),
}

/// What to do with scroll which the target cannot consume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollOverflow {
    /// Discard it: the scroll affects exactly one scroller.
    Clamp,
    /// Transfer it to the next scroller in the scroll chain (the parent node, and eventually
    /// the viewport).
    Chain,
}

/// Who initiated a scroll. Determines which overflow values count as scrollable:
/// `overflow: hidden` boxes have a scrolling box which programmatic scrolls apply to,
/// but no user scrolling mechanism.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollSource {
    /// A user-initiated scroll (wheel, touch, scrollbar drag, keyboard).
    User,
    /// A script-initiated scroll (`scrollTo` and friends, fragment navigation).
    Programmatic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrollRequest {
    pub(crate) target: ScrollTarget,
    pub(crate) amount: ScrollAmount,
    pub(crate) overflow: ScrollOverflow,
    pub(crate) source: ScrollSource,
    pub(crate) behavior: ScrollBehavior,
    /// Whether to abort a smooth scroll in progress. Set for user-initiated scrolls, so that
    /// an animation does not fight the user's input for the rest of its duration.
    pub(crate) interrupt_animation: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FlingState {
    pub(crate) target: NodeId,
    pub(crate) last_seen_time: f64,
    pub(crate) x_velocity: f64,
    pub(crate) y_velocity: f64,
}

/// State driving a smooth (animated) scroll towards a target offset. Used for
/// fragment navigation (`#anchor` links) and programmatic smooth scrolling.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScrollToState {
    /// What is being scrolled.
    pub(crate) target: ScrollTarget,
    /// The scroll offset at the start of the animation.
    pub(crate) start: Point<f64>,
    /// The scroll offset to animate towards.
    pub(crate) end: Point<f64>,
    /// Time (in milliseconds since the Unix epoch) at which the animation started.
    pub(crate) start_time: f64,
    /// Total duration of the animation in milliseconds.
    pub(crate) duration: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ScrollAnimationState {
    None,
    Fling(FlingState),
    /// A smooth scroll of the viewport towards a target offset.
    ScrollTo(ScrollToState),
}

mod scroll_ops;
use scroll_ops::*;
mod scroll_into_view;
