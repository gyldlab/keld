//! Explicit local initial-install continuation. Inputs are not trust assertions.
//! Only the exact retained signed Host may authenticate them and install.
use super::{BuildError, local_archive::LocalArchiveCandidate, local_source, windows_inputs};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
const MAX_REQUEST: usize = 64 * 1024;
fn refuse(e: impl std::fmt::Display) -> BuildError {
    BuildError::input("local initial install", e)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inputs {
    provenance: PathBuf,
    catalog: PathBuf,
    catalog_signature: PathBuf,
    manifest: PathBuf,
    manifest_signature: PathBuf,
    canonical_content: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionObservedInputs {
    install_root: PathBuf,
    update_root: PathBuf,
    version: String,
    catalog: PathBuf,
    catalog_signature: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedInputs {
    install_root: PathBuf,
    update_root: PathBuf,
    version: String,
    catalog: PathBuf,
    catalog_signature: PathBuf,
    manifest: PathBuf,
    manifest_signature: PathBuf,
}
#[derive(Serialize)]
struct ObservedRequest<'a> {
    schema: &'static str,
    install_root: &'a Path,
    update_root: &'a Path,
    version: &'a str,
    scaffold_mode: ScaffoldChoice,
    catalog: &'a Path,
    catalog_signature: &'a Path,
    manifest: &'a Path,
    manifest_signature: &'a Path,
    compressed_archive: &'a Path,
    release_public_key: [u8; 32],
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ScaffoldChoice {
    Existing,
    CreateNew,
}
enum InvocationKind {
    Provided,
    Observed(ScaffoldChoice),
}
enum InputMode {
    Provided(Inputs),
    Observed(ObservedInputs, ScaffoldChoice),
}
impl InputMode {
    fn files(&self, content_size: u64) -> Vec<(&Path, u64)> {
        match self {
            Self::Provided(i) => vec![
                (&i.catalog, 65536),
                (&i.catalog_signature, 64),
                (&i.manifest, 16 * 1024 * 1024),
                (&i.manifest_signature, 256),
                (&i.canonical_content, content_size),
                (&i.provenance, 65536),
            ],
            Self::Observed(i, _) => vec![
                (&i.catalog, 65536),
                (&i.catalog_signature, 64),
                (&i.manifest, 16 * 1024 * 1024),
                (&i.manifest_signature, 256),
            ],
        }
    }
    fn request_bytes(&self, candidate: &Path, key: [u8; 32]) -> Result<Vec<u8>, BuildError> {
        match self {
            Self::Provided(i) => request_bytes(i, candidate, key),
            Self::Observed(i, mode) => {
                if !candidate.is_absolute() {
                    return Err(refuse("candidate locator must be absolute"));
                }
                let request = ObservedRequest {
                    schema: "keld.windows-initial-install-request/v2",
                    install_root: &i.install_root,
                    update_root: &i.update_root,
                    version: &i.version,
                    scaffold_mode: *mode,
                    catalog: &i.catalog,
                    catalog_signature: &i.catalog_signature,
                    manifest: &i.manifest,
                    manifest_signature: &i.manifest_signature,
                    compressed_archive: candidate,
                    release_public_key: key,
                };
                let bytes = serde_json::to_vec(&request).map_err(refuse)?;
                if bytes.len() > MAX_REQUEST {
                    return Err(refuse("serialized request exceeds bounds"));
                }
                Ok(bytes)
            }
        }
    }
}
fn parse_observed(bytes: &[u8]) -> Result<ObservedInputs, BuildError> {
    if bytes.is_empty() || bytes.len() > MAX_REQUEST {
        return Err(refuse("input document exceeds bounds"));
    }
    let i: ObservedInputs = serde_json::from_slice(bytes).map_err(refuse)?;
    for path in [
        &i.install_root,
        &i.update_root,
        &i.catalog,
        &i.catalog_signature,
        &i.manifest,
        &i.manifest_signature,
    ] {
        if !path.is_absolute() {
            return Err(refuse("every observed input locator must be absolute"));
        }
    }
    if i.version.is_empty() || i.version.len() > 128 {
        return Err(refuse("exact requested version exceeds bounds"));
    }
    Ok(i)
}
#[derive(Serialize)]
struct Request<'a> {
    schema: &'static str,
    provenance: &'a Path,
    catalog: &'a Path,
    catalog_signature: &'a Path,
    manifest: &'a Path,
    manifest_signature: &'a Path,
    canonical_content: &'a Path,
    compressed_archive: &'a Path,
    release_public_key: [u8; 32],
}
fn parse(bytes: &[u8]) -> Result<Inputs, BuildError> {
    if bytes.is_empty() || bytes.len() > MAX_REQUEST {
        return Err(refuse("input document exceeds bounds"));
    }
    let inputs: Inputs = serde_json::from_slice(bytes).map_err(refuse)?;
    for path in [
        &inputs.provenance,
        &inputs.catalog,
        &inputs.catalog_signature,
        &inputs.manifest,
        &inputs.manifest_signature,
        &inputs.canonical_content,
    ] {
        if !path.is_absolute() {
            return Err(refuse("every input locator must be absolute"));
        }
    }
    Ok(inputs)
}
struct InputPin {
    _directories: Vec<File>,
    file: local_source::PinnedFile,
}
fn pin(path: &Path, limit: u64) -> Result<InputPin, BuildError> {
    if !path.is_absolute() {
        return Err(refuse("absolute input locator required"));
    }
    let parent = path.parent().ok_or_else(|| refuse("input parent absent"))?;
    let directories = windows_inputs::pin_input_root(parent)?;
    let file = local_source::pin_file(path, limit)?;
    Ok(InputPin {
        _directories: directories,
        file,
    })
}
fn request_bytes<'a>(
    inputs: &'a Inputs,
    candidate: &'a Path,
    key: [u8; 32],
) -> Result<Vec<u8>, BuildError> {
    if !candidate.is_absolute() {
        return Err(refuse("candidate locator must be absolute"));
    }
    let request = Request {
        schema: "keld.windows-initial-install-request/v1",
        provenance: &inputs.provenance,
        catalog: &inputs.catalog,
        catalog_signature: &inputs.catalog_signature,
        manifest: &inputs.manifest,
        manifest_signature: &inputs.manifest_signature,
        canonical_content: &inputs.canonical_content,
        compressed_archive: candidate,
        release_public_key: key,
    };
    let bytes = serde_json::to_vec(&request).map_err(refuse)?;
    if bytes.len() > MAX_REQUEST {
        return Err(refuse("serialized request exceeds bounds"));
    }
    Ok(bytes)
}
fn require_receipt(output: &[u8]) -> Result<(), BuildError> {
    if output != b"WINDOWS_INITIAL_BASELINE_COMMITTED\n"
        && output != b"WINDOWS_INITIAL_BASELINE_COMMITTED\r\n"
    {
        return Err(refuse(
            "retired Host did not emit the exact initial-baseline receipt",
        ));
    }
    Ok(())
}
/// Called while the borrowed candidate and original signed pair remain alive.
/// The outer owner never generates keys, catalog rows or signatures and does not
/// choose an ACL policy. Explicit create mode asks the authenticated Host to create
/// new fixed-profile directories; it never repairs existing permissions.
pub(super) fn run(
    candidate: &LocalArchiveCandidate<'_>,
    input_path: &Path,
) -> Result<(), BuildError> {
    run_with_mode(candidate, input_path, InvocationKind::Provided, None)
}
pub(super) fn run_observed(
    candidate: &LocalArchiveCandidate<'_>,
    input_path: &Path,
    signed: Option<&super::local_manifest_session::SignedManifest<'_, '_>>,
) -> Result<(), BuildError> {
    run_with_mode(
        candidate,
        input_path,
        InvocationKind::Observed(ScaffoldChoice::Existing),
        signed,
    )
}
pub(super) fn run_create_scaffold(
    candidate: &LocalArchiveCandidate<'_>,
    input_path: &Path,
    signed: Option<&super::local_manifest_session::SignedManifest<'_, '_>>,
) -> Result<(), BuildError> {
    run_with_mode(
        candidate,
        input_path,
        InvocationKind::Observed(ScaffoldChoice::CreateNew),
        signed,
    )
}
#[expect(
    clippy::needless_pass_by_value,
    reason = "Keep the candidate and its retained handles owned until installation returns."
)]
fn run_with_mode(
    candidate: &LocalArchiveCandidate<'_>,
    input_path: &Path,
    kind: InvocationKind,
    signed: Option<&super::local_manifest_session::SignedManifest<'_, '_>>,
) -> Result<(), BuildError> {
    if let Some(signed) = signed {
        signed.require_candidate(candidate)?;
    }
    let pair = candidate.signed_pair()?;
    let mut input = pin(input_path, MAX_REQUEST as u64)?;
    let mut bytes = Vec::new();
    input.file.file.read_to_end(&mut bytes).map_err(refuse)?;
    let inputs = match kind {
        InvocationKind::Provided => InputMode::Provided(parse(&bytes)?),
        InvocationKind::Observed(mode) => InputMode::Observed(
            if let Some(signed) = signed {
                let i: SessionObservedInputs = serde_json::from_slice(&bytes).map_err(refuse)?;
                // Same emitted/verified manifest replaces locators only in explicit session mode.
                // Closed session shape forbids a caller-supplied manifest or signature.
                let value = serde_json::json!({"install_root":i.install_root,"update_root":i.update_root,"version":i.version,
            "catalog":i.catalog,"catalog_signature":i.catalog_signature,"manifest":signed.manifest(),"manifest_signature":signed.signature()});
                parse_observed(&serde_json::to_vec(&value).map_err(refuse)?)?
            } else {
                parse_observed(&bytes)?
            },
            mode,
        ),
    };
    let mut pins = Vec::new();
    for (path, limit) in inputs.files(candidate.receipt().content_size()) {
        pins.push(pin(path, limit)?);
    }
    if pins[1].file.file.metadata().map_err(refuse)?.len() != 64 {
        return Err(refuse("catalog signature must be exactly 64 raw bytes"));
    }
    if matches!(&inputs, InputMode::Provided(_))
        && pins[4].file.file.metadata().map_err(refuse)?.len() != candidate.receipt().content_size()
    {
        return Err(refuse(
            "canonical length differs from same-execution archive",
        ));
    }
    let bytes = inputs.request_bytes(candidate.path(), pair.release_public_key)?;
    let root = candidate
        .path()
        .parent()
        .ok_or_else(|| refuse("candidate root absent"))?;
    let request_path = root.join("initial-install-request.json");
    // Close the creation writer before the Host's deny-write read open. Candidate
    // ancestry stays pinned. Reopen and compare exact bytes before any execution.
    {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_READ)
            .open(&request_path)
            .map_err(refuse)?;
        output.write_all(&bytes).map_err(refuse)?;
        output.sync_all().map_err(refuse)?;
    }
    let mut request = pin(&request_path, MAX_REQUEST as u64)?;
    let mut actual = Vec::new();
    request.file.file.read_to_end(&mut actual).map_err(refuse)?;
    if actual != bytes {
        return Err(refuse("request changed across read-only reopening"));
    }
    let mut environment = Vec::<(OsString, OsString)>::new();
    for key in ["SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(key) {
            environment.push((key.into(), value));
        }
    }
    for key in [
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "TEMP",
        "TMP",
    ] {
        environment.push((key.into(), root.as_os_str().to_owned()));
    }
    let null = File::open("NUL").map_err(refuse)?;
    let args = [
        OsString::from("--keld-install-initial-v3"),
        request_path.as_os_str().to_owned(),
    ];
    // Existing owner returns success only after exit zero and process-family
    // retirement. The verified Host and all inputs remain pinned through it.
    let output = local_source::run_captured(
        &pair.host.executable,
        &args,
        &environment,
        root,
        &null,
        4096,
    )?;
    require_receipt(&output)?;
    println!(
        "Initial baseline installed and read back; application launch/Strict admission not yet verified."
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn inputs() -> serde_json::Value {
        serde_json::json!({"provenance":"D:/p","catalog":"D:/c","catalog_signature":"D:/s","manifest":"D:/m","manifest_signature":"D:/ms","canonical_content":"D:/content"})
    }
    #[test]
    fn closed_inputs_cannot_choose_host_candidate_key_or_authority() {
        let value = inputs();
        let bytes = serde_json::to_vec(&value).unwrap();
        let parsed = parse(&bytes).unwrap();
        for extra in [
            "host",
            "compressed_archive",
            "release_public_key",
            "catalog_authority",
            "allow_unverified",
        ] {
            let mut altered = value.clone();
            altered[extra] = serde_json::json!("D:/attacker");
            assert!(parse(&serde_json::to_vec(&altered).unwrap()).is_err());
        }
        let text = String::from_utf8(bytes).unwrap();
        let duplicated = text.replacen("\"catalog\":", "\"catalog\":\"D:/other\",\"catalog\":", 1);
        assert!(parse(duplicated.as_bytes()).is_err());
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove("catalog");
        assert!(parse(&serde_json::to_vec(&missing).unwrap()).is_err());
        let mut relative = value.clone();
        relative["catalog"] = serde_json::json!("relative");
        assert!(parse(&serde_json::to_vec(&relative).unwrap()).is_err());
        assert!(parse(&vec![b' '; MAX_REQUEST + 1]).is_err());
        let encoded = request_bytes(&parsed, Path::new("D:/actual.candidate"), [9; 32]).unwrap();
        let emitted: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(emitted["schema"], "keld.windows-initial-install-request/v1");
        assert_eq!(emitted["compressed_archive"], "D:/actual.candidate");
        assert_eq!(
            emitted["release_public_key"],
            serde_json::json!(vec![9_u8; 32])
        );
    }
    #[test]
    fn observed_schema_is_explicit_without_provenance_or_identity_override() {
        let mut value = inputs();
        value.as_object_mut().unwrap().remove("provenance");
        value.as_object_mut().unwrap().remove("canonical_content");
        value["install_root"] = serde_json::json!("D:/app");
        value["update_root"] = serde_json::json!("D:/app/update");
        value["version"] = serde_json::json!("1.0.0");
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(parse(&bytes).is_err());
        let observed = parse_observed(&bytes).unwrap();
        let mode = InputMode::Observed(observed, ScaffoldChoice::Existing);
        let wire: serde_json::Value = serde_json::from_slice(
            &mode
                .request_bytes(Path::new("D:/same.candidate"), [7; 32])
                .unwrap(),
        )
        .unwrap();
        assert_eq!(wire["schema"], "keld.windows-initial-install-request/v2");
        assert!(wire.get("provenance").is_none());
        assert_eq!(wire["compressed_archive"], "D:/same.candidate");
        assert_eq!(wire["scaffold_mode"], "existing");
        let create =
            InputMode::Observed(parse_observed(&bytes).unwrap(), ScaffoldChoice::CreateNew);
        let wire: serde_json::Value = serde_json::from_slice(
            &create
                .request_bytes(Path::new("D:/same.candidate"), [7; 32])
                .unwrap(),
        )
        .unwrap();
        assert_eq!(wire["scaffold_mode"], "create-new");
        for key in [
            "scaffold_mode",
            "canonical_content",
            "provenance",
            "publisher_scope",
            "volume_guid",
            "profile_binding",
            "catalog_authority",
            "host",
            "release_public_key",
        ] {
            let mut bad = value.clone();
            bad[key] = serde_json::json!("override");
            assert!(parse_observed(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
    }
    #[test]
    fn signing_session_input_cannot_override_emitted_manifest() {
        let mut value = serde_json::json!({"install_root":"D:/app","update_root":"D:/app/update","version":"1.0.0","catalog":"D:/c","catalog_signature":"D:/s"});
        assert!(serde_json::from_value::<SessionObservedInputs>(value.clone()).is_ok());
        for field in [
            "manifest",
            "manifest_signature",
            "scaffold_mode",
            "release_public_key",
        ] {
            value[field] = serde_json::json!("D:/override");
            assert!(serde_json::from_value::<SessionObservedInputs>(value.clone()).is_err());
            value.as_object_mut().unwrap().remove(field);
        }
    }
    #[test]
    fn receipt_is_not_a_substring_or_launch_claim() {
        require_receipt(b"WINDOWS_INITIAL_BASELINE_COMMITTED\n").unwrap();
        for bad in [
            b"".as_slice(),
            b"prefix WINDOWS_INITIAL_BASELINE_COMMITTED\n",
            b"WINDOWS_INITIAL_BASELINE_COMMITTED\nextra",
            b"LAUNCHED\n",
        ] {
            assert!(require_receipt(bad).is_err());
        }
    }
}
