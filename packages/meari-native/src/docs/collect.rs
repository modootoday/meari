use super::{model::*, write_json};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = vec![];
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("upstream symlinks are not followed".into());
        }
        if kind.is_dir() {
            found.extend(files(&entry.path())?);
        } else if kind.is_file() {
            found.push(entry.path());
        }
    }
    found.sort();
    Ok(found)
}

fn relative(path: &Path, root: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .map_err(|e| e.to_string())
}

fn revision(root: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| e.to_string())?;
    let revision = String::from_utf8(output.stdout)
        .map_err(|e| e.to_string())?
        .trim()
        .to_owned();
    if !output.status.success()
        || revision.len() != 40
        || !revision.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("upstream must be a pinned Git checkout".into());
    }
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .map_err(|e| e.to_string())?;
    if !status.status.success() || !status.stdout.is_empty() {
        return Err("upstream checkout has tracked modifications".into());
    }
    Ok(revision)
}

fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn save(output: &Path, name: &str, code: &str) -> Result<(Option<String>, Option<String>), String> {
    fs::write(output.join(name), code).map_err(|e| e.to_string())?;
    Ok((Some(name.into()), Some(digest(code))))
}

pub fn collect(shadcn: &Path, tailwind: &Path, output: &Path) -> Result<(), String> {
    let shadcn_revision = revision(shadcn)?;
    let tailwind_revision = revision(tailwind)?;
    let license = read(&shadcn.join("LICENSE.md"))?;
    if !license.starts_with("MIT License") {
        return Err("review changed shadcn license".into());
    }
    let tailwind_readme = read(&tailwind.join("README.md"))?;
    if !tailwind_readme.contains("not licensed under an open-source license") {
        return Err("review changed Tailwind documentation license before collection".into());
    }
    fs::create_dir(output).map_err(|e| e.to_string())?;
    fs::create_dir(output.join("sources")).map_err(|e| e.to_string())?;
    fs::write(output.join("SHADCN-LICENSE.txt"), license).map_err(|e| e.to_string())?;
    let mut catalog = Catalog {
        schema: 1,
        sources: vec![
            Source {
                project: "shadcn".into(),
                revision: shadcn_revision,
                repository: "https://github.com/shadcn-ui/ui".into(),
                license: "MIT; SHADCN-LICENSE.txt".into(),
            },
            Source {
                project: "tailwind".into(),
                revision: tailwind_revision,
                repository: "https://github.com/tailwindlabs/tailwindcss.com".into(),
                license:
                    "No open-source license: metadata only; no upstream code or prose redistributed"
                        .into(),
            },
        ],
        pages: vec![],
        examples: vec![],
    };
    let reference =
        Regex::new(r"(?s)<Component(Preview|Source)\b([^>]*?)/?>").map_err(|e| e.to_string())?;
    let name = Regex::new(r#"\bname="([^"]+)""#).map_err(|e| e.to_string())?;
    let source_path = Regex::new(r#"\bsrc="([^"]+)""#).map_err(|e| e.to_string())?;
    let preview = Regex::new(r"<Example(?:\s|>)").map_err(|e| e.to_string())?;
    for (project, root, docs) in [
        ("shadcn", shadcn, "apps/v4/content/docs/components"),
        ("tailwind", tailwind, "src/docs"),
    ] {
        for file in files(&root.join(docs))?
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "mdx"))
        {
            let text = read(&file)?;
            let path = relative(&file, root)?;
            let short = relative(&file, &root.join(docs))?
                .trim_end_matches(".mdx")
                .to_owned();
            let provider = if project == "shadcn" {
                short.split('/').next().unwrap_or("shared")
            } else {
                "css"
            };
            let page = catalog.pages.len();
            catalog.pages.push(Page {
                project: project.into(),
                path,
                url: format!(
                    "https://{}/docs/{}{}",
                    if project == "shadcn" {
                        "ui.shadcn.com"
                    } else {
                        "tailwindcss.com"
                    },
                    if project == "shadcn" {
                        "components/"
                    } else {
                        ""
                    },
                    short
                ),
                sha256: digest(&text),
                provider: provider.into(),
            });
            for (line, language, code) in fences(&text)? {
                let id = catalog.examples.len();
                let (source, sha256) = if project == "shadcn" {
                    save(output, &format!("sources/{id}.txt"), &code)?
                } else {
                    (None, None)
                };
                catalog.examples.push(Example {
                    id,
                    page,
                    line,
                    kind: "fence".into(),
                    language,
                    name: format!("block-{line}"),
                    source,
                    sha256,
                    collection: if project == "shadcn" {
                        "collected"
                    } else {
                        "license-metadata-only"
                    }
                    .into(),
                });
            }
            if project == "shadcn" {
                for found in reference.captures_iter(&text) {
                    let matched = found.get(0).ok_or("missing match")?;
                    let line = text[..matched.start()]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                        + 1;
                    let mut referenced = name
                        .captures(&found[2])
                        .and_then(|m| m.get(1))
                        .map(|m| m.as_str())
                        .unwrap_or("");
                    if referenced.contains(['/', '\\']) || referenced.contains("..") {
                        return Err("unsafe reference name".into());
                    }
                    let candidate = if let Some(source) = source_path.captures(&found[2]) {
                        let source = source.get(1).ok_or("missing src")?.as_str();
                        let source = source
                            .strip_prefix('/')
                            .ok_or("absolute upstream src required")?;
                        if source.contains('\\')
                            || Path::new(source)
                                .components()
                                .any(|c| !matches!(c, std::path::Component::Normal(_)))
                        {
                            return Err("unsafe src path".into());
                        }
                        referenced = source;
                        root.join("apps/v4").join(source)
                    } else if &found[1] == "Preview" {
                        root.join(format!("apps/v4/examples/{provider}/{referenced}.tsx"))
                    } else {
                        root.join(format!(
                            "apps/v4/registry/bases/{provider}/ui/{referenced}.tsx"
                        ))
                    };
                    let id = catalog.examples.len();
                    let (source, sha256, collection) = if candidate.is_file() {
                        let code = read(&candidate)?;
                        let original = relative(&candidate, root)?;
                        let target = output.join("upstream").join(&original);
                        fs::create_dir_all(target.parent().ok_or("missing parent")?)
                            .map_err(|e| e.to_string())?;
                        fs::write(target, &code).map_err(|e| e.to_string())?;
                        (
                            Some(format!("upstream/{original}")),
                            Some(digest(code)),
                            "collected",
                        )
                    } else {
                        (None, None, "unresolved-reference")
                    };
                    catalog.examples.push(Example {
                        id,
                        page,
                        line,
                        kind: format!("Component{}", &found[1]),
                        language: "tsx".into(),
                        name: referenced.into(),
                        source,
                        sha256,
                        collection: collection.into(),
                    });
                }
            } else {
                for found in preview.find_iter(&text) {
                    let id = catalog.examples.len();
                    catalog.examples.push(Example {
                        id,
                        page,
                        line: text[..found.start()]
                            .bytes()
                            .filter(|b| *b == b'\n')
                            .count()
                            + 1,
                        kind: "Example".into(),
                        language: "mdx".into(),
                        name: "visual-example".into(),
                        source: None,
                        sha256: None,
                        collection: "license-metadata-only".into(),
                    });
                }
            }
        }
    }
    // Preserve the provider modules and all examples so local imports can be inspected,
    // even when the bounded native frontend cannot execute their original React code.
    for folder in ["apps/v4/examples", "apps/v4/registry/bases"] {
        for file in files(&shadcn.join(folder))? {
            let relative = relative(&file, shadcn)?;
            let target = output.join("upstream").join(relative);
            fs::create_dir_all(target.parent().ok_or("missing parent")?)
                .map_err(|e| e.to_string())?;
            fs::copy(file, target).map_err(|e| e.to_string())?;
        }
    }
    write_json(&output.join("catalog.json"), &catalog)?;
    println!(
        "pages={} examples={} unresolved={}",
        catalog.pages.len(),
        catalog.examples.len(),
        catalog
            .examples
            .iter()
            .filter(|e| e.collection == "unresolved-reference")
            .count()
    );
    Ok(())
}

