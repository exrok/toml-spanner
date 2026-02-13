use integ_tests::{invalid, valid};

valid!(fruit);
valid!(strings);
valid!(empty_table, "\n[foo]\n");
valid!(tables_in_arrays);
valid!(blank_literal, "foo = ''");
// Normally this would be in a file, but it's inlined to avoid git shenanigans
valid!(
    crlf,
    "\
[project]\r\n\
\r\n\
name = \"splay\"\r\n\
version = \"0.1.0\"\r\n\
authors = [\"alex@crichton.co\"]\r\n\
\r\n\
[[lib]]\r\n\
\r\n\
path = \"lib.rs\"\r\n\
name = \"splay\"\r\n\
description = \"\"\"\
A Rust implementation of a TAR file reader and writer. This library does not\r\n\
currently handle compression, but it is abstract over all I/O readers and\r\n\
writers. Additionally, great lengths are taken to ensure that the entire\r\n\
contents are never required to be entirely resident in memory all at once.\r\n\
\"\"\"\
"
);
valid!(many_blank, "foo = \"\"\"\n\n\n\"\"\"");
valid!(
    literal_eats_crlf,
    "foo = \"\"\"\\\r\n\"\"\"
bar = \"\"\"\\\r\n   \r\n   \r\n   a\"\"\""
);
invalid!(newline_string, "a = \"\n\"");
invalid!(newline_literal, "a = '\n'");
valid!(key_names);
// Ensures that a table with an array cannot then be followed by an array of the
// same name
invalid!(
    table_array_implicit,
    r#"
[[albums.songs]]
name = "Glory Days"

[[albums]]
name = "Born in the USA"
"#
);

valid!(table_9_valid);
valid!(table_9_reverse_valid);
invalid!(table_9_invalid);
invalid!(table_9_reverse_invalid);
valid!(array_within_dotted);

mod stray_cr {
    use super::invalid;

    invalid!(single, "\r");
    invalid!(array_value, "a = [ \r ]");
    invalid!(ml_basic, "\"\"\"\r\"\"\"");
    invalid!(ml_more_whitespace, "\"\"\"  \r  \"\"\"");
    invalid!(ml_basic2, "'''\r'''");
    invalid!(value_literal, "a = '\r'");
    invalid!(value_str, "a = \"\r\"");
}

mod bad_leading_zeros {
    use super::invalid;

    invalid!(two_zeros, "a = 00");
    invalid!(neg_two_zeros, "a = -00");
    invalid!(pos_two_zeros, "a = +00");
    invalid!(with_dec, "a = 00.0");
    invalid!(neg_with_dec, "a = -00.0");
    invalid!(pos_with_dec, "a = +00.0");
}

mod signed_radix {
    use super::invalid;

    invalid!(plus_bin, "a = +0b0");
    invalid!(plus_oct, "a = +0o0");
    invalid!(plus_hex, "a = +0x0");
    invalid!(neg_bin, "a = -0b0");
    invalid!(neg_oct, "a = -0o0");
    invalid!(neg_hex, "a = -0x0");
}

mod bad_floats {
    use super::invalid;

    invalid!(trailing_dec, "a = 0.");
    invalid!(trailing_exp, "a = 0.e");
    invalid!(trailing_exp2, "a = 0.E");
    invalid!(trailing_exp3, "a = 0.0E");
    invalid!(trailing_exp4, "a = 0.0e");
    invalid!(trailing_neg, "a = 0.0e-");
    invalid!(trailing_pos, "a = 0.0e+");
    invalid!(no_digit_before_dot_neg, "a = -.7");
    invalid!(no_digit_before_dot_pos, "a = +.7");
    invalid!(no_digit_after_neg, "a = -_2");
    invalid!(no_digit_after_pos, "a = +_2");
    invalid!(underscore_before_exp, "a = 73_e3");
    invalid!(underscore_after_exp, "a = 73e_3");
    invalid!(underscore_before_dot, "a = 73_.5");
    invalid!(underscore_after_dot, "a = 1._5");
}

mod bad_keys {
    use super::invalid;

