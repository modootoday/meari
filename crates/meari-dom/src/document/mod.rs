use crate::NodeTree;
use crate::events::{DragMode, handle_dom_event};
use crate::layout::construct::ConstructionTask;
use crate::layout::damage::ALL_DAMAGE;
use crate::mutator::ViewportMut;
use crate::net::{
    Resource, ResourceHandler, ResourceLoadResponse, StylesheetHandler, StylesheetLoader,
};
use crate::node::{ImageData, NodeFlags, RasterImageData, SpecialElementData, Status, TextBrush};
use crate::scrolling::ScrollAnimationState;
use crate::selection::TextSelection;
use crate::stylo_device::{DeviceChanges, make_device};
use crate::stylo_to_cursor_icon::stylo_to_cursor_icon;
use crate::traversal::TreeTraverser;
use crate::url::DocumentUrl;
use crate::util::ImageType;
use crate::{
    DEFAULT_CSS, DocumentConfig, DocumentMutator, DummyHtmlParserProvider, ElementData,
    EventDriver, HtmlParserProvider, Node, NodeData, NoopEventHandler, StyleThreading,
    TextNodeData,
};
use blitz_traits::devtools::DevtoolSettings;
use blitz_traits::events::{DomEvent, HitResult, UiEvent};
use blitz_traits::navigation::{DummyNavigationProvider, NavigationProvider};
use blitz_traits::net::{AbortSignal, DummyNetProvider, NetProvider, Request};
use blitz_traits::node_id::NodeId;
use blitz_traits::shell::{DummyShellProvider, ShellProvider, Viewport};
use cursor_icon::CursorIcon;
use linebender_resource_handle::Blob;
use markup5ever::{LocalName, local_name};
use parley::{FontContext, PlainEditorDriver};
use selectors::{Element, matching::QuirksMode};
use smallvec::SmallVec;
use std::any::Any;
use std::cell::RefCell;
use std::collections::{BTreeMap, Bound, HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock, RwLockReadGuard, RwLockWriteGuard};
use std::task::{Context as TaskContext, Waker};
use style::Atom;
use style::animation::DocumentAnimationSet;
use style::attr::{AttrIdentifier, AttrValue};
use style::computed_value_flags::ComputedValueFlags;
use style::data::{ElementData as StyloElementData, ElementStyles};
use style::invalidation::element::restyle_hints::RestyleHint;
use style::media_queries::MediaType;
use style::properties::ComputedValues;
use style::properties::style_structs::Font;
use style::selector_parser::ServoElementSnapshot;
use style::servo_arc::Arc as ServoArc;
use style::values::GenericAtomIdent;
use style::values::computed::UserSelect;
use style::values::computed::ui::CursorKind;
use style::{
    device::Device,
    dom::{TDocument, TNode},
    media_queries::MediaList,
    selector_parser::SnapshotMap,
    shared_lock::{SharedRwLock, StylesheetGuards},
    stylesheets::{AllowImportRules, DocumentStyleSheet, Origin, Stylesheet},
    stylist::Stylist,
};
use style_dom::ElementState;
use thin_vec::ThinVec;
use url::Url;
use web_time::Instant;

#[cfg(feature = "parallel-construct")]
use thread_local::ThreadLocal;

/// A user-agent stylesheet, parsed with a read-only lock so that it can be shared between documents.
///
/// Documents with the same user-agent stylesheets (in the same order) also share Stylo's
/// cached user-agent cascade data. `@import` rules are ignored.
#[derive(Clone, Debug, PartialEq)]
pub struct UserAgentStylesheet(DocumentStyleSheet);

