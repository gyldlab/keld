//! Explicit LOCAL TEST archive continuation. No installer or release publication.
use std::path::Path;
use std::io::{Seek, SeekFrom, Write as _};
use std::fs::{File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt as _;
use sha2::{Digest as _, Sha256};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
use super::{BuildError, compiler_toolchain::CapturedCompilerToolchain, local_archive, local_images};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum InitialInstallMode {Provided,Observed,CreateNew}
struct Invocation<'a> {
    images: &'a [String],
    tools: &'a [String],
    modules: &'a [String],
    published_at: Option<&'a str>,
    initial_install: Option<&'a str>,
    install_mode: InitialInstallMode,
    signing_session: bool,
}
fn parse(args: &[String]) -> Result<Invocation<'_>, BuildError> {
    let split = args.iter().position(|value| value == "--compiler")
        .ok_or_else(|| BuildError::invocation("local archive requires --compiler after existing image inputs"))?;
    let (images, rest) = args.split_at(split);
    // Reuse the image command's sole shape/cache-mode parser.
    local_images::image_source_args(images)?;
    let (mut module_index, published_at) = if rest.get(6).map(String::as_str) == Some("--published-at") {
        let stamp = rest.get(7).ok_or_else(|| BuildError::invocation("missing --published-at UTC timestamp"))?;
        validate_stamp(stamp)?;
        (9, Some(stamp.as_str()))
    } else { (7, None) };
    let signing_session=rest.get(module_index-1).map(String::as_str)==Some("--manifest-signing-session");
    if signing_session {module_index+=1;}
    let flag=rest.get(module_index-1).map(String::as_str);
    let install_mode=match flag {
        Some("--initial-install-observed-inputs")=>InitialInstallMode::Observed,
        Some("--initial-install-create-scaffold-inputs")=>InitialInstallMode::CreateNew,
        _=>InitialInstallMode::Provided,
    };
    let initial_install=if matches!(flag,Some("--initial-install-inputs"|"--initial-install-observed-inputs"|"--initial-install-create-scaffold-inputs")) {
        let path=rest.get(module_index).ok_or_else(||BuildError::invocation("missing initial-install input locator"))?;
        if !Path::new(path).is_absolute() {return Err(BuildError::invocation("initial-install inputs must be absolute"));}
        module_index+=2;Some(path.as_str())
    } else {None};
    if rest.len() <= module_index || rest[module_index - 1] != "--modules" {
        return Err(BuildError::invocation("expected --compiler TYPESCRIPT_ROOT BUN BUN_SHA256 TSC TSC_SHA256 [--published-at YYYY-MM-DDTHH:MM:SSZ] [--manifest-signing-session] [--initial-install-inputs ABS_JSON | --initial-install-observed-inputs ABS_JSON | --initial-install-create-scaffold-inputs ABS_JSON] --modules main.js [relative.js ...]"));
    }
    if rest[module_index..].iter().any(|name| name.starts_with("--")) {
        return Err(BuildError::invocation("unknown option in explicit module inventory"));
    }
    if signing_session && (published_at.is_none() || (initial_install.is_some() && install_mode==InitialInstallMode::Provided)) {
        return Err(BuildError::invocation("manifest signing session requires --published-at; optional initial install must use observed/create-scaffold inputs"));
    }
    Ok(Invocation { signing_session, images, tools: &rest[1..6], modules: &rest[module_index..], published_at, initial_install, install_mode })
}

