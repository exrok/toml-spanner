use super::*;
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn hash_of(s: &Str<'_>) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[test]
fn basics() {
    let empty = Str::from("");
    assert_eq!(&*empty, "");
    assert_eq!(empty.len(), 0);

    let s = Str::from("hello");
    assert_eq!(&*s, "hello");
    assert_eq!(s.len(), 5);

    let def = Str::default();
    assert_eq!(&*def, "");
    assert_eq!(def.len(), 0);
}

#[test]
fn conversions() {
    let s = Str::from("convert me");

    let boxed = s.into_boxed_str();
    assert_eq!(&*boxed, "convert me");

    let cow: Cow<'_, str> = Cow::from(s);
    assert!(matches!(cow, Cow::Borrowed("convert me")));

    let string: String = String::from(s);
    assert_eq!(string, "convert me");

    let boxed2: Box<str> = Box::from(s);
    assert_eq!(&*boxed2, "convert me");
}

#[test]
fn equality_and_ordering() {
    let a = Str::from("same");
    let b = Str::from("same");
    assert_eq!(a, b);
    assert_eq!(hash_of(&a), hash_of(&b));

    let c = Str::from("different");
    assert_ne!(a, c);

    assert_eq!(a, *"same");
    assert!(a == "same");

    let x = Str::from("aaa");
    let y = Str::from("bbb");
    assert!(x < y);
    assert!(y > x);
    assert_eq!(x.cmp(&x), std::cmp::Ordering::Equal);
}
