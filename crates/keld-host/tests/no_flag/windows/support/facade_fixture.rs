//! Bun roles and stage shapes of the Windows facade-boot cells (GH-1188 T1).

use std::fs;
use std::path::Path;
use std::process::Command;

use super::product::{StageFixture, bundle_public_role, copy_public_packages};
use crate::facade_ready::{SCHEMA_TWO_CONFIG, stage_boot};

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

/// Stages the ordinary unbundled packages beside the entry: the transport has no
/// preloaded sibling, so it evaluates only at the role's late `import()`.
pub(crate) fn stage_public_packages(stage: &keld_cli::boot::DevBootStage) {
    copy_public_packages(repo_root(), &stage.root().join("src/packages"));
}

pub(crate) const LATE_PACKAGE_ROLE: &str = r#"
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
const control = createConnection({ host: "127.0.0.1", port: Number(process.env.KELD_T1B_CONTROL) });
await new Promise<void>((resolve, reject) => { control.once("connect", resolve); control.once("error", reject); });
const report = (message: string) => control.write(`${message}\n`);
try {
// Reflect reads the live environment: a dotted read is the launch define (AC4).
report(`MODE ${JSON.stringify({
  pid: process.pid,
  hint: Reflect.get(process.env, "KELD_NATIVE_WINDOW_FACADE") ?? null,
  admission: Reflect.get(process.env, "KELD_TEST_WINDOWS_ADMIT_FACADE_BOOT") ?? null,
  stall: Reflect.get(process.env, "KELD_T1B_STALL_BEFORE_INIT") ?? null,
  bunOptions: Reflect.get(process.env, "BUN_OPTIONS") ?? null,
})}`);
Reflect.deleteProperty(process.env, "KELD_NATIVE_WINDOW_FACADE");
if (Reflect.get(process.env, "KELD_NATIVE_WINDOW_FACADE") !== undefined) throw new Error("actual env was not deleted");
const transport = await import("./packages/@keld/kipc/src/transport.ts");
report(`SELECTED ${transport.nativeFacadeBootSelected}`);
const { app, BrowserWindow } = await import("./packages/@keld/electron/src/index.ts");
let framesOnWindowChannel = 0;
const prototype = transport.WorkerLink.prototype;
const call = prototype.call;
const callBlocking = prototype.callBlocking;
prototype.call = function (channel: number, ...rest: unknown[]) {
  if (channel === transport.WINDOW_CHANNEL) framesOnWindowChannel += 1;
  return Reflect.apply(call, this, [channel, ...rest]);
};
prototype.callBlocking = function (channel: number, ...rest: unknown[]) {
  if (channel === transport.WINDOW_CHANNEL) framesOnWindowChannel += 1;
  return Reflect.apply(callBlocking, this, [channel, ...rest]);
};
const code = (error: unknown) => /KELD-[A-Z]+-\d+/.exec(String(error))?.[0] ?? `untyped:${String(error)}`;
if (app.isReady()) throw new Error("pre-Ready boundary missed");
let early = "returned";
try { new BrowserWindow({ width: 320, height: 240 }); } catch (error) { early = code(error); }
report(`PRE_READY ${early} frames=${framesOnWindowChannel} windows=${BrowserWindow.getAllWindows().length}`);
let quit = "accepted";
try { await app.quit(); } catch (error) { quit = code(error); }
report(`PREQUIT ${quit}`);
if (quit !== "KELD-COMPAT-012") throw new Error("late package import bypassed the native Quit barrier");
await app.whenReady();
report("READY");
let late = "returned";
try { new BrowserWindow({ width: 320, height: 240 }); } catch (error) { late = code(error); }
report(`POST_READY ${late} frames=${framesOnWindowChannel} windows=${BrowserWindow.getAllWindows().length}`);
for await (const command of createInterface({ input: control })) {
  if (command !== "QUIT") throw new Error(`unknown control request ${command}`);
  await app.quit();
  report("QUIT_ACCEPTED");
  break;
}
control.end();
} catch (error) {
  report(`ROLE_ERROR ${String(error)}`);
  throw error;
}
"#;

