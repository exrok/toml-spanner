use super::*;

#[test]
fn span_basics_and_conversions() {
    // Construction and field access
    let s = Span::new(10, 20);
    assert_eq!(s.start, 10);
    assert_eq!(s.end, 20);

    // is_empty
    assert!(Span::new(0, 0).is_empty());
    assert!(!Span::new(0, 1).is_empty());
    assert!(!Span::new(1, 0).is_empty());

    // Equality
    assert_eq!(Span::new(1, 2), Span::new(1, 2));
    assert_ne!(Span::new(1, 2), Span::new(1, 3));

    // Into (u32, u32)
    let t: (u32, u32) = Span::new(5, 10).into();
    assert_eq!(t, (5, 10));

    // Into (usize, usize)
    let t: (usize, usize) = Span::new(5, 10).into();
    assert_eq!(t, (5, 10));

    // From Range<u32>
    let s: Span = (3u32..7u32).into();
    assert_eq!(s.start, 3);
    assert_eq!(s.end, 7);

    // Into Range<u32>
    let r: std::ops::Range<u32> = Span::new(3, 7).into();
    assert_eq!(r, 3..7);

    // Into Range<usize>
    let r: std::ops::Range<usize> = Span::new(3, 7).into();
    assert_eq!(r, 3usize..7usize);
}

#[test]
fn spanned_basics_and_comparison() {
    // Construction with empty span
    let s = Spanned::new(42);
    assert_eq!(s.value, 42);
    assert!(s.span.is_empty());

    // Construction with explicit span
    let s = Spanned::with_span("hello", Span::new(1, 6));
    assert_eq!(s.value, "hello");
    assert_eq!(s.span, Span::new(1, 6));

    // take() extracts the value
    let s = Spanned::with_span(42, Span::new(0, 2));
    assert_eq!(s.take(), 42);

    // map() converts the inner value while preserving span
    let s = Spanned::with_span(42u32, Span::new(0, 2));
    let mapped: Spanned<u64> = s.map();
    assert_eq!(mapped.value, 42u64);
    assert_eq!(mapped.span, Span::new(0, 2));

    // PartialEq ignores span (custom behavior)
    let a = Spanned::with_span(42, Span::new(0, 2));
    let b = Spanned::with_span(42, Span::new(10, 12));
    assert_eq!(a, b);

    // PartialEq with different values
    assert_ne!(Spanned::new(1), Spanned::new(2));

    // PartialEq against raw value
    assert!(Spanned::new(42) == 42);

    // PartialOrd / Ord
    let a = Spanned::new(1);
    let b = Spanned::new(2);
    assert!(a < b);
    assert!(b > a);
    assert_eq!(a.cmp(&b), std::cmp::Ordering::Less);
    assert_eq!(a.cmp(&a), std::cmp::Ordering::Equal);

    // Default trait
    let def: Spanned<i32> = Spanned::default();
    assert_eq!(def.value, 0);
    assert!(def.span.is_empty());

    // AsRef trait
    let s = Spanned::with_span("hello", Span::new(0, 5));
    let r: &str = s.as_ref();
    assert_eq!(r, "hello");

    // Clone trait
    let s1 = Spanned::with_span(vec![1, 2, 3], Span::new(0, 5));
    let s2 = s1.clone();
    assert_eq!(s1.value, s2.value);
    assert_eq!(s1.span, s2.span);

    // Debug trait
    let s = Spanned::with_span(42, Span::new(0, 2));
    assert_eq!(format!("{:?}", s), "42");
}

#[test]
fn whitespace_grow_end_basic() {
    let text = b"key =   42";
    //           0123456789
    let mut span = Span::new(4, 5); // "="
    let b = span.whitespace_grow_end(text);
    assert_eq!(span.end, 8);
    assert_eq!(b, Some(b'4'));
}

#[test]
fn whitespace_grow_end_no_whitespace() {
    let text = b"key=42";
    let mut span = Span::new(3, 4); // "="
    let b = span.whitespace_grow_end(text);
    assert_eq!(span.end, 4);
    assert_eq!(b, Some(b'4'));
}

#[test]
fn whitespace_grow_end_eof() {
    let text = b"val   ";
    let mut span = Span::new(0, 3); // "val"
    let b = span.whitespace_grow_end(text);
    assert_eq!(span.end, 6);
    assert_eq!(b, None);
}

#[test]
fn whitespace_grow_end_tabs() {
    let text = b"val\t\t=";
    let mut span = Span::new(0, 3);
    let b = span.whitespace_grow_end(text);
    assert_eq!(span.end, 5);
    assert_eq!(b, Some(b'='));
}

