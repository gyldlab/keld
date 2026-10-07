//! The release helper's pre-`main` loader hardening (KEL-53 §4 "Helper launch and
//! self-anchor"; §7 "17 (helper launch and self-anchor)": the import-table and
//! planted-DLL cells).
//!
//! Ignored by default. The Windows CI step builds `cargo build --release -p
//! keld-updater-helper` and runs these tests with `KELD_UPDATER_HELPER_RELEASE` naming
//! that executable; each fails, never skips, when the variable is absent. The image is
//! read by the PE format, independently of the build script that produced it.
#![cfg(windows)]
#![allow(clippy::expect_used, clippy::panic)] // extra test crate: expect/panic are assertion oracles
#![allow(clippy::disallowed_methods)] // test-only: Command::output runs a copy of the release helper, the product under test

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

/// The only DLLs the release helper may import: System32 modules and one core API set,
/// which the loader resolves by its schema rather than by a search.
const IMPORT_ALLOWLIST: [&str; 6] = [
    "advapi32.dll",
    "api-ms-win-core-synch-l1-2-0.dll",
    "crypt32.dll",
    "kernel32.dll",
    "ntdll.dll",
    "wintrust.dll",
];

/// C runtime DLL names a dynamically linked image would import. The helper imports none,
/// so a planted copy of any of them must never load.
const RUNTIME_DLLS: [&str; 8] = [
    "vcruntime140.dll",
    "vcruntime140_1.dll",
    "msvcp140.dll",
    "ucrtbase.dll",
    "api-ms-win-crt-runtime-l1-1-0.dll",
    "api-ms-win-crt-heap-l1-1-0.dll",
    "api-ms-win-crt-stdio-l1-1-0.dll",
    "api-ms-win-crt-string-l1-1-0.dll",
];

/// `LOAD_LIBRARY_SEARCH_SYSTEM32`, which `/DEPENDENTLOADFLAG:0x800` records.
const LOAD_LIBRARY_SEARCH_SYSTEM32: u16 = 0x0800;
/// `IMAGE_SUBSYSTEM_WINDOWS_GUI`.
const WINDOWS_GUI: u16 = 2;

fn release_helper() -> PathBuf {
    std::env::var_os("KELD_UPDATER_HELPER_RELEASE").map_or_else(
        || {
            panic!(
                "KELD_UPDATER_HELPER_RELEASE must name the release keld-updater-helper.exe; \
                 this test never skips"
            )
        },
        PathBuf::from,
    )
}

/// The facts the loader reads from a PE32+ image before `main`.
struct LoaderFacts {
    imports: Vec<String>,
    delay_import_size: u32,
    dependent_load_flags: u16,
    subsystem: u16,
}

fn u16_at(image: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(image[at..at + 2].try_into().expect("two bytes"))
}

fn u32_at(image: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(image[at..at + 4].try_into().expect("four bytes"))
}

fn usize_at(image: &[u8], at: usize) -> usize {
    usize::try_from(u32_at(image, at)).expect("u32 fits usize")
}

/// Reads the import directory, the delay-import directory size, the load configuration's
/// `DependentLoadFlags` and the subsystem, by the PE format's offsets.
fn loader_facts(image: &[u8]) -> LoaderFacts {
    let nt = usize_at(image, 0x3C);
    assert_eq!(&image[nt..nt + 4], b"PE\0\0", "PE signature");
    assert_eq!(u16_at(image, nt + 4), 0x8664, "x64 machine");
    let sections = usize::from(u16_at(image, nt + 6));
    let optional = nt + 24;
    let optional_size = usize::from(u16_at(image, nt + 20));
    assert_eq!(u16_at(image, optional), 0x020B, "PE32+ optional header");
    let subsystem = u16_at(image, optional + 68);
    let directory = |index: usize| {
        let at = optional + 112 + index * 8;
        (usize_at(image, at), u32_at(image, at + 4))
    };
    let table = optional + optional_size;
    let offset = |rva: usize| {
        (0..sections)
            .map(|section| table + section * 40)
            .find_map(|header| {
                let virtual_address = usize_at(image, header + 12);
                let span = usize_at(image, header + 8).max(usize_at(image, header + 16));
                (virtual_address..virtual_address + span)
                    .contains(&rva)
                    .then(|| rva - virtual_address + usize_at(image, header + 20))
            })
            .unwrap_or_else(|| panic!("RVA {rva:#x} lies in no section"))
    };
    let c_string = |at: usize| {
        let end = image[at..]
            .iter()
            .position(|byte| *byte == 0)
            .expect("NUL-terminated name");
        std::str::from_utf8(&image[at..at + end])
            .expect("ASCII DLL name")
            .to_ascii_lowercase()
    };

    let (import_rva, _) = directory(1);
    let mut imports = Vec::new();
    let mut descriptor = offset(import_rva);
    while image[descriptor..descriptor + 20]
        .iter()
        .any(|byte| *byte != 0)
    {
        imports.push(c_string(offset(usize_at(image, descriptor + 12))));
        descriptor += 20;
    }

    let (load_config_rva, _) = directory(10);
    let load_config = offset(load_config_rva);
    assert!(
        usize_at(image, load_config) >= 0x50,
        "the load configuration reaches DependentLoadFlags"
    );
    let (_, delay_import_size) = directory(13);
    LoaderFacts {
        imports,
        delay_import_size,
        dependent_load_flags: u16_at(image, load_config + 0x4E),
        subsystem,
    }
}

