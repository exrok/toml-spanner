use toml_spanner::{Arena, Formatting, Item, Table};

#[derive(Clone, Debug)]
enum Expected {
    Integer(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Array(Vec<Expected>),
    Table(Vec<(String, Expected)>),
}

fn verify_table(actual: &Table<'_>, expected: &[(String, Expected)], path: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{path}: table length mismatch: actual={}, expected={}",
        actual.len(),
        expected.len(),
    );
    for (key, exp_val) in expected {
        let child = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        let item = actual
            .get(key)
            .unwrap_or_else(|| panic!("{child}: missing key"));
        verify_item(item, exp_val, &child);
    }
}

fn verify_item(actual: &Item<'_>, expected: &Expected, path: &str) {
    match expected {
        Expected::Integer(exp) => {
            let v = actual
                .as_i64()
                .unwrap_or_else(|| panic!("{path}: expected integer, got {}", actual.type_str()));
            assert_eq!(v, *exp, "{path}: integer mismatch");
        }
        Expected::Float(exp) => {
            let v = actual
                .as_f64()
                .unwrap_or_else(|| panic!("{path}: expected float, got {}", actual.type_str()));
            if exp.is_nan() {
                assert!(v.is_nan(), "{path}: expected NaN, got {v}");
            } else {
                assert_eq!(v, *exp, "{path}: float mismatch");
            }
        }
        Expected::Bool(exp) => {
            let v = actual
                .as_bool()
                .unwrap_or_else(|| panic!("{path}: expected bool, got {}", actual.type_str()));
            assert_eq!(v, *exp, "{path}: bool mismatch");
        }
        Expected::Str(exp) => {
            let v = actual
                .as_str()
                .unwrap_or_else(|| panic!("{path}: expected string, got {}", actual.type_str()));
            assert_eq!(v, exp.as_str(), "{path}: string mismatch");
        }
        Expected::Array(exp_items) => {
            let arr = actual
                .as_array()
                .unwrap_or_else(|| panic!("{path}: expected array, got {}", actual.type_str()));
            assert_eq!(
                arr.len(),
                exp_items.len(),
                "{path}: array length mismatch: actual={}, expected={}",
                arr.len(),
                exp_items.len(),
            );
            for (i, exp_item) in exp_items.iter().enumerate() {
                let child = format!("{path}[{i}]");
                verify_item(arr.get(i).unwrap(), exp_item, &child);
            }
        }
        Expected::Table(exp_entries) => {
            let tbl = actual
                .as_table()
                .unwrap_or_else(|| panic!("{path}: expected table, got {}", actual.type_str()));
            verify_table(tbl, exp_entries, path);
        }
    }
}

fn write_fmt(s: &mut String, args: std::fmt::Arguments<'_>) {
    use std::fmt::Write;
    s.write_fmt(args).unwrap();
}

fn gen_simple_string(rng: &mut fastrand::Rng) -> String {
    let len = rng.usize(1..=25);
    let pool = b"abcdefghijklmnopqrstuvwxyz0123456789";
    (0..len)
        .map(|_| pool[rng.usize(0..pool.len())] as char)
        .collect()
}

// 20 distinct base prefixes — each index maps to a unique base, guaranteeing
// no collisions within a single table scope.
const KEY_BASES: &[&str] = &[
    "name", "cfg", "path", "src", "opt", "val", "item", "data", "meta", "spec", "port", "host",
    "mode", "tag", "kind", "level", "scope", "fmt", "key", "addr",
];

const UNICODE_CHARS: &[char] = &[
    '\u{00E9}',
    '\u{00FC}',
    '\u{03B1}',
    '\u{03B2}',
    '\u{4E16}',
    '\u{754C}',
    '\u{1F642}',
];

fn key_base(idx: usize) -> String {
    let prefix = KEY_BASES[idx % KEY_BASES.len()];
    if idx < KEY_BASES.len() {
        prefix.to_string()
    } else {
        format!("{prefix}{}", idx / KEY_BASES.len())
    }
}

fn pick_unicode(rng: &mut fastrand::Rng) -> char {
    UNICODE_CHARS[rng.usize(0..UNICODE_CHARS.len())]
}

