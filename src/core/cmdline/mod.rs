//! Command-line parsing: a quote-aware shell lexer, and the generic span-edit
//! type a caller uses to rewrite a segment it lexed once.

pub(crate) mod edit;
pub mod lexer;
