use std::mem::MaybeUninit;

use crate::{Item, Key};

/// Partitions a slice stably in O(N) linear time in a single pass.
///
/// # Safety
/// The caller must uphold the following guarantees to avoid Undefined Behavior (UB):
/// 1. `T` must **not** implement a custom `Drop` (or `needs_drop::<T>()` must be false).
///    If an error occurs, elements may be duplicated or left in an inconsistent state.
/// 2. `predicate` must **not panic**. If a panic unwinds across this function,
///    the original slice will contain duplicated, uninitialized, or overlapping memory.
unsafe fn stable_partition_linear<T, F>(slice: &mut [T], mut predicate: F) -> usize
where
    F: FnMut(&T) -> bool,
{
    // SAFETY: All pointer arithmetic below stays within the slice (length `len`)
    // and the false_buf (capacity `len`).
    //
    // Loop invariant: at iteration i, slice[0..true_idx] contains compacted
    // true-predicate elements, and false_buf[0..false_idx] contains the
    // false-predicate elements seen so far. true_idx + false_idx == i, so
    // true_idx <= i, ensuring the copy-within-slice from position i to
    // true_idx never overlaps (single-element copies, distinct positions).
    //
    // After the loop, true_idx + false_idx == len, so the final copy from
    // false_buf to slice[true_idx..] fills exactly the remaining space.
    // false_buf is a Vec<MaybeUninit<T>> — its drop does not run destructors
    // on the moved-out elements. Since T is !Drop (caller guarantee), no
    // double-free occurs from the bitwise duplication during compaction.
    unsafe {
        let len = slice.len();
        if len == 0 {
            return 0;
        }

        let mut false_buf = Vec::<MaybeUninit<T>>::with_capacity(len);

        let slice_ptr = slice.as_mut_ptr();
        let false_ptr = false_buf.as_mut_ptr() as *mut T;

        let mut true_idx = 0;
        let mut false_idx = 0;

        for i in 0..len {
            let item_ptr = slice_ptr.add(i);

            if predicate(&*item_ptr) {
                if i != true_idx {
                    std::ptr::copy_nonoverlapping(item_ptr, slice_ptr.add(true_idx), 1);
                }
                true_idx += 1;
            } else {
                std::ptr::copy_nonoverlapping(item_ptr, false_ptr.add(false_idx), 1);
                false_idx += 1;
            }
        }

        std::ptr::copy_nonoverlapping(false_ptr, slice_ptr.add(true_idx), false_idx);

        true_idx
    }
}

pub(crate) fn ensure_body_order(entries: &mut [(Key<'_>, Item<'_>)]) {
    // Single pass: once we see a subsection, any subsequent body item means reorder.
    let mut seen_sub = false;
    let mut needs_reorder = false;
    for (_, item) in entries.iter() {
        if item.is_subsection() {
            seen_sub = true;
        } else if seen_sub {
            needs_reorder = true;
            break;
        }
    }

    if !needs_reorder {
        return;
    }

    // SAFETY:
    // - (Key, Item) has no Drop impl: Key is Copy, Item has no custom Drop.
    // - The predicate `!item.is_subsection()` calls only simple flag-check
    //   methods (has_header_bit, is_implicit_table, is_aot) that cannot panic.
    unsafe {
        stable_partition_linear(entries, |(_, item)| !item.is_subsection());
    }
}
