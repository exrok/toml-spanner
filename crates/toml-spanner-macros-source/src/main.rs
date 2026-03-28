#![allow(clippy::question_mark)]
#![allow(elided_lifetimes_in_paths)]
#![allow(dead_code)]
mod ast;
mod case;
mod codegen;
#[cfg(test)]
mod derive_tests;
mod lit;
mod template;
#[allow(unused)]
mod util;
mod writer;

use std::str::FromStr;

use proc_macro2::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

#[derive(Debug)]
struct InnerError {
    span: Span,
    message: String,
}
#[derive(Debug)]
pub(crate) struct Error(Box<InnerError>);

/// Error is technically not Send but only due to Span, which is `Copy`.
/// We need it to be Send to send over panics. We guarantee we will catch
/// the panic, and even if we didn't, since Span `Copy` it being dropped
/// on another thread isn't going to cause an issue.
unsafe impl Send for Error {}
impl Error {
    pub(crate) fn to_compiler_error(&self, wrap: bool) -> TokenStream {
        let mut group = TokenTree::Group(Group::new(
            Delimiter::Parenthesis,
            TokenStream::from_iter([TokenTree::Literal(Literal::string(&self.0.message))]),
        ));
        let mut punc = TokenTree::Punct(Punct::new('!', Spacing::Alone));
        punc.set_span(self.0.span);
        group.set_span(self.0.span);

        if wrap {
            TokenStream::from_iter([TokenTree::Group(Group::new(
                Delimiter::Brace,
                TokenStream::from_iter([
                    TokenTree::Ident(Ident::new("compile_error", self.0.span)),
                    punc,
                    group,
                    TokenTree::Punct(Punct::new(';', Spacing::Alone)),
                    TokenTree::Ident(Ident::new("String", self.0.span)),
                    TokenTree::Punct(Punct::new(':', Spacing::Joint)),
                    TokenTree::Punct(Punct::new(':', Spacing::Alone)),
                    TokenTree::Ident(Ident::new("new", self.0.span)),
                    TokenTree::Group(Group::new(Delimiter::Parenthesis, TokenStream::new())),
                ]),
            ))])
        } else {
            TokenStream::from_iter([
                TokenTree::Ident(Ident::new("compile_error", self.0.span)),
                punc,
                group,
                TokenTree::Punct(Punct::new(';', Spacing::Alone)),
            ])
        }
    }
    pub(crate) fn try_catch_handle(
        ts: TokenStream,
        func: fn(TokenStream) -> TokenStream,
    ) -> TokenStream {
        match std::panic::catch_unwind(move || func(ts)) {
            Ok(e) => e,
            Err(err) => {
                if let Some(value) = err.downcast_ref::<Error>() {
                    value.to_compiler_error(false)
                } else {
                    Error::from_ctx().to_compiler_error(false)
                }
            }
        }
    }
    #[track_caller]
    pub(crate) fn from_ctx() -> Error {
        Error(Box::new(InnerError {
            span: Span::call_site(),
            message: "Error in context".to_string(),
        }))
    }
    #[track_caller]
    pub(crate) fn msg(message: &str) -> ! {
        std::panic::panic_any(Error(Box::new(InnerError {
            span: Span::call_site(),
            message: message.to_string(),
        })));
    }

    #[track_caller]
    pub(crate) fn msg_ctx(message: &str, fmt: &dyn std::fmt::Display) -> ! {
        std::panic::panic_any(Error(Box::new(InnerError {
            span: Span::call_site(),
            message: format!("{}: {}", message, fmt),
        })));
    }
    #[track_caller]
    pub(crate) fn span_msg(message: &str, span: Span) -> ! {
        std::panic::panic_any(Error(Box::new(InnerError {
            span,
            message: message.to_string(),
        })));
    }
    #[track_caller]
    pub(crate) fn span_msg_ctx(message: &str, fmt: &dyn std::fmt::Display, span: Span) -> ! {
        std::panic::panic_any(Error(Box::new(InnerError {
            span,
            message: format!("{}: {}", fmt, message),
        })))
    }
}

#[allow(dead_code, unused)]
macro_rules! tokens {
    ($($tt:tt)*) => {
        TokenStream::from_str(stringify!($($tt)*)).unwrap()
    };
}

fn main() {
    if false {
        codegen::derive(tokens!());
    }
    util::print_pretty(codegen::derive(tokens! {
    #[derive(Toml, Debug)]
    struct SectionAliased {
        section: AliasedField,
    }
            }));
}
