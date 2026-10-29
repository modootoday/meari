use crate::domain::*;
use syn::parse::Parser;
use syn::{Expr, Item, Lit, Type, spanned::Spanned};
mod bool_expression;
mod prop_type;
mod records;
mod state;

fn span(source: &str, span: proc_macro2::Span) -> Span {
    fn offset(source: &str, location: proc_macro2::LineColumn) -> usize {
        let line_start = source
            .split_inclusive('\n')
            .take(location.line.saturating_sub(1))
            .map(str::len)
            .sum::<usize>();
        let line = &source[line_start..];
        line_start
            + line
                .char_indices()
                .nth(location.column)
                .map_or(line.len(), |(byte, _)| byte)
    }
    Span {
        start: offset(source, span.start()),
        end: offset(source, span.end()),
    }
}

struct Reader<'a> {
    source: &'a str,
    records: std::collections::BTreeMap<String, &'a syn::ItemStruct>,
}
impl Reader<'_> {
    fn err(&self, node: &impl Spanned, message: &str) -> Diagnostic {
        Diagnostic::unsupported(span(self.source, node.span()), message)
    }
    fn value(&self, expr: &Expr) -> Result<Value, Diagnostic> {
        match expr {
            Expr::Call(call)
                if call.attrs.is_empty()
                    && call.args.len() == 2
                    && matches!(&*call.func, Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("member")) =>
            {
                let path = self
                    .vector(&call.args[1])?
                    .iter()
                    .map(|field| self.string(field))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Member(member_value::MemberValue {
                    root: Box::new(self.value(&call.args[0])?),
                    path,
                }))
            }
            Expr::Call(call)
                if call.attrs.is_empty()
                    && matches!(&*call.func,Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("empty_node_slot"))
                    && call.args.is_empty() =>
            {
                Ok(Value::NodeSlot(crate::domain::node_slots::NodeSlot::Empty))
            }
            Expr::Call(call)
                if call.attrs.is_empty()
                    && matches!(&*call.func,Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("node_slot"))
                    && call.args.len() == 1 =>
            {
                Ok(Value::NodeSlot(
                    crate::domain::node_slots::NodeSlot::Content(
                        self.vector(&call.args[0])?
                            .iter()
                            .map(|node| self.node(node))
                            .collect::<Result<_, _>>()?,
                    ),
                ))
            }
            Expr::Call(call)
                if call.args.len() == 1
                    && call.attrs.is_empty()
                    && matches!(&*call.func, Expr::Path(path) if path.attrs.is_empty() && path.path.is_ident("local")) =>
            {
                Ok(Value::Local(self.string(&call.args[0])?))
            }
            Expr::Binary(binary)
                if binary.attrs.is_empty()
                    && matches!(
                        binary.op,
                        syn::BinOp::And(_)
                            | syn::BinOp::Or(_)
                            | syn::BinOp::Eq(_)
                            | syn::BinOp::Ne(_)
                    ) =>
            {
                self.bool_expression(expr)
                    .map(Box::new)
                    .map(Value::Predicate)
            }
            Expr::Paren(value) if value.attrs.is_empty() => self.value(&value.expr),
            Expr::Group(value) if value.attrs.is_empty() => self.value(&value.expr),
            Expr::Unary(unary)
                if unary.attrs.is_empty() && matches!(unary.op, syn::UnOp::Not(_)) =>
            {
                match self.value(&unary.expr) {
                    Ok(Value::Prop(prop)) => Ok(Value::NotProp(prop)),
                    _ => self
                        .bool_expression(expr)
                        .map(Box::new)
                        .map(Value::Predicate),
                }
            }
            Expr::Try(value) if value.attrs.is_empty() => {
                let Expr::Call(call) = &*value.expr else {
                    return Err(self.err(expr, "canonical state expression required"));
                };
                if !call.attrs.is_empty() {
                    return Err(self.err(expr, "state expression attributes unsupported"));
                }
                let Expr::Path(path) = &*call.func else {
                    return Err(self.err(expr, "named state function required"));
                };
                if !path.attrs.is_empty() || path.qself.is_some() {
                    return Err(self.err(expr, "plain state function required"));
                }
                if path.path.is_ident("state_read")
                    && call.args.len() == 2
                    && matches!(&call.args[0], Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("state"))
                {
                    Ok(Value::State(self.string(&call.args[1])?))
                } else if path.path.is_ident("input_event") && call.args.len() == 1 {
                    serde_json::from_str(&self.string(&call.args[0])?)
                        .map(Value::Input)
                        .map_err(|_| self.err(expr, "invalid input event JSON"))
                } else if path.path.is_ident("state_event") && call.args.len() == 1 {
                    serde_json::from_str(&self.string(&call.args[0])?)
                        .map(Value::Event)
                        .map_err(|_| self.err(expr, "invalid state event JSON"))
                } else if path.path.is_ident("state_captured_event")
                    && call.args.len() == 2
                    && matches!(&call.args[0],Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("state"))
                {
                    serde_json::from_str(&self.string(&call.args[1])?)
                        .map(Value::Event)
                        .map_err(|_| self.err(expr, "invalid captured state event JSON"))
                } else {
                    Err(self.err(expr, "unsupported fallible state expression"))
                }
            }
            Expr::Call(call)
                if call.attrs.is_empty()
                    && call.args.len() == 2
                    && matches!(&*call.func, Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("host_text_capture")) =>
            {
                let mut props = Vec::with_capacity(2);
                for argument in &call.args {
                    let Expr::Reference(reference) = argument else {
                        return Err(
                            self.err(expr, "captured callback requires borrowed typed props")
                        );
                    };
                    if reference.mutability.is_some() || !reference.attrs.is_empty() {
                        return Err(
                            self.err(expr, "captured callback requires immutable prop references")
                        );
                    }
                    let Value::Prop(prop) = self.value(&reference.expr)? else {
                        return Err(self.err(expr, "captured callback must reference props.name"));
                    };
                    props.push(prop);
                }
                Ok(Value::Callback(
                    crate::domain::host_callbacks::CallbackHandler {
                        prop: props.remove(0),
                        source: crate::domain::host_callbacks::CallbackSource::CapturedText,
                        capture_prop: props.pop(),
                        span: span(self.source, expr.span()),
                    },
                ))
            }
            Expr::Call(call)
                if call.attrs.is_empty()
                    && call.args.len() == 1
                    && matches!(&*call.func, Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && (path.path.is_ident("host_action") || path.path.is_ident("host_text") || path.path.is_ident("host_boolean") || path.path.is_ident("host_selection"))) =>
            {
                let Expr::Reference(reference) = &call.args[0] else {
                    return Err(self.err(expr, "host callback requires a borrowed typed prop"));
                };
                if reference.mutability.is_some() || !reference.attrs.is_empty() {
                    return Err(
                        self.err(expr, "host callback requires an immutable prop reference")
                    );
                }
                let Value::Prop(prop) = self.value(&reference.expr)? else {
                    return Err(self.err(expr, "host callback must reference props.name"));
                };
                let Expr::Path(path) = &*call.func else {
                    unreachable!()
                };
                Ok(Value::Callback(
                    crate::domain::host_callbacks::CallbackHandler {
                        prop,
                        source: if path.path.is_ident("host_action") {
                            crate::domain::host_callbacks::CallbackSource::Action
                        } else if path.path.is_ident("host_boolean") {
                            crate::domain::host_callbacks::CallbackSource::BooleanValue
                        } else if path.path.is_ident("host_selection") {
                            crate::domain::host_callbacks::CallbackSource::SelectionValue
                        } else {
                            crate::domain::host_callbacks::CallbackSource::CurrentTargetText
                        },
                        span: span(self.source, expr.span()),
                        capture_prop: None,
                    },
                ))
            }
            Expr::Call(call)
                if call.attrs.is_empty()
                    && call.args.len() == 1
                    && matches!(&*call.func,Expr::Path(p) if p.attrs.is_empty() && p.qself.is_none() && p.path.is_ident("use_id")) =>
            {
                match &call.args[0] {
                    Expr::Lit(l) if l.attrs.is_empty() => match &l.lit {
                        Lit::Str(s) => Ok(Value::Id(s.value())),
                        _ => Err(self.err(expr, "use_id requires literal binding name")),
                    },
                    _ => Err(self.err(expr, "use_id requires literal binding name")),
                }
            }
            Expr::Lit(lit) if lit.attrs.is_empty() => match &lit.lit {
                Lit::Str(s) => Ok(Value::String(s.value())),
                Lit::Bool(b) => Ok(Value::Bool(b.value)),
                _ => Err(self.err(expr, "only string/bool literals supported")),
            },
            Expr::Field(field)
                if field.attrs.is_empty()
                    && matches!(&*field.base, Expr::Path(p) if p.attrs.is_empty() && p.path.is_ident("props")) =>
            {
                if let syn::Member::Named(name) = &field.member {
                    Ok(Value::Prop(name.to_string()))
                } else {
                    Err(self.err(expr, "named props required"))
                }
            }
            Expr::MethodCall(call)
                if call.attrs.is_empty()
                    && call.method == "clone"
                    && call.args.is_empty()
                    && call.turbofish.is_none() =>
            {
                self.value(&call.receiver)
            }
            _ => Err(self.err(expr, "arbitrary expressions/state/effects are unsupported")),
        }
    }
    fn vector(&self, expr: &Expr) -> Result<Vec<Expr>, Diagnostic> {
        if let Expr::Macro(m) = expr
            && m.attrs.is_empty()
            && m.mac.path.is_ident("vec")
        {
            return syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated
                .parse2(m.mac.tokens.clone())
                .map(|v| v.into_iter().collect())
                .map_err(|e| self.err(expr, &e.to_string()));
        }
        Err(self.err(expr, "expected vec![...]"))
    }
    fn string(&self, expr: &Expr) -> Result<String, Diagnostic> {
        if let Value::String(value) = self.value(expr)? {
            Ok(value)
        } else {
            Err(self.err(expr, "literal string required"))
        }
    }
    fn node(&self, expr: &Expr) -> Result<Node, Diagnostic> {
        let Expr::Call(call) = expr else {
            return Err(self.err(expr, "expected element/text call"));
        };
        let Expr::Path(path) = &*call.func else {
            return Err(self.err(expr, "simple call required"));
        };
        if !call.attrs.is_empty() || !path.attrs.is_empty() {
            return Err(self.err(expr, "expression attributes unsupported"));
        }
        let kind = if (path.path.is_ident("repeat") || path.path.is_ident("repeat_member"))
            && call.args.len() == 4
        {
            let item = self.string(&call.args[1])?;
            let member_key = path.path.is_ident("repeat_member");
            NodeKind::Text(Value::Repeat(Box::new(crate::domain::repeat::Repeat {
                source: self.value(&call.args[0])?,
                item: item.clone(),
                key: if member_key {
                    item
                } else {
                    self.string(&call.args[2])?
                },
                key_path: if member_key {
                    self.vector(&call.args[2])?
                        .iter()
                        .map(|field| self.string(field))
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    Vec::new()
                },
                template: self.node(&call.args[3])?,
                source_span: span(self.source, call.args[0].span()),
                key_span: span(self.source, call.args[2].span()),
            })))
        } else if path.path.is_ident("text") && call.args.len() == 1 {
            NodeKind::Text(self.value(&call.args[0])?)
        } else if path.path.is_ident("element") && call.args.len() == 3 {
            let mut attributes = Vec::new();
            for attr in self.vector(&call.args[1])? {
                let Expr::Call(a) = &attr else {
                    return Err(self.err(&attr, "expected attribute call"));
                };
                if !a.attrs.is_empty()
                    || !matches!(&*a.func, Expr::Path(p) if p.attrs.is_empty() && p.path.is_ident("attribute"))
                    || a.args.len() != 2
                {
                    return Err(self.err(&attr, "expected attribute(name, value)"));
                }
                attributes.push(Attribute {
                    name: self.string(&a.args[0])?,
                    value: self.value(&a.args[1])?,
                });
            }
            NodeKind::Element {
                tag: self.string(&call.args[0])?,
                attributes,
                children: self
                    .vector(&call.args[2])?
                    .iter()
                    .map(|e| self.node(e))
                    .collect::<Result<_, _>>()?,
            }
        } else {
            return Err(self.err(
                expr,
                "only element(tag, attributes, children) and text(value) supported",
            ));
        };
        Ok(Node {
            kind,
            span: span(self.source, expr.span()),
        })
    }
}