    invalid!(newline, "key\n=3");
    invalid!(newline_after_equal, "key=\n3");
    invalid!(pipe, "key|=3");
    invalid!(none, "=3");
    invalid!(empty_pipe, "\"\"|=3");
    invalid!(newline2, "\"\n\"|=3");
    invalid!(newline3, "\"something\nsomething else\"=3");
    invalid!(cr, "\"\r\"|=3");
    invalid!(mutli_line, "''''''=3");
    invalid!(multi_line2, r#"""""""=3"#);
    invalid!(multi_line3, "'''key'''=3");
    invalid!(multi_line4, r#""""key"""=3"#);
    invalid!(dotted, "a = 1\na.b = 2");
}

valid!(table_names);

mod bad_table_names {
    use super::invalid;

    invalid!(empty, "[]");
    invalid!(period, "[.]");
    invalid!(trailing_period, "[a.]");
    invalid!(exclamation, "[!]");
    invalid!(newline, "[\"\n\"]");
    invalid!(redefinition, "[a.b]\n[a.\"b\"]");
    invalid!(unterminated_one, "[']");
    invalid!(unterminated_three, "[''']");
    invalid!(multi_empty, "['''''']");
    invalid!(multi_foo, "['''foo''']");
    invalid!(multi_bar, r#"["""bar"""]"#);
    invalid!(newline_literal, "['\n']");
    invalid!(crlf_literal, "['\r\n']");
}

// Outside the positive range of an i64
invalid!(integer_range_positive, "a = 9223372036854775808");
// Outside negative range of an i64
invalid!(integer_range_negative, "a = -9223372036854775809");
invalid!(bare_number, "4");

valid!(inline_tables);
valid!(inline_tables_multiline);
valid!(trailing_comma, "a = {a=1,}");
invalid!(eof, "key =");

mod bad_inline_tables {
    use super::invalid;

    invalid!(only_comma, "a = {,}");
    invalid!(duplicate_key, "a = {a=1,a=1}");
    // Note: newlines inside inline tables are now valid in TOML 1.1.0
    invalid!(eof, "a = {");
}

valid!(underscores);

mod bad_underscores {
    use super::invalid;

    invalid!(trailing, "foo = 0_");
    invalid!(trailing2, "foo = 1_0_");
    invalid!(double, "foo = 0__0");
    invalid!(double_leading, "foo = __0");
}

invalid!(bad_codepoint, "foo = \"\\uD800\"");
valid!(empty_string, r#"foo = """#);
valid!(key_no_space, "foo=42");

mod bad_strings {
    use super::invalid;

    invalid!(hex, "foo = \"\\uxx\"");
    invalid!(hex2, "foo = \"\\u\"");
    invalid!(unterminated, r#"foo = "\"#);
    invalid!(unterminated_literal, "foo = '");
}

valid!(booleans);

mod bad_booleans {
    use super::invalid;

    invalid!(true_trailing, "foo = true2");
    invalid!(false_trailing, "foo = false2");
    invalid!(leading_t, "foo = t2");
    invalid!(leading_f, "foo = f2");
}

mod bad_nesting {
    use super::invalid;

    invalid!(
        key_then_array,
        "
        a = [2]
        [[a]]
        b = 5
        "
    );
    invalid!(
        key_then_dotted,
        "
        a = 1
        [a.b]
        "
    );
    invalid!(
        array_then_dotted,
        "
        a = []
        [a.b]
        "
    );
    invalid!(
        array_then_dotted_array,
        "
        a = []
        [[a.b]]
        "
    );
    invalid!(
        inline,
        "
        [a]
        b = { c = 2, d = {} }
        [a.b]
        c = 2
        "
    );
}

mod redefine {
    use super::invalid;

    invalid!(
        table_then_dotted_then_table_again,
        r#"
[a]
foo="bar"
[a.b]
foo="bar"
[a]
"#
    );
    invalid!(
        table_then_table,
        r#"
[a]
foo="bar"
b = { foo = "bar" }
[a]
"#
    );
    invalid!(
        table_then_dotted,
        "
        [a]
        b = {}
        [a.b]
        "
    );
    invalid!(
        table_then_inline,
        "
        [a]
        b = {}
        [a]
        "
    );
}

mod datetimes {
    use super::invalid;

    invalid!(utc, "utc = 2016-09-09T09:09:09Z");
    invalid!(utc_punkt, "utc = 2016-09-09T09:09:09.1Z");
    invalid!(tz, "tz = 2016-09-09T09:09:09.2+10:00");
    invalid!(tz_neg, "tz = 2016-09-09T09:09:09.123456789-02:00");
    invalid!(utc_trailing_dot, "utc = 2016-09-09T09:09:09.Z");
    invalid!(utc_invalid, "utc = 2016-9-09T09:09:09Z");
    invalid!(tz2, "tz = 2016-09-09T09:09:09+2:00");
    invalid!(tz_neg2, "tz = 2016-09-09T09:09:09-2:00");
    invalid!(tz_neg3, "tz = 2016-09-09T09:09:09Z-2:00");
}

mod require_newlines {
    use super::invalid;

    invalid!(basic, "0=0r=false");
    invalid!(
        strings,
        r#"
0=""o=""m=""r=""00="0"q="""0"""e="""0"""
"#
    );
    invalid!(
        tables,
        r#"
[[0000l0]]
0="0"[[0000l0]]
0="0"[[0000l0]]
0="0"l="0"
"#
    );
    invalid!(
        arrays,
        r#"
0=[0]00=[0,0,0]t=["0","0","0"]s=[1000-00-00T00:00:00Z,2000-00-00T00:00:00Z]
"#
    );
    invalid!(basic2, "0=0r0=0r=false");
    invalid!(basic3, "0=0r0=0r=falsefal=false");
}

// TOML 1.1 features
mod toml_1_1 {
    use super::valid;

    // Inline tables can now have newlines
    valid!(
        inline_table_newlines,
        r#"
tbl = {
    key = "value",
    nested = {
        inner = 42,
    },
}
"#
    );

    // Trailing commas in inline tables
    valid!(inline_table_trailing_comma, "a = {a=1,}");

    // Empty inline table with newlines
    valid!(inline_table_empty_newlines, "a = {\n}");

    // \e escape sequence (escape character U+001B)
    valid!(escape_e, r#"csi = "\e[""#);

    // \xHH escape sequence for codepoints < 255
    valid!(
        escape_x,
        r#"
null = "\x00"
letter = "\x61"
"#
    );

    // Combined escape sequences
    valid!(
        escape_combined,
        r#"test = "\x00\x1b\e\x61\u0041\U00000042""#
    );

    // Complex inline table with comments
    valid!(
        inline_table_with_comments,
        r#"
tbl = {
    # This is a comment
    key = "value",
    # Another comment
    num = 123,
}
"#
    );
}
