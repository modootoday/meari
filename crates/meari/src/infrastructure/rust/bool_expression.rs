//! Prepared canonical Rust predicate parsing for the TSX boolean source profile.
use crate::domain::{
    Diagnostic,
    bool_expression::{
        BoolExpression, BoolExpressionKind as Kind, ScalarExpression,
        ScalarExpressionKind as Scalar,
    },
};
use syn::{BinOp, Expr, Lit, Member, spanned::Spanned};

impl super::Reader<'_> {
    pub(super) fn bool_expression(&self, expression: &Expr) -> Result<BoolExpression, Diagnostic> {
        fn visit(
            reader: &super::Reader<'_>,
            expression: &Expr,
            depth: usize,
            steps: &mut usize,
        ) -> Result<BoolExpression, Diagnostic> {
            *steps += if matches!(expression, Expr::Binary(binary) if matches!(binary.op, BinOp::Eq(_) | BinOp::Ne(_)))
            {
                3
            } else {
                1
            };
            if depth > 64 || *steps > 8192 {
                return Err(reader.err(expression, "boolean expression exceeds depth/node budget"));
            }
            let kind = match expression {
                Expr::Try(value) if value.attrs.is_empty() => {
                    let Expr::Call(call) = &*value.expr else {
                        return Err(reader.err(expression, "canonical Boolean state read required"));
                    };
                    if !call.attrs.is_empty()
                        || call.args.len() != 2
                        || !matches!(&*call.func, Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("state_bool"))
                        || !matches!(&call.args[0], Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("state"))
                    {
                        return Err(reader.err(expression, "canonical Boolean state read required"));
                    }
                    Kind::State(reader.string(&call.args[1])?)
                }
                Expr::Lit(value) if value.attrs.is_empty() => match &value.lit {
                    Lit::Bool(value) => Kind::Literal(value.value),
                    _ => return Err(reader.err(expression, "boolean literal required")),
                },
                Expr::Call(_) => match reader.value(expression)? {
                    crate::domain::Value::Member(member) => Kind::Member(member),
                    _ => return Err(reader.err(expression, "canonical record predicate required")),
                },
                Expr::Field(field) if field.attrs.is_empty() => {
                    let Expr::Path(object) = &*field.base else {
                        return Err(reader.err(expression, "plain props member required"));
                    };
                    let Member::Named(name) = &field.member else {
                        return Err(reader.err(expression, "named props member required"));
                    };
                    if !object.attrs.is_empty()
                        || object.qself.is_some()
                        || !object.path.is_ident("props")
                    {
                        return Err(reader.err(expression, "plain props member required"));
                    }
                    Kind::Prop(name.to_string())
                }
                Expr::Paren(value) if value.attrs.is_empty() => {
                    return visit(reader, &value.expr, depth + 1, steps);
                }
                Expr::Group(value) if value.attrs.is_empty() => {
                    return visit(reader, &value.expr, depth + 1, steps);
                }
                Expr::Unary(value)
                    if value.attrs.is_empty() && matches!(value.op, syn::UnOp::Not(_)) =>
                {
                    Kind::Not(Box::new(visit(reader, &value.expr, depth + 1, steps)?))
                }
                Expr::Binary(value) if value.attrs.is_empty() => {
                    if matches!(value.op, BinOp::Eq(_) | BinOp::Ne(_)) {
                        if depth >= 64 {
                            return Err(reader
                                .err(expression, "boolean expression exceeds depth/node budget"));
                        }
                        let left = reader.scalar_expression(&value.left)?;
                        let right = reader.scalar_expression(&value.right)?;
                        return Ok(BoolExpression {
                            kind: if matches!(value.op, BinOp::Eq(_)) {
                                Kind::Equal(left, right)
                            } else {
                                Kind::NotEqual(left, right)
                            },
                            span: super::span(reader.source, expression.span()),
                        });
                    }
                    let left = Box::new(visit(reader, &value.left, depth + 1, steps)?);
                    let right = Box::new(visit(reader, &value.right, depth + 1, steps)?);
                    match value.op {
                        BinOp::And(_) => Kind::And(left, right),
                        BinOp::Or(_) => Kind::Or(left, right),
                        _ => {
                            return Err(
                                reader.err(expression, "typed predicate requires logical && or ||")
                            );
                        }
                    }
                }
                _ => return Err(reader.err(expression, "canonical typed props predicate required")),
            };
            Ok(BoolExpression {
                kind,
                span: super::span(reader.source, expression.span()),
            })
        }
        visit(self, expression, 0, &mut 0)
    }

    fn scalar_expression(&self, expression: &Expr) -> Result<ScalarExpression, Diagnostic> {
        let kind = match expression {
            Expr::Call(_) => match self.value(expression)? {
                crate::domain::Value::Member(member) => Scalar::Member(member),
                _ => return Err(self.err(expression, "canonical record comparison required")),
            },
            Expr::Lit(value) if value.attrs.is_empty() => match &value.lit {
                Lit::Str(value) => Scalar::String(value.value()),
                Lit::Bool(value) => Scalar::Bool(value.value),
                _ => {
                    return Err(self.err(
                        expression,
                        "comparison requires a string or boolean literal",
                    ));
                }
            },
            Expr::Field(field) if field.attrs.is_empty() => {
                let Expr::Path(object) = &*field.base else {
                    return Err(self.err(expression, "comparison requires a plain props member"));
                };
                let Member::Named(name) = &field.member else {
                    return Err(self.err(expression, "comparison requires a named props member"));
                };
                if !object.attrs.is_empty()
                    || object.qself.is_some()
                    || !object.path.is_ident("props")
                {
                    return Err(self.err(expression, "comparison requires a plain props member"));
                }
                Scalar::Prop(name.to_string())
            }
            _ => return Err(self.err(expression, "canonical scalar comparison operand required")),
        };
        Ok(ScalarExpression {
            kind,
            span: super::span(self.source, expression.span()),
        })
    }
}
