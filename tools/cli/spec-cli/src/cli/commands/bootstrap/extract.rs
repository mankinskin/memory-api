use spec_api::code_ref::SymbolKind;
use syn::{
    Attribute,
    Item,
    ItemImpl,
    Visibility,
    spanned::Spanned,
};

#[derive(Debug)]
pub(super) struct ExtractedItem {
    pub(super) name: String,
    pub(super) kind: SymbolKind,
    pub(super) line_start: u32,
    pub(super) line_end: u32,
    pub(super) doc_comment: String,
}

pub(super) fn extract_public_items(ast: &syn::File) -> Vec<ExtractedItem> {
    let mut items = Vec::new();
    for item in &ast.items {
        collect_item(item, &mut items);
    }
    items
}

fn collect_item(
    item: &Item,
    out: &mut Vec<ExtractedItem>,
) {
    match item {
        Item::Struct(item_struct) if is_pub(&item_struct.vis) => {
            out.push(ExtractedItem {
                name: item_struct.ident.to_string(),
                kind: SymbolKind::Struct,
                line_start: span_line_start(item_struct.ident.span()),
                line_end: span_line_end(item_struct.span()),
                doc_comment: extract_doc_comment(&item_struct.attrs),
            });
        },
        Item::Enum(item_enum) if is_pub(&item_enum.vis) => {
            out.push(ExtractedItem {
                name: item_enum.ident.to_string(),
                kind: SymbolKind::Enum,
                line_start: span_line_start(item_enum.ident.span()),
                line_end: span_line_end(item_enum.span()),
                doc_comment: extract_doc_comment(&item_enum.attrs),
            });
        },
        Item::Trait(item_trait) if is_pub(&item_trait.vis) => {
            out.push(ExtractedItem {
                name: item_trait.ident.to_string(),
                kind: SymbolKind::Trait,
                line_start: span_line_start(item_trait.ident.span()),
                line_end: span_line_end(item_trait.span()),
                doc_comment: extract_doc_comment(&item_trait.attrs),
            });
        },
        Item::Fn(item_fn) if is_pub(&item_fn.vis) => {
            out.push(ExtractedItem {
                name: item_fn.sig.ident.to_string(),
                kind: SymbolKind::Function,
                line_start: span_line_start(item_fn.sig.ident.span()),
                line_end: span_line_end(item_fn.span()),
                doc_comment: extract_doc_comment(&item_fn.attrs),
            });
        },
        Item::Impl(item_impl) => {
            out.push(ExtractedItem {
                name: impl_type_name(item_impl),
                kind: SymbolKind::Impl,
                line_start: span_line_start(item_impl.self_ty.span()),
                line_end: span_line_end(item_impl.span()),
                doc_comment: extract_doc_comment(&item_impl.attrs),
            });
        },
        Item::Mod(item_mod) if is_pub(&item_mod.vis) => {
            if let Some((_, items)) = &item_mod.content {
                for inner in items {
                    collect_item(inner, out);
                }
            }
        },
        _ => {},
    }
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn span_line_start(span: proc_macro2::Span) -> u32 {
    span.start().line as u32
}

fn span_line_end(span: proc_macro2::Span) -> u32 {
    span.end().line as u32
}

fn impl_type_name(item_impl: &ItemImpl) -> String {
    match &*item_impl.self_ty {
        syn::Type::Path(type_path) => type_path
            .path
            .segments
            .last()
            .map(|segment| {
                if let Some(trait_) = &item_impl.trait_ {
                    let trait_name = trait_
                        .1
                        .segments
                        .last()
                        .map(|trait_segment| trait_segment.ident.to_string())
                        .unwrap_or_default();
                    format!("{}::{}", segment.ident, trait_name)
                } else {
                    segment.ident.to_string()
                }
            })
            .unwrap_or_else(|| "impl".to_string()),
        _ => "impl".to_string(),
    }
}

fn extract_doc_comment(attrs: &[Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(name_value) = &attr.meta {
                if let syn::Expr::Lit(lit) = &name_value.value {
                    if let syn::Lit::Str(string_lit) = &lit.lit {
                        lines.push(string_lit.value().trim().to_string());
                    }
                }
            }
        }
    }
    lines.join("\n")
}
