#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};
use toml_spanner::Arena;

fuzz_target!(|data: &[u8]| -> Corpus {
    if data.len() < 4 {
        return Corpus::Reject;
    }

    let mut g = fuzz::Gen::new(data);
    let arena = Arena::new();
    let mut root = fuzz::gen_tree::gen_root_table(&mut g, &arena);

    let normalized = root.normalize();

    let mut buf1 = Vec::new();
    toml_spanner::emit(normalized, &mut buf1);
    let emitted = std::str::from_utf8(&buf1).expect("emit must produce valid UTF-8");

    let arena2 = Arena::new();
    let root2 = toml_spanner::parse(emitted, &arena2).unwrap_or_else(|e| {
        panic!("emitted output failed to parse!\nemitted:\n{emitted}\nerror: {e:?}")
    });

    if let Err(msg) = fuzz::gen_tree::items_eq(
        normalized.table().as_item(),
        root2.table().as_item(),
        &mut Vec::new(),
    ) {
        panic!("{msg}\nemitted:\n{emitted}");
    }

    let normalized2 = root2
        .table()
        .try_as_normalized()
        .expect("round-tripped table should be valid");
    let mut buf2 = Vec::new();
    toml_spanner::emit(normalized2, &mut buf2);
    assert!(
        buf1 == buf2,
        "emit is not idempotent!\nfirst emit:\n{emitted}\nsecond emit:\n{}",
        String::from_utf8_lossy(&buf2),
    );

    Corpus::Keep
});
