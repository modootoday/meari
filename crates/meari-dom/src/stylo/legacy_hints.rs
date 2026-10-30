use super::*;

#[test]
pub(super) fn assert_size_of_equals() {
    // use std::mem;

    // fn assert_layout<E>() {
    //     assert_eq!(
    //         mem::size_of::<SharingCache<E>>(),
    //         mem::size_of::<TypelessSharingCache>()
    //     );
    //     assert_eq!(
    //         mem::align_of::<SharingCache<E>>(),
    //         mem::align_of::<TypelessSharingCache>()
    //     );
    // }

    // let size = mem::size_of::<StyleSharingCandidate<BlitzNode>>();
    // dbg!(size);
}

#[test]
pub(super) fn parse_inline() {
    // let attrs = style::attr::AttrValue::from_serialized_tokenlist(
    //     r#"visibility: hidden; left: 1306.5px; top: 50px; display: none;"#.to_string(),
    // );

    // let val = CSSInlineStyleDeclaration();
}

/// Body of `TElement::synthesize_presentational_hints_for_legacy_attributes`.
pub(super) fn legacy_presentational_hints<V>(
    this: BlitzNode<'_>,
    _visited_handling: VisitedHandlingMode,
    hints: &mut V,
) where
    V: Push<style::applicable_declarations::ApplicableDeclarationBlock>,
{
    let Some(elem) = this.data.downcast_element() else {
        return;
    };

    let tag = &elem.name.local;

    let mut push_style = |decl: PropertyDeclaration| {
        hints.push(ApplicableDeclarationBlock::from_declarations(
            Arc::new(
                this.guard()
                    .wrap(PropertyDeclarationBlock::with_one(decl, Importance::Normal)),
            ),
            CascadeLevel::new(CascadeOrigin::PresHints),
            LayerOrder::root(),
        ));
    };

    fn parse_color_attr(value: &str) -> Option<(u8, u8, u8, f32)> {
        if !value.starts_with('#') {
            return None;
        }

        let value = &value[1..];
        if value.len() == 3 {
            let r = u8::from_str_radix(&value[0..1], 16).ok()?;
            let g = u8::from_str_radix(&value[1..2], 16).ok()?;
            let b = u8::from_str_radix(&value[2..3], 16).ok()?;
            return Some((r, g, b, 1.0));
        }

        if value.len() == 6 {
            let r = u8::from_str_radix(&value[0..2], 16).ok()?;
            let g = u8::from_str_radix(&value[2..4], 16).ok()?;
            let b = u8::from_str_radix(&value[4..6], 16).ok()?;
            return Some((r, g, b, 1.0));
        }

        None
    }

    /// The HTML "rules for parsing dimension values" -- Stylo's
    /// implementation of them -- packaged as a specified
    /// `<length-percentage>`. `ignoring_zero` selects the spec's separate
    /// "maps to the dimension property (ignoring zero)" mapping, where a
    /// zero value is dropped rather than honoured.
    ///
    /// https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#rules-for-parsing-dimension-values
    fn parse_size_attr(
        value: &str,
        ignoring_zero: bool,
    ) -> Option<style::values::specified::LengthPercentage> {
        use style::servo::attr::{LengthOrPercentageOrAuto, parse_length, parse_nonzero_length};
        use style::values::specified::{LengthPercentage, NoCalcLength};
        let parsed = if ignoring_zero {
            parse_nonzero_length(value)
        } else {
            parse_length(value)
        };
        match parsed {
            LengthOrPercentageOrAuto::Length(length) => Some(LengthPercentage::Length(
                NoCalcLength::from_px(length.to_f32_px()),
            )),
            LengthOrPercentageOrAuto::Percentage(fraction) => Some(LengthPercentage::Percentage(
                NoCalcPercentage::new(fraction),
            )),
            LengthOrPercentageOrAuto::Auto => None,
        }
    }

    /// Parse the value of an SVG `width`/`height` presentation attribute.
    /// Unlike the legacy HTML dimension attributes, these accept any CSS
    /// <length-percentage> (e.g. `1em`), and a unitless number means user
    /// units, which map to CSS px.
    fn parse_svg_size_attr(value: &str) -> Option<style::values::specified::LengthPercentage> {
        use style::values::specified::{LengthPercentage, NoCalcLength};
        use style_traits::ParsingMode;

        let value = value.trim();
        if let Some(number) = value.strip_suffix('%') {
            let val: f32 = number.trim().parse().ok()?;
            return (val >= 0.0)
                .then(|| LengthPercentage::Percentage(NoCalcPercentage::new(val / 100.0)));
        }

        // Split into number and unit: the unit is the trailing run of
        // ASCII alphabetic characters (this never eats into a scientific
        // exponent such as `1e3`, which ends in a digit).
        let number_len = value
            .trim_end_matches(|c: char| c.is_ascii_alphabetic())
            .len();
        let (number, unit) = value.split_at(number_len);
        let val: f32 = number.trim().parse().ok().filter(|v| *v >= 0.0)?;
        let length = if unit.is_empty() {
            NoCalcLength::from_px(val)
        } else {
            NoCalcLength::parse_dimension_with_flags(ParsingMode::DEFAULT, false, val, unit).ok()?
        };
        Some(LengthPercentage::Length(length))
    }

    // `<input type=image>` is the only input type that is replaced
    // content, and it is the only one that takes the embedded-content
    // presentational attributes. The type attribute is matched ASCII
    // case-insensitively, as attribute keywords always are.
    let is_image_input = *tag == local_name!("input")
        && elem.attrs().iter().any(|attr| {
            attr.name.local == local_name!("type") && attr.value.eq_ignore_ascii_case("image")
        });

    // https://html.spec.whatwg.org/multipage/#the-lang-and-xml:lang-attributes
    let accepts_lang = elem.name.ns == ns!(html) || elem.name.ns == ns!(svg);
    let mut lang = None;

    for attr in elem.attrs() {
        let name = &attr.name.local;
        let value = attr.value.as_str();

        if *name == local_name!("lang") {
            if attr.name.ns == ns!(xml) {
                lang = Some(value);
            } else if attr.name.ns == ns!() && accepts_lang {
                lang = lang.or(Some(value));
            }
        }

        if *name == local_name!("align") {
            use style::values::specified::TextAlign;
            let keyword = match value {
                "left" => Some(StyloTextAlign::MozLeft),
                "right" => Some(StyloTextAlign::MozRight),
                "center" => Some(StyloTextAlign::MozCenter),
                _ => None,
            };

            if let Some(keyword) = keyword {
                push_style(PropertyDeclaration::TextAlign(TextAlign::Keyword(keyword)));
            }
        }

        // The width/height attributes on these elements map to the
        // corresponding dimension properties (percentages allowed):
        // https://html.spec.whatwg.org/multipage/rendering.html#dimRendering
        // https://html.spec.whatwg.org/multipage/rendering.html#attributes-for-embedded-content-and-images
        let is_width = *name == local_name!("width");
        let is_height = *name == local_name!("height");
        // The elements whose width/height attributes map to the dimension
        // properties. `input` only joins them as type=image, which is the
        // one replaced input type.
        let is_embedded = *tag == local_name!("iframe")
            || *tag == local_name!("embed")
            || *tag == local_name!("video")
            || *tag == local_name!("object")
            || *tag == local_name!("img")
            || *tag == local_name!("marquee")
            || is_image_input;
        let maps_to_dimension = if is_width {
            is_embedded
                || *tag == local_name!("table")
                || *tag == local_name!("col")
                || *tag == local_name!("colgroup")
                || *tag == local_name!("tr")
                || *tag == local_name!("td")
                || *tag == local_name!("th")
                || *tag == local_name!("hr")
        } else if is_height {
            is_embedded
                || *tag == local_name!("table")
                || *tag == local_name!("thead")
                || *tag == local_name!("tbody")
                || *tag == local_name!("tfoot")
                || *tag == local_name!("tr")
                || *tag == local_name!("td")
                || *tag == local_name!("th")
        } else {
            false
        };
        if maps_to_dimension {
            // Three of these use the "(ignoring zero)" variant of the
            // mapping, where a zero is dropped instead of honoured:
            // `table width`, and `td`/`th` in both axes.
            let is_cell = *tag == local_name!("td") || *tag == local_name!("th");
            let ignoring_zero = is_cell || (is_width && *tag == local_name!("table"));
            if let Some(size) = parse_size_attr(value, ignoring_zero) {
                use style::values::generics::{NonNegative, length::Size};
                let size = Size::LengthPercentage(NonNegative(size));
                push_style(if is_width {
                    PropertyDeclaration::Width(size)
                } else {
                    PropertyDeclaration::Height(size)
                });
            }
        }

        // hspace/vspace map to the horizontal and vertical margins as
        // dimension properties. This is a *smaller* set than the one that
        // takes width/height: `iframe` and `video` take width and height
        // but not hspace/vspace, and html/rendering/unmapped-attributes
        // checks exactly that -- browsers have got it wrong before.
        let takes_spacing = *tag == local_name!("embed")
            || *tag == local_name!("img")
            || *tag == local_name!("object")
            || *tag == local_name!("marquee")
            || is_image_input;
        if takes_spacing {
            let is_hspace = *name == local_name!("hspace");
            let is_vspace = *name == local_name!("vspace");
            if is_hspace || is_vspace {
                if let Some(size) = parse_size_attr(value, false) {
                    use style::values::generics::length::GenericMargin;
                    let margin = GenericMargin::LengthPercentage(size);
                    if is_hspace {
                        push_style(PropertyDeclaration::MarginLeft(margin.clone()));
                        push_style(PropertyDeclaration::MarginRight(margin));
                    } else {
                        push_style(PropertyDeclaration::MarginTop(margin.clone()));
                        push_style(PropertyDeclaration::MarginBottom(margin));
                    }
                }
            }
        }

        // https://svgwg.org/svg2-draft/geometry.html#Sizing
        // The `width` and `height` attributes on an `<svg>` element are
        // presentation attributes that map to the CSS `width`/`height`
        // properties, so e.g. `width="1em"` must resolve against the
        // element's font-size like any other CSS length.
        if *tag == local_name!("svg")
            && (*name == local_name!("width") || *name == local_name!("height"))
        {
            if let Some(size) = parse_svg_size_attr(value) {
                use style::values::generics::{NonNegative, length::Size};
                let size = Size::LengthPercentage(NonNegative(size));
                push_style(if *name == local_name!("width") {
                    PropertyDeclaration::Width(size)
                } else {
                    PropertyDeclaration::Height(size)
                });
            }
        }

        // The `border` attribute maps to the four border widths as a
        // pixel length, plus the four border styles as `solid` -- width
        // alone would compute back to zero against the default
        // border-style of `none`. It is only these three elements:
        // `embed`, `iframe`, `marquee` and non-image `input` all have a
        // `border` attribute that must stay unmapped.
        if *name == local_name!("border")
            && (*tag == local_name!("img") || *tag == local_name!("object") || is_image_input)
        {
            if let Ok(px) = style::servo::attr::parse_unsigned_integer(value.chars()) {
                use style::values::specified::{BorderSideWidth, BorderStyle};
                let width = BorderSideWidth::from_px(px as f32);
                push_style(PropertyDeclaration::BorderTopWidth(width.clone()));
                push_style(PropertyDeclaration::BorderRightWidth(width.clone()));
                push_style(PropertyDeclaration::BorderBottomWidth(width.clone()));
                push_style(PropertyDeclaration::BorderLeftWidth(width));
                push_style(PropertyDeclaration::BorderTopStyle(BorderStyle::Solid));
                push_style(PropertyDeclaration::BorderRightStyle(BorderStyle::Solid));
                push_style(PropertyDeclaration::BorderBottomStyle(BorderStyle::Solid));
                push_style(PropertyDeclaration::BorderLeftStyle(BorderStyle::Solid));
            }
        }

        // `body` carries four legacy margin attributes, as pixel lengths:
        // marginwidth and marginheight set both sides of an axis, and
        // leftmargin and topmargin set one side each.
        //
        // There is deliberately no `rightmargin` or `bottommargin`. They
        // look like the obvious counterparts to the two that exist, but
        // the spec does not define them and browsers ignore them in both
        // standards and quirks mode -- body-margin-3a/3b assert exactly
        // that.
        if *tag == local_name!("body") {
            // Matched as strings: these are not in the static atom set,
            // so `local_name!` will not compile for them.
            let sides: &[u8] = match &**name {
                "marginwidth" => b"lr",
                "marginheight" => b"tb",
                "leftmargin" => b"l",
                "topmargin" => b"t",
                _ => b"",
            };
            if !sides.is_empty() {
                if let Ok(px) = style::servo::attr::parse_unsigned_integer(value.chars()) {
                    use style::values::generics::length::GenericMargin;
                    use style::values::specified::{LengthPercentage, NoCalcLength};
                    let margin = GenericMargin::LengthPercentage(LengthPercentage::Length(
                        NoCalcLength::from_px(px as f32),
                    ));
                    for side in sides {
                        push_style(match side {
                            b'l' => PropertyDeclaration::MarginLeft(margin.clone()),
                            b'r' => PropertyDeclaration::MarginRight(margin.clone()),
                            b't' => PropertyDeclaration::MarginTop(margin.clone()),
                            b'b' => PropertyDeclaration::MarginBottom(margin.clone()),
                            _ => unreachable!("side table above only yields lrtb"),
                        });
                    }
                }
            }
        }

        if *name == local_name!("bgcolor") {
            use style::values::specified::Color;
            if let Some((r, g, b, a)) = parse_color_attr(value) {
                push_style(PropertyDeclaration::BackgroundColor(
                    Color::from_absolute_color(AbsoluteColor::srgb_legacy(r, g, b, a)),
                ));
            }
        }

        if *name == local_name!("hidden") {
            use style::values::specified::Display;
            push_style(PropertyDeclaration::Display(Display::None));
        }
    }

    if let Some(lang) = lang {
        push_style(PropertyDeclaration::XLang(XLang(Atom::from(lang))));
    }
}
