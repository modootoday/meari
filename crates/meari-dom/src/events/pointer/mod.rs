use blitz_traits::node_id::NodeId;
use std::collections::VecDeque;

use web_time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use blitz_traits::{
    events::{
        BlitzInputEvent, BlitzPointerEvent, BlitzPointerId, BlitzWheelDelta, BlitzWheelEvent,
        DomEvent, DomEventData, MouseEventButton, MouseEventButtons,
    },
    navigation::NavigationOptions,
};
use keyboard_types::Modifiers;
use markup5ever::local_name;
use style::values::computed::{Overflow, TouchAction, UserSelect};
use style_dom::ElementState;
use taffy::AbsoluteAxis;

use crate::{
    BaseDocument,
    node::{ScrollbarRef, SpecialElementData},
    scrolling::{FlingState, ScrollAnimationState},
};

use super::focus::generate_focus_events;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PanState {
    pub(crate) target: NodeId,
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
    /// Whether horizontal panning is permitted by the `touch-action` property.
    pub(crate) allow_x: bool,
    /// Whether vertical panning is permitted by the `touch-action` property.
    pub(crate) allow_y: bool,
    pub(crate) samples: VecDeque<PanSample>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PanSample {
    pub(crate) time: u64,
    pub(crate) dx: f32,
    pub(crate) dy: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScrollbarDragState {
    /// The thumb being dragged
    pub(crate) scrollbar: ScrollbarRef,
    /// Last pointer position along the drag axis, in page coordinates
    pub(crate) last_pos: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DragMode {
    /// We are not currently dragging
    None,
    /// We are currently dragging a selection (probably mouse)
    Selecting,
    /// We are currently panning the document with a drag (probably touch)
    Panning(PanState),
    /// We are currently dragging a scrollbar thumb
    ScrollbarDrag(ScrollbarDragState),
}

mod pointer_motion;
pub(crate) use pointer_motion::*;
mod pointer_release;
pub(crate) use pointer_release::*;
