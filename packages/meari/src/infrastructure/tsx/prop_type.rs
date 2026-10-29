use crate::domain::{Diagnostic, PropType};
use oxc_ast::ast::{PropertyKey, TSSignature, TSType, TSTypeName};

pub(super) fn parse(kind: &TSType<'_>, depth: usize) -> Result<PropType, Diagnostic> {
    if depth > 32 {
        return Err(super::err(kind, "prop type exceeds array depth budget"));
    }
    match kind {
        TSType::TSTypeReference(reference)
            if matches!(&reference.type_name, TSTypeName::IdentifierReference(name) if name.name == "ReactNode")
                && reference.type_arguments.is_none() =>
        {
            Ok(PropType::Node)
        }
        TSType::TSStringKeyword(_) => Ok(PropType::String),
        TSType::TSBooleanKeyword(_) => Ok(PropType::Bool),
        TSType::TSTypeLiteral(record) => {
            let mut fields = Vec::new();
            for member in &record.members {
                let TSSignature::TSPropertySignature(field) = member else {
                    return Err(super::err(
                        member,
                        "record fields require named required types",
                    ));
                };
                if field.optional || field.computed || field.readonly {
                    return Err(super::err(
                        field.as_ref(),
                        "record fields must be required plain data",
                    ));
                }
                let PropertyKey::StaticIdentifier(name) = &field.key else {
                    return Err(super::err(
                        field.as_ref(),
                        "record field requires a portable identifier",
                    ));
                };
                let annotation = field
                    .type_annotation
                    .as_ref()
                    .ok_or_else(|| super::err(field.as_ref(), "record field type required"))?;
                fields.push(crate::domain::Prop {
                    name: name.name.to_string(),
                    kind: parse(&annotation.type_annotation, depth + 1)?,
                });
            }
            let kind = PropType::Record(fields);
            kind.validate()
                .map_err(|message| super::err(record.as_ref(), &message))?;
            Ok(kind)
        }
        TSType::TSFunctionType(function) => {
            Ok(PropType::Callback(super::callbacks::signature(function)?))
        }
        TSType::TSArrayType(array) => Ok(PropType::Array(Box::new(parse(
            &array.element_type,
            depth + 1,
        )?))),
        TSType::TSTypeReference(reference) if matches!(&reference.type_name, TSTypeName::IdentifierReference(name) if name.name == "Array") =>
        {
            let arguments = reference
                .type_arguments
                .as_ref()
                .ok_or_else(|| super::err(kind, "Array requires its element type"))?;
            if arguments.params.len() != 1 {
                return Err(super::err(kind, "Array requires exactly one element type"));
            }
            Ok(PropType::Array(Box::new(parse(
                &arguments.params[0],
                depth + 1,
            )?)))
        }
        _ => Err(super::err(
            kind,
            "prop type requires string, boolean, a typed callback or Array<T>",
        )),
    }
}
