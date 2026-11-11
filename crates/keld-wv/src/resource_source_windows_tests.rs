//! Native filesystem component tests; no JavaScript or WebView success is claimed.

use super::*;
use sha2::{Digest as _, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);
const ENTRY: &str = "views/editor.html";
const ENTRY_BYTES: &[u8] = b"<html>selected fixture</html>\n";

pub(crate) struct Fixture {
    owned: PathBuf,
    root: PathBuf,
    source: Option<Arc<AdmittedWebAppSource>>,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
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
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&owned)
            .expect("retain owned fixture root");
        let retained =
            open_retained_graph_child(&parent, "selected assets", WindowsGraphChildKind::Directory)
                .expect("canonical selected child");
        // Independent fixed policy bytes/digest, not a page-supplied header.
        const POLICY: &str = "{\"profile\":\"legacy\",\"app\":{},\"windows\":{\"fixture\":{\"web\":{\"csp\":\"original-editor-offline\"}}}}\n";
        const EXPECTED: [u8; 32] = [
            215, 46, 191, 72, 140, 173, 92, 255, 48, 59, 90, 236, 196, 197, 67, 68, 75, 213, 119,
            101, 68, 44, 13, 125, 203, 40, 242, 147, 25, 32, 233, 33,
        ];
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
        // Independent fixture spelling, not the production URL constructor.
        format!(
            "file:///{}/{}",
            self.root.to_str().expect("fixture UTF8").replace('\\', "/"),
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
    assert_eq!(source.window_key(), "fixture");
    assert!(source.response_policy().contains("default-src"));
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
