//! Owns ordinary project inputs and stock CLI preparation, never live authority.
use crate::renderer_bridge::bundle_public_api_entry;
use crate::support::native_window::compile_native_window_census;
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) const TITLE: &str = "KEL140 Public FS Acceptance";
pub(super) const CONTENT: &[u8] = &[0, 255, 65, 226, 130, 185];
pub(super) const SENTINEL: &[u8] = b"KEL140-outside-sentinel-unchanged";
pub(super) const READ_SENTINEL: &[u8] = b"KEL140-read-sentinel-must-not-be-returned";
pub(super) const INTERNAL_INITIAL: &[u8] = b"KEL140-internal-link-original";
pub(super) const WRITE_SENTINEL: &[u8] = b"KEL140-oversize-write-target-unchanged";
pub(super) const COMPONENT_SENTINEL: &[u8] = b"KEL140-component-boundary-unchanged";
pub(super) const LINK_SENTINEL: &[u8] = b"KEL140-link-boundary-unchanged";
pub(super) const RECOVERY_AFTER_CONTENT: &[u8] = &[7, 0, 128, 255, 42];

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
    natural_case: NaturalCase,
    descendant_quit: Value,
    pub(super) policy: Policy,
    pub(super) outside: PathBuf,
    pub(super) scope: String,
    pub(super) census: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Exercise {
    RendererPointer,
    RendererRecovery,
    RendererPendingRecovery,
    RendererNativeJoint,
    PublicAppComponent,
    PublicAppNaturalCases,
    PublicAppRetirement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NaturalCase {
    DenyRead,
    LinksLimits,
    TraversalBounds,
}

pub(super) struct NaturalInputs {
    pub(super) case: NaturalCase,
    pub(super) read_sentinel: PathBuf,
    pub(super) internal_target: PathBuf,
    pub(super) internal_alias: PathBuf,
    pub(super) external_alias: PathBuf,
    pub(super) oversized_write: PathBuf,
    pub(super) oversized_read: PathBuf,
    pub(super) fresh: PathBuf,
    pub(super) maximum_content: usize,
    pub(super) oversized_read_digest: String,
    pub(super) component_sentinel: PathBuf,
    pub(super) components_pass: PathBuf,
    pub(super) components_deny: PathBuf,
    pub(super) link_sentinel: PathBuf,
    pub(super) links_pass: PathBuf,
    pub(super) links_deny: PathBuf,
}

pub(super) struct PreparedLaunch {
    pub(super) exercise: Exercise,
    pub(super) stage: keld_cli::boot::DevBootStage,
    pub(super) target: PathBuf,
    pub(super) digest: String,
    pub(super) host_digest: String,
    pub(super) natural: Option<NaturalInputs>,
    pub(super) recovery: Option<RecoveryInputs>,
    pub(super) bun_image: PathBuf,
}

pub(super) struct RecoveryInputs {
    pub(super) after: PathBuf,
    pub(super) witness: PathBuf,
    pub(super) pending_handler: bool,
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
            natural_case: if matches!(policy, Policy::Narrow) {
                NaturalCase::LinksLimits
            } else {
                NaturalCase::DenyRead
            },
            descendant_quit: Value::Null,
            policy,
            outside,
            scope,
            census,
        }
    }

    pub(super) fn with_traversal_boundaries() -> Self {
        let mut fixture = Self::new(Policy::Narrow);
        fixture.natural_case = NaturalCase::TraversalBounds;
        fixture
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

    /// Ordinary fixture data only; no host flag, grant, principal or policy change.
    pub(super) fn set_descendant_quit(&mut self, configuration: Value) {
        assert!(configuration.is_null() || configuration.is_object());
        self.descendant_quit = configuration;
    }

    pub(super) fn prepare(&self, launch: usize, port: u16) -> PreparedLaunch {
        self.prepare_for(launch, port, Exercise::RendererPointer)
    }

    pub(super) fn prepare_component(&self, launch: usize, port: u16) -> PreparedLaunch {
        self.prepare_for(launch, port, Exercise::PublicAppComponent)
    }

    /// Only the selected native causal witness arms a separate test-owned port.
    pub(super) fn prepare_component_with_quit_body_receipt(
        &self,
        launch: usize,
        port: u16,
        receipt_port: u16,
        token: &str,
    ) -> PreparedLaunch {
        self.prepare_for_with_receipt(
            launch,
            port,
            Exercise::PublicAppComponent,
            Some((receipt_port, token)),
        )
    }

    pub(super) fn prepare_natural(&self, launch: usize, port: u16) -> PreparedLaunch {
        self.prepare_for(launch, port, Exercise::PublicAppNaturalCases)
    }

    pub(super) fn prepare_retirement(&self, launch: usize, port: u16) -> PreparedLaunch {
        self.prepare_for(launch, port, Exercise::PublicAppRetirement)
    }

    pub(super) fn prepare_recovery(&self, launch: usize, port: u16) -> PreparedLaunch {
        self.prepare_for(launch, port, Exercise::RendererRecovery)
    }

    pub(super) fn prepare_pending_recovery(&self, launch: usize, port: u16) -> PreparedLaunch {
        self.prepare_for(launch, port, Exercise::RendererPendingRecovery)
    }

    pub(super) fn prepare_native_joint(&self, launch: usize, port: u16) -> PreparedLaunch {
        self.prepare_for(launch, port, Exercise::RendererNativeJoint)
    }

    /// Replace fixture resources, never the captured policy or host authority.
    pub(super) fn replace_recovery_anchor(&self, prepared: &PreparedLaunch) -> PathBuf {
        use std::os::unix::fs::MetadataExt as _;
        let before = fs::metadata(&self.allowed).expect("G1 granted root identity");
        let retired = self.owned_root().join("retired-allowed");
        fs::rename(&self.allowed, &retired).expect("rename original granted root");
        fs::create_dir(&self.allowed).expect("distinct replacement granted root");
        let after = fs::metadata(&self.allowed).expect("replacement root identity");
        assert_ne!((before.dev(), before.ino()), (after.dev(), after.ino()));
        let input = prepared.recovery.as_ref().expect("owned recovery inputs");
        fs::write(&input.witness, [1]).expect("seed replacement replay witness");
        eprintln!(
            "KELD_KEL143_ANCHOR_REPLACED old={:?} new={:?}",
            (before.dev(), before.ino()),
            (after.dev(), after.ino())
        );
        retired
    }

    fn natural_inputs(&self, launch: usize) -> NaturalInputs {
        let read_sentinel = self.allowed.join(format!("read-sentinel-{launch}.bin"));
        fs::write(&read_sentinel, READ_SENTINEL).expect("independent read-only sentinel");
        let internal_target = self.allowed.join(format!("internal-target-{launch}.bin"));
        let internal_alias = self.allowed.join(format!("internal-alias-{launch}"));
        let external_alias = self.allowed.join(format!("external-alias-{launch}"));
        let oversized_write = self.allowed.join(format!("oversized-write-{launch}.bin"));
        let oversized_read = self.allowed.join(format!("oversized-read-{launch}.bin"));
        let fresh = self.allowed.join(format!("fresh-control-{launch}.bin"));
        let maximum_content = native_content_ceiling();
        let mut oversized_read_digest = String::new();
        if self.natural_case == NaturalCase::LinksLimits {
            fs::write(&internal_target, INTERNAL_INITIAL).expect("inside link target");
            symlink(
                internal_target.file_name().expect("inside relative leaf"),
                &internal_alias,
            )
            .expect("internal relative alias");
            // The alias spelling is in scope; its relative target escapes the
            // retained root, independently of the guard's literal path match.
            symlink("../outside.txt", &external_alias).expect("external relative alias");
            fs::write(&oversized_write, WRITE_SENTINEL).expect("oversized write sentinel");
            fs::write(&oversized_read, vec![0x79; maximum_content + 1])
                .expect("parent-owned oversized read fixture");
            oversized_read_digest = file_digest(&oversized_read);
        }
        let component_sentinel = self
            .allowed
            .join(format!("component-sentinel-{launch}.bin"));
        let components_pass = self.allowed.join(format!("components-pass-{launch}"));
        let components_deny = self.allowed.join(format!("components-deny-{launch}"));
        let link_sentinel = self.allowed.join(format!("link-sentinel-{launch}.bin"));
        let links_pass = self.allowed.join(format!("links-pass-{launch}-0"));
        let links_deny = self.allowed.join(format!("links-deny-{launch}-0"));
        if self.natural_case == NaturalCase::TraversalBounds {
            fs::write(&component_sentinel, COMPONENT_SENTINEL)
                .expect("independent component-boundary sentinel");
            fs::write(&link_sentinel, LINK_SENTINEL).expect("independent link-boundary sentinel");
            let component_leaf = component_sentinel
                .file_name()
                .and_then(|name| name.to_str())
                .expect("component sentinel relative leaf");
            // The alias visit, literal Current components and final leaf are
            // 256/257. Pin the boundary independently of native constants.
            for (alias, current_components) in
                [(&components_pass, 254_usize), (&components_deny, 255_usize)]
            {
                symlink(
                    format!("{}{component_leaf}", "./".repeat(current_components)),
                    alias,
                )
                .expect("short component-boundary relative alias");
            }
            for (prefix, count) in [
                (format!("links-pass-{launch}"), 40_usize),
                (format!("links-deny-{launch}"), 41_usize),
            ] {
                for index in 0..count {
                    let target = if index + 1 == count {
                        PathBuf::from(link_sentinel.file_name().expect("link sentinel leaf"))
                    } else {
                        PathBuf::from(format!("{prefix}-{}", index + 1))
                    };
                    symlink(target, self.allowed.join(format!("{prefix}-{index}")))
                        .expect("relative link-expansion boundary chain");
                }
            }
        }
        NaturalInputs {
            case: self.natural_case,
            read_sentinel,
            internal_target,
            internal_alias,
            external_alias,
            oversized_write,
            oversized_read,
            fresh,
            maximum_content,
            oversized_read_digest,
            component_sentinel,
            components_pass,
            components_deny,
            link_sentinel,
            links_pass,
            links_deny,
        }
    }

    fn prepare_for(&self, launch: usize, port: u16, exercise: Exercise) -> PreparedLaunch {
        self.prepare_for_with_receipt(launch, port, exercise, None)
    }

    fn prepare_for_with_receipt(
        &self,
        launch: usize,
        port: u16,
        exercise: Exercise,
        quit_body_receipt: Option<(u16, &str)>,
    ) -> PreparedLaunch {
        let target = self.allowed.join(format!("result-{launch}.bin"));
        let natural =
            (exercise == Exercise::PublicAppNaturalCases).then(|| self.natural_inputs(launch));
        let recovery = matches!(
            exercise,
            Exercise::RendererRecovery | Exercise::RendererPendingRecovery
        )
        .then(|| {
            let witness = self.allowed.join(format!("recovery-witness-{launch}.bin"));
            fs::write(&witness, []).expect("seed independent completed-call witness");
            RecoveryInputs {
                after: self.allowed.join(format!("recovery-after-{launch}.bin")),
                witness,
                pending_handler: exercise == Exercise::RendererPendingRecovery,
            }
        });
        self.prepare_application_inputs(
            &target,
            port,
            exercise,
            natural.as_ref(),
            recovery.as_ref(),
            quit_body_receipt,
        );
        let (stage, digest, host_digest) = self.stage_and_verify_inputs();
        let bun_image = expected_source_bun_image(stage.root());
        PreparedLaunch {
            exercise,
            stage,
            target,
            digest,
            host_digest,
            natural,
            recovery,
            bun_image,
        }
    }

    fn prepare_application_inputs(
        &self,
        target: &Path,
        port: u16,
        exercise: Exercise,
        natural: Option<&NaturalInputs>,
        recovery: Option<&RecoveryInputs>,
        quit_body_receipt: Option<(u16, &str)>,
    ) {
        let receipt_config = quit_body_receipt.map_or(
            Value::Null,
            |(port, nonce)| json!({"port": port, "nonce": nonce}),
        );
        let recovery_config = recovery.map_or(Value::Null, |input| {
            json!({"after":input.after,"witness":input.witness,
                "afterContent":RECOVERY_AFTER_CONTENT,"pendingHandler":input.pending_handler})
        });
        let natural_case = natural.map_or(Value::Null, |input| {
            json!(match input.case {
                NaturalCase::DenyRead => "deny-read",
                NaturalCase::LinksLimits => "links-limits",
                NaturalCase::TraversalBounds => "traversal-bounds",
            })
        });
        let natural_config = natural.map_or(Value::Null, |input| {
            json!({
                "readSentinel":input.read_sentinel,
                "internalAlias":input.internal_alias,
                "externalAlias":input.external_alias,
                "oversizedWrite":input.oversized_write,
                "oversizedRead":input.oversized_read,
                "fresh":input.fresh,
                "maximumContent":input.maximum_content,
                "componentsPass":input.components_pass,
                "componentsDeny":input.components_deny,
                "linksPass":input.links_pass,
                "linksDeny":input.links_deny,
            })
        });
        let api = serde_json::to_string(
            &self
                .repo
                .join("packages/@keld/api/src/index.ts")
                .to_string_lossy(),
        )
        .expect("public index path JSON");
        let body = include_str!("../../../fixtures/public_fs_app.ts")
            .replace(
                "__RETIRE_AFTER_QUIT__",
                if exercise == Exercise::PublicAppRetirement {
                    "true"
                } else {
                    "false"
                },
            )
            .replace("__DESCENDANT_QUIT__", &self.descendant_quit.to_string())
            .replace("__RECOVERY_INPUTS__", &recovery_config.to_string())
            .replace("__QUIT_BODY_RECEIPT__", &receipt_config.to_string())
            .replace(
                "__NATIVE_JOINT__",
                if exercise == Exercise::RendererNativeJoint {
                    "true"
                } else {
                    "false"
                },
            )
            .replace("__NATURAL_CASE__", &natural_case.to_string())
            .replace("__NATURAL_INPUTS__", &natural_config.to_string())
            .replace(
                "__AUTOMATIC_COMPONENT__",
                if matches!(
                    exercise,
                    Exercise::PublicAppComponent | Exercise::PublicAppRetirement
                ) {
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
        self.prepare_renderer_page(exercise, port, recovery);
    }

    fn prepare_renderer_page(
        &self,
        exercise: Exercise,
        port: u16,
        recovery: Option<&RecoveryInputs>,
    ) {
        let page = include_str!("../../../fixtures/public_fs_page.html")
            .replace("__PORT__", &port.to_string())
            .replace(
                "__PENDING_RECOVERY__",
                if recovery.is_some_and(|input| input.pending_handler) {
                    "true"
                } else {
                    "false"
                },
            )
            .replace(
                "__RECOVERY_MODE__",
                if recovery.is_some() { "true" } else { "false" },
            )
            .replace(
                "__NATIVE_JOINT__",
                if exercise == Exercise::RendererNativeJoint {
                    "true"
                } else {
                    "false"
                },
            )
            .replace(
                "__JOINT_BUTTON__",
                match exercise {
                    Exercise::RendererNativeJoint
                    | Exercise::PublicAppComponent
                    | Exercise::PublicAppNaturalCases
                    | Exercise::PublicAppRetirement => "",
                    Exercise::RendererPointer
                    | Exercise::RendererRecovery
                    | Exercise::RendererPendingRecovery => {
                        "<button id=\"go\">Write, read and verify scope denial</button>"
                    }
                },
            );
        fs::write(self.project.join("index.html"), page).expect("public FS renderer");
    }

    fn stage_and_verify_inputs(&self) -> (keld_cli::boot::DevBootStage, String, String) {
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
        (stage, digest, host_digest)
    }
}

fn expected_source_bun_image(stage_root: &Path) -> PathBuf {
    // Resolve the same source Bun command/current directory as the guardian,
    // independently of every ordinary HTTP application identity report.
    let output = Command::new("bun")
        .current_dir(stage_root)
        .args(["-e", "process.stdout.write(process.execPath)"])
        .output()
        .expect("independent source Bun executable selection");
    assert!(output.status.success(), "source Bun selection: {output:?}");
    let image = PathBuf::from(String::from_utf8(output.stdout).expect("source Bun path UTF-8"));
    assert!(image.is_absolute(), "source Bun image must be absolute");
    fs::canonicalize(image).expect("canonical expected source Bun image")
}

pub(super) fn file_digest(path: &Path) -> String {
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

fn native_content_ceiling() -> usize {
    // Fixture input comes from the exact native owner, not a mirrored limit.
    // Admit only its current numeric-product form and fail on any schema edit.
    let source = include_str!("../../../../../keld-native/src/fs.rs");
    let declaration = source
        .lines()
        .find_map(|line| line.strip_prefix("pub const MAX_FS_CONTENT_BYTES: usize = "))
        .expect("owning native content ceiling declaration")
        .strip_suffix(';')
        .expect("owning native content ceiling terminator");
    declaration.split(" * ").fold(1_usize, |product, factor| {
        product
            .checked_mul(factor.parse::<usize>().expect("literal native size factor"))
            .expect("native fixture ceiling fits usize")
    })
}
