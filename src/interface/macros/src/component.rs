use crate::diagnostics::{Errors, message_with_help};
use crate::names::{pascal, snake};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, TokenStreamExt, format_ident, quote, quote_spanned};
use syn::{
    AngleBracketedGenericArguments, Attribute, FnArg, GenericArgument, GenericParam, Item, ItemFn,
    LitStr, Meta, Pat, PatIdent, Path, PathArguments, Signature, Stmt, Type, TypeImplTrait,
    TypeParam, TypePath, Visibility, parse::Parse, spanned::Spanned, token::Colon,
    visit_mut::VisitMut,
};

pub struct Model {
    signature: syn::Result<(Docs, Vec<Prop>)>,
    name: Ident,
    body: ItemFn,
}

impl Parse for Model {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut item = ItemFn::parse(input)?;

        convert_impl_trait_to_generic(&mut item.sig);

        let signature = Docs::new(&item.attrs).and_then(|docs| {
            let props = std::mem::take(&mut item.sig.inputs)
                .into_iter()
                .map(Prop::new)
                .collect::<syn::Result<_>>()?;
            Ok((docs, props))
        });
        Ok(Self {
            signature,
            name: convert_from_snake_case(&item.sig.ident)?,
            body: item,
        })
    }
}

fn convert_from_snake_case(name: &Ident) -> syn::Result<Ident> {
    let name_str = name.to_string();
    let name_str = name_str.strip_prefix("r#").unwrap_or(&name_str);
    if name_str == snake(name_str) {
        let mut converted = syn::parse_str::<Ident>(&pascal(name_str)).map_err(|_| {
            syn::Error::new(
                name.span(),
                "component name must form a valid Rust identifier",
            )
        })?;
        converted.set_span(name.span());
        Ok(converted)
    } else {
        Ok(name.clone())
    }
}

