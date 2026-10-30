use super::*;

#[cfg(feature = "text-transform-icu")]
pub(super) const CASE_MAPPER: CaseMapperBorrowed<'static> = CaseMapper::new();

#[cfg(feature = "text-transform-icu")]
pub(super) const TITLECASE_MAPPER: TitlecaseMapperBorrowed<'static> = TitlecaseMapper::new();

pub(super) const WORD_SEGMENTER: WordSegmenterBorrowed<'static> =
    WordSegmenter::new_for_non_complex_scripts(WordBreakInvariantOptions::default());

pub(super) const GENERAL_CATEGORY: CodePointMapDataBorrowed<'static, GeneralCategory> =
    CodePointMapData::<GeneralCategory>::new();

pub(super) const WIDTH_TRANSFORMS: TextTransform =
    TextTransform::FULL_WIDTH.union(TextTransform::FULL_SIZE_KANA);

/// The maximum number of bytes of preceding text kept as context for finding word boundaries.
pub(super) const MAX_CONTEXT_LEN: usize = 32;

impl CaseTransform {
    pub(crate) const NONE: Self = Self {
        text_transform: TextTransform::NONE,
        white_space_collapse: WhiteSpaceCollapse::Collapse,
        lang: LanguageIdentifier::UNKNOWN,
    };

    pub(crate) fn from_style(style: &ComputedValues) -> Self {
        let text_transform = style.clone_text_transform();
        if text_transform.is_empty() {
            return Self::NONE;
        }
        let white_space_collapse = style.clone_white_space_collapse();
        let lang = match text_transform.case() {
            TextTransformCase::None | TextTransformCase::MathAuto => LanguageIdentifier::UNKNOWN,
            _ => LanguageIdentifier::try_from_str(&style.get_font()._x_lang.0)
                .map(casing_language)
                .unwrap_or(LanguageIdentifier::UNKNOWN),
        };
        Self {
            text_transform,
            white_space_collapse,
            lang,
        }
    }

    /// Whether the language has case mappings for ASCII characters that differ from the
    /// default ones (`i` ↔ `İ` and `ı` ↔ `I`).
    pub(super) fn has_turkic_casing(&self) -> bool {
        cfg!(feature = "text-transform-icu") && matches!(self.lang.language.as_str(), "tr" | "az")
    }
}

/// Language-specific case mapping rules don't apply if an explicit script subtag contradicts
/// the language's script (e.g. `tr-Cyrl`).
///
/// <https://drafts.csswg.org/css-text-3/#script-tagging>
pub(super) fn casing_language(lang: LanguageIdentifier) -> LanguageIdentifier {
    let Some(script) = lang.script else {
        return lang;
    };
    // The languages with tailored case mappings in ICU4X.
    let language_script = match lang.language.as_str() {
        "az" | "lt" | "nl" | "tr" => "Latn",
        "el" => "Grek",
        "hy" => "Armn",
        _ => return lang,
    };
    if script.as_str() == language_script {
        lang
    } else {
        LanguageIdentifier::UNKNOWN
    }
}

impl TextTransformer {
    /// Forces a word boundary (e.g. at an atomic inline or a forced line break).
    pub(crate) fn word_break<B: Brush>(&mut self, builder: &TreeBuilder<'_, B>) {
        self.context_start = builder.text().len();
    }

