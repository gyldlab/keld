//! Configuration and actual-pump observation state; no native hold runs here.
use super::{HoldPoint, NativeHold, PUMP, PumpHold, WM_CLOSE, observe_pump_message};

#[test]
fn hold_parser_preserves_a_windows_drive_colon() {
    for (input, expected) in [
        (r"destroyed:D:\fixture\hold", HoldPoint::Destroyed),
        (r"build-pump:D:\fixture\hold", HoldPoint::BuildPump),
    ] {
        let hold = NativeHold::parse(input).expect("approved absolute hold directory");
        assert_eq!(hold.point, expected);
        assert_eq!(hold.directory, std::path::Path::new(r"D:\fixture\hold"));
    }
    for bad in [
        "",
        "destroyed",
        "destroyed:",
        "other:D:\\fixture",
        "destroyed:relative",
    ] {
        let error = NativeHold::parse(bad).expect_err("invalid fixture");
        assert!(
            error.to_string().contains("KELD_TEST_WINDOWS_NATIVE_HOLD"),
            "{error}"
        );
    }
}

#[test]
fn the_hold_requires_a_real_empty_pump_observation_after_release() {
    observe_pump_message(false, 0, 0);
    assert!(!PUMP.get().active);
    assert_eq!(PUMP.get().posted, None);
    {
        let _hold = PumpHold::begin().expect("one native hold");
        assert!(PUMP.get().active);
        assert_eq!(
            PUMP.get().posted,
            None,
            "missing instrumentation is not an empty queue"
        );
        observe_pump_message(true, WM_CLOSE, 101);
        assert_eq!(PUMP.get().posted, Some(true));
        observe_pump_message(false, 0, 0);
        assert_eq!(PUMP.get().posted, Some(false));
        assert!(
            PumpHold::begin().is_err(),
            "a nested hold cannot replace its caller"
        );
        assert!(PUMP.get().active);
    }
    assert!(!PUMP.get().active);
    assert_eq!(PUMP.get().posted, None);
}
