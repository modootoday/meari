//! Enable the dom to lay itself out using taffy
//!
//! In servo, style and layout happen together during traversal
//! However, in Blitz, we do a style pass then a layout pass.
//! This is slower, yes, but happens fast enough that it's not a huge issue.

use crate::node::{ComputedStyleRef, ImageData, NodeData, SpecialElementData};
use crate::{document::BaseDocument, dom_node_id, node::Node, taffy_node_id};
use markup5ever::{LocalName, local_name};
use std::cell::Ref;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use style::Atom;
use style::values::computed::CSSPixelLength;
use style::values::computed::length_percentage::CalcLengthPercentage;
use stylo_taffy::TaffyStyloStyle;
use taffy::{
    AxisStaticEdge, AxisStaticPosition, BlockContext, CoreStyle as _, DetailedLayoutInfo,
    FlexDirection, LayoutContainingBlock, LayoutPartialTree, MaybeMath as _, NodeId, OofCandidate,
    ResolveOrZero, RoundTree, RunMode, TraversePartialTree, TraverseTree, compute_block_layout,
    compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_leaf_layout,
    compute_oof_layout, prelude::*,
};

pub(crate) mod construct;
pub(crate) mod damage;
pub(crate) mod inline;
pub(crate) mod list;
pub(crate) mod paint_tree;
pub(crate) mod replaced;
pub(crate) mod table;
pub(crate) mod text_transform;
#[cfg(feature = "writing-mode")]
pub(crate) mod writing_mode;

use self::replaced::{
    IntrinsicSizes, ReplacedContext, compute_replaced_layout, is_replaced_element,
};
use self::table::TableTreeWrapper;

/// Per-pass layout state: wraps the document for the duration of a layout pass and carries
/// the Taffy tree trait implementations. Derefs to [`BaseDocument`].
pub(crate) struct LayoutPassState<'doc> {
    doc: &'doc mut BaseDocument,
    /// The writing mode of the box whose layout algorithm is currently running (see
    /// `layout::writing_mode`). Child styles read during that algorithm are expressed in its axes.
    #[cfg(feature = "writing-mode")]
    pub(crate) layout_wm: stylo_taffy::WritingMode,
    /// The root element and the writing mode its algorithm runs in (see `BaseDocument::root_layout_wm`).
    #[cfg(feature = "writing-mode")]
    root_id: Option<crate::NodeId>,
    #[cfg(feature = "writing-mode")]
    root_wm: stylo_taffy::WritingMode,
    /// Whether the node whose layout algorithm is running aligns its children's `align-self` in its
    /// inline axis (a column flex container).
    #[cfg(feature = "writing-mode")]
    pub(crate) current_align_axis_is_inline: bool,
    /// The box currently being laid out in an orthogonal flow and its containing block's inline
    /// size, against which its percentage padding resolves (see `layout::writing_mode`).
    #[cfg(feature = "writing-mode")]
    pub(crate) orthogonal_percent_basis: Option<(crate::NodeId, f32)>,
}

pub(crate) struct TaffyDebugTree<'doc>(pub(crate) &'doc BaseDocument);

// pub struct ChildIter<'a>(std::slice::Iter<'a, usize>);
// impl<'a> Iterator for ChildIter<'a> {
//     type Item = NodeId;
//     fn next(&mut self) -> Option<Self::Item> {
//         self.0.next().copied().map(NodeId::from)
//     }
// }

pub struct RefCellChildIter<'a> {
    items: Ref<'a, [crate::NodeId]>,
    idx: usize,
}

mod layout_tree;
pub(crate) use layout_tree::*;
mod taffy_cache;
mod taffy_tree;
