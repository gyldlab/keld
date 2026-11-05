//! Staged facade project and isolated profile resources. The existing run owner
//! retains all Host/Bun/process/pipe lifetimes separately.

use super::{BUILD_FAILURE, FacadeRun, NATIVE_HOLD, ephemeral_profile_dir};
use crate::facade_ready::stage_boot;
use crate::support::product::{ProductFixture, bundle_public_role};
use crate::support::profile_run::{
    ProfileTestRoot, actual_webview_process, assert_profile_test_root_feature,
};
use crate::{PRODUCT_DEADLINE, dev_stage_command};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::net::TcpListener;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LastWindowAction {
    Hold,
    DefaultQuit,
    DrawIoQuit,
}

impl LastWindowAction {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::DefaultQuit => "default",
            Self::DrawIoQuit => "drawio",
        }
    }
}

pub(crate) struct FacadeFixture {
    // Stage pins must retire before the project's TempDir, including on unwind.
    pub(crate) stage: keld_cli::boot::DevBootStage,
    pub(crate) project: ProductFixture,
    profiles: ProfileTestRoot,
    control: TcpListener,
}

impl FacadeFixture {
    pub(crate) fn new() -> Self {
        Self::with_last_window_action(LastWindowAction::Hold)
    }

    pub(crate) fn with_last_window_action(action: LastWindowAction) -> Self {
        let project = ProductFixture::schema_two();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        let import = |relative: &str| {
            serde_json::to_string(&repo.join(relative).to_string_lossy())
                .expect("absolute canonical package import")
        };
        let source = format!(
            "import {{ app, BrowserWindow }} from {};\n\
             import {{ app as hostApp, getWindowSnapshots, getDispatchedPendingWindowCloses, invokeWindow, invokeWindowBlocking, onWindowFactApplied }} from {};\nconst lastWindowAction: string = {};\nconst closeHoldRoot: string = {};\nconst closeHoldDeadlineMs = {};\n{}",
            import("packages/@keld/electron/src/index.ts"),
            import("packages/@keld/api/src/app.ts"),
            serde_json::to_string(action.as_str()).expect("fixed fixture last-window action"),
            serde_json::to_string(&project.project.join("close-hold"))
                .expect("owned fixture close hold"),
            PRODUCT_DEADLINE.as_millis(),
            include_str!("../fixtures/facade-window.ts"),
        );
        bundle_public_role(
            repo,
            &project.project,
            &source,
            &project.project.join("src/main.ts"),
        );
        fs::copy(
            repo.join("packages/@keld/kipc/src/transport.ts"),
            project.project.join("src/kipc-transport.ts"),
        )
        .expect("stage the one canonical transport sidecar");
        let stage = stage_boot(&project.project);
        let descriptor: Value = serde_json::from_slice(
            &fs::read(stage.root().join("keld.boot.json")).expect("staged descriptor"),
        )
        .expect("schema-two descriptor");
        assert_eq!(descriptor["schema"], 2);
        assert!(descriptor.get("renderer").is_none());
        let profiles = ProfileTestRoot::new();
        // An absent debug profile-root feature fails this prerequisite, never
        // silently redirects the cleanup checks into the user's profile store.
        assert_profile_test_root_feature(stage.host(), profiles.path());
        let control = TcpListener::bind(("127.0.0.1", 0)).expect("facade control listener");
        Self {
            project,
            stage,
            profiles,
            control,
        }
    }

    pub(crate) fn spawn(&self, hooks: &[(&str, String)]) -> (FacadeRun, Value) {
        let mut command = dev_stage_command(self.stage.root(), self.stage.host());
        command
            .env(
                "KELD_T1B_CONTROL",
                self.control
                    .local_addr()
                    .expect("control address")
                    .port()
                    .to_string(),
            )
            .env("KELD_PROFILE_TEST_ROOT", self.profiles.path())
            .env(
                "TEMP",
                self.profiles.path().parent().expect("test temp parent"),
            )
            .env(
                "TMP",
                self.profiles.path().parent().expect("test temp parent"),
            )
            .env("KELD_TEST_WINDOWS_T2_OBSERVE", "1")
            .env_remove(BUILD_FAILURE)
            .env_remove(NATIVE_HOLD)
            .env_remove("KELD_TEST_WINDOWS_PAGE_READBACK")
            .env_remove("KELD_TEST_WINDOWS_HANG_GUARD_SECS")
            .env_remove("KELD_TEST_WINDOWS_BARRIER_GRACE_EXPIRY")
            .env_remove("KELD_TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in hooks {
            command.env(name, value);
        }
        FacadeRun::spawn(command, &self.control)
    }

    pub(crate) fn reconnect(&self, run: &mut FacadeRun) -> Value {
        run.reconnect(&self.control)
    }

    pub(crate) fn profile_dir(&self) -> PathBuf {
        ephemeral_profile_dir(self.profiles.path())
    }

    pub(crate) fn assert_lease_held(&self) {
        let lease = self.profile_dir().join("profile.lock");
        let error = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(lease)
            .expect_err("the live engine must retain its share-none profile lease");
        assert_eq!(
            error.raw_os_error(),
            Some(32),
            "ERROR_SHARING_VIOLATION: {error}"
        );
    }

    pub(crate) fn browser(&self, run: &FacadeRun) -> Value {
        actual_webview_process(
            run.pid(),
            self.stage.host(),
            &self.profile_dir().join("webview2"),
            Instant::now() + PRODUCT_DEADLINE,
        )
    }

    pub(crate) fn close(self) {
        let Self {
            stage,
            project,
            profiles,
            control,
        } = self;
        drop(control);
        drop(stage);
        profiles.remove();
        project
            .root
            .close()
            .expect("remove facade project after stage retirement");
    }
}

#[cfg(all(test, feature = "profile-test-hooks"))]
mod tests {
    use super::FacadeFixture;
    use std::fs;

    fn assert_project_retired(explicit_close: bool) {
        let fixture = FacadeFixture::new();
        let project = fixture.project.root.path().to_path_buf();
        let profiles = fixture
            .profiles
            .path()
            .parent()
            .expect("owned profile temporary directory")
            .to_path_buf();
        if explicit_close {
            fixture.close();
        } else {
            drop(fixture);
        }
        let project_remained = project.try_exists().expect("inspect owned project");
        let profiles_remained = profiles.try_exists().expect("inspect owned profiles");
        // Recover only this test's retained paths after every owner has dropped.
        // An intended RED must not leave a fixture behind or hide a cleanup error.
        if project_remained {
            fs::remove_dir_all(&project).expect("recover released test project");
        }
        if profiles_remained {
            fs::remove_dir_all(&profiles).expect("recover released test profiles");
        }
        assert!(
            !project.exists() && !profiles.exists(),
            "recovery must finish"
        );
        println!(
            "KELD_FACADE_FIXTURE_RETIRED explicit_close={explicit_close} project_remained={project_remained} profiles_remained={profiles_remained} recovered=true project={} profiles={}",
            project.display(),
            profiles.display()
        );
        assert!(
            !profiles_remained,
            "fixture must retire its profile directory"
        );
        assert!(
            !project_remained,
            "fixture must retire its project after releasing stage handles"
        );
    }

    #[test]
    fn closed_facade_fixture_releases_stage_before_project() {
        assert_project_retired(true);
    }

    #[test]
    fn dropped_facade_fixture_releases_stage_before_project() {
        assert_project_retired(false);
    }
}
