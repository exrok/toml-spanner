use super::*;
use crate::Span;
use crate::arena::Arena;

fn sp() -> Span {
    Span::new(0, 0)
}

fn ival(i: i64) -> Item<'static> {
    Item::integer_spanned(i, sp())
}

#[test]
fn default_impl() {
    // Array::default() should work like Array::new()
    let arena = Arena::new();
    let mut a = InternalArray::default();
    assert!(a.is_empty());
    a.push(ival(1), &arena);
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].as_i64(), Some(1));
}

#[test]
fn push_and_realloc() {
    let arena = Arena::new();

    // with_single creates a one-element array
    let a = InternalArray::with_single(ival(42), &arena);
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].as_i64(), Some(42));

    // Push from empty through all realloc thresholds (0 -> 4 -> 8 -> beyond)
    let mut a = InternalArray::new();
    assert!(a.is_empty());
    assert_eq!(a.len(), 0);

    for i in 0..9_i64 {
        a.push(ival(i), &arena);
        assert_eq!(a.len(), i as usize + 1);
    }

    // Verify all values survived reallocs
    for i in 0..9 {
        assert_eq!(a[i].as_i64(), Some(i as i64));
    }
}

#[test]
fn get_and_get_mut() {
    let arena = Arena::new();
    let mut a = InternalArray::new();
    a.push(ival(10), &arena);
    a.push(ival(20), &arena);

    // get: valid and out of bounds
    assert_eq!(a[0].as_i64(), Some(10));
    assert_eq!(a[1].as_i64(), Some(20));
    assert!(a.get(2).is_none());
    assert!(a.get(100).is_none());

    // get_mut: modify first element
    let v = a.get_mut(0).unwrap();
    if let crate::item::ValueMut::Integer(i) = v.value_mut() {
        *i = 99;
    }
    assert_eq!(a[0].as_i64(), Some(99));
    assert_eq!(a[1].as_i64(), Some(20));

    // get_mut: out of bounds
    assert!(a.get_mut(2).is_none());

    // as_slice
    let s = a.as_slice();
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].as_i64(), Some(99));
    assert_eq!(s[1].as_i64(), Some(20));

    // as_mut_slice: modify through slice
    let s = a.as_mut_slice();
    if let crate::item::ValueMut::Integer(i) = s[1].value_mut() {
        *i = 200;
    }
    assert_eq!(a[1].as_i64(), Some(200));
}

#[test]
fn pop_and_last_mut() {
    let arena = Arena::new();
    let mut a = InternalArray::new();

    // last_mut on empty
    assert!(a.last_mut().is_none());

    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    a.push(ival(3), &arena);

    // last_mut: modify tail
    let last = a.last_mut().unwrap();
    if let crate::item::ValueMut::Integer(i) = last.value_mut() {
        *i = 99;
    }
    assert_eq!(a[2].as_i64(), Some(99));

    // pop returns (modified) last, then remaining in reverse
    assert_eq!(a.pop().unwrap().as_i64(), Some(99));
    assert_eq!(a.len(), 2);
    assert_eq!(a.pop().unwrap().as_i64(), Some(2));
    assert_eq!(a.pop().unwrap().as_i64(), Some(1));
    assert!(a.pop().is_none());
    assert!(a.is_empty());
}

#[test]
#[allow(clippy::drop_non_drop)]
fn iterators() {
    let arena = Arena::new();

    let mut a = InternalArray::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    a.push(ival(3), &arena);

    let vals: Vec<i64> = (&a).into_iter().map(|v| v.as_i64().unwrap()).collect();
    assert_eq!(vals, vec![1, 2, 3]);

    for v in &mut a {
        if let crate::item::ValueMut::Integer(i) = v.value_mut() {
            *i += 10;
        }
    }
    assert_eq!(a[0].as_i64(), Some(11));
    assert_eq!(a[1].as_i64(), Some(12));
    assert_eq!(a[2].as_i64(), Some(13));

    let vals: Vec<i64> = a.into_iter().map(|v| v.as_i64().unwrap()).collect();
    assert_eq!(vals, vec![11, 12, 13]);

    let mut a = InternalArray::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    a.push(ival(3), &arena);
    let mut iter = a.into_iter();
    assert_eq!(iter.size_hint(), (3, Some(3)));
    iter.next();
    assert_eq!(iter.size_hint(), (2, Some(2)));

    let mut a = InternalArray::new();
    for i in 0..5 {
        a.push(ival(i), &arena);
    }
    let mut iter = a.into_iter();
    assert_eq!(iter.next().unwrap().as_i64(), Some(0));
    assert_eq!(iter.next().unwrap().as_i64(), Some(1));
    drop(iter); // end borrow before creating next iterator

    let a = InternalArray::new();
    let mut iter = a.into_iter();
    assert!(iter.next().is_none());

    // Test iter() method directly
    let mut a = InternalArray::new();
    a.push(ival(10), &arena);
    a.push(ival(20), &arena);
    let vals: Vec<i64> = a.iter().map(|v| v.as_i64().unwrap()).collect();
    assert_eq!(vals, vec![10, 20]);
}

