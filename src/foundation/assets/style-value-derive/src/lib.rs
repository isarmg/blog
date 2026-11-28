use proc_macro::TokenStream;

mod parse;
mod to_css;
mod visit;

#[proc_macro_derive(Visit, attributes(visit, skip_visit, skip_type, visit_types))]
pub fn derive_visit_children(input: TokenStream) -> TokenStream {
  visit::derive_visit_children(input)
}

#[proc_macro_derive(Parse, attributes(css))]
pub fn derive_parse(input: TokenStream) -> TokenStream {
  parse::derive_parse(input)
}

#[proc_macro_derive(ToCss, attributes(css))]
pub fn derive_to_css(input: TokenStream) -> TokenStream {
  to_css::derive_to_css(input)
}

/// Spells an ASCII CSS enum keyword without splitting digits from its word.
#[proc_macro]
pub fn css_keyword(input: TokenStream) -> TokenStream {
  use convert_case::{Boundary, Case, Casing};

  let name = syn::parse_macro_input!(input as syn::Ident).to_string();
  let boundaries = [Boundary::Underscore, Boundary::LowerUpper, Boundary::Acronym];
  let keyword = name.with_boundaries(&boundaries).to_case(Case::Kebab);
  quote::quote!(#keyword).into()
}
