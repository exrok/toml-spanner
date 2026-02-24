use crate::{Gen, pick_unique_idx};
use std::fmt::Write;

pub const KEYS: [&str; 12] = [
    "a", "b", "c", "d", "e", "x", "y", "z", "0", "1", "00", "a-b",
];
pub const N_KEYS: usize = KEYS.len();

const NASTY_COMMENTS: &[&str] = &[
    "#a=\"",
    "#[",
    "#[[",
    "# {",
    "#,l",
    "# ,,,",
    "#[[",
    "#a.b.c.d.e = {}",
    "# = missing key",
    "#][[",
    "#{,}",
    "##",
    "#1",
    "##",
    "#\\n,#,#,",
];

fn fmt_key(g: &mut Gen<'_>, key: &str, out: &mut String) {
    if g.next() % 100 == 1 {
        write!(out, "\"{key}\"").unwrap();
    } else {
        out.push_str(key);
    }
}

fn maybe_comment_line(g: &mut Gen<'_>, out: &mut String) {
    if g.next() % 24 == 1 {
        out.push_str(*g.pick(NASTY_COMMENTS));
        out.push('\n');
    }
}

fn maybe_comment_eol(g: &mut Gen<'_>, out: &mut String) {
    if g.next() % 19 == 1 {
        out.push(' ');
        out.push_str(*g.pick(NASTY_COMMENTS));
    }
}

fn nasty_gap(g: &mut Gen<'_>, out: &mut String) {
    if g.next() % 31 == 1 {
        out.push(' ');
        out.push_str(*g.pick(NASTY_COMMENTS));
    }
    out.push('\n');
}

fn sp(g: &mut Gen<'_>, out: &mut String) {
    if g.next() % 5 == 0 {
        out.push(' ');
    }
}

fn gen_inline_value(g: &mut Gen<'_>, out: &mut String, depth: u8) {
    let scalar = depth >= 3 || g.next() % 3 != 0;
    if scalar {
        match g.next() % 5 {
            0 => out.push_str("\"\""),
            1 => out.push_str("\"a\""),
            2 => out.push_str("\"b\""),
            3 => {
                let n = g.next() % 2;
                write!(out, "{n}").unwrap();
            }
            _ => out.push_str(if g.next() % 2 == 0 { "true" } else { "false" }),
        }
    } else {
        match g.next() % 2 {
            0 => {
                out.push('[');
                let len = g.range(0, 4);
                let ml = len > 1 && g.next() % 3 == 0;
                if ml {
                    nasty_gap(g, out);
                }
                for i in 0..len {
                    gen_inline_value(g, out, depth + 1);
                    if i < len - 1 {
                        if ml {
                            nasty_gap(g, out);
                            out.push(',');
                            nasty_gap(g, out);
                        } else {
                            out.push(',');
                            sp(g, out);
                        }
                    }
                }
                if len > 0 && g.next() % 3 == 0 {
                    out.push(',');
                }
                if ml {
                    nasty_gap(g, out);
                }
                out.push(']');
            }
            _ => gen_inline_table(g, out, depth + 1),
        }
    }
}

fn gen_inline_table(g: &mut Gen<'_>, out: &mut String, depth: u8) {
    let count = g.range(0, 4) as usize;
    let ml = count > 1 && g.next() % 4 == 0;
    out.push('{');
    if ml {
        nasty_gap(g, out);
    }
    let mut used = [false; N_KEYS];
    let mut first = true;
    for _ in 0..count {
        let Some(ki) = pick_unique_idx(g, &mut used) else {
            break;
        };
        if !first {
            if ml {
                nasty_gap(g, out);
                out.push(',');
                nasty_gap(g, out);
            } else {
                out.push(',');
                sp(g, out);
            }
        }
        first = false;
        fmt_key(g, KEYS[ki], out);
        sp(g, out);
        out.push('=');
        sp(g, out);
        gen_inline_value(g, out, depth);
    }
    if !first && g.next() % 3 == 0 {
        out.push(',');
    }
    if ml {
        nasty_gap(g, out);
    }
    out.push('}');
}

