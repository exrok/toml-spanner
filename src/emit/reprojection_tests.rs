use super::reproject;
use crate::Table;
use crate::arena::Arena;
use crate::emit::EmitConfig;
use crate::item::{ArrayStyle, Item, Key, TableStyle, Value};
use crate::parser::parse;
use crate::{Document, emit, emit_with_config};

use crate::emit::test_data::{parse_test_cases, run_cases};

#[test]
fn data_reproject_recovers() {
    let cases = parse_test_cases(include_str!("testdata/reproject_recovers.toml"));
    run_cases(&cases, |case| {
        assert_reproject_recovers(case.source());
    });
}

#[test]
fn data_emit_identity() {
    let cases = parse_test_cases(include_str!("testdata/emit_identity.toml"));
    run_cases(&cases, |case| {
        let input = case.source();
        let result = emit_with_projection(input);
        let expected = case.expected.unwrap_or(input);
        if result != expected {
            let arena = Arena::new();
            let src_doc = parse(input, &arena).unwrap();
            let mut dest_doc = parse(input, &arena).unwrap();
            let mut items = Vec::new();
            reproject(&src_doc, &mut dest_doc.table, &mut items);
            panic!(
                "case {:?}: emit_with_projection mismatch\
                 \n── input ({} bytes) ──\n{input:?}\
                 \n── parsed source tree ──\n{}\
                 \n── dest tree (after reproject) ──\n{}\
                 \n── reprojected ({} items) ──\
                 \n── expected ──\n{expected:?}\
                 \n── actual ──\n{result:?}",
                case.name,
                input.len(),
                debug_table(src_doc.table()),
                debug_table(&dest_doc.table),
                items.len(),
            );
        }
    });
}

#[test]
fn data_reproject_edit() {
    let cases = parse_test_cases(include_str!("testdata/reproject_edit.toml"));
    run_cases(&cases, |case| {
        assert_reproject_edit(case.source(), case.dest());
    });
}

#[test]
fn data_edit_ordered_1() {
    let cases = parse_test_cases(include_str!("testdata/edit_ordered_1.toml"));
    run_cases(&cases, |case| {
        run_edit_ordered(case.source(), case.dest());
    });
}

#[test]
fn data_edit_ordered_2() {
    let cases = parse_test_cases(include_str!("testdata/edit_ordered_2.toml"));
    run_cases(&cases, |case| {
        run_edit_ordered(case.source(), case.dest());
    });
}

#[test]
fn data_edit_ordered_3() {
    let cases = parse_test_cases(include_str!("testdata/edit_ordered_3.toml"));
    run_cases(&cases, |case| {
        run_edit_ordered(case.source(), case.dest());
    });
}

#[test]
fn data_reproject_exact() {
    let cases = parse_test_cases(include_str!("testdata/reproject_exact.toml"));
    run_cases(&cases, |case| {
        let source = case.source();
        let modified = case.modified.unwrap_or(source);
        let expected = case.expected.unwrap_or(modified);
        assert_reproject_exact(source, modified, expected);
    });
}

#[test]
fn data_reorder_identity() {
    let cases = parse_test_cases(include_str!("testdata/reorder_identity.toml"));
    run_cases(&cases, |case| {
        let input = case.source();
        if let Some(dest) = case.dest {
            let result = reproject_edit_reorder(input, dest);
            let expected = case.expected();
            if result != expected {
                let arena = Arena::new();
                let src_doc = parse(input, &arena).unwrap();
                let mut dest_doc = parse(dest, &arena).unwrap();
                let mut items = Vec::new();
                reproject(&src_doc, &mut dest_doc.table, &mut items);
                panic!(
                    "case {:?}: reorder mismatch\
                     \n── source ({} bytes) ──\n{input:?}\
                     \n── dest ({} bytes) ──\n{dest:?}\
                     \n── parsed source tree ──\n{}\
                     \n── dest tree (after reproject) ──\n{}\
                     \n── reprojected ({} items) ──\
                     \n── expected ──\n{expected:?}\
                     \n── actual ──\n{result:?}",
                    case.name,
                    input.len(),
                    dest.len(),
                    debug_table(src_doc.table()),
                    debug_table(&dest_doc.table),
                    items.len(),
                );
            }
        } else {
            let result = emit_with_reorder(input);
            let expected = case.expected();
            if result != expected {
                let arena = Arena::new();
                let src_doc = parse(input, &arena).unwrap();
                let mut dest_doc = parse(input, &arena).unwrap();
                let mut items = Vec::new();
                reproject(&src_doc, &mut dest_doc.table, &mut items);
                panic!(
                    "case {:?}: reorder mismatch\
                     \n── input ({} bytes) ──\n{input:?}\
                     \n── parsed source tree ──\n{}\
                     \n── dest tree (after reproject) ──\n{}\
                     \n── reprojected ({} items) ──\
                     \n── expected ──\n{expected:?}\
                     \n── actual ──\n{result:?}",
                    case.name,
                    input.len(),
                    debug_table(src_doc.table()),
                    debug_table(&dest_doc.table),
                    items.len(),
                );
            }
        }
    });
}

/// Recursively sets all table kinds to Implicit and all array kinds to Inline,
/// destroying structural information.
fn erase_kinds(table: &mut Table<'_>) {
    for (_, item) in table {
        if let Some(t) = item.as_table_mut() {
            match t.style() {
                // Dotted and Inline encode user intent (dotted keys vs inline
                // tables); preserve them so normalization doesn't lose the
                // distinction.
                TableStyle::Dotted | TableStyle::Inline => {}
                _ => t.set_style(TableStyle::Implicit),
            }
            erase_kinds(t);
        } else if let Some(a) = item.as_array_mut() {
            a.set_style(ArrayStyle::Inline);
            for elem in a.as_mut_slice() {
                if let Some(t) = elem.as_table_mut() {
                    match t.style() {
                        TableStyle::Dotted | TableStyle::Inline => {}
                        _ => t.set_style(TableStyle::Implicit),
                    }
                    erase_kinds(t);
                }
            }
        }
    }
}

fn emit_table(table: &mut Table<'_>) -> String {
    let norm = table.normalize();
    let mut buf = Vec::new();
    emit::emit(norm, &mut buf);
    String::from_utf8(buf).unwrap()
}

/// Parse → emit gives a reference.
/// Parse → erase kinds → reproject from original → normalize → emit must match.
fn assert_reproject_recovers(input: &str) {
    let arena = Arena::new();
    let mut ref_root = parse(input, &arena).unwrap();
    let reference = emit_table(&mut ref_root.table);

    let src_doc = parse(input, &arena).unwrap();

    let mut dest_doc = parse(input, &arena).unwrap();
    erase_kinds(&mut dest_doc.table);

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    let reprojected = emit_table(&mut dest_doc.table);
    if reprojected != reference {
        panic!(
            "reprojected output should match reference\
             \n── input ({} bytes) ──\n{input:?}\
             \n── parsed source tree ──\n{}\
             \n── dest tree (after erase + reproject) ──\n{}\
             \n── reprojected ({} items) ──\
             \n── reference emit ──\n{reference:?}\
             \n── reprojected emit ──\n{reprojected:?}",
            input.len(),
            debug_table(src_doc.table()),
            debug_table(&dest_doc.table),
            items.len(),
        );
    }
}

// recovers tests: moved to testdata/reproject_recovers.toml

/// Builds a nested table `outer_key = { inner_key = value }` with Implicit
/// kind (reprojection assigns the correct kind).
fn make_nested<'de>(
    outer: &'de str,
    inner: &'de str,
    value: Item<'de>,
    arena: &'de Arena,
) -> (Key<'de>, Item<'de>) {
    let mut t = Table::default();
    t.insert(Key::anon(inner), value, arena);
    (Key::anon(outer), t.into_item())
}

