//! CSSOM stylesheet access (`document.styleSheets`, `CSSStyleSheet`,
//! `CSSRuleList`, `CSSRule` and friends) backed by stylo's stylesheet objects.
//!
//! Stylesheets are identified by the `NodeId` of their owner node (`<style>` or
//! `<link>`), and rules by a *path* of indices from the stylesheet's top-level
//! rule list down through nested rule lists (`@media`, `@supports`, nested
//! style rules, `@keyframes`, ...). An empty path denotes the stylesheet's own
//! top-level rule list.

use cssparser::Parser;
use selectors::matching::QuirksMode;
use selectors::parser::{ParseRelative, SelectorList};
use style::font_face::FontFaceRule;
use style::invalidation::stylesheets::RuleChangeKind;
use style::parser::ParserContext;
use style::properties::font_face::DescriptorId as FontFaceDescriptorId;
use style::properties::{
    Importance, PropertyDeclarationBlock, PropertyId, SourcePropertyDeclaration,
    parse_one_declaration_into,
};
use style::selector_parser::SelectorParser;
use style::servo_arc::Arc as ServoArc;
use style::shared_lock::{Locked, SharedRwLockReadGuard, ToCssWithGuard};
use style::stylesheets::keyframes_rule::{Keyframe, KeyframesRule};
use style::stylesheets::{
    AllowImportRules, CssRule, CssRuleRef, CssRuleType, CssRuleTypes, CssRules, DocumentStyleSheet,
    Origin, OriginSet, RulesMutateError, StylesheetInDocument,
};
use style_traits::{CssStringWriter, ParsingMode, ToCss};
// For `SelectorList::to_css_string`
use cssparser::ToCss as _;

use blitz_traits::node_id::NodeId;

use crate::BaseDocument;
use crate::net::StylesheetLoader;

/// Errors from CSSOM mutation operations, mirroring the `DOMException` names
/// which the corresponding JavaScript APIs throw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssomError {
    /// The owner node has no associated stylesheet, or the rule path does not
    /// resolve to a rule (list).
    NotFound,
    /// `SyntaxError`
    Syntax,
    /// `IndexSizeError`
    IndexSize,
    /// `HierarchyRequestError`
    HierarchyRequest,
    /// `InvalidStateError`
    InvalidState,
    /// `NotSupportedError`
    NotSupported,
}

/// A snapshot of the CSSOM-visible attributes of a rule
#[derive(Debug, Clone)]
pub struct CssRuleInfo {
    /// The name of the CSSOM interface for the rule (e.g. `"CSSStyleRule"`)
    pub interface: &'static str,
    /// The legacy `CSSRule.type` constant (0 for rules without one)
    pub rule_type: u16,
    /// `CSSRule.cssText`
    pub css_text: String,
    /// Whether the rule has a child rule list (`cssRules`)
    pub has_child_rules: bool,
    /// Whether the rule has a `style` declaration block
    pub has_style: bool,
    /// Additional interface-specific string attributes (e.g. `selectorText`)
    pub attributes: Vec<(&'static str, String)>,
}

/// A rule list that CSSOM rule paths can index into
#[derive(Clone)]
enum RuleList {
    Css(ServoArc<Locked<CssRules>>),
    Keyframes(ServoArc<Locked<KeyframesRule>>),
}

/// A rule resolved from a CSSOM rule path
#[derive(Clone)]
enum RuleHandle {
    Rule(CssRule),
    Keyframe(ServoArc<Locked<Keyframe>>),
}

/// The declarations of a rule (`CSSRule.style`)
enum DeclarationTarget {
    Block {
        block: ServoArc<Locked<PropertyDeclarationBlock>>,
        rule_type: CssRuleType,
    },
    FontFace(ServoArc<Locked<FontFaceRule>>),
}

mod rule_access;
mod rule_style;