fn gen_dotted_keys(g: &mut Gen<'_>, out: &mut String, parent_key: &str, depth: u8) {
    let count = g.range(1, 4);
    let mut used = [false; N_KEYS];
    for _ in 0..count {
        let Some(ki) = pick_unique_idx(g, &mut used) else {
            break;
        };
        maybe_comment_line(g, out);
        fmt_key(g, parent_key, out);
        out.push('.');
        fmt_key(g, KEYS[ki], out);
        sp(g, out);
        out.push('=');
        sp(g, out);
        gen_inline_value(g, out, depth);
        maybe_comment_eol(g, out);
        out.push('\n');
    }
}

const LEAF: u8 = 0;
const TABLE_INLINE: u8 = 1;
const TABLE_DOTTED: u8 = 2;
const TABLE_SECTION: u8 = 3;
const AOT_INLINE: u8 = 4;
const AOT_SECTION: u8 = 5;

fn gen_section(g: &mut Gen<'_>, out: &mut String, path: &mut String, depth: u8) {
    let count = g.range(1, 5) as usize;
    let mut used = [false; N_KEYS];
    let mut plan = [(0u8, 0u8); N_KEYS];
    let mut n = 0;
    for _ in 0..count {
        let Some(ki) = pick_unique_idx(g, &mut used) else {
            break;
        };
        let cat = if depth >= 4 {
            LEAF
        } else {
            match g.next() % 5 {
                0..=1 => LEAF,
                2..=3 => match g.next() % 3 {
                    0 => TABLE_INLINE,
                    1 => TABLE_DOTTED,
                    _ => TABLE_SECTION,
                },
                _ => {
                    if g.next() % 3 == 0 {
                        AOT_INLINE
                    } else {
                        AOT_SECTION
                    }
                }
            }
        };
        plan[n] = (ki as u8, cat);
        n += 1;
    }

    for &(ki, cat) in &plan[..n] {
        let key = KEYS[ki as usize];
        match cat {
            LEAF => {
                maybe_comment_line(g, out);
                fmt_key(g, key, out);
                sp(g, out);
                out.push('=');
                sp(g, out);
                gen_inline_value(g, out, depth);
                maybe_comment_eol(g, out);
                out.push('\n');
            }
            TABLE_INLINE => {
                maybe_comment_line(g, out);
                fmt_key(g, key, out);
                sp(g, out);
                out.push('=');
                sp(g, out);
                gen_inline_table(g, out, depth + 1);
                maybe_comment_eol(g, out);
                out.push('\n');
            }
            TABLE_DOTTED => {
                gen_dotted_keys(g, out, key, depth + 1);
            }
            AOT_INLINE => {
                maybe_comment_line(g, out);
                fmt_key(g, key, out);
                sp(g, out);
                out.push('=');
                sp(g, out);
                out.push('[');
                let aot_count = g.range(1, 3);
                let ml = aot_count > 1 && g.next() % 3 == 0;
                if ml {
                    nasty_gap(g, out);
                }
                for i in 0..aot_count {
                    if i > 0 {
                        if ml {
                            nasty_gap(g, out);
                            out.push(',');
                            nasty_gap(g, out);
                        } else {
                            out.push(',');
                            sp(g, out);
                        }
                    }
                    gen_inline_table(g, out, depth + 1);
                }
                if g.next() % 3 == 0 {
                    out.push(',');
                }
                if ml {
                    nasty_gap(g, out);
                }
                out.push(']');
                maybe_comment_eol(g, out);
                out.push('\n');
            }
            _ => {}
        }
    }

    for &(ki, cat) in &plan[..n] {
        let key = KEYS[ki as usize];
        match cat {
            TABLE_SECTION => {
                maybe_comment_line(g, out);
                let path_len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
                writeln!(out, "[{path}]").unwrap();
                gen_section(g, out, path, depth + 1);
                path.truncate(path_len);
            }
            AOT_SECTION => {
                let aot_count = g.range(1, 3);
                let path_len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
                for _ in 0..aot_count {
                    maybe_comment_line(g, out);
                    writeln!(out, "[[{path}]]").unwrap();
                    gen_section(g, out, path, depth + 1);
                }
                path.truncate(path_len);
            }
            _ => {}
        }
    }
}

pub fn random_toml(buffer: &mut String, random: &[u8]) {
    let mut path = String::new();
    let mut g = Gen::new(random);
    gen_section(&mut g, buffer, &mut path, 0);
}

