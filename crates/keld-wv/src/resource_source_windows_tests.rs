//! Native filesystem component tests; no JavaScript or `WebView` success is claimed.

use super::*;
use sha2::{Digest as _, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);
const ENTRY: &str = "views/editor.html";
const ENTRY_BYTES: &[u8] = b"<html>selected fixture</html>\n";

const POLICY: &str = "{\"profile\":\"legacy\",\"app\":{},\"windows\":{\"fixture\":{\"web\":{\"csp\":\"original-editor-offline\"}},\"alternate\":{\"web\":{\"csp\":\"original-editor-offline-empty-style\"}}}}\n";
const EXPECTED: [u8; 32] = [
    37, 191, 98, 241, 120, 231, 6, 2, 85, 199, 249, 253, 178, 140, 25, 141, 146, 212, 245, 118,
    190, 67, 19, 60, 224, 95, 225, 252, 213, 109, 178, 51,
];

pub(crate) struct Fixture {
    owned: PathBuf,
    root: PathBuf,
    source: Option<Arc<AdmittedWebAppSource>>,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        use ::windows::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
        use std::os::windows::fs::OpenOptionsExt as _;
        let id = NEXT_FIXTURE.fetch_add(1, AtomicOrdering::Relaxed);
        let owned =
            std::env::temp_dir().join(format!("keld-windows-source-{}-{id}", std::process::id()));
        fs::create_dir(&owned).expect("new private fixture root");
        let root = owned.join("selected assets");
        fs::create_dir(&root).expect("selected source directory");
        let mut assets = Vec::new();
        for (path, bytes) in [
            (ENTRY, ENTRY_BYTES),
            ("views/other.html", b"<html>not a document grant</html>\n"),
            ("styles/main.css", b"body{margin:0}\n"),
            ("data/Caf\u{e9}.txt", b"literal unicode resource\n"),
        ] {
            let absolute = root.join(path);
            fs::create_dir_all(absolute.parent().expect("parent")).expect("fixture directories");
            fs::write(absolute, bytes).expect("literal fixture data");
            assets.push(VerifiedWebAsset {
                path: PathBuf::from(path),
                size: bytes.len() as u64,
                sha256: Sha256::digest(bytes).into(),
            });
        }
        let parent = fs::OpenOptions::new()
            .read(true)
            .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
            .custom_flags((FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).0)
            .open(&owned)
            .expect("retain owned fixture root");
        let retained =
            open_retained_graph_child(&parent, "selected assets", WindowsGraphChildKind::Directory)
                .expect("canonical selected child");
        // Independent fixed policy bytes/digest, not a page-supplied header.
        let policy_path = owned.join("policy.jsonc");
        fs::write(&policy_path, POLICY).expect("policy literal");
        let policy = keld_guard::verified_manifest::load_verified_manifest(
            File::open(&policy_path).expect("policy file"),
            policy_path,
            EXPECTED,
        )
        .expect("independently pinned policy")
        .resolve_window_csp("fixture")
        .expect("fixed trusted window key");
        let source = AdmittedWebAppSource::from_admitted_windows_graph(
            retained,
            [7; 32],
            assets,
            Path::new(ENTRY),
            policy,
        )
        .expect("component fixture handoff; not publisher authentication");
        Self {
            owned,
            root,
            source: Some(source),
        }
    }

    fn source(&self) -> &AdmittedWebAppSource {
        self.source.as_deref().expect("live fixture source")
    }

    pub(crate) fn source_arc(&self) -> Arc<AdmittedWebAppSource> {
        Arc::clone(self.source.as_ref().expect("live fixture source"))
    }

    pub(crate) fn url(&self, member: &str) -> String {
        // Use the actual selected directory behind TEMP aliases and junctions.
        // Encoding remains independent of the production URL constructor.
        let canonical = fs::canonicalize(&self.root).expect("selected native fixture root");
        let root = canonical.to_str().expect("fixture UTF8");
        let dos = root.strip_prefix(r"\\?\").unwrap_or(root);
        format!(
            "file:///{}/{}",
            dos.replace('\\', "/")
                .replace('%', "%25")
                .replace('#', "%23"),
            member
        )
        .replace(' ', "%20")
    }

    fn read(&self, member: &str) -> Result<VerifiedBytes, ResourceSourceError> {
        let selected = self.source().qualify_url(&self.url(member))?;
        self.source().verified_read(
            &selected,
            ORIGINAL_WEBAPP_LIMITS.max_asset_bytes,
            &AtomicBool::new(false),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.source.take());
        let _ = fs::remove_dir_all(&self.owned);
    }
}

