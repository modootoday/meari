use super::Reader;
use crate::domain::{Diagnostic, hooks::HookSlot, state_program::StateBinding};
use syn::{Expr, Stmt};

pub(super) fn body<'a>(
    reader: &Reader<'_>,
    function: &'a syn::ItemFn,
) -> Result<(Vec<StateBinding>, Vec<HookSlot>, &'a Expr), Diagnostic> {
    let Some((Stmt::Expr(tail, None), declarations)) = function.block.stmts.split_last() else {
        return Err(reader.err(&function.block, "component requires tail expression"));
    };
    let mut states = Vec::new();
    let mut hooks = Vec::new();
    for statement in declarations {
        if let Stmt::Expr(Expr::Call(call), Some(_)) = statement
            && call.attrs.is_empty()
            && call.args.len() == 1
            && matches!(&*call.func, Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("use_id"))
        {
            hooks.push(HookSlot::Id(reader.string(&call.args[0])?));
            continue;
        }
        let Stmt::Expr(Expr::Try(value), Some(_)) = statement else {
            return Err(reader.err(statement, "state_declaration(...)? statement required"));
        };
        let Expr::Call(call) = &*value.expr else {
            return Err(reader.err(statement, "state declaration call required"));
        };
        if !call.attrs.is_empty()
            || !value.attrs.is_empty()
            || call.args.len() != 4
            || !matches!(&*call.func, Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("state_declaration"))
            || !matches!(&call.args[0], Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("state"))
        {
            return Err(reader.err(
                statement,
                "canonical state_declaration(state, name, setter, initial_json)? required",
            ));
        }
        let initial = serde_json::from_str(&reader.string(&call.args[3])?)
            .map_err(|_| reader.err(statement, "invalid state initializer JSON"))?;
        let name = reader.string(&call.args[1])?;
        hooks.push(HookSlot::State(name.clone()));
        states.push(StateBinding {
            name,
            setter: reader.string(&call.args[2])?,
            initial,
            span: super::span(reader.source, syn::spanned::Spanned::span(statement)),
        });
    }
    if states.is_empty() {
        if !hooks.is_empty() {
            return Err(reader.err(
                tail,
                "explicit ID hook declarations require mixed state layout",
            ));
        }
        return Ok((states, hooks, tail));
    }
    let Expr::Call(call) = tail else {
        return Err(reader.err(tail, "stateful component must return Ok(node)"));
    };
    if !call.attrs.is_empty()
        || call.args.len() != 1
        || !matches!(&*call.func,Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path.path.is_ident("Ok"))
    {
        return Err(reader.err(tail, "stateful component must return Ok(node)"));
    }
    if !hooks.iter().any(|hook| matches!(hook, HookSlot::Id(_))) {
        hooks.clear();
    }
    Ok((states, hooks, &call.args[0]))
}