// ── Roundtrip-safe TOML generator (v2) ──
//
// Generates the hardest possible TOML that roundtrips through
// parse → erase-kinds → reproject → normalize → emit.
//
// Only restriction: no whitespace around `.` in dotted/header key paths
// (applies to both bare and quoted keys).
//
// Everything else is pushed to extremes:
// - Deep inline arrays (7 nesting levels, up to 8 wide)
// - Nested inline tables within arrays and vice versa
// - Deeply nested table-of-arrays (AOTs within AOTs, 6 levels)
// - Trailing commas, blank lines, varied indentation
// - Multiline inline arrays/tables with interleaved comments
// - Multiline and literal strings, hex/octal/binary ints, all datetime forms
// - Quoted keys (double/single) in headers, dotted paths, and leaves
// - Comments in every legal position

const RT_SCALARS: &[&str] = &[
    // strings — empty, short, escaped
    "\"\"",
    "\"a\"",
    "\"hello\"",
    "'a'",
    "''",
    "'hello'",
    "\"a\\nb\"",
    "\"\\t\\n\"",
    // multiline basic strings
    "\"\"\"\nhello\n\"\"\"",
    "\"\"\"\n  a\n  b\n\"\"\"",
    // multiline literal strings
    "'''\nhello\n'''",
    "'''\n  a\n  b\n'''",
    // integers — signed, underscored, hex, octal, binary
    "0",
    "42",
    "-1",
    "+99",
    "1_000",
    "1_000_000",
    "0xFF",
    "0xDEAD_BEEF",
    "0o77",
    "0o755",
    "0b101",
    "0b1010_0101",
    // floats — signed, exponents, special
    "3.14",
    "-0.5",
    "+1.0",
    "1e10",
    "6.626e-34",
    "inf",
    "-inf",
    "+inf",
    "nan",
    // booleans
    "true",
    "false",
    // datetimes — all variants
    "1979-05-27",
    "07:32:00",
    "00:32:00.999999",
    "1979-05-27T07:32:00Z",
    "1979-05-27T07:32:00-07:00",
    "1979-05-27T07:32:00.999999Z",
    "1979-05-27 07:32:00",
];

fn rt_scalar(g: &mut Gen<'_>, out: &mut String) {
    out.push_str(*g.pick(RT_SCALARS));
}

fn rt_sp(g: &mut Gen<'_>, out: &mut String) {
    match g.next() % 5 {
        0 => {}
        1 => out.push(' '),
        2 => out.push_str("  "),
        3 => out.push_str("   "),
        _ => out.push('\t'),
    }
}

fn rt_indent(g: &mut Gen<'_>, out: &mut String) {
    match g.next() % 6 {
        0..=2 => {}
        3 => out.push_str("  "),
        4 => out.push_str("    "),
        _ => out.push('\t'),
    }
}

fn rt_comment_line(g: &mut Gen<'_>, out: &mut String) {
    if g.next() % 4 == 0 {
        rt_indent(g, out);
        out.push_str(*g.pick(NASTY_COMMENTS));
        out.push('\n');
        if g.next() % 4 == 0 {
            rt_indent(g, out);
            out.push_str(*g.pick(NASTY_COMMENTS));
            out.push('\n');
        }
    }
}

fn rt_comment_eol(g: &mut Gen<'_>, out: &mut String) {
    if g.next() % 4 == 0 {
        out.push(' ');
        out.push_str(*g.pick(NASTY_COMMENTS));
    }
}

fn rt_gap(g: &mut Gen<'_>, out: &mut String) {
    if g.next() % 4 == 0 {
        rt_sp(g, out);
        out.push_str(*g.pick(NASTY_COMMENTS));
    }
    out.push('\n');
}

fn rt_blank_lines(g: &mut Gen<'_>, out: &mut String) {
    match g.next() % 8 {
        0 => out.push('\n'),
        1 => out.push_str("\n\n"),
        _ => {}
    }
}

fn rt_key(g: &mut Gen<'_>, key: &str, out: &mut String) {
    match g.next() % 8 {
        0 => write!(out, "\"{key}\"").unwrap(),
        1 => write!(out, "'{key}'").unwrap(),
        _ => out.push_str(key),
    }
}

