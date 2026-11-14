//! Fixed inputs for the same process/profile owner; no project-controlled script selector.
use super::assert_d_path;
use std::fs;
use std::path::Path;
use std::process::Command;

const COMMON: [(&str, &[u8]); 2] = [
    (
        "keld.config.ts",
        include_bytes!("../../fixtures/original-bridge-config.ts"),
    ),
    (
        "bootstrap/native-preload.cjs",
        include_bytes!("../../fixtures/original-bridge-preload.fixture"),
    ),
];
const BRIDGE: [(&str, &[u8]); 5] = [
    COMMON[0],
    COMMON[1],
    (
        "main/app.mjs",
        include_bytes!("../../fixtures/original-bridge-main.mjs"),
    ),
    (
        "client/assets/editor.html",
        include_bytes!("../../fixtures/original-bridge-editor.html"),
    ),
    (
        "client/assets/original-bridge-probe.js",
        include_bytes!("../../fixtures/original-bridge-probe.js"),
    ),
];
const INLINE: [(&str, &[u8]); 5] = [
    BRIDGE[0],
    BRIDGE[1],
    BRIDGE[2],
    (
        "client/assets/editor.html",
        include_bytes!("../../fixtures/original-inline-editor.html"),
    ),
    BRIDGE[4],
];
const FRAME: [(&str, &[u8]); 6] = [
    COMMON[0],
    COMMON[1],
    (
        "main/app.mjs",
        include_bytes!("../../fixtures/original-frame-main.mjs"),
    ),
    (
        "client/assets/editor.html",
        include_bytes!("../../fixtures/original-frame-editor.html"),
    ),
    (
        "client/assets/original-frame-top.js",
        include_bytes!("../../fixtures/original-frame-top.js"),
    ),
    (
        "client/assets/original-frame-child.js",
        include_bytes!("../../fixtures/original-frame-child.js"),
    ),
];
const MISSING_REGISTRATION: [(&str, &[u8]); 5] = [
    COMMON[0],
    (
        "bootstrap/native-preload.cjs",
        include_bytes!("../../fixtures/original-startup-preload.fixture"),
    ),
    (
        "main/app.mjs",
        include_bytes!("../../fixtures/original-startup-main.mjs"),
    ),
    (
        "client/assets/editor.html",
        include_bytes!("../../fixtures/original-startup-editor.html"),
    ),
    (
        "client/assets/original-startup-probe.js",
        include_bytes!("../../fixtures/original-startup-probe.js"),
    ),
];
const RESOURCE_HOLD: [(&str, &[u8]); 6] = [
    COMMON[0],
    COMMON[1],
    (
        "main/app.mjs",
        include_bytes!("../../fixtures/original-resource-main.mjs"),
    ),
    (
        "client/assets/editor.html",
        include_bytes!("../../fixtures/original-resource-editor.html"),
    ),
    (
        "client/assets/original-resource-probe.js",
        include_bytes!("../../fixtures/original-resource-probe.js"),
    ),
    (
        "client/assets/x14-held-resource.svg",
        include_bytes!("../../fixtures/x14-held-resource.svg"),
    ),
];
const CANCELLED_NAVIGATION: [(&str, &[u8]); 5] = [
    COMMON[0],
    COMMON[1],
    (
        "main/app.mjs",
        include_bytes!("../../fixtures/original-cancelled-navigation-main.mjs"),
    ),
    (
        "client/assets/editor.html",
        include_bytes!("../../fixtures/original-cancelled-navigation-editor.html"),
    ),
    (
        "client/assets/original-cancelled-navigation-probe.js",
        include_bytes!("../../fixtures/original-cancelled-navigation-probe.js"),
    ),
];

