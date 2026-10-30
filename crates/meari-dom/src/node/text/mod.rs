use blitz_traits::{
    events::{BlitzImeEvent, BlitzKeyEvent},
    node_id::NodeId,
    shell::ShellProvider,
};
use keyboard_types::{Key, Modifiers, NamedKey};
use parley::{ContentWidths, FontContext, LayoutContext};

use crate::util::action_mod;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
/// Parley Brush type for Blitz which contains the Blitz node id
pub struct TextBrush {
    /// The node id for the span
    pub id: NodeId,
}

#[derive(Clone, Default)]
pub struct TextLayout {
    pub text: String,
    pub content_widths: Option<ContentWidths>,
    pub layout: parley::layout::Layout<TextBrush>,
    /// Block-axis offset (in CSS px) of the line boxes from the top of the container's content
    /// box, as applied by `align-content`.
    pub block_offset: f32,
    /// The out-of-flow positions for which some inline span with an inline box below it is a
    /// containing block (as `SPAN_*_CB` flags). The inline root claims those boxes in its place.
    /// Set by each `RunMode::PerformLayout` run of inline layout.
    pub span_cb_flags: stylo_taffy::StyleFlags,
}

// TODO: support keypress events
pub enum GeneratedTextInputEvent {
    Input,
    Select,
    PreEditChange,
    Submit,
}

pub struct TextInputData {
    /// A parley TextEditor instance
    pub editor: Box<parley::PlainEditor<TextBrush>>,
    /// Whether the input is a singleline or multiline input
    pub is_multiline: bool,
    /// The scroll offset of the text content within the input, in CSS (unscaled) pixels.
    ///
    /// For single-line inputs this is a horizontal offset; for multi-line inputs it is a
    /// vertical offset. It is kept up to date so that the caret remains visible within the
    /// input's content box.
    pub scroll_offset: f32,
}

mod keybindings;
mod text_input;