pub enum DocGuard<'a> {
    Ref(&'a BaseDocument),
    RefCell(std::cell::Ref<'a, BaseDocument>),
    RwLock(RwLockReadGuard<'a, BaseDocument>),
    Mutex(MutexGuard<'a, BaseDocument>),
}

pub enum DocGuardMut<'a> {
    Ref(&'a mut BaseDocument),
    RefCell(std::cell::RefMut<'a, BaseDocument>),
    RwLock(RwLockWriteGuard<'a, BaseDocument>),
    Mutex(MutexGuard<'a, BaseDocument>),
}

/// Abstraction over wrappers around [`BaseDocument`] to allow for them all to
/// be driven by [`blitz-shell`](https://docs.rs/blitz-shell)
pub trait Document: Any + 'static {
    fn inner(&self) -> DocGuard<'_>;
    fn inner_mut(&mut self) -> DocGuardMut<'_>;

    /// Update the [`Document`] in response to a [`UiEvent`] (click, keypress, etc)
    fn handle_ui_event(&mut self, event: UiEvent) {
        let mut doc = self.inner_mut();
        let mut driver = EventDriver::new(&mut *doc, NoopEventHandler);
        driver.handle_ui_event(event);
    }

    /// Poll any pending async operations, and flush changes to the underlying [`BaseDocument`]
    fn poll(&mut self, task_context: Option<TaskContext>) -> bool {
        // Default implementation does nothing
        let _ = task_context;
        false
    }

    /// Get the [`Document`]'s id
    fn id(&self) -> usize {
        self.inner().id
    }
}

pub struct PlainDocument(pub BaseDocument);

pub enum DocumentEvent {
    ResourceLoad(ResourceLoadResponse),
    /// A navigation originating from within an iframe's sub-document
    /// (e.g. a link click), to be applied to the iframe identified by `node_id`.
    NavigateIframe {
        node_id: NodeId,
        url: Url,
    },
}

pub struct BaseDocument {
    /// ID of the document
    id: usize,

    // Config
    /// Base url for resolving linked resources (stylesheets, images, fonts, etc)
    pub(crate) url: DocumentUrl,
    // Devtool settings. Currently used to render debug overlays
    pub(crate) devtool_settings: DevtoolSettings,
    // Viewport details such as the dimensions, HiDPI scale, and zoom factor,
    pub(crate) viewport: Viewport,
    /// CSS media type used to evaluate `@media` rules.
    pub(crate) media_type: MediaType,
    /// Changes to the stylist [`Device`] that have been requested since the
    /// last [`resolve`](Self::resolve), to be applied (coalesced into a single
    /// device rebuild) at the start of the next resolve.
    pub(crate) pending_device_changes: DeviceChanges,
    /// Strategy for Stylo's style traversal during `resolve`.
    pub(crate) style_threading: StyleThreading,
    /// Whether incremental layout is enabled for this document. When disabled,
    /// every node is treated as damaged on every `resolve`, so the same damage
    /// pipeline reconstructs and relays out the whole tree.
    pub(crate) incremental_layout: bool,
    /// How deeply this document is nested within other documents
    /// (0 for a root document). Used to limit `<iframe>` nesting depth.
    pub(crate) subdocument_depth: usize,

    // Events
    pub(crate) tx: Sender<DocumentEvent>,
    // rx will always be Some, except temporarily while processing events
    pub(crate) rx: Option<Receiver<DocumentEvent>>,

    /// A slotmap-backed tree of nodes
    ///
    /// We pin the tree to a guarantee to the nodes it creates that the tree is stable in memory.
    /// There is no way to create the tree - publicly or privately - that would invalidate that invariant.
    pub(crate) nodes: Box<NodeTree>,

    /// The id of the root node (a Document node)
    pub(crate) root_node_id: NodeId,

    // Stylo
    /// The Stylo engine
    pub(crate) stylist: Stylist,
    pub(crate) animations: DocumentAnimationSet,
    /// Stylo shared lock
    pub(crate) guard: SharedRwLock,
    /// Stylo invalidation map. We insert into this map prior to mutating nodes.
    pub(crate) snapshots: SnapshotMap,

    // Parley contexts
    /// A Parley font context
    pub(crate) font_ctx: Arc<Mutex<parley::FontContext>>,
    #[cfg(feature = "parallel-construct")]
    /// Thread-and-document-local copies to the font context
    pub(crate) thread_font_contexts: ThreadLocal<RefCell<Box<FontContext>>>,
    /// A Parley layout context
    pub(crate) layout_ctx: parley::LayoutContext<TextBrush>,

    /// The real (non-anonymous) node which is currently hovered (if any).
    /// This is never a layout-generated (anonymous) node, so it remains valid
    /// across box-tree reconstruction.
    pub(crate) hover_node_id: Option<NodeId>,
    /// The precise (may be anonymous) layout node under the pointer (if any).
    /// This can be invalidated by box-tree reconstruction, and is re-resolved against
    /// fresh layout at the end of every `resolve` pass.
    pub(crate) hover_hit_node_id: Option<NodeId>,
    /// Whether the node which is currently hovered is a text node/span
    pub(crate) hover_node_is_text: bool,
    /// The last known pointer position in client coordinates (viewport-relative, unscrolled).
    pub(crate) last_client_pointer_position: Option<taffy::Point<f32>>,
    /// The node which is currently focussed (if any)
    pub(crate) focus_node_id: Option<NodeId>,
    /// The node which is currently active (if any)
    pub(crate) active_node_id: Option<NodeId>,
    /// The node which recieved a mousedown event (if any)
    pub(crate) mousedown_node_id: Option<NodeId>,
    /// The last time a mousedown was made (for double-click detection)
    pub(crate) last_mousedown_time: Option<Instant>,
    /// The position where mousedown occurred (for selection drags and double-click detection)
    pub(crate) mousedown_position: taffy::Point<f32>,
    /// How many clicks have been made in quick succession
    pub(crate) click_count: u16,
    /// Whether we're currently in a text selection drag (moved 2px+ from mousedown)
    pub(crate) drag_mode: DragMode,
    /// The scrollbar thumb currently under the pointer, if any
    pub(crate) hovered_scrollbar: Option<crate::node::ScrollbarRef>,
    /// When each scroll container's overlay scrollbars were last shown
    /// (scrolled, or the pointer left the thumb); drives their fade-out
    pub(crate) scrollbar_activity: HashMap<NodeId, Instant>,
    /// Whether and what kind of scroll animation is currently in progress
    pub(crate) scroll_animation: ScrollAnimationState,

    /// Text selection state (for non-input text)
    pub(crate) text_selection: TextSelection,

    // TODO: collapse animating state into a bitflags
    /// Whether there are active CSS animations/transitions (so we should re-render every frame)
    pub(crate) has_active_animations: bool,
    /// Whether there is a `<canvas>` element in the DOM (so we should re-render every frame)
    pub(crate) has_canvas: bool,
    /// Whether there are subdocuments that are animating (so we should re-render every frame)
    pub(crate) subdoc_is_animating: bool,

    /// Map of id attribute values to node IDs for fast lookups.
    /// May contain multiple nodes for the same id: `get_element_by_id`
    /// returns the first in tree order.
    pub(crate) nodes_to_id: HashMap<String, SmallVec<[NodeId; 1]>>,
    /// Map of `<style>` and `<link>` node IDs to their associated stylesheet
    pub(crate) nodes_to_stylesheet: BTreeMap<NodeId, DocumentStyleSheet>,
    /// Incremented whenever `nodes_to_stylesheet` changes (a stylesheet is
    /// added, replaced or removed), so that CSSOM wrappers can detect that
    /// their cached rule data is stale.
    pub(crate) stylesheet_generation: u64,
    /// Map from form control node ID's to their associated forms node ID's
    pub(crate) controls_to_form: HashMap<NodeId, NodeId>,
    /// Nodes that contain sub documents
    pub(crate) sub_document_nodes: HashSet<NodeId>,
    /// Load state (abort controller and in-flight request id) for each
    /// `<iframe>` element whose sub-document is loaded automatically
    pub(crate) iframe_loads: HashMap<NodeId, crate::iframe::IframeLoad>,
    /// Set of changed nodes for updating the accessibility tree
    pub(crate) changed_nodes: HashSet<NodeId>,
    /// Set of changed nodes for updating the accessibility tree
    pub(crate) deferred_construction_nodes: Vec<ConstructionTask>,

    /// Nodes that contain custom widgets
    #[cfg(feature = "custom-widget")]
    pub(crate) custom_widget_nodes: HashSet<NodeId>,
    /// Rendering resources allocated by custom widgets that should be deallocated during the next render
    #[cfg(feature = "custom-widget")]
    pub(crate) pending_resource_deallocations: Vec<anyrender::ResourceId>,

    /// Cache of loaded images, keyed by URL. Allows reusing images across multiple
    /// elements without re-fetching from the network.
    pub(crate) image_cache: HashMap<String, ImageData>,

    /// Tracks in-flight image requests. When an image is being fetched, additional
    /// requests for the same URL are queued here instead of starting new fetches.
    /// Value is a list of (node_id, image_type) pairs waiting for the image.
    pub(crate) pending_images: HashMap<String, Vec<(NodeId, ImageType)>>,

    /// Nodes whose `background-image`/`mask-image` layers need flushing to
    /// dedicated storage on the node because their style changed (populated by
    /// the style traversal and by pseudo-element box construction).
    pub(crate) pending_style_image_nodes: Vec<NodeId>,

    // Tracks in-flight "critical" resources (e.g. stylesheets linked from the `<head>`),
    // keyed by request id
    pub(crate) pending_critical_resources: HashSet<usize>,

    // Service providers
    /// Network provider. Can be used to fetch assets.
    pub net_provider: Arc<dyn NetProvider>,
    /// Navigation provider. Can be used to navigate to a new page (bubbles up the event
    /// on e.g. clicking a Link)
    pub navigation_provider: Arc<dyn NavigationProvider>,
    /// Shell provider. Can be used to request a redraw or set the cursor icon
    pub shell_provider: Arc<dyn ShellProvider>,
    /// HTML parser provider. Used to parse HTML for setInnerHTML
    pub html_parser_provider: Arc<dyn HtmlParserProvider>,
    /// Carried on every sub-resource `Request` this document issues; aborting
    /// it cancels all in-flight fetches tied to this document. Set via
    /// [`DocumentConfig::abort_signal`].
    pub(crate) abort_signal: Option<AbortSignal>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[cfg(test)]
#[path = "tests/zoom_tests.rs"]
mod zoom_tests;

#[cfg(test)]
#[path = "tests/hover_state_tests.rs"]
mod hover_state_tests;

#[cfg(test)]
#[path = "tests/hover_invalidation_tests.rs"]
mod hover_invalidation_tests;

#[cfg(test)]
#[path = "tests/font_face_override_tests.rs"]
mod font_face_override_tests;

mod construction;
mod hover_viewport;
mod queries;
mod resources_and_hit;
mod tree_ops;
use queries::*;