const QUEUED_CLOSE: [(&str, &[u8]); 5] = [
    COMMON[0],
    COMMON[1],
    (
        "main/app.mjs",
        include_bytes!("../../fixtures/original-queued-close-main.mjs"),
    ),
    (
        "client/assets/editor.html",
        include_bytes!("../../fixtures/original-queued-close-editor.html"),
    ),
    (
        "client/assets/original-queued-close-probe.js",
        include_bytes!("../../fixtures/original-queued-close-probe.js"),
    ),
];

const NAVIGATION: [(&str, &[u8]); 5] = [
    COMMON[0],
    COMMON[1],
    (
        "main/app.mjs",
        include_bytes!("../../fixtures/original-navigation-main.mjs"),
    ),
    (
        "client/assets/editor.html",
        include_bytes!("../../fixtures/original-navigation-editor.html"),
    ),
    (
        "client/assets/original-navigation-probe.js",
        include_bytes!("../../fixtures/original-navigation-probe.js"),
    ),
];

const SELECTED_DIAGNOSTIC: [(&str, &[u8]); 5] = [
    COMMON[0],
    COMMON[1],
    (
        "main/app.mjs",
        include_bytes!("../../fixtures/original-selected-diagnostic-main.mjs"),
    ),
    (
        "client/assets/editor.html",
        include_bytes!("../../fixtures/original-selected-diagnostic-editor.html"),
    ),
    (
        "client/assets/original-selected-diagnostic-probe.js",
        include_bytes!("../../fixtures/original-selected-diagnostic-probe.js"),
    ),
];

const NATIVE_CAPACITY: [(&str, &[u8]); 5] = [
    COMMON[0],
    COMMON[1],
    (
        "main/app.mjs",
        include_bytes!("../../fixtures/original-native-capacity-main.mjs"),
    ),
    (
        "client/assets/editor.html",
        include_bytes!("../../fixtures/original-native-capacity-editor.html"),
    ),
    (
        "client/assets/original-native-capacity-probe.js",
        include_bytes!("../../fixtures/original-native-capacity-probe.js"),
    ),
];

#[derive(Clone, Copy)]
pub(crate) enum Fixture {
    Bridge,
    FirstInline,
    InitialFrame,
    MissingRegistration,
    ResourceHold,
    CancelledNavigation,
    QueuedCloseBefore,
    QueuedCloseAfter,
    ClosedProducer,
    SelectedDiagnostic,
    NativeCapacity,
    NavigationBefore,
    NavigationAfter,
    NavigationSynthetic,
}

impl Fixture {
    pub(super) fn observes_frame(self) -> bool {
        matches!(self, Self::InitialFrame)
    }

    pub(super) fn verify(self, project: &Path) {
        for (relative, expected) in self.inputs() {
            assert_eq!(
                fs::read(project.join(relative)).expect("exact fixture input"),
                *expected,
                "selected {relative}"
            );
        }
        if self.observes_frame()
            || matches!(
                self,
                Self::FirstInline
                    | Self::MissingRegistration
                    | Self::ResourceHold
                    | Self::CancelledNavigation
                    | Self::QueuedCloseBefore
                    | Self::QueuedCloseAfter
                    | Self::ClosedProducer
                    | Self::SelectedDiagnostic
                    | Self::NavigationBefore
                    | Self::NavigationAfter
                    | Self::NavigationSynthetic
                    | Self::NativeCapacity
            )
        {
            let manifest: serde_json::Value = serde_json::from_slice(
                &fs::read(project.join("keld.permissions.jsonc")).expect("actual permissions"),
            )
            .expect("fixture uses the JSON subset of JSONC");
            assert_eq!(manifest["profile"], "legacy");
            let windows = manifest["windows"]
                .as_object()
                .expect("actual window grants");
            assert_eq!(windows.len(), 1, "one selected fixture window grant row");
            let row = windows.values().next().expect("selected window row");
            assert_eq!(row["web"]["csp"], "original-editor-offline-empty-style");
            let channels = row["channels"].as_array().expect("actual channel array");
            let mut actual: Vec<_> = channels
                .iter()
                .map(|value| value.as_str().expect("literal grant"))
                .collect();
            actual.sort_unstable();
            let mut expected = [
                "el:rendererReq",
                "el:mainResp",
                "el:app-load-finished",
                "el:args-obj",
                "el:resize",
                "el:maximize",
                "el:unmaximize",
                "el:isModified",
                "el:isModified-result",
                "el:removeDraft",
                "el:draftRemoved",
                "el:saveAndClose",
                "el:saveAndClose-result",
            ];
            expected.sort_unstable();
            assert_eq!(
                actual, expected,
                "exact reviewed13; no frame-specific grant"
            );
        }
    }

