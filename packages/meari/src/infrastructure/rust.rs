use crate::domain::*;
use syn::parse::Parser;
use syn::{Expr, Item, Lit, Stmt, Type, spanned::Spanned};

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
}
impl Reader<'_> {
    fn err(&self, node: &impl Spanned, message: &str) -> Diagnostic {
        Diagnostic::unsupported(span(self.source, node.span()), message)
    }
    fn value(&self, expr: &Expr) -> Result<Value, Diagnostic> {
        match expr {
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
        let kind = if path.path.is_ident("text") && call.args.len() == 1 {
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
    let reader = Reader { source };
    let file = syn::parse_file(source).map_err(|e| Diagnostic {
        code: "syntax".into(),
        message: e.to_string(),
        span: span(source, e.span()),
    })?;
    if !file.attrs.is_empty() {
        return Err(reader.err(&file, "file attributes unsupported"));
    }
    let mut props = None;
    let mut client = false;
    let mut function = None;
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
                if !s.attrs.is_empty()
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
                    let kind = match &f.ty {
                        Type::Path(p) if p.qself.is_none() && p.path.is_ident("String") => {
                            PropType::String
                        }
                        Type::Path(p) if p.qself.is_none() && p.path.is_ident("bool") => {
                            PropType::Bool
                        }
                        _ => return Err(reader.err(f, "Props fields support only String/bool")),
                    };
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
            || !matches!(&a.meta, syn::Meta::List(l) if l.tokens.to_string() == "non_snake_case")
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
        || sig.inputs.len() != 1
    {
        return Err(reader.err(sig, "plain synchronous component(props: Props) required"));
    }
    let valid_input = matches!(&sig.inputs[0], syn::FnArg::Typed(p) if p.attrs.is_empty() && matches!(&*p.pat, syn::Pat::Ident(i) if i.ident == "props" && i.by_ref.is_none() && i.mutability.is_none() && i.subpat.is_none()) && matches!(&*p.ty, Type::Path(t) if t.qself.is_none() && t.path.is_ident("Props")));
    if !valid_input
        || !matches!(&sig.output, syn::ReturnType::Type(_, t) if matches!(&**t, Type::Path(p) if p.qself.is_none() && p.path.is_ident("Node")))
    {
        return Err(reader.err(sig, "expected (props: Props) -> Node"));
    }
    let [Stmt::Expr(expr, None)] = f.block.stmts.as_slice() else {
        return Err(reader.err(
            &f.block,
            "component must contain one pure tail expression; statements/hooks/effects unsupported",
        ));
    };
    Ok(Component {
        client,
        name: sig.ident.to_string(),
        props: props.ok_or_else(|| reader.err(f, "Props struct required"))?,
        root: reader.node(expr)?,
    })
}
