use crate::emit::emit;
use crate::item::{Item, Value};
use crate::{Arena, Array, ArrayStyle, Key, Table, TableStyle, parse};

use crate::emit::test_data::{parse_test_cases, run_cases};

macro_rules! item {
    ($s:literal in $arena: ident) => { Item::from($s) };
    ($($kind:ident @)? { $( $key:ident: $p1:tt $(@ $p2:tt)? ),* $(,)? } in $arena: ident) => {{
        #[allow(unused_mut)]
        let mut t = Table::default();
        $( t.insert(Key::anon(stringify!($key)), item!($p1 $(@ $p2)? in $arena), &$arena); )*
        $(t.set_style(TableStyle::$kind);)?
        t.into_item()
    }};
    ($($kind:ident @)? [ $( $p1:tt $(@ $p2:tt)? ),* $(,)? ] in $arena: ident) => {{
        #[allow(unused_mut)]
        let mut arr = Array::default();
        $( arr.push(item!($p1 $(@ $p2)? in $arena), &$arena); )*
        $(arr.set_style(ArrayStyle::$kind);)?
        arr.into_item()
    }};
}

macro_rules! table {
    (in $arena:ident; $( $key:ident:  $p1:tt $(@ $p2:tt)?),+ $(,)? ) => {{
        let mut t = Table::default();
        $( t.insert(Key::anon(stringify!($key)), item!($p1 $(@ $p2)? in $arena), &$arena); )+
        t
    }};
}

#[track_caller]
fn run_emit(input: &str) -> String {
    let arena = Arena::new();
    let doc = parse(input, &arena).unwrap();

    let normalized = doc
        .table()
        .try_as_normalized()
        .expect("parsed table should be valid for emission");
    let mut output = Vec::new();
    emit(normalized, &mut output);
    let output_str = String::from_utf8(output).unwrap();

    // Round-trip: parse the emitted output and verify structural equivalence
    let root2 = parse(&output_str, &arena).unwrap_or_else(|e| {
        panic!(
            "emitted output failed to parse!\ninput:\n{input}\nemitted:\n{output_str}\nerror: {e:?}"
        );
    });

    assert_items_equal_with_flags(
        doc.table().as_item(),
        root2.table().as_item(),
        &mut Vec::new(),
        input,
        &output_str,
    );

    // Idempotency: emitting the re-parsed output should produce identical bytes
    let normalized2 = root2
        .table()
        .try_as_normalized()
        .expect("round-tripped table should be valid for emission");
    let mut output2 = Vec::new();
    emit(normalized2, &mut output2);
    let output_str2 = String::from_utf8(output2).unwrap();
    assert_eq!(
        output_str, output_str2,
        "emit is not idempotent!\ninput:\n{input}\nfirst emit:\n{output_str}\nsecond emit:\n{output_str2}"
    );

    output_str
}

