use super::*;
use std::alloc::Layout;

// -- Arena basics -----------------------------------------------------------

#[test]
fn new_and_drop() {
    let arena = Arena::new();
    drop(arena);
}

#[test]
fn alloc_single_byte() {
    let arena = Arena::new();
    let layout = Layout::from_size_align(1, 1).unwrap();
    let ptr = arena.alloc(layout);
    unsafe { ptr.as_ptr().write(0xAB) };
    assert_eq!(unsafe { *ptr.as_ptr() }, 0xAB);
}

#[test]
fn alloc_returns_aligned_pointers() {
    let arena = Arena::new();
    for align in [1, 2, 4, 8] {
        let layout = Layout::from_size_align(16, align).unwrap();
        let ptr = arena.alloc(layout);
        assert_eq!(ptr.as_ptr() as usize % align, 0, "align={align}");
    }
}

#[test]
fn alloc_multiple_no_overlap() {
    let arena = Arena::new();
    let layout = Layout::from_size_align(64, 8).unwrap();
    let a = arena.alloc(layout);
    let b = arena.alloc(layout);
    let c = arena.alloc(layout);

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
    let layout = Layout::from_size_align(128, 1).unwrap();
    // Allocate well beyond INITIAL_SLAB_SIZE to force at least one slab growth.
    for _ in 0..20 {
        let ptr = arena.alloc(layout);
        // Write to verify the memory is usable.
        unsafe {
            std::ptr::write_bytes(ptr.as_ptr(), 0xCC, 128);
        }
    }
}

#[test]
fn alloc_large_single() {
    let arena = Arena::new();
    let layout = Layout::from_size_align(4096, 8).unwrap();
    let ptr = arena.alloc(layout);
    unsafe {
        std::ptr::write_bytes(ptr.as_ptr(), 0xDD, 4096);
    }
    assert_eq!(ptr.as_ptr() as usize % 8, 0);
}

#[test]
fn alloc_zst() {
    let arena = Arena::new();
    let layout = Layout::from_size_align(0, 1).unwrap();
    let ptr = arena.alloc(layout);
    assert_eq!(ptr.as_ptr() as usize, 1);
}

#[test]
fn alloc_zst_high_align() {
    let arena = Arena::new();
    let layout = Layout::from_size_align(0, 64).unwrap();
    let ptr = arena.alloc(layout);
    assert_eq!(ptr.as_ptr() as usize % 64, 0);
}

// -- Scratch basics ---------------------------------------------------------

#[test]
fn scratch_push_and_as_bytes() {
    let arena = Arena::new();
    // Trigger an initial allocation so the arena has a real slab.
    arena.alloc(Layout::from_size_align(1, 1).unwrap());

    let mut scratch = unsafe { arena.scratch() };
    scratch.push(b'h');
    scratch.push(b'i');
    assert_eq!(scratch.as_bytes(), b"hi");
}

#[test]
fn scratch_extend() {
    let arena = Arena::new();
    arena.alloc(Layout::from_size_align(1, 1).unwrap());

    let mut scratch = unsafe { arena.scratch() };
    scratch.extend(b"hello ");
    scratch.extend(b"world");
    assert_eq!(scratch.as_bytes(), b"hello world");
}

#[test]
fn scratch_len() {
    let arena = Arena::new();
    arena.alloc(Layout::from_size_align(1, 1).unwrap());

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
    arena.alloc(Layout::from_size_align(1, 1).unwrap());

    let mut scratch = unsafe { arena.scratch() };
    scratch.extend(b"committed");
    let slice = scratch.commit();
    assert_eq!(slice, b"committed");
}

#[test]
fn scratch_commit_advances_ptr() {
    let arena = Arena::new();
    arena.alloc(Layout::from_size_align(1, 1).unwrap());

    let mut scratch = unsafe { arena.scratch() };
    scratch.extend(b"data");
    let committed = scratch.commit();

    // Subsequent allocation should not overlap the committed region.
    let layout = Layout::from_size_align(4, 1).unwrap();
    let next = arena.alloc(layout);
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
    arena.alloc(Layout::from_size_align(1, 1).unwrap());
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
    arena.alloc(Layout::from_size_align(1, 1).unwrap());

    let mut scratch = unsafe { arena.scratch() };
    // Write enough to overflow the initial slab.
    let pattern: Vec<u8> = (0u8..=255).cycle().take(2048).collect();
    scratch.extend(&pattern);
    assert_eq!(scratch.as_bytes(), &pattern[..]);
}

#[test]
fn scratch_grow_on_push() {
    let arena = Arena::new();
    arena.alloc(Layout::from_size_align(1, 1).unwrap());

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
    arena.alloc(Layout::from_size_align(1, 1).unwrap());

    let mut scratch = unsafe { arena.scratch() };
    let data: Vec<u8> = (0..2048).map(|i| (i % 251) as u8).collect();
    scratch.extend(&data);
    let committed = scratch.commit();
    assert_eq!(committed, &data[..]);

    // Allocate after commit — should not overlap.
    let layout = Layout::from_size_align(64, 1).unwrap();
    let next = arena.alloc(layout);
    let committed_end = committed.as_ptr() as usize + committed.len();
    assert!(next.as_ptr() as usize >= committed_end);
}

// -- Interaction ------------------------------------------------------------

#[test]
fn alloc_then_scratch_then_alloc() {
    let arena = Arena::new();

    // First allocation.
    let layout = Layout::from_size_align(32, 8).unwrap();
    let a = arena.alloc(layout);
    unsafe { std::ptr::write_bytes(a.as_ptr(), 0xAA, 32) };

    // Scratch in the middle.
    let mut scratch = unsafe { arena.scratch() };
    scratch.extend(b"middle");
    let mid = scratch.commit();
    assert_eq!(mid, b"middle");

    // Second allocation.
    let b = arena.alloc(layout);
    unsafe { std::ptr::write_bytes(b.as_ptr(), 0xBB, 32) };

    // Verify first allocation wasn't corrupted.
    let first_bytes = unsafe { std::slice::from_raw_parts(a.as_ptr(), 32) };
    assert!(first_bytes.iter().all(|&b| b == 0xAA));
}

#[test]
fn scratch_dropped_then_new_scratch() {
    let arena = Arena::new();
    arena.alloc(Layout::from_size_align(1, 1).unwrap());

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