    /// Transforms the content of a text node that is about to be pushed to `builder`.
    #[inline]
    pub(crate) fn transform<'a, B: Brush>(
        &'a mut self,
        text: &'a str,
        transform: &CaseTransform,
        builder: &TreeBuilder<'_, B>,
    ) -> &'a str {
        if transform.text_transform.is_empty() {
            return text;
        }
        self.transform_nonempty(text, transform, builder)
    }

    pub(super) fn transform_nonempty<'a, B: Brush>(
        &'a mut self,
        text: &'a str,
        transform: &CaseTransform,
        builder: &TreeBuilder<'_, B>,
    ) -> &'a str {
        let case = transform.text_transform.case();
        if text.is_ascii()
            && !transform.has_turkic_casing()
            && !transform.text_transform.contains(TextTransform::FULL_WIDTH)
        {
            match case {
                TextTransformCase::Uppercase => {
                    return map_ascii(
                        text,
                        &mut self.output,
                        u8::is_ascii_lowercase,
                        str::make_ascii_uppercase,
                    );
                }
                TextTransformCase::Lowercase => {
                    return map_ascii(
                        text,
                        &mut self.output,
                        u8::is_ascii_uppercase,
                        str::make_ascii_lowercase,
                    );
                }
                TextTransformCase::None => return text,
                _ => {}
            }
        }

        let mut output = OutputSink::new(text, &mut self.output);
        output.width = transform.text_transform & WIDTH_TRANSFORMS;
        output.white_space_collapse = transform.white_space_collapse;
        match case {
            TextTransformCase::None => output.push_str(text),
            TextTransformCase::Uppercase => uppercase(text, &transform.lang, &mut output),
            TextTransformCase::Lowercase => lowercase(text, &transform.lang, &mut output),
            TextTransformCase::MathAuto => math_auto(text, &mut output),
            TextTransformCase::Capitalize => {
                // Pending (collapsed) whitespace always ends the preceding word, so no
                // context is needed.
                let context = if builder.has_pending_whitespace() {
                    ""
                } else {
                    let preceding = &builder.text()[self.context_start..];
                    let start = ceil_char_boundary(
                        preceding,
                        preceding.len().saturating_sub(MAX_CONTEXT_LEN),
                    );
                    &preceding[start..]
                };
                capitalize(
                    context,
                    text,
                    &transform.lang,
                    &mut self.segmentation_buffer,
                    &mut output,
                );
            }
        }
        output.finish()
    }
}

/// Maps single-character text nodes to mathematical italic characters.
/// <https://w3c.github.io/mathml-core/#italic-mappings>
pub(super) fn math_auto(text: &str, output: &mut OutputSink<'_>) {
    let mut chars = text.chars();
    let Some(c) = chars.next().filter(|_| chars.next().is_none()) else {
        output.push_str(text);
        return;
    };
    let codepoint = match c {
        'A'..='Z' => c as u32 + 0x1D3F3,
        'h' => 0x210E,
        'a'..='z' => c as u32 + 0x1D3ED,
        '\u{131}' => 0x1D6A4,
        '\u{237}' => 0x1D6A5,
        '\u{391}'..='\u{3A1}' | '\u{3A3}'..='\u{3A9}' => c as u32 + 0x1D351,
        '\u{3B1}'..='\u{3C9}' => c as u32 + 0x1D34B,
        '\u{3F4}' => 0x1D6F3,
        '\u{2207}' => 0x1D6FB,
        '\u{2202}' => 0x1D715,
        '\u{3F5}' => 0x1D716,
        '\u{3D1}' => 0x1D717,
        '\u{3F0}' => 0x1D718,
        '\u{3D5}' => 0x1D719,
        '\u{3F1}' => 0x1D71A,
        '\u{3D6}' => 0x1D71B,
        _ => {
            output.push_str(text);
            return;
        }
    };
    let mapped = char::from_u32(codepoint).unwrap();
    output.push_str(mapped.encode_utf8(&mut [0; 4]));
}

/// Whether Parley collapses `c` with the given `white-space-collapse`.
pub(super) fn is_collapsible(white_space_collapse: WhiteSpaceCollapse, c: char) -> bool {
    match white_space_collapse {
        WhiteSpaceCollapse::Collapse => c.is_ascii_whitespace(),
        WhiteSpaceCollapse::PreserveBreaks => matches!(c, ' ' | '\t'),
        WhiteSpaceCollapse::Preserve | WhiteSpaceCollapse::BreakSpaces => false,
    }
}

