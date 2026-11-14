//! Allocation oracle for privileged filesystem validation plus guard Allow.

#![allow(
    unsafe_code,
    reason = "test-only GlobalAlloc wrapper observes allocations without adding production unsafe"
)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use keld_guard::{Principal, RendererChannelQuery, parse_manifest};
use keld_ipc::guard_dispatch::{
    dispatch_privileged, dispatch_privileged_query, preflight_privileged_query,
};

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
fn privileged_filesystem_allow_path_allocates_nothing() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    let (scope, requested) = ("C:/safe/**", "C:/safe/file.txt");
    #[cfg(not(windows))]
    let (scope, requested) = ("/safe/**", "/safe/file.txt");
    let manifest = parse_manifest(&format!(r#"{{"app":{{"fs":{{"read":["{scope}"]}}}}}}"#))?;

    ALLOCATIONS.store(0, Ordering::Relaxed);
    TRACKING.store(true, Ordering::SeqCst);
    let selected = dispatch_privileged(
        &manifest,
        Principal::AppProcess,
        "fs.read",
        requested,
        keld_guard::ScopePermit::grant_index,
    );
    TRACKING.store(false, Ordering::SeqCst);

    assert_eq!(selected, Ok(0));
    assert_eq!(
        ALLOCATIONS.load(Ordering::Relaxed),
        0,
        "filesystem validation plus Allow must remain allocation-free"
    );

    let channels = parse_manifest(r#"{"windows":{"editor":{"channels":["el:rendererReq"]}}}"#)?;
    let query = RendererChannelQuery {
        channel: "rendererReq",
        current_window: channels.window_channel_grants("editor"),
    };
    let principal = Principal::Webview {
        id: 7,
        generation: 3,
    };
    // Warm both generic instantiations independently before counting Guard only.
    preflight_privileged_query(&channels, principal, query).expect("exact grant permits preflight");
    dispatch_privileged_query(&channels, principal, query, |_| ())
        .expect("exact grant permits final dispatch");
    ALLOCATIONS.store(0, Ordering::Relaxed);
    TRACKING.store(true, Ordering::SeqCst);
    let preflight = preflight_privileged_query(&channels, principal, query);
    let channel = dispatch_privileged_query(&channels, principal, query, |_| 17);
    TRACKING.store(false, Ordering::SeqCst);
    assert_eq!(preflight, Ok(()));
    assert_eq!(channel, Ok(17));
    assert_eq!(
        ALLOCATIONS.load(Ordering::Relaxed),
        0,
        "warmed channel Guard preflight plus final callback dispatch must allocate nothing"
    );
    Ok(())
}
