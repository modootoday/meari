use blitz_traits::node_id::NodeId;
use cssparser::Parser;
use kurbo::{Affine, Rect as KurboRect};
use linebender_resource_handle::Blob;
use markup5ever::{LocalName, QualName, local_name};
use selectors::matching::{ElementSelectorFlags, QuirksMode};
use std::cell::Cell;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use style::Atom;
use style::parser::ParserContext;
use style::properties::{Importance, PropertyDeclaration, PropertyId, SourcePropertyDeclaration};
use style::stylesheets::{DocumentStyleSheet, Origin, UrlExtraData};
use style::values::computed::Display as StyloDisplay;
use style::{
    properties::{PropertyDeclarationBlock, parse_style_attribute},
    servo_arc::Arc as ServoArc,
    shared_lock::{Locked, SharedRwLock},
    stylesheets::CssRuleType,
};
use style_dom::ElementState;
use style_traits::ParsingMode;
use taffy::{Cache, prelude::Layout};
use thin_vec::ThinVec;
use url::Url;

use super::stylo_data::StyloData;
#[cfg(feature = "svg")]
use super::svg::SvgImageData;
use super::{Attribute, Attributes};
use crate::Document;
use crate::layout::table::TableContext;
use crate::node::{TextBrush, TextInputData, TextLayout};

#[cfg(feature = "custom-widget")]
use super::custom_widget::CustomWidgetData;

macro_rules! local_names {
    ($($name:tt),+) => {
        [$(local_name!($name),)+]
    };
}

pub struct ElementData {
    /// The elements tag name, namespace and prefix
    pub name: QualName,

    /// The elements id attribute parsed as an atom (if it has one)
    pub id: Option<Atom>,

    /// The element's attributes
    pub attrs: Attributes,

    /// Whether the element is focussable
    pub is_focussable: bool,

    /// The element's parsed style attribute (used by stylo)
    pub style_attribute: Option<ServoArc<Locked<PropertyDeclarationBlock>>>,

    /// Heterogeneous data that depends on the element's type.
    /// For example:
    ///   - The image data for \<img\> elements.
    ///   - The parley Layout for inline roots.
    ///   - The text editor for input/textarea elements
    pub special_data: SpecialElementData,

    pub background_images: ThinVec<Option<ImageResourceData>>,

    pub mask_images: ThinVec<Option<ImageResourceData>>,

    /// Parley text layout (elements with inline inner display mode only)
    pub inline_layout_data: Option<Box<TextLayout>>,

    /// Data associated with display: list-item. Note that this display mode
    /// does not exclude inline_layout_data
    pub list_item_data: Option<Box<ListItemLayout>>,

    /// The element's template contents (\<template\> elements only)
    pub template_contents: Option<NodeId>,
    // /// Whether the node is a [HTML integration point] (https://html.spec.whatwg.org/multipage/#html-integration-point)
    // pub mathml_annotation_xml_integration_point: bool,

    // ---------------------------------------------------------------------
    // Fields moved from `Node`. These live on the element data so that the
    // `Node` struct itself only carries tree-structure information.
    // ---------------------------------------------------------------------
    /// Style data from stylo, plus a lock guard that allows access to it.
    pub stylo_element_data: StyloData,
    pub selector_flags: Cell<ElementSelectorFlags>,
    /// A clone of the document's shared style lock. Set when the owning
    /// [`Node`](super::Node) is constructed.
    pub guard: Option<SharedRwLock>,
    pub element_state: ElementState,
    pub has_snapshot: bool,
    pub snapshot_handled: AtomicBool,
    /// Whether any descendant of this node needs restyling.
    /// Used by Stylo's incremental style traversal to skip unchanged subtrees.
    pub dirty_descendants: AtomicBool,
    /// Whether this node or any of its descendants may carry `RestyleDamage`.
    /// Used by the damage propagation pass to skip unchanged subtrees.
    pub damaged_descendants: AtomicBool,

    // Pseudo element nodes
    pub before: Option<NodeId>,
    pub after: Option<NodeId>,

    /// Detailed layout information from the most recent layout (currently
    /// grid track sizing information for grid containers only). Used by
    /// devtools grid inspection and out-of-flow grid-area positioning.
    pub detailed_layout_info: taffy::DetailedLayoutInfo<Atom>,

    // Taffy layout data:
    pub display_constructed_as: StyloDisplay,
    /// Layout output state (`None` until layout first writes to this node).
    pub layout_data: Option<Box<LayoutData>>,
    pub transform: Option<Box<Affine>>,
}

