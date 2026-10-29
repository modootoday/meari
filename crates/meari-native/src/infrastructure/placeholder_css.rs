//! `::placeholder` projection.
//!
//! stylo treats `::placeholder` as a lazy pseudo-element that blitz never
//! cascades, so the placeholder color is projected onto the input itself as
//! `text-decoration-color` (meari-paint reads it only for placeholder text).
//! Original rules are kept; derived rules are appended.

const UA: &str = ":where(input,textarea){text-decoration-color:#757575}";
const HOST: &str = ":is(input,textarea)";

/// Append host rules carrying each `::placeholder` color.
pub fn project(css: &str) -> String {
    let derived = block(css);
    let mut out = String::with_capacity(UA.len() + css.len() + derived.len());
    out.push_str(UA);
    out.push_str(css);
    out.push_str(&derived);
    out
}

fn block(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(open) = find_top(rest, b'{') {
        let prelude = rest[..open].trim();
        let prelude = prelude.rsplit(';').next().unwrap_or("").trim();
        let close = matching(rest, open);
        let body = &rest[open + 1..close];
        if prelude.starts_with('@') {
            let inner = block(body);
            if !inner.is_empty() {
                out.push_str(prelude);
                out.push('{');
                out.push_str(&inner);
                out.push('}');
            }
        } else if prelude.contains("::placeholder") {
            rule(prelude, body, &mut out);
        }
        rest = &rest[(close + 1).min(rest.len())..];
    }
    out
}

fn rule(prelude: &str, body: &str, out: &mut String) {
    let selectors: Vec<String> = prelude
        .split(',')
        .filter(|s| s.contains("::placeholder"))
        .map(|s| s.trim().replace("::placeholder", HOST))
        .collect();
    let colors: Vec<&str> = body
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .filter(|(name, _)| name.trim() == "color")
        .map(|(_, value)| value.trim())
        .collect();
    if selectors.is_empty() || colors.is_empty() {
        return;
    }
    out.push_str(&selectors.join(","));
    out.push('{');
    for color in colors {
        out.push_str("text-decoration-color:");
        out.push_str(color);
        out.push(';');
    }
    out.push('}');
}

fn find_top(text: &str, needle: u8) -> Option<usize> {
    scan(text, 0, |depth, byte| depth == 0 && byte == needle)
}

fn matching(text: &str, open: usize) -> usize {
    let mut depth = 0usize;
    scan(text, open, |_, byte| {
        match byte {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        depth == 0
    })
    .unwrap_or(text.len())
}

/// Visit bytes outside strings and comments; parentheses raise `depth`.
fn scan(text: &str, from: usize, mut hit: impl FnMut(usize, u8) -> bool) -> Option<usize> {
    let bytes = text.as_bytes();
    let (mut i, mut parens) = (from, 0usize);
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = text[i + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |end| i + end + 3);
            }
            b'\\' => i += 1,
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            byte if hit(parens, byte) => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::project;

    #[test]
    fn projects_nested_placeholder_color() {
        let css = "a{color:red}@supports (x:y){::placeholder{opacity:1;color:currentColor}.p\\:a::placeholder,b{color:blue}}";
        let out = project(css);
        assert!(out.ends_with(
            "@supports (x:y){:is(input,textarea){text-decoration-color:currentColor;}.p\\:a:is(input,textarea){text-decoration-color:blue;}}"
        ), "{out}");
    }
}
