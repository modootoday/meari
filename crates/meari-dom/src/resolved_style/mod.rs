//! Computation of *resolved* CSS property values, as exposed to JavaScript by
//! `getComputedStyle()`.
//!
//! For most properties the resolved value is the stylo computed value. For
//! layout-dependent properties (`width`/`height`, margins, padding, insets,
//! `transform-origin`, grid track sizes) it is the *used* value, computed from
//! the most recent layout.

use cssparser::Parser;
use selectors::matching::QuirksMode;
use style::computed_values::box_sizing::T as BoxSizing;
use style::computed_values::position::T as Position;
use style::parser::ParserContext;
use style::properties::declaration_block::{Importance, parse_style_attribute};
use style::properties::{
    ComputedValues, NonCustomPropertyId, PropertyDeclaration, PropertyDeclarationBlock,
    PropertyDeclarationId, PropertyId, ShorthandId, SourcePropertyDeclaration,
    parse_one_declaration_into,
};
use style::servo_arc::Arc as ServoArc;
use style::stylesheets::supports_rule::parse_condition_or_declaration;
use style::stylesheets::{CssRuleType, Origin, OriginSet, UrlExtraData};
use style::stylist::RegisterCustomPropertyResult;
use style::values::computed::length::CSSPixelLength;
use style::values::computed::{GridTemplateAreas, GridTemplateComponent, LengthPercentage};
use style::values::generics::position::{Inset as GenericInset, PreferredRatio};
use style::values::resolved;
use style::values::specified::box_::{DisplayInside, DisplayOutside};
use style_traits::{CssStringWriter, ParsingMode, ToCss};

use blitz_traits::node_id::NodeId;
use url::Url;

use crate::BaseDocument;
use crate::layout::replaced::is_replaced_element;
use crate::local_name;

mod style_attr;
pub use style_attr::*;
mod resolved_values;
