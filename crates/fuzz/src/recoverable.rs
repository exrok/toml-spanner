use toml_spanner::{Arena, Item, Table, Value};

const MAX_RECOVER_ERRORS: usize = 25;

pub fn check_recoverable(text: &str) {
    let arena_r = Arena::new();
    let doc = toml_spanner::parse_recoverable(text, &arena_r);

    assert!(
        doc.errors().len() <= MAX_RECOVER_ERRORS,
        "too many errors: {} > {MAX_RECOVER_ERRORS}\ninput: {text:?}",
        doc.errors().len(),
    );

    check_tree_wellformed(doc.table(), text);

    let arena_p = Arena::new();
    let Ok(ref_doc) = toml_spanner::parse(text, &arena_p) else {
        return;
    };

    assert!(
        doc.errors().is_empty(),
        "parse succeeded but parse_recoverable reported {} error(s)\n\
         input: {text:?}\n\
         errors: {:?}",
        doc.errors().len(),
        doc.errors(),
    );

    assert!(
        items_deep_eq(doc.table().as_item(), ref_doc.table().as_item()),
        "parse_recoverable tree differs from parse tree\n\
         input: {text:?}\n\
         recoverable: {:?}\n\
         parse: {:?}",
        doc.table(),
        ref_doc.table(),
    );
}

fn check_tree_wellformed(table: &Table<'_>, input: &str) {
    for (_key, item) in table {
        check_item_wellformed(item, input);
    }
}

fn check_item_wellformed(item: &Item<'_>, input: &str) {
    let kind = item.kind() as u8;
    assert!(
        kind <= 6,
        "invalid kind discriminant {kind}\ninput: {input:?}",
    );

    match item.value() {
        Value::String(s) => {
            assert!(
                std::str::from_utf8(s.as_bytes()).is_ok(),
                "string value is not valid UTF-8\ninput: {input:?}",
            );
        }
        Value::Integer(_) | Value::Float(_) | Value::Boolean(_) | Value::DateTime(_) => {}
        Value::Table(t) => check_tree_wellformed(t, input),
        Value::Array(a) => {
            for elem in a.iter() {
                check_item_wellformed(elem, input);
            }
        }
    }
}

fn items_deep_eq(a: &Item<'_>, b: &Item<'_>) -> bool {
    if a.kind() as u8 != b.kind() as u8 {
        return false;
    }
    match (a.value(), b.value()) {
        (Value::String(sa), Value::String(sb)) => sa == sb,
        (Value::Integer(ia), Value::Integer(ib)) => ia == ib,
        (Value::Float(fa), Value::Float(fb)) => fa.to_bits() == fb.to_bits(),
        (Value::Boolean(ba), Value::Boolean(bb)) => ba == bb,
        (Value::DateTime(da), Value::DateTime(db)) => da == db,
        (Value::Table(ta), Value::Table(tb)) => {
            if ta.len() != tb.len() {
                return false;
            }
            for ((ka, va), (kb, vb)) in ta.into_iter().zip(tb.into_iter()) {
                if ka.name != kb.name || !items_deep_eq(va, vb) {
                    return false;
                }
            }
            true
        }
        (Value::Array(aa), Value::Array(ab)) => {
            if aa.len() != ab.len() {
                return false;
            }
            for (ea, eb) in aa.iter().zip(ab.iter()) {
                if !items_deep_eq(ea, eb) {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

pub fn run_cli(path: &str) {
    let data = std::fs::read(path).expect("failed to read artifact");

    println!("artifact: {path}");
    println!("bytes ({len}): {data:?}", len = data.len());
    println!();

    let text = match std::str::from_utf8(&data) {
        Ok(s) => s,
        Err(e) => {
            println!("artifact is not valid UTF-8: {e}");
            println!("fuzzer would reject this input");
            return;
        }
    };

    println!("── input ({} bytes) ──\n{text:?}\n", text.len());

    let arena_r = Arena::new();
    let doc = toml_spanner::parse_recoverable(text, &arena_r);

    println!("── errors ({}) ──", doc.errors().len());
    for (i, e) in doc.errors().iter().enumerate() {
        println!("  [{i}] {e}");
    }
    println!();

    crate::gen_tree::print_table(doc.table(), "recovered tree");
    println!();

    if doc.errors().len() > MAX_RECOVER_ERRORS {
        eprintln!(
            "FAILURE: error count {} exceeds limit {MAX_RECOVER_ERRORS}",
            doc.errors().len(),
        );
        std::process::exit(1);
    }

    println!("── tree well-formedness ──");
    check_tree_wellformed(doc.table(), text);
    println!("OK\n");

    let arena_p = Arena::new();
    match toml_spanner::parse(text, &arena_p) {
        Ok(ref_doc) => {
            crate::gen_tree::print_table(ref_doc.table(), "reference parse tree");
            println!();

            if !doc.errors().is_empty() {
                eprintln!(
                    "FAILURE: parse succeeded but parse_recoverable reported {} error(s)",
                    doc.errors().len(),
                );
                for (i, e) in doc.errors().iter().enumerate() {
                    eprintln!("  [{i}] {e}");
                }
                std::process::exit(1);
            }
            println!("── zero errors on valid input: OK ──");

            if items_deep_eq(doc.table().as_item(), ref_doc.table().as_item()) {
                println!("── tree equivalence: OK ──");
            } else {
                eprintln!(
                    "FAILURE: trees differ!\n\
                     recoverable: {:?}\n\
                     parse: {:?}",
                    doc.table(),
                    ref_doc.table(),
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            println!("── parse returned error (expected): {e} ──");
            println!("parse_recoverable produced partial tree with {} error(s)", doc.errors().len());
            println!("── OK (invalid input) ──");
        }
    }
}
