use super::IdentCacheEntry;
use proc_macro2::{Punct, Spacing};
pub static BLIT_SRC: &[u8] =
    b"\n\x0c\x0b\x00\x08\r\x05\x03\x06\x05\x03\t\x04\x00\x01\x0c\x0b\x04\x0b\x02\x07\r";
pub const IDENT_SIZE: usize = 6;
pub fn ident_cache_initial_state() -> Box<[IdentCacheEntry; IDENT_SIZE]> {
    Box::new([
        IdentCacheEntry::Empty("TextWriter"),
        IdentCacheEntry::Empty("into_string"),
        IdentCacheEntry::Empty("toml_spanner"),
        IdentCacheEntry::Empty("with_capacity"),
        IdentCacheEntry::Empty("let"),
        IdentCacheEntry::Empty("_builder"),
    ])
}
pub const PUNCT_SIZE: usize = 5;
pub fn punct_cache_initial_state() -> [Punct; PUNCT_SIZE] {
    [
        Punct::new('=', Spacing::Alone),
        Punct::new('&', Spacing::Alone),
        Punct::new('.', Spacing::Alone),
        Punct::new(':', Spacing::Alone),
        Punct::new(';', Spacing::Alone),
    ]
}
