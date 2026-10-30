use super::*;

/// Serialize a used length (in CSS pixels) the way stylo serializes computed lengths
pub(super) fn format_px(px: f32) -> String {
    CSSPixelLength::new(px).to_css_string()
}

/// Resolve a computed inset value to CSS pixels against the given percentage
/// basis. Returns `None` for `auto` (and unsupported anchor functions).
pub(super) fn resolve_inset<P>(val: &GenericInset<P, LengthPercentage>, basis: f32) -> Option<f32> {
    match val {
        GenericInset::LengthPercentage(lp) => Some(lp.resolve(CSSPixelLength::new(basis)).px()),
        _ => None,
    }
}

/// Serialize a transform matrix component rounded to 6 decimal places (as
/// browsers do when serializing resolved transform matrices)
pub(super) fn format_matrix_component(v: f64) -> String {
    let rounded = (v * 1e6).round() / 1e6;
    // Avoid "-0"
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    format!("{rounded}")
}

/// Serialize a used transform matrix the way `getComputedStyle()` does
/// (`matrix(...)` for 2D transforms, `matrix3d(...)` otherwise).
pub(super) fn format_matrix(m: &euclid::default::Transform3D<f64>, is_3d: bool) -> String {
    let c = format_matrix_component;
    if is_3d {
        format!(
            "matrix3d({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
            c(m.m11),
            c(m.m12),
            c(m.m13),
            c(m.m14),
            c(m.m21),
            c(m.m22),
            c(m.m23),
            c(m.m24),
            c(m.m31),
            c(m.m32),
            c(m.m33),
            c(m.m34),
            c(m.m41),
            c(m.m42),
            c(m.m43),
            c(m.m44)
        )
    } else {
        format!(
            "matrix({}, {}, {}, {}, {}, {})",
            c(m.m11),
            c(m.m12),
            c(m.m21),
            c(m.m22),
            c(m.m41),
            c(m.m42)
        )
    }
}

/// Check whether `name` names a CSS property supported by the style engine.
/// Custom properties (`--*`) are not considered "supported" here, matching the
/// behaviour of the `in` operator on `CSSStyleDeclaration` objects in browsers.
pub fn css_property_is_supported(name: &str) -> bool {
    matches!(
        PropertyId::parse_enabled_for_all_content(name),
        Ok(property_id) if !matches!(property_id, PropertyId::Custom(_))
    )
}

/// Parse a CSS `transform` list into a 4x4 matrix, as required by the
/// `DOMMatrix(DOMString)` constructor and `DOMMatrix.setMatrixValue()`
/// (<https://drafts.fxtf.org/geometry/#parse-a-string-into-an-abstract-matrix>).
///
/// Returns the 16 matrix components in column-major order (`m11, m12, ...,
/// m44`) and whether the list contained only 2D transform functions.
/// Returns `None` if the string fails to parse as a transform list or uses
/// relative lengths (which cannot be resolved without a context).
pub fn parse_transform_matrix(value: &str) -> Option<([f64; 16], bool)> {
    use style::properties::longhands::transform;
    // Transform lists cannot contain URLs, so any base URL will do
    let url_data = UrlExtraData(ServoArc::new(Url::parse("about:blank").unwrap()));
    let context = ParserContext::new(
        Origin::Author,
        &url_data,
        Some(CssRuleType::Style),
        ParsingMode::DEFAULT,
        QuirksMode::NoQuirks,
        Default::default(),
        None,
        None,
        Default::default(),
    );
    let mut parser = Parser::new(value);
    let transform = parser
        .parse_entirely(|t| transform::parse(&context, t))
        .ok()?;
    let (m, is_3d) = transform.to_transform_3d_matrix_f64(None).ok()?;
    Some((
        [
            m.m11, m.m12, m.m13, m.m14, m.m21, m.m22, m.m23, m.m24, m.m31, m.m32, m.m33, m.m34,
            m.m41, m.m42, m.m43, m.m44,
        ],
        !is_3d,
    ))
}

/// The names of the properties exposed by the `CSSStyleDeclaration` returned
/// from `getComputedStyle()` (its indexed properties): every enabled longhand,
/// sorted alphabetically.
pub fn resolved_style_property_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names: Vec<&'static str> = NonCustomPropertyId::iter()
            .filter_map(|id| id.as_longhand())
            .map(|longhand| longhand.name())
            .filter(|name| PropertyId::parse_enabled_for_all_content(name).is_ok())
            .collect();
        names.sort_unstable();
        names
    })
}

impl BaseDocument {
    /// Check whether `value` is a valid value for the CSS property `property`.
    /// Used by CSSOM APIs (`element.style.setProperty` and friends), which must
    /// ignore invalid declarations.
    pub fn css_declaration_is_valid(&self, property: &str, value: &str) -> bool {
        let Ok(property_id) = PropertyId::parse_enabled_for_all_content(property) else {
            return false;
        };
        let mut declarations = SourcePropertyDeclaration::default();
        parse_one_declaration_into(
            &mut declarations,
            property_id,
            value,
            Origin::Author,
            &self.url.url_extra_data(),
            None,
            ParsingMode::DEFAULT,
            QuirksMode::NoQuirks,
            CssRuleType::Style,
        )
        .is_ok()
    }

