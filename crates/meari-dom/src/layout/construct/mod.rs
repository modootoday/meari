use blitz_traits::node_id::NodeId;
use core::str;
use std::sync::Arc;

use markup5ever::{QualName, local_name, ns};
use parley::{
    FontContext, InlineBox, InlineBoxKind, LayoutContext, StyleProperty, TreeBuilder,
    WhiteSpaceCollapse,
};
use style::{
    computed_values::position::T as PositionProperty,
    data::ElementData as StyloElementData,
    shared_lock::StylesheetGuards,
    values::{
        computed::{Content, ContentItem, Display, Float},
        specified::box_::{DisplayInside, DisplayOutside},
    },
};
use thin_vec::ThinVec;

use crate::{
    BaseDocument, ElementData, Node, NodeData,
    layout::damage::{
        CONSTRUCT_BOX, CONSTRUCT_DESCENDENT, CONSTRUCT_FC, sort_layout_children_by_order,
    },
    node::{
        ListItemLayout, ListItemLayoutPosition, Marker, NodeFlags, NodeKind, SpecialElementData,
        TextBrush, TextInputData, TextLayout,
    },
    qual_name, stylo_to_parley,
    traversal::{iter_children, iter_children_and_pseudos},
};

use super::{
    damage::ALL_DAMAGE,
    list::{BULLET_FONT_FAMILY, collect_list_item_children},
    replaced::is_inline_box_element,
    table::build_table_context,
    text_transform::{CaseTransform, TextTransformer},
};

#[derive(Clone)]
pub(crate) struct ConstructionTask {
    pub(crate) node_id: NodeId,
    pub(crate) data: ConstructionTaskData,
}

pub(crate) struct ConstructionTaskResult {
    pub(crate) node_id: NodeId,
    pub(crate) data: ConstructionTaskResultData,
}

#[derive(Clone)]
pub(crate) enum ConstructionTaskData {
    InlineLayout(Box<TextLayout>),
}

pub(crate) enum ConstructionTaskResultData {
    InlineLayout(Box<TextLayout>),
}

/// Accumulator threaded through layout-child collection.
///
/// `children` is the list of layout children being built up, and
/// `anonymous_block_id` is the currently-open anonymous block container (if
/// any) that wrapping children are being appended to.
#[derive(Default)]
pub(crate) struct LayoutChildren {
    pub(crate) children: ThinVec<NodeId>,
    pub(crate) anonymous_block_id: Option<NodeId>,
    /// All anonymous blocks created while collecting these layout children.
    ///
    /// These are recorded on the container node so they can be deallocated the
    /// next time it is reconstructed.
    pub(crate) anonymous_blocks: ThinVec<NodeId>,
}

/// Wrapping policy of the nearest non-contents ancestor container. Children
/// hoisted out of display:contents nodes must be wrapped in anonymous blocks
/// exactly as if they were direct children of that container: without this,
/// a bare text node could end up as the layout child of a block/flex/grid
/// container, which cannot be laid out (text nodes carry no style).
#[derive(Copy, Clone)]
struct WrapContext {
    /// The container whose style anonymous blocks inherit from.
    container_node_id: NodeId,
    needs_wrap: fn(NodeKind, DisplayOutside) -> bool,
}

/// Result of classifying the in-flow children of a flow container as
/// all-block, all-inline and/or all-out-of-flow.
struct FlowClassification {
    all_block: bool,
    all_inline: bool,
    all_out_of_flow: bool,
    has_contents: bool,
}

mod child_collection;
pub(crate) use child_collection::*;
mod complex_children;
use complex_children::*;
mod inline_boxes;
pub(crate) use inline_boxes::*;
