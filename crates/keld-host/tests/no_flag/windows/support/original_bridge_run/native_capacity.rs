//! X11 uses the existing process owner and bounded readback paths.
use super::{Fixture, OriginalRun, ephemeral_profile_dir};
use crate::facade_window::fixture::{capture, census};
use crate::support::window::NativeWindowSnapshot;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

impl OriginalRun {
    pub(crate) fn start_native_capacity(cli: &Path, project: &Path, root: &Path) -> Self {
        Self::start_with_native_hold(cli, project, Fixture::NativeCapacity, Some(root))
    }

    /// Census precedes LOAD and every finite scheduling deadline.
    pub(crate) fn retain_native_capacity_window(&mut self) -> NativeWindowSnapshot {
        self.retain_resource_native_tree();
        let windows = census(self.host_pid, 1);
        let window = capture(windows[0], self.host_pid);
        assert!(
            !window.children.is_empty(),
            "actual controller child windows"
        );
        println!(
            "KELD_ORIGINAL_X11_AT_RISK {}",
            serde_json::json!({
                "host":self.host_pid,"sentinel":self.sentinel_pid,"bun":self.hello["bunPid"],
                "hwnd":window.hwnd,"children":window.children,
                "profile":ephemeral_profile_dir(self.profiles.path()),
                "retainedProcessHandles":self.retained.len(),
                "filesystemIdentityCapability":false,
            })
        );
        window
    }

    pub(crate) fn native_consumer_condition(
        &mut self,
        root: &Path,
        phase: &str,
        deadline: Instant,
    ) -> Value {
        assert!(
            matches!(phase, "entered" | "leaving"),
            "closed fixture observation"
        );
        let path = root.join(phase);
        loop {
            self.assert_live("actual synchronous fixture consumer condition", deadline);
            match fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    assert!(metadata.is_file() && !metadata.file_type().is_symlink());
                    assert!(
                        metadata.len() <= 4096,
                        "bounded completed consumer metadata"
                    );
                    let value: Value = serde_json::from_slice(
                        &fs::read(&path).expect("actual completed consumer metadata"),
                    )
                    .expect("actual complete consumer JSON");
                    assert_eq!(value["phase"], phase);
                    assert_eq!(value["bunPid"], self.hello["bunPid"]);
                    assert_eq!(value["effects"], 1);
                    return value;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("consumer condition read failed: {error}"),
            }
            std::thread::park_timeout(Duration::from_millis(10));
        }
    }
}
