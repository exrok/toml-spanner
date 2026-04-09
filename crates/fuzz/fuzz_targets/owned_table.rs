#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};
use toml_spanner::OwnedTable;

fuzz_target!(|data: &[u8]| -> Corpus {
    let Ok(text) = std::str::from_utf8(data) else {
        return Corpus::Reject;
    };
    let arena = toml_spanner::Arena::new();
    let Ok(doc) = toml_spanner::parse(text, &arena) else {
        return Corpus::Keep;
    };
    let table = doc.table();
    let owned = OwnedTable::from(table);
    assert_eq!(table, owned.table());
    Corpus::Keep
});
