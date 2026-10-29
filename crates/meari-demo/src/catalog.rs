//! Fixture catalog: the visual-test fixtures, a cursor over them, and per-fixture assets on disk.
use meari_visual::fixtures::{self, Fixture, Kind};
use std::path::{Path, PathBuf};

pub struct Catalog {
    fixtures: Vec<Fixture>,
    pub index: usize,
}

/// Files the two windows load for one fixture.
pub struct Prepared {
    pub name: String,
    pub source: PathBuf,
    pub css: PathBuf,
    pub dark: bool,
    /// Path of the web page relative to the served directory.
    pub page: String,
}

impl Catalog {
    pub fn load() -> Result<Self, String> {
        let fixtures = fixtures::load()?;
        if fixtures.is_empty() {
            return Err("no fixtures found".into());
        }
        Ok(Self { fixtures, index: 0 })
    }

    pub fn len(&self) -> usize {
        self.fixtures.len()
    }

    pub fn current(&self) -> &Fixture {
        &self.fixtures[self.index]
    }

    /// Select by 0-based index or by exact / partial name.
    pub fn select(&mut self, key: &str) -> Result<(), String> {
        let found = match key.parse::<usize>() {
            Ok(i) if i < self.len() => Some(i),
            _ => self
                .position(|f| f.name == key)
                .or_else(|| self.position(|f| f.name.contains(key))),
        };
        self.index = found.ok_or_else(|| format!("no fixture matches {key:?}"))?;
        Ok(())
    }

    pub fn step(&mut self, forward: bool) -> Result<(), String> {
        let n = self.len();
        self.index = if forward {
            (self.index + 1) % n
        } else {
            (self.index + n - 1) % n
        };
        Ok(())
    }

    pub fn listing(&self) -> String {
        self.fixtures
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let mark = if i == self.index { '>' } else { ' ' };
                let kind = if f.kind == Kind::Gap {
                    " (known gap)"
                } else {
                    ""
                };
                format!("{mark}{i:3} {}{kind}", f.name)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Compile the stylesheet and write source, CSS and the web page under `out/<name>/`.
    pub fn prepare(&self, out: &Path, tailwind: &Path) -> Result<Prepared, String> {
        let fixture = self.current();
        let dir = out.join(&fixture.name);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let css = meari_visual::render::style::resolve(fixture, tailwind)?;
        let html = meari_visual::render::web::html(&fixture.source, &css, fixture.dark)?.replacen(
            "<head>",
            &format!("<head><title>Meari Web - {}</title>", fixture.name),
            1,
        );
        let write = |file: &str, text: &str| -> Result<PathBuf, String> {
            let path = dir.join(file);
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(path)
        };
        write("web.html", &html)?;
        Ok(Prepared {
            name: fixture.name.clone(),
            source: write("source.tsx", &fixture.source)?,
            css: write("style.css", &css)?,
            dark: fixture.dark,
            page: format!("{}/web.html", fixture.name),
        })
    }

    fn position(&self, predicate: impl Fn(&Fixture) -> bool) -> Option<usize> {
        self.fixtures.iter().position(predicate)
    }
}