#[track_caller]
fn assert_items_equal_with_flags(
    a: &Item<'_>,
    b: &Item<'_>,
    path: &mut Vec<String>,
    input: &str,
    emitted: &str,
) {
    let path_str = if path.is_empty() {
        "<root>".to_string()
    } else {
        path.join(".")
    };

    assert_eq!(
        a.kind() as u8,
        b.kind() as u8,
        "kind mismatch at {path_str}\ninput:\n{input}\nemitted:\n{emitted}"
    );

    assert_eq!(
        a.flag(),
        b.flag(),
        "flag mismatch at {path_str}: original={} emitted={}\ninput:\n{input}\nemitted:\n{emitted}",
        flag_name(a.flag()),
        flag_name(b.flag()),
    );

    match a.value() {
        Value::String(s) => assert_eq!(
            b.as_str(),
            Some(*s),
            "string mismatch at {path_str}\ninput:\n{input}\nemitted:\n{emitted}"
        ),
        Value::Integer(i) => assert_eq!(
            b.as_i64(),
            Some(*i),
            "integer mismatch at {path_str}\ninput:\n{input}\nemitted:\n{emitted}"
        ),
        Value::Float(f) => {
            let bf = b.as_f64().unwrap();
            assert_eq!(
                f.to_bits(),
                bf.to_bits(),
                "float mismatch at {path_str}\ninput:\n{input}\nemitted:\n{emitted}"
            );
        }
        Value::Boolean(v) => assert_eq!(
            b.as_bool(),
            Some(*v),
            "boolean mismatch at {path_str}\ninput:\n{input}\nemitted:\n{emitted}"
        ),
        Value::DateTime(dt_a) => {
            let dt_b = b.as_datetime().unwrap();
            assert_eq!(
                dt_a, dt_b,
                "datetime mismatch at {path_str}\ninput:\n{input}\nemitted:\n{emitted}"
            );
        }
        Value::Array(arr_a) => {
            let arr_b = b.as_array().unwrap();
            assert_eq!(
                arr_a.len(),
                arr_b.len(),
                "array length mismatch at {path_str}\ninput:\n{input}\nemitted:\n{emitted}"
            );
            for i in 0..arr_a.len() {
                path.push(format!("[{i}]"));
                assert_items_equal_with_flags(
                    &arr_a.as_slice()[i],
                    &arr_b.as_slice()[i],
                    path,
                    input,
                    emitted,
                );
                path.pop();
            }
        }
        Value::Table(tab_a) => {
            let tab_b = b.as_table().unwrap();
            assert_eq!(
                tab_a.len(),
                tab_b.len(),
                "table length mismatch at {path_str}: original keys={:?} emitted keys={:?}\ninput:\n{input}\nemitted:\n{emitted}",
                tab_a
                    .entries()
                    .iter()
                    .map(|(k, _)| k.name)
                    .collect::<Vec<_>>(),
                tab_b
                    .entries()
                    .iter()
                    .map(|(k, _)| k.name)
                    .collect::<Vec<_>>(),
            );
            for (key, val_a) in tab_a {
                path.push(key.name.to_string());
                let val_b = tab_b.get(key.name).unwrap_or_else(|| {
                    panic!("key {path_str}.{} missing in emitted output\ninput:\n{input}\nemitted:\n{emitted}", key.name);
                });
                assert_items_equal_with_flags(val_a, val_b, path, input, emitted);
                path.pop();
            }
        }
    }
}

fn flag_name(flag: u32) -> &'static str {
    match flag {
        0 => "NONE",
        2 => "ARRAY",
        3 => "AOT",
        4 => "IMPLICIT",
        5 => "DOTTED",
        6 => "HEADER",
        7 => "FROZEN",
        _ => "UNKNOWN",
    }
}

#[test]
fn data_emit_roundtrip() {
    let cases = parse_test_cases(include_str!("emit/testdata/emit_roundtrip.toml"));
    run_cases(&cases, |case| {
        let result = run_emit(case.source());
        if let Some(expected) = case.expected {
            let expected = format!("{expected}\n");
            assert_eq!(result, expected, "case {:?}: emit mismatch", case.name);
        }
    });
}

#[test]
fn datetime() {
    let result = run_emit("dt = 2024-01-15T10:30:00Z");
    assert!(result.starts_with("dt = 2024-01-15"));
}

#[test]
fn constructed_implicit_rejects() {
    // into_item() produces FLAG_TABLE (implicit), so scalars inside
    // would be silently lost — try_as_normalized must reject this.
    let arena = Arena::new();
    let table = table! { in arena; a: { x: 0, y: 1 }, b: "y" };
    assert!(table.try_as_normalized().is_none());
}

#[test]
fn normalize_implicit_promoted_to_header() {
    // Explicit Implicit@ produces FLAG_TABLE (implicit) without auto-style,
    // so scalars inside would be lost without normalize promoting to header.
    let arena = Arena::new();
    let mut table = table! { in arena; a: Implicit @ { x: 1, y: 2 }, b: "hi" };
    let s = emit_normalized(&mut table);
    assert!(s.contains("[a]"), "expected header section: {s}");
    assert!(s.contains("x = 1"), "expected x: {s}");
    assert!(s.contains("b = \"hi\""), "expected b: {s}");
}

// Removed: normalize_scalar_with_table_flag
// Setting a table flag on a scalar violates the tag/flag invariant and
// causes UB (as_table reinterprets scalar memory as a table).