#[test]
fn selected_entry_url_fixture_resolves_extended_windows_temp_spelling() {
    let mut fixture = Fixture::new();
    let path = fixture.root.to_str().expect("fixture UTF8");
    let extended = if path.starts_with(r"\\?\") {
        path.to_owned()
    } else {
        format!(r"\\?\{path}")
    };
    fixture.root = PathBuf::from(extended);
    let url = fixture.url(ENTRY);
    assert_eq!(
        url,
        fixture
            .source()
            .admitted_entry_url(None)
            .expect("exact admitted entry")
    );
    assert!(fixture.source().qualify_document_url(&url).is_ok());
}

#[test]
fn selected_entry_urls_preserve_query_presence_and_refuse_other_documents() {
    let fixture = Fixture::new();
    let source = fixture.source_arc();
    for query in [None, Some(""), Some("appLang=fr&queryObj=%257b&x=%252f")] {
        let url = source.admitted_entry_url(query).expect("admitted entry");
        let expected = match query {
            None => fixture.url(ENTRY),
            Some(query) => format!("{}?{query}", fixture.url(ENTRY)),
        };
        assert_eq!(url, expected);
        let parsed = source
            .qualify_document_url(&url)
            .expect("selected document only");
        assert_eq!(parsed.raw_query.as_deref(), query);
        assert_eq!(
            source
                .qualify_original_entry_uri(&url)
                .expect("same parser")
                .as_optional_str(),
            Ok(query)
        );
    }
    assert_eq!(
        source.qualify_document_url(&fixture.url("views/other.html")),
        Err(ResourceSourceError::InvalidEntry)
    );
    assert!(source.qualify_url(&fixture.url("views/other.html")).is_ok());
    assert_eq!(
        fixture
            .read("views/other.html")
            .expect("allowed subresource")
            .bytes,
        b"<html>not a document grant</html>\n"
    );
    assert!(source.admitted_entry_url(Some("x#fragment")).is_none());
    assert_eq!(source.source_id(), VerifiedSourceId([7; 32]));
    assert_eq!(source.window_key(), "fixture");
    assert!(source.response_policy().contains("default-src"));
}

#[test]
fn original_primary_selector_accepts_windows_node_file_url_without_widening_native_urls() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let canonical = fixture.url(ENTRY);
    let path = canonical.strip_prefix("file:///").expect("local file URL");
    let (directory, entry) = path.rsplit_once('/').expect("selected editor filename");
    // node:url.format with Windows path.join yields raw DOS directory separators
    // and spaces, then the application's literal slash before the filename.
    let directory = directory.replace('/', "\\").replace("%20", " ");
    let mixed = format!("file:///{directory}/{entry}");
    let query = r"appLang=fr&raw=%252f&literal=a\b";
    assert!(source.qualify_document_url(&mixed).is_err());
    assert!(source.qualify_url(&mixed).is_err());
    let selected = source
        .qualify_original_entry_uri(&format!("{mixed}?{query}"))
        .expect("Bun's original selected-entry spelling");
    assert_eq!(selected.as_optional_str(), Ok(Some(query)));
    assert_eq!(
        source.admitted_entry_url(Some(query)),
        Some(format!("{canonical}?{query}"))
    );
    assert_eq!(
        source
            .qualify_original_entry_uri(&format!("{mixed}?"))
            .expect("explicit empty query")
            .as_optional_str(),
        Ok(Some(""))
    );

    for refused in [
        format!("file://localhost/{directory}/{entry}"),
        format!("file:///{directory}/other.html"),
        format!(r"file:///{directory}\..\views/{entry}"),
        format!("file:///{directory}%5C{entry}"),
        format!(r"file:///{directory}\sentinel/{entry}"),
    ] {
        assert!(
            source.qualify_original_entry_uri(&refused).is_err(),
            "{refused}"
        );
    }
}