/// Parses `input`, applies `mutate` to the dest document, reprojects from
/// the original, normalizes, and emits.
fn reproject_after_mutation(
    input: &str,
    mutate: impl for<'a> FnOnce(&mut Table<'a>, &'a Arena),
) -> String {
    let arena = Arena::new();
    let src_doc = parse(input, &arena).unwrap();

    let mut dest_doc = parse(input, &arena).unwrap();

    mutate(&mut dest_doc.table, &arena);

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    emit_table(&mut dest_doc.table)
}

#[test]
fn new_sibling_inherits_dotted_kind() {
    let result = reproject_after_mutation("[A]\nb.c = 1", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("d", "e", Item::from(2i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "[A]\nb.c = 1\nd.e = 2\n");
}

#[test]
fn new_sibling_inherits_inline_kind() {
    let result = reproject_after_mutation("[A]\nb = { c = 1 }", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("d", "e", Item::from(2i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "[A]\nb = { c = 1 }\nd = { e = 2 }\n");
}

#[test]
fn multiple_new_siblings_all_inherit_dotted() {
    let result = reproject_after_mutation("[A]\nb.c = 1", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("d", "e", Item::from(2i64), arena);
        section.insert(k, v, arena);
        let (k, v) = make_nested("f", "g", Item::from(3i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "[A]\nb.c = 1\nd.e = 2\nf.g = 3\n");
}

#[test]
fn multiple_new_siblings_all_inherit_inline() {
    let result = reproject_after_mutation("[A]\nb = { c = 1 }", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("d", "e", Item::from(2i64), arena);
        section.insert(k, v, arena);
        let (k, v) = make_nested("f", "g", Item::from(3i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "[A]\nb = { c = 1 }\nd = { e = 2 }\nf = { g = 3 }\n");
}

#[test]
fn new_sibling_after_multiple_dotted() {
    let result = reproject_after_mutation("[A]\nb.c = 1\nb.d = 2\nx.y = 3", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("z", "w", Item::from(99i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "[A]\nb.c = 1\nb.d = 2\nx.y = 3\nz.w = 99\n");
}

#[test]
fn new_sibling_inherits_last_match_dotted_after_inline() {
    // b is inline, x is dotted → new sibling after x inherits dotted
    let result = reproject_after_mutation("[A]\nb = { c = 1 }\nx.y = 3", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("z", "w", Item::from(4i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "[A]\nb = { c = 1 }\nx.y = 3\nz.w = 4\n");
}

#[test]
fn new_sibling_inherits_last_match_inline_after_dotted() {
    // b is dotted, x is inline → new sibling after x inherits inline
    let result = reproject_after_mutation("[A]\nb.c = 1\nx = { y = 3 }", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("z", "w", Item::from(4i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "[A]\nb.c = 1\nx = { y = 3 }\nz = { w = 4 }\n");
}

#[test]
fn new_sibling_before_match_backfills_dotted() {
    // Construct dest manually so new entries come before the matched entry.
    let arena = Arena::new();
    let src_doc = parse("[A]\nb.c = 1", &arena).unwrap();

    // Build dest: A = { d.e=2, b.c=1 } — d is new (before matched b)
    let mut section_a = Table::new();
    section_a.set_style(TableStyle::Header);
    let (k, v) = make_nested("d", "e", Item::from(2i64), &arena);
    section_a.insert(k, v, &arena);
    let (k, v) = make_nested("b", "c", Item::from(1i64), &arena);
    section_a.insert(k, v, &arena);
    let mut dest = Table::default();
    dest.insert(Key::anon("A"), section_a.into_item(), &arena);

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest, &mut items);

    let result = emit_table(&mut dest);
    // d should be backfilled with dotted kind from b (first match)
    assert_eq!(result, "[A]\nd.e = 2\nb.c = 1\n");
}

#[test]
fn new_sibling_before_match_backfills_inline() {
    let arena = Arena::new();
    let src_doc = parse("[A]\nb = { c = 1 }", &arena).unwrap();

    let mut section_a = Table::new();
    section_a.set_style(TableStyle::Header);
    let (k, v) = make_nested("d", "e", Item::from(2i64), &arena);
    section_a.insert(k, v, &arena);
    let (k, v) = make_nested("b", "c", Item::from(1i64), &arena);
    section_a.insert(k, v, &arena);
    let mut dest = Table::default();
    dest.insert(Key::anon("A"), section_a.into_item(), &arena);

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest, &mut items);

    let result = emit_table(&mut dest);
    assert_eq!(result, "[A]\nd = { e = 2 }\nb = { c = 1 }\n");
}

#[test]
fn new_scalar_alongside_dotted() {
    let result = reproject_after_mutation("[A]\nb.c = 1", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        section.insert(Key::anon("x"), Item::from(42i64), arena);
    });
    assert_eq!(result, "[A]\nb.c = 1\nx = 42\n");
}

#[test]
fn new_scalar_alongside_inline() {
    let result = reproject_after_mutation("[A]\nb = { c = 1 }", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        section.insert(Key::anon("x"), Item::from(42i64), arena);
    });
    assert_eq!(result, "[A]\nb = { c = 1 }\nx = 42\n");
}

#[test]
fn new_sibling_deep_dotted_nesting() {
    // [A]\nb.c.d = 1 → insert b.c.e = 2 (sibling inside the dotted chain)
    let result = reproject_after_mutation("[A]\nb.c.d = 1", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let bc = section.get_mut("b").unwrap().as_table_mut().unwrap();
        let c = bc.get_mut("c").unwrap().as_table_mut().unwrap();
        c.insert(Key::anon("e"), Item::from(2i64), arena);
    });
    assert_eq!(result, "[A]\nb.c.d = 1\nb.c.e = 2\n");
}

#[test]
fn new_sibling_deep_inline_nesting() {
    let result = reproject_after_mutation("[A]\nb = { c = { d = 1 } }", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let b = section.get_mut("b").unwrap().as_table_mut().unwrap();
        let c = b.get_mut("c").unwrap().as_table_mut().unwrap();
        c.insert(Key::anon("e"), Item::from(2i64), arena);
    });
    assert_eq!(result, "[A]\nb = { c = { d = 1, e = 2 } }\n");
}

#[test]
fn new_sibling_at_root_alongside_header() {
    let result = reproject_after_mutation("[A]\nx = 1\n\n[B]\ny = 2", |root, arena| {
        let mut section_c = Table::default();
        section_c.insert(Key::anon("z"), Item::from(3i64), arena);
        root.insert(Key::anon("C"), section_c.into_item(), arena);
    });
    // C has no match in src, but A and B are both headers → C inherits header
    assert!(
        result.contains("[C]"),
        "expected C as header section: {result}"
    );
    assert!(result.contains("z = 3"), "expected z: {result}");
}

#[test]
fn new_root_scalar_alongside_headers() {
    let result = reproject_after_mutation("top = 1\n\n[A]\nx = 2", |root, arena| {
        root.insert(Key::anon("extra"), Item::from(99i64), arena);
    });
    assert!(result.contains("top = 1"), "{result}");
    assert!(result.contains("extra = 99"), "{result}");
}

#[test]
fn new_dotted_sibling_with_header_subsection() {
    // [A]\nb.c = 1\n[A.b.d]\nval = 2 → insert e.f = 3 into A
    let result = reproject_after_mutation("[A]\nb.c = 1\n\n[A.b.d]\nval = 2", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("e", "f", Item::from(3i64), arena);
        section.insert(k, v, arena);
    });
    assert!(result.contains("b.c = 1"), "expected dotted b.c: {result}");
    assert!(result.contains("e.f = 3"), "expected dotted e.f: {result}");
    assert!(
        result.contains("[A.b.d]"),
        "expected header subsection: {result}"
    );
}

#[test]
fn new_sibling_alongside_aot() {
    let result = reproject_after_mutation(
        "[[servers]]\nname = \"alpha\"\n\n[[servers]]\nname = \"beta\"",
        |root, arena| {
            root.insert(Key::anon("version"), Item::from("1.0"), arena);
        },
    );
    assert!(result.contains("version = \"1.0\""), "{result}");
    assert!(result.contains("[[servers]]"), "{result}");
}

#[test]
fn new_array_sibling_inherits_aot_style() {
    // Source has [[servers]] (AOT). Mutation adds a new array `tasks` with
    // table entries. The new array should inherit AOT style from servers.
    let result = reproject_after_mutation(
        "[[servers]]\nname = \"alpha\"\n\n[[servers]]\nname = \"beta\"",
        |root, arena| {
            let mut arr = crate::Array::new();
            let mut t = Table::default();
            t.insert(Key::anon("name"), Item::from("build"), arena);
            arr.push(t.into_item(), arena);
            root.insert(Key::anon("tasks"), arr.into_item(), arena);
        },
    );
    assert!(result.contains("[[servers]]"), "{result}");
    assert!(
        result.contains("[[tasks]]"),
        "new array should inherit AOT style: {result}"
    );
    assert!(result.contains("name = \"build\""), "{result}");
}

#[test]
fn new_array_sibling_before_match_backfills_aot() {
    // Dest has new array `jobs` before matched `servers`. The backfill
    // should apply AOT style from the first matched array.
    let arena = Arena::new();
    let src_doc = parse(
        "[[servers]]\nname = \"a\"\n\n[[servers]]\nname = \"b\"",
        &arena,
    )
    .unwrap();

    let mut dest = Table::default();
    // Insert `jobs` first (new, before matched `servers`)
    let mut arr = crate::Array::new();
    let mut t = Table::default();
    t.insert(Key::anon("id"), Item::from(1i64), &arena);
    arr.push(t.into_item(), &arena);
    dest.insert(Key::anon("jobs"), arr.into_item(), &arena);
    // Insert `servers` (will match src)
    let mut arr2 = crate::Array::new();
    let mut t1 = Table::default();
    t1.insert(Key::anon("name"), Item::from("a"), &arena);
    arr2.push(t1.into_item(), &arena);
    let mut t2 = Table::default();
    t2.insert(Key::anon("name"), Item::from("b"), &arena);
    arr2.push(t2.into_item(), &arena);
    dest.insert(Key::anon("servers"), arr2.into_item(), &arena);

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest, &mut items);

    let result = emit_table(&mut dest);
    assert!(
        result.contains("[[jobs]]"),
        "new array before match should be backfilled to AOT: {result}"
    );
    assert!(result.contains("[[servers]]"), "{result}");
}

#[test]
fn constructed_new_sibling_dotted_via_macro() {
    // Source: A = Header { b = Dotted { c = 1 } }
    // Dest: A = Header { b = { c = 1 }, d = { e = 2 } }
    // After reproject: d should get Dotted from b
    let result = reproject_after_mutation("[A]\nb.c = 1", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("d", "e", Item::from(2i64), arena);
        section.insert(k, v, arena);
    });
    assert!(result.contains("b.c = 1"), "expected dotted b.c: {result}");
    assert!(result.contains("d.e = 2"), "expected dotted d.e: {result}");
}

#[test]
fn constructed_new_sibling_inline_via_macro() {
    // Source: A = Header { b = Inline { c = 1 } }
    // Dest: A = Header { b = { c = 1 }, d = { e = 2 } }
    // After reproject: d should get Inline from b
    let result = reproject_after_mutation("[A]\nb = { c = 1 }", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("d", "e", Item::from(2i64), arena);
        section.insert(k, v, arena);
    });
    assert!(
        result.contains("b = { c = 1 }"),
        "expected inline b: {result}"
    );
    assert!(
        result.contains("d = { e = 2 }"),
        "expected inline d: {result}"
    );
}

#[test]
fn many_dotted_siblings_new_at_end() {
    let input = "\
[A]
a.x = 1
b.x = 2
c.x = 3
d.x = 4
e.x = 5";
    let result = reproject_after_mutation(input, |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("f", "x", Item::from(6i64), arena);
        section.insert(k, v, arena);
        let (k, v) = make_nested("g", "x", Item::from(7i64), arena);
        section.insert(k, v, arena);
    });
    assert!(result.contains("f.x = 6"), "expected dotted f.x: {result}");
    assert!(result.contains("g.x = 7"), "expected dotted g.x: {result}");
    // Verify original entries survived
    for ch in ['a', 'b', 'c', 'd', 'e'] {
        let pat = format!("{ch}.x = ");
        assert!(result.contains(&pat), "missing {ch}: {result}");
    }
}

#[test]
fn many_inline_siblings_new_at_end() {
    let input = "\
[A]
a = { x = 1 }
b = { x = 2 }
c = { x = 3 }";
    let result = reproject_after_mutation(input, |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("d", "x", Item::from(4i64), arena);
        section.insert(k, v, arena);
        let (k, v) = make_nested("e", "x", Item::from(5i64), arena);
        section.insert(k, v, arena);
    });
    assert!(
        result.contains("d = { x = 4 }"),
        "expected inline d: {result}"
    );
    assert!(
        result.contains("e = { x = 5 }"),
        "expected inline e: {result}"
    );
}

#[test]
fn new_deep_nested_sibling_inherits_dotted() {
    // Original has b.c = 1, insert d.e.f.g = 2 (3 levels deep).
    // d inherits dotted from b. Normalization demotes inner tables e
    // and f to dotted to keep the parent body-level, preserving
    // source ordering in reprojected_order mode.
    let result = reproject_after_mutation("[A]\nb.c = 1", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let mut f_table = Table::default();
        f_table.insert(Key::anon("g"), Item::from(2i64), arena);
        let mut e_table = Table::default();
        e_table.insert(Key::anon("f"), f_table.into_item(), arena);
        let mut d_table = Table::default();
        d_table.insert(Key::anon("e"), e_table.into_item(), arena);
        section.insert(Key::anon("d"), d_table.into_item(), arena);
    });
    assert_eq!(result, "[A]\nb.c = 1\nd.e.f.g = 2\n");
}

#[test]
fn new_deep_nested_sibling_inherits_inline() {
    let result = reproject_after_mutation("[A]\nb = { c = 1 }", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let mut f_table = Table::default();
        f_table.insert(Key::anon("g"), Item::from(2i64), arena);
        let mut e_table = Table::default();
        e_table.insert(Key::anon("f"), f_table.into_item(), arena);
        let mut d_table = Table::default();
        d_table.insert(Key::anon("e"), e_table.into_item(), arena);
        section.insert(Key::anon("d"), d_table.into_item(), arena);
    });
    assert_eq!(
        result,
        "[A]\nb = { c = 1 }\nd = { e = { f = { g = 2 } } }\n"
    );
}

