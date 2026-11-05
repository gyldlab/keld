//! The approved native-event forwarding table, without native window creation.
//! These assertions constrain the production selector, not Core's state filtering.

use super::{NativeFacts, native_facts};
use tao::dpi::{PhysicalPosition, PhysicalSize};
use tao::event::WindowEvent;

#[test]
fn scale_factor_change_selects_bounds_and_fullscreen_without_maximized() {
    let mut suggested = PhysicalSize::new(960, 720);
    let event = WindowEvent::ScaleFactorChanged {
        scale_factor: 1.5,
        new_inner_size: &mut suggested,
    };
    assert_eq!(
        native_facts(&event, || false),
        NativeFacts::BoundsAndFullScreen { maximized: false },
        "ScaleFactorChanged must observe bounds and fullscreen without a maximized fact",
    );
    assert_eq!(
        suggested,
        PhysicalSize::new(960, 720),
        "fact selection must not alter Tao's suggested native size",
    );
}

#[test]
fn minimized_windows_suppress_geometry_facts_but_keep_focus_observation() {
    let mut suggested = PhysicalSize::new(960, 720);
    let geometry = [
        WindowEvent::Moved(PhysicalPosition::new(50, 70)),
        WindowEvent::Resized(PhysicalSize::new(640, 480)),
        WindowEvent::ScaleFactorChanged {
            scale_factor: 1.5,
            new_inner_size: &mut suggested,
        },
    ];
    for event in geometry {
        assert_eq!(
            native_facts(&event, || true),
            NativeFacts::QueuedFocus,
            "minimized native geometry queues activation while suppressing geometry facts",
        );
    }
    for focused in [true, false] {
        assert_eq!(
            native_facts(&WindowEvent::Focused(focused), || {
                panic!("focus selection must not query native geometry")
            }),
            NativeFacts::Focus,
            "minimize does not suppress the independent activation observation",
        );
    }
}

#[test]
fn resized_alone_adds_maximized_to_geometry_facts() {
    assert_eq!(
        native_facts(&WindowEvent::Moved(PhysicalPosition::new(-200, 30)), || {
            false
        },),
        NativeFacts::BoundsAndFullScreen { maximized: false },
    );
    // The native minimized state is the authority; 0x0 alone is not its proxy.
    for size in [PhysicalSize::new(640, 480), PhysicalSize::new(0, 0)] {
        assert_eq!(
            native_facts(&WindowEvent::Resized(size), || false),
            NativeFacts::BoundsAndFullScreen { maximized: true },
        );
    }
    for event in [WindowEvent::CloseRequested, WindowEvent::Destroyed] {
        assert_eq!(
            native_facts(&event, || panic!(
                "unrelated event must not query native geometry"
            )),
            NativeFacts::None,
            "close and teardown retain their existing separate owners",
        );
    }
}
