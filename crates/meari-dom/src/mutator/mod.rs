use blitz_traits::node_id::NodeId;
use std::collections::HashSet;
use std::mem;
use std::ops::{Deref, DerefMut};

use crate::layout::damage::ALL_DAMAGE;
use crate::net::{ImageHandler, ResourceHandler, StylesheetHandler};
use crate::node::{CanvasData, NodeFlags, SpecialElementData};
use crate::stylo_device::DeviceChanges;
use crate::util::ImageType;
use crate::{
    Attribute, BaseDocument, Document, ElementData, Node, NodeData, QualName, local_name, qual_name,
};
use blitz_traits::shell::Viewport;
use style::Atom;
use style::invalidation::element::restyle_hints::RestyleHint;
use style::stylesheets::OriginSet;
use thin_vec::ThinVec;

macro_rules! tag_and_attr {
    ($tag:tt, $attr:tt) => {
        (&local_name!($tag), &local_name!($attr))
    };
}

#[derive(Debug, Clone)]
pub enum AppendTextErr {
    /// The node is not a text node
    NotTextNode,
}

/// Operations that happen almost immediately, but are deferred within a
/// function for borrow-checker reasons.
enum SpecialOp {
    LoadImage(NodeId),
    LoadIframe(NodeId),
    LoadStylesheet(NodeId),
    UnloadStylesheet(NodeId),
    LoadCustomPaintSource(NodeId),
    ProcessButtonInput(NodeId),
    UnloadSubDocument(NodeId),
    #[cfg(feature = "custom-widget")]
    UnloadCustomWidget(NodeId),
}

pub struct DocumentMutator<'doc> {
    /// Document is public as an escape hatch, but users of this API should ideally avoid using it
    /// and prefer exposing additional functionality in DocumentMutator.
    pub doc: &'doc mut BaseDocument,

    eager_op_queue: Vec<SpecialOp>,

    // Tracked nodes for deferred processing when mutations have completed
    title_node: Option<NodeId>,
    style_nodes: HashSet<NodeId>,
    form_nodes: HashSet<NodeId>,

    /// Whether an element/attribute that affect animation status has been seen
    recompute_is_animating: bool,

    /// Whether any mutation that affects rendered output has been performed
    mutations_occurred: bool,

    /// The (latest) node which has been mounted in and had autofocus=true, if any
    #[cfg(feature = "autofocus")]
    node_to_autofocus: Option<NodeId>,
}

/// Type that allows mutable access to the viewport
/// And syncs it back to stylist on drop.
pub struct ViewportMut<'doc> {
    doc: &'doc mut BaseDocument,
    initial_viewport: Viewport,
}

#[cfg(test)]
#[path = "tests/test.rs"]
mod test;

mod node_creation;
mod resource_loading;
mod tree_edits;
use resource_loading::*;