fn is_bare_key(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn can_use_single_line_literal(s: &str) -> bool {
    s.chars().all(|ch| match ch {
        '\'' | '\n' | '\r' => false,
        '\t' => true,
        c if c.is_control() => false,
        _ => true,
    })
}

fn can_use_multiline_literal(s: &str) -> bool {
    if !s.contains('\n') {
        return false;
    }

    let mut consecutive_quotes = 0u8;
    for ch in s.chars() {
        match ch {
            '\'' => {
                consecutive_quotes += 1;
                if consecutive_quotes == 3 {
                    return false;
                }
            }
            '\n' | '\t' => consecutive_quotes = 0,
            '\r' => return false,
            c if c.is_control() => return false,
            _ => consecutive_quotes = 0,
        }
    }
    true
}

fn push_codepoint_escape(rng: &mut fastrand::Rng, out: &mut String, value: u32) {
    match rng.u8(0..3) {
        0 if value <= 0xFF => write_fmt(out, format_args!("\\x{value:02X}")),
        1 if value <= 0xFFFF => write_fmt(out, format_args!("\\u{value:04X}")),
        _ => write_fmt(out, format_args!("\\U{value:08X}")),
    }
}

fn push_basic_char(
    rng: &mut fastrand::Rng,
    out: &mut String,
    ch: char,
    allow_multiline_raw_newlines: bool,
) {
    match ch {
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '\n' if allow_multiline_raw_newlines => out.push('\n'),
        '\n' => out.push_str("\\n"),
        '\t' => {
            if rng.u8(0..4) == 0 {
                out.push('\t');
            } else {
                out.push_str("\\t");
            }
        }
        '\r' => out.push_str("\\r"),
        '\u{08}' => {
            if rng.u8(0..4) == 0 {
                out.push_str("\\b");
            } else {
                push_codepoint_escape(rng, out, ch as u32);
            }
        }
        '\u{0C}' => {
            if rng.u8(0..4) == 0 {
                out.push_str("\\f");
            } else {
                push_codepoint_escape(rng, out, ch as u32);
            }
        }
        '\u{1B}' => {
            if rng.u8(0..4) == 0 {
                out.push_str("\\e");
            } else {
                push_codepoint_escape(rng, out, ch as u32);
            }
        }
        c if c.is_control() => push_codepoint_escape(rng, out, c as u32),
        c if !c.is_ascii() && rng.u8(0..4) == 0 => push_codepoint_escape(rng, out, c as u32),
        c => out.push(c),
    }
}

fn render_basic_single_line(rng: &mut fastrand::Rng, s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        push_basic_char(rng, &mut out, ch, false);
    }
    out.push('"');
    out
}

fn render_basic_multiline(rng: &mut fastrand::Rng, s: &str) -> String {
    let mut out = String::from("\"\"\"");
    for ch in s.chars() {
        push_basic_char(rng, &mut out, ch, true);
    }
    out.push_str("\"\"\"");
    out
}

fn render_literal_single_line(s: &str) -> String {
    format!("'{s}'")
}

fn render_literal_multiline(s: &str) -> String {
    format!("'''{s}'''")
}

fn render_key_expr(rng: &mut fastrand::Rng, key: &str) -> String {
    if is_bare_key(key) && rng.u8(0..4) == 0 {
        key.to_string()
    } else if can_use_single_line_literal(key) && rng.bool() {
        render_literal_single_line(key)
    } else {
        render_basic_single_line(rng, key)
    }
}

fn render_string_expr(rng: &mut fastrand::Rng, s: &str) -> String {
    if s.contains('\n') {
        match rng.u8(0..5) {
            0 if can_use_multiline_literal(s) => render_literal_multiline(s),
            1 | 2 => render_basic_multiline(rng, s),
            _ => render_basic_single_line(rng, s),
        }
    } else if can_use_single_line_literal(s) && rng.u8(0..3) == 0 {
        render_literal_single_line(s)
    } else {
        render_basic_single_line(rng, s)
    }
}

