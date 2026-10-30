//! Conversion functions from Stylo types to Parley types
use blitz_traits::node_id::NodeId;
use std::borrow::Cow;

use style::values::computed::Length;

use crate::node::TextBrush;

// Module of type aliases so we can refer to stylo types with nicer names
#[path = "tests/stylo.rs"]
pub(crate) mod stylo;

pub(crate) mod parley {
    pub(crate) use parley::BaseDirection;
    pub(crate) use parley::FontFeature;
    pub(crate) use parley::FontVariation;
    pub(crate) use parley::fontique::QueryFamily;
    pub(crate) use parley::layout::Alignment;
    pub(crate) use parley::setting::*;
    pub(crate) use parley::style::*;
}

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;

mod font_conversion;
pub(crate) use font_conversion::*;
