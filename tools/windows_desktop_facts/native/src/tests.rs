//! Pure tests only: none calls Windows, reads settings, or creates an HWND.
use crate::args::{Args, Control, Mode};
use crate::{display, error, identity};
use serde_json::json;
use windows_sys::Win32::Foundation::RECT;

fn args(values: &[&str]) -> Result<Args, error::ProbeError> {
    Args::parse(values.iter().map(|value| value.to_string()))
}

#[test]
fn cli_requires_explicit_context_and_rejects_ambiguous_requests() {
    assert!(args(&[]).is_err());
    assert!(args(&["--mode", "pmv1"]).is_err());
    assert!(args(&["--mode", "pmv2", "--mode", "unaware"]).is_err());
    assert!(args(&["--mode", "pmv2", "--control", "unknown"]).is_err());
    assert_eq!(
        args(&["--mode", "unaware", "--control", "failed-dpi"]).unwrap(),
        Args {
            mode: Mode::Unaware,
            control: Some(Control::FailedDpi)
        }
    );
}

#[test]
fn failed_or_unusable_dpi_never_becomes_scale_one() {
    let error = display::validate_dpi(0x80070057u32 as i32, 120, 120).unwrap_err();
    assert_eq!(error.kind, "native-failure");
    assert_eq!(error.source, "GetDpiForMonitor");
    assert!(display::validate_dpi(0, 0, 0).is_err());
    assert!(display::validate_dpi(0, 120, 96).is_err());
    assert_eq!(display::validate_dpi(0, 120, 120).unwrap(), 1.25);
    assert_eq!(display::validate_dpi(0, 144, 144).unwrap(), 1.5);
}

#[test]
fn negative_monitor_origins_are_preserved_but_empty_rectangles_are_refused() {
    assert_eq!(
        display::rectangle(RECT {
            left: -1920,
            top: -100,
            right: 0,
            bottom: 980
        })
        .unwrap(),
        json!({"x": -1920, "y": -100, "width": 1920, "height": 1080})
    );
    assert!(display::rectangle(RECT::default()).is_err());
    assert!(
        display::rectangle(RECT {
            left: 10,
            top: 0,
            right: 5,
            bottom: 10
        })
        .is_err()
    );
}

#[test]
fn source_join_retains_clone_targets_and_refuses_missing_identity() {
    let paths = vec![
        identity::NativePath {
            source_name: "source-a".into(),
            facts: json!({"targetId": 7}),
        },
        identity::NativePath {
            source_name: "source-a".into(),
            facts: json!({"targetId": 9}),
        },
        identity::NativePath {
            source_name: "source-b".into(),
            facts: json!({"targetId": 11}),
        },
    ];
    assert_eq!(
        identity::for_source(&paths, "source-a").unwrap(),
        vec![json!({"targetId": 7}), json!({"targetId": 9})]
    );
    assert!(identity::for_source(&paths, "missing").is_err());
}

#[test]
fn malformed_native_names_are_refused_instead_of_lossily_reidentified() {
    assert_eq!(error::utf16(&[65, 0, 66], "fixture").unwrap(), "A");
    assert!(error::utf16(&[65, 66], "fixture").is_err());
    assert!(error::utf16(&[0xd800, 0], "fixture").is_err());
}
