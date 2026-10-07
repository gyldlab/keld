//! Real Windows loader proof for the updater helper's System32-only DLL search
//! (KEL-53 §7 row "17 (helper launch and self-anchor)": a DLL planted in the
//! current directory or in a user-writable `PATH` entry is not loaded after
//! `main`).
//!
//! The setting is process-wide and irreversible, so each observation runs in a
//! child copy of this test executable. The same fixture first proves that the
//! standard search loads every planted copy (the negative control), then that
//! a child which restricts its search first loads none of them.

#![cfg(windows)]
#![allow(unsafe_code)] // isolated test-only loader observation with local ABI proofs
#![allow(clippy::expect_used, clippy::panic)] // fixture invariants must abort the proof loudly
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use keld_runtime::windows_job::restrict_dll_search_to_system32;
use windows_sys::Win32::Foundation::{ERROR_MOD_NOT_FOUND, FreeLibrary};
use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, LoadLibraryW};

const CHILD_TEST: &str = "dll_search_child";
const MODE_ENV: &str = "KELD_TEST_DLL_SEARCH_MODE";
const NAMES_ENV: &str = "KELD_TEST_DLL_SEARCH_NAMES";
/// Planted beside the child image: the standard search's application directory.
const IMAGE_DLL: &str = "keld-planted-image-dir.dll";
/// Planted in the child's current directory.
const CWD_DLL: &str = "keld-planted-current-dir.dll";
/// Planted in a user-writable directory prepended to the child's `PATH`.
const PATH_DLL: &str = "keld-planted-path-entry.dll";

/// What one child load reported.
#[derive(Debug, PartialEq, Eq)]
enum Load {
    /// The module loaded from this canonical path.
    Loaded(PathBuf),
    /// `LoadLibraryW` failed with this Windows error.
    Refused(i32),
}

#[test]
fn system32_only_search_skips_dlls_planted_beside_the_image_in_the_current_directory_and_on_path() {
    let fixture = tempfile::tempdir().expect("loader fixture");
    let image_dir = make_dir(fixture.path(), "image");
    let current_dir = make_dir(fixture.path(), "current");
    let path_entry = make_dir(fixture.path(), "path-entry");
    let child_image = image_dir.join("keld-dll-search-child.exe");
    fs::copy(
        env::current_exe().expect("this test executable"),
        &child_image,
    )
    .expect("copy the test executable beside its planted DLL");

    // A real System32 DLL under a name that no search path, KnownDLLs entry or
    // loaded module already holds, so only the planted copies can satisfy it.
    let source = system32_dll("version.dll");
    let planted = [
        (IMAGE_DLL, image_dir.join(IMAGE_DLL)),
        (CWD_DLL, current_dir.join(CWD_DLL)),
        (PATH_DLL, path_entry.join(PATH_DLL)),
    ];
    for (_, path) in &planted {
        fs::copy(&source, path).expect("plant a DLL copy");
    }

    // Each location is reported on its own, so one failure shows all three.
    let loaded_from_plant: BTreeMap<String, Load> = planted
        .iter()
        .map(|(name, path)| ((*name).to_owned(), Load::Loaded(canonical(path))))
        .collect();
    assert_eq!(
        run_child(&child_image, &current_dir, &path_entry, "standard"),
        loaded_from_plant,
        "negative control: the standard search must load every planted copy, or this machine \
         cannot show the restriction's effect"
    );

    let not_found: BTreeMap<String, Load> = planted
        .iter()
        .map(|(name, _)| {
            (
                (*name).to_owned(),
                Load::Refused(ERROR_MOD_NOT_FOUND.cast_signed()),
            )
        })
        .collect();
    assert_eq!(
        run_child(&child_image, &current_dir, &path_entry, "system32"),
        not_found,
        "after restrict_dll_search_to_system32 no planted copy may load"
    );
}

#[test]
#[ignore = "private subprocess entry point"]
fn dll_search_child() {
    let Ok(mode) = env::var(MODE_ENV) else {
        return;
    };
    match mode.as_str() {
        "standard" => {}
        "system32" => {
            if let Err(error) = restrict_dll_search_to_system32() {
                println!("RESTRICT error {error}");
                std::process::exit(70);
            }
        }
        other => panic!("unknown DLL search mode {other}"),
    }
    let names = env::var(NAMES_ENV).expect("planted DLL names");
    for name in names.split(';') {
        match load_by_name(name) {
            Ok(path) => println!("LOAD {name} loaded {}", path.display()),
            Err(error) => println!(
                "LOAD {name} refused {}",
                error
                    .raw_os_error()
                    .expect("LoadLibraryW sets a Windows error")
            ),
        }
    }
}

fn run_child(
    image: &Path,
    current_dir: &Path,
    path_entry: &Path,
    mode: &str,
) -> BTreeMap<String, Load> {
    let mut path = path_entry.as_os_str().to_os_string();
    if let Some(inherited) = env::var_os("PATH") {
        path.push(";");
        path.push(inherited);
    }
    let output = Command::new(image)
        .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
        .current_dir(current_dir)
        .env("PATH", path)
        .env(MODE_ENV, mode)
        .env(NAMES_ENV, [IMAGE_DLL, CWD_DLL, PATH_DLL].join(";"))
        .stdin(Stdio::null())
        .output()
        .expect("run the loader child");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{mode} child failed: {}\nstdout:\n{stdout}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let mut loads = BTreeMap::new();
    for line in stdout.lines() {
        let Some(record) = line.strip_prefix("LOAD ") else {
            continue;
        };
        let mut fields = record.splitn(3, ' ');
        let (Some(name), Some(kind), Some(value)) = (fields.next(), fields.next(), fields.next())
        else {
            panic!("malformed child record {line:?}");
        };
        let load = match kind {
            "loaded" => Load::Loaded(canonical(Path::new(value))),
            "refused" => Load::Refused(value.parse().expect("Windows error code")),
            _ => panic!("malformed child record {line:?}"),
        };
        assert!(
            loads.insert(name.to_owned(), load).is_none(),
            "duplicate record for {name}"
        );
    }
    assert_eq!(
        loads.len(),
        3,
        "{mode} child must report every name:\n{stdout}"
    );
    loads
}

/// Loads `name` by module name only, so the process search path decides where
/// it comes from, and returns the loaded module's path.
fn load_by_name(name: &str) -> io::Result<PathBuf> {
    let wide: Vec<u16> = OsStr::new(name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `wide` is a live NUL-terminated UTF-16 buffer for the call.
    let module = unsafe { LoadLibraryW(wide.as_ptr()) };
    if module.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0_u16; 32_768];
    let capacity = u32::try_from(buffer.len()).expect("buffer length fits u32");
    // SAFETY: `module` is the live module just loaded and `buffer` is writable
    // for `capacity` UTF-16 units.
    let written = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), capacity) };
    let result = if written == 0 || written == capacity {
        Err(io::Error::last_os_error())
    } else {
        let length = usize::try_from(written).expect("u32 fits usize");
        Ok(PathBuf::from(std::ffi::OsString::from_wide(
            &buffer[..length],
        )))
    };
    // SAFETY: `module` came from the successful LoadLibraryW above and is
    // released exactly once.
    assert_ne!(
        unsafe { FreeLibrary(module) },
        0,
        "release the loaded module"
    );
    result
}

fn make_dir(parent: &Path, name: &str) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir(&dir).expect("fixture directory");
    dir
}

fn system32_dll(name: &str) -> PathBuf {
    let root = env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
    let path = Path::new(&root).join("System32").join(name);
    assert!(path.is_file(), "{} must exist", path.display());
    path
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path)
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", path.display()))
}