/// Generate a key with random source quoting and escape style.
/// The `idx` parameter guarantees uniqueness within a table scope.
fn gen_key(rng: &mut fastrand::Rng, idx: usize) -> (String, String) {
    let base = key_base(idx);
    let key = match rng.u8(0..16) {
        0 => base,
        1 => format!("{base}_z"),
        2 => format!("{base}-q"),
        3 => format!("{base} expr"),
        4 => format!("{base}.dot"),
        5 => format!("{base}:port"),
        6 => format!("{base}/path"),
        7 => format!("{base}#{idx}"),
        8 => format!("{base}\"q"),
        9 => format!("{base}'q"),
        10 => format!("{base}\\q"),
        11 => format!("{base}\tx"),
        12 => format!("{base}\nw"),
        13 => format!("{base}\u{1B}e"),
        14 => format!("{base}{}", pick_unicode(rng)),
        _ => format!("{base} {}", pick_unicode(rng)),
    };
    let encoded = render_key_expr(rng, &key);
    (encoded, key)
}

/// Re-encode an existing key string into TOML representation.
fn re_quote_key(rng: &mut fastrand::Rng, key_str: &str) -> String {
    render_key_expr(rng, key_str)
}

fn gen_string_actual(rng: &mut fastrand::Rng) -> String {
    let a = gen_simple_string(rng);
    let b = gen_simple_string(rng);
    match rng.u8(0..18) {
        0 => a,
        1 => format!("{a} {b}"),
        2 => format!("{a}.{b}/{}", gen_simple_string(rng)),
        3 => format!("{a}\"{b}\"\\{}", gen_simple_string(rng)),
        4 => format!("{a}'{b}'"),
        5 => format!("{a}\t{b}"),
        6 => format!("{a}\n{b}"),
        7 => format!("{a}\n{b}\n{}", gen_simple_string(rng)),
        8 => format!("{a}{}{}", pick_unicode(rng), b),
        9 => format!("{a}\n{}\\{}", gen_simple_string(rng), b),
        10 => format!("{a}\n{}\"{}\"", gen_simple_string(rng), b),
        11 => format!("{a}\u{1B}{b}"),
        12 => format!("{a}\u{08}{b}"),
        13 => format!("{a}\u{0C}{b}"),
        14 => format!("{a}:{}#{}", b, gen_simple_string(rng)),
        15 => String::new(),
        16 => format!("{} {a} {}", pick_unicode(rng), pick_unicode(rng)),
        _ => format!("{a}\n{b}'{}", gen_simple_string(rng)),
    }
}

fn gen_scalar(rng: &mut fastrand::Rng) -> (String, Expected) {
    match rng.u8(0..12) {
        0 => {
            let v = rng.i64(i64::MIN / 2..=i64::MAX / 2);
            (v.to_string(), Expected::Integer(v))
        }
        1 => {
            let v = gen_float(rng);
            (format_float(v), Expected::Float(v))
        }
        2 => {
            let v = rng.bool();
            (v.to_string(), Expected::Bool(v))
        }
        _ => {
            let s = gen_string_actual(rng);
            (render_string_expr(rng, &s), Expected::Str(s))
        }
    }
}

fn gen_float(rng: &mut fastrand::Rng) -> f64 {
    let int_part = rng.i32(-999..=999);
    let frac = rng.u32(0..=99999);
    let s = format!("{int_part}.{frac:05}");
    s.parse::<f64>().unwrap()
}

fn format_float(v: f64) -> String {
    let s = format!("{v}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
    }
}

/// Generate a value that may be a scalar, inline table, or inline array
/// depending on remaining `depth`. At depth 0, always returns a scalar.
fn gen_value(rng: &mut fastrand::Rng, depth: usize) -> (String, Expected) {
    if depth == 0 {
        return gen_scalar(rng);
    }
    match rng.u8(0..8) {
        0 => gen_inline_table_val(rng, depth - 1),
        1 => gen_inline_array_val(rng, depth - 1),
        _ => gen_scalar(rng),
    }
}

fn gen_inline_table_val(rng: &mut fastrand::Rng, depth: usize) -> (String, Expected) {
    let n = rng.usize(1..=6);
    let mut parts_toml = Vec::new();
    let mut parts_exp = Vec::new();
    for i in 0..n {
        let (k_toml, k_str) = gen_key(rng, i);
        let (v_toml, v_exp) = gen_value(rng, depth);
        parts_toml.push(format!("{k_toml} = {v_toml}"));
        parts_exp.push((k_str, v_exp));
    }
    (
        format!("{{{}}}", parts_toml.join(", ")),
        Expected::Table(parts_exp),
    )
}