pub(super) struct CompilerRequest {
    pub(super) typescript_root:std::path::PathBuf,
    pub(super) bun:std::path::PathBuf,
    pub(super) bun_sha256:[u8;32],
    pub(super) tsc:std::path::PathBuf,
    pub(super) tsc_sha256:[u8;32],
}
pub(super) fn run(args: &[String]) -> Result<(), BuildError> {
    let invocation=parse(args)?;let t=invocation.tools;
    let compiler=CompilerRequest {typescript_root:t[0].clone().into(),bun:t[1].clone().into(),bun_sha256:local_images::hex::<32>(&t[2])?,tsc:t[3].clone().into(),tsc_sha256:local_images::hex::<32>(&t[4])?};
    execute(local_images::request_from_args(invocation.images)?,compiler,invocation.modules,invocation.published_at,
        invocation.initial_install.map(|path|(Path::new(path),invocation.install_mode)),invocation.signing_session)
}
/// Ordinary Windows test-only product route. Same retained archive transaction;
/// no installer invocation or new key/certificate/trust is selected here.
pub(super) fn run_product(images:local_images::ImageRequest,compiler:CompilerRequest,modules:&[String],published_at:&str)->Result<(),BuildError> {
    validate_stamp(published_at)?;
    execute(images,compiler,modules,Some(published_at),None,true)
}
#[expect(clippy::needless_pass_by_value, reason = "Keep compiler inputs owned for this complete build transaction.")]
fn execute(images:local_images::ImageRequest,tools:CompilerRequest,modules:&[String],published_at:Option<&str>,initial_install:Option<(&Path,InitialInstallMode)>,signing_session:bool)->Result<(),BuildError> {
    let compiler=CapturedCompilerToolchain::capture(&tools.typescript_root,&tools.bun,&tools.bun_sha256,&tools.tsc,&tools.tsc_sha256)?;
    let channel=images.channel.clone();let selected_commit=images.source.commit.clone();
    let pair=local_images::prepare_request(images,initial_install.is_some()||signing_session)?;
    if pair.source.source_commit!=selected_commit {return Err(BuildError::input("source receipt","actual source owner returned a different commit"));}
    let app_id = pair.authoring.app.id.clone();
    let version = pair.authoring.app.version.clone();
    let mut candidate = local_archive::prepare(&pair, &compiler, modules)?;
    let stored_size = candidate.retained_reader()
        .and_then(|mut reader| reader.seek(SeekFrom::End(0)))
        .map_err(|error| BuildError::input("local archive readback", error))?;
    if stored_size != candidate.receipt().compressed_size() {
        return Err(BuildError::input("local archive readback", "retained output size differs from producer receipt"));
    }
    let manifest_files = published_at.map(|stamp| {
        emit_manifest(&candidate, &app_id, &version, &channel, stamp)
    }).transpose()?;
    let signed_manifest=if signing_session {
        let emitted=manifest_files.as_ref().ok_or_else(||BuildError::invocation("emitted manifest required"))?;
        Some(super::local_manifest_session::wait(&candidate,&emitted.0)?)
    } else {None};
    if let Some((inputs,mode))=initial_install {
        match mode {
            InitialInstallMode::Provided=>super::local_initial_install::run(&candidate,inputs)?,
            InitialInstallMode::Observed=>super::local_initial_install::run_observed(&candidate,inputs,signed_manifest.as_ref())?,
            InitialInstallMode::CreateNew=>super::local_initial_install::run_create_scaffold(&candidate,inputs,signed_manifest.as_ref())?,
        }
    }
    println!("Selected integrated Host source commit: {}",pair.source.source_commit);
    match pair.source.cache_sha256 {
        Some(digest)=>println!("Offline dependency snapshot SHA256: {}",super::local_source::observed_hex(&digest)),
        None=>println!("NONHERMETIC: mutable developer Cargo cache reused without copying, measuring or pinning dependency contents."),
    }
    println!("Source preparation remains nonhermetic; selected commit is not an official release/source-trust claim.");
    println!("Prepared LOCAL TEST application archive candidate: {}", candidate.path().display());
    println!("Canonical archive bytes: compressed={}, content={}", candidate.receipt().compressed_size(), candidate.receipt().content_size());
    if initial_install.is_none() { println!("No installation was requested; this candidate is not installed/Strict qualification."); }
    println!("{}",serde_json::json!({"event":"windows-local-test-build-complete","sourceCommit":&pair.source.source_commit,
        "sourceRecipeSha256":super::local_source::observed_hex(&pair.source.recipe_sha256),
        "archive":candidate.path(),"archiveBlake3":super::local_source::observed_hex(candidate.receipt().compressed_blake3()),
        "signedManifest":signed_manifest.is_some(),"initialBaselineCommitted":initial_install.is_some(),
        "dependencyMode":if pair.source.cache_sha256.is_some(){"snapshot"}else{"mutable-developer"},
        "nonhermetic":true,"launchedOrStrictQualified":false}));
    Ok(())
}

fn validate_stamp(stamp: &str) -> Result<(), BuildError> {
    let b = stamp.as_bytes();
    let valid_shape = b.len() == 20 && [4, 7, 10, 13, 16, 19].into_iter()
        .zip(b"--T::Z").all(|(index, expected)| b[index] == *expected)
        && b.iter().enumerate().all(|(index, byte)| [4,7,10,13,16,19].contains(&index) || byte.is_ascii_digit());
    if !valid_shape { return Err(BuildError::invocation("--published-at requires exact UTC YYYY-MM-DDTHH:MM:SSZ")); }
    let number = |range: std::ops::Range<usize>| stamp[range].parse::<u32>().map_err(|_| BuildError::invocation("invalid UTC timestamp"));
    let year = number(0..4)?; let month = number(5..7)?; let day = number(8..10)?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month { 1|3|5|7|8|10|12 => 31, 4|6|9|11 => 30, 2 if leap => 29, 2 => 28, _ => 0 };
    if year < 1970 || day == 0 || day > days || number(11..13)? > 23 || number(14..16)? > 59 || number(17..19)? > 59 {
        return Err(BuildError::invocation("invalid UTC timestamp"));
    }
    Ok(())
}

