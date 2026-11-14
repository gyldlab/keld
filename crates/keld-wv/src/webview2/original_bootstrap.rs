//! Fixed document-start bootstrap for the Host-qualified original image.
//! One emitter supplies counted UTF8 tests and counted native UTF16 storage.

use crate::WvError;
use crate::preload_image::{AppPreloadImage, preload_error};

mod sinks;
use sinks::{Measured, NativeWide, write_json};

const RUNTIME: &str = include_str!("../wkwebview/macos_preload_runtime.generated.js");
const PREFIX: &str = "(() => {\n\"use strict\";\nif(window.top!==window)return;\n";
const CAPTURE: &str = "\nconst runtime=globalThis.__keldOriginalPreloadRuntime; delete globalThis.__keldOriginalPreloadRuntime;\nruntime.installWindows(";
const FUNCTION: &str =
    ",function(require,process,exports,module,Buffer,global,setImmediate,clearImmediate){\n";
const SUFFIX: &str = "\n});\n})();";

fn emit(image: &AppPreloadImage, output: &mut impl std::fmt::Write) -> Result<(), WvError> {
    #[derive(serde::Serialize)]
    struct Facts<'a> {
        argv: &'a [String],
        versions: &'a std::collections::BTreeMap<String, String>,
    }
    let write_failed = || preload_error("counted bootstrap rendering refused");
    output.write_str(PREFIX).map_err(|_| write_failed())?;
    output.write_str(RUNTIME).map_err(|_| write_failed())?;
    output.write_str(CAPTURE).map_err(|_| write_failed())?;
    write_json(
        output,
        &Facts {
            argv: image.additional_arguments(),
            versions: image.versions(),
        },
    )
    .map_err(|()| write_failed())?;
    output.write_str(FUNCTION).map_err(|_| write_failed())?;
    output
        .write_str(image.source())
        .map_err(|_| write_failed())?;
    output.write_str(SUFFIX).map_err(|_| write_failed())
}

fn measure(image: &AppPreloadImage) -> Result<Measured, WvError> {
    let mut measured = Measured::default();
    emit(image, &mut measured)?;
    Ok(measured)
}

/// One fixed native registration input; this is not a CONTROL carrier or an
/// addition to the immutable source-image quota. The native View retains this
/// separately counted allocation through `AddScript` completion or actual Close.
pub(super) struct NativeBootstrapText {
    wide: Vec<u16>,
    #[cfg(test)]
    utf8_bytes: usize,
}
impl NativeBootstrapText {
    pub(super) fn as_nul_terminated_utf16(&self) -> &[u16] {
        &self.wide
    }
    #[cfg(test)]
    pub(super) fn allocated_bytes(&self) -> usize {
        self.wide.capacity() * std::mem::size_of::<u16>()
    }
    #[cfg(test)]
    pub(super) fn emitted_utf8_bytes(&self) -> usize {
        self.utf8_bytes
    }
}

/// Count exact immutable inputs before allocating once; the native path never
/// creates an intermediate rendered UTF8 String or serialized-facts buffer.
pub(super) fn render_native(image: &AppPreloadImage) -> Result<NativeBootstrapText, WvError> {
    let measured = measure(image)?;
    let mut output = NativeWide::reserve(measured.utf16_units)
        .map_err(|()| preload_error("native bootstrap storage reservation refused"))?;
    emit(image, &mut output)?;
    let wide = output
        .finish()
        .map_err(|()| preload_error("native bootstrap capacity changed after reservation"))?;
    Ok(NativeBootstrapText {
        wide,
        #[cfg(test)]
        utf8_bytes: measured.utf8_bytes,
    })
}

/// UTF8 output is only needed by the executable composition oracle.
#[cfg(test)]
pub(super) fn render(image: &AppPreloadImage) -> Result<String, WvError> {
    let measured = measure(image)?;
    let mut output = String::new();
    output
        .try_reserve_exact(measured.utf8_bytes)
        .map_err(|_| preload_error("bootstrap UTF8 test storage reservation refused"))?;
    emit(image, &mut output)?;
    if output.len() != measured.utf8_bytes || output.capacity() > measured.utf8_bytes {
        return Err(preload_error("bootstrap UTF8 test capacity changed"));
    }
    Ok(output)
}