    /// Parse a `style` attribute string into a [`PropertyDeclarationBlock`].
    /// Invalid declarations are dropped (per CSS error recovery).
    pub(super) fn parse_style_attr_block(&self, style_attr: &str) -> PropertyDeclarationBlock {
        parse_style_attribute(
            style_attr,
            &self.url.url_extra_data(),
            None,
            QuirksMode::NoQuirks,
            CssRuleType::Style,
        )
    }

    /// The canonical CSSOM serialization of a `style` attribute string
    /// (`CSSStyleDeclaration.cssText` getter).
    pub fn style_attr_serialize(&self, style_attr: &str) -> String {
        let block = self.parse_style_attr_block(style_attr);
        let mut css = CssStringWriter::new();
        let _ = block.to_css(&mut css);
        css
    }

    /// The canonical serialization of `property`'s value in a `style` attribute
    /// string (`CSSStyleDeclaration.getPropertyValue()`). Handles shorthands.
    /// Returns an empty string if the property is not set or not recognised.
    pub fn style_attr_get_property(&self, style_attr: &str, property: &str) -> String {
        let Ok(property_id) = PropertyId::parse_enabled_for_all_content(property) else {
            return String::new();
        };
        let block = self.parse_style_attr_block(style_attr);
        let mut css = CssStringWriter::new();
        let _ = block.property_value_to_css(&property_id, &mut css);
        css
    }

    /// Set `property` to `value` in a `style` attribute string
    /// (`CSSStyleDeclaration.setProperty()`), expanding shorthands.
    ///
    /// Returns the new (canonically serialized) style attribute, or `None` if
    /// the declaration was invalid (in which case the style is unchanged, per
    /// CSSOM). An empty `value` removes the property.
    pub fn style_attr_set_property(
        &self,
        style_attr: &str,
        property: &str,
        value: &str,
        important: bool,
    ) -> Option<String> {
        let property_id = PropertyId::parse_enabled_for_all_content(property).ok()?;
        let mut block = self.parse_style_attr_block(style_attr);

        if value.trim().is_empty() {
            if let Some(first_declaration) = block.first_declaration_to_remove(&property_id) {
                block.remove_property(&property_id, first_declaration);
            }
        } else {
            let mut source = SourcePropertyDeclaration::default();
            parse_one_declaration_into(
                &mut source,
                property_id,
                value,
                Origin::Author,
                &self.url.url_extra_data(),
                None,
                ParsingMode::DEFAULT,
                QuirksMode::NoQuirks,
                CssRuleType::Style,
            )
            .ok()?;
            let importance = if important {
                Importance::Important
            } else {
                Importance::Normal
            };
            block.extend(source.drain(), importance);
        }

        let mut css = CssStringWriter::new();
        let _ = block.to_css(&mut css);
        Some(css)
    }

    /// Remove `property` from a `style` attribute string
    /// (`CSSStyleDeclaration.removeProperty()`), removing all longhands if it
    /// is a shorthand.
    ///
    /// Returns the new (canonically serialized) style attribute and the removed
    /// property's previous serialized value.
    pub fn style_attr_remove_property(
        &self,
        style_attr: &str,
        property: &str,
    ) -> Option<(String, String)> {
        let property_id = PropertyId::parse_enabled_for_all_content(property).ok()?;
        let mut block = self.parse_style_attr_block(style_attr);

        let mut removed_value = CssStringWriter::new();
        let _ = block.property_value_to_css(&property_id, &mut removed_value);
        if let Some(first_declaration) = block.first_declaration_to_remove(&property_id) {
            block.remove_property(&property_id, first_declaration);
        }

        let mut css = CssStringWriter::new();
        let _ = block.to_css(&mut css);
        Some((css, removed_value))
    }

    /// Evaluate a `@supports` condition (e.g. `(display: grid)`,
    /// `selector(:hover)`) or a bare declaration (e.g. `display: grid`), as
    /// used by the CSSOM `CSS.supports(conditionText)` API. Returns `false`
    /// for unparseable conditions.
    pub fn css_supports_condition(&self, condition: &str) -> bool {
        let mut parser = Parser::new(condition);
        let Ok(condition) = parser.parse_entirely(parse_condition_or_declaration) else {
            return false;
        };

        let url_data = self.url.url_extra_data();
        let context = ParserContext::new(
            Origin::Author,
            &url_data,
            Some(CssRuleType::Style),
            ParsingMode::DEFAULT,
            QuirksMode::NoQuirks,
            Default::default(),
            None,
            None,
            Default::default(),
        );
        condition.eval(&context)
    }

    /// Register a custom property via script (`CSS.registerProperty()`).
    /// On success all styles are invalidated so that declarations of the
    /// property are re-parsed against the new registration.
    pub fn register_custom_property(
        &mut self,
        name: &str,
        syntax: &str,
        inherits: bool,
        initial_value: Option<&str>,
    ) -> RegisterCustomPropertyResult {
        let url_data = self.url.url_extra_data();
        let result =
            self.stylist
                .register_custom_property(&url_data, name, syntax, inherits, initial_value);
        if matches!(result, RegisterCustomPropertyResult::SuccessfullyRegistered) {
            self.stylist
                .force_stylesheet_origins_dirty(OriginSet::all());
        }
        result
    }
}