#[test]
fn resource_reader_authenticates_bytes_mime_and_ordinal_namespace() {
    let fixture = Fixture::new();
    let entry = fixture.read(ENTRY).expect("literal entry bytes");
    assert_eq!(entry.bytes, ENTRY_BYTES);
    assert_eq!(entry.mime, "text/html; charset=utf-8");
    let style = fixture
        .read("STYLES/MAIN.CSS")
        .expect("Windows ordinal resource selection");
    assert_eq!(style.bytes, b"body{margin:0}\n");
    assert_eq!(style.mime, "text/css; charset=utf-8");
    assert_eq!(
        fixture
            .read("data/Caf%C3%A9.txt")
            .expect("one UTF8 decode")
            .bytes,
        b"literal unicode resource\n"
    );
}

#[test]
fn unlisted_and_outside_files_never_become_resource_authority() {
    let fixture = Fixture::new();
    let outside = fixture.owned.join("outside.txt");
    fs::write(&outside, b"OUTSIDE-INDEPENDENT-SENTINEL").expect("readable outside control");
    fs::write(fixture.root.join("unlisted.txt"), b"UNLISTED")
        .expect("physically present unlisted file");
    assert_eq!(
        fixture
            .source()
            .qualify_url(&fixture.url("unlisted.txt"))
            .err(),
        Some(ResourceSourceError::AbsentAsset)
    );
    for member in [
        "../outside.txt",
        "%2e%2e/outside.txt",
        "%2foutside.txt",
        "views/editor.html:hidden",
        "alias.LNK",
    ] {
        assert_eq!(
            fixture.source().qualify_url(&fixture.url(member)).err(),
            Some(ResourceSourceError::InvalidUrl),
            "{member}"
        );
    }
    let outside_uri = format!(
        "file:///{}/outside.txt",
        fixture
            .owned
            .to_str()
            .expect("owned UTF8")
            .replace('\\', "/")
    );
    assert!(fixture.source().qualify_url(&outside_uri).is_err());
    assert_eq!(
        fs::read(outside).expect("outside actually readable"),
        b"OUTSIDE-INDEPENDENT-SENTINEL"
    );
}

#[test]
fn changed_bytes_forged_expected_digest_limits_and_cancellation_refuse() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let selected = source
        .qualify_url(&fixture.url(ENTRY))
        .expect("admitted request");
    let mut forged = selected.clone();
    forged.sha256[0] ^= 1;
    assert_eq!(
        source
            .verified_read(&forged, usize::MAX, &AtomicBool::new(false))
            .err(),
        Some(ResourceSourceError::SourceChanged)
    );
    assert_eq!(
        source
            .verified_read(&selected, 1, &AtomicBool::new(false))
            .err(),
        Some(ResourceSourceError::TooLarge)
    );
    assert_eq!(
        source
            .verified_read(&selected, usize::MAX, &AtomicBool::new(true))
            .err(),
        Some(ResourceSourceError::Cancelled)
    );
    fs::write(fixture.root.join(ENTRY), vec![b'X'; ENTRY_BYTES.len()])
        .expect("same-size different bytes");
    assert_eq!(
        source
            .verified_read(&selected, usize::MAX, &AtomicBool::new(false))
            .err(),
        Some(ResourceSourceError::SourceChanged)
    );
}

#[test]
fn retained_source_root_pins_its_name_until_the_source_is_released() {
    let mut fixture = Fixture::new();
    let moved = fixture.owned.join("moved");
    assert!(
        fs::rename(&fixture.root, &moved).is_err(),
        "retained source must deny namespace replacement"
    );
    drop(fixture.source.take());
    fs::rename(&fixture.root, &moved).expect("rename after actual source custody release");
}