fn gen_inline_array_val(rng: &mut fastrand::Rng, depth: usize) -> (String, Expected) {
    let n = rng.usize(0..=8);
    let mut parts_toml = Vec::new();
    let mut parts_exp = Vec::new();
    for _ in 0..n {
        let (v_toml, v_exp) = gen_value(rng, depth);
        parts_toml.push(v_toml);
        parts_exp.push(v_exp);
    }
    (
        format!("[{}]", parts_toml.join(", ")),
        Expected::Array(parts_exp),
    )
}

fn append_quote_stress_cases(
    rng: &mut fastrand::Rng,
    toml: &mut String,
    root_exp: &mut Vec<(String, Expected)>,
    root_kidx: &mut usize,
) {
    // Literal key source that auto-emit will rewrite as a basic key, paired
    // with a basic string that auto-emit prefers as a literal string.
    let key1 = format!("{} expr", key_base(*root_kidx));
    *root_kidx += 1;
    let val1 = format!(
        "{}\"{}\"\\{}",
        gen_simple_string(rng),
        gen_simple_string(rng),
        gen_simple_string(rng),
    );
    write_fmt(
        toml,
        format_args!(
            "{} = {}\n",
            render_literal_single_line(&key1),
            render_basic_single_line(rng, &val1),
        ),
    );
    root_exp.push((key1, Expected::Str(val1)));

    // Basic string source with embedded newlines, quotes, and backslashes that
    // auto-emit will often normalize into a multiline literal at non-inline sites.
    let key2 = format!("{}-ml", key_base(*root_kidx));
    *root_kidx += 1;
    let val2 = format!(
        "{}\n{}\"{}\"\\{}",
        gen_simple_string(rng),
        gen_simple_string(rng),
        gen_simple_string(rng),
        gen_simple_string(rng),
    );
    let val2_toml = if rng.bool() {
        render_basic_multiline(rng, &val2)
    } else {
        render_basic_single_line(rng, &val2)
    };
    write_fmt(
        toml,
        format_args!("{} = {val2_toml}\n", render_key_expr(rng, &key2)),
    );
    root_exp.push((key2, Expected::Str(val2)));
}