#[cfg(test)]
#[allow(clippy::expect_used)] // Executable contract assertions own their fixture failures.
mod tests {
    use super::render;
    use crate::preload_image::AppPreloadBudget;
    use sha2::{Digest as _, Sha256};
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn emitted_bootstrap_guards_frames_and_preserves_exact_image_wrapper() {
        let original = "// exact authenticated bytes: ü\r\nglobalThis.body = [require,process,exports,module,Buffer,global,setImmediate,clearImmediate].map(value => value.slot);";
        let argv = vec![
            String::from("--path=D:\\quoted ' ü"),
            String::from("--line=one\ntwo"),
        ];
        let versions = std::collections::BTreeMap::from([
            (String::from("keld"), String::from("captured-host")),
            (String::from("bun"), String::from("captured-runtime")),
        ]);
        let image = AppPreloadBudget::default()
            .reserve_window()
            .expect("fixture image budget")
            .from_qualified_graph(
                original.as_bytes().to_vec(),
                Sha256::digest(original.as_bytes()).into(),
                argv.clone(),
                versions.clone(),
            )
            .expect("exact qualified fixture bytes");
        let source = render(&image).expect("render actual immutable image");
        let input = serde_json::json!({
            "source": source, "original": original, "argv": argv, "versions": versions,
        });
        // Replace only the factory consumer in this composition test. The real
        // generated factory still executes; canonical runtime/transport behavior
        // has its own executable Windows suite and native delivery qualification.
        let program = format!(
            r#"
const assert = require("node:assert/strict");
const vm = require("node:vm");
const input = {input};
for (const main of [false, true]) {{
  const context = vm.createContext({{ TextEncoder, TextDecoder, console, main }});
  vm.runInContext(`
    globalThis.window = globalThis;
    window.top = main ? window : {{}};
    globalThis.factories = 0;
    globalThis.installs = 0;
    const originalDefine = Object.defineProperty;
    Object.defineProperty = function(target, name, descriptor) {{
      if (name === "__keldOriginalPreloadRuntime") {{
        ++globalThis.factories;
        descriptor = {{ ...descriptor, value: {{
          installWindows(facts, original) {{
            ++globalThis.installs;
            globalThis.facts = facts;
            globalThis.functionSource = Function.prototype.toString.call(original);
            original(...Array.from({{ length: 8 }}, (_, slot) => ({{ slot }})));
          }}
        }} }};
      }}
      return originalDefine(target, name, descriptor);
    }};
  `, context);
  vm.runInContext(input.source, context, {{ timeout: 1000 }});
  assert.equal(context.factories, main ? 1 : 0);
  assert.equal(context.installs, main ? 1 : 0);
  assert.equal(Object.hasOwn(context, "__keldOriginalPreloadRuntime"), false);
  if (main) {{
    assert.deepEqual(JSON.parse(JSON.stringify(context.facts)), {{
      argv: input.argv, versions: input.versions
    }});
    assert.deepEqual(Array.from(context.body), [0,1,2,3,4,5,6,7]);
    assert.ok(context.functionSource.includes("\n" + input.original + "\n"));
  }} else {{
    assert.equal(context.body, undefined);
    assert.equal(context.facts, undefined);
  }}
}}
console.log("exact bootstrap wrapper and main-frame guard passed");
"#
        );
        run_bun(&program);
    }