fn manifest_bytes(receipt: &keld_pack::ProducedFull, app_id: &str, version: &str, channel: &str, stamp: &str) -> Result<Vec<u8>, BuildError> {
    validate_stamp(stamp)?;
    // Serialization only. Update remains the sole authenticated manifest/parser owner.
    serde_json::to_vec(&serde_json::json!({
        "schema":1, "channel":channel, "target":"windows-x64", "app":{"id":app_id},
        "releases":[{"version":version, "publishedAt":stamp, "deltas":[], "full":{
            "url":"app.keld.tar.zst.candidate", "size":receipt.compressed_size(),
            "blake3":super::local_source::observed_hex(receipt.compressed_blake3()),
            "contentSize":receipt.content_size(),
            "contentBlake3":super::local_source::observed_hex(receipt.content_blake3())
        }}]
    })).map_err(|error| BuildError::input("local manifest serialization", error))
}

fn write_new_manifest_file(path: &Path, bytes: &[u8]) -> Result<File, BuildError> {
    let mut file = OpenOptions::new().read(true).write(true).create_new(true)
        .share_mode(FILE_SHARE_READ).open(path)
        .map_err(|error| BuildError::input("local manifest creation", error))?;
    file.write_all(bytes).and_then(|()| file.sync_all())
        .map_err(|error| BuildError::input("local manifest write", error))?;
    Ok(file)
}

