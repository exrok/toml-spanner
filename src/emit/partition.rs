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
    unsafe {
        let len = slice.len();
        if len == 0 {
            return 0;
        }

        // Allocate an uninitialized buffer large enough to hold all elements
        // in the worst-case scenario (all elements evaluate to `false`).
        let mut false_buf = Vec::<MaybeUninit<T>>::with_capacity(len);

        let slice_ptr = slice.as_mut_ptr();
        let false_ptr = false_buf.as_mut_ptr() as *mut T;

        let mut true_idx = 0;
        let mut false_idx = 0;

        for i in 0..len {
            let item_ptr = slice_ptr.add(i);

            // SAFETY: The caller guarantees `predicate` will not panic.
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

        // Append the `false` elements right after the `true` elements.
        std::ptr::copy_nonoverlapping(false_ptr, slice_ptr.add(true_idx), false_idx);

        // false_buf is dropped here without dropping its inner MaybeUninit<T> elements.

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

    unsafe {
        stable_partition_linear(entries, |(_, item)| !item.is_subsection());
    }
}