    pub(super) fn configure_native_hold(self, command: &mut Command, native_hold: Option<&Path>) {
        if matches!(self, Self::NativeCapacity) {
            let root = native_hold.expect("native pressure owns its condition root");
            // Reuse the same closed controls and their ordinary-file validation.
            Self::SelectedDiagnostic.configure_native_hold(command, Some(&root.join("physical")));
            Self::QueuedCloseBefore.configure_native_hold(command, Some(&root.join("native")));
            command.env("KELD_TEST_X11_NATIVE_PRESSURE", "four-route-ready-v1");
            return;
        }
        let (environment, point, markers) = match self {
            Self::ResourceHold => (
                "KELD_TEST_WINDOWS_NATIVE_HOLD",
                Some("original-resource-result"),
                ["release", "checkpoint"],
            ),
            Self::QueuedCloseBefore | Self::NavigationBefore => (
                "KELD_TEST_WINDOWS_NATIVE_HOLD",
                Some("renderer-outcomes"),
                ["arm", "release"],
            ),
            Self::QueuedCloseAfter | Self::NavigationAfter => (
                "KELD_TEST_WINDOWS_NATIVE_HOLD",
                Some("renderer-after-event"),
                ["arm", "release"],
            ),
            Self::ClosedProducer => (
                "KELD_TEST_WINDOWS_NATIVE_HOLD",
                Some("renderer-fresh-offers"),
                ["arm", "release"],
            ),
            Self::NavigationSynthetic => (
                "KELD_TEST_WINDOWS_NATIVE_HOLD",
                Some("original-post-synthetic"),
                ["arm", "release"],
            ),
            Self::SelectedDiagnostic => ("KELD_TEST_PRIMARY_VALUE_HOLD", None, ["arm", "release"]),
            _ => {
                assert!(native_hold.is_none(), "ordinary fixture has no native hold");
                return;
            }
        };
        let directory = native_hold.expect("fixed fixture requires its owned hold directory");
        assert_d_path(directory);
        assert!(directory.is_dir(), "owned native hold directory");
        for name in markers {
            assert!(
                matches!(
                    fs::symlink_metadata(directory.join(name)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound
                ),
                "fresh native fixture barrier: {name}"
            );
        }
        let value = point.map_or_else(
            || directory.display().to_string(),
            |point| format!("{point}:{}", directory.display()),
        );
        command.env(environment, value);
    }

    fn inputs(self) -> &'static [(&'static str, &'static [u8])] {
        match self {
            Self::Bridge => &BRIDGE,
            Self::FirstInline => &INLINE,
            Self::InitialFrame => &FRAME,
            Self::MissingRegistration => &MISSING_REGISTRATION,
            Self::ResourceHold => &RESOURCE_HOLD,
            Self::CancelledNavigation => &CANCELLED_NAVIGATION,
            Self::QueuedCloseBefore | Self::QueuedCloseAfter | Self::ClosedProducer => {
                &QUEUED_CLOSE
            }
            Self::SelectedDiagnostic => &SELECTED_DIAGNOSTIC,
            Self::NativeCapacity => &NATIVE_CAPACITY,
            Self::NavigationBefore | Self::NavigationAfter | Self::NavigationSynthetic => {
                &NAVIGATION
            }
        }
    }
}