fn rt_inline_array(g: &mut Gen<'_>, out: &mut String, depth: u8) {
    out.push('[');
    let len = match depth {
        0..=1 => g.range(0, 8),
        2..=3 => g.range(0, 6),
        _ => g.range(0, 4),
    };
    let ml = len > 0 && g.next() % 2 == 0;
    if ml {
        rt_gap(g, out);
    } else {
        rt_sp(g, out);
    }
    for i in 0..len {
        if i > 0 {
            rt_sp(g, out);
            out.push(',');
            if ml {
                rt_gap(g, out);
                rt_comment_line(g, out);
            } else {
                rt_sp(g, out);
            }
        }
        if ml {
            rt_indent(g, out);
        }
        rt_inline_value(g, out, depth + 1);
    }
    if len > 0 && g.next() % 2 == 0 {
        rt_sp(g, out);
        out.push(',');
    }
    if ml {
        rt_gap(g, out);
        rt_indent(g, out);
    } else {
        rt_sp(g, out);
    }
    out.push(']');
}

fn rt_inline_sep(g: &mut Gen<'_>, out: &mut String, ml: bool, first: &mut bool) {
    if !*first {
        rt_sp(g, out);
        out.push(',');
        if ml {
            rt_gap(g, out);
            rt_comment_line(g, out);
        } else {
            rt_sp(g, out);
        }
    }
    *first = false;
    if ml {
        rt_indent(g, out);
    }
    rt_sp(g, out);
}

fn rt_dotted_segments(g: &mut Gen<'_>, out: &mut String, extra: u8) {
    for _ in 0..extra {
        out.push('.');
        out.push_str(KEYS[g.next() as usize % N_KEYS]);
    }
}

fn rt_inline_table(g: &mut Gen<'_>, out: &mut String, depth: u8) {
    let count = match depth {
        0..=2 => g.range(0, 6) as usize,
        _ => g.range(0, 4) as usize,
    };
    let ml = count > 0 && g.next() % 2 == 0;
    out.push('{');
    if ml {
        rt_gap(g, out);
    } else {
        rt_sp(g, out);
    }
    let mut used = [false; N_KEYS];
    let mut first = true;
    for _ in 0..count {
        let Some(ki) = pick_unique_idx(g, &mut used) else {
            break;
        };
        let is_group = depth < 4 && g.next() % 4 == 0;
        if is_group {
            // Dotted key group: parent.sub1 = v, parent.sub2.x.y = v, ...
            let sub_count = g.range(2, 5);
            let mut sub_used = [false; N_KEYS];
            for _ in 0..sub_count {
                let Some(si) = pick_unique_idx(g, &mut sub_used) else {
                    break;
                };
                rt_inline_sep(g, out, ml, &mut first);
                out.push_str(KEYS[ki]);
                out.push('.');
                out.push_str(KEYS[si]);
                let extra = g.next() % 4;
                rt_dotted_segments(g, out, extra);
                rt_sp(g, out);
                out.push('=');
                rt_sp(g, out);
                rt_inline_value(g, out, depth + 1);
                rt_sp(g, out);
            }
        } else {
            rt_inline_sep(g, out, ml, &mut first);
            rt_key(g, KEYS[ki], out);
            if depth < 4 {
                let extra = g.next() % 3;
                rt_dotted_segments(g, out, extra);
            }
            rt_sp(g, out);
            out.push('=');
            rt_sp(g, out);
            rt_inline_value(g, out, depth);
            rt_sp(g, out);
        }
    }
    if !first && g.next() % 2 == 0 {
        rt_sp(g, out);
        out.push(',');
    }
    if ml {
        rt_gap(g, out);
        rt_indent(g, out);
    } else {
        rt_sp(g, out);
    }
    out.push('}');
}

fn rt_inline_value(g: &mut Gen<'_>, out: &mut String, depth: u8) {
    let go_deep = match depth {
        0..=1 => g.next() % 10 < 7,
        2 => g.next() % 2 == 0,
        3 => g.next() % 3 == 0,
        4..=5 => g.next() % 5 == 0,
        _ => false,
    };
    if go_deep {
        if g.next() % 10 < 7 {
            rt_inline_array(g, out, depth);
        } else {
            rt_inline_table(g, out, depth);
        }
    } else {
        rt_scalar(g, out);
    }
}

fn gen_rt_dotted(g: &mut Gen<'_>, out: &mut String, parent_key: &str, depth: u8) {
    let count = g.range(1, 5);
    let mut used = [false; N_KEYS];
    for _ in 0..count {
        let Some(ki) = pick_unique_idx(g, &mut used) else {
            break;
        };
        rt_comment_line(g, out);
        rt_indent(g, out);
        out.push_str(parent_key);
        out.push('.');
        rt_key(g, KEYS[ki], out);
        rt_sp(g, out);
        out.push('=');
        rt_sp(g, out);
        rt_inline_value(g, out, depth);
        rt_comment_eol(g, out);
        out.push('\n');
    }
}