#[test]
fn normalize_aot_with_non_table_elements() {
    use crate::item::{FLAG_AOT, FLAG_ARRAY};

    // Manually create an AOT-flagged array with non-table elements.
    let arena = Arena::new();
    let mut table = table! { in arena; arr: [1, 2] };
    table.get_mut("arr").unwrap().set_flag(FLAG_AOT);

    let normalized = table.normalize();

    // Should have been downgraded to inline array.
    let arr_item = normalized.get("arr").unwrap();
    assert_eq!(arr_item.flag(), FLAG_ARRAY);

    let s = emit_table(normalized);
    assert_eq!(s, "arr = [1, 2]\n");
}

#[test]
fn normalize_nested_implicit_chain() {
    // root -> a (implicit) -> b (implicit) -> val = 1
    // b must be promoted to header; a can stay implicit or be promoted
    let arena = Arena::new();
    let mut root = table! { in arena; a: { b: { val: 1 } } };
    let s = emit_normalized(&mut root);
    assert!(s.contains("val = 1"), "expected val: {s}");
}

#[test]
fn normalize_frozen_children_fixed() {
    use crate::item::FLAG_FROZEN;

    // HEADER flag inside frozen context must be fixed to FROZEN.
    let arena = Arena::new();
    let mut root = table! { in arena; t: Inline @ { sub: Header @ { x: 1 } } };

    let normalized = root.normalize();

    // The inner table should now be FROZEN, not HEADER.
    let t = normalized.get("t").unwrap();
    let sub = t.as_table().unwrap().get("sub").unwrap();
    assert_eq!(sub.flag(), FLAG_FROZEN);

    let s = emit_table(normalized);
    assert_eq!(s, "t = { sub = { x = 1 } }\n");
}

#[test]
fn normalize_dotted_in_implicit_promoted() {
    // root -> a (implicit) -> b (dotted) -> c = 1
    // Either a or a.b must become a header so c is reachable.
    let arena = Arena::new();
    let mut root = table! { in arena; a: { b: Dotted @ { c: 1 } } };
    let s = emit_normalized(&mut root);
    assert!(s.contains("c = 1"), "expected c: {s}");
}

#[test]
fn normalize_valid_constructed_tree_unchanged() {
    let arena = Arena::new();
    let mut root = table! {
        in arena;
        name: "test",
        server: Header @ { host: "localhost", port: 8080 }
    };

    fn collect_flags_deep(table: &Table<'_>, out: &mut Vec<(String, u32)>, prefix: &str) {
        for (k, v) in table {
            let path = if prefix.is_empty() {
                k.name.to_string()
            } else {
                format!("{prefix}.{}", k.name)
            };
            out.push((path.clone(), v.flag()));
            if let Some(sub) = v.as_table() {
                collect_flags_deep(sub, out, &path);
            }
        }
    }

    let mut before = Vec::new();
    collect_flags_deep(&root, &mut before, "");

    let normalized = root.normalize();

    let mut after = Vec::new();
    collect_flags_deep(normalized.table(), &mut after, "");

    assert_eq!(before, after);

    let s = emit_table(normalized);
    assert!(s.contains("name = \"test\""), "{s}");
    assert!(s.contains("[server]"), "{s}");
}

/// Normalize a table, then verify emit → parse roundtrip and idempotency.
#[track_caller]
fn check_normalize(root: &mut Table<'_>) {
    let normalized = root.normalize();

    let mut buf1 = Vec::new();
    emit(normalized, &mut buf1);
    let emitted = String::from_utf8(buf1.clone()).expect("emit must produce valid UTF-8");

    let arena = Arena::new();
    let root2 = parse(&emitted, &arena)
        .unwrap_or_else(|e| panic!("parse failed!\nemitted:\n{emitted}\nerror: {e:?}"));

    assert_items_equal_with_flags(
        normalized.table().as_item(),
        root2.table().as_item(),
        &mut Vec::new(),
        "(constructed tree)",
        &emitted,
    );

    let normalized2 = root2
        .table()
        .try_as_normalized()
        .expect("round-tripped table should be valid for emission");
    let mut buf2 = Vec::new();
    emit(normalized2, &mut buf2);
    assert_eq!(
        buf1,
        buf2,
        "emit not idempotent!\nfirst:\n{emitted}\nsecond:\n{}",
        String::from_utf8_lossy(&buf2),
    );
}

