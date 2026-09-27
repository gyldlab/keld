//! Read-only provenance loader and separate initializer-only seed verification.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};

use cap_fs_ext::{DirExt as _, MetadataExt as _};
use cap_std::fs::{Dir, File};

use super::{
    LoadedWindowsBaseline, Roots, VersionPins, WindowsBaselineTrust, ensure_directory, error,
    exact_entries, open_machine_file, open_roots, read_record,
};
use crate::records::{self, PointerKind};
use crate::{ArchiveEntryKind, InstallOwner, ProvenanceObservation, UpdateError};

/// Loads coherent protected initial-baseline metadata without a writable handle.
///
/// Expected identity, publisher and volume must be trusted host configuration. The
/// returned owner retains namespace and record pins; it provides no repair, mutation,
/// active-package selection, current-image signer proof or strict-role authority.
///
/// # Errors
/// Refuses unknown/missing/unprotected records, unsupported ancestry, identity or
/// publisher/volume mismatches, non-baseline pointers/floor/completion, or state
/// requiring later activation/recovery. Private incomplete stages are diagnostic only.
pub fn load_windows_baseline(
    trust: &WindowsBaselineTrust,
) -> Result<LoadedWindowsBaseline, UpdateError> {
    let roots =
        open_roots(trust, false).map_err(|cause| error("committed scaffold admission", cause))?;
    let install = roots
        .install()
        .map_err(|cause| error("install root", cause))?;
    let (provenance_file, provenance_bytes) = read_record(install, "install-provenance")?;
    let record = records::decode_provenance(&provenance_bytes)?;
    crate::provenance::match_identity(&trust.installation, &record.provenance.identity)?;
    if record.publisher_scope != trust.publisher_scope || record.volume_guid != trust.volume_guid {
        return Err(error(
            "provenance trust binding",
            "publisher or expected volume differs",
        ));
    }
    if let InstallOwner::Managed { mechanism } = &record.provenance.owner {
        return Err(UpdateError::ManagedInstall {
            mechanism: mechanism.clone(),
        });
    }
    let metadata = read_initial_metadata(&roots)?;
    let floor = metadata.floor;
    let mut retained = metadata.records;
    retained.push(provenance_file);
    retained.push(metadata.completion.marker);
    retained.push(metadata.completion.content);
    Ok(LoadedWindowsBaseline {
        observation: ProvenanceObservation::Protected {
            record: record.provenance,
            version_floor: Some(floor.clone()),
        },
        floor,
        publisher_scope: record.publisher_scope,
        roots,
        _records: retained,
        _baseline_version: metadata.completion.version,
        _baseline_tree: metadata.completion.tree,
    })
}

struct BaselineCompletion {
    version: Dir,
    tree: Dir,
    content: File,
    marker: File,
    record: records::CompleteRecord,
}

struct InitialMetadata {
    floor: String,
    records: Vec<File>,
    completion: BaselineCompletion,
    lock_present: bool,
}

fn read_initial_metadata(roots: &Roots) -> Result<InitialMetadata, UpdateError> {
    let baseline = &roots.trust.installation.baseline;
    let update_name = roots
        .trust
        .installation
        .update_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| error("seed topology", "update component absent"))?;
    exact_entries(
        roots
            .install()
            .map_err(|cause| error("install root", cause))?,
        &[update_name, "install-provenance"],
    )
    .map_err(|cause| error("initial install contents", cause))?;
    let mut lock_present = false;
    for entry in roots
        .update
        .entries()
        .map_err(|cause| error("initial update contents", cause))?
    {
        let name = entry
            .map_err(|cause| error("initial update contents", cause))?
            .file_name();
        match name.to_str() {
            Some("versions" | "version-floor" | "current" | "last-known-good") => {}
            Some("bootstrap.lock") => lock_present = true,
            _ => {
                return Err(error(
                    "initial update contents",
                    "unknown state requires activation/recovery admission",
                ));
            }
        }
    }
    validate_version_census(roots)?;
    let mut retained = Vec::new();
    if lock_present {
        let (lock, bytes) = read_record(&roots.update, "bootstrap.lock")?;
        if !bytes.is_empty() {
            return Err(error("initial lock", "bootstrap marker is not empty"));
        }
        retained.push(lock);
    }
    let (floor_file, bytes) = read_record(&roots.update, "version-floor")?;
    let floor = records::decode_floor(&bytes)?;
    if floor != baseline.version {
        return Err(error("initial floor", "floor differs from exact baseline"));
    }
    retained.push(floor_file);
    for (name, kind) in [
        ("current", PointerKind::Current),
        ("last-known-good", PointerKind::LastKnownGood),
    ] {
        let (file, bytes) = read_record(&roots.update, name)?;
        if records::decode_pointer(kind, &bytes)? != *baseline {
            return Err(error(
                "initial pointer",
                "pointer differs from exact baseline",
            ));
        }
        retained.push(file);
    }
    Ok(InitialMetadata {
        floor,
        records: retained,
        completion: read_baseline_completion(roots)?,
        lock_present,
    })
}

