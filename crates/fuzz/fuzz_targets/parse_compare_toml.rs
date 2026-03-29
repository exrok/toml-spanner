#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};

fuzz_target!(|data: &[u8]| -> Corpus {
    let Ok(text) = std::str::from_utf8(data) else {
        return Corpus::Keep;
    };

    match fuzz::parse_compare::compare(text) {
        fuzz::parse_compare::Outcome::Skip => Corpus::Reject,
        fuzz::parse_compare::Outcome::Ok => Corpus::Keep,
    }
});