/// The fixture's first module: replaces `Reflect.apply`, which the transport captures
/// at module load, with a recorder of the calls made from the transport.
const CAPTURE_ORDER_ENTRY: &str = r#"
const applyIntrinsic = Reflect.apply;
const recorded = { armed: false, transportCalls: 0 };
Reflect.set(globalThis, "__keldCaptureOrder", recorded);
Reflect.apply = function (target: Function, self: unknown, args: ArrayLike<unknown>) {
  if (recorded.armed && (new Error().stack ?? "").includes("kipc-transport")) recorded.transportCalls += 1;
  return applyIntrinsic(target, self, args);
};
await import("./capture-order-role.ts");
"#;

const CAPTURE_ORDER_ROLE: &str = r#"
import { createConnection } from "node:net";
const control = createConnection({ host: "127.0.0.1", port: Number(process.env.KELD_T1B_CONTROL) });
await new Promise<void>((resolve, reject) => { control.once("connect", resolve); control.once("error", reject); });
const report = (message: string) => control.write(`${message}\n`);
const recorded = Reflect.get(globalThis, "__keldCaptureOrder") as { armed: boolean; transportCalls: number };
try {
  report(`BOUND ${process.pid}`);
  report(`MODE ${Reflect.get(process.env, "KELD_NATIVE_WINDOW_FACADE") ?? "absent"} ${nativeFacadeBootSelected}`);
  recorded.armed = true;
  await app.whenReady();
  windowCall();
  recorded.armed = false;
  report(`CAPTURE ${recorded.transportCalls}`);
} catch (error) {
  report(`ROLE_ERROR ${String(error)}`);
  throw error;
}
"#;

/// One public role, bundled with the transport left external as the same staged
/// `./kipc-transport.ts` the host preloads, behind the recording first module.
pub(crate) fn stage_capture_order(
    fixture: &StageFixture,
    facade: bool,
) -> keld_cli::boot::DevBootStage {
    fs::write(fixture.project.join("src/main.ts"), CAPTURE_ORDER_ENTRY)
        .expect("capture-order first module");
    if facade {
        fs::write(fixture.project.join("keld.config.ts"), SCHEMA_TWO_CONFIG)
            .expect("schema-two config");
        fs::remove_file(fixture.project.join("index.html")).expect("no renderer source");
    }
    let stage = stage_boot(&fixture.project);
    let role = if facade {
        format!(
            "import {{ BrowserWindow, app }} from {};\nimport {{ nativeFacadeBootSelected }} from {};\nconst windowCall = () => {{ try {{ new BrowserWindow({{ width: 320, height: 240 }}); }} catch {{ /* answered KELD-CORE-042 */ }} }};\n{CAPTURE_ORDER_ROLE}",
            package_import("packages/@keld/electron/src/index.ts"),
            package_import("packages/@keld/kipc/src/transport.ts"),
        )
    } else {
        format!(
            "import {{ app }} from {};\nimport {{ nativeFacadeBootSelected }} from {};\nconst windowCall = () => {{}};\n{CAPTURE_ORDER_ROLE}",
            package_import("packages/@keld/api/src/app.ts"),
            package_import("packages/@keld/kipc/src/transport.ts"),
        )
    };
    bundle_public_role(
        repo_root(),
        &fixture.project,
        &role,
        &stage.root().join("src/capture-order-role.ts"),
    );
    stage
}

fn package_import(relative: &str) -> String {
    serde_json::to_string(&repo_root().join(relative).to_string_lossy())
        .expect("absolute package import")
}

pub(crate) fn junction(link: &Path, target: &Path) {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:KELD_TEST_LINK -Target $env:KELD_TEST_TARGET | Out-Null",
        ])
        .env("KELD_TEST_LINK", link)
        .env("KELD_TEST_TARGET", target)
        .output()
        .expect("junction fixture command");
    assert!(
        output.status.success(),
        "junction setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
