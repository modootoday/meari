//! Web path: the WebDocument adapter's DOM plan as static HTML, screenshotted by headless Chrome.
use meari::application::{
    binding,
    dom::{self, DomNode},
    state_program::StateProgram,
};
use std::{collections::BTreeMap, path::Path, process::Command};

/// Mirrors `WebDocument::mount_internal` up to `Frame::build` (attributes set verbatim, text nodes).
pub fn html(source: &str, css: &str, dark: bool) -> Result<String, String> {
    let component = meari::parse(source, meari::application::Language::Tsx)
        .map_err(|d| format!("parse: {}", d.message))?;
    // Same scope derivation as WebDocument::mount_internal: hex of "{len}:{id}:{epoch}".
    let (document_id, epoch) = ("visual", 1u64);
    let scope: String = format!("{}:{document_id}:{epoch}", document_id.len())
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    let bound = binding::bind_with_scope(&component, &BTreeMap::new(), Some(&scope))?;
    let program = StateProgram::mount(bound, document_id.into(), epoch)?;
    let projection = program.project()?;
    let plan = dom::prepare(&projection.component)?;
    let mut body = String::new();
    serialize(&plan, &mut body);
    Ok(format!(
        "<!doctype html><html class=\"{}\"><head><meta charset=\"utf-8\"></head><body><div data-meari-document=\"\" data-meari-scope=\"{scope}\"><style>{css}</style>{body}</div></body></html>",
        if dark { "dark" } else { "" }
    ))
}

fn escape(text: &str, attribute: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn serialize(node: &DomNode, out: &mut String) {
    match node {
        DomNode::Text(text) => out.push_str(&escape(text, false)),
        DomNode::Element {
            tag,
            attributes,
            children,
        } => {
            out.push('<');
            out.push_str(tag);
            for (name, value) in attributes {
                out.push_str(&format!(" {name}=\"{}\"", escape(value, true)));
            }
            out.push('>');
            const VOID: [&str; 6] = ["input", "img", "br", "hr", "col", "source"];
            if VOID.contains(&tag.as_str()) {
                return;
            }
            for child in children {
                serialize(child, out);
            }
            out.push_str(&format!("</{tag}>"));
        }
    }
}

/// `chrome --screenshot` never exits in the box sandbox, so a dependency-free Node CDP driver
/// (`scripts/cdp-screenshot.mjs`, `--remote-debugging-pipe`) captures the viewport instead.
pub fn screenshot(
    chrome: &str,
    html_path: &Path,
    png_path: &Path,
    scroll: &[crate::scroll::Offset],
) -> Result<Vec<u8>, String> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/cdp-screenshot.mjs");
    let output = Command::new("node")
        .env("TMPDIR", "/tmp") // .cargo/config.toml points TMPDIR outside the repo
        .arg(script)
        .arg(chrome)
        .arg(crate::host::file_url(html_path))
        .arg(png_path)
        .arg(crate::WIDTH.to_string())
        .arg(crate::HEIGHT.to_string())
        .arg(crate::scroll::script(scroll))
        .output()
        .map_err(|e| format!("node: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "web screenshot failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .trim()
                .replace('\n', " ")
        ));
    }
    crate::compare::read_png(png_path)
}