impl Model {
    pub(crate) fn expand(
        self,
        is_transparent: bool,
        errors: &mut Errors,
    ) -> syn::Result<TokenStream> {
        let Self {
            signature,
            name,
            body,
        } = self;
        let (docs, props) = signature?;
        let unknown_attrs = body.attrs.iter().filter(|attr| {
            !is_lint_attr(attr)
                && !matches!(&attr.meta, Meta::NameValue(value) if value.path.is_ident("doc"))
        });
        let vis = &body.vis;
        let ret = &body.sig.output;
        if !is_transparent {
            let ends_semi = body.block.stmts.last().and_then(|stmt| match stmt {
                Stmt::Item(Item::Macro(mac)) => mac.semi_token.as_ref(),
                Stmt::Local(_) | Stmt::Item(_) | Stmt::Expr(..) | Stmt::Macro(_) => None,
            });
            if let Some(semi) = ends_semi {
                errors.push(syn::Error::new(
                    semi.span(),
                    "A component ending in a macro followed by a semicolon returns an empty view. Return () explicitly if that is intended.",
                ));
            }
        }
        let (impl_generics, generics, where_clause) = body.sig.generics.split_for_impl();
        let props_name = format_ident!("{name}Props");
        let props_builder_name = format_ident!("{name}PropsBuilder");
        let prop_builder_fields = prop_builder_fields(vis, &props)?;
        let prop_names = prop_names(&props);
        let builder_name_doc = LitStr::new(
            &format!(" Props for the [`{name}`] component."),
            name.span(),
        );
        let component_fn_prop_docs = generate_component_fn_prop_docs(&props)?;
        let docs_and_prop_docs = if component_fn_prop_docs.is_empty() {
            quote! { #docs }
        } else {
            quote! { #docs #[doc = ""] #component_fn_prop_docs }
        };
        let body_name = unmodified_fn_name_from_fn_name(&body.sig.ident);
        let body_expr = quote! { #body_name(#prop_names) };
        let component = if is_transparent {
            body_expr
        } else {
            quote! {
                ::blog_interface::reactive::graph::untrack_with_diagnostics(move || {
                    #body_expr
                })
            }
        };
        let (props_arg, destructure_props) = if props.is_empty() {
            (quote! {}, quote! {})
        } else {
            (
                quote! { props: #props_name #generics },
                quote! { let #props_name { #prop_names } = props; },
            )
        };
        Ok(quote! {
            #[doc = #builder_name_doc]
            #[doc = ""]
            #docs_and_prop_docs
            #[derive(::blog_interface::typed_builder_macro::TypedBuilder)]
            #[builder(crate_module_path=::blog_interface::typed_builder)]
            #[allow(non_snake_case)]
            #vis struct #props_name #impl_generics #where_clause {
                #prop_builder_fields
            }
            impl #impl_generics ::blog_interface::component::Props for #props_name #generics #where_clause {
                type Builder = #props_builder_name #generics;
                fn builder() -> Self::Builder { #props_name::builder() }
            }
            #(#unknown_attrs)*
            #docs_and_prop_docs
            #[allow(non_snake_case, clippy::too_many_arguments)]
            #[allow(clippy::needless_lifetimes)]
            #vis fn #name #impl_generics (#props_arg) #ret #where_clause {
                #destructure_props
                #component
            }
        })
    }
}

/// A model that is more lenient in case of a syntax error in the function body,
/// but does not actually implement the behavior of the real model. This is
/// used to improve IDEs and rust-analyzer's auto-completion behavior in case
/// of a syntax error.
pub struct DummyModel {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub sig: Signature,
    pub body: TokenStream,
}

impl Parse for DummyModel {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut attrs = input.call(Attribute::parse_outer)?;
        // Drop unknown attributes like #[deprecated]
        attrs.retain(|attr| is_lint_attr(attr) || attr.path().is_ident("doc"));

        let vis: Visibility = input.parse()?;
        let mut sig: Signature = input.parse()?;
        for arg in &mut sig.inputs {
            if let FnArg::Typed(ty) = arg {
                ty.attrs.retain(|attr| {
                    let (path, hidden) = match &attr.meta {
                        Meta::List(list) => (&list.path, "prop"),
                        Meta::NameValue(value) => (&value.path, "doc"),
                        Meta::Path(_) => return true,
                    };
                    path.segments
                        .first()
                        .is_none_or(|segment| segment.ident != hidden)
                });
            }
        }

        // The body is left untouched, so it will not cause an error
        // even if the syntax is invalid.
        let body: TokenStream = input.parse()?;

        Ok(Self {
            attrs,
            vis,
            sig,
            body,
        })
    }
}

impl ToTokens for DummyModel {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            attrs,
            vis,
            sig,
            body,
        } = self;

        let output = quote! {
            #(#attrs)*
            #vis #sig #body
        };

        tokens.append_all(output);
    }
}

struct Prop {
    docs: Docs,
    options: PropOpt,
    name: PatIdent,
    ty: Type,
}

fn prop_identifier(name: &str, span: Span) -> syn::Result<Ident> {
    let invalid = || {
        syn::Error::new(
            span,
            "prop name must be a valid Rust identifier; use r#name for a keyword",
        )
    };
    // parse_str consumes exactly one identifier but permits surrounding trivia.
    // Reject trivia explicitly: compiler identifiers normalize NFD to NFC, so
    // comparing their printed spelling to the original string would reject
    // valid Unicode names even though fallback parser tests accept them.
    if name.chars().any(|character| {
        character.is_whitespace() || matches!(character, '/' | '\u{200e}' | '\u{200f}' | '\u{feff}')
    }) {
        return Err(invalid());
    }
    let mut identifier = syn::parse_str::<Ident>(name).map_err(|_| invalid())?;
    identifier.set_span(span);
    Ok(identifier)
}

