use crate::domain::{LayoutBox, Viewport};
pub mod file_picker;
mod groups;
pub mod icons;
pub mod markup;
pub mod media;
pub mod screen;
pub(crate) mod toggles;

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
    let component = &bound;
    let session = crate::domain::widgets::Session::from_tree(&component.root)?;
    let markup = markup::build(component, &session)?;
    compiler.compile(&markup.classes, theme)
}

pub fn bind(
    component: &meari::domain::Component,
    props: &std::collections::BTreeMap<String, meari::domain::Value>,
) -> Result<meari::domain::Component, String> {
    use meari::domain::{Node, NodeKind, PropType, Value};
    meari::domain::validation::validate(component).map_err(|e| e.message)?;
    if props.len() != component.props.len() {
        return Err("exact declared bindings required".into());
    }
    for prop in &component.props {
        if !matches!(
            (prop.kind, props.get(&prop.name)),
            (PropType::String, Some(Value::String(_))) | (PropType::Bool, Some(Value::Bool(_)))
        ) {
            return Err(format!("invalid binding: {}", prop.name));
        }
    }
    fn visit(node: &mut Node, props: &std::collections::BTreeMap<String, Value>) {
        let bind = |v: &mut Value| {
            if let Value::Prop(name) = v {
                *v = props[name].clone();
            } else if let Value::Id(name) = v {
                *v = Value::String(meari::domain::hooks::stable_id(name));
            }
        };
        match &mut node.kind {
            NodeKind::Text(value) => bind(value),
            NodeKind::Element {
                attributes,
                children,
                ..
            } => {
                for attr in attributes {
                    bind(&mut attr.value);
                }
                for child in children {
                    visit(child, props);
                }
            }
        }
    }
    let mut bound = component.clone();
    visit(&mut bound.root, props);
    bound.props.clear();
    Ok(bound)
}

pub trait Renderer {
    fn layout(&mut self, viewport: &Viewport, time_seconds: f64) -> Result<(), String>;
    fn bounds(&self, id: &str) -> Result<LayoutBox, String>;
    fn rasterize(&mut self, viewport: &Viewport) -> Result<Vec<u8>, String>;
}
