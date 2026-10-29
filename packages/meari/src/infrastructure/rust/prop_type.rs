use crate::domain::{Diagnostic, PropType, host_callbacks::CallbackSignature};
use syn::{GenericArgument, PathArguments, Type};

impl super::Reader<'_> {
    pub(super) fn prop_type(&self, kind: &Type, depth: usize) -> Result<PropType, Diagnostic> {
        if depth > 32 {
            return Err(self.err(kind, "prop type exceeds array depth budget"));
        }
        let Type::Path(path) = kind else {
            return Err(self.err(kind, "plain typed Props field required"));
        };
        if path.qself.is_some() {
            return Err(self.err(kind, "qualified Props type unsupported"));
        }
        for (name, ty) in [
            ("NodeSlot", PropType::Node),
            ("String", PropType::String),
            ("bool", PropType::Bool),
            (
                "ActionCallback",
                PropType::Callback(CallbackSignature::Action),
            ),
            ("TextCallback", PropType::Callback(CallbackSignature::Text)),
            (
                "BooleanCallback",
                PropType::Callback(CallbackSignature::Boolean),
            ),
        ] {
            if path.path.is_ident(name) {
                return Ok(ty);
            }
        }
        if path.path.segments.len() == 1 && path.path.segments[0].ident == "Vec" {
            let PathArguments::AngleBracketed(arguments) = &path.path.segments[0].arguments else {
                return Err(self.err(kind, "Vec requires an element type"));
            };
            if arguments.args.len() != 1 {
                return Err(self.err(kind, "Vec requires exactly one element type"));
            }
            let GenericArgument::Type(element) = &arguments.args[0] else {
                return Err(self.err(kind, "Vec element must be a supported type"));
            };
            return Ok(PropType::Array(Box::new(
                self.prop_type(element, depth + 1)?,
            )));
        }
        if path.path.segments.len() == 1
            && matches!(path.path.segments[0].arguments, PathArguments::None)
            && let Some(record) = self.records.get(&path.path.segments[0].ident.to_string())
        {
            return Ok(PropType::Record(self.record_fields(record, depth)?));
        }
        Err(self.err(
            kind,
            "Props requires String/bool/callback/Vec<T>/a declared record",
        ))
    }
}
