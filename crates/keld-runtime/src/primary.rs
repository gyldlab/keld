//! Platform primary-role coordinator names frozen by KEL-75 T1b/T8.
//!
//! The implementation lives in `role`. T2 reuses that same
//! authenticated coordinator for an independent `app-bound` slot rather than
//! wrapping a second restart loop around [`crate::Supervisor`].

pub use crate::role::{
    BUN_OPTIONS_ENV, BoundRoleGeneration as BoundPrimaryGeneration,
    RoleConfig as PrimaryRoleConfig, RoleEvent as PrimaryRoleEvent, RoleGeneration, RoleOwner,
    RolePrincipal, RoleProcessDrained, RoleRecoveryGate as PrimaryRecoveryGate,
    RoleRevocationCause as PrimaryRoleRevocationCause, RoleSupervisor as PrimaryRoleSupervisor,
    admit_staged_transport_sidecar, staged_bun_launch_args, strip_verbatim_prefix,
};

/// Removes ambient startup controls from a locally installed macOS Bun command.
///
/// The caller must already have obtained the executable and application inputs
/// from Core's validated installed origin. This function neither selects Bun nor
/// grants authority. Apply the authenticated app-link value and canonical preload
/// after this call. Development and other-platform launch behavior are unchanged.
///
/// Only ordinary user, locale and temporary-directory values are retained. Bun,
/// Node, dynamic-loader and inherited KELD controls are not copied. Explicit empty
/// runtime/TypeScript configuration, no automatic environment loading, no package
/// fetching and a disabled transpiler cache keep startup independent of developer
/// state. Application path aliases must be resolved before packaging; they cannot
/// redirect the fixed bootstrap or the verified entry at installed startup.
#[cfg(target_os = "macos")]
pub fn configure_local_macos_bun_startup(command: &mut std::process::Command) {
    local_macos_bun_startup_with_environment(command, std::env::vars_os());
}

/// Clears ambient startup controls on a verified local bundle's Guardian.
///
/// Core calls this before the existing authenticated bootstrap adds its private
/// registration and liveness data. This only constrains inherited environment;
/// it does not create a Guardian, approve a bundle, or replace authentication.
#[cfg(target_os = "macos")]
pub fn configure_local_macos_guardian_startup(command: &mut std::process::Command) {
    local_macos_environment(command, std::env::vars_os());
}

#[cfg(target_os = "macos")]
fn local_macos_bun_startup_with_environment(
    command: &mut std::process::Command,
    environment: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) {
    local_macos_environment(command, environment);
    command
        .env("BUN_RUNTIME_TRANSPILER_CACHE_PATH", "0")
        .arg("--config=/dev/null")
        .arg("--tsconfig-override=/dev/null")
        .arg("--no-env-file")
        .arg("--no-install");
}

#[cfg(target_os = "macos")]
fn local_macos_environment(
    command: &mut std::process::Command,
    environment: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) {
    command.env_clear();
    for (key, value) in environment {
        if matches!(
            key.to_str(),
            Some("HOME" | "USER" | "LOGNAME" | "TMPDIR" | "LANG" | "LC_ALL" | "LC_CTYPE" | "TZ")
        ) {
            command.env(key, value);
        }
    }
    command.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
}

#[cfg(all(test, target_os = "macos"))]
mod local_macos_tests {
    use std::collections::BTreeMap;
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    use super::local_macos_bun_startup_with_environment;