fn validate_version_census(roots: &Roots) -> Result<(), UpdateError> {
    for entry in roots
        .versions
        .entries()
        .map_err(|cause| error("initial versions", cause))?
    {
        let entry = entry.map_err(|cause| error("initial versions", cause))?;
        let name = entry.file_name();
        if name.to_str() == Some(roots.trust.installation.baseline.version.as_str()) {
            continue;
        }
        let diagnostic = name
            .to_str()
            .and_then(|name| name.strip_prefix("incomplete-"))
            .is_some_and(|suffix| {
                suffix.len() == 64
                    && suffix
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            });
        if !diagnostic {
            return Err(error(
                "initial versions",
                "only the exact baseline and named incomplete diagnostics are admitted",
            ));
        }
        // Windows DirEntry metadata comes from parent enumeration; do not open a
        // SYSTEM-private stage or inspect its contents merely to admit its name.
        let metadata = entry
            .metadata()
            .map_err(|cause| error("diagnostic directory metadata", cause))?;
        ensure_directory(&metadata).map_err(|cause| error("diagnostic directory kind", cause))?;
    }
    Ok(())
}

pub(super) fn validate_initial_seed(roots: &Roots) -> Result<VersionPins, UpdateError> {
    let metadata = read_initial_metadata(roots)?;
    if !metadata.lock_present {
        return Err(error(
            "initial lock",
            "initializer bootstrap marker is missing",
        ));
    }
    // The initializer's postcommit proof is stricter than later read-only metadata
    // admission: its fresh transaction has not produced diagnostic sibling stages.
    exact_entries(
        &roots.versions,
        &[&roots.trust.installation.baseline.version],
    )
    .map_err(|cause| error("initial versions", cause))?;
    let mut version = validate_version_contents(roots, metadata.completion)?;
    version.files.extend(metadata.records);
    Ok(version)
}

pub(super) fn validate_initial_version(roots: &Roots) -> Result<VersionPins, UpdateError> {
    validate_version_contents(roots, read_baseline_completion(roots)?)
}

fn read_baseline_completion(roots: &Roots) -> Result<BaselineCompletion, UpdateError> {
    let baseline = &roots.trust.installation.baseline;
    let version = open_directory(&roots.versions, &baseline.version)?;
    exact_entries(&version, &["content.tar", "tree", ".complete"])
        .map_err(|cause| error("version contents", cause))?;
    let (complete, bytes) = read_record(&version, ".complete")?;
    let complete_record = records::decode_complete(&bytes)?;
    if complete_record.artifact != *baseline {
        return Err(error(
            "completion marker",
            "artifact differs from exact baseline",
        ));
    }
    let content = open_machine_file(&version, "content.tar")
        .map_err(|cause| error("retained archive", cause))?;
    if content
        .metadata()
        .map_err(|cause| error("archive metadata", cause))?
        .len()
        != complete_record.content_size
    {
        return Err(error(
            "completion size",
            "marker size differs from retained archive length",
        ));
    }
    let tree = open_directory(&version, "tree")?;
    Ok(BaselineCompletion {
        version,
        tree,
        content,
        marker: complete,
        record: complete_record,
    })
}

