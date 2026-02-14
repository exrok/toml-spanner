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
fn spanned_deserialize() {
    let arena = crate::arena::Arena::new();
    let input = "v = 42";
    let mut table = crate::parse(input, &arena).unwrap();
    let val: Spanned<i64> = table.required("v").unwrap();
    assert_eq!(val.value, 42);
    assert_eq!(&input[val.span.start as usize..val.span.end as usize], "42");
}
