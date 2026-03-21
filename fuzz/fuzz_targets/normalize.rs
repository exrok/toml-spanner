#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};
use toml_spanner::Arena;

fuzz_target!(|data: &[u8]| -> Corpus {
    if data.len() < 4 {
        return Corpus::Reject;
    }

    let mut g = fuzz::Gen::new(data);
    let arena = Arena::new();
    let root = fuzz::gen_tree::gen_root_table(&mut g, &arena);

    let buf1 = toml_spanner::Formatting::default()
        .format_table_to_bytes(root, &arena);
    let emitted = std::str::from_utf8(&buf1).expect("emit must produce valid UTF-8");

    let arena2 = Arena::new();
    let root2 = toml_spanner::parse(emitted, &arena2).unwrap_or_else(|e| {
        panic!("emitted output failed to parse!\nemitted:\n{emitted}\nerror: {e:?}")
    });

    // Idempotency: re-emitting the parsed output must produce identical bytes.
    let buf2 = toml_spanner::Formatting::default()
        .format_table_to_bytes(root2.into_table(), &arena2);
    assert!(
        buf1 == buf2,
        "emit is not idempotent!\nfirst emit:\n{emitted}\nsecond emit:\n{}",
        String::from_utf8_lossy(&buf2),
    );

    Corpus::Keep
});
