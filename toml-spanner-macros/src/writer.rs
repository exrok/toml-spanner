pub mod cache;
use proc_macro::{Delimiter, Group, Ident, Punct, Span, TokenStream, TokenTree};

pub struct RustWriter {
    pub buf: Vec<TokenTree>,
    pub default_span: Span,
    pub ident: [Option<Ident>; cache::IDENT_SIZE],
    pub punct: [Punct; cache::PUNCT_SIZE],
}

impl RustWriter {
    pub fn new() -> Self {
        RustWriter {
            buf: Vec::new(),
            default_span: Span::mixed_site(),
            ident: [const { None }; cache::IDENT_SIZE],
            punct: cache::punct_cache_initial_state(),
        }
    }
    #[inline(never)]
    pub fn tt_group(&mut self, delimiter: Delimiter, from: usize) {
        let group = TokenTree::Group(Group::new(
            delimiter,
            TokenStream::from_iter(self.buf.drain(from..)),
        ));
        self.buf.push(group);
    }
    pub fn split_off_stream(&mut self, from: usize) -> TokenStream {
        TokenStream::from_iter(self.buf.drain(from..))
    }
    pub fn tt_group_empty(&mut self, delimiter: Delimiter) {
        let group = TokenTree::Group(Group::new(delimiter, TokenStream::new()));
        self.buf.push(group);
    }
    #[allow(dead_code)]
    #[inline(never)]
    pub fn blit(&mut self, start: u32, len: u32) {
        let start = start as usize;
        let len = len as usize;
        let src = &cache::BLIT_SRC[start..start + len];
        self.buf.extend(src.iter().map(|i| {
            let index = *i as usize;
            if let Some(punct) = self.punct.get(index) {
                return TokenTree::Punct(punct.clone());
            }
            let index_index = index - self.punct.len();
            if let Some(ident) = self.ident.get_mut(index_index) {
                match ident {
                    None => {
                        let re = Ident::new(cache::NAMES[index_index], self.default_span);
                        let ret = re.clone();
                        *ident = Some(re);
                        return TokenTree::Ident(ret);
                    }
                    Some(ident) => return TokenTree::Ident(ident.clone()),
                }
            }
            let idx = index - self.punct.len() - self.ident.len();
            let del = match idx {
                0 => Delimiter::Parenthesis,
                1 => Delimiter::Brace,
                _ => Delimiter::Bracket,
            };
            TokenTree::Group(Group::new(del, TokenStream::new()))
        }));
    }
    #[allow(dead_code)]
    pub fn blit_punct(&mut self, index: usize) {
        self.buf.push(TokenTree::Punct(self.punct[index].clone()));
    }

    #[allow(dead_code)]
    pub fn push_ident(&mut self, ident: &Ident) {
        self.buf.push(TokenTree::Ident(ident.clone()));
    }

    #[allow(dead_code)]
    #[inline(never)]
    pub fn blit_ident(&mut self, index: usize) {
        let entry = &mut self.ident[index];
        match entry {
            None => {
                let ident = Ident::new(cache::NAMES[index], self.default_span);
                self.buf.push(TokenTree::Ident(ident.clone()));
                *entry = Some(ident);
            }
            Some(ident) => self.buf.push(TokenTree::Ident(ident.clone())),
        }
    }
}