#[test]
fn empty_array_edge_cases() {
    let mut a = InternalArray::new();

    // Empty array slices
    assert_eq!(a.as_slice().len(), 0);
    assert_eq!(a.as_mut_slice().len(), 0);

    // Empty array iter
    assert_eq!(a.iter().count(), 0);

    // Debug formatting
    let debug = format!("{:?}", a);
    assert_eq!(debug, "[]");

    let arena = Arena::new();
    let mut a = InternalArray::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    let debug = format!("{:?}", a);
    assert!(debug.contains('1') && debug.contains('2'));
}

#[test]
fn index_operator() {
    let arena = Arena::new();
    let mut a = InternalArray::new();
    a.push(ival(10), &arena);
    a.push(ival(20), &arena);
    a.push(ival(30), &arena);

    // Valid indices return MaybeItem with the value
    assert_eq!(a[0].as_i64(), Some(10));
    assert_eq!(a[1].as_i64(), Some(20));
    assert_eq!(a[2].as_i64(), Some(30));

    // Out-of-bounds returns NONE (no panic)
    assert!(a[3].item().is_none());
    assert!(a[100].item().is_none());
    assert!(a[usize::MAX].item().is_none());

    // Empty array always returns NONE
    let empty = InternalArray::new();
    assert!(empty[0].item().is_none());

    // NONE propagates through chained indexing
    assert!(a[0][0].item().is_none());
    assert!(a[99]["key"].item().is_none());
}

// == clone_in tests ===========================================================

#[test]
fn clone_in_basic() {
    let arena = Arena::new();
    let mut a = InternalArray::new();
    a.push(ival(10), &arena);
    a.push(ival(20), &arena);
    a.push(ival(30), &arena);

    let cloned = a.clone_in(&arena);
    assert_eq!(cloned.len(), 3);
    assert_eq!(cloned[0].as_i64(), Some(10));
    assert_eq!(cloned[1].as_i64(), Some(20));
    assert_eq!(cloned[2].as_i64(), Some(30));
}

#[test]
fn clone_in_empty() {
    let arena = Arena::new();
    let a = InternalArray::new();
    let cloned = a.clone_in(&arena);
    assert!(cloned.is_empty());
}

#[test]
fn clone_in_array_wrapper() {
    let arena = Arena::new();
    let mut arr = Array::new_spanned(Span::new(5, 20));
    arr.push(ival(1), &arena);
    arr.push(ival(2), &arena);

    let cloned = arr.clone_in(&arena);
    assert_eq!(cloned.len(), 2);
    assert_eq!(cloned.span_unchecked(), Span::new(5, 20));
    assert_eq!(cloned[0].as_i64(), Some(1));
    assert_eq!(cloned[1].as_i64(), Some(2));
    assert_eq!(cloned.style(), arr.style());
}

#[test]
fn clone_in_preserves_kind() {
    let arena = Arena::new();
    let mut arr = Array::new_spanned(Span::new(0, 10));
    arr.push(ival(1), &arena);

    arr.set_style(crate::item::ArrayStyle::Header);
    let cloned = arr.clone_in(&arena);
    assert_eq!(cloned.style(), crate::item::ArrayStyle::Header);

    arr.set_style(crate::item::ArrayStyle::Inline);
    let cloned = arr.clone_in(&arena);
    assert_eq!(cloned.style(), crate::item::ArrayStyle::Inline);
}

#[test]
fn clone_in_independent_of_source() {
    let arena = Arena::new();
    let mut a = InternalArray::new();
    a.push(ival(10), &arena);
    a.push(ival(20), &arena);

    let cloned = a.clone_in(&arena);

    // Mutate the original
    if let Some(v) = a.get_mut(0) {
        if let crate::item::ValueMut::Integer(i) = v.value_mut() {
            *i = 999;
        }
    }
    // Clone is unaffected
    assert_eq!(cloned[0].as_i64(), Some(10));
}

#[test]
fn clone_in_nested_arrays() {
    let arena = Arena::new();

    let mut inner = InternalArray::new();
    inner.push(ival(1), &arena);
    inner.push(ival(2), &arena);

    let mut outer = InternalArray::new();
    outer.push(Item::array(inner, Span::new(0, 5)), &arena);
    outer.push(ival(99), &arena);

    let cloned = outer.clone_in(&arena);
    assert_eq!(cloned.len(), 2);
    assert_eq!(cloned[0].as_array().unwrap().len(), 2);
    assert_eq!(cloned[0].as_array().unwrap()[0].as_i64(), Some(1));
    assert_eq!(cloned[0].as_array().unwrap()[1].as_i64(), Some(2));
    assert_eq!(cloned[1].as_i64(), Some(99));
}
