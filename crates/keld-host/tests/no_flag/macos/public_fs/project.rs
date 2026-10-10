//! Owns ordinary project inputs and stock CLI preparation, never live authority.
use crate::renderer_bridge::bundle_public_api_entry;
use crate::support::native_window::compile_native_window_census;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) const TITLE: &str = "KEL140 Public FS Acceptance";
pub(super) const CONTENT: &[u8] = &[0, 255, 65, 226, 130, 185];
pub(super) const SENTINEL: &[u8] = b"KEL140-outside-sentinel-unchanged";

#[derive(Clone, Copy, Debug)]
pub(super) enum Policy {
    Narrow,
    Empty,
    Absent,
}

pub(super) struct PublicFsProject {
    _owned: tempfile::TempDir,
    project: PathBuf,
    repo: PathBuf,
    allowed: PathBuf,
    policy_bytes: Vec<u8>,
    pub(super) policy: Policy,
    pub(super) outside: PathBuf,
    pub(super) scope: String,
    pub(super) census: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Exercise {
    RendererPointer,
    PublicAppComponent,
}

pub(super) struct PreparedLaunch {
    pub(super) exercise: Exercise,
    pub(super) stage: keld_cli::boot::DevBootStage,
    pub(super) target: PathBuf,
    pub(super) digest: String,
    pub(super) host_digest: String,
}

impl PublicFsProject {
    pub(super) fn new(policy: Policy) -> Self {
        let owned = tempfile::tempdir().expect("owned public FS project");
        let root = fs::canonicalize(owned.path()).expect("canonical project parent");
        let project = root.join("public-fs");
        fs::create_dir_all(project.join("src")).expect("ordinary project source directory");
        let allowed = root.join("allowed");
        fs::create_dir(&allowed).expect("existing granted scope root");
        let scope = format!("{}/**", allowed.to_str().expect("UTF-8 scope"));
        let outside = root.join("outside.txt");
        fs::write(&outside, SENTINEL).expect("independent outside sentinel");
        let policy_bytes = match policy {
            Policy::Narrow => json!({"app":{"fs":{"read":[scope],"write":[scope]}}})
                .to_string()
                .into_bytes(),
            Policy::Empty => b"{}".to_vec(),
            Policy::Absent => b"{}\n".to_vec(),
        };
        if !matches!(policy, Policy::Absent) {
            fs::write(project.join("keld.permissions.jsonc"), &policy_bytes)
                .expect("explicit project policy");
        }
        fs::write(project.join("keld.config.ts"), format!(
            "export default {{\n  name: {TITLE:?},\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n}} as const;\n"
        )).expect("ordinary configured project");
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("repository root")
            .to_path_buf();
        let census = compile_native_window_census(&root);
        Self {
            _owned: owned,
            project,
            repo,
            allowed,
            policy_bytes,
            policy,
            outside,
            scope,
            census,
        }
    }

    pub(super) fn with_project_policy_source(bytes: &[u8]) -> Self {
        let mut fixture = Self::new(Policy::Narrow);
        fixture.policy_bytes = bytes.to_vec();
        fs::write(
            fixture.project.join("keld.permissions.jsonc"),
            &fixture.policy_bytes,
        )
        .expect("literal project policy input before stock staging");
        fixture
    }

    pub(super) fn owned_root(&self) -> &Path {
        self.project
            .parent()
            .expect("canonical owned project parent")
    }

    pub(super) fn prepare(&self, launch: usize, port: u16) -> PreparedLaunch {
        self.prepare_for(launch, port, Exercise::RendererPointer)
    }

    pub(super) fn prepare_component(&self, launch: usize, port: u16) -> PreparedLaunch {
        self.prepare_for(launch, port, Exercise::PublicAppComponent)
    }

    fn prepare_for(&self, launch: usize, port: u16, exercise: Exercise) -> PreparedLaunch {
        let target = self.allowed.join(format!("result-{launch}.bin"));
        let api = serde_json::to_string(
            &self
                .repo
                .join("packages/@keld/api/src/index.ts")
                .to_string_lossy(),
        )
        .expect("public index path JSON");
        let body = include_str!("../../../fixtures/public_fs_app.ts")
            .replace(
                "__AUTOMATIC_COMPONENT__",
                if exercise == Exercise::PublicAppComponent {
                    "true"
                } else {
                    "false"
                },
            )
            .replace("__PORT__", &port.to_string())
            .replace(
                "__TARGET__",
                &serde_json::to_string(&target.to_string_lossy()).expect("target JSON"),
            )
            .replace(
                "__OUTSIDE__",
                &serde_json::to_string(&self.outside.to_string_lossy()).expect("outside JSON"),
            )
            .replace(
                "__CONTENT__",
                &serde_json::to_string(CONTENT).expect("content JSON"),
            );
        assert!(!body.contains("WorkerLink") && !body.contains("invokeFs"));
        bundle_public_api_entry(
            &self.project,
            &self.repo,
            "kel140-public-fs",
            &format!(
                "import {{ app, channels, echoChannel, fs, isCallError }} from {api};\n{body}"
            ),
        );
        fs::write(
            self.project.join("index.html"),
            include_str!("../../../fixtures/public_fs_page.html")
                .replace("__PORT__", &port.to_string()),
        )
        .expect("public FS renderer");
        let stage = keld_cli::boot::stage_dev_boot(
            &self.project,
            Path::new(env!("CARGO_BIN_EXE_keld-host")),
        )
        .expect("stock CLI stages explicit project inputs");
        assert_eq!(
            fs::read(stage.root().join("keld.permissions.jsonc")).expect("staged bytes"),
            self.policy_bytes
        );
        let boot: Value = serde_json::from_slice(
            &fs::read(stage.root().join("keld.boot.json")).expect("boot bytes"),
        )
        .expect("boot descriptor");
        eprintln!("KELD_KEL140_STAGED_BOOT {boot}");
        assert_eq!(
            boot["name"], TITLE,
            "stock producer selected the configured window name"
        );
        assert_eq!(
            boot["entry"], "src/main.ts",
            "stock producer selected the public app entry"
        );
        assert_eq!(
            boot["renderer"], "index.html",
            "stock producer selected the fixture document"
        );
        let digest = file_digest(&stage.root().join("keld.permissions.jsonc"));
        assert_eq!(
            boot["permissions"]["content_sha256"],
            format!("sha256:{digest}")
        );
        let host_digest = file_digest(stage.host());
        assert_eq!(
            host_digest,
            file_digest(Path::new(env!("CARGO_BIN_EXE_keld-host")))
        );
        PreparedLaunch {
            exercise,
            stage,
            target,
            digest,
            host_digest,
        }
    }
}

fn file_digest(path: &Path) -> String {
    let digest = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .expect("independent staged byte digest");
    assert!(digest.status.success(), "{digest:?}");
    String::from_utf8(digest.stdout)
        .expect("hash UTF-8")
        .split_whitespace()
        .next()
        .expect("SHA256 field")
        .to_owned()
}