/// Maps `c` to its full-width form: the reverse of its `<wide>` decomposition mapping or its
/// `<narrow>` decomposition mapping, if any.
/// <https://drafts.csswg.org/css-text-3/#text-transform-mapping>
pub fn full_width(c: char) -> char {
    const HALFWIDTH_KATAKANA: [char; 63] = [
        '\u{3002}', '\u{300C}', '\u{300D}', '\u{3001}', '\u{30FB}', '\u{30F2}', '\u{30A1}',
        '\u{30A3}', '\u{30A5}', '\u{30A7}', '\u{30A9}', '\u{30E3}', '\u{30E5}', '\u{30E7}',
        '\u{30C3}', '\u{30FC}', '\u{30A2}', '\u{30A4}', '\u{30A6}', '\u{30A8}', '\u{30AA}',
        '\u{30AB}', '\u{30AD}', '\u{30AF}', '\u{30B1}', '\u{30B3}', '\u{30B5}', '\u{30B7}',
        '\u{30B9}', '\u{30BB}', '\u{30BD}', '\u{30BF}', '\u{30C1}', '\u{30C4}', '\u{30C6}',
        '\u{30C8}', '\u{30CA}', '\u{30CB}', '\u{30CC}', '\u{30CD}', '\u{30CE}', '\u{30CF}',
        '\u{30D2}', '\u{30D5}', '\u{30D8}', '\u{30DB}', '\u{30DE}', '\u{30DF}', '\u{30E0}',
        '\u{30E1}', '\u{30E2}', '\u{30E4}', '\u{30E6}', '\u{30E8}', '\u{30E9}', '\u{30EA}',
        '\u{30EB}', '\u{30EC}', '\u{30ED}', '\u{30EF}', '\u{30F3}', '\u{3099}', '\u{309A}',
    ];
    let codepoint = match c {
        ' ' => 0x3000,
        '!'..='~' => c as u32 + 0xFEE0,
        '\u{A2}' => 0xFFE0,
        '\u{A3}' => 0xFFE1,
        '\u{A5}' => 0xFFE5,
        '\u{A6}' => 0xFFE4,
        '\u{AC}' => 0xFFE2,
        '\u{AF}' => 0xFFE3,
        '\u{20A9}' => 0xFFE6,
        '\u{2985}' => 0xFF5F,
        '\u{2986}' => 0xFF60,
        '\u{FF61}'..='\u{FF9F}' => return HALFWIDTH_KATAKANA[c as usize - 0xFF61],
        // Halfwidth Hangul
        '\u{FFA0}' => 0x3164,
        '\u{FFA1}'..='\u{FFBE}' => c as u32 - 0xCE70,
        '\u{FFC2}'..='\u{FFC7}' => c as u32 - 0xCE73,
        '\u{FFCA}'..='\u{FFCF}' => c as u32 - 0xCE75,
        '\u{FFD2}'..='\u{FFD7}' => c as u32 - 0xCE77,
        '\u{FFDA}'..='\u{FFDC}' => c as u32 - 0xCE79,
        '\u{FFE8}' => 0x2502,
        '\u{FFE9}'..='\u{FFEC}' => c as u32 - 0xDE59,
        '\u{FFED}' => 0x25A0,
        '\u{FFEE}' => 0x25CB,
        _ => return c,
    };
    char::from_u32(codepoint).unwrap()
}

