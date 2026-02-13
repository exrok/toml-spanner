use super::*;
use crate::arena::Arena;
use crate::str::Str;
use crate::Span;

fn sp() -> Span {
    Span::new(0, 0)
}

fn ival(i: i64) -> Item<'static> {
    Item::integer(i, sp())
}

// -- Empty array ------------------------------------------------------------

#[test]
fn new_empty_drop() {
    let a = Array::new();
    assert!(a.is_empty());
    assert_eq!(a.len(), 0);
}

#[test]
fn default_is_empty() {
    let a = Array::default();
    assert!(a.is_empty());
}

// -- with_capacity / with_single -------------------------------------------

#[test]
fn with_capacity_zero() {
    let arena = Arena::new();
    let a = Array::with_capacity(0, &arena);
    assert!(a.is_empty());
}

#[test]
fn with_capacity_nonzero() {
    let arena = Arena::new();
    let a = Array::with_capacity(8, &arena);
    assert!(a.is_empty());
}

#[test]
fn with_single_value() {
    let arena = Arena::new();
    let a = Array::with_single(ival(42), &arena);
    assert_eq!(a.len(), 1);
    assert_eq!(a.get(0).unwrap().as_integer(), Some(42));
}

// -- Push + allocation thresholds -------------------------------------------

#[test]
fn push_first_triggers_alloc() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(1), &arena);
    assert_eq!(a.len(), 1);
    assert_eq!(a.get(0).unwrap().as_integer(), Some(1));
}

#[test]
fn push_to_capacity_4() {
    let arena = Arena::new();
    let mut a = Array::new();
    for i in 0..4 {
        a.push(ival(i), &arena);
    }
    assert_eq!(a.len(), 4);
    for i in 0..4 {
        assert_eq!(a.get(i).unwrap().as_integer(), Some(i as i64));
    }
}

#[test]
fn push_beyond_4_realloc() {
    let arena = Arena::new();
    let mut a = Array::new();
    for i in 0..5 {
        a.push(ival(i), &arena);
    }
    assert_eq!(a.len(), 5);
    for i in 0..5 {
        assert_eq!(a.get(i).unwrap().as_integer(), Some(i as i64));
    }
}

#[test]
fn push_beyond_8_realloc() {
    let arena = Arena::new();
    let mut a = Array::new();
    for i in 0..9 {
        a.push(ival(i), &arena);
    }
    assert_eq!(a.len(), 9);
    for i in 0..9 {
        assert_eq!(a.get(i).unwrap().as_integer(), Some(i as i64));
    }
}

// -- get / get_mut ----------------------------------------------------------

#[test]
fn get_out_of_bounds() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(1), &arena);
    assert!(a.get(0).is_some());
    assert!(a.get(1).is_none());
    assert!(a.get(100).is_none());
}

#[test]
fn get_mut_modifies() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(10), &arena);
    a.push(ival(20), &arena);
    let v = a.get_mut(0).unwrap();
    if let crate::value::ValueMut::Integer(i) = v.as_mut() {
        *i = 99;
    }
    assert_eq!(a.get(0).unwrap().as_integer(), Some(99));
    assert_eq!(a.get(1).unwrap().as_integer(), Some(20));
}

#[test]
fn get_mut_out_of_bounds() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(1), &arena);
    assert!(a.get_mut(0).is_some());
    assert!(a.get_mut(1).is_none());
}

// -- pop --------------------------------------------------------------------

#[test]
fn pop_returns_last() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    a.push(ival(3), &arena);
    let v = a.pop().unwrap();
    assert_eq!(v.as_integer(), Some(3));
    assert_eq!(a.len(), 2);
}

#[test]
fn pop_to_empty() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    assert_eq!(a.pop().unwrap().as_integer(), Some(2));
    assert_eq!(a.pop().unwrap().as_integer(), Some(1));
    assert!(a.pop().is_none());
    assert!(a.is_empty());
}

// -- last_mut ---------------------------------------------------------------

#[test]
fn last_mut_empty() {
    let mut a = Array::new();
    assert!(a.last_mut().is_none());
}

#[test]
fn last_mut_modify() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    let last = a.last_mut().unwrap();
    if let crate::value::ValueMut::Integer(i) = last.as_mut() {
        *i = 99;
    }
    assert_eq!(a.get(1).unwrap().as_integer(), Some(99));
}

// -- as_slice / as_mut_slice ------------------------------------------------

#[test]
fn as_slice_empty() {
    let a = Array::new();
    assert!(a.as_slice().is_empty());
}

#[test]
fn as_slice_contents() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(10), &arena);
    a.push(ival(20), &arena);
    let s = a.as_slice();
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].as_integer(), Some(10));
    assert_eq!(s[1].as_integer(), Some(20));
}

#[test]
fn as_mut_slice_modify() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    let s = a.as_mut_slice();
    if let crate::value::ValueMut::Integer(i) = s[0].as_mut() {
        *i = 100;
    }
    assert_eq!(a.get(0).unwrap().as_integer(), Some(100));
}

// -- Iterators --------------------------------------------------------------

#[test]
fn iter_ref() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    a.push(ival(3), &arena);
    let vals: Vec<i64> = a.iter().map(|v| v.as_integer().unwrap()).collect();
    assert_eq!(vals, vec![1, 2, 3]);
}

#[test]
fn into_iter_full() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(10), &arena);
    a.push(ival(20), &arena);
    a.push(ival(30), &arena);
    let vals: Vec<i64> = a.into_iter().map(|v| v.as_integer().unwrap()).collect();
    assert_eq!(vals, vec![10, 20, 30]);
}

#[test]
fn into_iter_partial_drop() {
    let arena = Arena::new();
    let mut a = Array::new();
    for i in 0..5 {
        a.push(ival(i), &arena);
    }
    let mut iter = a.into_iter();
    assert_eq!(iter.next().unwrap().as_integer(), Some(0));
    assert_eq!(iter.next().unwrap().as_integer(), Some(1));
    drop(iter);
}

#[test]
fn into_iter_empty() {
    let a = Array::new();
    let mut iter = a.into_iter();
    assert!(iter.next().is_none());
}

#[test]
fn into_iter_size_hint() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    a.push(ival(3), &arena);
    let mut iter = a.into_iter();
    assert_eq!(iter.size_hint(), (3, Some(3)));
    iter.next();
    assert_eq!(iter.size_hint(), (2, Some(2)));
}

// -- Drop with heap-owning values -------------------------------------------

#[test]
fn drop_with_owned_strings() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(Item::string(Str::from("owned1"), sp()), &arena);
    a.push(Item::string(Str::from("owned2"), sp()), &arena);
    a.push(Item::string(Str::from("owned3"), sp()), &arena);
}

#[test]
fn drop_with_nested_arrays() {
    let arena = Arena::new();
    let mut inner = Array::new();
    inner.push(ival(1), &arena);
    inner.push(ival(2), &arena);
    let mut outer = Array::new();
    outer.push(Item::array(inner, sp()), &arena);
    outer.push(ival(3), &arena);
}

// -- Debug ------------------------------------------------------------------

#[test]
fn debug_format() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    let s = format!("{a:?}");
    assert!(s.contains('1'));
    assert!(s.contains('2'));
}
