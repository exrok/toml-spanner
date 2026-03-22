use aho_corasick::AhoCorasick;
use ra_ap_rustc_lexer::{TokenKind, tokenize};
use std::collections::HashMap;
use std::io::Write;
use std::ops::Range;
use std::path::PathBuf;
use std::slice::Iter as SliceIter;

// note the order here is important for the sorting of the merges
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
enum Kind {
    PushPunctAlone,
    PushPunctJoint,
    PushIdent,
    PushEmptyGroup,
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct Stmt<'a> {
    kind: Kind,
    buffer: &'a str,
    literal: &'a str,
}
fn munch_thing<'a>(
    raw: &'a str,
    from: &mut SliceIter<'_, (TokenKind, Range<usize>)>,
) -> Option<(Stmt<'a>, Range<usize>)> {
    let (start, kind) = loop {
        match from.next() {
            Some((TokenKind::Ident, range)) => match &raw[range.clone()] {
                "tt_ident" => break (range, Kind::PushIdent),
                "tt_punct_joint" => break (range, Kind::PushPunctJoint),
                "tt_punct_alone" => break (range, Kind::PushPunctAlone),
                "tt_group_empty" => break (range, Kind::PushEmptyGroup),
                _ => return None,
            },
            Some((TokenKind::Whitespace, _)) => continue,
            _ => return None,
        }
    };
    let mut group = 0;
    let writer_start = 'foo: {
        for (i, &ch) in raw.as_bytes()[..start.start].iter().enumerate().rev() {
            if group > 0 {
                if ch == b'(' {
                    group -= 1;
                }
                if ch == b')' {
                    group += 1;
                }
                continue;
            }
            if ch == b')' {
                group += 1;
                continue;
            }
            if ch == b'.' || ch.is_ascii_alphabetic() || ch == b'_' {
                continue;
            }
            break 'foo i + 1;
        }
        0
    };
    let raw_writer = &raw[writer_start..start.start];
    let writer = raw_writer.strip_suffix('.').unwrap_or(raw_writer).trim();
    let mut toks = from.clone();
    // let mut toks = toks.filter(|(tok, _)| !matches!(tok, TokenKind::Whitespace));
    let mut next = || loop {
        match toks.next() {
            Some((TokenKind::Whitespace, _)) => continue,
            other => return other,
        }
    };
    let (paren, _) = next()?;
    if *paren != TokenKind::OpenParen {
        return None;
    }
    if kind == Kind::PushEmptyGroup {
        let (TokenKind::Ident { .. }, _) = next()? else {
            return None;
        };
        let (TokenKind::Colon { .. }, _) = next()? else {
            return None;
        };
        let (TokenKind::Colon { .. }, _) = next()? else {
            return None;
        };
        let (TokenKind::Ident { .. }, value) = next()? else {
            return None;
        };
        if next()?.0 != TokenKind::CloseParen {
            return None;
        }
        let (TokenKind::Semi, end) = next()? else {
            return None;
        };
        *from = toks;
        return Some((
            Stmt {
                kind,
                buffer: writer,
                literal: &raw[value.clone()],
            },
            writer_start..end.end,
        ));
    }

    let (TokenKind::Literal { .. }, value) = next()? else {
        return None;
    };
    if next()?.0 != TokenKind::CloseParen {
        return None;
    }
    let (TokenKind::Semi, end) = next()? else {
        return None;
    };
    *from = toks;
    Some((
        Stmt {
            kind,
            buffer: writer,
            literal: &raw[value.clone()],
        },
        writer_start..end.end,
    ))
}

fn modules(input: &str) -> impl Iterator<Item = (&str, &str)> {
    input.split("\nmod ").filter_map(|foo| {
        let (name, rest) = foo.split_once("{\n")?;
        let (body, _) = rest.split_once("\n}")?;
        Some((name.trim(), body))
    })
}

