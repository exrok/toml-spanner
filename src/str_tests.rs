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
fn borrowed_empty() {
    let s = Str::from("");
    assert_eq!(&*s, "");
    assert_eq!(s.len(), 0);
}

#[test]
fn borrowed_normal() {
    let s = Str::from("hello");
    assert_eq!(&*s, "hello");
    assert_eq!(s.len(), 5);
}

#[test]
fn copy_semantics() {
    let a = Str::from("copy me");
    let b = a; // Copy
    assert_eq!(&*a, "copy me");
    assert_eq!(&*b, "copy me");
    assert_eq!(&*a, &*b);
}

#[test]
fn clone_borrowed() {
    let orig = Str::from("borrowed");
    let cloned = orig.clone();
    assert_eq!(&*orig, &*cloned);
}

#[test]
fn into_boxed_str() {
    let s = Str::from("borrow me");
    let boxed = s.into_boxed_str();
    assert_eq!(&*boxed, "borrow me");
}

#[test]
fn into_cow_borrowed() {
    let s = Str::from("to cow");
    let cow: Cow<'_, str> = Cow::from(s);
    assert!(matches!(cow, Cow::Borrowed("to cow")));
}

#[test]
fn into_string() {
    let s = Str::from("to string");
    let string: String = String::from(s);
    assert_eq!(string, "to string");
}

#[test]
fn into_box_str() {
    let s = Str::from("to box");
    let boxed: Box<str> = Box::from(s);
    assert_eq!(&*boxed, "to box");
}

#[test]
fn equality() {
    let a = Str::from("same");
    let b = Str::from("same");
    assert_eq!(a, b);
    assert_eq!(hash_of(&a), hash_of(&b));
}

#[test]
fn default_is_empty() {
    let s = Str::default();
    assert_eq!(&*s, "");
}

#[test]
fn partial_eq_str() {
    let s = Str::from("hello");
    assert_eq!(s, *"hello");
    assert!(s == "hello");
}

#[test]
fn ord_consistency() {
    let a = Str::from("aaa");
    let b = Str::from("bbb");
    assert!(a < b);
    assert!(b > a);
    assert_eq!(a.cmp(&a), std::cmp::Ordering::Equal);
}

#[test]
fn display_and_debug() {
    let s = Str::from("test");
    assert_eq!(format!("{s}"), "test");
    assert_eq!(format!("{s:?}"), "\"test\"");
}