#[test]
fn whitespace_grow_end_stops_at_newline() {
    let text = b"val  \nnext";
    let mut span = Span::new(0, 3);
    let b = span.whitespace_grow_end(text);
    assert_eq!(span.end, 5);
    assert_eq!(b, Some(b'\n'));
}

#[test]
fn whitespace_grow_start_basic() {
    let text = b"key   =   42";
    //           012345678901
    let mut span = Span::new(10, 12); // "42"
    let b = span.whitespace_grow_start(text);
    assert_eq!(span.start, 7);
    assert_eq!(b, Some(b'='));
}

#[test]
fn whitespace_grow_start_no_whitespace() {
    let text = b"key=42";
    let mut span = Span::new(4, 6);
    let b = span.whitespace_grow_start(text);
    assert_eq!(span.start, 4);
    assert_eq!(b, Some(b'='));
}

#[test]
fn whitespace_grow_start_bof() {
    let text = b"   val";
    let mut span = Span::new(3, 6);
    let b = span.whitespace_grow_start(text);
    assert_eq!(span.start, 0);
    assert_eq!(b, None);
}

#[test]
fn whitespace_grow_start_stops_at_newline() {
    let text = b"prev\n   val";
    let mut span = Span::new(8, 11);
    let b = span.whitespace_grow_start(text);
    assert_eq!(span.start, 5);
    assert_eq!(b, Some(b'\n'));
}

#[test]
fn comments_grow_end_basic() {
    let text = b"val = 1 # comment\nnext = 2";
    let mut span = Span::new(6, 7); // "1"
    let b = span.whitespace_and_comments_grow_end(text);
    assert_eq!(span.end, 18);
    assert_eq!(b, Some(b'n'));
}

#[test]
fn comments_grow_end_multiple_comment_lines() {
    let text = b"val = 1\n# line1\n# line2\nnext = 2";
    let mut span = Span::new(6, 7); // "1"
    let b = span.whitespace_and_comments_grow_end(text);
    assert_eq!(span.end, 24);
    assert_eq!(b, Some(b'n'));
}

#[test]
fn comments_grow_end_eof_in_comment() {
    let text = b"val = 1 # trailing";
    let mut span = Span::new(6, 7);
    let b = span.whitespace_and_comments_grow_end(text);
    assert_eq!(span.end, text.len() as u32);
    assert_eq!(b, None);
}

#[test]
fn comments_grow_end_blank_lines_and_comments() {
    let text = b"val = 1\n\n  \n# comment\n\nnext";
    let mut span = Span::new(6, 7);
    let b = span.whitespace_and_comments_grow_end(text);
    assert_eq!(b, Some(b'n'));
    assert_eq!(&text[span.end as usize..], b"next");
}

#[test]
fn comments_grow_end_no_comment() {
    let text = b"val = 1\nnext = 2";
    let mut span = Span::new(6, 7);
    let b = span.whitespace_and_comments_grow_end(text);
    assert_eq!(span.end, 8);
    assert_eq!(b, Some(b'n'));
}

#[test]
fn comments_grow_start_basic() {
    //                p r e v \n #  c o m m  e  n  t  \n  v  a  l
    let text = b"prev\n# comment\nval = 1";
    let mut span = Span::new(15, 18); // "val"
    let b = span.whitespace_and_comments_grow_start(text);
    // Skips \n, comment line, and \n after "prev"; stops at 'v' of "prev".
    assert_eq!(span.start, 4);
    assert_eq!(b, Some(b'v'));
}

#[test]
fn comments_grow_start_multiple_comment_lines() {
    let text = b"prev\n# c1\n  # c2\n  val = 1";
    let val_start = text.windows(3).position(|w| w == b"val").unwrap() as u32;
    let mut span = Span::new(val_start, val_start + 3);
    let b = span.whitespace_and_comments_grow_start(text);
    // Skips whitespace, both comment lines, and newline after "prev".
    assert_eq!(span.start, 4);
    assert_eq!(b, Some(b'v'));
}

#[test]
fn comments_grow_start_bof_comment() {
    let text = b"# comment\nval = 1";
    let val_start = text.iter().position(|&b| b == b'v').unwrap() as u32;
    let mut span = Span::new(val_start, val_start + 3);
    let b = span.whitespace_and_comments_grow_start(text);
    assert_eq!(span.start, 0);
    assert_eq!(b, None);
}

#[test]
fn comments_grow_start_no_comment() {
    let text = b"prev\n   val = 1";
    let val_start = text.windows(3).position(|w| w == b"val").unwrap() as u32;
    let mut span = Span::new(val_start, val_start + 3);
    let b = span.whitespace_and_comments_grow_start(text);
    // Skips spaces and the newline; stops at 'v' of "prev".
    assert_eq!(span.start, 4);
    assert_eq!(b, Some(b'v'));
}