impl Prop {
    fn new(arg: FnArg) -> syn::Result<Self> {
        let FnArg::Typed(typed) = arg else {
            return Err(syn::Error::new_spanned(arg, "receiver not allowed in `fn`"));
        };

        let options = PropOpt::from_attributes(&typed.attrs)?;

        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "syn patterns are non-exhaustive; other prop patterns produce a boundary diagnostic"
        )]
        let name = match *typed.pat {
            Pat::Ident(i) => {
                if let Some((name, span)) = &options.name {
                    PatIdent {
                        attrs: vec![],
                        by_ref: None,
                        mutability: None,
                        ident: prop_identifier(name, *span)?,
                        subpat: None,
                    }
                } else {
                    i
                }
            }
            pat @ (Pat::Struct(_) | Pat::Tuple(_) | Pat::TupleStruct(_)) => {
                let Some((name, span)) = options.name.as_ref() else {
                    return Err(syn::Error::new_spanned(
                        pat,
                        "destructured props must be given a name e.g. \
                         #[prop(name = \"data\")]",
                    ));
                };
                PatIdent {
                    attrs: vec![],
                    by_ref: None,
                    mutability: None,
                    ident: prop_identifier(name, *span)?,
                    subpat: None,
                }
            }
            pat => {
                return Err(syn::Error::new_spanned(
                    pat,
                    "only `prop: bool` style types are allowed within the \
                     `#[component]` macro",
                ));
            }
        };

        Ok(Self {
            docs: Docs::new(&typed.attrs)?,
            options,
            name,
            ty: *typed.ty,
        })
    }
}

pub struct Docs(Vec<(String, Span)>);

impl ToTokens for Docs {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let s = self
            .0
            .iter()
            .map(|(doc, span)| quote_spanned!(*span=> #[doc = #doc]))
            .collect::<TokenStream>();

        tokens.append_all(s);
    }
}

impl Docs {
    pub fn new(attrs: &[Attribute]) -> syn::Result<Self> {
        let docs = attrs
            .iter()
            .filter_map(|attribute| {
                let Meta::NameValue(attribute) = &attribute.meta else {
                    return None;
                };
                if !attribute.path.is_ident("doc") {
                    return None;
                }
                Some(
                    value_to_string(&attribute.value)
                        .map(|value| (value, attribute.path.span()))
                        .ok_or_else(|| {
                            syn::Error::new_spanned(
                                attribute,
                                "expected string literal in value of doc comment",
                            )
                        }),
                )
            })
            .collect::<syn::Result<Vec<_>>>()?;
        Ok(Self(docs))
    }

