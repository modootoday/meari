use crate::domain::{LayoutBox, Viewport};
mod boolean_profile;
pub mod file_picker;
mod groups;
pub mod icons;
pub mod markup;
pub mod media;
pub mod screen;
pub(crate) mod state_program;
#[cfg(any(feature = "window", test))]
pub(crate) mod surface_slot;
pub(crate) mod toggles;
pub mod window_actions;
pub mod window_lifecycle;

#[derive(Clone, Debug)]
pub struct WindowOptions {
    pub title: String,
}
impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "Meari Native".into(),
        }
    }
}
impl WindowOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty()
            || self.title.len() > 256
            || self.title.chars().any(char::is_control)
        {
            return Err("invalid native window title".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CompiledStylesheet {
    pub css: String,
    pub original: meari::domain::design::stylesheet::StylesheetProject,
    pub source_sha256: std::collections::BTreeMap<String, String>,
    pub css_sha256: String,
    pub compiler: String,
    pub native_full_acceptance: bool,
}

pub trait StylesheetProjectCompiler {
    fn compile_project(
        &self,
        project: &meari::domain::design::stylesheet::StylesheetProject,
        classes: &str,
        theme: &meari::domain::design::theme::Theme,
    ) -> Result<CompiledStylesheet, String>;
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StylesheetRenderRequest {
    pub source: String,
    pub language: meari::application::Language,
    #[serde(default)]
    pub props: std::collections::BTreeMap<String, meari::domain::Value>,
    #[serde(default)]
    pub theme: meari::domain::design::theme::Theme,
    pub stylesheet: meari::domain::design::stylesheet::StylesheetProject,
    pub viewport: Viewport,
    #[serde(default)]
    pub screen_element: Option<String>,
}

pub fn prepare_stylesheet(
    request: &StylesheetRenderRequest,
    compiler: &dyn StylesheetProjectCompiler,
) -> Result<(meari::domain::Component, CompiledStylesheet), String> {
    request.viewport.validate()?;
    let source = meari::parse(&request.source, request.language).map_err(|e| e.message)?;
    let source = bind(&source, &request.props)?;
    let css = stylesheet_project(&source, &request.stylesheet, &request.theme, compiler)?;
    Ok((source, css))
}

pub fn stylesheet_project(
    component: &meari::domain::Component,
    project: &meari::domain::design::stylesheet::StylesheetProject,
    theme: &meari::domain::design::theme::Theme,
    compiler: &dyn StylesheetProjectCompiler,
) -> Result<CompiledStylesheet, String> {
    let bound = bind(component, &std::collections::BTreeMap::new())?;
    let bound = initial_state_projection(bound)?;
    let component = &bound;
    let session = crate::domain::widgets::Session::from_tree(&component.root)?;
    let markup = markup::build(component, &session)?;
    compiler.compile_project(project, &markup.classes, theme)
}

pub trait StylesheetCompiler {
    fn compile(
        &self,
        classes: &str,
        theme: &meari::domain::design::theme::Theme,
    ) -> Result<String, String>;
}

pub fn stylesheet(
    component: &meari::domain::Component,
    theme: &meari::domain::design::theme::Theme,
    compiler: &dyn StylesheetCompiler,
) -> Result<String, String> {
    let bound = bind(component, &std::collections::BTreeMap::new())?;
    let bound = initial_state_projection(bound)?;
    let component = &bound;
    let session = crate::domain::widgets::Session::from_tree(&component.root)?;
    let markup = markup::build(component, &session)?;
    compiler.compile(&markup.classes, theme)
}

pub(crate) fn initial_state_projection(
    component: meari::domain::Component,
) -> Result<meari::domain::Component, String> {
    state_program::validate(&component)?;
    if component.states.is_empty() {
        return Ok(component);
    }
    Ok(meari::application::state_program::StateProgram::mount(
        component,
        "stylesheet-projection".into(),
        1,
    )?
    .project()?
    .component)
}

pub fn bind(
    component: &meari::domain::Component,
    props: &std::collections::BTreeMap<String, meari::domain::Value>,
) -> Result<meari::domain::Component, String> {
    meari::application::binding::bind(component, props)
}

pub trait Renderer {
    fn layout(&mut self, viewport: &Viewport, time_seconds: f64) -> Result<(), String>;
    fn bounds(&self, id: &str) -> Result<LayoutBox, String>;
    fn rasterize(&mut self, viewport: &Viewport) -> Result<Vec<u8>, String>;
}