#[test]
fn comments_grow_start_content_with_hash() {
    // A line with content before # should NOT be skipped.
    let text = b"key = 1 # inline\nval = 2";
    let val_start = text.windows(3).position(|w| w == b"val").unwrap() as u32;
    let mut span = Span::new(val_start, val_start + 3);
    let b = span.whitespace_and_comments_grow_start(text);
    // Skips the \n but stops at 'e' of "inline" (content line not skipped).
    assert_eq!(span.start, 16);
    assert_eq!(b, Some(b'e'));
}

#[test]
fn grow_key_to_header_simple() {
    let text = b"[server]\nhost = 1";
    let mut span = Span::new(1, 7); // "server"
    span.grow_key_to_header(text);
    assert_eq!(&text[span.range()], b"[server]");
}

#[test]
fn grow_key_to_header_dotted() {
    let text = b"[a.b.c]\nval = 1";
    // key "b" is at 3..4
    let mut span = Span::new(3, 4);
    span.grow_key_to_header(text);
    assert_eq!(&text[span.range()], b"[a.b.c]");
}

#[test]
fn grow_key_to_header_aot() {
    let text = b"[[pkg]]\nname = 1";
    let mut span = Span::new(2, 5); // "pkg"
    span.grow_key_to_header(text);
    assert_eq!(&text[span.range()], b"[[pkg]]");
}

#[test]
fn grow_key_to_header_spaced() {
    let text = b"[ a . b . c ]\nval = 1";
    // key "a" at 2..3
    let mut span = Span::new(2, 3);
    span.grow_key_to_header(text);
    assert_eq!(&text[span.range()], b"[ a . b . c ]");
}

#[test]
fn grow_key_to_header_quoted_with_bracket() {
    let text = b"[a.\"]key\".c]\nval = 1";
    // key "a" at 1..2
    let mut span = Span::new(1, 2);
    span.grow_key_to_header(text);
    assert_eq!(&text[span.range()], b"[a.\"]key\".c]");
}

#[test]
fn grow_key_to_header_literal_quoted() {
    let text = b"[a.']key'.c]\nval = 1";
    let mut span = Span::new(1, 2); // "a"
    span.grow_key_to_header(text);
    assert_eq!(&text[span.range()], b"[a.']key'.c]");
}

#[test]
fn grow_key_to_header_not_first_line() {
    let text = b"[a]\nval = 1\n[b.c]\nval = 2";
    // key "b" at 12..13 (inside [b.c])
    let mut span = Span::new(12, 13);
    span.grow_key_to_header(text);
    assert_eq!(&text[span.range()], b"[b.c]");
}

#[test]
fn extract_header_span_simple() {
    let text = b"[server]\nhost = 1\nport = 2";
    let header_span = Span::new(0, text.len() as u32);
    let hdr = header_span.extract_header_span(text);
    assert_eq!(&text[hdr.range()], b"[server]");
}

#[test]
fn extract_header_span_with_comment() {
    let text = b"[a] # comment\nval = 1";
    let header_span = Span::new(0, text.len() as u32);
    let hdr = header_span.extract_header_span(text);
    assert_eq!(&text[hdr.range()], b"[a]");
}

#[test]
fn extract_header_span_aot() {
    let text = b"[[fruit]]\nname = 1";
    let header_span = Span::new(0, text.len() as u32);
    let hdr = header_span.extract_header_span(text);
    assert_eq!(&text[hdr.range()], b"[[fruit]]");
}

#[test]
fn extract_header_span_dotted_header() {
    let text = b"[a.\" b\".c]\nval = 1";
    let header_span = Span::new(0, text.len() as u32);
    let hdr = header_span.extract_header_span(text);
    assert_eq!(&text[hdr.range()], b"[a.\" b\".c]");
}

#[test]
fn extract_header_span_empty_section() {
    let text = b"[a]\n[b]\nval = 1";
    // [a] is an empty header: span=0..4
    let header_span = Span::new(0, 4);
    let hdr = header_span.extract_header_span(text);
    assert_eq!(&text[hdr.range()], b"[a]");
}

#[cfg(feature = "from-toml")]
#[test]
fn spanned_deserialize() {
    let arena = crate::arena::Arena::new();
    let input = "v = 42";
    let mut root = crate::parser::parse(input, &arena).unwrap();
    let val: Spanned<i64> = {
        let mut helper = root.helper();
        helper.required("v").unwrap()
    };
    assert_eq!(val.value, 42);
    assert_eq!(&input[val.span.start as usize..val.span.end as usize], "42");
}