#[test]
fn modified_value_plus_new_sibling_dotted() {
    let result = reproject_after_mutation("[A]\nb.c = 1", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        // Change b.c from 1 to 99
        let b = section.get_mut("b").unwrap().as_table_mut().unwrap();
        let c = b.get_mut("c").unwrap();
        *c = Item::from(99i64);
        // Add new sibling
        let (k, v) = make_nested("d", "e", Item::from(2i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "[A]\nb.c = 99\nd.e = 2\n");
}

#[test]
fn new_siblings_in_different_sections() {
    let input = "\
[A]
b.c = 1

[B]
x = { y = 2 }";
    let result = reproject_after_mutation(input, |root, arena| {
        // Add dotted sibling to A
        let section_a = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("d", "e", Item::from(3i64), arena);
        section_a.insert(k, v, arena);
        // Add inline sibling to B
        let section_b = root.get_mut("B").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("z", "w", Item::from(4i64), arena);
        section_b.insert(k, v, arena);
    });
    // A's new sibling should be dotted, B's should be inline
    assert!(
        result.contains("d.e = 3"),
        "expected dotted d.e in A: {result}"
    );
    assert!(
        result.contains("z = { w = 4 }"),
        "expected inline z in B: {result}"
    );
}

/// Parse input, reproject identity (same src and dest), emit with config,
/// and return the output. Unchanged scalars should be preserved verbatim.
fn emit_with_projection(input: &str) -> String {
    let arena = Arena::new();
    let src_doc = parse(input, &arena).unwrap();

    let mut dest_doc = parse(input, &arena).unwrap();

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    let normalized = dest_doc.table.normalize();
    let config = EmitConfig {
        projected_source_text: input,
        projected_source_items: &items,
        reprojected_order: false,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(normalized, &config, &mut buf);
    String::from_utf8(buf).unwrap()
}

/// Parse input, apply mutation to dest, reproject from original, emit with config.
fn emit_projected_after_mutation(
    input: &str,
    mutate: impl for<'a> FnOnce(&mut Table<'a>, &'a Arena),
) -> String {
    let arena = Arena::new();
    let src_doc = parse(input, &arena).unwrap();

    let mut dest_doc = parse(input, &arena).unwrap();

    mutate(&mut dest_doc.table, &arena);

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    let normalized = dest_doc.table.normalize();
    let config = EmitConfig {
        projected_source_text: input,
        projected_source_items: &items,
        reprojected_order: false,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(normalized, &config, &mut buf);
    String::from_utf8(buf).unwrap()
}

// Scalar format preservation tests: moved to testdata/emit_identity.toml

#[test]
fn changed_value_uses_default_format() {
    let input = "a = 0xFF\nb = 0o77";
    let result = emit_projected_after_mutation(input, |root, _arena| {
        let b = root.get_mut("b").unwrap();
        *b = Item::from(99i64);
    });
    // a is unchanged → preserved as hex; b is changed → default decimal
    assert!(
        result.contains("a = 0xFF"),
        "a should be preserved: {result}"
    );
    assert!(result.contains("b = 99"), "b should be decimal: {result}");
}

#[test]
fn new_value_uses_default_format() {
    let input = "a = 0xFF";
    let result = emit_projected_after_mutation(input, |root, arena| {
        root.insert(Key::anon("b"), Item::from(42i64), arena);
    });
    assert!(
        result.contains("a = 0xFF"),
        "a should be preserved: {result}"
    );
    assert!(result.contains("b = 42"), "b should be decimal: {result}");
}

#[test]
fn preserves_scalars_in_inline_table() {
    let result = emit_with_projection("t = { color = 0xFF0000, name = 'test' }");
    assert!(
        result.contains("0xFF0000"),
        "hex should be preserved: {result}"
    );
    assert!(
        result.contains("'test'"),
        "literal string should be preserved: {result}"
    );
}

#[test]
fn preserves_scalars_in_array() {
    let result = emit_with_projection("vals = [0xFF, 0o77, 0b1010]");
    assert!(result.contains("0xFF"), "hex: {result}");
    assert!(result.contains("0o77"), "octal: {result}");
    assert!(result.contains("0b1010"), "binary: {result}");
}

#[test]
fn preserves_scalars_in_header_section() {
    let input = "[config]\nport = 0x1F90\nname = 'myapp'";
    let result = emit_with_projection(input);
    assert!(result.contains("0x1F90"), "hex port: {result}");
    assert!(result.contains("'myapp'"), "literal string: {result}");
}

// Key format preservation tests: moved to testdata/emit_identity.toml

#[test]
fn new_key_uses_default_format() {
    let input = "'existing' = 1";
    let result = emit_projected_after_mutation(input, |root, arena| {
        root.insert(Key::anon("new key"), Item::from(2i64), arena);
    });
    assert!(result.contains("'existing'"), "existing: {result}");
    // New key has no source span, falls back to default (basic quoted)
    assert!(result.contains("\"new key\""), "new: {result}");
}

// Whitespace & comment preservation tests: moved to testdata/emit_identity.toml

#[test]
fn changed_inline_table_element_falls_back() {
    let input = "t = {  x = 1 ,  y = 2  }";
    let result = emit_projected_after_mutation(input, |root, _arena| {
        let t = root.get_mut("t").unwrap().as_table_mut().unwrap();
        let x = t.get_mut("x").unwrap();
        *x = Item::from(99i64);
    });
    // Container was modified, so it falls back to formatted emit
    assert!(result.contains("x = 99"), "x should be 99: {result}");
    assert!(result.contains("y = 2"), "y should be 2: {result}");
}

#[test]
fn changed_array_element_falls_back() {
    let input = "a = [  1 ,  2 ,  3  ]";
    let result = emit_projected_after_mutation(input, |root, _arena| {
        let a = root.get_mut("a").unwrap().as_array_mut().unwrap();
        a.as_mut_slice()[1] = Item::from(99i64);
    });
    assert!(result.contains("99"), "should contain 99: {result}");
}

#[test]
fn array_append_preserves_comment() {
    let input = "a = [\n  1,\n  2, # Two is a nice number\n  3,\n]";
    let result = emit_projected_after_mutation(input, |root, arena| {
        let a = root.get_mut("a").unwrap().as_array_mut().unwrap();
        a.push(Item::from(4i64), arena);
    });
    assert_eq!(
        result,
        "a = [\n  1,\n  2, # Two is a nice number\n  3,\n  4,\n]"
    );
}

#[test]
fn inline_table_append_preserves_comment() {
    let input = "t = {\n  x = 1,\n  y = 2, # Two is nice\n  z = 3,\n}";
    let result = emit_projected_after_mutation(input, |root, arena| {
        let t = root.get_mut("t").unwrap().as_table_mut().unwrap();
        t.insert(Key::anon("w"), Item::from(4i64), arena);
    });
    assert_eq!(
        result,
        "t = {\n  x = 1,\n  y = 2, # Two is nice\n  z = 3,\n  w = 4,\n}"
    );
}

#[test]
fn inline_table_remove_entry_preserves_comment() {
    let input = "t = {\n  x = 1,\n  y = 2, # Two is nice\n  z = 3,\n}";
    let result = emit_projected_after_mutation(input, |root, _arena| {
        let t = root.get_mut("t").unwrap().as_table_mut().unwrap();
        t.remove_entry("x");
    });
    // x removed; y and z preserved with comment on y
    assert!(
        result.contains("y = 2, # Two is nice"),
        "comment should be preserved: {result}"
    );
    assert!(!result.contains("x = 1"), "x should be removed: {result}");
    assert!(result.contains("z = 3"), "z should remain: {result}");
}

#[test]
fn array_remove_element_preserves_comment() {
    let input = "a = [\n  1,\n  2, # Two is nice\n  3,\n]";
    let result = emit_projected_after_mutation(input, |root, _arena| {
        let a = root.get_mut("a").unwrap().as_array_mut().unwrap();
        a.as_mut_slice()[0] = Item::from(99i64);
    });
    // Element 0 changed; element 1 (with comment) and 2 preserved
    assert!(
        result.contains("# Two is nice"),
        "comment should be preserved: {result}"
    );
    assert!(
        result.contains("99"),
        "changed value should appear: {result}"
    );
}

// More whitespace/comment/key preservation tests: moved to testdata/emit_identity.toml

#[test]
fn plain_emit_ignores_whitespace() {
    // Without reprojection, extra whitespace is normalized
    let arena = Arena::new();
    let doc = parse("x  =  1 # comment", &arena).unwrap();
    let normalized = doc.table().try_as_normalized().unwrap();
    let mut buf = Vec::new();
    emit::emit(normalized, &mut buf);
    let result = String::from_utf8(buf).unwrap();
    assert_eq!(result, "x = 1\n");
}

// full_document_whitespace_preservation: moved to testdata/emit_identity.toml

fn flag_name(flag: u32) -> &'static str {
    match flag {
        0 => "NONE",
        1 => "???1",
        2 => "ARRAY",
        3 => "AOT",
        4 => "IMPLICIT",
        5 => "DOTTED",
        6 => "HEADER",
        7 => "FROZEN",
        _ => "UNKNOWN",
    }
}

/// Formats a table's entries for debug output, including spans, flags,
/// and hints-mode metadata (similar to fuzz/src/gen_tree.rs print_item).
fn debug_table(table: &Table<'_>) -> String {
    fn span_str(item: &Item<'_>) -> String {
        let span = item.span();
        if span.is_empty() {
            "no-span".to_string()
        } else {
            format!("{}..{}", span.start, span.end)
        }
    }

    fn key_span_str(key: &Key<'_>) -> String {
        if key.span.is_empty() {
            "no-span".to_string()
        } else {
            format!("{}..{}", key.span.start, key.span.end)
        }
    }

    fn fmt_item(item: &Item<'_>, indent: usize, prefix: &str, out: &mut String) {
        use std::fmt::Write;
        let pad = " ".repeat(indent);
        let flag = flag_name(item.flag());
        let sp = span_str(item);

        match item.value() {
            Value::String(s) => {
                writeln!(out, "{pad}{prefix}String({flag}) [{sp}] = {s:?}").unwrap();
            }
            Value::Integer(i) => {
                writeln!(out, "{pad}{prefix}Integer({flag}) [{sp}] = {i}").unwrap();
            }
            Value::Float(f) => {
                writeln!(out, "{pad}{prefix}Float({flag}) [{sp}] = {f}").unwrap();
            }
            Value::Boolean(b) => {
                writeln!(out, "{pad}{prefix}Boolean({flag}) [{sp}] = {b}").unwrap();
            }
            Value::DateTime(dt) => {
                writeln!(out, "{pad}{prefix}DateTime({flag}) [{sp}] = {dt:?}").unwrap();
            }
            Value::Array(arr) => {
                writeln!(
                    out,
                    "{pad}{prefix}Array({flag}) [{sp}] [{} elements]",
                    arr.len()
                )
                .unwrap();
                for (i, elem) in arr.iter().enumerate() {
                    fmt_item(elem, indent + 2, &format!("[{i}] "), out);
                }
            }
            Value::Table(tab) => {
                writeln!(
                    out,
                    "{pad}{prefix}Table({flag}) [{sp}] {{{} entries}}",
                    tab.len()
                )
                .unwrap();
                for (key, val) in tab {
                    let ks = key_span_str(key);
                    fmt_item(val, indent + 2, &format!("{} [key:{ks}] = ", key.name), out);
                }
            }
        }
    }
    let mut out = String::new();
    for (key, val) in table {
        let ks = key_span_str(key);
        fmt_item(val, 0, &format!("{} [key:{ks}] = ", key.name), &mut out);
    }
    out
}

/// Reprojects src formatting onto dest (two different documents), emits, and
/// asserts the output is semantically equal to dest.
///
/// Provides detailed debug output on failure showing parsed trees, reprojected
/// items, and emitted output for easy reproduction and diagnosis.
fn assert_reproject_edit(src_text: &str, dest_text: &str) {
    // Parse source.
    let arena = Arena::new();
    let src_doc = parse(src_text, &arena).unwrap_or_else(|e| {
        panic!("src failed to parse: {e:?}\nsrc: {src_text:?}");
    });

    // Parse dest (reference copy for semantic comparison).
    let ref_root = parse(dest_text, &arena).unwrap_or_else(|e| {
        panic!("dest failed to parse: {e:?}\ndest: {dest_text:?}");
    });

    // Parse dest (working copy for reproject + normalize).
    let mut dest_doc = parse(dest_text, &arena).unwrap_or_else(|e| {
        panic!("dest failed to parse: {e:?}\ndest: {dest_text:?}");
    });

    // Reproject source structure onto dest.
    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    // Normalize and emit with reprojection config.
    let norm = dest_doc.table.normalize();
    let config = EmitConfig {
        projected_source_text: src_text,
        projected_source_items: &items,
        reprojected_order: false,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(norm, &config, &mut buf);

    // Check: valid UTF-8.
    let output = String::from_utf8(buf).unwrap_or_else(|e| {
        panic!(
            "emit produced invalid UTF-8!\n\
             src: {src_text:?}\n\
             dest: {dest_text:?}\n\
             error: {e}"
        );
    });

    // Check: parses as valid TOML.
    let out_root = parse(&output, &arena).unwrap_or_else(|e| {
        panic!(
            "emit output is not valid TOML!\n\
             \n── source text ({} bytes) ──\n{src_text:?}\
             \n── dest text ({} bytes) ──\n{dest_text:?}\
             \n── parsed source ──\n{}\
             \n── parsed dest (reference) ──\n{}\
             \n── reprojected ({} items) ──\
             \n── emit output ({} bytes) ──\n{output:?}\
             \n── parse error ──\n{e:?}",
            src_text.len(),
            dest_text.len(),
            debug_table(src_doc.table()),
            debug_table(ref_root.table()),
            items.len(),
            output.len(),
        );
    });

    // Check: semantically equal to dest.
    if ref_root.table().as_item() != out_root.table().as_item() {
        panic!(
            "FAILURE: emit output differs semantically from dest!\n\
             \n── source text ({} bytes) ──\n{src_text:?}\
             \n── dest text ({} bytes) ──\n{dest_text:?}\
             \n── parsed source ──\n{}\
             \n── parsed dest (reference) ──\n{}\
             \n── reprojected ({} items) ──\
             \n── emit output ({} bytes) ──\n{output:?}\
             \n── re-parsed output ──\n{}",
            src_text.len(),
            dest_text.len(),
            debug_table(src_doc.table()),
            debug_table(ref_root.table()),
            items.len(),
            output.len(),
            debug_table(out_root.table()),
        );
    }
}

// Cross-document reprojection edit tests: moved to testdata/reproject_edit.toml

/// Reprojects `src_text` onto `dest_text` and emits with config, returning the output string.
fn reproject_edit_output(src_text: &str, dest_text: &str) -> String {
    let arena = Arena::new();
    let src_doc = parse(src_text, &arena).unwrap();

    let mut dest_doc = parse(dest_text, &arena).unwrap();

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    let norm = dest_doc.table.normalize();
    let config = EmitConfig {
        projected_source_text: src_text,
        projected_source_items: &items,
        reprojected_order: false,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(norm, &config, &mut buf);
    String::from_utf8(buf).unwrap()
}

#[test]
fn aot_body_entry_at_eof_is_idempotent() {
    // Body entry at EOF without trailing newline must emit a newline-
    // terminated line; otherwise the next section's \n separator doubles
    // as both line terminator and separator on first pass but not on second.
    let input = "[[a]]\n[[b]]\n[[a]]\nx=1";
    let first = reproject_edit_output(input, input);
    let second = reproject_edit_output(&first, &first);
    assert_eq!(first, second, "emit_with_config must be idempotent");
}

/// Parse input, self-reproject, emit with `reprojected_order: true`.
fn emit_with_reorder(input: &str) -> String {
    let arena = Arena::new();
    let src_doc = parse(input, &arena).unwrap();

    let mut dest_doc = parse(input, &arena).unwrap();

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    let normalized = dest_doc.table.normalize();
    let config = EmitConfig {
        projected_source_text: input,
        projected_source_items: &items,
        reprojected_order: true,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(normalized, &config, &mut buf);
    String::from_utf8(buf).unwrap()
}

/// Reprojects `src_text` onto `dest_text` with `reprojected_order: true`.
fn reproject_edit_reorder(src_text: &str, dest_text: &str) -> String {
    let arena = Arena::new();
    let src_doc = parse(src_text, &arena).unwrap();

    let mut dest_doc = parse(dest_text, &arena).unwrap();

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    let norm = dest_doc.table.normalize();
    let config = EmitConfig {
        projected_source_text: src_text,
        projected_source_items: &items,
        reprojected_order: true,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(norm, &config, &mut buf);
    String::from_utf8(buf).unwrap()
}

// Reorder identity tests: moved to testdata/reorder_identity.toml

/// Like [`run_edit`] but additionally checks that projected entries preserve
/// their source-relative ordering in the output (fuzz invariant 5).
#[track_caller]
fn run_edit_ordered(src_text: &str, dest_text: &str) {
    // Parse source.
    let arena = Arena::new();
    let src_doc = parse(src_text, &arena).unwrap_or_else(|e| {
        panic!("src failed to parse: {e:?}\nsrc: {src_text:?}");
    });

    // Parse dest (reference for semantic comparison).
    let ref_root = parse(dest_text, &arena).unwrap_or_else(|e| {
        panic!("dest failed to parse: {e:?}\ndest: {dest_text:?}");
    });

    // Parse dest (working copy for reproject + normalize).
    let mut dest_doc = parse(dest_text, &arena).unwrap_or_else(|e| {
        panic!("dest failed to parse: {e:?}\ndest: {dest_text:?}");
    });

    // Collect projected source key positions before reproject.
    let mut src_positions: Vec<(Vec<String>, u32)> = Vec::new();
    collect_key_positions(src_doc.table(), &mut Vec::new(), &mut src_positions);

    // Reproject source structure onto dest.
    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    // Normalize and emit with reprojected order.
    let norm = dest_doc.table.normalize();
    let config = EmitConfig {
        projected_source_text: src_text,
        projected_source_items: &items,
        reprojected_order: true,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(norm, &config, &mut buf);

    let output = String::from_utf8(buf.clone()).unwrap_or_else(|e| {
        panic!("emit produced invalid UTF-8!\nsrc: {src_text:?}\ndest: {dest_text:?}\nerror: {e}");
    });

    // Must parse as valid TOML.
    let out_root = parse(&output, &arena).unwrap_or_else(|e| {
        panic!(
            "emit output is not valid TOML!\
             \n── source text ({} bytes) ──\n{src_text:?}\
             \n── dest text ({} bytes) ──\n{dest_text:?}\
             \n── parsed source tree ──\n{}\
             \n── parsed dest (reference) ──\n{}\
             \n── reprojected ({} items) ──\
             \n── emit output ({} bytes) ──\n{output:?}\
             \n── parse error ──\n{e:?}",
            src_text.len(),
            dest_text.len(),
            debug_table(src_doc.table()),
            debug_table(ref_root.table()),
            items.len(),
            output.len(),
        );
    });

    // Semantically equal to dest.
    if ref_root.table().as_item() != out_root.table().as_item() {
        panic!(
            "emit output differs semantically from dest!\
             \n── source text ({} bytes) ──\n{src_text:?}\
             \n── dest text ({} bytes) ──\n{dest_text:?}\
             \n── parsed source tree ──\n{}\
             \n── parsed dest (reference) ──\n{}\
             \n── reprojected ({} items) ──\
             \n── emit output ({} bytes) ──\n{output:?}\
             \n── re-parsed output ──\n{}",
            src_text.len(),
            dest_text.len(),
            debug_table(src_doc.table()),
            debug_table(ref_root.table()),
            items.len(),
            output.len(),
            debug_table(out_root.table()),
        );
    }

    // Idempotency: reproject output onto itself → identical bytes.
    let src2 = parse(&output, &arena).unwrap();
    let mut dest2 = parse(&output, &arena).unwrap();
    let mut items2 = Vec::new();
    reproject(&src2, &mut dest2.table, &mut items2);
    let norm2 = dest2.table.normalize();
    let config2 = EmitConfig {
        projected_source_text: &output,
        projected_source_items: &items2,
        reprojected_order: true,
    };
    let mut buf2 = Vec::new();
    emit::emit_with_config(norm2, &config2, &mut buf2);
    if buf != buf2 {
        let output2 = String::from_utf8_lossy(&buf2);
        panic!(
            "emit not idempotent!\
             \n── source text ({} bytes) ──\n{src_text:?}\
             \n── dest text ({} bytes) ──\n{dest_text:?}\
             \n── parsed source tree ──\n{}\
             \n── parsed dest (reference) ──\n{}\
             \n── reprojected ({} items) ──\
             \n── first emit ──\n{output:?}\
             \n── second emit ──\n{output2:?}",
            src_text.len(),
            dest_text.len(),
            debug_table(src_doc.table()),
            debug_table(ref_root.table()),
            items.len(),
        );
    }

    // Order preservation: projected entries keep source-relative ordering.
    let mut out_positions: Vec<(Vec<String>, u32)> = Vec::new();
    collect_key_positions(out_root.table(), &mut Vec::new(), &mut out_positions);
    assert_order_preserved(&src_positions, &out_positions, src_text, dest_text, &output);
}

/// Collects (key_path, key_span_start) for all entries with non-empty key spans.
fn collect_key_positions(
    table: &Table<'_>,
    path: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, u32)>,
) {
    for (key, item) in table {
        if key.span.is_empty() {
            continue;
        }
        path.push(key.name.to_string());
        out.push((path.clone(), key.span.start));
        match item.value() {
            Value::Table(sub) => {
                collect_key_positions(sub, path, out);
            }
            Value::Array(arr) => {
                for (i, elem) in arr.iter().enumerate() {
                    if let Some(sub) = elem.as_table() {
                        path.push(format!("[{i}]"));
                        collect_key_positions(sub, path, out);
                        path.pop();
                    }
                }
            }
            _ => {}
        }
        path.pop();
    }
}

/// Verifies that for every pair of entries (A, B) present in both source and
/// output, if src_pos(A) < src_pos(B) then out_pos(A) < out_pos(B).
#[track_caller]
fn assert_order_preserved(
    src_positions: &[(Vec<String>, u32)],
    out_positions: &[(Vec<String>, u32)],
    src_text: &str,
    dest_text: &str,
    output: &str,
) {
    use std::collections::HashMap;
    let out_map: HashMap<&[String], u32> = out_positions
        .iter()
        .map(|(path, pos)| (path.as_slice(), *pos))
        .collect();

    let mut matched: Vec<(&[String], u32, u32)> = Vec::new();
    for (path, src_pos) in src_positions {
        if let Some(&out_pos) = out_map.get(path.as_slice()) {
            matched.push((path.as_slice(), *src_pos, out_pos));
        }
    }

    for i in 1..matched.len() {
        let (path_a, src_a, out_a) = &matched[i - 1];
        let (path_b, src_b, out_b) = &matched[i];
        if src_a < src_b {
            assert!(
                out_a < out_b,
                "order violation: {:?} (src={src_a}, out={out_a}) should appear \
                 before {:?} (src={src_b}, out={out_b})\n\
                 src: {src_text:?}\ndest: {dest_text:?}\noutput: {output:?}",
                path_a,
                path_b,
            );
        }
    }
}

// edit_* ordered tests: moved to testdata/edit_ordered.toml

#[test]
fn edit_bom_preserved_at_start_new_dotted_entry_not_inline() {
    run_edit_ordered("\u{feff}A=0\nB.c.d=0", "\u{feff}Z=0\nA.y=0");
}

#[test]
fn edit_bom_preserved_when_new_entry_inserted_first() {
    run_edit_ordered("\u{feff}A=1\nB=0", "\u{feff}Z=1\nA=0");
}

/// Parses source and modified texts, erases aggregate kinds on the dest,
/// reprojects from source into dest, normalizes, emits with config,
/// and asserts the exact output matches `expected`.
#[track_caller]
fn assert_reproject_exact(source: &str, modified: &str, expected: &str) {
    let arena = Arena::new();
    let src_doc = parse(source, &arena).unwrap();

    let mut dest_doc = parse(modified, &arena).unwrap();
    // Some tests are actually testing the dest style are being
    // preserved as well, when they are new
    // erase_kinds(&mut dest_doc.table);

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    let norm = dest_doc.table.normalize();
    let config = EmitConfig {
        projected_source_text: source,
        projected_source_items: &items,
        reprojected_order: true,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(norm, &config, &mut buf);
    let output = String::from_utf8(buf).unwrap();

    if output != expected {
        // Re-parse to get trees for debug (norm consumed dest_doc.table)
        let src_doc2 = parse(source, &arena).unwrap();
        let mut dest_doc2 = parse(modified, &arena).unwrap();
        let mut items2 = Vec::new();
        reproject(&src_doc2, &mut dest_doc2.table, &mut items2);
        panic!(
            "reproject_exact mismatch\
             \n── source ({} bytes) ──\n{source:?}\
             \n── modified ({} bytes) ──\n{modified:?}\
             \n── parsed source tree ──\n{}\
             \n── dest tree (after reproject) ──\n{}\
             \n── reprojected ({} items) ──\
             \n── expected ──\n{expected:?}\
             \n── actual ──\n{output:?}",
            source.len(),
            modified.len(),
            debug_table(src_doc2.table()),
            debug_table(&dest_doc2.table),
            items2.len(),
        );
    }
}

// exact_* and test_* tests: moved to testdata/reproject_exact.toml and testdata/edit_ordered.toml

#[test]
fn ignore_source_order_skips_reordering() {
    // Source has keys in order: a, b, c.
    // Dest has keys reversed: c, b, a — with ignore_source_order set.
    // With reprojected_order=true, normally the emitter sorts by source
    // position (a, b, c). The flag should prevent that, preserving c, b, a.
    let src_text = "c = 3\nb = 2\na = 1\n";

    let arena = Arena::new();
    let src_doc = parse(src_text, &arena).unwrap();

    // Build dest with reversed key order.
    let mut dest_doc = parse("a = 1\nb = 2\nc = 3\n", &arena).unwrap();

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    // Set the flag on the root table.
    dest_doc.table.set_ignore_source_order();

    let norm = dest_doc.table.normalize();
    let config = EmitConfig {
        projected_source_text: src_text,
        projected_source_items: &items,
        reprojected_order: true,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(norm, &config, &mut buf);
    let output = String::from_utf8(buf).unwrap();

    // Keys should follow dest insertion order (a, b, c), not source order (c, b, a).
    // Trailing newline comes from the source text gap handling.
    assert_eq!(output, "a = 1\nb = 2\nc = 3\n");
}

#[test]
fn hints_survive_reprojection() {
    let src_text = "[package]\nname = \"test\"\nversion = \"1.0\"\n";
    let arena = Arena::new();
    let src_doc = parse(src_text, &arena).unwrap();
    let mut dest_doc = parse(src_text, &arena).unwrap();

    // Set hint flag BEFORE reprojection.
    dest_doc.table.set_ignore_source_order();
    assert!(dest_doc.table.ignore_source_order());

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    // The flag must survive reprojection (hints_preserve_mask fix).
    assert!(
        dest_doc.table.ignore_source_order(),
        "ignore_source_order hint was destroyed by reprojection"
    );
}

#[test]
fn ignore_source_style_uses_dest_structure() {
    // Source uses header sections.
    let src_text = "[package]\nname = \"test\"\nversion = \"1.0\"\n";
    let arena = Arena::new();
    let src_doc = parse(src_text, &arena).unwrap();

    // Dest: build programmatically with dotted keys (body-level style).
    let mut pkg = Table::new();
    pkg.set_style(TableStyle::Dotted);
    pkg.insert(Key::anon("name"), Item::from("test"), &arena);
    pkg.insert(Key::anon("version"), Item::from("1.0"), &arena);
    let mut dest = Table::default();
    dest.insert(Key::anon("package"), pkg.into_item(), &arena);

    // Enable ignore_source_style on root: reprojection must not copy
    // Header style from source onto dest's package table.
    dest.set_ignore_source_style();

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest, &mut items);

    // After reprojection, package should still be Dotted (dest's style),
    // not Header (source's style).
    assert_eq!(
        dest["package"].as_table().unwrap().style(),
        TableStyle::Dotted,
        "ignore_source_style should prevent source Header from overwriting dest Dotted"
    );

    // Emit should produce dotted-key output, not header sections.
    let result = emit_table(&mut dest);
    assert!(
        !result.contains("[package]"),
        "output should not contain header section when ignore_source_style is set"
    );
    assert!(
        result.contains("package.name"),
        "output should use dotted keys from dest structure"
    );
}

#[test]
fn ignore_source_style_per_table() {
    // Source: dotted keys inside each header section.
    let src_text = "[a]\nw.x = 1\n\n[b]\nw.x = 2\n";
    let arena = Arena::new();
    let src_doc = parse(src_text, &arena).unwrap();

    // Dest: sections as Header (matching source), but inner tables as Inline
    // (different from source's Dotted).

    let mut inner_a = Table::new();
    inner_a.set_style(TableStyle::Inline);
    inner_a.insert(Key::anon("x"), Item::from(1i64), &arena);
    let mut sect_a = Table::new();
    sect_a.set_style(TableStyle::Header);
    sect_a.insert(Key::anon("w"), inner_a.into_item(), &arena);

    let mut inner_b = Table::new();
    inner_b.set_style(TableStyle::Inline);
    inner_b.insert(Key::anon("x"), Item::from(2i64), &arena);
    let mut sect_b = Table::new();
    sect_b.set_style(TableStyle::Header);
    sect_b.insert(Key::anon("w"), inner_b.into_item(), &arena);

    let mut dest = Table::default();
    dest.insert(Key::anon("a"), sect_a.into_item(), &arena);
    dest.insert(Key::anon("b"), sect_b.into_item(), &arena);

    // Only set ignore_source_style on section "a".
    dest.get_mut("a")
        .unwrap()
        .as_table_mut()
        .unwrap()
        .set_ignore_source_style();

    let mut items = Vec::new();
    reproject(&src_doc, &mut dest, &mut items);

    assert_eq!(
        dest["a"]["w"].as_table().unwrap().style(),
        TableStyle::Inline,
        "section 'a' with ignore_source_style: inner 'w' should keep Inline"
    );

    assert_eq!(
        dest["b"]["w"].as_table().unwrap().style(),
        TableStyle::Dotted,
        "section 'b' without ignore_source_style: inner 'w' should get Dotted from source"
    );
}
// todo should put text in Context.
fn to_toml(reference: &Document<'_>, text: &str, mut table: Table<'_>) -> String {
    let mut buf = Vec::new();
    reproject(&reference, &mut table, &mut buf);
    let emit_config = EmitConfig {
        projected_source_text: text,
        projected_source_items: &buf,
        reprojected_order: true,
    };
    let mut output = Vec::new();
    emit_with_config(table.normalize(), &emit_config, &mut output);
    String::from_utf8(output).expect("serializied TOML to be valid UTF-8")
}

#[test]
fn dependency_add_style_ignore() {
    // Source: dotted keys inside each header section.
    let src_text = r#"
[dependencies]
vim.workspace = true
"#;

    let expected_preserve_style_text = r#"
[dependencies]
vim.workspace = true
vim.features = ["spelling"]
"#;

    let expected_ignored_source_style_text = r#"
[dependencies]
vim = { workspace = true, features = ["spelling"] }
"#;

    let arena = Arena::new();
    let src_doc = parse(src_text, &arena).unwrap();
    // let expected_preserve_style = parse(expected_ignored_source_style_text, &arena).unwrap();
    let expected_ignore_style = parse(expected_ignored_source_style_text, &arena).unwrap();

    let output = to_toml(
        &src_doc,
        src_text,
        expected_ignore_style.table().clone_in(&arena),
    );
    assert_eq!(output, expected_preserve_style_text);

    let mut table = expected_ignore_style.into_table();
    table
        .get_mut("dependencies")
        .unwrap()
        .as_table_mut()
        .unwrap()
        .set_ignore_source_style();

    let output = to_toml(&src_doc, src_text, table);
    assert_eq!(output, expected_ignored_source_style_text);
}

#[test]
fn sort_dependencies_while_preserving_style_and_comments() {
    // Source: dotted keys inside each header section.
    let src_text = r#"
[dependencies]
canary = { path = "../canary" } # used for bird stuff
beta = { version = "0.5", features = [
    "nitro" # gotta go fast
]}
alpha.workspace = true
eta = "0.1"

[dependencies.delta] # This comment is lost when style is ignored
path = "../delta" # same with this comment, but both are kept otherwise
features = [ "inline-everything" ]
"#;

    let sorted_by_style_kept = r#"
[dependencies]
alpha.workspace = true
beta = { version = "0.5", features = [
    "nitro" # gotta go fast
]}
canary = { path = "../canary" } # used for bird stuff
eta = "0.1"

[dependencies.delta] # This comment is lost when style is ignored
path = "../delta" # same with this comment, but both are kept otherwise
features = [ "inline-everything" ]
"#;
    let sorted_by_style_discarded = r#"
[dependencies]
alpha.workspace = true
beta = { version = "0.5", features = [
    "nitro" # gotta go fast
]}
canary = { path = "../canary" } # used for bird stuff

delta = { path = "../delta", features = [ "inline-everything" ] }
eta = "0.1"
"#;

    let arena = Arena::new();
    let src_doc = parse(src_text, &arena).unwrap();
    let mut copy = src_doc.table().clone_in(&arena);
    let dep_table = copy
        .get_mut("dependencies")
        .unwrap()
        .as_table_mut()
        .unwrap();

    dep_table
        .entries_mut()
        .sort_unstable_by_key(|(key, _)| key.name);

    dep_table.set_ignore_source_order();

    let output = to_toml(&src_doc, src_text, copy.clone_in(&arena));
    if output != sorted_by_style_kept {
        println!("=== Expected ===\n {}", sorted_by_style_kept);
        println!("=== Got ===\n {}", output);
        panic!("TOML didn't match expected result after serialization:");
    }

    let dep_table = copy
        .get_mut("dependencies")
        .unwrap()
        .as_table_mut()
        .unwrap();

    for (_, entry) in dep_table {
        if let Some(table) = entry.as_table_mut() {
            table.set_style(TableStyle::Inline);
            table.set_ignore_source_style();
        }
    }
    let output = to_toml(&src_doc, src_text, copy);
    if output != sorted_by_style_discarded {
        println!("=== Expected ===\n {}", sorted_by_style_discarded);
        println!("=== Got ===\n {}", output);
        panic!("TOML didn't match expected result after serialization:");
    }
}

//
// These set FORCE_HASH_COLLISIONS so that every array element hashes to the
// same value, forcing the collision-group cross-product code path.

fn with_forced_collisions(f: impl FnOnce()) {
    super::FORCE_HASH_COLLISIONS.set(true);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    super::FORCE_HASH_COLLISIONS.set(false);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn forced_collision_reorder_matched() {
    // All elements collide → single collision group. Different values break
    // the group prefix, exercising the cross-product fallback.
    with_forced_collisions(|| {
        let src = "a = [1, 2, 3]";
        let dest = "a = [3, 1, 2]";
        assert_reproject_edit(src, dest);
    });
}

#[test]
fn forced_collision_partial_overlap() {
    // src has 3 elements, dest has 4 (one new). Cross-product matches the
    // overlapping elements; the remainder gets fallback pairing.
    with_forced_collisions(|| {
        let src = "a = [1, 2, 3]";
        let dest = "a = [3, 4, 1, 2]";
        assert_reproject_edit(src, dest);
    });
}

#[test]
fn forced_collision_removal() {
    // Dest has fewer elements than src. Cross-product matches what it can;
    // excess src entries go into the fallback list.
    with_forced_collisions(|| {
        let src = "a = [1, 2, 3, 4]";
        let dest = "a = [4, 2]";
        assert_reproject_edit(src, dest);
    });
}

#[test]
fn forced_collision_tables_in_array() {
    // Table elements with different keys in a single collision group.
    with_forced_collisions(|| {
        let src = "a = [{x = 1}, {y = 2}, {z = 3}]";
        let dest = "a = [{z = 3}, {x = 1}]";
        assert_reproject_edit(src, dest);
    });
}

#[test]
fn forced_collision_exact_reorder_preserves_format() {
    // With reprojection, reordered elements should still emit valid TOML
    // and be semantically equivalent to dest.
    with_forced_collisions(|| {
        assert_reproject_exact(
            "a = [1, 2, 3]\nb = 10",
            "a = [2, 3, 1]\nb = 10",
            "a = [2, 3, 1]\nb = 10",
        );
    });
}

#[test]
fn forced_collision_exceeds_cap() {
    // With COLLISION_CAP=16 in test mode, 5 distinct src × 5 distinct dest
    // (25 > 16) exceeds the cap, triggering the skip path.
    with_forced_collisions(|| {
        let src = "a = [1, 2, 3, 4, 5]";
        let dest = "a = [6, 7, 8, 9, 10]";
        assert_reproject_edit(src, dest);
    });
}

#[test]
fn positional_fallback_large_array() {
    // With INDEX_LIMIT=32 in test mode, an array with >32 elements falls
    // back to positional matching.
    let arena = Arena::new();
    let n = 35;
    let mut src_parts = Vec::new();
    let mut dest_parts = Vec::new();
    for i in 0..n {
        src_parts.push(format!("{i}"));
        dest_parts.push(format!("{}", n - 1 - i));
    }
    let src_text = format!("a = [{}]", src_parts.join(", "));
    let dest_text = format!("a = [{}]", dest_parts.join(", "));

    let src_doc = parse(&src_text, &arena).unwrap();
    let mut dest_doc = parse(&dest_text, &arena).unwrap();
    let mut items = Vec::new();
    reproject(&src_doc, &mut dest_doc.table, &mut items);

    // Positional fallback: each dest[i] pairs with src[i].
    // Result should be valid TOML and semantically equal to dest.
    let norm = dest_doc.table.normalize();
    let mut buf = Vec::new();
    emit::emit(norm, &mut buf);
    let output = String::from_utf8(buf).unwrap();
    let out_root = parse(&output, &arena).unwrap();
    assert_eq!(
        out_root.table().as_item(),
        parse(&dest_text, &arena).unwrap().table().as_item(),
        "positional fallback should preserve dest semantics"
    );
}
