//! Case, mathematical italic, full-width and full-size kana mapping for the CSS `text-transform`
//! property.
//!
//! <https://drafts.csswg.org/css-text/#text-transform-property>

#[cfg(feature = "text-transform-icu")]
use icu_casemap::options::{LeadingAdjustment, TitlecaseOptions, TrailingCase};
#[cfg(feature = "text-transform-icu")]
use icu_casemap::{CaseMapper, CaseMapperBorrowed, TitlecaseMapper, TitlecaseMapperBorrowed};
use icu_locale_core::LanguageIdentifier;
use icu_properties::props::{GeneralCategory, GeneralCategoryGroup};
use icu_properties::{CodePointMapData, CodePointMapDataBorrowed};
use icu_segmenter::{WordSegmenter, WordSegmenterBorrowed, options::WordBreakInvariantOptions};
use parley::{Brush, TreeBuilder};
use style::computed_values::white_space_collapse::T as WhiteSpaceCollapse;
use style::properties::ComputedValues;
use style::values::computed::TextTransform;
use style::values::specified::text::TextTransformCase;
#[cfg(feature = "text-transform-icu")]
use writeable::Writeable;

/// The text transforms (and language) for an element's text content.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CaseTransform {
    text_transform: TextTransform,
    /// Determines which spaces `full-width` maps.
    white_space_collapse: WhiteSpaceCollapse,
    lang: LanguageIdentifier,
}

/// Applies text transforms to the sequence of text nodes in an inline formatting context.
///
/// `capitalize` operates on words, which may span multiple text nodes, so the text already
/// pushed to the [`TreeBuilder`] is used as context for word segmentation. Only look-behind
/// is needed: whether a word boundary precedes a letter never depends on the text that
/// follows it.
#[derive(Default)]
pub(crate) struct TextTransformer {
    /// The offset in the builder's text of the last forced word boundary.
    context_start: usize,
    /// Reused buffer for the context followed by the text to be capitalized.
    segmentation_buffer: String,
    /// Reused buffer for the transformed text.
    output: String,
}

/// Collects transformed text, borrowing the original text if it is unchanged and otherwise
/// copying it into a reused buffer from the first difference onwards.
struct OutputSink<'a> {
    original: &'a str,
    /// The length of the output so far if it is a prefix of `original`, or `None` if the output
    /// has diverged and is in `buffer`.
    unchanged_len: Option<usize>,
    buffer: &'a mut String,
    /// `full-width` and/or `full-size-kana`, applied to each pushed character.
    width: TextTransform,
    white_space_collapse: WhiteSpaceCollapse,
}

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

mod transforms;
pub use transforms::*;