fn line_by_line_transform(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut iter = memchr::memchr_iter(b'\n', input);
    let mut line_start = 0;
    let mut write_from = 0;
    loop {
        let mut content_start = line_start;
        let mut indent = 0;
        while input.get(content_start) == Some(&b' ') {
            indent += 1;
            content_start += 1;
        }
        let prefix = b"use proc_macro2";
        if input[content_start..].starts_with(b"#[rustfmt::skip]") {
            output.extend_from_slice(&input[write_from..content_start]);
            write_from = content_start + "#[rustfmt::skip]".len();
        } else if input[content_start..].starts_with(prefix) {
            output.extend_from_slice(&input[write_from..content_start]);
            write_from = content_start + prefix.len();
            output.extend_from_slice(b"use proc_macro");
        } else if input[content_start..].starts_with(b"macro_rules! ") {
            output.extend_from_slice(&input[write_from..line_start]);
            loop {
                let line_start = iter.next().expect("Well formed macro") + 1;
                let mut inner_content_start = line_start;
                let mut inner_indent = 0;
                while input.get(inner_content_start) == Some(&b' ') {
                    inner_indent += 1;
                    inner_content_start += 1;
                }
                if inner_indent != indent {
                    continue;
                }
                if input[inner_content_start..].starts_with(b"}\n") {
                    write_from = inner_content_start + 2;
                    break;
                }
            }
        }

        line_start = match iter.next() {
            Some(end) => end + 1,
            None => break,
        };
    }
    output.extend_from_slice(&input[write_from..]);
    output
}

struct TTMergeBlocks<'a> {
    stmts: Vec<(Stmt<'a>, Range<usize>)>,
    splits: Vec<usize>,
}

