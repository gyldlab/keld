//! Fixed document-start bootstrap for the Host-qualified original image.
//! Native installation owns source/document admission; this is pure rendering.

use crate::WvError;
use crate::preload_image::{AppPreloadImage, preload_error};

const RUNTIME: &str = include_str!("../wkwebview/macos_preload_runtime.generated.js");

/// Render only the immutable image and canonical Host facts already admitted by Core.
/// The top-frame guard precedes even factory publication. No path is reopened and
/// the exact source is the body of the same eight-argument original wrapper as macOS.
pub(super) fn render(image: &AppPreloadImage) -> Result<String, WvError> {
    #[derive(serde::Serialize)]
    struct Facts<'a> {
        argv: &'a [String],
        versions: &'a std::collections::BTreeMap<String, String>,
    }
    let facts = serde_json::to_string(&Facts {
        argv: image.additional_arguments(),
        versions: image.versions(),
    })
    .map_err(|_| preload_error("canonical process facts serialization refused"))?;
    Ok(format!(
        "(() => {{\n\"use strict\";\nif(window.top!==window)return;\n{RUNTIME}\nconst runtime=globalThis.__keldOriginalPreloadRuntime; delete globalThis.__keldOriginalPreloadRuntime;\nruntime.installWindows({facts},function(require,process,exports,module,Buffer,global,setImmediate,clearImmediate){{\n{}\n}});\n}})();",
        image.source()
    ))
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