fn validate_version_contents(
    roots: &Roots,
    completion: BaselineCompletion,
) -> Result<VersionPins, UpdateError> {
    let baseline = &roots.trust.installation.baseline;
    let BaselineCompletion {
        version,
        tree,
        mut content,
        marker: complete,
        record: complete_record,
    } = completion;
    let expected = crate::full::ContentIdentity {
        identity: baseline.clone(),
        content_size: complete_record.content_size,
        content_blake3: baseline.content_blake3,
    };
    let validated = crate::archive::parse_content_archive(
        &expected,
        &mut content,
        keld_guard::validate_windows_package_paths,
    )?;
    let mut directories = vec![version, tree];
    let mut files = vec![complete];
    let mut parents = BTreeMap::from([(String::new(), 1_usize)]);
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::from([(String::new(), Vec::new())]);
    for entry in validated.entries() {
        let (parent_name, leaf) = entry.name().rsplit_once('/').unwrap_or(("", entry.name()));
        let index = *parents
            .get(parent_name)
            .ok_or_else(|| error("tree order", "validated parent absent"))?;
        children
            .entry(parent_name.to_owned())
            .or_default()
            .push(leaf.to_owned());
        let parent = &directories[index];
        match entry.kind() {
            ArchiveEntryKind::Directory => {
                let directory = open_directory(parent, leaf)?;
                parents.insert(entry.name().to_owned(), directories.len());
                children.entry(entry.name().to_owned()).or_default();
                directories.push(directory);
            }
            ArchiveEntryKind::File => {
                let mut file =
                    open_machine_file(parent, leaf).map_err(|cause| error("tree file", cause))?;
                if file
                    .metadata()
                    .map_err(|cause| error("tree metadata", cause))?
                    .len()
                    != entry.size()
                {
                    return Err(error(
                        "tree bytes",
                        "member length differs from authenticated archive",
                    ));
                }
                content
                    .seek(SeekFrom::Start(entry.data_offset()))
                    .map_err(|cause| error("archive member seek", cause))?;
                let mut remaining = entry.size();
                let mut actual = [0_u8; 16 * 1024];
                let mut wanted = [0_u8; 16 * 1024];
                while remaining != 0 {
                    let size = usize::try_from(remaining.min(actual.len() as u64))
                        .map_err(|cause| error("member size", cause))?;
                    file.read_exact(&mut actual[..size])
                        .map_err(|cause| error("tree readback", cause))?;
                    content
                        .read_exact(&mut wanted[..size])
                        .map_err(|cause| error("archive readback", cause))?;
                    if actual[..size] != wanted[..size] {
                        return Err(error(
                            "tree bytes",
                            "member differs from authenticated archive",
                        ));
                    }
                    remaining -= size as u64;
                }
                files.push(file);
            }
        }
    }
    for (name, index) in parents {
        let wanted = children
            .get(&name)
            .ok_or_else(|| error("tree census", "directory census absent"))?;
        let wanted: Vec<&str> = wanted.iter().map(String::as_str).collect();
        exact_entries(&directories[index], &wanted).map_err(|cause| error("tree census", cause))?;
    }
    files.push(content);
    Ok(VersionPins {
        _directories: directories,
        files,
    })
}

fn open_directory(parent: &Dir, leaf: &str) -> Result<Dir, UpdateError> {
    let directory = parent
        .open_dir_nofollow(leaf)
        .map_err(|cause| error("protected directory open", cause))?;
    ensure_directory(
        &directory
            .dir_metadata()
            .map_err(|cause| error("directory metadata", cause))?,
    )
    .map_err(|cause| error("directory kind", cause))?;
    if directory
        .dir_metadata()
        .map_err(|cause| error("directory volume", cause))?
        .dev()
        != parent
            .dir_metadata()
            .map_err(|cause| error("parent volume", cause))?
            .dev()
    {
        return Err(error("directory volume", "directory crosses parent volume"));
    }
    keld_guard::validate_windows_machine_directory(
        &directory
            .try_clone()
            .map_err(|cause| error("directory handle", cause))?
            .into_std_file(),
    )
    .map_err(|cause| error("directory protection", cause))?;
    Ok(directory)
}