    #[test]
    fn emitted_windows_numeric_runtime_owns_its_canonical_channel() {
        let original: String = serde_json::from_str(include_str!(
            "../../tests/fixtures/drawio-original-preload.json"
        ))
        .expect("existing unchanged original preload fixture");
        let source = render(&native_fixture(&original)).expect("actual generated runtime wrapper");
        let input = serde_json::json!({
            "source": source,
            "transport": concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../packages/@keld/kipc/src/transport.ts"
            ),
        });
        // Execute the real factory and installWindows, without the build symbol
        // in the isolated context. The outer Bun process reads the canonical
        // transport constant, independently of the generator's substituted value.
        let program = format!(
            r#"
const assert = require("node:assert/strict");
const vm = require("node:vm");
const input = {input};
const {{ ECHO_CHANNEL }} = require(input.transport);
process.exitCode = 1;
(async () => {{
  const context = vm.createContext({{
    TextEncoder, TextDecoder, EventTarget, CustomEvent, Event, URL, URLSearchParams, console
  }});
  assert.equal(Object.hasOwn(context, "__KELD_ECHO_CHANNEL__"), false);
  vm.runInContext(`
    globalThis.window = globalThis;
    window.top = window;
    window.document = new EventTarget();
    const lifecycle = new EventTarget();
    window.addEventListener = lifecycle.addEventListener.bind(lifecycle);
    const stored = new Map();
    window.localStorage = {{
      getItem: key => stored.get(key) ?? null,
      setItem: (key, value) => stored.set(key, value)
    }};
    globalThis.posted = [];
    const listeners = new Set();
    const port = {{
      postMessage: text => posted.push(JSON.parse(text)),
      addEventListener: (_name, fn) => listeners.add(fn),
      removeEventListener: (_name, fn) => listeners.delete(fn)
    }};
    window.chrome = {{ webview: port }};
    globalThis.receive = value => {{
      for (const fn of listeners)
        fn({{ data: JSON.stringify(value), source: port }});
    }};
  `, context);
  vm.runInContext(input.source, context, {{ timeout: 1000 }});
  assert.equal(Object.hasOwn(context, "__keldOriginalPreloadRuntime"), false);
  assert.equal(Object.hasOwn(context, "__KELD_ECHO_CHANNEL__"), false);
  assert.deepEqual(JSON.parse(JSON.stringify(context.posted)), [{{ v: 2, kind: "bind" }}]);
  const pending = vm.runInContext(
    `window.keld.invoke(${{ECHO_CHANNEL}}, new Uint8Array([4, 5]))`, context
  );
  const observed = Promise.resolve(pending).then(
    value => ({{ value: Array.from(value) }}),
    error => ({{ error: String(error) }})
  );
  const document = "0123456789abcdef0123456789abcdef";
  context.receive({{ v: 2, kind: "bound", document, ready: true }});
  const calls = JSON.parse(JSON.stringify(context.posted)).filter(row => row.kind === "invoke");
  assert.deepEqual(calls, [{{
    v: 2, kind: "invoke", document, request: 1,
    channel: ECHO_CHANNEL, payload: [4, 5]
  }}]);
  context.receive({{ v: 2, kind: "reply", document, request: 1, ok: true, payload: [7, 6] }});
  assert.deepEqual(await observed, {{ value: [7, 6] }});
  console.log("generated numeric channel and reply passed without ambient build input");
  process.exitCode = 0;
}})().catch(error => {{ console.error(error); process.exitCode = 1; }});
"#
        );
        run_bun(&program);
    }

    fn native_fixture(source: &str) -> crate::preload_image::AppPreloadImage {
        AppPreloadBudget::default()
            .reserve_window()
            .expect("fixture source capacity")
            .from_qualified_graph(
                source.as_bytes().to_vec(),
                Sha256::digest(source.as_bytes()).into(),
                vec![String::from("--quoted=\"\u{1f680}\n")],
                std::collections::BTreeMap::from([(
                    String::from("keld"),
                    String::from("fixture-\u{3bb}"),
                )]),
            )
            .expect("same admitted fixture image")
    }

    #[test]
    fn counted_native_render_is_exact_utf16_without_a_second_rendered_utf8_buffer() {
        let image =
            native_fixture("// exact original \u{1f680}\r\nglobalThis.fixture = '\u{3bb}';");
        let utf8 = render(&image).expect("same canonical emitter oracle");
        let native = super::render_native(&image).expect("count once then native allocation");
        let expected: Vec<u16> = utf8.encode_utf16().chain(std::iter::once(0)).collect();
        assert_eq!(native.as_nul_terminated_utf16(), expected);
        assert_eq!(native.emitted_utf8_bytes(), utf8.len());
        assert_eq!(native.allocated_bytes(), expected.len() * 2);
    }

    #[test]
    fn admitted_nul_in_original_source_cannot_truncate_native_registration() {
        let image = native_fixture("globalThis.before=1;\0globalThis.after=2;");
        assert!(super::render_native(&image).is_err());
        assert!(render(&image).is_err());
    }

    fn run_bun(program: &str) {
        let mut child = Command::new("bun")
            .args([
                "-e",
                "require('node:vm').runInThisContext(require('node:fs').readFileSync(0,'utf8'))",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("workspace Bun for executable bootstrap contract");
        child
            .stdin
            .take()
            .expect("owned Bun input")
            .write_all(program.as_bytes())
            .expect("bounded bootstrap fixture input");
        let started = Instant::now();
        let mut timed_out = false;
        while child.try_wait().expect("observe exact Bun child").is_none() {
            if started.elapsed() > Duration::from_secs(15) {
                child.kill().expect("stop timed-out owned Bun child");
                timed_out = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let stopped = Instant::now();
        while child.try_wait().expect("reap exact Bun child").is_none() {
            assert!(
                stopped.elapsed() < Duration::from_secs(2),
                "owned Bun child did not stop"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child
            .wait_with_output()
            .expect("reap completed Bun fixture");
        assert!(!timed_out, "bootstrap fixture exceeded its deadline");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