pub fn parse(source: &str) -> Result<Component, Diagnostic> {
    let file = syn::parse_file(source).map_err(|e| Diagnostic {
        code: "syntax".into(),
        message: e.to_string(),
        span: span(source, e.span()),
    })?;
    let mut records = std::collections::BTreeMap::new();
    for item in &file.items {
        if let Item::Struct(record) = item
            && record.ident != "Props"
            && records.insert(record.ident.to_string(), record).is_some()
        {
            return Err(Diagnostic::unsupported(
                span(source, record.span()),
                "duplicate record declaration",
            ));
        }
    }
    let reader = Reader { source, records };
    if !file.attrs.is_empty() {
        return Err(reader.err(&file, "file attributes unsupported"));
    }
    let mut props = None;
    let mut client = false;
    let mut function = None;
    let mut record_implementations = std::collections::BTreeSet::new();
    for item in &file.items {
        match item {
            Item::Const(c)
                if !client
                    && c.ident == "BRIDGE_USE_CLIENT"
                    && c.attrs.is_empty()
                    && c.generics.params.is_empty()
                    && c.generics.where_clause.is_none()
                    && matches!(c.vis, syn::Visibility::Public(_))
                    && matches!(&*c.ty, Type::Path(t) if t.qself.is_none() && t.path.is_ident("bool"))
                    && matches!(&*c.expr, Expr::Lit(l) if l.attrs.is_empty() && matches!(&l.lit,Lit::Bool(b) if b.value)) =>
            {
                client = true;
            }
            Item::Use(u)
                if source
                    .get(span(source, u.span()).start..span(source, u.span()).end)
                    .is_some_and(|s| s == "use meari::runtime::*;") => {}
            Item::Struct(s) if s.ident == "Props" && props.is_none() => {
                if s.attrs.len() > 1 || s.attrs.iter().any(|attribute| !attribute.path().is_ident("allow") || !matches!(&attribute.meta, syn::Meta::List(list) if list.tokens.to_string() == "non_snake_case"))
                    || !s.generics.params.is_empty()
                    || s.generics.where_clause.is_some()
                {
                    return Err(reader.err(s, "plain Props struct required"));
                }
                let syn::Fields::Named(fields) = &s.fields else {
                    return Err(reader.err(s, "named Props fields required"));
                };
                let mut values = Vec::new();
                for f in &fields.named {
                    if !f.attrs.is_empty() {
                        return Err(reader.err(f, "prop attributes unsupported"));
                    }
                    let kind = reader.prop_type(&f.ty, 0)?;
                    values.push(Prop {
                        name: f.ident.as_ref().expect("named fields").to_string(),
                        kind,
                    });
                }
                props = Some(values);
            }
            Item::Fn(f) if function.is_none() => {
                function = Some(f);
            }
            Item::Struct(record) if record.ident != "Props" => {
                let fields = reader.record_fields(record, 0)?;
                PropType::Record(fields)
                    .validate()
                    .map_err(|message| reader.err(record, &message))?;
            }
            Item::Impl(implementation) => {
                if !reader.record_implementation(implementation)? {
                    return Err(reader.err(
                        implementation,
                        "only generated typed record conversions are supported",
                    ));
                }
                let syn::Type::Path(record) = implementation.self_ty.as_ref() else {
                    unreachable!("canonical record implementation")
                };
                let (_, trait_path, _) = implementation
                    .trait_
                    .as_ref()
                    .expect("canonical record trait");
                let identity = (
                    record.path.segments[0].ident.to_string(),
                    trait_path.segments[0].ident.to_string(),
                );
                if !record_implementations.insert(identity) {
                    return Err(reader.err(implementation, "duplicate typed record conversion"));
                }
            }
            _ => {
                return Err(reader.err(
                    item,
                    "only Props struct and one component function supported",
                ));
            }
        }
    }
    let f = function
        .ok_or_else(|| Diagnostic::unsupported(Span::default(), "component function required"))?;
    // The generated lint attribute is the only permitted function attribute.
    if f.attrs.iter().any(|a| {
        !a.path().is_ident("allow")
            || !matches!(&a.meta, syn::Meta::List(l) if matches!(l.tokens.to_string().as_str(), "non_snake_case" | "non_snake_case , unused_variables"))
    }) {
        return Err(reader.err(f, "component attributes unsupported"));
    }
    let sig = &f.sig;
    if sig.asyncness.is_some()
        || sig.constness.is_some()
        || sig.unsafety.is_some()
        || sig.abi.is_some()
        || sig.variadic.is_some()
        || !sig.generics.params.is_empty()
        || sig.generics.where_clause.is_some()
        || ![1, 2].contains(&sig.inputs.len())
    {
        return Err(reader.err(sig, "plain synchronous component(props: Props) required"));
    }
    let valid_input = matches!(&sig.inputs[0], syn::FnArg::Typed(p) if p.attrs.is_empty() && matches!(&*p.pat, syn::Pat::Ident(i) if i.ident == "props" && i.by_ref.is_none() && i.mutability.is_none() && i.subpat.is_none()) && matches!(&*p.ty, Type::Path(t) if t.qself.is_none() && t.path.is_ident("Props")));
    let stateful = sig.inputs.len() == 2;
    let valid_state = !stateful
        || matches!(&sig.inputs[1], syn::FnArg::Typed(p) if p.attrs.is_empty()
        && matches!(&*p.pat,syn::Pat::Ident(i) if i.ident == "state" && i.by_ref.is_none() && i.mutability.is_none() && i.subpat.is_none())
        && matches!(&*p.ty, Type::Reference(reference) if reference.lifetime.is_none() && reference.mutability.is_none() && matches!(&*reference.elem,Type::Path(path) if path.qself.is_none() && path.path.is_ident("StateContext"))));
    if !valid_input
        || !valid_state
        || !matches!(&sig.output, syn::ReturnType::Type(_, t) if matches!(&**t, Type::Path(p) if p.qself.is_none() && p.path.is_ident(if stateful { "RenderResult" } else { "Node" })))
    {
        return Err(reader.err(sig, "expected (props: Props) -> Node"));
    }
    let (states, hook_order, expr) = state::body(&reader, f)?;
    if stateful == states.is_empty() {
        return Err(reader.err(
            f,
            "state declarations require StateContext and RenderResult",
        ));
    }
    Ok(Component {
        hook_order,
        states,
        client,
        name: sig.ident.to_string(),
        props: props.ok_or_else(|| reader.err(f, "Props struct required"))?,
        root: reader.node(expr)?,
    })
}
