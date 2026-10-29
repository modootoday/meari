use cssparser::{Parser, Token};

pub(super) fn validate(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("SVG decode byte budget exceeded".into());
    }
    let source = std::str::from_utf8(bytes).map_err(|_| "SVG requires UTF-8 source")?;
    let document = roxmltree::Document::parse_with_options(
        source,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 65536,
            ..Default::default()
        },
    )
    .map_err(|error| format!("SVG XML decode failed: {error}"))?;
    let root = document.root_element();
    if root.tag_name().name() != "svg"
        || root
            .tag_name()
            .namespace()
            .is_some_and(|namespace| namespace != "http://www.w3.org/2000/svg")
    {
        return Err("SVG asset requires an SVG root".into());
    }
    for node in document.descendants() {
        if node.is_pi() {
            return Err("SVG processing instructions require a declared adapter".into());
        }
        if !node.is_element() {
            continue;
        }
        if ["script", "foreignObject"].contains(&node.tag_name().name()) {
            return Err("active SVG content is not a declared image resource".into());
        }
        if node.tag_name().name() == "style" {
            for child in node.children().filter(|child| child.is_text()) {
                css(&mut Parser::new(child.text().unwrap_or_default()), 0)?;
            }
        }
        for attribute in node.attributes() {
            if attribute.name().to_ascii_lowercase().starts_with("on") {
                return Err("SVG event attributes are not image resources".into());
            }
            if attribute.name() == "href"
                && (!attribute.value().trim().starts_with('#')
                    || ["image", "feImage"].contains(&node.tag_name().name()))
            {
                return Err(
                    "SVG embedded image references require an explicit declared-resource adapter"
                        .into(),
                );
            }
            // Covers CSS paint servers and inline style, including escaped url().
            css(&mut Parser::new(attribute.value()), 0)?;
        }
    }
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_string: Box::new(|_, _| None),
            resolve_data: Box::new(|_, _, _| None),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_data(bytes, &options)
        .map_err(|error| format!("SVG geometry decode failed: {error}"))?;
    if tree.size().width() > 8192.0 || tree.size().height() > 8192.0 {
        return Err("SVG image dimensions exceed render budget".into());
    }
    Ok(())
}

fn css(parser: &mut Parser<'_>, depth: usize) -> Result<(), String> {
    if depth > 128 {
        return Err("SVG CSS depth budget exceeded".into());
    }
    while !parser.is_exhausted() {
        let token = parser
            .next_including_whitespace_and_comments()
            .map_err(|error| format!("SVG CSS token: {error:?}"))?
            .clone();
        match token {
            Token::UnquotedUrl(url) if !url.starts_with('#') => {
                return Err("SVG CSS resource reference is not a local fragment".into());
            }
            Token::Function(name) if name.eq_ignore_ascii_case("url") => {
                let url = parser
                    .parse_nested_block(|p| {
                        let url = p.expect_string_cloned()?.to_string();
                        p.expect_exhausted()?;
                        Ok::<_, cssparser::ParseError<()>>(url)
                    })
                    .map_err(|error| format!("SVG CSS URL: {error:?}"))?;
                if !url.starts_with('#') {
                    return Err("SVG CSS resource reference is not a local fragment".into());
                }
            }
            Token::AtKeyword(name) if name.eq_ignore_ascii_case("import") => {
                return Err("SVG stylesheet imports require a declared adapter".into());
            }
            Token::CurlyBracketBlock
            | Token::SquareBracketBlock
            | Token::ParenthesisBlock
            | Token::Function(_) => {
                parser
                    .parse_nested_block(|p| {
                        css(p, depth + 1).map_err(cssparser::ParseError::<String>::custom)
                    })
                    .map_err(|error| format!("SVG CSS block: {error:?}"))?;
            }
            Token::BadString(_) | Token::BadUrl(_) => {
                return Err("SVG CSS value is malformed".into());
            }
            _ => {}
        }
    }
    Ok(())
}