/// Maps small kana to full-size kana.
/// <https://drafts.csswg.org/css-text-3/#small-kana>
pub fn full_size_kana(c: char) -> char {
    const SMALL_KATAKANA_EXTENSIONS: [char; 16] = [
        '\u{30AF}', '\u{30B7}', '\u{30B9}', '\u{30C8}', '\u{30CC}', '\u{30CF}', '\u{30D2}',
        '\u{30D5}', '\u{30D8}', '\u{30DB}', '\u{30E0}', '\u{30E9}', '\u{30EA}', '\u{30EB}',
        '\u{30EC}', '\u{30ED}',
    ];
    let codepoint = match c {
        '\u{3041}' | '\u{3043}' | '\u{3045}' | '\u{3047}' | '\u{3049}' | '\u{3063}'
        | '\u{3083}' | '\u{3085}' | '\u{3087}' | '\u{308E}' | '\u{30A1}' | '\u{30A3}'
        | '\u{30A5}' | '\u{30A7}' | '\u{30A9}' | '\u{30C3}' | '\u{30E3}' | '\u{30E5}'
        | '\u{30E7}' | '\u{30EE}' => c as u32 + 1,
        '\u{3095}' => 0x304B,
        '\u{3096}' => 0x3051,
        '\u{30F5}' => 0x30AB,
        '\u{30F6}' => 0x30B1,
        '\u{31F0}'..='\u{31FF}' => return SMALL_KATAKANA_EXTENSIONS[c as usize - 0x31F0],
        '\u{FF67}'..='\u{FF6B}' => c as u32 + 10,
        '\u{FF6C}' => 0xFF94,
        '\u{FF6D}' => 0xFF95,
        '\u{FF6E}' => 0xFF96,
        '\u{FF6F}' => 0xFF82,
        '\u{1B132}' => 0x3053,
        '\u{1B150}'..='\u{1B152}' => c as u32 - 0x180C0,
        '\u{1B155}' => 0x30B3,
        '\u{1B164}'..='\u{1B167}' => c as u32 - 0x18074,
        _ => return c,
    };
    char::from_u32(codepoint).unwrap()
}

/// Case maps ASCII `text`, borrowing it if no bytes need mapping and otherwise copying it into
/// `buffer`.
pub(super) fn map_ascii<'a>(
    text: &'a str,
    buffer: &'a mut String,
    needs_mapping: impl Fn(&u8) -> bool,
    map: impl FnOnce(&mut str),
) -> &'a str {
    let Some(first) = text.bytes().position(|b| needs_mapping(&b)) else {
        return text;
    };
    buffer.clear();
    buffer.push_str(text);
    map(&mut buffer[first..]);
    buffer
}

impl<'a> OutputSink<'a> {
    pub(super) fn new(original: &'a str, buffer: &'a mut String) -> Self {
        Self {
            original,
            unchanged_len: Some(0),
            buffer,
            width: TextTransform::NONE,
            white_space_collapse: WhiteSpaceCollapse::Collapse,
        }
    }

    pub(super) fn push_str(&mut self, s: &str) {
        if self.width.is_empty() {
            self.push_unmapped(s);
        } else {
            s.chars().for_each(|c| self.push_char(c));
        }
    }

    /// Pushes `c` after applying the `full-width` and `full-size-kana` mappings.
    ///
    /// Text transforms apply after white space collapsing, so collapsible white space is left
    /// for Parley to collapse.
    // TODO: collapsed spaces should become U+3000 for `full-width`, which needs Parley support.
    pub(super) fn push_char(&mut self, mut c: char) {
        if self.width.contains(TextTransform::FULL_WIDTH)
            && !is_collapsible(self.white_space_collapse, c)
        {
            c = full_width(c);
        }
        if self.width.contains(TextTransform::FULL_SIZE_KANA) {
            c = full_size_kana(c);
        }
        self.push_unmapped(c.encode_utf8(&mut [0; 4]));
    }

    #[inline(always)]
    pub(super) fn push_unmapped(&mut self, s: &str) {
        if let Some(len) = self.unchanged_len {
            if self.original[len..].starts_with(s) {
                self.unchanged_len = Some(len + s.len());
                return;
            }
            self.buffer.clear();
            self.buffer.push_str(&self.original[..len]);
            self.unchanged_len = None;
        }
        self.buffer.push_str(s);
    }

    #[cfg(feature = "text-transform-icu")]
    pub(super) fn write(&mut self, writeable: impl Writeable) {
        // Writing to an `OutputSink` is infallible.
        let _ = writeable.write_to(self);
    }

    pub(super) fn finish(self) -> &'a str {
        match self.unchanged_len {
            Some(len) => &self.original[..len],
            None => self.buffer,
        }
    }
}

#[cfg(feature = "text-transform-icu")]
impl std::fmt::Write for OutputSink<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