impl<'a> TTMergeBlocks<'a> {
    fn groups(&self) -> impl Iterator<Item = &[(Stmt<'a>, Range<usize>)]> {
        let mut prev = 0;
        self.splits.iter().map(move |&end| {
            let start = prev;
            prev = end;
            &self.stmts[start..end]
        })
    }
    fn new(bytes: &'a [u8]) -> Self {
        let mut this = TTMergeBlocks {
            stmts: Vec::new(),
            splits: Vec::new(),
        };
        let mut end = 0;
        let data = std::str::from_utf8(bytes).unwrap();
        let tokens: Vec<(TokenKind, Range<usize>)> = tokenize(&data)
            .map(move |tok| {
                let start = end;
                end += tok.len as usize;
                (tok.kind, start..end)
            })
            .collect();
        let mut iter = tokens.iter();
        let mut current_buffer = "";
        let mut last_split = 0;
        let mut last_end = 0;
        while iter.len() > 0 {
            if let Some((fnctl, range)) = munch_thing(&data, &mut iter) {
                if data[last_end..range.start]
                    .as_bytes()
                    .iter()
                    .any(|ch| !ch.is_ascii_whitespace())
                    && data[last_end..range.start]
                        .as_bytes()
                        .iter()
                        .all(|ch| ch.is_ascii_whitespace() || *ch == b';')
                {
                    //println!("{:?} {:#?}", &data[last_end - 10..range.start + 10], fnctl);
                }

                if fnctl.buffer != current_buffer
                    || data[last_end..range.start]
                        .as_bytes()
                        .iter()
                        .any(|ch| !ch.is_ascii_whitespace())
                {
                    if this.stmts.len() != last_split {
                        this.splits.push(this.stmts.len());
                        last_split = this.stmts.len();
                    }
                }
                last_end = range.end;
                current_buffer = fnctl.buffer;
                this.stmts.push((fnctl, range))
            }
        }
        if this.stmts.len() != last_split {
            this.splits.push(this.stmts.len());
        }
        this
    }
}

fn pipe_rustfmt(data: &[u8]) -> Vec<u8> {
    let mut rustfmt_path = format!(
        "{}/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustfmt",
        std::env::var("HOME").unwrap_or_default()
    );
    if !std::path::Path::exists(rustfmt_path.as_ref()) {
        rustfmt_path = "rustfmt".into()
    }
    let mut rustfmt = std::process::Command::new(rustfmt_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    rustfmt.stdin.as_mut().unwrap().write_all(data).unwrap();
    let output = rustfmt.wait_with_output().unwrap();
    output.stdout
}
const REPLACEMENTS: &[(&str, &str)] = &[
    ("((&mut output),", "(&mut output,"),
    (" (&mut output).", " output."),
    ("((&mut out),", "(&mut out,"),
    (" (&mut out).", " out."),
    (" (&mut rust_writer).", " rust_writer."),
    (
        "TokenTree::from(ctx.crate_path.clone())",
        "ctx.crate_path.clone()",
    ),
    (
        "::core::panicking::panic(\"not yet implemented\")",
        "todo!()",
    ),
];

fn export_merged_blocks(files: &[&[u8]]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let merges: Vec<_> = files.iter().map(|data| TTMergeBlocks::new(data)).collect();
    let mut distinct_stmts: HashMap<(Kind, &str), u64> = HashMap::new();
    distinct_stmts.insert((Kind::PushEmptyGroup, "Parenthesis"), 0x1_0000_0000);
    distinct_stmts.insert((Kind::PushEmptyGroup, "Brace"), 0x2_0000_0000);
    distinct_stmts.insert((Kind::PushEmptyGroup, "Bracket"), 0x3_0000_0000);
    distinct_stmts.insert((Kind::PushEmptyGroup, "Empty"), 0x4_0000_0000);
    for merge in &merges {
        for (stmt, _) in &merge.stmts {
            *distinct_stmts.entry((stmt.kind, stmt.literal)).or_insert(0) += 1;
        }
    }
    // We sort these to ensure reproduceable results.
    // assert!((distinct_stmts.len() + (b' ' as usize)) < u8::MAX as usize);
    assert!((distinct_stmts.len()) < u8::MAX as usize);
    let mut distinct_stmts: Vec<(u64, Kind, &str)> = distinct_stmts
        .into_iter()
        .map(|((kind, text), count)| (count | ((kind as u64) << 49), kind, text))
        .collect();
    // Try to map most common punctionuation to `\t`, `\n` & `\r`
    {
        let mut puncts = Vec::new();
        for stmt in &mut distinct_stmts {
            if !matches!(stmt.1, Kind::PushPunctAlone | Kind::PushPunctJoint) {
                continue;
            }
            puncts.push(stmt);
        }
        puncts.sort_by_key(|(k, kind, text)| (*k as u32, *kind, *text));
        let mut iter = puncts.iter_mut().rev();
        let minimizing = [b'\t' as u64, b'\n' as u64, b'\r' as u64];
        for idx in minimizing {
            if let Some((c, _, _)) = iter.next() {
                *c = idx;
            }
        }
        let mut i = 0;
        for (c, _, _) in iter {
            while minimizing.contains(&i) {
                i += 1;
            }
            *c = i;
            i += 1;
        }
    }
    distinct_stmts.sort_unstable();
    let stmt_id: HashMap<(Kind, &str), u8> = distinct_stmts
        .iter()
        .enumerate()
        .map(|(i, (_, kind, text))| ((*kind, *text), (i as u8)))
        .collect();
    let mut max_ident = 0usize;
    let mut max_punct = 0usize;
    for (i, (_, kind, _)) in distinct_stmts.iter().enumerate() {
        if matches!(kind, Kind::PushPunctAlone | Kind::PushPunctJoint) {
            max_punct = i;
        }
        if matches!(kind, Kind::PushIdent) {
            max_ident = i;
        }
    }
    if distinct_stmts.is_empty() {
        panic!("NO stmts found");
    }
    let puncts = &distinct_stmts[..max_punct + 1];
    let idents = &distinct_stmts[max_punct + 1..max_ident + 1];
    // we implement greedy substring compression
    // at the time of implementation brings down the size from 1400 -> 1036
    let mut stmt_slice_buffer: Vec<u8> = Vec::new();

    let mut current_slice = Vec::<u8>::new();
    // todo consider remapping the space so that &\x00 never happen and
    // that the majority of text gets mapped to asscie characters
    let mut g_count = 0;
    let mut og_count = 0;
    let mut outputs: Vec<Vec<u8>> = Vec::new();
    for (merge, data) in merges.iter().zip(files) {
        let mut output: Vec<u8> = Vec::new();
        let mut written = 0;
        for group in merge.groups() {
            g_count += 1;
            og_count += group.len();
            current_slice.clear();
            for (stmt, _) in group {
                let id = stmt_id[&(stmt.kind, stmt.literal)];
                current_slice.push(id);
            }
            output.extend_from_slice(&data[written..group[0].1.start]);
            written = group.last().unwrap().1.end;
            if let [(stmt, _)] = group {
                match stmt.kind {
                    Kind::PushPunctAlone | Kind::PushPunctJoint => {
                        write!(output, "{}.blit_punct({});", stmt.buffer, current_slice[0])
                            .unwrap();
                    }
                    Kind::PushIdent => {
                        write!(
                            output,
                            "{}.blit_ident({});",
                            stmt.buffer,
                            current_slice[0] - puncts.len() as u8
                        )
                        .unwrap();
                    }
                    Kind::PushEmptyGroup => {
                        output.extend_from_slice(&data[group[0].1.start..written])
                    }
                }
            } else if group.len() > 0 {
                let start =
                    if let Some(start) = memchr::memmem::find(&stmt_slice_buffer, &current_slice) {
                        start
                    } else {
                        let start = stmt_slice_buffer.len();
                        stmt_slice_buffer.extend_from_slice(&current_slice);
                        start
                    };
                write!(
                    output,
                    "{}.blit({}, {});",
                    group[0].0.buffer,
                    start,
                    current_slice.len()
                )
                .unwrap();
            }
        }
        output.extend_from_slice(&data[written..]);
        outputs.push(output);
    }
    println!("Calls: {} -> {}", og_count, g_count);
    // println!("{}", stmt_slice_buffer.len());
    // println!("{}", stmt_slice_buffer.escape_ascii().to_string());
    // println!("{}", stmt_slice_buffer.escape_ascii().to_string().len());
    // println!("{}", [7, 8, 9, 10, 11, 12, 13].escape_ascii());
    let cache_template = stringify! {
        use proc_macro::{ Punct, Spacing };
        pub static BLIT_SRC: &[u8] = __PLACEHOLDER__;

        pub const IDENT_SIZE: usize = __PLACEHOLDER__;
        pub static NAMES: [&str; __PLACEHOLDER__] = [__PLACEHOLDER__];

        pub const PUNCT_SIZE: usize = __PLACEHOLDER__;
        pub fn punct_cache_initial_state() -> [Punct; PUNCT_SIZE] {
            [__PLACEHOLDER__]
        }
    };
    let mut segments = cache_template.split("__PLACEHOLDER__");

    let mut cache_output = Vec::<u8>::new();

    cache_output.extend_from_slice(segments.next().unwrap().as_bytes());

    let _ = write!(cache_output, "b\"{}\"", stmt_slice_buffer.escape_ascii());
    cache_output.extend_from_slice(segments.next().unwrap().as_bytes());
    let _ = write!(cache_output, "{}", idents.len());
    cache_output.extend_from_slice(segments.next().unwrap().as_bytes());
    let _ = write!(cache_output, "{}", idents.len());
    cache_output.extend_from_slice(segments.next().unwrap().as_bytes());
    for ident in idents {
        let _ = write!(cache_output, "{},", ident.2);
    }
    cache_output.extend_from_slice(segments.next().unwrap().as_bytes());
    let _ = write!(cache_output, "{}", puncts.len());
    cache_output.extend_from_slice(segments.next().unwrap().as_bytes());
    for (_, kind, literal) in puncts {
        match kind {
            Kind::PushPunctAlone => {
                let _ = write!(cache_output, "Punct::new({}, Spacing::Alone),", literal);
            }
            Kind::PushPunctJoint => {
                let _ = write!(cache_output, "Punct::new({}, Spacing::Joint),", literal);
            }
            _ => (),
        }
    }
    cache_output.extend_from_slice(segments.next().unwrap().as_bytes());

    return (cache_output, outputs);
}

#[derive(Debug)]
struct Match<'a, 'b> {
    text: &'a str,
    components: &'b [MatchComponent<'a>; 4],
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct TokPair<'a> {
    kind: TokenKind,
    text: &'a str,
}

#[derive(Debug)]
struct MatchComponent<'a> {
    text: &'a str,
    tokens: &'a [TokPair<'a>],
}