/// Taffy layout output state (cache, layouts, scroll offset and overflow).
///
/// Lazily boxed on [`ElementData`] / [`DocumentData`]: inline-level elements
/// are positioned by the inline (parley) layout rather than by taffy, so most
/// nodes on text-heavy pages never allocate one.
#[derive(Debug, Clone)]
pub struct LayoutData {
    pub cache: Cache,
    pub unrounded_layout: Layout,
    pub final_layout: Layout,
    pub scroll_offset: crate::Point<f64>,
    pub scrollable_overflow: KurboRect,
    /// The writing mode this node's layout algorithm ran in (the root takes it from `<body>`),
    /// recorded by the layout pass for CSSOM geometry lookups.
    #[cfg(feature = "writing-mode")]
    pub writing_mode: stylo_taffy::WritingMode,
}

/// Data specific to the [`Document`](super::super::Document) root node.
///
/// The document node participates in layout and styling like an element, so it
/// carries the same style/layout fields that were previously stored directly on
/// [`Node`](super::Node).
pub struct DocumentData {
    pub stylo_element_data: StyloData,
    /// Selector flags deposited here by `apply_selector_flags` when a
    /// `for_parent()` flag is applied while matching the root `<html>` element,
    /// whose parent node is the document.
    pub selector_flags: Cell<ElementSelectorFlags>,
    /// A clone of the document's shared style lock. Set when the owning
    /// [`Node`](super::Node) is constructed.
    pub guard: Option<SharedRwLock>,
    pub dirty_descendants: AtomicBool,
    pub damaged_descendants: AtomicBool,
    pub element_state: ElementState,
    pub has_snapshot: bool,
    pub snapshot_handled: AtomicBool,
    pub display_constructed_as: StyloDisplay,
    /// Layout output state (`None` until layout first writes to this node).
    pub layout_data: Option<Box<LayoutData>>,
    pub transform: Option<Box<Affine>>,
}

#[derive(Copy, Clone, Default)]
#[non_exhaustive]
pub enum SpecialElementType {
    Stylesheet,
    Image,
    Canvas,
    TableRoot,
    TextInput,
    CheckboxInput,
    #[cfg(feature = "file-input")]
    FileInput,
    #[default]
    None,
}

/// Heterogeneous data that depends on the element's type.
#[derive(Default)]
pub enum SpecialElementData {
    /// A sub-document such an \<iframe\> or \<web-view\> element
    SubDocument(Box<dyn Document>),
    /// A custom widget
    #[cfg(feature = "custom-widget")]
    CustomWidget(CustomWidgetData),
    /// A stylesheet
    Stylesheet(DocumentStyleSheet),
    /// An \<img\> element's image data
    Image(Box<ImageData>),
    /// A \<canvas\> element's custom paint source
    Canvas(CanvasData),
    /// Pre-computed table layout data
    TableRoot(Arc<TableContext>),
    /// Parley text editor (text inputs)
    TextInput(TextInputData),
    /// Checkbox checked state
    CheckboxInput(bool),
    /// Selected files
    #[cfg(feature = "file-input")]
    FileInput(FileData),
    /// No data (for nodes that don't need any node-specific data)
    #[default]
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RasterImageData {
    /// The width of the image
    pub width: u32,
    /// The height of the image
    pub height: u32,
    /// The raw image data in RGBA8 format
    pub data: Blob<u8>,
}

#[derive(Debug, Clone)]
pub enum ImageData {
    Raster(RasterImageData),
    #[cfg(feature = "svg")]
    Svg(SvgImageData),
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Ok,
    Error,
    Loading,
}

#[derive(Debug, Clone)]
pub struct ImageResourceData {
    /// The url of the background image
    pub url: ServoArc<Url>,
    /// The loading status of the background image
    pub status: Status,
    /// The image data
    pub image: ImageData,
}

#[derive(Debug, Clone)]
pub struct CanvasData {
    pub custom_paint_source_id: u64,
}

#[derive(Clone)]
pub struct ListItemLayout {
    pub marker: Marker,
    pub position: ListItemLayoutPosition,
}

//We seperate chars from strings in order to optimise rendering - ie not needing to
//construct a whole parley layout for simple char markers
#[derive(Debug, PartialEq, Clone)]
pub enum Marker {
    Char(char),
    String(String),
}

//Value depends on list-style-position, determining whether a seperate layout is created for it
#[derive(Clone)]
pub enum ListItemLayoutPosition {
    Inside,
    Outside(Box<parley::Layout<TextBrush>>),
}

#[cfg(feature = "file-input")]
mod file_data {
    use std::ops::{Deref, DerefMut};
    use std::path::PathBuf;

    #[derive(Clone, Debug)]
    pub struct FileData(pub Vec<PathBuf>);
    impl Deref for FileData {
        type Target = Vec<PathBuf>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl DerefMut for FileData {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl From<Vec<PathBuf>> for FileData {
        fn from(files: Vec<PathBuf>) -> Self {
            Self(files)
        }
    }
}
#[cfg(feature = "file-input")]
pub use file_data::FileData;

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

mod element_data;
mod element_mutation;