#[track_caller]
fn emit_table(table: &crate::emit::NormalizedTable<'_>) -> String {
    let mut buf = Vec::new();
    emit(table, &mut buf);
    String::from_utf8(buf).unwrap()
}

#[track_caller]
fn emit_normalized(table: &mut Table<'_>) -> String {
    emit_table(table.normalize())
}

#[test]
fn normalize_reg_empty_table_promoted_to_header() {
    // Empty implicit table must become HEADER so it
    // survives emit→parse roundtrip as `[x]`.
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        x: Implicit @ {},
        a: ""
    });
}

#[test]
fn normalize_reg_nested_array_flags() {
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        d: [[], ""]
    });
}

#[test]
fn normalize_reg_table_in_inline_array_must_be_frozen() {
    // Table element of an inline array must be FROZEN, not DOTTED.
    // DOTTED has no meaning for a positional array element.
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        z: [Dotted @ {}]
    });
}

#[test]
fn normalize_reg_empty_dotted_in_frozen_context() {
    // An empty DOTTED table inside a frozen/inline table emits nothing
    // via the dotted-key path. Must be promoted to FROZEN → `y = {}`.
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        a: [Inline @ { y: Dotted @ {}, x: 1 }]
    });
}

#[test]
fn normalize_reg_dotted_children_all_promoted() {
    // A DOTTED table whose only child is an empty DOTTED table (promoted
    // to HEADER) ends up with no body items. Must become IMPLICIT to
    // match what the parser produces.
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        d: Dotted @ { d: Dotted @ {} },
        x: ""
    });
}

#[test]
fn normalize_reg_aot_elements_are_header() {
    // Each AOT element emits as `[[section]]` and parses back as HEADER.
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        d: Header @ [Implicit @ {}]
    });
}

#[test]
fn normalize_reg_empty_aot_in_implicit_table() {
    // Empty AOT gets downgraded to inline array (body item), so the
    // parent implicit table must be promoted to HEADER.
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        d: Implicit @ { a: Header @ [] }
    });
}

#[test]
fn normalize_reg_promoted_header_before_dotted_sibling() {
    // An empty DOTTED table (promoted to HEADER) that precedes a
    // DOTTED sibling with body items causes non-idempotent emit:
    // the HEADER entry emits as a subsection after all body items,
    // so re-parse moves it to the end of the parent's entry list.
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        c: {
            x: Dotted @ {},
            y: Dotted @ { a: Dotted @ {}, v: "" }
        }
    });
}

#[test]
fn normalize_dotted_with_header_child_demoted() {
    // A DOTTED table with a HEADER child: the child must be demoted
    // to DOTTED so the parent can still emit via dotted-key syntax.
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        d: Dotted @ { sub: Header @ { val: 1 }, name: "x" },
        top: ""
    });
}

#[test]
fn normalize_dotted_with_implicit_child_demoted() {
    // A DOTTED table with an implicit child: child is demoted to
    // DOTTED, and if it has body items they remain reachable.
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        d: Dotted @ { sub: Implicit @ { val: 1, inner: "y" }, name: "x" },
        top: ""
    });
}

#[test]
fn normalize_dotted_with_aot_child_demoted() {
    // A DOTTED table with an AOT child: the AOT must be demoted to
    // inline array since dotted tables can't contain header sections.
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        d: Dotted @ {
            arr: Header @ [{ a: 1 }, { a: 2 }],
            x: ""
        },
        top: ""
    });
}

#[test]
fn normalize_dotted_all_children_promoted_no_body() {
    // A DOTTED table whose ALL children are empty tables (promoted to
    // headers) — no body items remain, so the dotted table must become
    // IMPLICIT.
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        d: Dotted @ { a: Header @ {}, b: Implicit @ {} },
        x: ""
    });
}

// A DOTTED table with effective_body=true whose children are all headers
// (no body items remain after normalization). The demotion block must
// also handle scalars and inline arrays that are already body-level.
#[test]
fn normalize_dotted_body_with_scalar_and_inline_children() {
    let arena = Arena::new();
    // Dotted table inside body context, with a scalar child (already body),
    // an inline array child (already body), and a Dotted child (already body).
    // This exercises the "else" branches (lines 234, 243, 247) in demotion.
    check_normalize(&mut table! {
        in arena;
        parent: Header @ {
            d: Dotted @ {
                scalar_val: "hello",
                arr_val: [1, 2],
                sub: Dotted @ { x: 1 }
            }
        }
    });
}

