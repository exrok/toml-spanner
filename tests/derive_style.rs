use toml_spanner_macros::Toml;

// --- Style attribute tests ---

#[derive(Toml, Debug, PartialEq)]
#[toml(Toml)]
struct StyleInner {
    a: u32,
    b: u32,
}

#[derive(Toml, Debug, PartialEq)]
#[toml(ToToml)]
struct StyleOuter {
    #[toml(style = Header)]
    header: StyleInner,
    #[toml(style = Inline)]
    inline: StyleInner,
}

#[test]
fn style_header_and_inline() {
    let v = StyleOuter {
        header: StyleInner { a: 0, b: 1 },
        inline: StyleInner { a: 2, b: 3 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(
        s, "inline = { a = 2, b = 3 }\n\n[header]\na = 0\nb = 1\n",
        "got:\n{s}"
    );
}

#[test]
fn style_inline_only() {
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct Wrapper {
        #[toml(style = Inline)]
        inner: StyleInner,
    }
    let v = Wrapper {
        inner: StyleInner { a: 10, b: 20 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "inner = { a = 10, b = 20 }\n");
}

#[test]
fn style_header_only() {
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct Wrapper {
        #[toml(style = Header)]
        inner: StyleInner,
    }
    let v = Wrapper {
        inner: StyleInner { a: 5, b: 6 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "[inner]\na = 5\nb = 6\n");
}

#[derive(Toml, Debug, PartialEq)]
#[toml(Toml)]
struct StyleRoundTrip {
    name: String,
    #[toml(style = Inline)]
    data: StyleInner,
}

#[test]
fn style_inline_roundtrip() {
    let v = StyleRoundTrip {
        name: "test".to_string(),
        data: StyleInner { a: 1, b: 2 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "name = \"test\"\ndata = { a = 1, b = 2 }\n");
    let restored: StyleRoundTrip = toml_spanner::from_str(&s).unwrap();
    assert_eq!(v, restored);
}

#[test]
fn style_with_option_some() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(Toml)]
    struct WithOpt {
        #[toml(style = Inline)]
        item: Option<StyleInner>,
    }
    let v = WithOpt {
        item: Some(StyleInner { a: 3, b: 4 }),
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "item = { a = 3, b = 4 }\n");
    let restored: WithOpt = toml_spanner::from_str(&s).unwrap();
    assert_eq!(v, restored);
}

#[test]
fn style_with_option_none() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(Toml)]
    struct WithOpt {
        #[toml(style = Inline)]
        item: Option<StyleInner>,
    }
    let v = WithOpt { item: None };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "");
}

#[test]
fn style_with_default() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(Toml)]
    struct WithDefault {
        name: String,
        #[toml(default, style = Inline)]
        data: Option<StyleInner>,
    }
    let input = r#"name = "hello""#;
    let v: WithDefault = toml_spanner::from_str(input).unwrap();
    assert_eq!(v.data, None);

    let v2 = WithDefault {
        name: "hello".to_string(),
        data: Some(StyleInner { a: 1, b: 2 }),
    };
    let s = toml_spanner::to_string(&v2).unwrap();
    assert_eq!(s, "name = \"hello\"\ndata = { a = 1, b = 2 }\n");
}

#[test]
fn style_with_vec_header() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(ToToml)]
    struct WithVec {
        #[toml(style = Header)]
        items: Vec<StyleInner>,
    }
    let v = WithVec {
        items: vec![StyleInner { a: 1, b: 2 }, StyleInner { a: 3, b: 4 }],
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "[[items]]\na = 1\nb = 2\n\n[[items]]\na = 3\nb = 4\n");
}

#[test]
fn style_with_vec_inline() {
    #[derive(Toml, Debug, PartialEq)]
    #[toml(ToToml)]
    struct WithVec {
        #[toml(style = Inline)]
        items: Vec<StyleInner>,
    }
    let v = WithVec {
        items: vec![StyleInner { a: 1, b: 2 }, StyleInner { a: 3, b: 4 }],
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "items = [{ a = 1, b = 2 }, { a = 3, b = 4 }]\n");
}