    #[test]
    fn local_startup_has_one_bounded_environment_and_explicit_runtime_controls() {
        let mut command = Command::new("/verified/bundle/Contents/Helpers/bun");
        command.env("BUN_OPTIONS", "--preload=/not-approved.js");
        command.env("KELD_APP_LINK", "untrusted");
        let inherited = [
            ("HOME", "/Users/local"),
            ("LANG", "en_US.UTF-8"),
            ("PATH", "/developer/bin"),
            ("BUN_OPTIONS", "--preload=/not-approved.js"),
            ("NODE_OPTIONS", "--require=/not-approved.js"),
            ("NODE_PATH", "/not-approved"),
            ("DYLD_INSERT_LIBRARIES", "/not-approved.dylib"),
            ("BUN_RUNTIME_TRANSPILER_CACHE_PATH", "/not-approved"),
            ("KELD_APP_LINK", "untrusted"),
        ];
        local_macos_bun_startup_with_environment(
            &mut command,
            inherited
                .into_iter()
                .map(|(key, value)| (OsString::from(key), OsString::from(value))),
        );
        let actual: BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            actual,
            BTreeMap::from([
                (OsStr::new("HOME"), Some(OsStr::new("/Users/local"))),
                (OsStr::new("LANG"), Some(OsStr::new("en_US.UTF-8"))),
                (
                    OsStr::new("PATH"),
                    Some(OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin")),
                ),
                (
                    OsStr::new("BUN_RUNTIME_TRANSPILER_CACHE_PATH"),
                    Some(OsStr::new("0")),
                ),
            ])
        );
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "--config=/dev/null",
                "--tsconfig-override=/dev/null",
                "--no-env-file",
                "--no-install",
            ]
            .map(OsStr::new)
        );
        assert_eq!(
            command.get_program(),
            "/verified/bundle/Contents/Helpers/bun",
            "startup sanitation must not replace Core's selected executable"
        );
    }

    #[test]
    fn actual_bun_local_startup_rejects_ambient_preloads_and_runtime_config() {
        // This test-only tool lookup supplies the runtime under test. Installed
        // production launch never discovers Bun through PATH or BUN_BIN.
        let bun = std::env::var_os("BUN_BIN")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("PATH").and_then(|paths| {
                    std::env::split_paths(&paths)
                        .map(|directory| directory.join("bun"))
                        .find(|candidate| candidate.is_file())
                })
            })
            .expect("real Bun required for macOS startup controls")
            .canonicalize()
            .expect("canonical test Bun");
        let temporary = tempfile::tempdir().expect("isolated startup fixture");
        let root = temporary.path().canonicalize().expect("canonical fixture");
        let home = root.join("home");
        let project = root.join("project");
        fs::create_dir(&home).expect("fixture HOME");
        fs::create_dir(&project).expect("fixture project");
        let preload = root.join("unapproved.mjs");
        fs::write(&preload, "console.log('UNAPPROVED_PRELOAD');\n").expect("preload");
        let config = format!(
            "preload = [{}]\n",
            serde_json::to_string(&preload).expect("quoted path")
        );
        fs::write(project.join("bunfig.toml"), &config).expect("project configuration");
        fs::write(home.join(".bunfig.toml"), &config).expect("global fixture configuration");
        let entry = project.join("entry.mjs");
        fs::write(
            &entry,
            "console.log('ENTRY:' + (process.env.KELD_APP_LINK ?? 'none'));\n",
        )
        .expect("entry");

        let mut uncontrolled = Command::new(&bun);
        uncontrolled
            .env_clear()
            .env("HOME", &home)
            .arg("run")
            .arg(&entry)
            .current_dir(&project);
        let negative = uncontrolled.output().expect("uncontrolled Bun control");
        assert!(negative.status.success(), "{negative:?}");
        assert!(String::from_utf8_lossy(&negative.stdout).contains("UNAPPROVED_PRELOAD"));

        let mut controlled = Command::new(&bun);
        let mut options = OsString::from("--preload=");
        options.push(&preload);
        controlled.env("BUN_OPTIONS", &options);
        local_macos_bun_startup_with_environment(
            &mut controlled,
            [
                (OsString::from("HOME"), home.into_os_string()),
                (OsString::from("BUN_OPTIONS"), options),
                (OsString::from("KELD_APP_LINK"), OsString::from("forged")),
            ],
        );
        controlled
            .env("KELD_APP_LINK", "authenticated-fixture")
            .arg("run")
            .arg(&entry)
            .current_dir(project);
        let positive = controlled.output().expect("controlled Bun launch");
        assert!(positive.status.success(), "{positive:?}");
        assert_eq!(positive.stdout, b"ENTRY:authenticated-fixture\n");
        assert!(positive.stderr.is_empty(), "{positive:?}");
    }

    #[test]
    fn actual_local_startup_keeps_preload_and_entry_outside_app_tsconfig_remapping() {
        let bun = std::env::var_os("BUN_BIN")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("PATH").and_then(|paths| {
                    std::env::split_paths(&paths)
                        .map(|directory| directory.join("bun"))
                        .find(|candidate| candidate.is_file())
                })
            })
            .expect("real Bun required for resolver controls")
            .canonicalize()
            .expect("canonical Bun input");
        let temporary = tempfile::tempdir().expect("isolated resolution fixture");
        let root = temporary.path().canonicalize().expect("canonical fixture");
        let preload = root.join("fixed-bootstrap.mjs");
        let entry = root.join("actual-entry.mjs");
        fs::write(&preload, "globalThis.bootstrapRan = true;\n").expect("fixed bootstrap");
        fs::write(
            &entry,
            "console.log('ENTRY:' + globalThis.bootstrapRan + ':' + import.meta.main);\n",
        )
        .expect("real entry");
        fs::write(root.join("redirect.mjs"), "console.log('WRONG_MODULE');\n")
            .expect("unselected alias target");
        for selector in [
            "*".to_owned(),
            preload.to_string_lossy().into_owned(),
            entry.to_string_lossy().into_owned(),
        ] {
            let configuration = serde_json::json!({
                "compilerOptions": {"baseUrl": ".", "paths": {selector.as_str(): ["./redirect.mjs"]}}
            });
            fs::write(root.join("tsconfig.json"), configuration.to_string())
                .expect("application resolver override");
            let mut command = Command::new(&bun);
            local_macos_bun_startup_with_environment(&mut command, []);
            let output = command
                .arg(format!("--preload={}", preload.display()))
                .arg("run")
                .arg(&entry)
                .current_dir(&root)
                .output()
                .expect("real selected module launch");
            assert!(output.status.success(), "selector={selector}: {output:?}");
            assert_eq!(output.stdout, b"ENTRY:true:true\n", "selector={selector}");
            assert_eq!(output.stderr, b"", "selector={selector}");
        }
    }
}
