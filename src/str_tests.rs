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
    assert_eq!(s.str_len(), 0);
    assert!(!s.is_owned());
}

#[test]
fn borrowed_normal() {
    let s = Str::from("hello");
    assert_eq!(&*s, "hello");
    assert_eq!(s.str_len(), 5);
    assert!(!s.is_owned());
}

#[test]
fn owned_from_string() {
    let s = Str::from(String::from("world"));
    assert_eq!(&*s, "world");
    assert!(s.is_owned());
    assert_eq!(s.str_len(), 5);
}

#[test]
fn owned_empty() {
    let s = Str::from(String::from(""));
    assert_eq!(&*s, "");
    assert!(s.is_owned());
    assert_eq!(s.str_len(), 0);
}

#[test]
fn clone_borrowed() {
    let orig = Str::from("borrowed");
    let cloned = orig.clone();
    assert_eq!(&*orig, &*cloned);
    assert!(!orig.is_owned());
    assert!(!cloned.is_owned());
}

#[test]
fn clone_owned() {
    let orig = Str::from(String::from("owned"));
    let cloned = orig.clone();
    assert_eq!(&*orig, &*cloned);
    assert!(orig.is_owned());
    assert!(cloned.is_owned());
}

#[test]
fn drop_owned() {
    let s = Str::from(String::from("will be freed"));
    drop(s);
}

#[test]
fn into_boxed_str_borrowed() {
    let s = Str::from("borrow me");
    let boxed = s.into_boxed_str();
    assert_eq!(&*boxed, "borrow me");
}

#[test]
fn into_boxed_str_owned() {
    let s = Str::from(String::from("own me"));
    let boxed = s.into_boxed_str();
    assert_eq!(&*boxed, "own me");
}

#[test]
fn from_cow_borrowed() {
    let cow: Cow<'_, str> = Cow::Borrowed("cow borrow");
    let s = Str::from(cow);
    assert_eq!(&*s, "cow borrow");
    assert!(!s.is_owned());
}

#[test]
fn from_cow_owned() {
    let cow: Cow<'_, str> = Cow::Owned(String::from("cow owned"));
    let s = Str::from(cow);
    assert_eq!(&*s, "cow owned");
    assert!(s.is_owned());
}

#[test]
fn into_cow_borrowed() {
    let s = Str::from("to cow");
    let cow: Cow<'_, str> = Cow::from(s);
    assert!(matches!(cow, Cow::Borrowed("to cow")));
}

#[test]
fn into_cow_owned() {
    let s = Str::from(String::from("to cow owned"));
    let cow: Cow<'_, str> = Cow::from(s);
    assert_eq!(&*cow, "to cow owned");
    assert!(matches!(cow, Cow::Owned(_)));
}

#[test]
fn into_string_owned() {
    let s = Str::from(String::from("to string"));
    let string: String = String::from(s);
    assert_eq!(string, "to string");
}

#[test]
fn into_string_borrowed() {
    let s = Str::from("to string borrow");
    let string: String = String::from(s);
    assert_eq!(string, "to string borrow");
}

#[test]
fn into_box_str_owned() {
    let s = Str::from(String::from("to box"));
    let boxed: Box<str> = Box::from(s);
    assert_eq!(&*boxed, "to box");
}

#[test]
fn into_box_str_borrowed() {
    let s = Str::from("to box borrow");
    let boxed: Box<str> = Box::from(s);
    assert_eq!(&*boxed, "to box borrow");
}

#[test]
fn equality_across_ownership() {
    let borrowed = Str::from("same");
    let owned = Str::from(String::from("same"));
    assert_eq!(borrowed, owned);
    assert_eq!(hash_of(&borrowed), hash_of(&owned));
}

#[test]
fn default_is_empty() {
    let s = Str::default();
    assert_eq!(&*s, "");
    assert!(!s.is_owned());
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
