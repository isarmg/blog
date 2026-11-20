#![expect(clippy::missing_errors_doc)]

mod ast_impl;
mod diagnostics;
mod options;
mod parser;
mod surrogate_pair;

mod generated {
    #[cfg(debug_assertions)]
    mod assert_layouts;
    mod derive_clone_in;
    mod derive_content_eq;
}

pub mod ast;
pub use crate::regular_expression::{
    ast_impl::support::is_regexp_syntax_supported,
    ast_impl::visit,
    options::Options,
    parser::{ConstructorParser, LiteralParser},
};
