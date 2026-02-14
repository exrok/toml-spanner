use super::*;

// -- Arena basics -----------------------------------------------------------

#[test]
fn new_and_drop() {
    let arena = Arena::new();
    drop(arena);
}

#[test]
fn alloc_single() {
    let arena = Arena::new();
    let ptr = arena.alloc(8);
    unsafe { ptr.as_ptr().write(0xAB) };
    assert_eq!(unsafe { *ptr.as_ptr() }, 0xAB);
}

#[test]
fn alloc_returns_aligned_pointers() {
    let arena = Arena::new();
    for size in [8, 16, 24, 64] {
        let ptr = arena.alloc(size);
        assert_eq!(ptr.as_ptr() as usize % 8, 0, "size={size}");
    }
}

#[test]
fn alloc_multiple_no_overlap() {
    let arena = Arena::new();
    let a = arena.alloc(64);
    let b = arena.alloc(64);
    let c = arena.alloc(64);

    let a_range = a.as_ptr() as usize..a.as_ptr() as usize + 64;
    let b_start = b.as_ptr() as usize;
    let c_start = c.as_ptr() as usize;

    assert!(!a_range.contains(&b_start));
    assert!(!a_range.contains(&c_start));
    assert_ne!(b_start, c_start);
}

#[test]
fn alloc_triggers_slab_growth() {
    let arena = Arena::new();
    // Allocate well beyond INITIAL_SLAB_SIZE to force at least one slab growth.
    for _ in 0..20 {
        let ptr = arena.alloc(128);
        // Write to verify the memory is usable.
        unsafe {
            std::ptr::write_bytes(ptr.as_ptr(), 0xCC, 128);
        }
    }
}

#[test]
fn alloc_large_single() {
    let arena = Arena::new();
    let ptr = arena.alloc(4096);
    unsafe {
        std::ptr::write_bytes(ptr.as_ptr(), 0xDD, 4096);
    }
    assert_eq!(ptr.as_ptr() as usize % 8, 0);
}

// -- Realloc ----------------------------------------------------------------

#[test]
fn realloc_in_place() {
    let arena = Arena::new();
    let ptr = arena.alloc(24);
    unsafe { std::ptr::write_bytes(ptr.as_ptr(), 0xAA, 24) };

    // No intervening alloc — should extend in place.
    let new_ptr = unsafe { arena.realloc(ptr, 24, 48) };
    assert_eq!(new_ptr, ptr, "expected in-place extension");

    // Original data preserved.
    let bytes = unsafe { std::slice::from_raw_parts(new_ptr.as_ptr(), 24) };
    assert!(bytes.iter().all(|&b| b == 0xAA));
}

#[test]
fn realloc_intervening_alloc_forces_copy() {
    let arena = Arena::new();
    let a = arena.alloc(24);
    unsafe { std::ptr::write_bytes(a.as_ptr(), 0xBB, 24) };

    // Intervening allocation moves the head.
    let _b = arena.alloc(8);

    let new_ptr = unsafe { arena.realloc(a, 24, 48) };
    assert_ne!(new_ptr, a, "expected copy to new location");

    // Original data preserved in new location.
    let bytes = unsafe { std::slice::from_raw_parts(new_ptr.as_ptr(), 24) };
    assert!(bytes.iter().all(|&b| b == 0xBB));
}

#[test]
fn realloc_cross_slab() {
    let arena = Arena::new();
    // Fill the first slab almost completely.
    let ptr = arena.alloc(INITIAL_SLAB_SIZE - HEADER_SIZE);
    unsafe { std::ptr::write_bytes(ptr.as_ptr(), 0xCC, 64) };

    // Realloc to a size that cannot fit in the remaining slab space.
    let new_ptr = unsafe { arena.realloc(ptr, INITIAL_SLAB_SIZE - HEADER_SIZE, INITIAL_SLAB_SIZE) };
    assert_ne!(new_ptr, ptr, "expected new slab allocation");

    // First 64 bytes of data preserved.
    let bytes = unsafe { std::slice::from_raw_parts(new_ptr.as_ptr(), 64) };
    assert!(bytes.iter().all(|&b| b == 0xCC));
}

#[test]
fn realloc_multiple_in_place_grows() {
    let arena = Arena::new();
    let mut ptr = arena.alloc(24);
    unsafe { std::ptr::write_bytes(ptr.as_ptr(), 0xDD, 24) };
    let original = ptr;

    // Successive reallocs with no intervening allocs should all extend in place.
    for new_size in [48, 96, 192] {
        let old_size = if new_size == 48 {
            24
        } else {
            new_size / 2
        };
        ptr = unsafe { arena.realloc(ptr, old_size, new_size) };
        assert_eq!(ptr, original, "size={new_size}: expected in-place extension");
    }

    // Original data still intact.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), 24) };
    assert!(bytes.iter().all(|&b| b == 0xDD));
}

// -- Scratch basics ---------------------------------------------------------

#[test]
fn scratch_push_and_as_bytes() {
    let arena = Arena::new();
    // Trigger an initial allocation so the arena has a real slab.
    arena.alloc(8);

    let mut scratch = unsafe { arena.scratch() };
    scratch.push(b'h');
    scratch.push(b'i');
    assert_eq!(scratch.as_bytes(), b"hi");
}

