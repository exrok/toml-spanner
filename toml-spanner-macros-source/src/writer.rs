use proc_macro2::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};

pub struct RustWriter {
    pub buf: Vec<TokenTree>,
    cache: Cache,
}

struct Cache {
    default_span: Span,
}

impl RustWriter {
    pub fn new() -> Self {
        RustWriter {
            buf: Vec::new(),
            cache: Cache {
                default_span: Span::mixed_site(),
            },
        }
    }

    pub fn tt_punct_alone(&mut self, chr: char) {
        self.buf
            .push(TokenTree::Punct(Punct::new(chr, Spacing::Alone)));
    }

    pub fn tt_punct_joint(&mut self, chr: char) {
        self.buf
            .push(TokenTree::Punct(Punct::new(chr, Spacing::Joint)));
    }
    pub fn tt_ident(&mut self, ident: &str) {
        self.buf
            .push(TokenTree::Ident(Ident::new(ident, self.cache.default_span)));
    }
    pub fn tt_group(&mut self, delimiter: Delimiter, from: usize) {
        let group = TokenTree::Group(Group::new(
            delimiter,
            TokenStream::from_iter(self.buf.drain(from..)),
        ));
        self.buf.push(group);
    }
    pub fn tt_group_empty(&mut self, delimiter: Delimiter) {
        let group = TokenTree::Group(Group::new(delimiter, TokenStream::new()));
        self.buf.push(group);
    }
    pub fn split_off_stream(&mut self, from: usize) -> TokenStream {
        TokenStream::from_iter(self.buf.drain(from..))
    }

    #[allow(dead_code)]
    pub fn push_ident(&mut self, ident: &Ident) {
        self.buf.push(TokenTree::Ident(ident.clone()));
    }
}