fn emit_manifest(candidate: &local_archive::LocalArchiveCandidate<'_>, app_id: &str, version: &str, channel: &str, stamp: &str) -> Result<(File, File), BuildError> {
    let bytes = manifest_bytes(candidate.receipt(), app_id, version, channel, stamp)?;
    let hash = super::local_source::observed_hex(&Sha256::digest(&bytes));
    // Root is private to the existing candidate owner; its directory pins stay live.
    let root = candidate.path().parent().ok_or_else(|| BuildError::input("local manifest", "candidate parent absent"))?;
    let manifest = write_new_manifest_file(&root.join("app.keld.manifest.json"), &bytes)?;
    let receipt = serde_json::to_vec(&serde_json::json!({"scope":"local-test-only", "manifestSha256":hash}))
        .map_err(|error| BuildError::input("local manifest receipt", error))?;
    let receipt = write_new_manifest_file(&root.join("app.keld.manifest.receipt.json"), &receipt)?;
    println!("Prepared LOCAL TEST unsigned manifest SHA256 {hash}; detached signature still required.");
    let manifest=local_archive::reopen_emitted_archive(manifest,&root.join("app.keld.manifest.json"),bytes.len() as u64)?;
    Ok((manifest, receipt))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn arguments(developer: bool) -> Vec<String> {
        let mut args: Vec<String> = (0..15).map(|i| format!("image-{i}")).collect();
        if developer { args.push("--use-developer-cache".into()); }
        args.extend(["--compiler", "typescript", "bun", "bun-sha", "tsc", "tsc-sha", "--modules", "main.js"].map(str::to_owned));
        args
    }
    #[test]
    fn signing_requires_timestamp_and_does_not_require_installation() {
        let mut args=arguments(false);let i=args.iter().position(|v|v=="--modules").unwrap();
        args.splice(i..i,["--published-at","2026-11-10T00:00:00Z","--manifest-signing-session","--initial-install-observed-inputs","D:/inputs.json"].map(str::to_owned));
        assert!(parse(&args).unwrap().signing_session);
        let mut archive_only=args.clone();archive_only.drain(i+3..i+5);let parsed=parse(&archive_only).unwrap();assert!(parsed.signing_session);assert!(parsed.initial_install.is_none());
        let mut wrong=args.clone();wrong[i+3]="--initial-install-inputs".into();assert!(parse(&wrong).is_err());
        let mut no_stamp=args;no_stamp.drain(i..i+2);assert!(parse(&no_stamp).is_err());
    }
    #[test]
    fn initial_install_is_explicit_closed_and_absolute() {
        let mut args=arguments(false);let index=args.iter().position(|s|s=="--modules").unwrap();
        args.splice(index..index,["--initial-install-inputs".into(),"D:/inputs.json".into()]);
        assert_eq!(parse(&args).unwrap().initial_install,Some("D:/inputs.json"));
        assert_eq!(parse(&args).unwrap().install_mode,InitialInstallMode::Provided);
        args[index]="--initial-install-observed-inputs".into();assert_eq!(parse(&args).unwrap().install_mode,InitialInstallMode::Observed);
        args[index]="--initial-install-create-scaffold-inputs".into();assert_eq!(parse(&args).unwrap().install_mode,InitialInstallMode::CreateNew);
        args[index]="--initial-install-inputs".into();
        args[index+1]="relative.json".into();assert!(parse(&args).is_err());
        args[index+1]="D:/inputs.json".into();args.insert(index+2,"--initial-install-inputs".into());assert!(parse(&args).is_err());
    }
    #[test]
    fn archive_invocation_preserves_explicit_image_cache_mode() {
        for developer in [false, true] {
            let args = arguments(developer);
            let invocation = parse(&args).expect("bounded archive invocation");
            assert_eq!(invocation.images.len(), if developer {16} else {15});
            assert_eq!(invocation.tools.len(), 5);
            assert_eq!(invocation.modules, ["main.js"]);
        }
    }
    #[test]
    fn archive_invocation_refuses_missing_or_ambiguous_segments() {
        let args = arguments(false);
        for end in 0..args.len() { assert!(parse(&args[..end]).is_err()); }
        let mut unknown = args.clone(); unknown.push("--retry".into());
        assert!(parse(&unknown).is_err());
        let mut wrong_separator = args; wrong_separator[21] = "--other".into();
        assert!(parse(&wrong_separator).is_err());
    }
    #[test]
    fn archive_manifest_mode_requires_explicit_valid_timestamp() {
        let mut args = arguments(false);
        args.splice(21..21, ["--published-at".to_owned(), "2026-11-10T23:59:59Z".to_owned()]);
        let invocation = parse(&args).expect("explicit local manifest");
        assert_eq!(invocation.published_at, Some("2026-11-10T23:59:59Z"));
        assert_eq!(invocation.modules, ["main.js"]);
        for invalid in ["", "2026-02-29T00:00:00Z", "2026-11-10T24:00:00Z", "2026-11-10T00:00:00+00:00"] {
            args[22] = invalid.to_owned();
            assert!(parse(&args).is_err());
        }
        assert!(validate_stamp("2024-02-29T00:00:00Z").is_ok());
    }

    #[test]
    fn local_manifest_uses_actual_pack_receipt_and_preserves_collisions() {
        let mut input = std::io::Cursor::new(b"synthetic helper, never executed");
        let mut entries = [keld_pack::PackageEntry::File {name:"keld-updater-helper.exe",size:32,input:&mut input}];
        let receipt = keld_pack::produce_windows_v0(&mut entries, &mut Vec::new()).expect("real Pack owner");
        let bytes = manifest_bytes(&receipt,"dev.keld.fixture","1.0.0","stable","2026-11-10T00:00:00Z").expect("serialize");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("test-only inspect");
        let full = &json["releases"][0]["full"];
        assert_eq!(full["size"], receipt.compressed_size());
        assert_eq!(full["contentSize"], receipt.content_size());
        assert_eq!(full["blake3"], super::super::local_source::observed_hex(receipt.compressed_blake3()));
        assert_eq!(full["contentBlake3"], super::super::local_source::observed_hex(receipt.content_blake3()));
        let root = tempfile::tempdir().expect("owned collision fixture");
        let path = root.path().join("manifest.json");
        std::fs::write(&path,b"unchanged").expect("existing fixture");
        assert!(write_new_manifest_file(&path,&bytes).is_err());
        assert_eq!(std::fs::read(path).expect("preserved"),b"unchanged");
    }
}

#[cfg(test)]
mod native_parser_regression {
    use super::*;
    fn arguments(developer: bool) -> Vec<String> {
        let mut args: Vec<String> = (0..15).map(|i| format!("image-{i}")).collect();
        if developer {
            args.push("--use-developer-cache".into());
        }
        args.extend(
            [
                "--compiler",
                "typescript",
                "bun",
                "bun-sha",
                "tsc",
                "tsc-sha",
                "--modules",
                "main.js",
            ]
            .map(str::to_owned),
        );
        args
    }
    #[test]
    fn archive_invocation_preserves_explicit_image_cache_mode() {
        for developer in [false, true] {
            let args = arguments(developer);
            let invocation = parse(&args).expect("bounded archive invocation");
            assert_eq!(invocation.images.len(), if developer { 16 } else { 15 });
            assert_eq!(invocation.tools.len(), 5);
            assert_eq!(invocation.modules, ["main.js"]);
        }
    }
    #[test]
    fn archive_invocation_refuses_missing_or_ambiguous_segments() {
        let args = arguments(false);
        for end in 0..args.len() {
            assert!(parse(&args[..end]).is_err());
        }
        let mut unknown = args.clone();
        unknown.push("--retry".into());
        assert!(parse(&unknown).is_err());
        let mut wrong_separator = args;
        wrong_separator[21] = "--other".into();
        assert!(parse(&wrong_separator).is_err());
    }
}
