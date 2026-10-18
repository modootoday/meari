use super::{
    components, diagnostic,
    theme::{self, Environment, Interaction, ResolvedStyle, Theme},
    utilities,
};
use crate::domain::{Attribute, Component, Diagnostic, Node, NodeKind, Value, validation};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
pub struct NativeNode {
    pub path: String,
    pub source: crate::domain::Span,
    pub component: String,
    pub widget: String,
    pub attributes: BTreeMap<String, Value>,
    pub style: ResolvedStyle,
    pub text: Option<String>,
    pub children: Vec<NativeNode>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NativePlan {
    pub profile: &'static str,
    pub source: Component,
    pub root: NativeNode,
}

pub struct Context<'a> {
    pub props: &'a BTreeMap<String, Value>,
    pub theme: &'a Theme,
    pub environment: Environment,
    /// Node-path-specific state, never one global hover flag for the whole tree.
    pub interactions: &'a BTreeMap<String, Interaction>,
}

fn bind(value: &Value, node: &Node, context: &Context<'_>) -> Result<Value, Diagnostic> {
    match value {
        Value::Id(name) => Ok(Value::String(crate::domain::hooks::stable_id(name))),
        Value::Prop(name) => context
            .props
            .get(name)
            .cloned()
            .ok_or_else(|| diagnostic(node, "native-binding", format!("missing prop: {name}"))),
        literal => Ok(literal.clone()),
    }
}

fn lower(node: &Node, path: String, context: &Context<'_>) -> Result<NativeNode, Diagnostic> {
    let error = |message| diagnostic(node, "native-style", message);
    let mut result = NativeNode {
        path: path.clone(),
        source: node.span,
        component: String::new(),
        widget: String::new(),
        attributes: BTreeMap::new(),
        style: BTreeMap::new(),
        text: None,
        children: Vec::new(),
    };
    match &node.kind {
        NodeKind::Text(value) => {
            let Value::String(text) = bind(value, node, context)? else {
                return Err(diagnostic(node, "native-binding", "text requires string"));
            };
            result.widget = "text".into();
            result.text = Some(text);
        }
        NodeKind::Element {
            tag,
            attributes,
            children,
        } => {
            // className is intentionally static until expression dependency tracking exists.
            if attributes
                .iter()
                .any(|a| a.name == "className" && !matches!(a.value, Value::String(_)))
            {
                return Err(error(
                    "native className bindings require expression IR".into(),
                ));
            }
            let bound: Vec<Attribute> = attributes
                .iter()
                .map(|a| {
                    Ok(Attribute {
                        name: a.name.clone(),
                        value: bind(&a.value, node, context)?,
                    })
                })
                .collect::<Result<_, Diagnostic>>()?;
            components::validate(node, tag, &bound, children)?;
            result.component = tag.clone();
            result.widget = components::intrinsic(tag)
                .expect("validated registry")
                .into();
            result.attributes = bound
                .iter()
                .map(|a| (a.name.clone(), a.value.clone()))
                .collect();
            let string = |name: &str, default: &str| match result.attributes.get(name) {
                Some(Value::String(value)) => value.clone(),
                _ => default.into(),
            };
            let variant = string("variant", "default");
            let size = string("size", "default");
            let classes = string("className", "");
            let disabled = result.attributes.get("disabled") == Some(&Value::Bool(true));
            let mut state = context.interactions.get(&path).copied().unwrap_or_default();
            state.disabled = disabled;
            let recipe =
                utilities::parse(&components::recipe(tag, &variant, &size)).map_err(error)?;
            result.style = theme::resolve(&recipe, context.theme, context.environment, state)
                .map_err(error)?;
            let rules = utilities::parse(&classes).map_err(error)?;
            result.style.extend(
                theme::resolve(&rules, context.theme, context.environment, state).map_err(error)?,
            );
            for (index, child) in children.iter().enumerate() {
                result
                    .children
                    .push(lower(child, format!("{path}/{index}"), context)?);
            }
        }
    }
    Ok(result)
}

pub fn prepare(component: &Component, context: &Context<'_>) -> Result<NativePlan, Diagnostic> {
    validation::validate(component)?;
    let error = |message| diagnostic(&component.root, "native-binding", message);
    context.environment.validate().map_err(error)?;
    context.theme.validate().map_err(error)?;
    if context.props.len() != component.props.len() {
        return Err(error("exact declared prop bindings required".into()));
    }
    for prop in &component.props {
        let valid = matches!(
            (prop.kind, context.props.get(&prop.name)),
            (crate::domain::PropType::String, Some(Value::String(_)))
                | (crate::domain::PropType::Bool, Some(Value::Bool(_)))
        );
        if !valid {
            return Err(error(format!(
                "missing or incorrectly typed prop: {}",
                prop.name
            )));
        }
    }
    Ok(NativePlan {
        profile: super::PROFILE,
        source: component.clone(),
        root: lower(&component.root, "0".into(), context)?,
    })
}
