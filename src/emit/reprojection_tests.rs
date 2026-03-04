use super::reproject;
use crate::Table;
use crate::arena::Arena;
use crate::emit::EmitConfig;
use crate::item::items_equal;
use crate::item::{ArrayStyle, Item, Key, TableStyle, Value};
use crate::parser::parse;
use crate::{Root, emit, emit_with_config};

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

    let src_root = parse(input, &arena).unwrap();

    let mut dest_root = parse(input, &arena).unwrap();
    erase_kinds(&mut dest_root.table);

    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    let reprojected = emit_table(&mut dest_root.table);
    assert_eq!(
        reprojected, reference,
        "reprojected output should match reference"
    );
}

#[test]
fn header_sections() {
    assert_reproject_recovers(
        "\
[package]
name = \"test\"
version = \"1.0\"

[dependencies]
serde = \"1\"",
    );
}

#[test]
fn dotted_keys() {
    assert_reproject_recovers(
        "\
[a]
b.c = 1
b.d = 2",
    );
}

#[test]
fn inline_tables() {
    assert_reproject_recovers(
        "\
[dependencies]
foo = { version = \"1\" }
bar = { version = \"2\" }",
    );
}

#[test]
fn array_of_tables() {
    assert_reproject_recovers(
        "\
[[servers]]
name = \"alpha\"
port = 8080

[[servers]]
name = \"beta\"
port = 9090",
    );
}

#[test]
fn mixed_structure() {
    assert_reproject_recovers(
        "\
[package]
name = \"demo\"

[dependencies]
serde = { version = \"1\", features = [\"derive\"] }

[[bin]]
name = \"main\"
path = \"src/main.rs\"

[[bin]]
name = \"alt\"
path = \"src/alt.rs\"",
    );
}

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

/// Parses `input`, applies `mutate` to the dest root, reprojects from
/// the original, normalizes, and emits.
fn reproject_after_mutation(
    input: &str,
    mutate: impl for<'a> FnOnce(&mut Table<'a>, &'a Arena),
) -> String {
    let arena = Arena::new();
    let src_root = parse(input, &arena).unwrap();

    let mut dest_root = parse(input, &arena).unwrap();

    mutate(&mut dest_root.table, &arena);

    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    emit_table(&mut dest_root.table)
}

#[test]
fn new_sibling_inherits_dotted_kind() {
    let result = reproject_after_mutation("[A]\nb.c = 1", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("d", "e", Item::from(2i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "\n[A]\nb.c = 1\nd.e = 2");
}

#[test]
fn new_sibling_inherits_inline_kind() {
    let result = reproject_after_mutation("[A]\nb = { c = 1 }", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("d", "e", Item::from(2i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "\n[A]\nb = { c = 1 }\nd = { e = 2 }");
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
    assert_eq!(result, "\n[A]\nb.c = 1\nd.e = 2\nf.g = 3");
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
    assert_eq!(result, "\n[A]\nb = { c = 1 }\nd = { e = 2 }\nf = { g = 3 }");
}

#[test]
fn new_sibling_after_multiple_dotted() {
    let result = reproject_after_mutation("[A]\nb.c = 1\nb.d = 2\nx.y = 3", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("z", "w", Item::from(99i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "\n[A]\nb.c = 1\nb.d = 2\nx.y = 3\nz.w = 99");
}

#[test]
fn new_sibling_inherits_last_match_dotted_after_inline() {
    // b is inline, x is dotted → new sibling after x inherits dotted
    let result = reproject_after_mutation("[A]\nb = { c = 1 }\nx.y = 3", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("z", "w", Item::from(4i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "\n[A]\nb = { c = 1 }\nx.y = 3\nz.w = 4");
}

#[test]
fn new_sibling_inherits_last_match_inline_after_dotted() {
    // b is dotted, x is inline → new sibling after x inherits inline
    let result = reproject_after_mutation("[A]\nb.c = 1\nx = { y = 3 }", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let (k, v) = make_nested("z", "w", Item::from(4i64), arena);
        section.insert(k, v, arena);
    });
    assert_eq!(result, "\n[A]\nb.c = 1\nx = { y = 3 }\nz = { w = 4 }");
}

#[test]
fn new_sibling_before_match_backfills_dotted() {
    // Construct dest manually so new entries come before the matched entry.
    let arena = Arena::new();
    let src_root = parse("[A]\nb.c = 1", &arena).unwrap();

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
    reproject(&src_root, &mut dest, &mut items);

    let result = emit_table(&mut dest);
    // d should be backfilled with dotted kind from b (first match)
    assert_eq!(result, "\n[A]\nd.e = 2\nb.c = 1");
}

#[test]
fn new_sibling_before_match_backfills_inline() {
    let arena = Arena::new();
    let src_root = parse("[A]\nb = { c = 1 }", &arena).unwrap();

    let mut section_a = Table::new();
    section_a.set_style(TableStyle::Header);
    let (k, v) = make_nested("d", "e", Item::from(2i64), &arena);
    section_a.insert(k, v, &arena);
    let (k, v) = make_nested("b", "c", Item::from(1i64), &arena);
    section_a.insert(k, v, &arena);
    let mut dest = Table::default();
    dest.insert(Key::anon("A"), section_a.into_item(), &arena);

    let mut items = Vec::new();
    reproject(&src_root, &mut dest, &mut items);

    let result = emit_table(&mut dest);
    assert_eq!(result, "\n[A]\nd = { e = 2 }\nb = { c = 1 }");
}

#[test]
fn new_scalar_alongside_dotted() {
    let result = reproject_after_mutation("[A]\nb.c = 1", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        section.insert(Key::anon("x"), Item::from(42i64), arena);
    });
    assert_eq!(result, "\n[A]\nb.c = 1\nx = 42");
}

#[test]
fn new_scalar_alongside_inline() {
    let result = reproject_after_mutation("[A]\nb = { c = 1 }", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        section.insert(Key::anon("x"), Item::from(42i64), arena);
    });
    assert_eq!(result, "\n[A]\nb = { c = 1 }\nx = 42");
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
    assert_eq!(result, "\n[A]\nb.c.d = 1\nb.c.e = 2");
}

#[test]
fn new_sibling_deep_inline_nesting() {
    let result = reproject_after_mutation("[A]\nb = { c = { d = 1 } }", |root, arena| {
        let section = root.get_mut("A").unwrap().as_table_mut().unwrap();
        let b = section.get_mut("b").unwrap().as_table_mut().unwrap();
        let c = b.get_mut("c").unwrap().as_table_mut().unwrap();
        c.insert(Key::anon("e"), Item::from(2i64), arena);
    });
    assert_eq!(result, "\n[A]\nb = { c = { d = 1, e = 2 } }");
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
    assert_eq!(result, "\n[A]\nb.c = 1\nd.e.f.g = 2");
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
        "\n[A]\nb = { c = 1 }\nd = { e = { f = { g = 2 } } }"
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
    assert_eq!(result, "\n[A]\nb.c = 99\nd.e = 2");
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

// ==== Scalar format preservation via emit_with_config ====

/// Parse input, reproject identity (same src and dest), emit with config,
/// and return the output. Unchanged scalars should be preserved verbatim.
fn emit_with_projection(input: &str) -> String {
    let arena = Arena::new();
    let src_root = parse(input, &arena).unwrap();

    let mut dest_root = parse(input, &arena).unwrap();

    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    let normalized = dest_root.table.normalize();
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
    let src_root = parse(input, &arena).unwrap();

    let mut dest_root = parse(input, &arena).unwrap();

    mutate(&mut dest_root.table, &arena);

    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    let normalized = dest_root.table.normalize();
    let config = EmitConfig {
        projected_source_text: input,
        projected_source_items: &items,
        reprojected_order: false,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(normalized, &config, &mut buf);
    String::from_utf8(buf).unwrap()
}

#[test]
fn preserves_literal_string() {
    let result = emit_with_projection("name = 'hello'");
    assert_eq!(result, "name = 'hello'");
}

#[test]
fn preserves_basic_string() {
    let result = emit_with_projection(r#"name = "hello""#);
    assert_eq!(result, r#"name = "hello""#);
}

#[test]
fn preserves_multiline_basic_string() {
    let input = "bio = \"\"\"\nhello\nworld\"\"\"";
    let result = emit_with_projection(input);
    assert_eq!(result, input);
}

#[test]
fn preserves_multiline_literal_string() {
    let input = "bio = '''\nhello\nworld'''";
    let result = emit_with_projection(input);
    assert_eq!(result, input);
}

#[test]
fn preserves_string_with_escapes() {
    let input = r#"path = "C:\\Users\\test""#;
    let result = emit_with_projection(input);
    assert_eq!(result, input);
}

#[test]
fn preserves_hex_integer() {
    let result = emit_with_projection("color = 0xFF0000");
    assert_eq!(result, "color = 0xFF0000");
}

#[test]
fn preserves_octal_integer() {
    let result = emit_with_projection("perm = 0o755");
    assert_eq!(result, "perm = 0o755");
}

#[test]
fn preserves_binary_integer() {
    let result = emit_with_projection("mask = 0b11010110");
    assert_eq!(result, "mask = 0b11010110");
}

#[test]
fn preserves_underscored_integer() {
    let result = emit_with_projection("big = 1_000_000");
    assert_eq!(result, "big = 1_000_000");
}

#[test]
fn preserves_hex_with_underscores() {
    let result = emit_with_projection("addr = 0xdead_beef");
    assert_eq!(result, "addr = 0xdead_beef");
}

#[test]
fn preserves_underscored_float() {
    let result = emit_with_projection("val = 1_000.5");
    assert_eq!(result, "val = 1_000.5");
}

#[test]
fn preserves_exponent_float() {
    let result = emit_with_projection("val = 5e+22");
    assert_eq!(result, "val = 5e+22");
}

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

#[test]
fn preserves_boolean() {
    let result = emit_with_projection("flag = true");
    assert_eq!(result, "flag = true");
}

#[test]
fn preserves_datetime() {
    let result = emit_with_projection("dt = 2024-01-15T10:30:00Z");
    assert!(result.contains("2024-01-15T10:30:00Z"), "{result}");
}

// ==== Key format preservation ====

#[test]
fn preserves_literal_quoted_key() {
    let result = emit_with_projection("'hello world' = 1");
    assert_eq!(result, "'hello world' = 1");
}

#[test]
fn preserves_basic_quoted_key() {
    let result = emit_with_projection(r#""hello world" = 1"#);
    assert_eq!(result, r#""hello world" = 1"#);
}

#[test]
fn preserves_bare_key() {
    let result = emit_with_projection("my-key = 1");
    assert_eq!(result, "my-key = 1");
}

#[test]
fn preserves_quoted_key_in_header() {
    let result = emit_with_projection("['my section']\nval = 1");
    assert!(result.contains("['my section']"), "header key: {result}");
}

#[test]
fn preserves_quoted_key_in_dotted() {
    let result = emit_with_projection("'a b'.c = 1");
    assert_eq!(result, "'a b'.c = 1");
}

#[test]
fn preserves_quoted_key_in_inline_table() {
    let result = emit_with_projection("t = { 'a b' = 1, c = 2 }");
    assert!(result.contains("'a b'"), "inline key: {result}");
}

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

// ==== Whitespace & comment preservation ====

#[test]
fn preserves_inline_table_whitespace() {
    // Extra spaces inside inline table should be preserved
    let result = emit_with_projection("t = {  x = 1 ,  y = 2  }");
    assert_eq!(result, "t = {  x = 1 ,  y = 2  }");
}

#[test]
fn preserves_inline_array_whitespace() {
    let result = emit_with_projection("a = [  1 ,  2 ,  3  ]");
    assert_eq!(result, "a = [  1 ,  2 ,  3  ]");
}

#[test]
fn preserves_inline_table_with_comment() {
    // TOML 1.1 allows newlines in inline tables; comments possible
    let input = "t = {\n  x = 1, # comment\n  y = 2,\n}";
    let result = emit_with_projection(input);
    assert_eq!(result, input);
}

#[test]
fn preserves_multiline_inline_array() {
    let input = "a = [\n  1,\n  2,\n  3,\n]";
    let result = emit_with_projection(input);
    assert_eq!(result, input);
}

#[test]
fn preserves_inline_array_trailing_comma() {
    let input = "a = [1, 2, 3,]";
    let result = emit_with_projection(input);
    assert_eq!(result, input);
}

#[test]
fn preserves_inline_table_trailing_comma() {
    let input = "t = { x = 1, y = 2, }";
    let result = emit_with_projection(input);
    assert_eq!(result, input);
}

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

#[test]
fn preserves_trailing_comment() {
    let result = emit_with_projection("x = 1 # my comment");
    assert_eq!(result, "x = 1 # my comment");
}

#[test]
fn preserves_trailing_comment_in_section() {
    let input = "[pkg]\nname = 'test' # the name\nversion = '1.0' # the version";
    let result = emit_with_projection(input);
    // Section headers always get a \n prefix in emit output
    assert_eq!(
        result,
        "\n[pkg]\nname = 'test' # the name\nversion = '1.0' # the version"
    );
}

#[test]
fn preserves_equals_whitespace() {
    let result = emit_with_projection("x  =  1");
    assert_eq!(result, "x  =  1");
}

#[test]
fn preserves_no_spaces_around_equals() {
    let result = emit_with_projection("x=1");
    assert_eq!(result, "x=1");
}

#[test]
fn preserves_blank_lines_between_entries() {
    let input = "a = 1\n\nb = 2\n\nc = 3";
    let result = emit_with_projection(input);
    assert_eq!(result, input);
}

#[test]
fn preserves_standalone_comments_between_entries() {
    let input = "a = 1\n# standalone comment\nb = 2";
    let result = emit_with_projection(input);
    assert_eq!(result, input);
}

#[test]
fn preserves_multiple_comment_lines() {
    let input = "a = 1\n# line 1\n# line 2\nb = 2";
    let result = emit_with_projection(input);
    assert_eq!(result, input);
}

#[test]
fn preserves_blank_lines_and_comments_mixed() {
    let input = "a = 1\n\n# comment\n\nb = 2";
    let result = emit_with_projection(input);
    assert_eq!(result, input);
}

#[test]
fn preserves_header_comment() {
    let input = "[pkg] # package section\nname = 'test'";
    let result = emit_with_projection(input);
    assert_eq!(result, "\n[pkg] # package section\nname = 'test'");
}

#[test]
fn preserves_header_whitespace() {
    let input = "[  pkg  ]\nname = 'test'";
    let result = emit_with_projection(input);
    assert_eq!(result, "\n[  pkg  ]\nname = 'test'");
}

#[test]
fn preserves_aot_header_comment() {
    let input = "[[ servers ]] # server list\nname = 'alpha'";
    let result = emit_with_projection(input);
    assert_eq!(result, "\n[[ servers ]] # server list\nname = 'alpha'");
}

#[test]
fn preserves_section_body_comments() {
    let input = "[pkg]\n# name of package\nname = 'test'\n# version\nversion = '1.0'";
    let result = emit_with_projection(input);
    assert_eq!(
        result,
        "\n[pkg]\n# name of package\nname = 'test'\n# version\nversion = '1.0'"
    );
}

#[test]
fn preserves_section_body_blank_lines() {
    let input = "[pkg]\nname = 'test'\n\nversion = '1.0'";
    let result = emit_with_projection(input);
    assert_eq!(result, "\n[pkg]\nname = 'test'\n\nversion = '1.0'");
}

#[test]
fn preserves_dotted_entry_comment() {
    let result = emit_with_projection("a.b = 1 # dotted comment");
    assert_eq!(result, "a.b = 1 # dotted comment");
}

#[test]
fn preserves_dotted_entry_whitespace() {
    let result = emit_with_projection("a.b  =  1");
    assert_eq!(result, "a.b  =  1");
}

#[test]
fn plain_emit_ignores_whitespace() {
    // Without reprojection, extra whitespace is normalized
    let arena = Arena::new();
    let root = parse("x  =  1 # comment", &arena).unwrap();
    let normalized = root.table().try_as_normalized().unwrap();
    let mut buf = Vec::new();
    emit::emit(normalized, &mut buf);
    let result = String::from_utf8(buf).unwrap();
    assert_eq!(result, "x = 1");
}

#[test]
fn full_document_whitespace_preservation() {
    let input = "\
# Config file

title = 'My App'

[package] # package info
name = 'test'
version = '1.0' # semver

[dependencies]
serde = { version = '1', features = ['derive'] }";
    let result = emit_with_projection(input);
    // Root body entries are emitted first (preserving comments/gaps),
    // then sections with \n prefix. The root comment and title before [package]
    // are preserved as inter-entry gaps.
    assert_eq!(result, input);
}

// ==== Cross-document reprojection (edit scenario) ====

/// Formats a table's entries for debug output.
fn debug_table(table: &Table<'_>) -> String {
    fn fmt_item(item: &Item<'_>, indent: usize, prefix: &str, out: &mut String) {
        use std::fmt::Write;
        let pad = " ".repeat(indent);
        match item.value() {
            Value::String(s) => writeln!(out, "{pad}{prefix}String = {s:?}").unwrap(),
            Value::Integer(i) => writeln!(out, "{pad}{prefix}Integer = {i}").unwrap(),
            Value::Float(f) => writeln!(out, "{pad}{prefix}Float = {f}").unwrap(),
            Value::Boolean(b) => writeln!(out, "{pad}{prefix}Boolean = {b}").unwrap(),
            Value::DateTime(dt) => writeln!(out, "{pad}{prefix}DateTime = {dt:?}").unwrap(),
            Value::Array(arr) => {
                writeln!(out, "{pad}{prefix}Array [{} elements]", arr.len()).unwrap();
                for (i, elem) in arr.iter().enumerate() {
                    fmt_item(elem, indent + 2, &format!("[{i}] "), out);
                }
            }
            Value::Table(tab) => {
                writeln!(out, "{pad}{prefix}Table ({} entries)", tab.len()).unwrap();
                for (key, val) in tab {
                    fmt_item(val, indent + 2, &format!("{} = ", key.name), out);
                }
            }
        }
    }
    let mut out = String::new();
    for (key, val) in table {
        fmt_item(val, 0, &format!("{} = ", key.name), &mut out);
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
    let src_root = parse(src_text, &arena).unwrap_or_else(|e| {
        panic!("src failed to parse: {e:?}\nsrc: {src_text:?}");
    });

    // Parse dest (reference copy for semantic comparison).
    let ref_root = parse(dest_text, &arena).unwrap_or_else(|e| {
        panic!("dest failed to parse: {e:?}\ndest: {dest_text:?}");
    });

    // Parse dest (working copy for reproject + normalize).
    let mut dest_root = parse(dest_text, &arena).unwrap_or_else(|e| {
        panic!("dest failed to parse: {e:?}\ndest: {dest_text:?}");
    });

    // Reproject source structure onto dest.
    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    // Normalize and emit with reprojection config.
    let norm = dest_root.table.normalize();
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
            debug_table(src_root.table()),
            debug_table(ref_root.table()),
            items.len(),
            output.len(),
        );
    });

    // Check: semantically equal to dest.
    if !items_equal(ref_root.table().as_item(), out_root.table().as_item()) {
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
            debug_table(src_root.table()),
            debug_table(ref_root.table()),
            items.len(),
            output.len(),
            debug_table(out_root.table()),
        );
    }
}

#[test]
fn reproject_edit_prefix_key_not_leaked() {
    // Source has "a-b" before "a". When dest only wants "a", the emit must
    // not leak the "a-b" entry from source via gap emission.
    assert_reproject_edit(
        "\"a-b\" = []\n\"a\" = []\n\"b\" = []\n\"c\" = []\n\"d\" = []\n",
        "\"a\" = []\n",
    );
}

#[test]
fn reproject_edit_unmatched_parent_clears_child_spans() {
    // When dest nests a key under a new parent that doesn't exist in source,
    // child key spans from dest must be cleared so emit doesn't index into
    // source text at wrong positions.
    assert_reproject_edit("a = 0\n", "[b]\na = 0\n");
}

#[test]
fn reproject_edit_type_mismatch_clears_child_spans() {
    // When source key `a` is a scalar but dest key `a` is a table,
    // the type mismatch must clear stale dest-text spans on children
    // so emit doesn't index into source text at wrong positions.
    assert_reproject_edit("a=0", "[a]\nb=0");
}

#[test]
fn reproject_edit_array_excess_element_clears_child_spans() {
    // When source array is empty but dest array has elements (e.g. AOT),
    // unmatched elements' children must have stale spans cleared.
    assert_reproject_edit("a=[]", "[[a]]\nb=0");
}

#[test]
fn reproject_edit_container_element_partial_not_verbatim() {
    // Source array element is empty table, dest element has a child.
    // Emit must not use verbatim source (which omits the child) just
    // because the container element has a reprojection index.
    assert_reproject_edit("a=[{}]", "a=[{b=0}]");
}

#[test]
fn reproject_edit_multiline_array_partial_element() {
    // Source has multiline array with empty table; dest adds a child.
    // try_emit_array_partial must not emit source {} verbatim for
    // a partially-matched element — it must use format_value instead.
    assert_reproject_edit("a = [\n{},\n]", "a = [{b = 0}]");
}

#[test]
fn reproject_edit_multiline_array_element_on_opening_line() {
    // Source array has first element on the same line as [.
    // Indent detection must not pick up key prefix as indentation.
    assert_reproject_edit("a = [{\n},]", "a = [{b = 0}]");
}

#[test]
fn reproject_edit_aot_source_inline_dest() {
    // Source has AOT ([[a]]) but dest has inline array (a = [...]).
    // AOT span ends with ] from inner value (a = []), which fools
    // try_emit_array_partial's bracket check. Must skip AOT spans.
    assert_reproject_edit("[[a]]\na = []\n[[a]]\na = []", "a = [{}, 0]");
}

#[test]
fn reproject_edit_array_trailing_comment_with_commas() {
    // Source element has trailing comment containing commas (# ,,,).
    // The comma check must ignore commas inside comments, otherwise
    // no real separator comma is added between elements.
    assert_reproject_edit("a = [\n\"b\" # ,,,\n,\n]", "a = [\"b\", 0]");
}

#[test]
fn reproject_edit_nested_array_clears_stale_spans() {
    // Unmatched array element that is itself an array containing a table:
    // the inner table's key spans must be cleared recursively.
    assert_reproject_edit("a=[]", "a=[[{b=0}]]");
}

#[test]
fn reproject_edit_body_entry_not_captured_by_aot() {
    // Body entry belongs to [a], not to [[a.a.z]].
    // A dotted subtable with an AOT child must not capture parent body entries.
    assert_reproject_edit("[[a.a.z]]\n[a]\nx=0\na.c=0", "[[a.a.z]]\n[a]\nx=0\na.c=0");
    // Same pattern with a root-level AOT sibling.
    assert_reproject_edit(
        "[[b]]\n[[a.a.z]]\n[a]\nx=0\na.c=0",
        "[[b]]\n[[a.a.z]]\n[a]\nx=0\na.c=0",
    );
}

/// Reprojects `src_text` onto `dest_text` and emits with config, returning the output string.
fn reproject_edit_output(src_text: &str, dest_text: &str) -> String {
    let arena = Arena::new();
    let src_root = parse(src_text, &arena).unwrap();

    let mut dest_root = parse(dest_text, &arena).unwrap();

    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    let norm = dest_root.table.normalize();
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

// ==== Fragment-based reordering (reprojected_order) ====

/// Parse input, self-reproject, emit with `reprojected_order: true`.
fn emit_with_reorder(input: &str) -> String {
    let arena = Arena::new();
    let src_root = parse(input, &arena).unwrap();

    let mut dest_root = parse(input, &arena).unwrap();

    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    let normalized = dest_root.table.normalize();
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
    let src_root = parse(src_text, &arena).unwrap();

    let mut dest_root = parse(dest_text, &arena).unwrap();

    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    let norm = dest_root.table.normalize();
    let config = EmitConfig {
        projected_source_text: src_text,
        projected_source_items: &items,
        reprojected_order: true,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(norm, &config, &mut buf);
    String::from_utf8(buf).unwrap()
}

#[test]
fn reorder_interleaved_dotted_keys() {
    // Source: b.c=1, e.f=2, b.g=3 — normalization groups b entries together,
    // but reprojected_order should preserve original interleaving.
    let input = "[A]\nb.c = 1\ne.f = 2\nb.g = 3";
    let result = emit_with_reorder(input);
    // With fragment reordering, body entries should follow source order
    assert_eq!(result, "\n[A]\nb.c = 1\ne.f = 2\nb.g = 3");
}

#[test]
fn reorder_interleaved_headers() {
    // Source: [A.b.c], [A.e.f], [A.b.g] — normalization may reorder these,
    // but reprojected_order preserves source ordering.
    let input = "[A.b.c]\nx = 1\n\n[A.e.f]\ny = 2\n\n[A.b.g]\nz = 3";
    let result = emit_with_reorder(input);
    assert_eq!(
        result,
        "\n[A.b.c]\nx = 1\n\n[A.e.f]\ny = 2\n\n[A.b.g]\nz = 3"
    );
}

#[test]
fn reorder_inline_table_dotted_keys() {
    // Source: { b.c = 1, e.f = 2, b.g = 3 }
    // Normalization groups b entries; reprojected_order restores interleaving.
    let input = "t = { b.c = 1, e.f = 2, b.g = 3 }";
    let result = emit_with_reorder(input);
    assert_eq!(result, "t = { b.c = 1, e.f = 2, b.g = 3 }");
}

#[test]
fn reorder_new_key_between_projected() {
    // Source has a=1, c=3. Dest has a=1, b=2, c=3.
    // New key b should sort between a and c.
    let src = "a = 1\nc = 3";
    let dest = "a = 1\nb = 2\nc = 3";
    let result = reproject_edit_reorder(src, dest);
    assert_eq!(result, "a = 1\nb = 2\nc = 3");
}

#[test]
fn reorder_mixed_dotted_and_headers() {
    let input = "a.b = 1\n\n[c]\nx = 2\n\n[a.d]\ny = 3";
    let result = emit_with_reorder(input);
    assert_eq!(result, "a.b = 1\n\n[c]\nx = 2\n\n[a.d]\ny = 3");
}

#[test]
fn reorder_no_interleaving_identity() {
    let input = "[A]\nx = 1\ny = 2\n\n[B]\nz = 3";
    let result = emit_with_reorder(input);
    assert_eq!(result, "\n[A]\nx = 1\ny = 2\n\n[B]\nz = 3");
}

#[test]
fn reorder_interleaved_aot() {
    let input = "[[a]]\nx = 1\n\n[[b]]\ny = 2\n\n[[a]]\nz = 3";
    let result = emit_with_reorder(input);
    assert_eq!(result, "\n[[a]]\nx = 1\n\n[[b]]\ny = 2\n\n[[a]]\nz = 3");
}

#[test]
fn reorder_root_body_with_headers() {
    let input = "name = 'test'\n\n[pkg]\nversion = '1.0'\n\n[deps]\nserde = '1'";
    let result = emit_with_reorder(input);
    assert_eq!(
        result,
        "name = 'test'\n\n[pkg]\nversion = '1.0'\n\n[deps]\nserde = '1'"
    );
}

/// Like [`run_edit`] but additionally checks that projected entries preserve
/// their source-relative ordering in the output (fuzz invariant 5).
#[track_caller]
fn run_edit_ordered(src_text: &str, dest_text: &str) {
    // Parse source.
    let arena = Arena::new();
    let src_root = parse(src_text, &arena).unwrap_or_else(|e| {
        panic!("src failed to parse: {e:?}\nsrc: {src_text:?}");
    });

    // Parse dest (reference for semantic comparison).
    let ref_root = parse(dest_text, &arena).unwrap_or_else(|e| {
        panic!("dest failed to parse: {e:?}\ndest: {dest_text:?}");
    });

    // Parse dest (working copy for reproject + normalize).
    let mut dest_root = parse(dest_text, &arena).unwrap_or_else(|e| {
        panic!("dest failed to parse: {e:?}\ndest: {dest_text:?}");
    });

    // Collect projected source key positions before reproject.
    let mut src_positions: Vec<(Vec<String>, u32)> = Vec::new();
    collect_key_positions(src_root.table(), &mut Vec::new(), &mut src_positions);

    // Reproject source structure onto dest.
    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    // Normalize and emit with reprojected order.
    let norm = dest_root.table.normalize();
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
            "emit output is not valid TOML!\n\
             src: {src_text:?}\ndest: {dest_text:?}\n\
             output: {output:?}\nerror: {e:?}"
        );
    });

    // Semantically equal to dest.
    if !items_equal(ref_root.table().as_item(), out_root.table().as_item()) {
        panic!(
            "emit output differs semantically from dest!\n\
             src: {src_text:?}\ndest: {dest_text:?}\n\
             output: {output:?}\n\
             parsed src:\n{}\nparsed dest:\n{}\nparsed output:\n{}",
            debug_table(src_root.table()),
            debug_table(ref_root.table()),
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
    assert_eq!(
        buf,
        buf2,
        "emit not idempotent!\nsrc: {src_text:?}\ndest: {dest_text:?}\n\
         first: {output:?}\nsecond: {:?}",
        String::from_utf8_lossy(&buf2),
    );

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

#[test]
fn edit_dotted_key_order_preserved() {
    run_edit_ordered(
        "hello.world = \"a\"\ngoodbye = \"b\"\nhello.moon = \"c\"",
        "hello.world = \"a\"\ngoodbye = \"b\"\nhello.moon = \"c\"\nnitro = false",
    );
}

#[test]
fn edit_extending_explicit_table() {
    // Header [fruit.apple.texture] precedes body entries under [fruit] in source;
    // TOML requires body before subsections, so source order can't be preserved.
    run_edit_ordered(
        "[fruit.apple.texture]\nsmooth = true\n\n[fruit]\napple.color = \"red\"\napple.taste.sweet = true",
        "[fruit.apple.texture]\nsmooth = true\n\n[fruit]\napple.color = \"red\"\napple.taste.sweet = true\napple.size = \"large\"",
    );
}

#[test]
fn edit_dotted_root_rename() {
    run_edit_ordered("a.b.c=22", "ax.b.c = 22");
}

#[test]
fn edit_empty_table_header_child_rename() {
    run_edit_ordered("[a.b]", "[a.c]");
}

#[test]
fn edit_empty_table_header_root_rename() {
    run_edit_ordered("[x.b]", "[y.b]");
}

#[test]
fn edit_empty_tables_rename_first() {
    run_edit_ordered("[A]\n[B]", "[X]\n\n[B]");
}

#[test]
fn edit_empty_tables_rename_second() {
    run_edit_ordered("[A]\n[B]", "[A]\n\n[Y]");
}

#[test]
fn edit_table_insertion_order_preserved() {
    run_edit_ordered("[A]\n[C]", "[A]\n\n[B]\n\n[C]");
    run_edit_ordered("[A]\n[C]\n[E]", "[B]\n\n[A]\n[C]");
    run_edit_ordered("[A]\n[C]\n[E]", "[A]\n[C]\n\n[B]");
    run_edit_ordered("[A]\n[C]\n[E]", "[A]\n\n[B]\n\n[C]");
}

#[test]
fn edit_replace_full_dotted_key_table() {
    run_edit_ordered("A.b=0\nD=1\nA.c=2", "Z.y = 0\nD=1\nA.c=2");
}

#[test]
fn edit_multi_key_change() {
    run_edit_ordered("A.b.c=2\nD=0", "A.z.y = 2\nD=0");
}

#[test]
fn edit_headed_table_to_scalar() {
    run_edit_ordered("[A]\nb=0", "A = 1");
}

#[test]
fn edit_remove_dotted_replace_with_distinct_header() {
    run_edit_ordered("A.b=0", "[Z.y]");
}

#[test]
fn edit_dotted_header_with_separate_parent_leaf_change() {
    run_edit_ordered("[A]\n[A.b.c]", "[A]\n[A.b.z]");
}

#[test]
fn edit_double_header_to_scalar() {
    run_edit_ordered("[A.b]\nc=1", "A = 2");
}

#[test]
fn edit_header_to_dotted_key_scalar() {
    run_edit_ordered("[C]", "C.z = 2");
}

#[test]
fn edit_header_to_inline_table() {
    run_edit_ordered("[C]\nz = 1", "C = { z = 2 }");
}

#[test]
fn edit_subsection_to_scalar_inside_section() {
    run_edit_ordered("[a]\nx = 1\n[a.b]\ny = 2", "[a]\nx = 1\nb = 42");
}

#[test]
fn edit_subsection_to_dotted_inside_section() {
    run_edit_ordered("[a]\nx = 1\n[a.b]\ny = 2", "[a]\nx = 1\nb.z = 3");
}

#[test]
fn edit_deep_dotted_header_key_change() {
    run_edit_ordered("[A.b.c.d]\n[A]", "[A.b.x.y]\n[A]");
}

#[test]
fn edit_double_dotted_header_complete_replace() {
    run_edit_ordered("[7.2A.A]\n[M]", "[2.2A.A]\n[2]");
}

#[test]
fn edit_dotted_key_rename_with_sibling_header_section() {
    run_edit_ordered("A.b=0\n[A.c]", "A.z=0\n[A.c]");
}

#[test]
fn edit_new_root_entry_inserted_before_nested_section() {
    run_edit_ordered("\nA.b.c=0\n[A.d.e]\n", "\nA.b.c=0\nZ.y=0\n[A.d.e]\n");
}

#[test]
fn edit_new_dotted_sibling_inserted_before_nested_section_deep() {
    run_edit_ordered("A.b.c=0\n[A.b.d.e]", "A.z.y=0\nA.b.c=0\n[A.b.d.e]");
}

#[test]
fn edit_inline_table_dotted_depth_reduced() {
    run_edit_ordered("A={b.c.d=4}\n", "Z={y.x.w=4}\nA={b.c=4}\n");
}

#[test]
fn edit_section_child_replaced_by_scalar_in_dotted_intermediate() {
    run_edit_ordered("A.b.c=0\n[A.d]", "A.d=0\n[A.z]");
}

#[test]
fn edit_implicit_becomes_dotted_with_section_child_no_duplicate_entry() {
    run_edit_ordered("A.b=0\n[C.d]", "C.z=0\n[C.d]");
}

#[test]
fn edit_section_renamed_inside_dotted_intermediate() {
    run_edit_ordered("A.b=0\n[A.c]", "A.b=0\n[A.z]");
}

#[test]
fn edit_deep_dotted_key_value_change_inside_inline_table() {
    run_edit_ordered(
        "A={b=1,c.d.e=0,c.g=1,c.d.f=5}",
        "A={b=1,c.d.e=2,c.g=1,c.d.f=5}",
    );
}

#[test]
fn edit_header_to_nested_inline_table() {
    run_edit_ordered("[A.b.c]\n[A]\nd={c={}}", "[A.e.c]\n[A]\nb={c={}}");
}

#[test]
fn edit_leaf_becomes_implicit_table_via_section_header() {
    run_edit_ordered("A=[1.2]", "[A.z]");
}

#[test]
fn edit_section_leaf_in_implicit_intermediate_removed() {
    run_edit_ordered("[zA.0.4]\n[zA]\n0.A=1", "[zA.0.4]\n[zA]");
}

#[test]
fn edit_implicit_with_in_region_leaf_becomes_dotted_no_duplicate() {
    run_edit_ordered("[zA.0.4]\n[zA]\n0.0=1", "[zA]\n0.0=1");
}

#[test]
fn edit_deeply_nested_implicit_leaf_removed() {
    run_edit_ordered("[zA.0.0.8]\n[zA]\n0.0.4=0", "[zA.0.0.8]\n[zA]");
}

#[test]
fn edit_rename_nested_section_sibling_preserved() {
    run_edit_ordered("[A.b.c]\n[A]\nb.d=0", "[A.b.z]\n[A]\nb.d=0");
}

#[test]
fn edit_rename_deep_section_with_dotted_sibling() {
    run_edit_ordered(
        "[zA.0.4.RRfalse4]\n[zA]\n0.s=1",
        "[zA.0.4.RRflse4]\n[zA]\n0.s=1",
    );
}

#[test]
fn edit_implicit_to_dotted_no_in_region_leaves_no_dup() {
    // Replacing an implicit table (created by [zA.4.4]) with a dotted key
    // in the parent section must not emit the new entry twice.
    run_edit_ordered("[zA.4.4]\n[zA]\n3.28=12", "[zA]\n4.28=12");
}

#[test]
fn edit_new_implicit_with_dotted_leaves_not_lost() {
    // New section [zA.5.4] + dotted leaf 5.28=1 under [zA]: the leaf
    // must appear in the output.
    run_edit_ordered("[zA.4.4]\n[zA]\n4.28=1", "[zA.5.4]\n[zA]\n5.28=1");
}

#[test]
fn edit_cross_section_sub_key_replaces_header_entry() {
    run_edit_ordered("[zA.4.4]\n[zA]\n4.14=1", "[zA]\n4.4=1");
}

#[test]
fn edit_dotted_sub_key_scalar_to_header_no_dup() {
    // Dotted key's sub-key changes from scalar to header section: must
    // not produce both inline and header for the same key.
    run_edit_ordered("A.b=0\n[C.d]", "A.z=0\n[A.b]");
}

#[test]
fn edit_dotted_to_implicit_new_leaf_not_lost() {
    run_edit_ordered("[zA.4.4]\n[zA]\n5.28=126", "[zA.5.4]\n[zA]\n5.29=126");
}

#[test]
fn edit_implicit_leaf_diff_preserves_parent_prefix() {
    // Implicit key with nested dotted sub-entry that changes must keep
    // the full parent prefix (0.s.KEY, not just s.KEY).
    run_edit_ordered(
        "[zA.0.48.44]\n[zA]\n0.s.8ssl8wb8=1",
        "[zA.0.48.44]\n[zA]\n0.s.8ssl8wb9=1",
    );
}

#[test]
fn edit_new_section_with_implicit_dotted_body_not_lost() {
    // New [A] section with body dotted key b.x=1 through implicit b
    // (from [A.b.c]) must not drop the b.x=1 line.
    run_edit_ordered("", "[A.b.c]\n[A]\nb.x=1");
}

#[test]
fn edit_dotted_leaf_lost_on_second_patch_with_renamed_subsection() {
    run_edit_ordered("[a.b.c.x]\n[a]\nb.c.d=1", "[a.b.c.y]\n[a]\nb.c.d=1");
}

#[test]
fn edit_deep_implicit_leaf_key_rename() {
    // Key rename inside doubly-nested implicit table.
    run_edit_ordered("[a.b.c.x]\n[a]\nb.c.old=1", "[a.b.c.x]\n[a]\nb.c.new=1");
}

#[test]
fn edit_implicit_to_dotted_leaf_value_change_in_body() {
    // Nested implicit key loses section anchor but keeps body leaf;
    // the value change inside must not be dropped.
    run_edit_ordered("[a.b.b.x]\n[a]\nb.b.c=0", "[a.b.y.x]\n[a]\nb.b.c=1");
}

#[test]
fn edit_header_child_renamed_to_leaf_in_body() {
    run_edit_ordered("[a.b.c]\n[a]\nb.d=1", "[a]\nb.c=1");
}

#[test]
fn edit_doubly_nested_body_leaf_across_section_rename() {
    run_edit_ordered(
        "[zA.0.48.4]\n[zA]\n0.2.28=1",
        "[zA.0.4.4]\n[zA.0.4.5]\n[zA]\n0.4.28=1",
    );
}

#[test]
fn edit_dotted_scalar_replaced_by_section_no_inline_dup() {
    run_edit_ordered("a.b=0", "a.z=0\n[a.b.x]");
}

#[test]
fn edit_header_to_dotted_key_no_duplicate() {
    run_edit_ordered("[a.b.c]\ne=1", "a.b.z=1");
}

#[test]
fn edit_deleted_implicit_body_leaf_is_removed() {
    run_edit_ordered("[a.b.c.d]\n[a]\nb.c.e=1", "[a.b.f.d]\n[a]\ng.e=1");
}

#[test]
fn edit_new_implicit_with_nested_implicit_emits_body_leaves() {
    run_edit_ordered("[a.b.c.d]\n[a]\nb.c.e=1", "[a.f.c.d]\n[a]\nf.c.e=1");
}

#[test]
fn edit_header_replaced_by_dotted_no_duplicate() {
    run_edit_ordered("[a.b]\n[a]\nc.d=1", "[e.b]\n[a]\nb.d=1");
}

#[test]
fn edit_deep_implicit_header_child_becomes_leaf() {
    run_edit_ordered("[a.b.c.d.e]\n[a]\nb.c.d.f=1", "[a.g.c.d.e]\n[a]\nb.c.d.e=1");
}

#[test]
fn edit_deep_implicit_leaf_rename_to_header_key_in_body() {
    run_edit_ordered(
        "[zA.0.27z4.e.p1.4]\n[zA]\n0.27z4.e.p41=1",
        "[zA.027z4.e.p1.4]\n[zA]\n0.27z4.e.p1=1",
    );
}

#[test]
fn edit_implicit_to_dotted_transition_depth3_emits_new_leaf() {
    run_edit_ordered("[a.b.c.d.e]\n[a]\nb.c.d.f=1", "[a.h.c.d.e]\n[a]\nb.c.d.e=1");
}

#[test]
fn edit_implicit_body_leaf_new_key_when_implicit_child_renamed() {
    run_edit_ordered("[A.b.c.d]\n[A]\nb.e=1", "[A.b.z.y]\n[A]\nb.c=1");
}

#[test]
fn edit_scalar_to_dotted_under_implicit_at_depth1() {
    run_edit_ordered(
        "[zA.0.4]\n[zA]\n0.1.-=19\n0.2=0",
        "[zA.0.4]\n[zA]\n0.2.-=19",
    );
}

#[test]
fn edit_dotted_to_scalar_under_implicit_at_depth1() {
    run_edit_ordered("[zA.0.4]\n[zA]\n0.2.-=19", "[zA.0.4]\n[zA]\n0.2=0");
}

#[test]
fn edit_aot_header_to_inline_array() {
    run_edit_ordered("[[A]]\nb = []\n", "A = [{}]\n");
}

#[test]
fn edit_aot_sub_section_placed_with_correct_entry() {
    // AOT sub-section [[z.e]] must stay with z[0] when new entries are added.
    run_edit_ordered(
        "[[z]]\na = 1\n[z.s]\nb = 2\n[[z.e]]\nc = 3\n",
        "[[z]]\na = 1\n[[z.e]]\nc = 3\n[z.s]\nb = 2\n[[z]]\nd = 4\n",
    );
}

#[test]
fn edit_new_sub_section_under_first_aot_entry_with_reordered_siblings() {
    run_edit_ordered(
        "[[z]]\n[[z.a]]\nx = 1\n[[z.a]]\ny = 2\n[[z.b]]\nw = 3\n",
        "[[z]]\n[[z.b]]\nw = 3\n[[z.a]]\nx = 1\n[[z.a.c]]\nv = 4\n[[z.a]]\ny = 2\n",
    );
}

#[test]
fn edit_bom_preserved_at_start_new_dotted_entry_not_inline() {
    run_edit_ordered("\u{feff}A=0\nB.c.d=0", "\u{feff}Z=0\nA.y=0");
}

#[test]
fn edit_bom_preserved_when_new_entry_inserted_first() {
    run_edit_ordered("\u{feff}A=1\nB=0", "\u{feff}Z=1\nA=0");
}

#[test]
fn edit_audit_new_header_with_sub_header() {
    run_edit_ordered(
        "name = \"foo\"\n",
        "name = \"foo\"\n\n[a]\nx = 1\n\n[a.b]\ny = 2\n",
    );
}

#[test]
fn edit_audit_new_aot_with_nested_aot() {
    run_edit_ordered(
        "name = \"foo\"\n",
        "name = \"foo\"\n\n[[x]]\na = 1\n\n[[x.y]]\nb = 2\n",
    );
}

#[test]
fn edit_audit_inline_to_header_transition() {
    run_edit_ordered("deps = {foo = \"1.0\"}\n", "[deps]\nfoo = \"2.0\"\n");
}

#[test]
fn edit_dotted_key_inline_to_header_transition() {
    run_edit_ordered(
        "deps.second = {foo = \"1.0\"}\n",
        "[deps.second]\nfoo = \"2.0\"\n",
    );
}

#[test]
fn edit_dotted_key_inline_to_header_transition_slurp() {
    run_edit_ordered(
        "deps.second = {foo.bar = \"1.0\", baz = {key.naro = 32, lax=1}}\n",
        "[deps.second]\nbaz.key = 1\nbaz.lax = 1\n\n[deps.second.foo]\nbar = \"2.0\"\n",
    );
}

/// Parses source and modified texts, erases aggregate kinds on the dest,
/// reprojects from source into dest, normalizes, emits with config,
/// and asserts the exact output matches `expected`.
#[track_caller]
fn assert_reproject_exact(source: &str, modified: &str, expected: &str) {
    let arena = Arena::new();
    let src_root = parse(source, &arena).unwrap();

    let mut dest_root = parse(modified, &arena).unwrap();
    // Some tests are actually testing the dest style are being
    // preserved as well, when they are new
    // erase_kinds(&mut dest_root.table);

    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    let norm = dest_root.table.normalize();
    let config = EmitConfig {
        projected_source_text: source,
        projected_source_items: &items,
        reprojected_order: true,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(norm, &config, &mut buf);
    let output = String::from_utf8(buf).unwrap();

    assert_eq!(
        output, expected,
        "\n--- emitted ---\n{output}\n--- expected ---\n{expected}\nsource:\n{source}\nmodified:\n{modified}"
    );
}

#[test]
fn exact_add_key_preserves_comments() {
    let source = r#"
[package]
name = "foo"

# Some comment
# Another comment


[data]
value = 32
"#;
    let modified_and_dest = r#"
[package]
name = "foo"
added = true

# Some comment
# Another comment


[data]
value = 32
"#;
    assert_reproject_exact(source, modified_and_dest, modified_and_dest);
}

#[test]
fn exact_add_key_preserves_trailing_whitespace() {
    let source = r#"
[package]
name = "foo"

# Some comment
# Another comment


[data]
value = 32
"#;
    let modified_and_dest = r#"
[package]
name = "foo"
added = true

# Some comment
# Another comment


[data]
value = 32
"#;
    assert_reproject_exact(source, modified_and_dest, modified_and_dest);
}

#[test]
fn exact_identity_round_trip() {
    let source = r#"
[package]
name = "test"
version = "1.0"

# Dependencies
[dependencies]
serde = "1"
"#;
    assert_reproject_exact(source, source, source);
}

#[test]
fn exact_remove_key_preserves_comments() {
    let source = r#"
[package]
name = "foo"
version = "1.0"

# Build settings
[build]
target = "x86"
#trailing comment
"#;
    let modified = r#"
[package]
name = "foo"

# Build settings
[build]
target = "x86"
#trailing comment
"#;
    assert_reproject_exact(source, modified, modified);
}

#[test]
fn exact_multiple_comment_blocks() {
    let source = r#"
# File header
# Second line

[a]
x = 1

# Between a and b

[b]
y = 2

# Trailing comment
"#;
    let modified = r#"
# File header
# Second line

[a]
x = 1
z = 3

# Between a and b

[b]
y = 2

# Trailing comment
"#;
    assert_reproject_exact(source, modified, modified);
}

#[test]
fn exact_add_section_preserves_comments() {
    let source = r#"
[package]
name = "foo"

# End of file comment
"#;
    let modified = r#"
[package]
name = "foo"

[new_section]
key = true

# End of file comment
"#;
    assert_reproject_exact(source, modified, modified);
}

#[test]
fn exact_aot_with_comments() {
    let source = r#"
# Server list

[[servers]]
name = "alpha"

# Second server
[[servers]]
name = "beta"
"#;
    let modified = r#"
# Server list

[[servers]]
name = "alpha"
port = 8080

# Second server
[[servers]]
name = "beta"
"#;
    assert_reproject_exact(source, modified, modified);
}

#[test]
fn test_body_reorder_source_order() {
    // Body-only entries should be reordered by source position when reprojected_order=true.
    run_edit_ordered("a = 0\nb = 0\n", "b = 0\na = 0\n");
}

#[test]
fn test_dotted_unmatched_sort_position() {
    // Unprojected entries inside a dotted container should sort near
    // the container's source position, not at position 0.
    run_edit_ordered("a = []\nb = []\nd = []\n", "d.x = []\na = []\nb = []\n");
}

#[test]
fn test_type_mismatch_body_to_header_ordering() {
    // When source b is a body entry but dest b is a header table,
    // reprojection should set b to Dotted so it emits as a body entry
    // and preserves source ordering (a < b < c).
    run_edit_ordered("a = 0\nb = 0\nc = 0\n", "a = 0\nc = 0\n[b]\nx = 0\n");
}

#[test]
fn test_type_mismatch_empty_table_stays_inline() {
    // When src has a body entry and dest has an empty table {},
    // it should stay as inline (body entry), not become Header.
    run_edit_ordered("a = []\nb = []\nc = []\n", "a = {}\nb = []\nc = []\n");
}

#[test]
fn test_dotted_container_plus_body_reorder() {
    // A dotted container and a body entry should be reordered by source
    // position. The dotted container's children become body segments after
    // flattening.
    run_edit_ordered("a = []\nb = []\n", "b = []\n[a]\na = []\n");
}

#[test]
fn test_type_mismatch_aot_demoted_to_inline() {
    // When src has a body entry (table) and dest has an AOT, the AOT
    // should be demoted to Inline so it can be emitted as a body entry,
    // preserving source ordering.
    run_edit_ordered("e = {}\na = 0\n", "a = 0\n[[e]]\na = 0\n");
}

#[test]
fn test_empty_table_mismatch_ordering() {
    // Empty frozen table in dest with array in src should stay Inline,
    // preserving source ordering.
    run_edit_ordered("b.a = []\na = []\nc = []\n", "a = {}\nb = []\nc = []\n");
}

#[test]
fn test_dotted_kind_on_empty_dest_becomes_inline() {
    // When src is Dotted and dest is an empty table, kind Dotted on an
    // empty table would be promoted to Header by normalization. Use
    // Inline instead so it stays a body entry.
    run_edit_ordered("a.x = []\nb = []\n", "a = {}\nb = []\n");
}

#[test]
fn test_keep_dotted_when_src_is_header() {
    // When dest uses dotted keys but src uses headers, keep the dest
    // kind (Dotted) to preserve body-level ordering.
    run_edit_ordered("[a]\na = []\n[b]\na = []\n", "a.a = []\nb = []\n");
}

#[test]
fn test_keep_inline_array_when_src_is_aot() {
    // When dest has an inline array and src has AOT, keep the Inline
    // kind so it stays a body entry, preserving ordering.
    run_edit_ordered("[[d]]\na = []\n[[e]]\na = []\n", "d = [{}]\ne = []\n");
}

#[test]
fn test_empty_frozen_inherits_header_kind_from_source() {
    // When source has [a] as a header after [b] and dest converts a to an
    // empty inline table {}, reprojection should restore Header kind so a
    // emits as an empty [a] section, preserving source ordering (b before a).
    run_edit_ordered("[b]\nx = 0\n[a]\nx = 0\n", "a = {}\n[b]\nx = 0\n");
}

#[test]
fn test_aot_demoted_when_sibling_stuck_as_body() {
    // When source has [[b]] and [[d]] as AOTs but dest converts d to an
    // empty inline array d=[], the remaining [[b]] must also be demoted
    // to Inline so all formerly-subsection entries stay body-level and
    // source ordering is preserved (b at src=0 before d at src=14).
    run_edit_ordered("[[b]]\nx = 0\n[[d]]\nx = 0\n", "d = []\n[[b]]\nx = 0\n");
}

#[test]
fn test_header_demoted_when_type_mismatch_sibling() {
    // When source has [z] and [00] as headers but dest converts 00 to
    // an array (type mismatch), [z] must be demoted to Inline so both
    // are body-level and source ordering is preserved (z before 00).
    run_edit_ordered("[z]\na = 0\n[a]\na = 0\n", "a = []\n[z]\na = 0\n");
}

#[test]
fn test_aot_to_empty_table_is_stuck() {
    // When source has [[a]] (AOT) but dest has a={} (empty FROZEN table),
    // it's a type mismatch stuck at body level. Sibling [a-b] must not
    // be promoted to Header or it would sort after the stuck entry.
    run_edit_ordered("[a-b]\na = 0\n[[a]]\na = 0\n", "a-b = {}\na = {}\n");
}

#[test]
fn test_inline_table_partial_reorder_bail() {
    // Multiline inline table with entries reordered in dest. Partial
    // emit must bail when projected elements aren't in source order,
    // falling through to format_inline_table which respects ordering.
    run_edit_ordered("b = {\na = 0,\nb = 0,\n}\n", "[b]\nb = 0\na = 0\nc = 0\n");
}

#[test]
fn test_order_violation_aot_body_before_header() {
    // Source has a before b. Dest converts b to a single-element AOT
    // (normalized to inline array) while a becomes implicit with a
    // header child [[a.d]]. The inline b is body-level and gets emitted
    // before the subsection [[a.d]], violating source order of a before b.
    run_edit_ordered("a = 0\nb = 0\n", "[a]\n[[a.d]]\n[[b]]\n");
}

#[test]
fn test_promotable_entry_not_promoted_when_stuck_exists() {
    // When a stuck entry exists (a: Header→scalar type mismatch) and a
    // promotable empty table (b: Header→{}) has a source position after
    // another subsection (x: Header→Header), the promotable entry must
    // be promoted to Header so it sorts with x by source position,
    // preserving source order (x before b).
    run_edit_ordered("[a]\nk=0\n[x]\nk=0\n[b]\nk=0\n", "a=0\nb={}\n[x]\nk=0\n");
}

#[test]
fn test_inline_table_reorder_type_mismatch_entry() {
    // When a frozen table entry changes type (array→string), the value
    // projection is cleared but the key span still holds the source
    // position. The sort in partial inline emit must use the key span
    // for ordering so type-mismatched entries sort correctly.
    run_edit_ordered("a = {\nb = 0,\nc = 0,\n}\n", "a.c = 0\na.b = \"\"\n");
}

#[test]
fn test_inline_table_dotted_child_inherits_parent_position() {
    // When a frozen table entry changes type (array→table with dotted
    // children), the dotted subtree should sort by the parent's source
    // key position, not by the preceding sibling's position.
    run_edit_ordered("e = {\na = 0,\nb = 0,\n}\n", "[e]\nb = 0\n[e.a]\na = 0\n");
}

#[test]
fn test_dotted_parent_with_header_child_stays_body() {
    // When source c is DOTTED (body-level) and dest adds a HEADER
    // subsection child, the parent must stay body-level so it doesn't
    // jump to the subsection group and violate source ordering.
    run_edit_ordered("c.a=0\nd=0\ne=0\n", "[c.b]\na=0\n[d]\na=0\n[e]\na=0\n");
}

#[test]
fn test_empty_frozen_not_promoted_in_body_parent() {
    // When x is DOTTED (body-level) and x.a is an empty frozen table
    // matched to a source HEADER, promotion to Header inside the body
    // parent creates a subsection that sorts after root body items,
    // violating source order (x.a at src=3 before a at src=7).
    run_edit_ordered("[x.a]\n[a]\n", "x.a={}\nx.b=0\na=0\n");
}

#[test]
fn dotted_inline_table_split() {
    let source = r#"
a = {
    b.c.d = 4,
    s = 4, # comment
    b.f = [1, 2],
}
"#;
    let modified = r#"
a = {
    b.c.d = 4,
    s = 4, # comment
    b.f = [1, 2, 3],
}
"#;
    assert_reproject_exact(source, modified, modified);
}

#[test]
fn dotted_inline_table_split_nx() {
    let source = r#"
a = {
    b.c.d = { x = 1, z = 3 },
    s = 4, # comment
    b.f = [1, 2],
}
"#;
    let modified = r#"
a = {
    b.c.d.x = 1,
    b.c.d.y.y = 2,
    b.c.d.z = 3,
    s = 4, # comment
    b.f = [1, 2, 3],
}
"#;
    let expected = r#"
a = {
    b.c.d = { x = 1, y.y = 2, z = 3 },
    s = 4, # comment
    b.f = [1, 2, 3],
}
"#;
    assert_reproject_exact(source, modified, expected);
}

#[test]
fn dotted_inline_table_split_inline() {
    let source = r#"
a = {
    b.c.d = { x = 1, z = 3 },
    s = 4, # comment
    b.f = [1, 2],
}
"#;
    let modified = r#"
a = {
    b.c.d.x = 1,
    b.c.d.y = { y = 2 },
    b.c.d.z = 3,
    s = 4, # comment
    b.f = [1, 2, 3],
}
"#;
    let expected = r#"
a = {
    b.c.d = { x = 1, y = { y = 2 }, z = 3 },
    s = 4, # comment
    b.f = [1, 2, 3],
}
"#;
    assert_reproject_exact(source, modified, expected);
}

#[test]
fn interleaved_table_headers() {
    let source = r#"
[a.b]
x = 0
[a.c]
y = 1
[a.b.d]
z = 2
"#;
    assert_reproject_exact(source, source, source);
}

#[test]
fn indented_tables() {
    let source = r#"
[a]
  [a.b]
  x = 0
  [a.c]
  y = 0
"#;
    assert_reproject_exact(source, source, source);
}

#[test]
fn indented_tables_interleaved() {
    let source = r#"
[a]
  [a.b]
  x = 0
  [a.c]
  y = 1
  [a.b.d]
  z.w = 2
"#;
    assert_reproject_exact(source, source, source);
}

#[test]
fn indented_tables_tab() {
    let source = "\n[a]\n\t[a.b]\n\tx = 0\n\t[a.c]\n\ty = 0\n";
    assert_reproject_exact(source, source, source);
}

#[test]
fn implict_sub_table_comment() {
    let source = r#"
[a.b.c]
v = 1
[a.b]
v = 1
"#;
    assert_reproject_exact(source, source, source);
}

#[test]
fn ignore_source_order_skips_reordering() {
    // Source has keys in order: a, b, c.
    // Dest has keys reversed: c, b, a — with ignore_source_order set.
    // With reprojected_order=true, normally the emitter sorts by source
    // position (a, b, c). The flag should prevent that, preserving c, b, a.
    let src_text = "c = 3\nb = 2\na = 1\n";

    let arena = Arena::new();
    let src_root = parse(src_text, &arena).unwrap();

    // Build dest with reversed key order.
    let mut dest_root = parse("a = 1\nb = 2\nc = 3\n", &arena).unwrap();

    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    // Set the flag on the root table.
    dest_root.table.set_ignore_source_order();

    let norm = dest_root.table.normalize();
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
    let src_root = parse(src_text, &arena).unwrap();
    let mut dest_root = parse(src_text, &arena).unwrap();

    // Set hint flag BEFORE reprojection.
    dest_root.table.set_ignore_source_order();
    assert!(dest_root.table.ignore_source_order());

    let mut items = Vec::new();
    reproject(&src_root, &mut dest_root.table, &mut items);

    // The flag must survive reprojection (hints_preserve_mask fix).
    assert!(
        dest_root.table.ignore_source_order(),
        "ignore_source_order hint was destroyed by reprojection"
    );
}

#[test]
fn ignore_source_style_uses_dest_structure() {
    // Source uses header sections.
    let src_text = "[package]\nname = \"test\"\nversion = \"1.0\"\n";
    let arena = Arena::new();
    let src_root = parse(src_text, &arena).unwrap();

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
    reproject(&src_root, &mut dest, &mut items);

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
    let src_root = parse(src_text, &arena).unwrap();

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
    reproject(&src_root, &mut dest, &mut items);

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
// todo should but text in Context.
fn to_toml(reference: &Root<'_>, text: &str, mut table: Table<'_>) -> String {
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
    let src_root = parse(src_text, &arena).unwrap();
    // let expected_preserve_style = parse(expected_ignored_source_style_text, &arena).unwrap();
    let expected_ignore_style = parse(expected_ignored_source_style_text, &arena).unwrap();

    let output = to_toml(
        &src_root,
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

    let output = to_toml(&src_root, src_text, table);
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
    let src_root = parse(src_text, &arena).unwrap();
    let mut copy = src_root.table().clone_in(&arena);
    let dep_table = copy
        .get_mut("dependencies")
        .unwrap()
        .as_table_mut()
        .unwrap();

    dep_table
        .entries_mut()
        .sort_unstable_by_key(|(key, _)| key.name);

    dep_table.set_ignore_source_order();

    let output = to_toml(&src_root, src_text, copy.clone_in(&arena));
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
    let output = to_toml(&src_root, src_text, copy);
    if output != sorted_by_style_discarded {
        println!("=== Expected ===\n {}", sorted_by_style_discarded);
        println!("=== Got ===\n {}", output);
        panic!("TOML didn't match expected result after serialization:");
    }
}

#[test]
fn array_removal() {
    let source = r#"
[a]
c = 1
[[a.b]]
id = 1 # First entry
[[a.b]]
id = 2 # Item to be removed
[[a.b]]
id = 3 # Last Item to be kep
"#;
    let modified = r#"
[a]
c = 1
[[a.b]]
id = 1 # First entry
[[a.b]]
id = 3 # Last Item to be kep
"#;
    assert_reproject_exact(source, modified, modified);
}

#[test]
fn array_reordering() {
    let source = r#"
[a]
c = 1
[[a.b]]
id = 1 # First entry
values = [1,2,3]
[[a.b]]
id = 2 # Item to be removed
[[a.b]]
id = 3 # Last Item to be kep
"#;
    let modified = r#"
[a]
c = 1
[[a.b]]
id = 2 # Item to be removed
[[a.b]]
id = 1 # First entry
values = [1,2,3]
[[a.b]]
id = 3 # Last Item to be kep
"#;
    assert_reproject_exact(source, modified, modified);
}

#[test]
fn partial_array_reordering_and_modification() {
    let source = r#"
[a]
c = 1
[[a.b]]
id = 1 # First entry
values = [1,2,3]
[[a.b]]
id = 2 # Item to be removed
[[a.b]]
id = 3 # Last Item to be kep
"#;
    let modified = r#"
[a]
c = 1
[[a.b]]
id = 2 # Item to be removed
[[a.b]]
id = 1 # First entry
values = [1,2]
[[a.b]]
id = 3 # Last Item to be kep
"#;
    let expected = r#"
[a]
c = 1
[[a.b]]
id = 2 # Item to be removed
[[a.b]]
id = 1 # First entry
values = [1, 2]
[[a.b]]
id = 3 # Last Item to be kep
"#;
    assert_reproject_exact(source, modified, expected);
}

#[test]
fn reordered_array_preserves_outer_comments() {
    let source = r#"
[a]
c = 1

# Preserve this
[[a.b]]
id = 1 # First entry

# Preserve this too
[[a.b]]
id = 2 # Item to be removed
"#;
    let modified = r#"
[a]
c = 1

# Preserve this too
[[a.b]]
id = 2 # Item to be removed

# Preserve this
[[a.b]]
id = 1 # First entry
"#;
    assert_reproject_exact(source, modified, modified);
}

#[test]
fn reordered_inline_array_preserves_comments_scalars() {
    let source = r#"
a = [
    1, # Alpha
    2, # Beta
    3, # Canary
]
"#;
    let modified = r#"
a = [
    2, # Beta
    1, # Alpha
    3, # Canary
]
"#;
    assert_reproject_exact(source, modified, modified);
}

#[test]
fn reordered_inline_array_preserves_comments_complex() {
    let source = r#"
a = [
    { b.c = 4 }, # Alpha
    { x = 1, y = 2 }, # Beta
    [1, 2, 3], # Canary
]
"#;
    let modified = r#"
a = [
    [1, 2, 3], # Canary
    { y = 2, x = 1 }, # Beta
    { b.c = 4 }, # Alpha
]
"#;
    let expected = r#"
a = [
    [1, 2, 3], # Canary
    { x = 1, y = 2 }, # Beta
    { b.c = 4 }, # Alpha
]
"#;
    assert_reproject_exact(source, modified, expected);
}

#[test]
fn normalization_preserves_introduced_inline_array() {
    let source = r#"
a = 1
b = 1
"#;
    let modified = r#"
a = { x = 1 }
b = 1
"#;
    assert_reproject_exact(source, modified, modified);
}