/// Writes `text` to `output`, titlecasing the first typographic letter unit of each word and
/// leaving other characters unchanged. `context` is the text preceding `text` in the same word
/// segmentation context. `buffer` is scratch space.
pub(super) fn capitalize(
    context: &str,
    text: &str,
    lang: &LanguageIdentifier,
    buffer: &mut String,
    output: &mut OutputSink<'_>,
) {
    let context_len = context.len();
    let combined = if context.is_empty() {
        text
    } else {
        buffer.clear();
        buffer.push_str(context);
        buffer.push_str(text);
        buffer
    };

    let mut segment_start = 0;
    for segment_end in WORD_SEGMENTER.segment_str(combined).skip(1) {
        if segment_end > context_len {
            let part_start = segment_start.max(context_len);
            let part = &combined[part_start..segment_end];

            // A word that started in a previous text node has already had its first
            // letter unit (if any) transformed (or not) as part of that text node.
            let first_letter_already_seen = combined[segment_start..part_start]
                .chars()
                .any(is_typographic_letter_unit);

            match part.find(is_typographic_letter_unit) {
                Some(letter_start) if !first_letter_already_seen => {
                    output.push_str(&part[..letter_start]);
                    titlecase_segment(&part[letter_start..], lang, output);
                }
                _ => output.push_str(part),
            }
        }
        segment_start = segment_end;
    }
}

#[cfg(feature = "text-transform-icu")]
pub(super) fn uppercase(text: &str, lang: &LanguageIdentifier, output: &mut OutputSink<'_>) {
    output.write(CASE_MAPPER.uppercase(text, lang));
}

#[cfg(feature = "text-transform-icu")]
pub(super) fn lowercase(text: &str, lang: &LanguageIdentifier, output: &mut OutputSink<'_>) {
    output.write(CASE_MAPPER.lowercase(text, lang));
}

/// Titlecases the first character of `text`, which must start with a typographic letter unit,
/// leaving the rest unchanged.
#[cfg(feature = "text-transform-icu")]
pub(super) fn titlecase_segment(
    text: &str,
    lang: &LanguageIdentifier,
    output: &mut OutputSink<'_>,
) {
    let mut options = TitlecaseOptions::default();
    options.leading_adjustment = Some(LeadingAdjustment::None);
    options.trailing_case = Some(TrailingCase::Unchanged);
    output.write(TITLECASE_MAPPER.titlecase_segment(text, lang, options));
}

#[cfg(not(feature = "text-transform-icu"))]
pub(super) fn uppercase(text: &str, _lang: &LanguageIdentifier, output: &mut OutputSink<'_>) {
    for c in text.chars() {
        push_chars(output, c.to_uppercase());
    }
}

#[cfg(not(feature = "text-transform-icu"))]
pub(super) fn lowercase(text: &str, _lang: &LanguageIdentifier, output: &mut OutputSink<'_>) {
    // Only `str::to_lowercase` implements the context-sensitive final sigma rule.
    if text.contains('Σ') {
        output.push_str(&text.to_lowercase());
        return;
    }
    for c in text.chars() {
        push_chars(output, c.to_lowercase());
    }
}

/// Approximates titlecasing with uppercasing, as the standard library has no titlecase mapping.
#[cfg(not(feature = "text-transform-icu"))]
pub(super) fn titlecase_segment(
    text: &str,
    _lang: &LanguageIdentifier,
    output: &mut OutputSink<'_>,
) {
    let mut chars = text.chars();
    if let Some(first) = chars.next() {
        push_chars(output, first.to_uppercase());
    }
    output.push_str(chars.as_str());
}

#[cfg(not(feature = "text-transform-icu"))]
pub(super) fn push_chars(output: &mut OutputSink<'_>, chars: impl Iterator<Item = char>) {
    for c in chars {
        output.push_str(c.encode_utf8(&mut [0; 4]));
    }
}

/// <https://drafts.csswg.org/css-text/#typographic-letter-unit>
pub(super) fn is_typographic_letter_unit(c: char) -> bool {
    let category = GENERAL_CATEGORY.get(c);
    GeneralCategoryGroup::Letter.contains(category)
        || GeneralCategoryGroup::Number.contains(category)
}

pub(super) fn ceil_char_boundary(s: &str, mut index: usize) -> usize {
    while !s.is_char_boundary(index) {
        index += 1;
    }
    index
}
