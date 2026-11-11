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

#[test]
fn initial_activation_stimulus_is_one_build_and_build_pump_only() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "keld-initial-activation-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).expect("create unique fixture directory");
    let hold = NativeHold::parse(&format!("build-pump:{}", directory.display()))
        .expect("build-pump fixture");
    let default = hold.initially_focused(2).expect("missing marker");
    let marker = directory.join("2.initially-inactive");
    std::fs::write(&marker, b"inactive\n").expect("exact build marker");
    let selected: Vec<_> = (1..=3)
        .map(|build| hold.initially_focused(build).expect("read exact marker"))
        .collect();
    let destroyed = NativeHold::parse(&format!("destroyed:{}", directory.display()))
        .expect("destroyed fixture");
    let unrelated = destroyed
        .initially_focused(2)
        .expect("unrelated hold point");
    let malformed = directory.join("3.initially-inactive");
    std::fs::create_dir(&malformed).expect("non-file marker");
    let rejected = hold.initially_focused(3).is_err();
    std::fs::remove_dir(malformed).expect("remove non-file marker");
    std::fs::remove_file(marker).expect("remove exact marker");
    std::fs::remove_dir(directory).expect("remove empty fixture directory");
    assert!(default, "ordinary first show retains its existing default");
    assert_eq!(selected, [true, false, true]);
    assert!(
        unrelated,
        "destroyed holds cannot change initial activation"
    );
    assert!(
        rejected,
        "malformed stimulus must fail before creating a HWND"
    );
}
