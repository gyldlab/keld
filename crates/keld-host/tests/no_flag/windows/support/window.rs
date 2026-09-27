//! Independent native HWND and window-title observation.

use crate::PRODUCT_TITLE;
use serde_json::Value;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) fn wait_for_host_window(pid: u32, deadline: Instant) -> Value {
    loop {
        let script = format!(
            "$p=Get-Process -Id {pid} -ErrorAction Stop; [pscustomobject]@{{handle=[uint64]$p.MainWindowHandle.ToInt64();title=$p.MainWindowTitle}} | ConvertTo-Json -Compress"
        );
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .expect("query host window");
        if output.status.success() {
            let observation: Value =
                serde_json::from_slice(&output.stdout).expect("host window observation JSON");
            if observation["handle"]
                .as_u64()
                .is_some_and(|handle| handle != 0)
                && observation["title"] == PRODUCT_TITLE
            {
                return observation;
            }
        }
        assert!(
            Instant::now() < deadline,
            "host HWND/title observation timed out"
        );
        thread::park_timeout(Duration::from_millis(20));
    }
}
