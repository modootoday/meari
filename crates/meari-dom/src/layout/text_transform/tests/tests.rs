use super::*;
use parley::{FontContext, LayoutContext, TextStyle};

fn transform(kind: TextTransform, lang: &str, texts: &[&str]) -> Vec<String> {
    let transform = CaseTransform {
        text_transform: kind,
        lang: casing_language(LanguageIdentifier::try_from_str(lang).unwrap()),
        ..CaseTransform::NONE
    };
    with_builder(|builder, transformer| {
        texts
            .iter()
            .map(|text| push(builder, transformer, text, &transform))
            .collect()
    })
}

fn with_builder<R>(f: impl FnOnce(&mut TreeBuilder<'_, ()>, &mut TextTransformer) -> R) -> R {
    let mut font_ctx = FontContext::new();
    let mut layout_ctx = LayoutContext::new();
    let mut builder = layout_ctx.tree_builder(&mut font_ctx, 1.0, true, &TextStyle::default());
    f(&mut builder, &mut TextTransformer::default())
}

fn push(
    builder: &mut TreeBuilder<'_, ()>,
    transformer: &mut TextTransformer,
    text: &str,
    transform: &CaseTransform,
) -> String {
    let output = transformer.transform(text, transform, builder).to_owned();
    builder.push_text(&output);
    output
}

fn capitalize(texts: &[&str]) -> Vec<String> {
    transform(TextTransform::CAPITALIZE, "und", texts)
}

#[test]
fn capitalize_words() {
    assert_eq!(capitalize(&["hello world"]), ["Hello World"]);
    assert_eq!(capitalize(&["foo-bar"]), ["Foo-Bar"]);
    assert_eq!(capitalize(&["john's apple"]), ["John's Apple"]);
    assert_eq!(capitalize(&["foo_bar"]), ["Foo_bar"]);
    assert_eq!(capitalize(&["cancel·lar"]), ["Cancel·lar"]);
    assert_eq!(capitalize(&["un·e député·e"]), ["Un·e Député·e"]);
    assert_eq!(capitalize(&["3rd place"]), ["3rd Place"]);
    assert_eq!(capitalize(&["aBC"]), ["ABC"]);
    // Circled letters are symbols, not letters
    assert_eq!(capitalize(&["ⓐⓐⓐ ⓑⓑⓑ"]), ["ⓐⓐⓐ ⓑⓑⓑ"]);
}

#[test]
fn capitalize_skips_leading_punctuation() {
    assert_eq!(
        capitalize(&["({[-–\"«'.<?!transform"]),
        ["({[-–\"«'.<?!Transform"]
    );
}

#[test]
#[cfg(feature = "text-transform-icu")]
fn capitalize_uses_titlecase() {
    assert_eq!(capitalize(&["ǆǆ"]), ["ǅǆ"]);
    assert_eq!(capitalize(&["ßa"]), ["Ssa"]);
    assert_eq!(capitalize(&["ᾳᾳ"]), ["ᾼᾳ"]);
}

#[test]
fn capitalize_across_text_nodes() {
    assert_eq!(
        capitalize(&["hel", "lo ", "world"]),
        ["Hel", "lo ", "World"]
    );
    assert_eq!(capitalize(&["don", "'t"]), ["Don", "'t"]);
    assert_eq!(capitalize(&["(", "abc"]), ["(", "Abc"]);
    assert_eq!(
        capitalize(&["hello ", " ", "world"]),
        ["Hello ", " ", "World"]
    );
}

#[test]
fn capitalize_mid_word_text_node() {
    let capitalize = CaseTransform {
        text_transform: TextTransform::CAPITALIZE,
        lang: LanguageIdentifier::UNKNOWN,
        ..CaseTransform::NONE
    };
    with_builder(|builder, transformer| {
        assert_eq!(push(builder, transformer, "a", &CaseTransform::NONE), "a");
        assert_eq!(push(builder, transformer, "b", &capitalize), "b");
        assert_eq!(push(builder, transformer, "c", &CaseTransform::NONE), "c");
    });
}

#[test]
fn capitalize_after_word_break() {
    let capitalize = CaseTransform {
        text_transform: TextTransform::CAPITALIZE,
        lang: LanguageIdentifier::UNKNOWN,
        ..CaseTransform::NONE
    };
    with_builder(|builder, transformer| {
        assert_eq!(push(builder, transformer, "abc", &capitalize), "Abc");
        transformer.word_break(builder);
        assert_eq!(push(builder, transformer, "def", &capitalize), "Def");
    });
}

#[test]
fn capitalize_after_collapsed_whitespace() {
    assert_eq!(capitalize(&["hello ", "world"]), ["Hello ", "World"]);
    assert_eq!(capitalize(&["hello\n", "world"]), ["Hello\n", "World"]);
    assert_eq!(capitalize(&["hello ", "(world"]), ["Hello ", "(World"]);
    assert_eq!(
        capitalize(&["hello ", "\u{301}world"]),
        ["Hello ", "\u{301}World"]
    );
}

#[test]
fn capitalize_long_words() {
    let long_word = "a".repeat(100);
    let output = capitalize(&[&long_word, "bc de"]);
    assert_eq!(output[1], "bc De");
}

#[test]
#[cfg(feature = "text-transform-icu")]
fn capitalize_language_sensitive() {
    assert_eq!(
        transform(TextTransform::CAPITALIZE, "nl", &["ijsland"]),
        ["IJsland"]
    );
}

#[test]
fn unchanged_text_is_borrowed() {
    let cases = [
        (TextTransform::UPPERCASE, "ABC 123", true),
        (TextTransform::UPPERCASE, "ABc", false),
        (TextTransform::LOWERCASE, "abc 123", true),
        (TextTransform::LOWERCASE, "abC", false),
        (TextTransform::CAPITALIZE, "Hello World", true),
        (TextTransform::CAPITALIZE, "Hello world", false),
    ];
    with_builder(|builder, transformer| {
        for (kind, text, borrowed) in cases {
            let transform = CaseTransform {
                text_transform: kind,
                lang: LanguageIdentifier::UNKNOWN,
                ..CaseTransform::NONE
            };
            transformer.word_break(builder);
            let output = transformer.transform(text, &transform, builder);
            assert_eq!(output.as_ptr() == text.as_ptr(), borrowed, "{text:?}");
            builder.push_text(" ");
        }
    });
}

#[test]
#[cfg(feature = "text-transform-icu")]
fn ascii_fast_path_matches_icu() {
    let ascii: String = (0..128u8).map(char::from).collect();
    for lang in ["und", "en", "lt", "nl", "el", "tr", "az"] {
        let locale = LanguageIdentifier::try_from_str(lang).unwrap();
        assert_eq!(
            transform(TextTransform::UPPERCASE, lang, &[&ascii]),
            [CASE_MAPPER.uppercase_to_string(&ascii, &locale)],
            "{lang}"
        );
        assert_eq!(
            transform(TextTransform::LOWERCASE, lang, &[&ascii]),
            [CASE_MAPPER.lowercase_to_string(&ascii, &locale)],
            "{lang}"
        );
    }
}

#[test]
fn uppercase_and_lowercase() {
    assert_eq!(
        transform(TextTransform::UPPERCASE, "und", &["straße"]),
        ["STRASSE"]
    );
    assert_eq!(
        transform(TextTransform::LOWERCASE, "und", &["ABC"]),
        ["abc"]
    );
    assert_eq!(
        transform(TextTransform::LOWERCASE, "und", &["ΟΔΟΣ Σ"]),
        ["οδος σ"]
    );
}

#[test]
fn math_auto_italic_mappings() {
    // MathML Core, Appendix C.1.
    let mappings = [
        ("ABCDEFGHIJKLMNOPQRSTUVWXYZ", "𝐴𝐵𝐶𝐷𝐸𝐹𝐺𝐻𝐼𝐽𝐾𝐿𝑀𝑁𝑂𝑃𝑄𝑅𝑆𝑇𝑈𝑉𝑊𝑋𝑌𝑍"),
        ("abcdefghijklmnopqrstuvwxyz", "𝑎𝑏𝑐𝑑𝑒𝑓𝑔ℎ𝑖𝑗𝑘𝑙𝑚𝑛𝑜𝑝𝑞𝑟𝑠𝑡𝑢𝑣𝑤𝑥𝑦𝑧"),
        ("ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡϴΣΤΥΦΧΨΩ", "𝛢𝛣𝛤𝛥𝛦𝛧𝛨𝛩𝛪𝛫𝛬𝛭𝛮𝛯𝛰𝛱𝛲𝛳𝛴𝛵𝛶𝛷𝛸𝛹𝛺"),
        ("αβγδεζηθικλμνξοπρςστυφχψω", "𝛼𝛽𝛾𝛿𝜀𝜁𝜂𝜃𝜄𝜅𝜆𝜇𝜈𝜉𝜊𝜋𝜌𝜍𝜎𝜏𝜐𝜑𝜒𝜓𝜔"),
        ("ıȷ∇∂ϵϑϰϕϱϖ", "𝚤𝚥𝛻𝜕𝜖𝜗𝜘𝜙𝜚𝜛"),
    ];
    for (original, italic) in mappings {
        assert_eq!(original.chars().count(), italic.chars().count());
        for (input, expected) in original.chars().zip(italic.chars()) {
            assert_eq!(
                transform(
                    TextTransform::MATH_AUTO,
                    "und",
                    &[input.encode_utf8(&mut [0; 4])]
                ),
                [expected.to_string()],
                "U+{:04X}",
                input as u32,
            );
        }
    }
}

#[test]
fn math_auto_operates_on_each_text_node() {
    assert_eq!(
        transform(
            TextTransform::MATH_AUTO,
            "und",
            &["a", "b", "ab", "∂", "∂∇"]
        ),
        ["𝑎", "𝑏", "ab", "𝜕", "∂∇"],
    );
    assert_eq!(transform(TextTransform::MATH_AUTO, "tr", &["i"]), ["𝑖"]);
}

#[test]
fn math_auto_unchanged_text_is_borrowed() {
    let transform = CaseTransform {
        text_transform: TextTransform::MATH_AUTO,
        lang: LanguageIdentifier::UNKNOWN,
        ..CaseTransform::NONE
    };
    with_builder(|builder, transformer| {
        for text in [
            "", "ab", "a ", " a", "a\u{301}", "∂∇", "1", "+", "é", "\u{3A2}", "ℎ", "𝑎", "😀",
        ] {
            let output = transformer.transform(text, &transform, builder);
            assert_eq!(output, text);
            assert_eq!(output.as_ptr(), text.as_ptr());
        }
        assert_eq!(transformer.output.capacity(), 0);
    });
}

fn transform_with(
    text_transform: TextTransform,
    white_space_collapse: WhiteSpaceCollapse,
    texts: &[&str],
) -> Vec<String> {
    let transform = CaseTransform {
        text_transform,
        white_space_collapse,
        ..CaseTransform::NONE
    };
    with_builder(|builder, transformer| {
        texts
            .iter()
            .map(|text| push(builder, transformer, text, &transform))
            .collect()
    })
}

fn full_width_with(white_space_collapse: WhiteSpaceCollapse, text: &str) -> String {
    transform_with(TextTransform::FULL_WIDTH, white_space_collapse, &[text]).remove(0)
}

#[test]
fn full_width_mappings() {
    let ascii: String = ('!'..='~').collect();
    let wide: String = ('\u{FF01}'..='\u{FF5E}').collect();
    assert_eq!(full_width_with(WhiteSpaceCollapse::Collapse, &ascii), wide);
    // <wide> decompositions, reversed.
    assert_eq!(
        full_width_with(WhiteSpaceCollapse::Collapse, "¢£¬¯¦¥₩⦅⦆"),
        "￠￡￢￣￤￥￦｟｠"
    );
    // <narrow> decompositions.
    assert_eq!(
        full_width_with(WhiteSpaceCollapse::Collapse, "｡｢｣､･ｦｧｯｰｱﾝﾞﾟ"),
        "。「」、・ヲァッーアン\u{3099}\u{309A}"
    );
    assert_eq!(
        full_width_with(WhiteSpaceCollapse::Collapse, "\u{FFA0}ﾡﾾￂￇￊￏￒￗￚￜ"),
        "\u{3164}ㄱㅎㅏㅔㅕㅚㅛㅠㅡㅣ"
    );
    assert_eq!(
        full_width_with(WhiteSpaceCollapse::Collapse, "￨￩￪￫￬￭￮"),
        "│←↑→↓■○"
    );
    // Unassigned code points between the halfwidth Hangul blocks are left as is.
    assert_eq!(
        full_width_with(WhiteSpaceCollapse::Collapse, "\u{FFBF}\u{FFC8}\u{FFDD}"),
        "\u{FFBF}\u{FFC8}\u{FFDD}"
    );
    assert_eq!(
        full_width_with(WhiteSpaceCollapse::Collapse, "é\u{A0}あＡ"),
        "é\u{A0}あＡ"
    );
}

#[test]
fn full_width_preserved_spaces() {
    for collapse in [
        WhiteSpaceCollapse::Preserve,
        WhiteSpaceCollapse::BreakSpaces,
    ] {
        assert_eq!(
            full_width_with(collapse, " a  b\t\n"),
            "\u{3000}ａ\u{3000}\u{3000}ｂ\t\n"
        );
    }
}

#[test]
fn full_width_leaves_collapsible_whitespace() {
    use WhiteSpaceCollapse::{Collapse, PreserveBreaks};
    assert_eq!(full_width_with(Collapse, " a \n\t b "), " ａ \n\t ｂ ");
    assert_eq!(full_width_with(PreserveBreaks, "a \t\n b"), "ａ \t\n ｂ");
}

#[test]
fn full_size_kana_mappings() {
    let small = "ぁぃぅぇぉゕゖ𛄲っゃゅょゎ𛅐𛅑𛅒ァィゥェォヵㇰヶ𛅕ㇱㇲッㇳㇴㇵㇶㇷㇸㇹㇺャュョㇻㇼㇽㇾㇿヮ𛅤𛅥𛅦𛅧ｧｨｩｪｫｬｭｮｯ";
    let full = "あいうえおかけこつやゆよわゐゑをアイウエオカクケコシスツトヌハヒフヘホムヤユヨラリルレロワヰヱヲンｱｲｳｴｵﾔﾕﾖﾂ";
    assert_eq!(small.chars().count(), full.chars().count());
    let output = transform_with(
        TextTransform::FULL_SIZE_KANA,
        WhiteSpaceCollapse::Collapse,
        &[small],
    );
    assert_eq!(output, [full]);
}

#[test]
fn combined_transforms() {
    let collapse = WhiteSpaceCollapse::Collapse;
    let transform =
        |text_transform, text| transform_with(text_transform, collapse, &[text]).remove(0);
    assert_eq!(
        transform(
            TextTransform::UPPERCASE | TextTransform::FULL_WIDTH,
            "HELLO Transformed world"
        ),
        "ＨＥＬＬＯ ＴＲＡＮＳＦＯＲＭＥＤ ＷＯＲＬＤ"
    );
    assert_eq!(
        transform(
            TextTransform::CAPITALIZE | TextTransform::FULL_WIDTH,
            "HELLO Transformed world"
        ),
        "ＨＥＬＬＯ Ｔｒａｎｓｆｏｒｍｅｄ Ｗｏｒｌｄ"
    );
    assert_eq!(
        transform(
            TextTransform::UPPERCASE | TextTransform::FULL_WIDTH,
            "straße"
        ),
        "ＳＴＲＡＳＳＥ"
    );
    assert_eq!(
        transform(
            TextTransform::UPPERCASE | TextTransform::FULL_SIZE_KANA,
            "Katakana: ァィゥ"
        ),
        "KATAKANA: アイウ"
    );
    assert_eq!(
        transform(
            TextTransform::LOWERCASE | TextTransform::FULL_WIDTH | TextTransform::FULL_SIZE_KANA,
            "Hiragana: ぁぃ"
        ),
        "ｈｉｒａｇａｎａ： あい"
    );
    // full-width is applied before full-size-kana.
    assert_eq!(
        transform(
            TextTransform::FULL_WIDTH | TextTransform::FULL_SIZE_KANA,
            "ｧｯ"
        ),
        "アツ"
    );
}

#[test]
fn width_transforms_unchanged_text_is_borrowed() {
    let cases = [
        (TextTransform::FULL_SIZE_KANA, "abc あア ｱ"),
        (TextTransform::FULL_WIDTH, "ＡＢ\u{3000}あ"),
        (TextTransform::FULL_WIDTH, "  ＡＢ  "),
        (
            TextTransform::UPPERCASE | TextTransform::FULL_SIZE_KANA,
            "ABC",
        ),
        (TextTransform::UPPERCASE | TextTransform::FULL_WIDTH, "ＡＢ"),
    ];
    with_builder(|builder, transformer| {
        for (text_transform, text) in cases {
            let transform = CaseTransform {
                text_transform,
                ..CaseTransform::NONE
            };
            let output = transformer.transform(text, &transform, builder);
            assert_eq!(output, text);
            assert_eq!(output.as_ptr(), text.as_ptr(), "{text:?}");
        }
        assert_eq!(transformer.output.capacity(), 0);
    });
}

#[test]
#[cfg(feature = "text-transform-icu")]
fn uppercase_and_lowercase_language_sensitive() {
    assert_eq!(transform(TextTransform::UPPERCASE, "tr", &["i"]), ["İ"]);
    assert_eq!(transform(TextTransform::LOWERCASE, "tr", &["I"]), ["ı"]);
    assert_eq!(
        transform(TextTransform::LOWERCASE, "tr-Latn", &["I"]),
        ["ı"]
    );
    assert_eq!(
        transform(TextTransform::LOWERCASE, "tr-Cyrl", &["I"]),
        ["i"]
    );
}