/// Generate a complete TOML document mixing root key-value pairs (scalars,
/// dotted keys, inline tables, inline arrays), table header sections
/// (simple and nested), array-of-tables (simple and nested with child
/// subtables), and the super-table-afterwards pattern.
fn gen_document(rng: &mut fastrand::Rng) -> (String, Vec<(String, Expected)>) {
    let mut toml = String::new();
    let mut root_exp: Vec<(String, Expected)> = Vec::new();
    let mut root_kidx = 0usize;

    append_quote_stress_cases(rng, &mut toml, &mut root_exp, &mut root_kidx);

    // Mix of plain scalars, dotted keys, inline tables, inline arrays.
    let n_root = rng.usize(2..=10);
    for _ in 0..n_root {
        let (key_toml, key_str) = gen_key(rng, root_kidx);
        root_kidx += 1;

        match rng.u8(0..8) {
            0..=1 => {
                // Dotted key: key.sub = value
                let (sub_toml, sub_str) = gen_key(rng, 0);
                let (val_toml, val_exp) = gen_value(rng, 1);
                write_fmt(
                    &mut toml,
                    format_args!("{key_toml}.{sub_toml} = {val_toml}\n"),
                );
                root_exp.push((key_str, Expected::Table(vec![(sub_str, val_exp)])));
            }
            2 => {
                let (val_toml, val_exp) = gen_inline_table_val(rng, 2);
                write_fmt(&mut toml, format_args!("{key_toml} = {val_toml}\n"));
                root_exp.push((key_str, val_exp));
            }
            3 => {
                let (val_toml, val_exp) = gen_inline_array_val(rng, 1);
                write_fmt(&mut toml, format_args!("{key_toml} = {val_toml}\n"));
                root_exp.push((key_str, val_exp));
            }
            _ => {
                let (val_toml, val_exp) = gen_scalar(rng);
                write_fmt(&mut toml, format_args!("{key_toml} = {val_toml}\n"));
                root_exp.push((key_str, val_exp));
            }
        }
    }

    let n_sections = rng.usize(0..=4);
    for _ in 0..n_sections {
        let depth = rng.usize(1..=3);
        let mut path_keys: Vec<(String, String)> = Vec::new();
        for d in 0..depth {
            if d == 0 {
                path_keys.push(gen_key(rng, root_kidx));
                root_kidx += 1;
            } else {
                path_keys.push(gen_key(rng, d));
            }
        }
        let header: Vec<&str> = path_keys.iter().map(|(t, _)| t.as_str()).collect();
        write_fmt(&mut toml, format_args!("[{}]\n", header.join(".")));

        let mut inner_exp = Vec::new();
        let mut inner_kidx = 0;
        gen_section_kvs(rng, &mut toml, &mut inner_exp, &mut inner_kidx);

        let mut current = Expected::Table(inner_exp);
        for d in (1..depth).rev() {
            current = Expected::Table(vec![(path_keys[d].1.clone(), current)]);
        }
        root_exp.push((path_keys[0].1.clone(), current));
    }

    let n_aots = rng.usize(0..=2);
    for _ in 0..n_aots {
        let depth = rng.usize(1..=3);
        let mut path_keys: Vec<(String, String)> = Vec::new();
        for d in 0..depth {
            if d == 0 {
                path_keys.push(gen_key(rng, root_kidx));
                root_kidx += 1;
            } else {
                path_keys.push(gen_key(rng, d));
            }
        }

        let n_entries = rng.usize(1..=8);
        let mut aot_entries = Vec::new();

        for _ in 0..n_entries {
            let header: Vec<String> = path_keys
                .iter()
                .map(|(_, s)| re_quote_key(rng, s))
                .collect();
            write_fmt(&mut toml, format_args!("[[{}]]\n", header.join(".")));

            let mut inner_exp = Vec::new();
            let mut inner_kidx = 0;
            gen_section_kvs(rng, &mut toml, &mut inner_exp, &mut inner_kidx);

            if rng.u8(0..4) == 0 {
                let (child_toml, child_str) = gen_key(rng, inner_kidx);
                let mut child_header: Vec<String> = path_keys
                    .iter()
                    .map(|(_, s)| re_quote_key(rng, s))
                    .collect();
                child_header.push(child_toml);
                write_fmt(&mut toml, format_args!("[{}]\n", child_header.join(".")));
                let mut ch_exp = Vec::new();
                let mut ch_kidx = 0;
                gen_section_kvs(rng, &mut toml, &mut ch_exp, &mut ch_kidx);
                inner_exp.push((child_str, Expected::Table(ch_exp)));
            }

            aot_entries.push(Expected::Table(inner_exp));
        }

        let mut result = Expected::Array(aot_entries);
        for d in (1..depth).rev() {
            result = Expected::Table(vec![(path_keys[d].1.clone(), result)]);
        }
        root_exp.push((path_keys[0].1.clone(), result));
    }

    // [sec.sub1], [sec.sub2], ..., then [sec] adding direct keys.
    if rng.u8(0..3) == 0 {
        let (_, sec_str) = gen_key(rng, root_kidx);
        let mut sec_exp = Vec::new();
        let mut sec_kidx = 0;

        let n_sub = rng.usize(2..=5);
        for _ in 0..n_sub {
            let (sub_toml, sub_str) = gen_key(rng, sec_kidx);
            sec_kidx += 1;
            write_fmt(
                &mut toml,
                format_args!("[{}.{}]\n", re_quote_key(rng, &sec_str), sub_toml),
            );
            let mut sub_exp = Vec::new();
            let mut sub_kidx = 0;
            gen_section_kvs(rng, &mut toml, &mut sub_exp, &mut sub_kidx);
            sec_exp.push((sub_str, Expected::Table(sub_exp)));
        }

        write_fmt(
            &mut toml,
            format_args!("[{}]\n", re_quote_key(rng, &sec_str)),
        );
        gen_section_kvs(rng, &mut toml, &mut sec_exp, &mut sec_kidx);
        root_exp.push((sec_str, Expected::Table(sec_exp)));
    }

    (toml, root_exp)
}