fn gen_rt_section(g: &mut Gen<'_>, out: &mut String, path: &mut String, depth: u8) {
    let count = match depth {
        0 => g.range(1, 8) as usize,
        1..=2 => g.range(1, 6) as usize,
        3..=4 => g.range(1, 4) as usize,
        _ => g.range(1, 3) as usize,
    };
    let mut used = [false; N_KEYS];
    let mut plan = [(0u8, 0u8); N_KEYS];
    let mut n = 0;
    for _ in 0..count {
        let Some(ki) = pick_unique_idx(g, &mut used) else {
            break;
        };
        let cat = if depth >= 6 {
            LEAF
        } else {
            match g.next() % 6 {
                0..=1 => LEAF,
                2 => TABLE_INLINE,
                3 => match g.next() % 2 {
                    0 => TABLE_DOTTED,
                    _ => TABLE_SECTION,
                },
                4 => AOT_SECTION,
                _ => {
                    if g.next() % 3 == 0 {
                        AOT_INLINE
                    } else {
                        AOT_SECTION
                    }
                }
            }
        };
        plan[n] = (ki as u8, cat);
        n += 1;
    }

    for &(ki, cat) in &plan[..n] {
        let key = KEYS[ki as usize];
        match cat {
            LEAF => {
                rt_blank_lines(g, out);
                rt_comment_line(g, out);
                rt_indent(g, out);
                rt_key(g, key, out);
                rt_sp(g, out);
                out.push('=');
                rt_sp(g, out);
                rt_inline_value(g, out, depth);
                rt_comment_eol(g, out);
                out.push('\n');
            }
            TABLE_INLINE => {
                rt_blank_lines(g, out);
                rt_comment_line(g, out);
                rt_indent(g, out);
                rt_key(g, key, out);
                rt_sp(g, out);
                out.push('=');
                rt_sp(g, out);
                rt_inline_table(g, out, depth + 1);
                rt_comment_eol(g, out);
                out.push('\n');
            }
            TABLE_DOTTED => {
                rt_blank_lines(g, out);
                gen_rt_dotted(g, out, key, depth + 1);
            }
            AOT_INLINE => {
                rt_blank_lines(g, out);
                rt_comment_line(g, out);
                rt_indent(g, out);
                rt_key(g, key, out);
                rt_sp(g, out);
                out.push('=');
                rt_sp(g, out);
                rt_inline_array(g, out, depth);
                rt_comment_eol(g, out);
                out.push('\n');
            }
            _ => {}
        }
    }

    for &(ki, cat) in &plan[..n] {
        let key = KEYS[ki as usize];
        match cat {
            TABLE_SECTION => {
                rt_blank_lines(g, out);
                rt_comment_line(g, out);
                let path_len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
                rt_indent(g, out);
                write!(out, "[{path}]").unwrap();
                rt_comment_eol(g, out);
                out.push('\n');
                gen_rt_section(g, out, path, depth + 1);
                path.truncate(path_len);
            }
            AOT_SECTION => {
                let aot_count = g.range(1, 5);
                let path_len = path.len();
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
                for _ in 0..aot_count {
                    rt_blank_lines(g, out);
                    rt_comment_line(g, out);
                    rt_indent(g, out);
                    write!(out, "[[{path}]]").unwrap();
                    rt_comment_eol(g, out);
                    out.push('\n');
                    gen_rt_section(g, out, path, depth + 1);
                }
                path.truncate(path_len);
            }
            _ => {}
        }
    }
}

pub fn random_roundtrip_toml(buffer: &mut String, random: &[u8]) {
    let mut path = String::new();
    let mut g = Gen::new(random);
    gen_rt_section(&mut g, buffer, &mut path, 0);
}

pub fn random_double_toml(buffer: &mut String, random: &[u8]) -> usize {
    let [split, rest @ ..] = random else {
        return 0;
    };
    let split = (rest.len() * *split as usize) / 255;
    let (first, second) = rest.split_at(split);

    let mut path = String::new();
    {
        let mut g = Gen::new(first);
        gen_section(&mut g, buffer, &mut path, 0);
    }
    let output_split = buffer.len();
    {
        let mut g = Gen::new(second);
        path.clear();
        gen_section(&mut g, buffer, &mut path, 0);
    }
    output_split
}
