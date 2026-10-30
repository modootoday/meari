use crate::Document;
use crate::layout::paint_tree::{HoistedPaintChild, StackingContext};
use bitflags::bitflags;
use blitz_traits::events::{
    BlitzPointerEvent, BlitzPointerId, DomEventData, HitResult, PointerCoords,
};
use blitz_traits::node_id::NodeId;
use blitz_traits::shell::ShellProvider;
use euclid::{Point2D, Rect, Size2D};
use keyboard_types::Modifiers;
use kurbo::{Affine, Rect as KurboRect};
use markup5ever::{LocalName, local_name};
use parley::{BreakReason, Cluster, ClusterSide};
use selectors::matching::ElementSelectorFlags;
use std::cell::{Cell, RefCell};
use std::fmt::Write;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "writing-mode")]
use style::computed_values::flex_direction::T as FlexDirection;
use style::computed_values::isolation::T as Isolation;
use style::computed_values::white_space_collapse::T as WhiteSpaceCollapse;
use style::invalidation::element::restyle_hints::RestyleHint;
use style::properties::ComputedValues;
use style::properties::generated::longhands::position::computed_value::T as Position;
use style::selector_parser::RestyleDamage;
use style::servo_arc::Arc as ServoArc;
use style::shared_lock::SharedRwLock;
use style::stylesheets::UrlExtraData;
use style::values::computed::CSSPixelLength;
use style::values::computed::Display as StyloDisplay;
use style::values::computed::Rotate;
use style::values::generics::transform::{Scale, Translate};
use style::values::specified::box_::{DisplayInside, DisplayOutside};
use style_dom::ElementState;
use style_traits::values::ToCss;
use taffy::{Cache, prelude::Layout};
use thin_vec::ThinVec;

use super::stylo_data::{ComputedStyleRef, StyloData};
use super::{Attribute, DocumentData, ElementData, LayoutData};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayOuter {
    Block,
    Inline,
    None,
}

pub struct Node {
    // The actual tree we belong to. This is unsafe!!
    tree: *mut crate::NodeTree,

    /// Our Id
    pub id: NodeId,
    /// Our parent's ID
    pub parent: Option<NodeId>,
    // What are our children?
    pub children: ThinVec<NodeId>,
    /// Our parent in the layout hierachy: a separate list that includes anonymous collections of inline elements
    pub layout_parent: Cell<Option<NodeId>>,
    /// A separate child list that includes anonymous collections of inline elements
    pub layout_children: RefCell<Option<ThinVec<NodeId>>>,
    /// Out-of-flow (absolutely/fixed positioned) boxes for which this node is the
    /// containing block. Recorded by Taffy's out-of-flow positioning pass. The
    /// `Layout.location` of these boxes is relative to this node's border box.
    pub hoisted_children: RefCell<ThinVec<NodeId>>,
    /// For an out-of-flow box that was hoisted by Taffy's out-of-flow positioning
    /// pass, the node whose `hoisted_children` list contains it (its containing
    /// block). The box's `Layout.location` is relative to this node rather than
    /// to its `layout_parent`. `None` for in-flow boxes.
    pub oof_containing_block: Cell<Option<NodeId>>,
    /// Anonymous block boxes created for this node during layout construction.
    ///
    /// Anonymous blocks live only in the slab (they are not part of the DOM
    /// `children` list), so we track the ones we own here to be able to
    /// deallocate them when this node is reconstructed.
    pub anonymous_blocks: ThinVec<NodeId>,
    /// The same as layout_children, but sorted by z-index
    pub paint_children: RefCell<Option<ThinVec<NodeId>>>,
    pub stacking_context: Option<Box<StackingContext>>,
    /// The hoisted entries this node's subtree pushed into the enclosing
    /// stacking context on its last (non-skipped) paint-tree build. Replayed
    /// when the subtree is skipped as clean. Empty for stacking-context roots.
    pub sc_contribution_cache: RefCell<ThinVec<HoistedPaintChild>>,

    // Flags
    pub flags: NodeFlags,

    /// Node type (Element, TextNode, etc) specific data.
    ///
    /// For element nodes this holds the [`ElementData`], which stores most of
    /// the per-node style/layout state. For the document node it holds the
    /// [`DocumentData`]. Access the moved fields through the forwarding methods
    /// on [`Node`] (e.g. [`Node::style`], [`Node::final_layout`]).
    pub data: NodeData,
}

/// Generates forwarding accessors for fields that live on both [`ElementData`]
/// (element / anonymous block nodes) and [`DocumentData`] (the document node).
macro_rules! universal_accessors {
    ($($(#[$meta:meta])* $field:ident / $field_mut:ident : $ty:ty),* $(,)?) => {
        impl Node {
            $(
                $(#[$meta])*
                #[inline]
                pub fn $field(&self) -> &$ty {
                    match &self.data {
                        NodeData::Element(data) | NodeData::AnonymousBlock(data) => &data.$field,
                        NodeData::Document(data) => &data.$field,
                        _ => panic!(concat!("`", stringify!($field), "` is not available on this node kind")),
                    }
                }

                $(#[$meta])*
                #[inline]
                pub fn $field_mut(&mut self) -> &mut $ty {
                    match &mut self.data {
                        NodeData::Element(data) | NodeData::AnonymousBlock(data) => &mut data.$field,
                        NodeData::Document(data) => &mut data.$field,
                        _ => panic!(concat!("`", stringify!($field), "` is not available on this node kind")),
                    }
                }
            )*
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeKind {
    Document,
    Element,
    AnonymousBlock,
    Text,
    Comment,
}

/// The different kinds of nodes in the DOM.
#[derive(Debug, Clone)]
pub enum NodeData {
    /// The `Document` itself - the root node of a HTML document.
    Document(Box<DocumentData>),

    /// An element with attributes.
    Element(Box<ElementData>),

    /// An anonymous block box
    AnonymousBlock(Box<ElementData>),

    /// A text node.
    Text(TextNodeData),

    /// A comment.
    Comment {
        /// The textual content of the comment
        contents: String,
    },
    // /// A `DOCTYPE` with name, public id, and system id. See
    // /// [document type declaration on wikipedia][https://en.wikipedia.org/wiki/Document_type_declaration]
    // Doctype { name: String, public_id: String, system_id: String },

    // /// A Processing instruction.
    // ProcessingInstruction { target: String, contents: String },
}

#[derive(Debug, Clone)]
pub struct TextNodeData {
    /// The textual content of the text node
    pub content: String,
}

#[cfg(test)]
#[path = "tests/test.rs"]
mod test;

mod layout_state;
pub use layout_state::*;
mod geometry;
mod hit_testing;
mod interaction_state;