    pub fn padded(&self) -> TokenStream {
        self.0
            .iter()
            .enumerate()
            .map(|(idx, (doc, span))| {
                let doc = if idx == 0 {
                    format!("    - {doc}")
                } else {
                    format!("      {doc}")
                };

                let doc = LitStr::new(&doc, *span);

                quote! { #[doc = #doc] }
            })
            .collect()
    }

    pub fn typed_builder(&self) -> String {
        let doc_str = self
            .0
            .iter()
            .map(|s| s.0.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        if doc_str.chars().any(|c| c != '\n') {
            format!("\n\n{doc_str}")
        } else {
            String::new()
        }
    }
}

fn is_lint_attr(attr: &Attribute) -> bool {
    let path = &attr.path();
    path.is_ident("allow")
        || path.is_ident("warn")
        || path.is_ident("expect")
        || path.is_ident("deny")
        || path.is_ident("forbid")
}

#[derive(Debug, Default)]
struct PropOpt {
    mode: PropMode,
    default: Option<syn::Expr>,
    into: bool,
    attrs: bool,
    name: Option<(String, Span)>,
    marker: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PropMode {
    #[default]
    Required,
    Optional,
    OptionalNoStrip,
    StripOption,
}

impl PropOpt {
    fn from_attributes(attributes: &[Attribute]) -> syn::Result<Self> {
        let mut result = Self::default();
        let mut seen = std::collections::HashMap::new();
        for attribute in attributes
            .iter()
            .filter(|attr| attr.path().is_ident("prop"))
        {
            attribute.parse_nested_meta(|meta| {
                let ident = meta
                    .path
                    .get_ident()
                    .ok_or_else(|| meta.error("expected a prop option"))?;
                let key = ident.to_string();
                let std::collections::hash_map::Entry::Vacant(entry) = seen.entry(key.clone())
                else {
                    return Err(meta.error(format!("duplicate prop option `{key}`")));
                };
                let enabled = match key.as_str() {
                    "optional" | "optional_no_strip" | "strip_option" | "into" | "attrs"
                    | "marker" => {
                        if meta.input.peek(syn::Token![=]) {
                            meta.value()?.parse::<syn::LitBool>()?.value
                        } else {
                            true
                        }
                    }
                    "default" => {
                        result.default = Some(meta.value()?.parse()?);
                        true
                    }
                    "name" => {
                        let value = meta.value()?.parse::<LitStr>()?;
                        result.name = Some((value.value(), value.span()));
                        true
                    }
                    _ => return Err(meta.error(format!("unknown prop option `{key}`"))),
                };
                entry.insert((ident.span(), enabled));
                Ok(())
            })?;
        }
        let enabled = |key: &str| seen.get(key).is_some_and(|&(_, active)| active);
        let mut modes = [
            ("optional", PropMode::Optional),
            ("optional_no_strip", PropMode::OptionalNoStrip),
            ("strip_option", PropMode::StripOption),
        ]
        .into_iter()
        .filter(|&(key, _)| enabled(key));
        if let Some((key, mode)) = modes.next() {
            if modes.next().is_some() {
                return Err(syn::Error::new(seen[key].0, "conflicting prop options"));
            }
            result.mode = mode;
        }
        result.marker = enabled("marker");
        if result.marker
            && seen
                .iter()
                .any(|(key, &(_, active))| key != "marker" && active)
        {
            return Err(syn::Error::new(
                seen["marker"].0,
                "conflicting prop options",
            ));
        }
        result.into = enabled("into");
        result.attrs = enabled("attrs");
        Ok(result)
    }

    fn optional(&self) -> bool {
        matches!(self.mode, PropMode::Optional | PropMode::OptionalNoStrip)
    }

    fn builder_attribute(&self, ty: &Type) -> syn::Result<TokenStream> {
        if self.marker {
            return Ok(quote! { #[builder(default, setter(skip))] });
        }
        let default = if let Some(expr) = &self.default {
            let code = expr.to_token_stream().to_string();
            quote! { default_code=#code, }
        } else if self.optional() || self.attrs {
            quote! { default, }
        } else {
            quote! {}
        };
        let strip =
            self.mode == PropMode::StripOption || self.mode == PropMode::Optional && is_option(ty);
        let setter = match (self.into, strip) {
            (true, strip_option) => {
                let (parameter, result, body) = if strip_option {
                    let inner = unwrap_option(ty)?;
                    (
                        inner,
                        quote!(Option<#inner>),
                        quote!(Some(value.into_reactive_value())),
                    )
                } else {
                    (ty, quote!(#ty), quote!(value.into_reactive_value()))
                };
                quote! { setter(fn transform<__IntoReactiveValueMarker>(
                    value: impl ::blog_interface::prelude::IntoReactiveValue<#parameter, __IntoReactiveValueMarker>
                ) -> #result { #body },) }
            }
            (false, true) => quote!(setter(strip_option,)),
            (false, false) => quote!(),
        };
        Ok(if default.is_empty() && setter.is_empty() {
            quote! {}
        } else {
            quote! { #[builder(#default #setter)] }
        })
    }
}

fn prop_builder_fields(vis: &Visibility, props: &[Prop]) -> syn::Result<TokenStream> {
    props
        .iter()
        .map(|prop| {
            let Prop {
                docs,
                name,
                options,
                ty,
            } = prop;

            let PatIdent { ident, by_ref, .. } = &name;

            let builder_attrs = options.builder_attribute(ty)?;

            let builder_docs = prop_to_doc(prop, PropDocStyle::Inline)?;

            // Children and markers won't need documentation in many cases
            let allow_missing_docs = if options.marker || ident == "children" {
                quote!(#[allow(missing_docs)])
            } else {
                quote!()
            };
            Ok(quote! {
                #docs
                #builder_docs
                #builder_attrs
                #allow_missing_docs
                #vis #by_ref #ident: #ty,
            })
        })
        .collect()
}

fn prop_names(props: &[Prop]) -> TokenStream {
    props
        .iter()
        .map(|Prop { name, .. }| {
            // fields like mutability are removed because unneeded
            // in the contexts in which this is used
            let ident = &name.ident;
            quote! { #ident, }
        })
        .collect()
}

fn generate_component_fn_prop_docs(props: &[Prop]) -> syn::Result<TokenStream> {
    let section = |optional, title| -> syn::Result<TokenStream> {
        let fields = props
            .iter()
            .filter(|p| {
                !p.options.marker
                    && (p.options.optional() || p.options.default.is_some()) == optional
            })
            .map(|p| prop_to_doc(p, PropDocStyle::List))
            .collect::<syn::Result<TokenStream>>()?;
        Ok(if fields.is_empty() {
            fields
        } else {
            quote! { #[doc = #title] #fields }
        })
    };
    let required = section(false, " # Required Props")?;
    let optional = section(true, " # Optional Props")?;
    Ok(quote! { #required #optional })
}

pub fn is_option(ty: &Type) -> bool {
    option_segment(ty).is_some()
}

fn option_segment(ty: &Type) -> Option<&syn::PathSegment> {
    let Type::Path(path) = ty else { return None };
    let segments = &path.path.segments;
    segments
        .first()
        .filter(|segment| segments.len() == 1 && segment.ident == "Option")
}

pub fn unwrap_option(ty: &Type) -> syn::Result<&Type> {
    const STD_OPTION_MSG: &str = "make sure you're not shadowing the `std::option::Option` type that \
         is automatically imported from the standard prelude";

    if let Some(segment) = option_segment(ty)
        && let PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) =
            &segment.arguments
        && args.len() == 1
        && let Some(GenericArgument::Type(inner)) = args.first()
    {
        return Ok(inner);
    }
    Err(syn::Error::new_spanned(
        ty,
        message_with_help("`Option` must be `std::option::Option`", STD_OPTION_MSG),
    ))
}

#[derive(Clone, Copy)]
enum PropDocStyle {
    List,
    Inline,
}

fn prop_to_doc(
    Prop {
        docs,
        name,
        ty,
        options,
    }: &Prop,
    style: PropDocStyle,
) -> syn::Result<TokenStream> {
    let ty = if matches!(options.mode, PropMode::Optional | PropMode::StripOption) && is_option(ty)
    {
        unwrap_option(ty)?
    } else {
        ty
    };

    // Documentation displays this already-parsed type; a second full Rust
    // source printer is unnecessary for either generated code or validation.
    let pretty_ty = ty.to_token_stream().to_string();

    match style {
        PropDocStyle::List => {
            let arg_ty_doc = LitStr::new(
                &if options.into {
                    format!(
                        " - **{}**: [`impl Into<{pretty_ty}>`]({pretty_ty})",
                        quote!(#name),
                    )
                } else {
                    format!(" - **{}**: [`{pretty_ty}`]", quote!(#name))
                },
                name.ident.span(),
            );

            let arg_user_docs = docs.padded();

            Ok(quote! {
                #[doc = #arg_ty_doc]
                #arg_user_docs
            })
        }
        PropDocStyle::Inline => {
            let arg_ty_doc = LitStr::new(
                &if options.into {
                    format!(
                        "**{}**: `impl`[`Into<{}>`]{}",
                        quote!(#name),
                        pretty_ty,
                        docs.typed_builder()
                    )
                } else {
                    format!(
                        "**{}**: [`{}`]{}",
                        quote!(#name),
                        pretty_ty,
                        docs.typed_builder()
                    )
                },
                name.ident.span(),
            );

            Ok(quote! {
                #[builder(setter(doc = #arg_ty_doc))]
            })
        }
    }
}

pub fn unmodified_fn_name_from_fn_name(ident: &Ident) -> Ident {
    let spelling = ident.to_string();
    let spelling = spelling.strip_prefix("r#").unwrap_or(&spelling);
    Ident::new(&format!("__component_{}", snake(spelling)), ident.span())
}

/// Converts all `impl Trait`s in a function signature to use generic params instead.
fn convert_impl_trait_to_generic(sig: &mut Signature) {
    fn new_generic_ident(i: usize, span: Span) -> Ident {
        Ident::new(&format!("__ImplTrait{i}"), span)
    }

    // First: visit all `impl Trait`s and replace them with new generic params.
    #[derive(Default)]
    struct RemoveImplTrait(Vec<TypeImplTrait>);
    impl VisitMut for RemoveImplTrait {
        fn visit_type_mut(&mut self, ty: &mut Type) {
            syn::visit_mut::visit_type_mut(self, ty);
            if matches!(ty, Type::ImplTrait(_)) {
                let ident = new_generic_ident(self.0.len(), ty.span());
                let generic_type = Type::Path(TypePath {
                    attrs: Vec::new(),
                    qself: None,
                    path: Path::from(ident),
                });
                let Type::ImplTrait(impl_trait) = std::mem::replace(ty, generic_type) else {
                    unreachable!();
                };
                self.0.push(impl_trait);
            }
        }

        // Early exits.
        fn visit_attribute_mut(&mut self, _: &mut Attribute) {}
        fn visit_pat_mut(&mut self, _: &mut Pat) {}
    }
    let mut visitor = RemoveImplTrait::default();
    for fn_arg in &mut sig.inputs {
        visitor.visit_fn_arg_mut(fn_arg);
    }
    let RemoveImplTrait(impl_traits) = visitor;

    // Second: Add the new generic params into the signature.
    for (i, impl_trait) in impl_traits.into_iter().enumerate() {
        let span = impl_trait.span();
        let ident = new_generic_ident(i, span);
        // We can simply append to the end (only lifetime params must be first).
        // Note currently default generics are not allowed in `fn`, so not a concern.
        sig.generics.params.push(GenericParam::Type(TypeParam {
            attrs: vec![],
            ident,
            colon_token: Some(Colon { spans: [span] }),
            bounds: impl_trait.bounds,
            default: None,
        }));
    }
}

// Literal extraction formerly supplied by leptos_hot_reload::parsing.
// The original MIT source and archive identity remain in compliance provenance.
fn value_to_string(value: &syn::Expr) -> Option<String> {
    let syn::Expr::Lit(lit) = value else {
        return None;
    };
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "syn literals are non-exhaustive; only these displayable literal kinds carry documentation text"
    )]
    match &lit.lit {
        syn::Lit::Str(s) => Some(s.value()),
        syn::Lit::Char(c) => Some(c.value().to_string()),
        syn::Lit::Int(i) => Some(i.base10_digits().to_string()),
        syn::Lit::Float(f) => Some(f.base10_digits().to_string()),
        _ => None,
    }
}

#[path = "../../../../test/interface/macros/unit/component/maintained_doc_tests.rs"]
#[cfg(test)]
mod maintained_doc_tests;

#[path = "../../../../test/interface/macros/unit/component/prop_name_tests.rs"]
#[cfg(test)]
mod prop_name_tests;

#[path = "../../../../test/interface/macros/unit/component/component_name_tests.rs"]
#[cfg(test)]
mod component_name_tests;