/// Generate key-value pairs for a table section.
/// Mixes plain scalars, dotted keys, inline tables, and inline arrays.
fn gen_section_kvs(
    rng: &mut fastrand::Rng,
    toml: &mut String,
    expected: &mut Vec<(String, Expected)>,
    kidx: &mut usize,
) {
    let n = rng.usize(1..=10);
    for _ in 0..n {
        let (key_toml, key_str) = gen_key(rng, *kidx);
        *kidx += 1;

        match rng.u8(0..10) {
            0 => {
                let (val_toml, val_exp) = gen_inline_table_val(rng, 2);
                write_fmt(toml, format_args!("{key_toml} = {val_toml}\n"));
                expected.push((key_str, val_exp));
            }
            1 => {
                let (val_toml, val_exp) = gen_inline_array_val(rng, 1);
                write_fmt(toml, format_args!("{key_toml} = {val_toml}\n"));
                expected.push((key_str, val_exp));
            }
            2 => {
                let (sub_toml, sub_str) = gen_key(rng, *kidx);
                *kidx += 1;
                let (val_toml, val_exp) = gen_value(rng, 1);
                write_fmt(toml, format_args!("{key_toml}.{sub_toml} = {val_toml}\n"));
                expected.push((key_str, Expected::Table(vec![(sub_str, val_exp)])));
            }
            _ => {
                let (val_toml, val_exp) = gen_scalar(rng);
                write_fmt(toml, format_args!("{key_toml} = {val_toml}\n"));
                expected.push((key_str, val_exp));
            }
        }
    }
}

fn parse_checked<'a>(
    seed: u64,
    phase: &str,
    input: &'a str,
    arena: &'a Arena,
) -> toml_spanner::Document<'a> {
    toml_spanner::parse(input, arena).unwrap_or_else(|e| {
        panic!(
            "\n============= {phase} FAILED =============\n\
             Seed:     {seed}\n\
             Input ({} bytes):\n{input}\n\
             Error: {e:?}\n\
             ============================================",
            input.len(),
        );
    })
}

fn verify_checked(
    seed: u64,
    phase: &str,
    input: &str,
    actual: &Table<'_>,
    expected: &[(String, Expected)],
) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        verify_table(actual, expected, "");
    }));

    if let Err(panic_info) = result {
        let msg = if let Some(s) = panic_info.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = panic_info.downcast_ref::<&str>() {
            s.to_string()
        } else {
            "unknown panic".to_string()
        };
        panic!(
            "\n============== {phase} MISMATCH =============\n\
             Seed:     {seed}\n\
             Input ({} bytes):\n{input}\n\
             Error: {msg}\n\
             ========================================",
            input.len(),
        );
    }
}

fn emit_auto(seed: u64, phase: &str, source: &str, table: &Table<'_>) -> String {
    let arena = Arena::new();
    let output = Formatting::default().format_table_to_bytes(table.clone_in(&arena), &arena);
    String::from_utf8(output).unwrap_or_else(|e| {
        panic!(
            "\n============= {phase} UTF-8 FAILED =============\n\
             Seed:     {seed}\n\
             Source ({} bytes):\n{source}\n\
             Error: {e:?}\n\
             ============================================",
            source.len(),
        );
    })
}

fn main() {
    let total_iterations: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(50_000);

    let mut rng = fastrand::Rng::new();
    let mut total_checks = 0u64;

    for seed in 0..total_iterations {
        rng.seed(seed);
        let (toml_text, expected) = gen_document(&mut rng);

        let parse_arena = Arena::new();
        let doc = parse_checked(seed, "PARSE ORIGINAL", &toml_text, &parse_arena);
        verify_checked(seed, "VERIFY ORIGINAL", &toml_text, doc.table(), &expected);

        let emitted = emit_auto(seed, "AUTO EMIT", &toml_text, doc.table());

        let reparsed_arena = Arena::new();
        let reparsed = parse_checked(seed, "PARSE EMITTED", &emitted, &reparsed_arena);
        verify_checked(
            seed,
            "VERIFY EMITTED",
            &emitted,
            reparsed.table(),
            &expected,
        );

        let emitted_again = emit_auto(seed, "RE-EMIT EMITTED", &emitted, reparsed.table());
        assert_eq!(
            emitted_again,
            emitted,
            "\n============== EMIT NOT IDEMPOTENT =============\n\
             Seed:     {seed}\n\
             First emit ({} bytes):\n{emitted}\n\
             Second emit ({} bytes):\n{emitted_again}\n\
             ========================================",
            emitted.len(),
            emitted_again.len(),
        );

        if seed % 10_000 == 0 && seed > 0 {
            eprintln!("Progress: {seed}/{total_iterations} seeds ({total_checks} checks)");
        }

        total_checks += 3;
    }
    eprintln!("All {total_iterations} seeds passed ({total_checks} checks).");
}
