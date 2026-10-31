//! Existing Windows project-fixture resource owners.

use std::{fs, process::Command};

use crate::{DARK_BG, PRODUCT_TITLE};

pub(crate) struct StageFixture {
    root: tempfile::TempDir,
    pub(crate) project: std::path::PathBuf,
}

impl StageFixture {
    pub(crate) fn new() -> Self {
        let root = tempfile::tempdir().expect("stage fixture root");
        let project = root.path().join("project");
        fs::create_dir_all(project.join("src")).expect("project src");
        fs::write(
            project.join("keld.config.ts"),
            "export default {\n  name: \"KEL96 T4 Fixture\",\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n} as const;\n",
        )
        .expect("project config");
        fs::write(project.join("src/main.ts"), "console.log('fixture');\n").expect("project entry");
        fs::write(project.join("index.html"), "<p id=exact>fixture</p>\n")
            .expect("project renderer");
        Self { root, project }
    }

    /// Builds a real public API role with the canonical external transport sidecar.
    pub(crate) fn legacy_api_ready() -> Self {
        let fixture = Self::new();
        fs::write(
            fixture.project.join("keld.config.ts"),
            format!(
                "export default {{\n  name: \"{PRODUCT_TITLE}\",\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n}} as const;\n"
            ),
        )
        .expect("write legacy API native window title");
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace root");
        let api = serde_json::to_string(&repo.join("packages/@keld/api/src/app.ts"))
            .expect("encode canonical app import");
        let transport = serde_json::to_string(&repo.join("packages/@keld/kipc/src/transport.ts"))
            .expect("encode canonical transport import");
        let source = fixture.project.join("src/legacy-api-ready.ts");
        fs::write(
            &source,
            format!(
                "import {{ app }} from {api};\n\
                 import {{ nativeFacadeBootSelected, isCallError }} from {transport};\n{LEGACY_API_READY_ROLE}"
            ),
        )
        .expect("write real legacy API role");
        let output = Command::new("bun")
            .arg(repo.join("packages/@keld/api/fixtures/staged-api-bundle.ts"))
            .arg(&source)
            .arg(fixture.project.join("src/main.ts"))
            .output()
            .expect("bundle legacy API through existing sidecar owner");
        assert!(
            output.status.success(),
            "legacy API bundle failed: {output:?}"
        );
        let bundle = fs::read_to_string(fixture.project.join("src/main.ts"))
            .expect("read legacy API bundle");
        assert!(
            bundle.contains("from \"./kipc-transport.ts\"") && !bundle.contains("class WorkerLink"),
            "legacy API must use the canonical external transport Worker entry"
        );
        fs::copy(
            repo.join("packages/@keld/kipc/src/transport.ts"),
            fixture.project.join("src/kipc-transport.ts"),
        )
        .expect("stage canonical transport sidecar");
        fs::remove_file(source).expect("remove bundle-only role source");
        fs::write(
            fixture.project.join("index.html"),
            format!("<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title>\n"),
        )
        .expect("write legacy API renderer");
        fixture
    }

    /// Removes the test-owned workspace after staged directory guards are released.
    /// This is fixture cleanup, not proof of CLI-owned dev-stage deletion.
    pub(crate) fn close_fixture(self) {
        let root = self.root.path().to_owned();
        self.root
            .close()
            .expect("close test-owned fixture workspace");
        assert!(
            !root.exists(),
            "test-owned fixture workspace survived close"
        );
    }
}

const LEGACY_API_READY_ROLE: &str = r#"
import { createConnection } from "node:net";
const control = createConnection({ host: "127.0.0.1", port: Number(process.env.KELD_T1B_CONTROL) });
await new Promise<void>((resolve, reject) => {
  control.once("connect", resolve);
  control.once("error", reject);
});
function report(record: string): Promise<void> {
  return new Promise((resolve, reject) => {
    control.write(`${record}\n`, (error) => error ? reject(error) : resolve());
  });
}
// The live environment, read through Reflect: a dotted read is substituted by the
// launch define while parsing and would hide an inherited hint (GH-1188 AC4).
await report(`MODE ${JSON.stringify({
  pid: process.pid, hint: Reflect.get(process.env, "KELD_NATIVE_WINDOW_FACADE") ?? null,
  selected: nativeFacadeBootSelected,
})}`);
try {
  await app.whenReady();
  await report("READY");
} catch (error) {
  await report(`ERROR ${JSON.stringify({
    code: isCallError(error) ? error.code : null,
    message: error instanceof Error ? error.message : String(error),
  })}`);
}
// The test retires the live Host through its existing dev lease after observing
// the actual API result. Keep the control socket open until Bun is reaped.
"#;

