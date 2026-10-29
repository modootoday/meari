//! Fixed scroll offsets: one `<selector> <x> <y>` line per scroll container, applied to both
//! renderers before capture so clipping and scrolled content can be compared at a known position.

#[derive(Clone, Debug, PartialEq)]
pub struct Offset {
    pub selector: String,
    pub x: f64,
    pub y: f64,
}

/// Lines starting with `//` are comments (`#` starts an id selector).
pub fn parse(text: &str) -> Result<Vec<Offset>, String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let [selector, x, y] = parts[..] else {
                return Err(format!(
                    "scroll line `{line}`: expected `<selector> <x> <y>`"
                ));
            };
            let number = |v: &str| v.parse::<f64>().map_err(|e| format!("scroll `{v}`: {e}"));
            Ok(Offset {
                selector: selector.to_owned(),
                x: number(x)?,
                y: number(y)?,
            })
        })
        .collect()
}

/// The same offsets as a script for the Web capture (`Element.scrollTo`, instant).
pub fn script(offsets: &[Offset]) -> String {
    offsets
        .iter()
        .map(|o| {
            format!(
                "document.querySelector({:?}).scrollTo({{left:{},top:{},behavior:'instant'}});",
                o.selector, o.x, o.y
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines_and_skips_comments() {
        let offsets = parse("// pinned\n[data-s=a] 0 40\n").unwrap();
        assert_eq!(
            offsets,
            vec![Offset {
                selector: "[data-s=a]".into(),
                x: 0.0,
                y: 40.0
            }]
        );
        assert!(parse("only-two 1").is_err());
        assert!(script(&offsets).contains("top:40"));
    }
}
