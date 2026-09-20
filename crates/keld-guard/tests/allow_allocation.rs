//! Allocation oracle for the filesystem request-validation plus Allow path.

#![allow(
    unsafe_code,
    reason = "test-only GlobalAlloc wrapper observes allocations without adding production unsafe"
)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use keld_guard::{Decision, Principal, evaluate, parse_manifest, validate_fs_request};

struct CountingAllocator;

static TRACKING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACKING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: this wrapper forwards the exact layout to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if TRACKING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: this wrapper forwards the exact layout to the system allocator.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer/layout pair came from the forwarded system allocation.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if TRACKING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the pointer/layout pair came from System and `new_size` is forwarded.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn windows_style_filesystem_allow_path_allocates_nothing() {
    let manifest = parse_manifest(r#"{"app":{"fs":{"read":["C:/safe/**"]}}}"#).expect("manifest");

    ALLOCATIONS.store(0, Ordering::Relaxed);
    TRACKING.store(true, Ordering::SeqCst);
    let syntax = validate_fs_request(&manifest, "fs.read", "C:/safe/file.txt");
    let decision = evaluate(
        &manifest,
        Principal::AppProcess,
        "fs.read",
        "C:/safe/file.txt",
    );
    TRACKING.store(false, Ordering::SeqCst);

    assert!(syntax.is_ok());
    assert!(matches!(decision, Decision::Allow(_)));
    assert_eq!(
        ALLOCATIONS.load(Ordering::Relaxed),
        0,
        "filesystem validation plus Allow must remain allocation-free"
    );
}