#[test]
fn style_multiple_headers() {
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct Multi {
        x: u32,
        #[toml(style = Header)]
        first: StyleInner,
        #[toml(style = Header)]
        second: StyleInner,
    }
    let v = Multi {
        x: 42,
        first: StyleInner { a: 1, b: 2 },
        second: StyleInner { a: 3, b: 4 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(
        s,
        "x = 42\n\n[first]\na = 1\nb = 2\n\n[second]\na = 3\nb = 4\n"
    );
}

#[test]
fn style_with_rename() {
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct Renamed {
        #[toml(rename = "my-section", style = Header)]
        section: StyleInner,
    }
    let v = Renamed {
        section: StyleInner { a: 7, b: 8 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "[my-section]\na = 7\nb = 8\n");
}

#[test]
fn style_combined_with_with_attr() {
    use toml_spanner::helper::flatten_any;

    #[derive(Toml, Debug, PartialEq)]
    #[toml(Toml)]
    struct FlatInner {
        x: u32,
        y: u32,
    }

    #[derive(Toml, Debug, PartialEq)]
    #[toml(Toml)]
    struct WithFlatStyle {
        name: String,
        #[toml(flatten, with = flatten_any)]
        nested: FlatInner,
    }

    let v = WithFlatStyle {
        name: "test".to_string(),
        nested: FlatInner { x: 10, y: 20 },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert!(s.contains("name = \"test\""), "got:\n{s}");
    assert!(s.contains("x = 10"), "got:\n{s}");
    assert!(s.contains("y = 20"), "got:\n{s}");
}

// --- Nested style tests ---
//
// All tests serialize the same logical data: a.b.c.d = 4
// Different style combinations at each nesting level produce
// different TOML representations.
//
// Key insight: Implicit (default) tables are transparent pass-throughs
// that merge into child header paths. Header tables always emit their
// own [section] line. Dotted tables render as dotted key prefixes.
// Inline tables render as { ... }.

#[derive(Toml, Debug)]
#[toml(ToToml)]
struct Leaf {
    d: u32,
}

fn leaf() -> Leaf {
    Leaf { d: 4 }
}

#[test]
fn nested_style_implicit_implicit_header() {
    // Implicit a and b merge into c's header path
    // [a.b.c]
    // d = 4
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct C {
        #[toml(style = Header)]
        c: Leaf,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct B {
        b: C,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct Root {
        a: B,
    }

    let v = Root {
        a: B { b: C { c: leaf() } },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "[a.b.c]\nd = 4\n", "got:\n{s}");
}

#[test]
fn nested_style_implicit_header_dotted() {
    // Implicit a merges into b's header, dotted c renders as key prefix
    // [a.b]
    // c.d = 4
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct C {
        #[toml(style = Dotted)]
        c: Leaf,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct B {
        #[toml(style = Header)]
        b: C,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct Root {
        a: B,
    }

    let v = Root {
        a: B { b: C { c: leaf() } },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "[a.b]\nc.d = 4\n", "got:\n{s}");
}

#[test]
fn nested_style_implicit_header_inline() {
    // Implicit a merges into b's header, inline c renders as { ... }
    // [a.b]
    // c = { d = 4 }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct C {
        #[toml(style = Inline)]
        c: Leaf,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct B {
        #[toml(style = Header)]
        b: C,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct Root {
        a: B,
    }

    let v = Root {
        a: B { b: C { c: leaf() } },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "[a.b]\nc = { d = 4 }\n", "got:\n{s}");
}

#[test]
fn nested_style_header_dotted_inline() {
    // Header a gets [a], dotted b is a key prefix, inline c is { ... }
    // [a]
    // b.c = { d = 4 }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct C {
        #[toml(style = Inline)]
        c: Leaf,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct B {
        #[toml(style = Dotted)]
        b: C,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct Root {
        #[toml(style = Header)]
        a: B,
    }

    let v = Root {
        a: B { b: C { c: leaf() } },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "[a]\nb.c = { d = 4 }\n", "got:\n{s}");
}

#[test]
fn nested_style_header_inline_dotted() {
    // Header a gets [a], inline b is { ... }, dotted c inside inline
    // [a]
    // b = { c.d = 4 }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct C {
        #[toml(style = Dotted)]
        c: Leaf,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct B {
        #[toml(style = Inline)]
        b: C,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct Root {
        #[toml(style = Header)]
        a: B,
    }

    let v = Root {
        a: B { b: C { c: leaf() } },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "[a]\nb = { c.d = 4 }\n", "got:\n{s}");
}

#[test]
fn nested_style_header_inline_inline() {
    // Header a gets [a], inline b and c nest as { ... { ... } }
    // [a]
    // b = { c = { d = 4 } }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct C {
        #[toml(style = Inline)]
        c: Leaf,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct B {
        #[toml(style = Inline)]
        b: C,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct Root {
        #[toml(style = Header)]
        a: B,
    }

    let v = Root {
        a: B { b: C { c: leaf() } },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "[a]\nb = { c = { d = 4 } }\n", "got:\n{s}");
}

#[test]
fn nested_style_all_dotted() {
    // All dotted, everything collapses to a single dotted key
    // a.b.c.d = 4
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct C {
        #[toml(style = Dotted)]
        c: Leaf,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct B {
        #[toml(style = Dotted)]
        b: C,
    }
    #[derive(Toml, Debug)]
    #[toml(ToToml)]
    struct Root {
        #[toml(style = Dotted)]
        a: B,
    }

    let v = Root {
        a: B { b: C { c: leaf() } },
    };
    let s = toml_spanner::to_string(&v).unwrap();
    assert_eq!(s, "a.b.c.d = 4\n", "got:\n{s}");
}
