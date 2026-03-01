#![allow(elided_lifetimes_in_paths)]
#![allow(dead_code)]
mod ast;
mod case;
mod codegen;
mod lit;
mod util;
mod writer;

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

#[derive(Debug)]
struct InnerError {
    span: Span,
    message: String,
}

#[derive(Debug)]
pub(crate) struct Error(InnerError);

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
    pub(crate) fn from_ctx() -> Error {
        Error(InnerError {
            span: Span::call_site(),
            message: "Error in context".to_string(),
        })
    }
    pub fn throw(self) -> ! {
        std::panic::panic_any(self);
    }
    pub(crate) fn msg(message: &str) -> ! {
        Error(InnerError {
            span: Span::call_site(),
            message: message.to_string(),
        })
        .throw();
    }

    pub(crate) fn msg_ctx(message: &str, fmt: &dyn std::fmt::Display) -> ! {
        Error(InnerError {
            span: Span::call_site(),
            message: format!("{}: {}", message, fmt),
        })
        .throw();
    }
    pub(crate) fn span_msg(message: &str, span: Span) -> ! {
        Error(InnerError {
            span,
            message: message.to_string(),
        })
        .throw();
    }
    pub(crate) fn span_msg_ctx(message: &str, fmt: &dyn std::fmt::Display, span: Span) -> ! {
        Error(InnerError {
            span,
            message: format!("{}: {}", fmt, message),
        })
        .throw()
    }
}

#[proc_macro_derive(Toml, attributes(toml))]
pub fn derive_toml(input: TokenStream) -> TokenStream {
    codegen::derive(input)
}