#[derive(Debug)]
enum MatchEntry<'a> {
    Const(TokPair<'a>),
    Define(usize, fn(TokPair<'_>) -> bool),
    Use(usize),
}

fn ext<'a>(foo: &'a str, a: &str, bb: &str) -> &'a str {
    let a = a.as_bytes().as_ptr();
    let b = bb.as_bytes().as_ptr();
    unsafe {
        if !foo.as_bytes().as_ptr_range().contains(&a)
            || !foo.as_bytes().as_ptr_range().contains(&b)
        {
            return "";
        }
        if a > b {
            return "";
        }
        let ptr = foo.as_bytes().as_ptr();
        let offset = a.offset_from(ptr) as usize;
        let len = b.offset_from(a) as usize + bb.len();
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr.add(offset), len as usize))
    }
}

// returns the last token of the match token
fn try_match<'a, 'b>(
    raw_input: &'a str,
    iter: &mut SliceIter<'a, TokPair<'a>>,
    entries: &'a [MatchEntry<'_>],
    bindings: &'b mut [MatchComponent<'a>; 4],
) -> Option<Match<'a, 'b>> {
    let mut iter = iter.clone();
    let start = iter.as_slice();
    let mut entries = entries.iter().peekable();
    while let Some(entry) = entries.next() {
        match entry {
            MatchEntry::Const(pair) => {
                let pt = iter.next()?;
                if pair != pt {
                    return None;
                }
                if entries.len() == 0 {
                    return Some(Match {
                        text: ext(raw_input, &start[0].text, pt.text),
                        components: bindings,
                    });
                }
            }
            MatchEntry::Define(idx, fnx) => {
                let terminal = entries
                    .next()
                    .expect("Variables to always have a next token");
                let MatchEntry::Const(terminal) = terminal else {
                    panic!("Vars might be followed by a const")
                };
                let lookahead = if let Some(MatchEntry::Const(lookahead)) = entries.peek() {
                    Some(lookahead)
                } else {
                    None
                };
                let start = iter.as_slice();
                let tok = iter.next()?;
                if !fnx(*tok) {
                    return None;
                }
                let mut len = 1;
                let mut tok = iter.next()?;
                loop {
                    if !fnx(*tok) {
                        if tok != terminal {
                            return None;
                        }
                        break;
                    }
                    if tok == terminal {
                        if let Some(lookahead) = lookahead {
                            if let Some(f) = iter.as_slice().first() {
                                if f == lookahead {
                                    break;
                                }
                            }
                        } else {
                            break;
                        }
                    }
                    tok = iter.next()?;
                    len += 1;
                }
                bindings[*idx] = MatchComponent {
                    text: ext(raw_input, start[0].text, start[len - 1].text),
                    tokens: &start[..len],
                };
            }
            MatchEntry::Use(idx) => {
                let comp = &bindings[*idx];
                if !iter.as_slice().starts_with(comp.tokens) {
                    return None;
                }
                for _ in 0..comp.tokens.len() {
                    iter.next();
                }
            }
        }
    }
    todo!();
}

