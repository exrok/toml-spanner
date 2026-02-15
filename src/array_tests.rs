use super::*;
use crate::Span;
use crate::arena::Arena;

fn sp() -> Span {
    Span::new(0, 0)
}

fn ival(i: i64) -> Item<'static> {
    Item::integer(i, sp())
}

#[test]
fn push_and_realloc() {
    let arena = Arena::new();

    // with_single creates a one-element array
    let a = Array::with_single(ival(42), &arena);
    assert_eq!(a.len(), 1);
    assert_eq!(a.get(0).unwrap().as_i64(), Some(42));

    // Push from empty through all realloc thresholds (0 -> 4 -> 8 -> beyond)
    let mut a = Array::new();
    assert!(a.is_empty());
    assert_eq!(a.len(), 0);

    for i in 0..9_i64 {
        a.push(ival(i), &arena);
        assert_eq!(a.len(), i as usize + 1);
    }

    // Verify all values survived reallocs
    for i in 0..9 {
        assert_eq!(a.get(i).unwrap().as_i64(), Some(i as i64));
    }
}

#[test]
fn get_and_get_mut() {
    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(10), &arena);
    a.push(ival(20), &arena);

    // get: valid and out of bounds
    assert_eq!(a.get(0).unwrap().as_i64(), Some(10));
    assert_eq!(a.get(1).unwrap().as_i64(), Some(20));
    assert!(a.get(2).is_none());
    assert!(a.get(100).is_none());

    // get_mut: modify first element
    let v = a.get_mut(0).unwrap();
    if let crate::value::ValueMut::Integer(i) = v.value_mut() {
        *i = 99;
    }
    assert_eq!(a.get(0).unwrap().as_i64(), Some(99));
    assert_eq!(a.get(1).unwrap().as_i64(), Some(20));

    // get_mut: out of bounds
    assert!(a.get_mut(2).is_none());

    // as_slice
    let s = a.as_slice();
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].as_i64(), Some(99));
    assert_eq!(s[1].as_i64(), Some(20));

    // as_mut_slice: modify through slice
    let s = a.as_mut_slice();
    if let crate::value::ValueMut::Integer(i) = s[1].value_mut() {
        *i = 200;
    }
    assert_eq!(a.get(1).unwrap().as_i64(), Some(200));
}

#[test]
fn pop_and_last_mut() {
    let arena = Arena::new();
    let mut a = Array::new();

    // last_mut on empty
    assert!(a.last_mut().is_none());

    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    a.push(ival(3), &arena);

    // last_mut: modify tail
    let last = a.last_mut().unwrap();
    if let crate::value::ValueMut::Integer(i) = last.value_mut() {
        *i = 99;
    }
    assert_eq!(a.get(2).unwrap().as_i64(), Some(99));

    // pop returns (modified) last, then remaining in reverse
    assert_eq!(a.pop().unwrap().as_i64(), Some(99));
    assert_eq!(a.len(), 2);
    assert_eq!(a.pop().unwrap().as_i64(), Some(2));
    assert_eq!(a.pop().unwrap().as_i64(), Some(1));
    assert!(a.pop().is_none());
    assert!(a.is_empty());
}

#[test]
fn iterators() {
    let arena = Arena::new();

    let mut a = Array::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    a.push(ival(3), &arena);

    let vals: Vec<i64> = (&a).into_iter().map(|v| v.as_i64().unwrap()).collect();
    assert_eq!(vals, vec![1, 2, 3]);

    for v in &mut a {
        if let crate::value::ValueMut::Integer(i) = v.value_mut() {
            *i += 10;
        }
    }
    assert_eq!(a.get(0).unwrap().as_i64(), Some(11));
    assert_eq!(a.get(1).unwrap().as_i64(), Some(12));
    assert_eq!(a.get(2).unwrap().as_i64(), Some(13));

    let vals: Vec<i64> = a.into_iter().map(|v| v.as_i64().unwrap()).collect();
    assert_eq!(vals, vec![11, 12, 13]);

    let mut a = Array::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    a.push(ival(3), &arena);
    let mut iter = a.into_iter();
    assert_eq!(iter.size_hint(), (3, Some(3)));
    iter.next();
    assert_eq!(iter.size_hint(), (2, Some(2)));

    let mut a = Array::new();
    for i in 0..5 {
        a.push(ival(i), &arena);
    }
    let mut iter = a.into_iter();
    assert_eq!(iter.next().unwrap().as_i64(), Some(0));
    assert_eq!(iter.next().unwrap().as_i64(), Some(1));
    drop(iter);

    let a = Array::new();
    let mut iter = a.into_iter();
    assert!(iter.next().is_none());

    // Test iter() method directly
    let mut a = Array::new();
    a.push(ival(10), &arena);
    a.push(ival(20), &arena);
    let vals: Vec<i64> = a.iter().map(|v| v.as_i64().unwrap()).collect();
    assert_eq!(vals, vec![10, 20]);
}

#[test]
fn empty_array_edge_cases() {
    let mut a = Array::new();

    // Empty array slices
    assert_eq!(a.as_slice().len(), 0);
    assert_eq!(a.as_mut_slice().len(), 0);

    // Empty array iter
    assert_eq!(a.iter().count(), 0);

    // Debug formatting
    let debug = format!("{:?}", a);
    assert_eq!(debug, "[]");

    let arena = Arena::new();
    let mut a = Array::new();
    a.push(ival(1), &arena);
    a.push(ival(2), &arena);
    let debug = format!("{:?}", a);
    assert!(debug.contains('1') && debug.contains('2'));
}

#[test]
fn index_operator() {
    let arena = Arena::new();
    let mut a = Array::new();
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
    let empty = Array::new();
    assert!(empty[0].item().is_none());

    // NONE propagates through chained indexing
    assert!(a[0][0].item().is_none());
    assert!(a[99]["key"].item().is_none());
}