fn fences(text: &str) -> Result<Vec<(usize, String, String)>, String> {
    let mut result = vec![];
    let mut open: Option<(usize, String, String, usize, u8)> = None;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let marker = trimmed.as_bytes().first().copied().unwrap_or_default();
        let count = trimmed.bytes().take_while(|b| *b == marker).count();
        if let Some((start, language, code, length, delimiter)) = open.as_mut() {
            if marker == *delimiter && count >= *length && trimmed[count..].trim().is_empty() {
                result.push((*start, language.clone(), code.clone()));
                open = None;
            } else {
                code.push_str(line);
                code.push('\n');
            }
        } else if matches!(marker, b'`' | b'~') && count >= 3 {
            open = Some((
                index + 1,
                trimmed[count..]
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .into(),
                String::new(),
                count,
                marker,
            ));
        }
    }
    if open.is_some() {
        return Err("unclosed upstream code fence".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::fences;
    #[test]
    fn longer_fences_preserve_nested_code_and_line_numbers() {
        let found = fences("title\n````tsx title=demo\n```\n<x />\n```\n````\n").unwrap();
        assert_eq!(found, vec![(2, "tsx".into(), "```\n<x />\n```\n".into())]);
        assert!(fences("```tsx\nunclosed").is_err());
    }
}
