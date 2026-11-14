//! Pinned Rust1.99.1 / serde1.0.228 / `serde_json1.0.150` allocation bounds.
//! Requested backing capacities, not allocator pages or process RSS.
use std::alloc::Layout;

const FIXED_METADATA: usize = 1024;
// Pinned lexical::bhcomp on Windows x64/ARM64 uses64-bit limbs. Its
// two Bigints plus the largest old/new growth overlap fit320 limbs.
const FLOAT_ROUNDTRIP_SCRATCH: usize = 320 * size_of::<u64>();
const BTREE_CAPACITY: usize = 11;
const BTREE_EDGES: usize = 12;

pub(super) fn initial_bytes(input: usize) -> Option<usize> {
    // The32-bit limb/Karatsuba path has no reviewed allocation derivation yet.
    if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
        return None;
    }
    // SliceRead's one reusable escape scratch appends at most input bytes.
    // RawVec doubles: old+new growth peak <= 3*input + 2*minimum(8).
    // Include the exact raw UTF8 copy and fixed error/decoder metadata.
    input
        .checked_mul(4)?
        .checked_add(16 + FIXED_METADATA + FLOAT_ROUNDTRIP_SCRATCH)
}

pub(super) fn string_bytes(length: usize) -> Option<usize> {
    // Escaped serde Content owns a String while the final variant may copy it.
    // Unescaped Content borrows, so the same two-copy bound is conservative.
    length.checked_mul(2)
}

pub(super) fn sequence_step(element: Layout, first: bool) -> Option<usize> {
    let size = element.size();
    if size == 0 {
        return Some(0);
    }
    // RawVec::min_non_zero_cap: 8 bytes, 4 elements <=1024, otherwise1.
    // No size_hint is exposed. Before every consumer push, cumulative
    // 3*N + 2*minimum covers final capacity and old/new growth overlap.
    let minimum = if size == 1 {
        8
    } else if size <= 1024 {
        4
    } else {
        1
    };
    let slots = if first { 3 + 2 * minimum } else { 3 };
    size.checked_mul(slots)
}

fn padded(size: usize, alignment: usize) -> Option<usize> {
    size.checked_add(alignment.checked_sub(1)?)?
        .checked_div(alignment)?
        .checked_mul(alignment)
}

pub(super) fn map_entry(key: Layout, value: Layout) -> Option<usize> {
    let alignment = key.align().max(value.align()).max(align_of::<usize>());
    // LeafNode has repr(Rust), so do not pretend its field order is repr(C).
    // Padding EACH field to the maximum alignment bounds every allowed field
    // ordering; the enclosing repr(C) InternalNode adds12 native pointer edges.
    let leaf = [
        size_of::<usize>(),
        size_of::<u16>(),
        size_of::<u16>(),
        key.size().checked_mul(BTREE_CAPACITY)?,
        value.size().checked_mul(BTREE_CAPACITY)?,
    ]
    .into_iter()
    .try_fold(0usize, |total, field| {
        total.checked_add(padded(field, alignment)?)
    })?;
    let internal = leaf.checked_add(padded(
        size_of::<usize>().checked_mul(BTREE_EDGES)?,
        alignment,
    )?)?;
    // Across N actual insertion attempts, at most2*N nodes can have been
    // allocated (initial root, each split sibling, each height-growth root).
    // Credit accumulates before insertion, so a later cascade at any depth
    // spends already retained credit. This also dominates Content's pair Vec.
    internal.checked_mul(2)
}
