//! Ordinary inputs for the combined facade/profile recovery scenario.
use super::project::{Policy, PublicFsProject, file_digest};
use crate::renderer_bridge::bundle_public_api_entry;
use std::fs;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

pub(super) struct RecoveryFixture {
    pub(super) stage: keld_cli::boot::DevBootStage,
    pub(super) project: PublicFsProject,
    pub(super) target: PathBuf,
    pub(super) digest: String,
    pub(super) control: UnixListener,
    pub(super) control_path: PathBuf,
}

impl RecoveryFixture {
    pub(super) fn new() -> Self {
        let project = PublicFsProject::new(Policy::Narrow).with_declared_profile("legacy");
        let target = project.allowed_root().join("facade-recovery.bin");
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace");
        let api =
            serde_json::to_string(&repo.join("packages/@keld/api/src/index.ts")).expect("API path");
        let transport = serde_json::to_string(&repo.join("packages/@keld/kipc/src/transport.ts"))
            .expect("canonical transport path");
        let application_observer_path =
            serde_json::to_string(&repo.join("packages/@keld/api/src/app.ts"))
                .expect("existing application state observer");
        let role = include_str!("../../../fixtures/facade_profile_recovery.ts")
            .replace(
                "__TARGET__",
                &serde_json::to_string(&target).expect("target"),
            )
            .replace(
                "__OUTSIDE__",
                &serde_json::to_string(&project.outside).expect("outside"),
            );
        bundle_public_api_entry(
            project.project_root(),
            repo,
            "facade-profile-recovery",
            &format!(
                "import {{ app, fs, isCallError }} from {api};\nimport {{ getWindowSnapshots }} from {application_observer_path};\nimport {{ WorkerLink, LIFECYCLE_CHANNEL, WINDOW_CHANNEL, nativeFacadeBootSelected }} from {transport};\n{role}"
            ),
        );
        fs::write(project.project_root().join("keld.config.ts"),
            "export default { name: 'Facade Profile Recovery', entry: 'src/main.ts', initialWindow: 'none' } as const;\n").expect("explicit facade input");
        let stage = keld_cli::boot::stage_dev_boot(
            project.project_root(),
            Path::new(env!("CARGO_BIN_EXE_keld-host")),
        )
        .expect("stock compiler");
        let descriptor: serde_json::Value = serde_json::from_slice(
            &fs::read(stage.root().join("keld.boot.json")).expect("boot bytes"),
        )
        .expect("boot JSON");
        eprintln!("KEL78_SCHEMA4_BOOT {descriptor}");
        assert_eq!(descriptor["schema"], 4);
        assert!(descriptor.get("renderer").is_none() && descriptor.get("auth").is_none());
        assert!(!stage.root().join("index.html").exists());
        let digest = file_digest(&stage.root().join("keld.permissions.jsonc"));
        assert_eq!(
            digest,
            file_digest(&project.project_root().join("keld.permissions.jsonc"))
        );
        assert_eq!(
            descriptor["permissions"]["content_sha256"],
            format!("sha256:{digest}")
        );
        let control_path = project.owned_root().join("facade-recovery.sock");
        let control = UnixListener::bind(&control_path).expect("test control");
        control.set_nonblocking(true).expect("bounded accept");
        Self {
            stage,
            project,
            target,
            digest,
            control,
            control_path,
        }
    }
}