/// Bundles one public-API `role` to `output`, the transport left external as the staged
/// `./kipc-transport.ts`, through the existing sidecar owner.
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub(crate) fn bundle_public_role(
    repo: &std::path::Path,
    project: &std::path::Path,
    role: &str,
    output: &std::path::Path,
) {
    let source = project.join("src/bundled-role.ts");
    fs::write(&source, role).expect("write public role source");
    let result = Command::new("bun")
        .arg(repo.join("packages/@keld/api/fixtures/staged-api-bundle.ts"))
        .arg(&source)
        .arg(output)
        .output()
        .expect("bundle public role through existing sidecar owner");
    assert!(
        result.status.success(),
        "public role bundle failed: {result:?}"
    );
    let bundle = fs::read_to_string(output).expect("read public role bundle");
    assert!(
        bundle.contains("from \"./kipc-transport.ts\"") && !bundle.contains("class WorkerLink"),
        "the role must use the staged external transport"
    );
    fs::remove_file(source).expect("remove bundle-only role source");
}

/// Copies the ordinary package layout: every source file keeps its bytes.
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub(crate) fn copy_public_packages(repo: &std::path::Path, destination: &std::path::Path) {
    fn copy_directory(source: &std::path::Path, destination: &std::path::Path) {
        fs::create_dir_all(destination).expect("package source destination");
        for entry in fs::read_dir(source).expect("canonical package source directory") {
            let entry = entry.expect("canonical source entry");
            let target = destination.join(entry.file_name());
            if entry.file_type().expect("canonical source type").is_dir() {
                copy_directory(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).expect("byte-identical package source");
            }
        }
    }
    for package in ["api", "electron", "kipc"] {
        copy_directory(
            &repo.join(format!("packages/@keld/{package}/src")),
            &destination.join(format!("@keld/{package}/src")),
        );
    }
}

fn bundle_t1b_entry(repo: &std::path::Path, project: &std::path::Path) -> String {
    let source = project.join("src/t1b-entry.ts");
    let bundled = project.join("src/t1b-entry.bundle.js");
    let api_link = serde_json::to_string(
        &repo
            .join("packages/@keld/api/src/link.ts")
            .to_string_lossy(),
    )
    .expect("encode canonical @keld/api link path");
    let kipc = serde_json::to_string(
        &repo
            .join("packages/@keld/kipc/src/transport.ts")
            .to_string_lossy(),
    )
    .expect("encode canonical @keld/kipc transport path");
    let prelude = format!(
        "import {{ DrainSignal, FrameKind, FrameReader, LIFECYCLE_CHANNEL, WriteQueue, errorFromErrFrame, isWin32PipeEndpoint, parseAppLink, withIoDeadline }} from {api_link};\n\
         import {{ parseWin32DiagnosticPort }} from {kipc};\n"
    );
    fs::write(
        &source,
        format!(
            "{prelude}{}",
            include_str!("../../../fixtures/t1b_harness.ts")
        ),
    )
    .expect("write Windows T1b bundle entry");
    let output = Command::new("bun")
        .arg("build")
        .arg(&source)
        .args(["--target=bun", "--format=esm", "--outfile"])
        .arg(&bundled)
        .output()
        .expect("bundle Windows T1b fixture through canonical @keld/api owner");
    assert!(
        output.status.success(),
        "Windows T1b API bundle failed: {output:?}"
    );
    let entry = fs::read_to_string(&bundled).expect("read bundled Windows T1b entry");
    fs::remove_file(source).expect("remove temporary Windows T1b source");
    fs::remove_file(bundled).expect("remove temporary Windows T1b bundle");
    entry
}

pub(crate) struct ProductFixture {
    pub(crate) root: tempfile::TempDir,
    pub(crate) project: std::path::PathBuf,
}

impl ProductFixture {
    pub(crate) fn new() -> Self {
        let root = tempfile::tempdir().expect("product fixture root");
        let project = root.path().join("project");
        fs::create_dir_all(project.join("src")).expect("product project src");
        fs::write(
            project.join("keld.config.ts"),
            format!(
                "export default {{\n  name: \"{PRODUCT_TITLE}\",\n  entry: \"src/main.ts\",\n  renderer: \"index.html\",\n}} as const;\n"
            ),
        )
        .expect("product config");
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace root");
        let entry = bundle_t1b_entry(repo, &project);
        fs::write(project.join("src/main.ts"), entry).expect("product entry");
        fs::write(
            project.join("index.html"),
            format!("<!doctype html>{DARK_BG}\n"),
        )
        .expect("product renderer");
        Self { root, project }
    }

    /// The authenticated raw lifecycle fixture as a rendererless schema-two project,
    /// changing only the compiler-selected initial-window declaration.
    pub(crate) fn schema_two() -> Self {
        let fixture = Self::new();
        fs::write(
            fixture.project.join("keld.config.ts"),
            format!(
                "export default {{\n  name: \"{PRODUCT_TITLE}\",\n  entry: \"src/main.ts\",\n  initialWindow: \"none\",\n}} as const;\n"
            ),
        )
        .expect("schema-two config");
        fs::remove_file(fixture.project.join("index.html")).expect("no renderer source");
        fixture
    }
}