#[test]
fn scratch_extend() {
    let arena = Arena::new();
    arena.alloc(8);

    let mut scratch = unsafe { arena.scratch() };
    scratch.extend(b"hello ");
    scratch.extend(b"world");
    assert_eq!(scratch.as_bytes(), b"hello world");
}

#[test]
fn scratch_len() {
    let arena = Arena::new();
    arena.alloc(8);

    let mut scratch = unsafe { arena.scratch() };
    scratch.extend(b"abc");
    assert_eq!(scratch.as_bytes().len(), 3);
}

#[test]
fn scratch_as_bytes_empty() {
    let arena = Arena::new();
    let scratch = unsafe { arena.scratch() };
    assert_eq!(scratch.as_bytes(), b"");
}

#[test]
fn scratch_commit_returns_slice() {
    let arena = Arena::new();
    arena.alloc(8);

    let mut scratch = unsafe { arena.scratch() };
    scratch.extend(b"committed");
    let slice = scratch.commit();
    assert_eq!(slice, b"committed");
}

#[test]
fn scratch_commit_advances_ptr() {
    let arena = Arena::new();
    arena.alloc(8);

    let mut scratch = unsafe { arena.scratch() };
    scratch.extend(b"data");
    let committed = scratch.commit();

    // Subsequent allocation should not overlap the committed region.
    let next = arena.alloc(8);
    let committed_range =
        committed.as_ptr() as usize..committed.as_ptr() as usize + committed.len();
    assert!(!committed_range.contains(&(next.as_ptr() as usize)));
}

#[test]
fn scratch_commit_empty() {
    let arena = Arena::new();
    let scratch = unsafe { arena.scratch() };
    let slice = scratch.commit();
    assert_eq!(slice, b"");
}

#[test]
fn scratch_drop_without_commit_is_safe() {
    let arena = Arena::new();
    arena.alloc(8);
    let ptr_before = arena.ptr.get();

    {
        let mut scratch = unsafe { arena.scratch() };
        scratch.extend(b"discarded");
        // Dropped without commit.
    }

    // Arena ptr should be unchanged — the scratch space is reusable.
    assert_eq!(arena.ptr.get(), ptr_before);
}

// -- Scratch growth ---------------------------------------------------------

#[test]
fn scratch_grow_preserves_data() {
    let arena = Arena::new();
    arena.alloc(8);

    let mut scratch = unsafe { arena.scratch() };
    // Write enough to overflow the initial slab.
    let pattern: Vec<u8> = (0u8..=255).cycle().take(2048).collect();
    scratch.extend(&pattern);
    assert_eq!(scratch.as_bytes(), &pattern[..]);
}

#[test]
fn scratch_grow_on_push() {
    let arena = Arena::new();
    arena.alloc(8);

    let mut scratch = unsafe { arena.scratch() };
    for i in 0..2048u16 {
        scratch.push((i & 0xFF) as u8);
    }
    assert_eq!(scratch.as_bytes().len(), 2048);
    for (i, &b) in scratch.as_bytes().iter().enumerate() {
        assert_eq!(b, (i & 0xFF) as u8, "mismatch at index {i}");
    }
}

#[test]
fn scratch_multiple_grows() {
    let arena = Arena::new();

    let mut scratch = unsafe { arena.scratch() };
    let mut expected = Vec::new();

    // Force several growths by extending in chunks.
    for round in 0u8..10 {
        let chunk: Vec<u8> = std::iter::repeat(round).take(512).collect();
        scratch.extend(&chunk);
        expected.extend(&chunk);
    }

    assert_eq!(scratch.as_bytes(), &expected[..]);
}

#[test]
fn scratch_commit_after_grow() {
    let arena = Arena::new();
    arena.alloc(8);

    let mut scratch = unsafe { arena.scratch() };
    let data: Vec<u8> = (0..2048).map(|i| (i % 251) as u8).collect();
    scratch.extend(&data);
    let committed = scratch.commit();
    assert_eq!(committed, &data[..]);

    // Allocate after commit — should not overlap.
    let next = arena.alloc(64);
    let committed_end = committed.as_ptr() as usize + committed.len();
    assert!(next.as_ptr() as usize >= committed_end);
}

// -- Interaction ------------------------------------------------------------

#[test]
fn alloc_then_scratch_then_alloc() {
    let arena = Arena::new();

    // First allocation.
    let a = arena.alloc(32);
    unsafe { std::ptr::write_bytes(a.as_ptr(), 0xAA, 32) };

    // Scratch in the middle.
    let mut scratch = unsafe { arena.scratch() };
    scratch.extend(b"middle");
    let mid = scratch.commit();
    assert_eq!(mid, b"middle");

    // Second allocation.
    let b = arena.alloc(32);
    unsafe { std::ptr::write_bytes(b.as_ptr(), 0xBB, 32) };

    // Verify first allocation wasn't corrupted.
    let first_bytes = unsafe { std::slice::from_raw_parts(a.as_ptr(), 32) };
    assert!(first_bytes.iter().all(|&b| b == 0xAA));
}

#[test]
fn scratch_dropped_then_new_scratch() {
    let arena = Arena::new();
    arena.alloc(8);

    {
        let mut scratch = unsafe { arena.scratch() };
        scratch.extend(b"discarded");
    }

    // Create a second scratch — should work fine.
    let mut scratch = unsafe { arena.scratch() };
    scratch.extend(b"kept");
    let committed = scratch.commit();
    assert_eq!(committed, b"kept");
}