#[test]
#[ignore = "needs KELD_UPDATER_HELPER_RELEASE naming a workspace-built release keld-updater-helper.exe"]
fn the_release_helper_imports_only_its_allowlist_and_resolves_them_from_system32() {
    let image = std::fs::read(release_helper()).expect("read the release helper");
    let facts = loader_facts(&image);
    println!(
        "KELD_UPDATER_HELPER_IMPORTS {} dependent_load_flags={:#06x} delay_import_size={} subsystem={}",
        facts.imports.join(","),
        facts.dependent_load_flags,
        facts.delay_import_size,
        facts.subsystem
    );
    assert!(
        facts.imports.iter().any(|name| name == "kernel32.dll"),
        "the import directory was read: {:?}",
        facts.imports
    );
    let outside: Vec<&String> = facts
        .imports
        .iter()
        .filter(|name| !IMPORT_ALLOWLIST.contains(&name.as_str()))
        .collect();
    assert!(
        outside.is_empty(),
        "imports outside the allowlist (a C runtime DLL means the static runtime is gone): {outside:?}"
    );
    assert_eq!(facts.delay_import_size, 0, "no delay-loaded imports");
    assert_eq!(
        facts.dependent_load_flags, LOAD_LIBRARY_SEARCH_SYSTEM32,
        "/DEPENDENTLOADFLAG:0x800"
    );
    assert_eq!(facts.subsystem, WINDOWS_GUI, "the windows subsystem");
}

/// A copy of the release helper in a user-writable directory, beside a planted DLL for
/// every name it imports and every C runtime name, still reaches its own typed
/// self-anchor refusal: the unsigned image fails its KEL-135 verification
/// (`KELD-HELPER-002`, `TRUST_E_NOSIGNATURE`) instead of a loader status such as
/// `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139). Each plant is a copy of a System32 DLL
/// that exports none of the functions the helper imports.
#[test]
#[ignore = "needs KELD_UPDATER_HELPER_RELEASE naming a workspace-built release keld-updater-helper.exe"]
fn planted_dlls_beside_a_release_helper_copy_are_not_loaded() {
    let release = release_helper();
    let image = std::fs::read(&release).expect("read the release helper");
    let system32 =
        PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set")).join("System32");
    let plant = system32.join("version.dll");
    let directory = tempfile::tempdir().expect("a user-writable directory");
    let copy = directory.path().join("keld-updater-helper.exe");
    std::fs::copy(&release, &copy).expect("copy the release helper");
    let names: BTreeSet<String> = loader_facts(&image)
        .imports
        .into_iter()
        .chain(RUNTIME_DLLS.iter().map(|name| (*name).to_owned()))
        .collect();
    for name in &names {
        std::fs::copy(&plant, directory.path().join(name)).expect("plant a DLL");
    }

    let rendezvous =
        r"\\.\pipe\keld-attempt-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let output = Command::new(&copy)
        .arg(rendezvous)
        .current_dir(directory.path())
        .output()
        .expect("start the planted copy");
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!(
        "KELD_UPDATER_HELPER_PLANTED names={} status={:?} stderr={stderr}",
        names.len(),
        output.status.code().map(|code| format!("{code:#x}"))
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "the helper must start and refuse, not fail to load: {stderr}"
    );
    assert!(
        stderr.starts_with(
            "KELD-HELPER-002: keld-updater-helper.exe could not verify its own image \
             (WinVerifyTrust rejected the current executable with status 0x800b0100)."
        ),
        "{stderr}"
    );
}