// DOTTED table at root level (effective_body=false) where all children
// are subsections: exercises the `!effective_body` → true path (line 216)
// and subsequent demotion to IMPLICIT when no body items are produced.
#[test]
fn normalize_dotted_at_root_all_subsections() {
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        d: Dotted @ { a: Header @ { v: 1 }, b: Implicit @ { w: 2 } },
        x: ""
    });
}

// Empty AOT gets downgraded and normalized correctly via check_normalize
#[test]
fn normalize_reg_empty_aot_downgraded() {
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        arr: Header @ []
    });
}

#[test]
fn emit_string_escape_sequences() {
    // Exercises format_string escape paths: backslash, newline, tab, etc.
    let arena = Arena::new();
    let mut t = Table::default();
    t.insert(
        Key::anon("esc"),
        Item::string("line1\nline2\ttab\\back\"quote"),
        &arena,
    );
    t.insert(
        Key::anon("ctrl"),
        Item::string("\r\u{0008}\u{000C}"),
        &arena,
    );
    t.insert(Key::anon("low"), Item::string("\x01\x1F"), &arena);
    let s = emit_normalized(&mut t);
    assert!(s.contains(r#"\n"#), "newline escape: {s}");
    assert!(s.contains(r#"\t"#), "tab escape: {s}");
    assert!(s.contains(r#"\\"#), "backslash escape: {s}");
    assert!(s.contains(r#"\""#), "quote escape: {s}");
    assert!(s.contains(r#"\r"#), "carriage return escape: {s}");
    assert!(s.contains(r#"\b"#), "backspace escape: {s}");
    assert!(s.contains(r#"\f"#), "formfeed escape: {s}");
    assert!(s.contains(r#"\u"#), "unicode escape: {s}");
}

#[test]
fn emit_special_floats() {
    // Exercises format_float NaN and infinity paths
    let arena = Arena::new();
    let mut t = Table::default();
    t.insert(Key::anon("pos_nan"), Item::from(f64::NAN), &arena);
    t.insert(Key::anon("neg_nan"), Item::from(-f64::NAN), &arena);
    t.insert(Key::anon("pos_inf"), Item::from(f64::INFINITY), &arena);
    t.insert(Key::anon("neg_inf"), Item::from(f64::NEG_INFINITY), &arena);
    let s = emit_normalized(&mut t);
    assert!(s.contains("nan"), "NaN: {s}");
    assert!(s.contains("inf"), "infinity: {s}");
    assert!(s.contains("-inf"), "neg infinity: {s}");
}

#[test]
fn emit_nested_inline_dotted() {
    // Exercises format_inline_dotted_kv recursive path
    let arena = Arena::new();
    let mut t = Table::default();
    let mut inner = Table::default();
    inner.set_style(TableStyle::Dotted);
    let mut deep = Table::default();
    deep.set_style(TableStyle::Dotted);
    deep.insert(Key::anon("val"), Item::from(42i64), &arena);
    inner.insert(Key::anon("deep"), deep.into_item(), &arena);
    inner.insert(Key::anon("x"), Item::from(1i64), &arena);
    t.insert(Key::anon("outer"), inner.into_item(), &arena);
    t.set_style(TableStyle::Inline);
    let mut root = Table::default();
    root.insert(Key::anon("t"), t.into_item(), &arena);
    let s = emit_normalized(&mut root);
    assert!(s.contains("42"), "deep value: {s}");
}

#[test]
fn auto_style_table_becomes_inline() {
    let arena = Arena::new();
    let mut root = table! { in arena; a: { x: 1, y: 2 }, b: "hi" };
    let s = emit_normalized(&mut root);
    assert!(s.contains("a = { "), "expected inline table: {s}");
    assert!(s.contains("b = \"hi\""), "expected b: {s}");
}

#[test]
fn auto_style_table_too_large_stays_header() {
    let arena = Arena::new();
    let mut root = table! { in arena; a: { x: 1, y: 2, z: 3 }, b: "hi" };
    let s = emit_normalized(&mut root);
    assert!(s.contains("[a]"), "expected header section: {s}");
}

#[test]
fn auto_style_table_with_non_small_value() {
    let arena = Arena::new();
    let mut root = table! { in arena; a: { x: 1, nested: { v: 1 } }, b: "hi" };
    let s = emit_normalized(&mut root);
    assert!(s.contains("[a]"), "expected header section: {s}");
}

#[test]
fn auto_style_array_becomes_inline() {
    let arena = Arena::new();
    let mut root = table! { in arena; arr: [1, 2] };
    let s = emit_normalized(&mut root);
    assert_eq!(s, "arr = [1, 2]\n");
}

#[test]
fn auto_style_array_of_tables_becomes_aot() {
    let arena = Arena::new();
    let mut root = table! { in arena; servers: [{ host: "a" }, { host: "b" }] };
    let s = emit_normalized(&mut root);
    assert!(s.contains("[[servers]]"), "expected AOT: {s}");
    assert!(s.contains("host = \"a\""), "expected first host: {s}");
    assert!(s.contains("host = \"b\""), "expected second host: {s}");
}

#[test]
fn auto_style_array_of_tables_roundtrip() {
    let arena = Arena::new();
    check_normalize(&mut table! {
        in arena;
        items: [{ name: "x", val: 1 }, { name: "y", val: 2 }]
    });
}

#[test]
fn auto_style_cleared_by_set_style() {
    let mut t = Table::new();
    assert!(t.is_auto_style());
    t.set_style(TableStyle::Header);
    assert!(!t.is_auto_style());

    let mut a = Array::new();
    assert!(a.is_auto_style());
    a.set_style(ArrayStyle::Inline);
    assert!(!a.is_auto_style());
}

#[test]
fn auto_style_string_boundary() {
    let arena = Arena::new();
    let short = "a".repeat(30);
    let long = "a".repeat(31);

    let mut inner_short = Table::default();
    inner_short.insert(Key::anon("s"), Item::string(&short), &arena);
    let mut root_short = Table::default();
    root_short.insert(Key::anon("t"), inner_short.into_item(), &arena);
    let s = emit_normalized(&mut root_short);
    assert!(
        s.contains("t = { "),
        "30-char string should auto-inline: {s}"
    );

    let mut inner_long = Table::default();
    inner_long.insert(Key::anon("s"), Item::string(&long), &arena);
    let mut root_long = Table::default();
    root_long.insert(Key::anon("t"), inner_long.into_item(), &arena);
    let s = emit_normalized(&mut root_long);
    assert!(
        s.contains("[t]"),
        "31-char string should not auto-inline: {s}"
    );
}

#[test]
fn auto_style_string_with_control_chars() {
    let arena = Arena::new();
    let mut root = Table::default();
    root.insert(Key::anon("t"), item!({ s: "hi\nthere" } in arena), &arena);
    let s = emit_normalized(&mut root);
    assert!(
        s.contains("[t]"),
        "string with control char should not auto-inline: {s}"
    );
}

#[test]
fn auto_style_empty_containers_are_small() {
    let arena = Arena::new();
    let mut root = table! { in arena; t: { a: [], b: {} } };
    let s = emit_normalized(&mut root);
    assert!(
        s.contains("t = { "),
        "empty containers should be small: {s}"
    );
}

#[test]
fn auto_style_array_mixed_types_downgraded_from_header() {
    // resolve_auto_array sets Header for arrays that don't meet inline
    // small-value criteria. normalize_array must downgrade to Inline when
    // elements are not all tables.
    let arena = Arena::new();
    let mut root = table! { in arena; arr: [1, 2, 3] };
    let s = emit_normalized(&mut root);
    assert_eq!(s, "arr = [1, 2, 3]\n");

    let mut root2 = table! { in arena; arr: [{ x: 1 }, "not a table"] };
    let s2 = emit_normalized(&mut root2);
    assert!(
        s2.contains("arr = ["),
        "mixed array must be inline, not AOT: {s2}"
    );
    assert!(!s2.contains("[["), "must not contain AOT header: {s2}");
}
