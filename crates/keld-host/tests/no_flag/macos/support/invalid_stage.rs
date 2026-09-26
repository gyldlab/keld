use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

#[derive(Clone, Copy)]
pub(crate) enum InvalidBoot {
    MissingBoot,
    UnreadableBoot,
    DirectoryBoot,
    SymlinkBoot,
    Malformed,
    Duplicate,
    Unknown,
    Version,
    NonUtf8,
    Oversize,
    EmptyName,
    UnsafePath,
    BadDigest,
    WrongPermissionsFile,
    MissingEntry,
    DirectoryEntry,
    SymlinkEntry,
    UnreadableEntry,
    MissingRenderer,
    DirectoryRenderer,
    SymlinkRenderer,
    UnreadableRenderer,
    MissingPermissions,
    DirectoryPermissions,
    SymlinkPermissions,
    UnreadablePermissions,
    WrongRootMode,
}

impl InvalidBoot {
    pub(crate) const ALL: [Self; 27] = [
        Self::MissingBoot,
        Self::UnreadableBoot,
        Self::DirectoryBoot,
        Self::SymlinkBoot,
        Self::Malformed,
        Self::Duplicate,
        Self::Unknown,
        Self::Version,
        Self::NonUtf8,
        Self::Oversize,
        Self::EmptyName,
        Self::UnsafePath,
        Self::BadDigest,
        Self::WrongPermissionsFile,
        Self::MissingEntry,
        Self::DirectoryEntry,
        Self::SymlinkEntry,
        Self::UnreadableEntry,
        Self::MissingRenderer,
        Self::DirectoryRenderer,
        Self::SymlinkRenderer,
        Self::UnreadableRenderer,
        Self::MissingPermissions,
        Self::DirectoryPermissions,
        Self::SymlinkPermissions,
        Self::UnreadablePermissions,
        Self::WrongRootMode,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::MissingBoot => "missing-boot",
            Self::UnreadableBoot => "unreadable-boot",
            Self::DirectoryBoot => "directory-boot",
            Self::SymlinkBoot => "symlink-boot",
            Self::Malformed => "malformed-json",
            Self::Duplicate => "duplicate-field",
            Self::Unknown => "unknown-field",
            Self::Version => "unknown-version",
            Self::NonUtf8 => "non-utf8",
            Self::Oversize => "oversize",
            Self::EmptyName => "empty-name",
            Self::UnsafePath => "unsafe-path",
            Self::BadDigest => "bad-digest",
            Self::WrongPermissionsFile => "wrong-permissions-file",
            Self::MissingEntry => "missing-entry",
            Self::DirectoryEntry => "directory-entry",
            Self::SymlinkEntry => "symlink-entry",
            Self::UnreadableEntry => "unreadable-entry",
            Self::MissingRenderer => "missing-renderer",
            Self::DirectoryRenderer => "directory-renderer",
            Self::SymlinkRenderer => "symlink-renderer",
            Self::UnreadableRenderer => "unreadable-renderer",
            Self::MissingPermissions => "missing-permissions",
            Self::DirectoryPermissions => "directory-permissions",
            Self::SymlinkPermissions => "symlink-permissions",
            Self::UnreadablePermissions => "unreadable-permissions",
            Self::WrongRootMode => "wrong-root-mode",
        }
    }

    pub(crate) const fn expected_code(self) -> &'static str {
        match self {
            Self::Malformed
            | Self::Duplicate
            | Self::Unknown
            | Self::Version
            | Self::NonUtf8
            | Self::Oversize
            | Self::EmptyName
            | Self::BadDigest
            | Self::WrongPermissionsFile => "KELD-CORE-035",
            Self::MissingBoot
            | Self::UnreadableBoot
            | Self::DirectoryBoot
            | Self::SymlinkBoot
            | Self::UnsafePath
            | Self::MissingEntry
            | Self::DirectoryEntry
            | Self::SymlinkEntry
            | Self::UnreadableEntry
            | Self::MissingRenderer
            | Self::DirectoryRenderer
            | Self::SymlinkRenderer
            | Self::UnreadableRenderer
            | Self::MissingPermissions
            | Self::DirectoryPermissions
            | Self::SymlinkPermissions
            | Self::UnreadablePermissions
            | Self::WrongRootMode => "KELD-CORE-036",
        }
    }

    pub(crate) fn apply(self, root: &Path, fixture_root: &Path) {
        let boot = root.join("keld.boot.json");
        let entry = root.join("src/main.ts");
        let renderer = root.join("index.html");
        let permissions = root.join("keld.permissions.jsonc");
        match self {
            Self::MissingBoot => fs::remove_file(boot).expect("remove boot"),
            Self::UnreadableBoot => unreadable(&boot),
            Self::DirectoryBoot => replace_with_directory(&boot),
            Self::SymlinkBoot => replace_with_symlink(&boot, fixture_root, "outside-boot"),
            Self::Malformed => replace_boot(&boot, b"{not schema v1}"),
            Self::Duplicate => replace_boot(
                &boot,
                br#"{"schema":1,"schema":1,"name":"x","entry":"src/main.ts","renderer":"index.html","permissions":{"file":"keld.permissions.jsonc","content_sha256":"sha256:ca3d163bab055381827226140568f3bef7eaac187cebd76878e0b63e9e442356"}}"#,
            ),
            Self::Unknown => mutate_boot(&boot, |document| document["unknown"] = 1.into()),
            Self::Version => mutate_boot(&boot, |document| document["schema"] = 2.into()),
            Self::NonUtf8 => replace_boot(&boot, &[0xff]),
            Self::Oversize => replace_boot(&boot, &vec![b' '; 64 * 1024 + 1]),
            Self::EmptyName => mutate_boot(&boot, |document| document["name"] = "".into()),
            Self::UnsafePath => {
                mutate_boot(&boot, |document| document["entry"] = "../escape.ts".into());
            }
            Self::BadDigest => mutate_boot(&boot, |document| {
                document["permissions"]["content_sha256"] = "SHA256:BAD".into();
            }),
            Self::WrongPermissionsFile => mutate_boot(&boot, |document| {
                document["permissions"]["file"] = "other.permissions.jsonc".into();
            }),
            Self::MissingEntry => fs::remove_file(entry).expect("remove entry"),
            Self::DirectoryEntry => replace_with_directory(&entry),
            Self::SymlinkEntry => replace_with_symlink(&entry, fixture_root, "outside-entry"),
            Self::UnreadableEntry => unreadable(&entry),
            Self::MissingRenderer => fs::remove_file(renderer).expect("remove renderer"),
            Self::DirectoryRenderer => replace_with_directory(&renderer),
            Self::SymlinkRenderer => {
                replace_with_symlink(&renderer, fixture_root, "outside-renderer");
            }
            Self::UnreadableRenderer => unreadable(&renderer),
            Self::MissingPermissions => {
                fs::remove_file(permissions).expect("remove permissions");
            }
            Self::DirectoryPermissions => replace_with_directory(&permissions),
            Self::SymlinkPermissions => {
                replace_with_symlink(&permissions, fixture_root, "outside-permissions");
            }
            Self::UnreadablePermissions => unreadable(&permissions),
            Self::WrongRootMode => {
                fs::set_permissions(root, fs::Permissions::from_mode(0o755))
                    .expect("set invalid root mode");
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum InvalidPolicy {
    Malformed,
    NonUtf8,
    DigestMismatch,
    Oversized,
    DuplicateKeys,
}

impl InvalidPolicy {
    pub(crate) const ALL: [Self; 5] = [
        Self::Malformed,
        Self::NonUtf8,
        Self::DigestMismatch,
        Self::Oversized,
        Self::DuplicateKeys,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::NonUtf8 => "non-utf8",
            Self::DigestMismatch => "digest-mismatch",
            Self::Oversized => "oversized",
            Self::DuplicateKeys => "duplicate-keys",
        }
    }

    pub(crate) const fn expected_code(self) -> &'static str {
        match self {
            Self::Malformed | Self::NonUtf8 | Self::DuplicateKeys => "KELD-GUARD005",
            Self::DigestMismatch => "KELD-GUARD016",
            Self::Oversized => "KELD-GUARD017",
        }
    }

    pub(crate) fn apply(self, root: &Path) {
        let policy = root.join("keld.permissions.jsonc");
        let boot = root.join("keld.boot.json");
        fs::set_permissions(&policy, fs::Permissions::from_mode(0o600))
            .expect("make policy writable");
        match self {
            Self::Malformed => {
                fs::write(&policy, b"{nope}\n").expect("write malformed policy");
                set_policy_digest(
                    &boot,
                    "ed4d18e4d7f58b800fafc0e89f02e9b76eca431e8a8314df677d02cee467920e",
                );
            }
            Self::NonUtf8 => {
                fs::write(&policy, [0xff]).expect("write non-UTF-8 policy");
                set_policy_digest(
                    &boot,
                    "a8100ae6aa1940d0b663bb31cd466142ebbdbd5187131b92d93818987832eb89",
                );
            }
            Self::DigestMismatch => {
                fs::write(&policy, b"{not the described bytes}\n")
                    .expect("write digest-mismatched policy");
            }
            Self::Oversized => {
                fs::write(&policy, vec![b' '; 64 * 1024 + 1]).expect("write oversized policy");
            }
            Self::DuplicateKeys => {
                fs::write(
                    &policy,
                    br#"{"app":{"fs":{"read":[],"read":["/outside/**"]}}}"#,
                )
                .expect("write duplicate-key policy");
                set_policy_digest(
                    &boot,
                    "06d74274d5d6a0351deac64c75ad477105c8316f61a0e6e62eb59e3e0f73e0d1",
                );
            }
        }
    }
}

fn set_policy_digest(boot: &Path, hex: &str) {
    mutate_boot(boot, |document| {
        document["permissions"]["content_sha256"] = format!("sha256:{hex}").into();
    });
}

fn replace_boot(path: &Path, bytes: &[u8]) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).expect("make boot writable");
    fs::write(path, bytes).expect("replace boot bytes");
}

fn mutate_boot(path: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("read boot")).expect("parse staged boot");
    mutate(&mut document);
    replace_boot(
        path,
        &serde_json::to_vec(&document).expect("serialize mutated boot"),
    );
}

fn unreadable(path: &Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o000)).expect("make target unreadable");
}

fn replace_with_directory(path: &Path) {
    fs::remove_file(path).expect("remove file before directory substitution");
    fs::create_dir(path).expect("create directory substitution");
}

fn replace_with_symlink(path: &Path, fixture_root: &Path, name: &str) {
    let outside = fixture_root.join(name);
    fs::write(&outside, b"outside substitution").expect("outside substitution target");
    fs::remove_file(path).expect("remove file before symlink substitution");
    symlink(outside, path).expect("create symlink substitution");
}