#[test]
fn windows_resource_custody_follows_the_shared_source_account() {
    use crate::resource_intake::RetainedByteCredit;

    let fixture = Fixture::new();
    let source = fixture.source_arc();
    let window = Arc::clone(&source);
    let independent = AdmittedWebAppSource::from_admitted_windows_graph(
        source
            .webapp_root
            .try_clone()
            .expect("same retained directory"),
        [7; 32],
        source.assets.values().cloned().collect(),
        Path::new(ENTRY),
        source.policy.clone(),
    )
    .expect("independent source with the same inventory and policy");
    assert_eq!(source.source_id(), independent.source_id());
    assert_eq!(source.root_identity, independent.root_identity);
    assert!(source.shares_windows_resource_custody(&window));
    assert!(!source.shares_windows_resource_custody(&independent));

    let account = window.retained_credit_account();
    let credit = RetainedByteCredit::reserve(&source.retained_credit_account(), 64)
        .expect("retain first resource result");
    drop(source);
    assert!(credit.covers(&account, 64));
    assert!(!credit.covers(&independent.retained_credit_account(), 1));
    let pending = credit.reserve_more(32).expect("retain pending metadata");
    assert_eq!(account.load(Ordering::Acquire), 96);
    drop(credit);
    assert_eq!(account.load(Ordering::Acquire), 32);
    drop(pending);
    assert_eq!(account.load(Ordering::Acquire), 0);
}

#[test]
fn relative_reader_refuses_hardlink_and_intermediate_junction_substitution() {
    let fixture = Fixture::new();
    let outside = fixture.owned.join("outside");
    fs::create_dir(&outside).expect("outside control directory");
    fs::write(outside.join("editor.html"), ENTRY_BYTES).expect("identical bytes outside closure");
    fs::remove_file(fixture.root.join(ENTRY)).expect("remove original fixture entry");
    fs::hard_link(outside.join("editor.html"), fixture.root.join(ENTRY)).expect("hardlink alias");
    assert_eq!(
        fixture.read(ENTRY).err(),
        Some(ResourceSourceError::EscapingFile)
    );
    fs::remove_file(fixture.root.join(ENTRY)).expect("remove only fixture hardlink");
    fs::remove_dir(fixture.root.join("views"))
        .expect_err("other captured member keeps directory nonempty");
    fs::remove_file(fixture.root.join("views/other.html")).expect("remove fixture other member");
    fs::remove_dir(fixture.root.join("views")).expect("empty fixture directory");
    let link = fixture.root.join("views");
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command",
            "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:KELD_SOURCE_LINK -Target $env:KELD_SOURCE_TARGET | Out-Null"])
        .env("KELD_SOURCE_LINK", &link).env("KELD_SOURCE_TARGET", &outside)
        .output().expect("owned junction setup");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = fixture.read(ENTRY);
    fs::remove_dir(&link).expect("remove only owned junction, not its target");
    assert_eq!(result.err(), Some(ResourceSourceError::EscapingFile));
    assert_eq!(
        fs::read(outside.join("editor.html")).expect("outside remains"),
        ENTRY_BYTES
    );
}

#[test]
fn oversize_inventory_does_not_refuse_an_unrequested_source_asset() {
    let (assets, _) = admit_asset_census(
        vec![VerifiedWebAsset {
            path: PathBuf::from("large.bin"),
            size: 64 * 1024 * 1024 + 1,
            sha256: [9; 32],
        }],
        Path::new("large.bin"),
        |_| Ok(()),
    )
    .expect("inventory metadata creates no body allocation");
    let selected = &assets["large.bin"];
    assert_eq!(selected.size, 64 * 1024 * 1024 + 1);
    let fixture = Fixture::new();
    let mut assets: Vec<_> = fixture.source().assets.values().cloned().collect();
    assets.push(selected.clone());
    let source = AdmittedWebAppSource::from_admitted_windows_graph(
        fixture
            .source()
            .webapp_root
            .try_clone()
            .expect("same retained root"),
        [7; 32],
        assets,
        Path::new(ENTRY),
        fixture.source().policy.clone(),
    )
    .expect("an unrequested oversized row does not prevent source admission");
    let request = source
        .qualify_url(&fixture.url("large.bin"))
        .expect("selected inventory row");
    assert_eq!(
        source
            .verified_read(&request, usize::MAX, &AtomicBool::new(false))
            .err(),
        Some(ResourceSourceError::TooLarge)
    );
}
