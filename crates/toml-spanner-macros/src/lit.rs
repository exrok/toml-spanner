fn byte(s: &str, idx: usize) -> u8 {
    let s = s.as_bytes();
    if idx < s.len() {
        s[idx]
    } else {
        0
    }
}

fn backslash_u(mut s: &str) -> (char, &str) {
    if byte(s, 0) != b'{' {
        panic!("{}", "expected { after \\u");
    }
    s = &s[1..];

    let mut ch = 0;
    let mut digits = 0;
    loop {
        let b = byte(s, 0);
        let digit = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => 10 + b - b'a',
            b'A'..=b'F' => 10 + b - b'A',
            b'_' if digits > 0 => {
                s = &s[1..];
                continue;
            }
            b'}' if digits == 0 => panic!("invalid empty unicode escape"),
            b'}' => break,
            _ => panic!("unexpected non-hex character after \\u"),
        };
        if digits == 6 {
            panic!("overlong unicode escape (must have at most 6 hex digits)");
        }
        ch *= 0x10;
        ch += u32::from(digit);
        digits += 1;
        s = &s[1..];
    }
    assert!(byte(s, 0) == b'}');
    s = &s[1..];

    if let Some(ch) = char::from_u32(ch) {
        (ch, s)
    } else {
        panic!("character code {:x} is not a valid unicode character", ch);
    }
}
fn backslash_x(s: &str) -> (u8, &str) {
    let mut ch = 0;
    let b0 = byte(s, 0);
    let b1 = byte(s, 1);
    ch += 0x10
        * match b0 {
            b'0'..=b'9' => b0 - b'0',
            b'a'..=b'f' => 10 + (b0 - b'a'),
            b'A'..=b'F' => 10 + (b0 - b'A'),
            _ => panic!("unexpected non-hex character after \\x"),
        };
    ch += match b1 {
        b'0'..=b'9' => b1 - b'0',
        b'a'..=b'f' => 10 + (b1 - b'a'),
        b'A'..=b'F' => 10 + (b1 - b'A'),
        _ => panic!("unexpected non-hex character after \\x"),
    };
    (ch, &s[2..])
}

fn next_chr(s: &str) -> char {
    s.chars().next().unwrap_or('\0')
}
// Clippy false positive
// https://github.com/rust-lang-nursery/rust-clippy/issues/2329
fn parse_lit_str_raw(mut s: &str) -> String {
    assert_eq!(byte(s, 0), b'r');
    s = &s[1..];

    let mut pounds = 0;
    while byte(s, pounds) == b'#' {
        pounds += 1;
    }
    assert_eq!(byte(s, pounds), b'"');
    let close = s.rfind('"').unwrap();
    for end in s[close + 1..close + 1 + pounds].bytes() {
        assert_eq!(end, b'#');
    }

    let content = s[pounds + 1..close].to_owned();
    content
}

#[allow(clippy::needless_continue)]
fn parse_lit_str_cooked(mut s: &str) -> String {
    assert_eq!(byte(s, 0), b'"');
    s = &s[1..];

    let mut content = String::new();
    'outer: loop {
        let ch = match byte(s, 0) {
            b'"' => break,
            b'\\' => {
                let b = byte(s, 1);
                s = &s[2..];
                match b {
                    b'x' => {
                        let (byte, rest) = backslash_x(s);
                        s = rest;
                        assert!(byte <= 0x80, "Invalid \\x byte in string literal");
                        char::from_u32(u32::from(byte)).unwrap()
                    }
                    b'u' => {
                        let (chr, rest) = backslash_u(s);
                        s = rest;
                        chr
                    }
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    b'\\' => '\\',
                    b'0' => '\0',
                    b'\'' => '\'',
                    b'"' => '"',
                    b'\r' | b'\n' => loop {
                        let b = byte(s, 0);
                        match b {
                            b' ' | b'\t' | b'\n' | b'\r' => s = &s[1..],
                            _ => continue 'outer,
                        }
                    },
                    b => panic!("unexpected byte {:?} after \\ character in byte literal", b),
                }
            }
            b'\r' => {
                assert_eq!(byte(s, 1), b'\n', "Bare CR not allowed in string");
                s = &s[2..];
                '\n'
            }
            _ => {
                let ch = next_chr(s);
                s = &s[ch.len_utf8()..];
                ch
            }
        };
        content.push(ch);
    }

    assert!(s.starts_with('"'));
    content
}

pub enum InlineKind {
    String(String),
    Raw(String),
    None,
}

pub fn literal_inline(raw: String) -> InlineKind {
    match raw.as_bytes()[0] {
        b'"' => {
            let content = parse_lit_str_cooked(&raw[..]);
            InlineKind::String(content)
        }
        b'r' => {
            if raw.as_bytes()[1] == b'b' {
                // in the future, we can do this
                return InlineKind::None;
            }
            let content = parse_lit_str_raw(&raw[..]);
            InlineKind::String(content)
        }
        b'0'..=b'9' => {
            if raw.as_bytes().iter().all(|ch| ch.is_ascii_digit()) {
                return InlineKind::Raw(raw.into());
            }
            InlineKind::None
        }
        _ => InlineKind::None,
    }
}
