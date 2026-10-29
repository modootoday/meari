use crate::domain::{Diagnostic, Prop};
use syn::{Item, ItemStruct, spanned::Spanned};

impl super::Reader<'_> {
    pub(super) fn record_fields(
        &self,
        record: &ItemStruct,
        depth: usize,
    ) -> Result<Vec<Prop>, Diagnostic> {
        if depth > 32 || !record.generics.params.is_empty() || record.generics.where_clause.is_some()
            || record.attrs.iter().any(|attribute| {
                !matches!(&attribute.meta, syn::Meta::List(list)
                    if (attribute.path().is_ident("derive") && list.tokens.to_string() == "Clone")
                        || (attribute.path().is_ident("allow") && list.tokens.to_string() == "non_snake_case"))
            })
        {
            return Err(self.err(record, "record requires plain nonrecursive named fields"));
        }
        let syn::Fields::Named(fields) = &record.fields else {
            return Err(self.err(record, "record requires named fields"));
        };
        fields
            .named
            .iter()
            .map(|field| {
                if !field.attrs.is_empty() {
                    return Err(self.err(field, "record field attributes unsupported"));
                }
                Ok(Prop {
                    name: field.ident.as_ref().expect("named field").to_string(),
                    kind: self.prop_type(&field.ty, depth + 1)?,
                })
            })
            .collect()
    }

    pub(super) fn record_implementation(&self, item: &syn::ItemImpl) -> Result<bool, Diagnostic> {
        let syn::Type::Path(path) = item.self_ty.as_ref() else {
            return Ok(false);
        };
        if path.qself.is_some() || path.path.segments.len() != 1 {
            return Ok(false);
        }
        let name = path.path.segments[0].ident.to_string();
        let Some(record) = self.records.get(&name) else {
            return Ok(false);
        };
        let fields = self.record_fields(record, 0)?;
        let expected = crate::infrastructure::generate::records::implementations(&fields, &name);
        let expected_file = syn::parse_file(&expected)
            .map_err(|_| self.err(item, "invalid generated record conversion"))?;
        let actual_span = super::span(self.source, item.span());
        let actual = self
            .source
            .get(actual_span.start..actual_span.end)
            .ok_or_else(|| self.err(item, "record implementation source span missing"))?
            .parse::<proc_macro2::TokenStream>()
            .map_err(|_| self.err(item, "invalid record implementation tokens"))?
            .to_string();
        for candidate in &expected_file.items {
            if matches!(candidate, Item::Impl(_)) {
                let range = super::span(&expected, candidate.span());
                let tokens = expected[range.start..range.end]
                    .parse::<proc_macro2::TokenStream>()
                    .map_err(|_| self.err(item, "invalid canonical record implementation tokens"))?
                    .to_string();
                if actual == tokens {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}
