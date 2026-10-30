//! Enable the dom to participate in styling by servo
//!

use blitz_traits::node_id::NodeId;
use std::ptr::NonNull;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use crate::StyleThreading;
use crate::layout::damage::compute_layout_damage;
use crate::node::Node;
use crate::node::NodeData;
use markup5ever::{LocalName, LocalNameStaticSet, Namespace, NamespaceStaticSet, local_name, ns};
use selectors::bloom::BLOOM_HASH_MASK;
use selectors::{
    Element, OpaqueElement,
    attr::{AttrSelectorOperation, NamespaceConstraint},
    matching::{ElementSelectorFlags, MatchingContext, VisitedHandlingMode},
    sink::Push,
};
use style::CaseSensitivityExt;
use style::animation::AnimationSetKey;
use style::animation::AnimationState;
use style::applicable_declarations::ApplicableDeclarationBlock;
use style::bloom::each_relevant_element_hash;
use style::color::AbsoluteColor;
use style::data::{ElementDataMut, ElementDataRef};
use style::global_style_data::STYLE_THREAD_POOL;
use style::invalidation::element::restyle_hints::RestyleHint;
use style::properties::ComputedValues;
use style::properties::{Importance, PropertyDeclaration};
use style::rule_tree::CascadeLevel;
use style::rule_tree::CascadeOrigin;
use style::selector_parser::PseudoElement;
use style::selector_parser::RestyleDamage;
use style::stylesheets::layer_rule::LayerOrder;
use style::stylesheets::scope_rule::ImplicitScopeRoot;
use style::values::AtomString;
use style::values::specified::{NoCalcPercentage, XLang};
use style::{
    Atom,
    context::{
        QuirksMode, RegisteredSpeculativePainter, RegisteredSpeculativePainters,
        SharedStyleContext, StyleContext, ThreadLocalStyleContext,
    },
    dom::{LayoutIterator, NodeInfo, OpaqueNode, TDocument, TElement, TNode, TShadowRoot},
    global_style_data::GLOBAL_STYLE_DATA,
    properties::PropertyDeclarationBlock,
    selector_parser::{NonTSPseudoClass, SelectorImpl},
    servo_arc::{Arc, ArcBorrow},
    shared_lock::{Locked, SharedRwLock, StylesheetGuards},
    stylist::RuleInclusion,
    thread_state::ThreadState,
    traversal::{DomTraversal, resolve_style},
    traversal_flags::TraversalFlags,
    values::{AtomIdent, GenericAtomIdent},
};
use style_dom::ElementState;

use style::values::computed::text::TextAlign as StyloTextAlign;

/// A handle to a node that Servo's style traits are implemented against
///
/// Since BlitzNodes are not persistent (IE we don't keep the pointers around between frames), we choose to just implement
/// the tree structure in the nodes themselves, and temporarily give out pointers during the layout phase.
type BlitzNode<'a> = &'a Node;

pub struct Traverser<'a> {
    // dom: &'a Slab<Node>,
    parent: BlitzNode<'a>,
    child_index: usize,
}

/// Handle custom painters like images for layouting
///
/// todo: actually implement this
pub struct RegisteredPaintersImpl;

use style::traversal::recalc_style_at;

pub struct RecalcStyle<'a> {
    context: SharedStyleContext<'a>,
    /// Nodes whose `background-image`/`mask-image` layers need flushing to
    /// dedicated storage on the node (see `flush_image_layers_from_style`)
    /// because their style changed during this traversal.
    nodes_needing_style_image_flush: Mutex<Vec<NodeId>>,
}

mod document_styling;
mod element_traversal;
mod legacy_hints;
mod selector_matching;
use legacy_hints::*;
