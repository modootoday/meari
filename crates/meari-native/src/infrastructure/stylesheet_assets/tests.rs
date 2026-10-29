use super::*;
use meari::domain::{
    assets::AssetDescriptor,
    build::{ContentHash, InputRole},
    design::stylesheet::StylesheetMode,
};

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
fn assets(files: &[(&str, InputRole, &str)]) -> AssetManifest {
    AssetManifest {
        schema_version: 1,
        files: files
            .iter()
            .map(|(name, role, mime)| {
                (
                    path(name),
                    AssetDescriptor {
                        role: role.clone(),
                        content: ContentHash::of(name.as_bytes()),
                        length: name.len() as u64,
                        media_type: (*mime).into(),
                        license: None,
                    },
                )
            })
            .collect(),
    }
}
fn project(entry: &str, files: &[(&str, &str)]) -> StylesheetProject {
    StylesheetProject {
        mode: StylesheetMode::Css,
        entry: entry.into(),
        files: files
            .iter()
            .map(|(name, css)| ((*name).into(), (*css).into()))
            .collect(),
        bridge_theme: false,
    }
}

#[test]
fn resolves_each_original_stylesheet_before_import_expansion_and_preserves_input() {
    let entry = "src/presentation/web_ui/styles/tokens.css";
    let original = project(
        entry,
        &[
            (
                entry,
                "@font-face{src:url('../../../../assets/fonts/body.ttf')} .a{mask:url('/assets/icons/control.svg')}",
            ),
            (
                "other/styles/theme.css",
                ".b{background:var(--image,url('../icons/control.svg'))}",
            ),
        ],
    );
    let before = original.clone();
    let manifest = assets(&[
        ("assets/fonts/body.ttf", InputRole::Font, "font/ttf"),
        ("assets/icons/control.svg", InputRole::Icon, "image/svg+xml"),
        ("other/icons/control.svg", InputRole::Image, "image/svg+xml"),
    ]);
    let output = rewrite(&original, &manifest).unwrap();
    assert_eq!(original, before);
    assert_eq!(output.references.len(), 3);
    for reference in &output.references {
        assert_eq!(
            reference.output,
            manifest.files[&reference.source].output_path().unwrap()
        );
        assert!(
            original.files[reference.file.as_str()]
                [reference.original_span.start..reference.original_span.end]
                .starts_with("url(")
        );
        let generated = &output.project.files[reference.file.as_str()]
            [reference.generated_span.start..reference.generated_span.end];
        assert!(generated.contains(reference.output.as_str()));
    }
    assert!(
        output
            .references
            .iter()
            .any(|reference| reference.source == path("other/icons/control.svg"))
    );
    assert!(
        output
            .references
            .iter()
            .any(|reference| reference.source == path("assets/fonts/body.ttf"))
    );
}

#[test]
fn css_escapes_nested_functions_and_query_aliases_keep_exact_source_ranges() {
    let css = "/* 한국어 */ @media (min-width: 1px){.a{mask:var(--fallback,url(../icons/arrow\\2d left.svg?rev=2));background:url(\"../icons/arrow-left.svg\")}}";
    let original = project("src/styles/a.css", &[("src/styles/a.css", css)]);
    let manifest = assets(&[("src/icons/arrow-left.svg", InputRole::Icon, "image/svg+xml")]);
    let output = rewrite(&original, &manifest).unwrap();
    assert_eq!(output.references.len(), 2);
    let first = &output.references[0];
    assert_eq!(first.original_url, "../icons/arrow-left.svg?rev=2");
    assert!(first.url.ends_with(".svg?rev=2"));
    assert_eq!(
        &css[first.original_span.start..first.original_span.end],
        "url(../icons/arrow\\2d left.svg?rev=2)"
    );
    assert_eq!(first.output, output.references[1].output);
}

#[test]
fn import_and_reference_urls_stay_owned_by_the_stylesheet_resolver() {
    for directive in ["import", "reference"] {
        let css =
            format!("@{directive} url('./other.css'); .a{{mask:url('../icons/control.svg')}}");
        let mut original = project(
            "styles/a.css",
            &[
                ("styles/a.css", &css),
                ("styles/other.css", ".b{color:red}"),
            ],
        );
        if directive == "reference" {
            original.mode = StylesheetMode::Tailwind;
        }
        let output = rewrite(
            &original,
            &assets(&[("icons/control.svg", InputRole::Icon, "image/svg+xml")]),
        )
        .unwrap();
        assert_eq!(output.references.len(), 1);
        assert!(
            output.project.files["styles/a.css"]
                .starts_with(&format!("@{directive} url('./other.css');"))
        );
    }
}

#[test]
fn image_set_literals_are_resources_while_type_and_content_strings_are_preserved() {
    let css = ".a{background:image-set('../icons/control.svg' type('image/svg+xml') 1x,url('../icons/control.svg') 2x);content:'url(not-a-resource.svg)'}";
    let original = project("styles/a.css", &[("styles/a.css", css)]);
    let output = rewrite(
        &original,
        &assets(&[("icons/control.svg", InputRole::Icon, "image/svg+xml")]),
    )
    .unwrap();
    assert_eq!(output.references.len(), 2);
    assert_eq!(
        &css[output.references[0].original_span.start..output.references[0].original_span.end],
        "'../icons/control.svg'"
    );
    assert!(output.project.files["styles/a.css"].contains("type('image/svg+xml')"));
    assert!(output.project.files["styles/a.css"].contains("content:'url(not-a-resource.svg)'"));
}

#[test]
fn missing_external_encoded_fragment_and_escape_urls_have_original_file_diagnostics() {
    let manifest = assets(&[("icons/control.svg", InputRole::Icon, "image/svg+xml")]);
    for url in [
        "missing.svg",
        "../../../outside.svg",
        "\\2e \\2e /\\2e \\2e /outside.svg",
        "https://example.test/icon.svg",
        "//example.test/icon.svg",
        "data:image/svg+xml,svg",
        "../icons/%63ontrol.svg",
        "../icons/control.svg#part",
        "#part",
    ] {
        let css = format!("/* 원본 */ .a{{mask:url('{url}')}}");
        let original = project("styles/a.css", &[("styles/a.css", &css)]);
        let before = original.clone();
        let error = rewrite(&original, &manifest).unwrap_err();
        assert_eq!(error.file, "styles/a.css", "{url}");
        assert!(error.diagnostic.span.start > 0, "{url}");
        assert!(
            css[error.diagnostic.span.start..error.diagnostic.span.end].starts_with("url("),
            "{url}"
        );
        assert_eq!(original, before);
    }
}

#[test]
fn block_depth_and_bad_tokens_fail_without_mutating_the_original_project() {
    let manifest = assets(&[]);
    for css in [
        format!(
            ".a{{color:{}red{}}}",
            "var(--x,".repeat(130),
            ")".repeat(130)
        ),
        ".a{background:url('broken\n')}".into(),
    ] {
        let original = project("styles/a.css", &[("styles/a.css", &css)]);
        assert!(rewrite(&original, &manifest).is_err());
    }
}