fn structural_replace(
    pattern: &str,
    input_string: &str,
    bindings: &[(&str, fn(TokPair<'_>) -> bool)],
    mapper: &mut dyn FnMut(&mut Vec<u8>, Match<'_, '_>) -> bool,
) -> Vec<u8> {
    let mut defined_vars = 0u32;
    let match_entries: Vec<MatchEntry<'_>> = tokenize(&pattern)
        .map({
            let mut end = 0;
            move |tok| {
                let start = end;
                end += tok.len as usize;
                (tok.kind, &pattern[start..end])
            }
        })
        .filter(|(tok, _)| {
            if let TokenKind::Whitespace = tok {
                return false;
            }
            true
        })
        .map(|(tok, text)| {
            if tok == TokenKind::Ident {
                for (i, (name, fnx)) in bindings.iter().enumerate() {
                    if text == *name {
                        if defined_vars & (1 << i) == 0 {
                            defined_vars |= 1 << i;
                            return MatchEntry::Define(i, *fnx);
                        } else {
                            return MatchEntry::Use(i);
                        }
                    }
                }
            }

            MatchEntry::Const(TokPair { kind: tok, text })
        })
        .collect();
    let tokens: Vec<TokPair<'_>> = tokenize(&input_string)
        .map({
            let mut end = 0;
            move |tok| {
                let start = end;
                end += tok.len as usize;
                (tok.kind, &input_string[start..end])
            }
        })
        .filter(|(tok, _)| {
            if let TokenKind::Whitespace = tok {
                return false;
            }
            true
        })
        .map(|(tok, text)| TokPair { kind: tok, text })
        .collect();
    let mut vars = [
        MatchComponent {
            text: "",
            tokens: &[],
        },
        MatchComponent {
            text: "",
            tokens: &[],
        },
        MatchComponent {
            text: "",
            tokens: &[],
        },
        MatchComponent {
            text: "",
            tokens: &[],
        },
    ];
    let mut iter = tokens.iter();
    let mut output: Vec<u8> = Vec::new();
    let mut written = 0usize;
    loop {
        let mut trial = iter.clone();
        if let Some(mat) = try_match(input_string, &mut trial, &match_entries, &mut vars) {
            iter = trial;
            let start = unsafe { mat.text.as_ptr().offset_from(input_string.as_ptr()) };
            if (written as usize) >= start as usize {
                if iter.next().is_none() {
                    break;
                }
                continue;
            }
            output.extend_from_slice(&input_string.as_bytes()[written..start as usize]);
            written = start as usize + mat.text.len();
            mapper(&mut output, mat);
        }
        if iter.next().is_none() {
            break;
        }
    }
    output.extend_from_slice(&input_string.as_bytes()[written..]);
    output
}

fn collapse_extra_parens(input: String) -> Vec<u8> {
    let genx = structural_replace(
        stringify!({ { V_INNER } }),
        &input,
        &[("V_INNER", |x| {
            x.kind != TokenKind::CloseBrace && x.kind != TokenKind::OpenBrace
        })],
        &mut |output: &mut Vec<u8>, Match { components, .. }| {
            let _ = write!(output, "{{ {} }}", components[0].text);
            false
        },
    );
    let input = String::from_utf8(genx).unwrap();
    let genx = structural_replace(
        stringify!({
            {
                V_INNER
            };
        }),
        &input,
        &[("V_INNER", |x| {
            x.kind != TokenKind::CloseBrace && x.kind != TokenKind::OpenBrace
        })],
        &mut |output: &mut Vec<u8>, Match { components, .. }| {
            let _ = write!(output, "{{ {} }}", components[0].text);
            false
        },
    );
    let input = String::from_utf8(genx).unwrap();
    let genx = structural_replace(
        "; { V_INNER } }",
        &input,
        &[("V_INNER", |x| {
            x.kind != TokenKind::CloseBrace && x.kind != TokenKind::OpenBrace
        })],
        &mut |output: &mut Vec<u8>, Match { components, .. }| {
            let _ = write!(output, "; {} }}", components[0].text);
            false
        },
    );
    let input = String::from_utf8(genx).unwrap();
    let genx = structural_replace(
        "V_ST { V_METHOD ( V_ARGS ) } ;",
        &input,
        &[
            ("V_ST", |x| {
                matches!(x.kind, TokenKind::OpenBrace | TokenKind::Semi)
            }),
            ("V_METHOD", |x| {
                matches!(x.kind, TokenKind::Ident | TokenKind::Dot)
            }),
            ("V_ARGS", |x| {
                !matches!(x.kind, TokenKind::OpenParen | TokenKind::CloseParen)
            }),
        ],
        &mut |output: &mut Vec<u8>, Match { components, .. }| {
            let _ = write!(
                output,
                "{} {} ( {});",
                components[0].text, components[1].text, components[2].text
            );
            false
        },
    );
    return genx;
}

fn main() {
    let mut args = std::env::args();
    let _ = args.next();
    let source_crate_path: PathBuf = args.next().unwrap().into();
    let final_crate_path: PathBuf = args.next().unwrap().into();
    let ac = AhoCorasick::builder()
        .build(REPLACEMENTS.iter().map(|(x, _)| x))
        .unwrap();
    let apply_replacements = |data: &[u8]| {
        let mut output: Vec<u8> = Vec::with_capacity(data.len());
        ac.replace_all_with_bytes(data, &mut output, |m, _, dst| {
            dst.extend_from_slice(REPLACEMENTS[m.pattern().as_usize()].1.as_bytes());
            true
        });
        output
    };
    let base_transform = |data: &[u8]| {
        let res = line_by_line_transform(data);
        let res = apply_replacements(&res);
        collapse_extra_parens(String::from_utf8(res).unwrap())
    };
    let output = std::process::Command::new("cargo")
        .args([
            "expand",
            "--bin",
            "toml_macros_source",
            "--ugly",
            "--color",
            "never",
            "--no-default-features",
        ])
        .current_dir(source_crate_path)
        .output()
        .unwrap();

    let data = String::from_utf8(output.stdout).unwrap();
    let data = data.replace("#[allow(non_exhaustive_omitted_patterns)]", "");

    let mut codegen_code: Option<Vec<u8>> = None;
    let mut template_code: Option<Vec<u8>> = None;
    for (name, body) in modules(&data) {
        if name == "ast" {
            let code = base_transform(body.as_bytes());
            std::fs::write(final_crate_path.join("src/ast.rs"), pipe_rustfmt(&code)).unwrap();
        } else if name == "codegen" {
            codegen_code = Some(base_transform(body.as_bytes()));
        } else if name == "template" {
            template_code = Some(base_transform(body.as_bytes()));
        }
    }
    let codegen_code = codegen_code.expect("to find codegen module");
    let template_code = template_code.expect("to find template module");

    let (cache_file, processed) = export_merged_blocks(&[&codegen_code, &template_code]);
    std::fs::write(
        final_crate_path.join("src/writer/cache.rs"),
        pipe_rustfmt(&cache_file),
    )
    .unwrap();
    std::fs::write(
        final_crate_path.join("src/codegen.rs"),
        pipe_rustfmt(&processed[0]),
    )
    .unwrap();
    std::fs::write(
        final_crate_path.join("src/template.rs"),
        pipe_rustfmt(&processed[1]),
    )
    .unwrap();

    return;
}
