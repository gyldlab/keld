//! Embedded hello-world template (KEL-29).

use sha2::{Digest, Sha256};

/// Files written by `keld create`.
#[derive(Debug)]
pub struct TemplateFile {
    /// Relative path within the project directory.
    pub path: &'static str,
    /// File contents (`{{name}}` replaced with the project name).
    pub contents: &'static str,
}

/// The canonical transport's build-time declaration (#528 T3).
pub const KIPC_RELEASE_DECLARATION: &str = "declare const KELD_KIPC_RELEASE: boolean | undefined;";

/// What a created app's `src/kipc-transport.ts` carries instead: the release
/// constant defined, so no created app can reach the `WorkerLink` test hooks;
/// they stay only in the in-repo canonical file.
pub const KIPC_RELEASE_DEFINITION: &str = "const KELD_KIPC_RELEASE: boolean | undefined = true;";

/// The transport stamp's line prefix (#653). It mirrors `TRANSPORT_STAMP_PREFIX`
/// in `packages/@keld/kipc/src/transport.ts`, which owns the format and the
/// check; [`restamp_transport`] reproducing the canonical file's own stamp is
/// the test that the two agree.
pub const TRANSPORT_STAMP_PREFIX: &str = "// @keld/kipc-transport sha256:";

/// Returns `source` behind a fresh transport stamp: its first line replaced
/// (or, if it has no stamp, prefixed) by [`TRANSPORT_STAMP_PREFIX`] and the
/// lowercase hex SHA-256 of every byte after that line.
#[must_use]
pub fn restamp_transport(source: &str) -> String {
    let unstamped = match source.strip_prefix(TRANSPORT_STAMP_PREFIX) {
        Some(rest) => rest.split_once('\n').map_or("", |(_, body)| body),
        None => source,
    };
    let digest = Sha256::digest(unstamped.as_bytes());
    format!("{TRANSPORT_STAMP_PREFIX}{digest:x}\n{unstamped}")
}

impl TemplateFile {
    /// The file as `keld create` writes it for project `name`: `{{name}}`
    /// substituted, and the transport with [`KIPC_RELEASE_DEFINITION`] in
    /// place of [`KIPC_RELEASE_DECLARATION`], restamped over its new bytes.
    #[must_use]
    pub fn render(&self, name: &str) -> String {
        let rendered = self.contents.replace("{{name}}", name);
        if self.path == "src/kipc-transport.ts" {
            restamp_transport(&rendered.replacen(
                KIPC_RELEASE_DECLARATION,
                KIPC_RELEASE_DEFINITION,
                1,
            ))
        } else {
            rendered
        }
    }
}

/// All template files for the vanilla hello project.
pub const HELLO_TEMPLATE: &[TemplateFile] = &[
    TemplateFile {
        path: "keld.config.ts",
        contents: include_str!("../templates/hello/keld.config.ts"),
    },
    TemplateFile {
        path: "package.json",
        contents: include_str!("../templates/hello/package.json"),
    },
    TemplateFile {
        path: "index.html",
        contents: include_str!("../templates/hello/index.html"),
    },
    TemplateFile {
        path: "src/kipc-transport.ts",
        contents: include_str!("../../../packages/@keld/kipc/src/transport.ts"),
    },
    TemplateFile {
        path: "src/echo.generated.ts",
        contents: include_str!("../templates/hello/src/echo.generated.ts"),
    },
    TemplateFile {
        path: "src/main.ts",
        contents: concat!(
            include_str!("../templates/hello/src/kipc.ts"),
            "\n",
            include_str!("../templates/hello/src/main-body.ts")
        ),
    },
    TemplateFile {
        path: "src/kipc.ts",
        contents: include_str!("../templates/hello/src/kipc-compat.ts"),
    },
    TemplateFile {
        path: ".gitignore",
        contents: include_str!("../templates/hello/.gitignore"),
    },
];

#[cfg(test)]
mod tests {
    use super::HELLO_TEMPLATE;

    /// `kipc.test.ts`, the in-repo transport shim, and `main-body.ts` are
    /// scaffold-internal sources, not separately copied app files.
    /// `HELLO_TEMPLATE` is an explicit allow-list (not a directory glob) so
    /// the echo adapter and app body can be composed into one staged entry,
    /// the canonical transport is embedded as `src/kipc-transport.ts`, and the
    /// historical `src/kipc.ts` import path remains a tiny re-export facade.
    #[test]
    fn template_does_not_embed_test_files() {
        for file in HELLO_TEMPLATE {
            assert!(
                !file.path.ends_with(".test.ts"),
                "KEL-30: {} must not be embedded in keld create's scaffold output",
                file.path
            );
        }
    }

    /// KEL-71: `node:fs` (or bare `fs`) is Bun's own filesystem API, not
    /// Keld's — a scaffolded app that wants host-brokered, guard-checked
    /// file I/O uses `keld_ipc`'s `fs.read`/`fs.write` channel
    /// (`keld_native::fs`), not `node:fs` directly. Negative control: an app
    /// author (or a future template edit) adding `import ... from "node:fs"`
    /// / `require("fs")` to the template makes this fail immediately.
    #[test]
    fn template_never_imports_node_fs() {
        let banned = [
            "node:fs",
            "require(\"fs\")",
            "require('fs')",
            "from \"fs\"",
            "from 'fs'",
        ];
        for file in HELLO_TEMPLATE {
            for needle in banned {
                assert!(
                    !file.contents.contains(needle),
                    "KEL-71: {} must not use Bun's node:fs ({needle}) — use the host-brokered \
                     fs.read/fs.write kipc channel instead",
                    file.path
                );
            }
        }
    }

    #[test]
    fn template_embeds_canonical_transport_not_the_in_repo_shim() {
        let file = HELLO_TEMPLATE
            .iter()
            .find(|file| file.path == "src/kipc-transport.ts")
            .expect("keld create must emit the canonical transport");
        assert_eq!(
            file.contents,
            include_str!("../../../packages/@keld/kipc/src/transport.ts"),
            "embedded transport must be the one @keld/kipc source, not a second copy"
        );
        assert!(
            file.contents.contains("export class FrameReader"),
            "generated app must receive the transport implementation"
        );
        let shim = include_str!("../templates/hello/src/kipc-transport.ts");
        assert!(
            shim.contains("packages/@keld/kipc/src/transport.ts"),
            "in-repo hello tests re-export the canonical file"
        );
        assert!(
            !shim.contains("export class FrameReader"),
            "the in-repo shim must not be a second FrameReader"
        );
    }

    /// #528 T3: a created app's transport defines the release constant, so its
    /// test-hook branches are dead; the rendering depends on the canonical
    /// file declaring the constant exactly once (a missed rename fails here).
    #[test]
    fn created_transport_defines_the_release_constant() {
        use super::{KIPC_RELEASE_DECLARATION, KIPC_RELEASE_DEFINITION};

        let file = HELLO_TEMPLATE
            .iter()
            .find(|file| file.path == "src/kipc-transport.ts")
            .expect("keld create must emit the canonical transport");
        assert_eq!(file.contents.matches(KIPC_RELEASE_DECLARATION).count(), 1);
        let rendered = file.render("demo");
        assert!(
            !rendered.contains(KIPC_RELEASE_DECLARATION),
            "declaration left in place"
        );
        assert_eq!(rendered.matches(KIPC_RELEASE_DEFINITION).count(), 1);
        assert_eq!(
            super::restamp_transport(&rendered.replacen(
                KIPC_RELEASE_DEFINITION,
                KIPC_RELEASE_DECLARATION,
                1
            )),
            file.contents,
            "the release constant and its restamp are the only change to the canonical transport"
        );
    }

    /// #653: the canonical transport is stamped by the TypeScript generator, and
    /// this Rust restamp reproduces that exact stamp, so the two agree on the
    /// format and the digest. A created transport is stamped over its own bytes.
    #[test]
    fn rust_restamp_matches_the_canonical_stamp_and_stamps_created_transports() {
        use super::{TRANSPORT_STAMP_PREFIX, restamp_transport};
        use sha2::{Digest, Sha256};

        let canonical = include_str!("../../../packages/@keld/kipc/src/transport.ts");
        assert!(
            canonical.starts_with(TRANSPORT_STAMP_PREFIX),
            "canonical transport is unstamped"
        );
        assert_eq!(
            restamp_transport(canonical),
            canonical,
            "Rust and TypeScript stamps disagree"
        );

        let file = HELLO_TEMPLATE
            .iter()
            .find(|file| file.path == "src/kipc-transport.ts")
            .expect("keld create must emit the canonical transport");
        let rendered = file.render("demo");
        let (line, body) = rendered.split_once('\n').expect("stamp line");
        assert_eq!(
            line,
            format!(
                "{TRANSPORT_STAMP_PREFIX}{:x}",
                Sha256::digest(body.as_bytes())
            ),
            "created transport must be stamped over its own bytes"
        );
        assert_ne!(
            rendered, canonical,
            "the created transport differs, so its stamp must too"
        );

        let unstamped = canonical.split_once('\n').expect("stamp line").1;
        assert_eq!(
            restamp_transport(unstamped),
            canonical,
            "an unstamped source gains the stamp"
        );
    }

    #[test]
    fn template_writes_eight_scaffold_files() {
        assert_eq!(
            HELLO_TEMPLATE.len(),
            8,
            "keld create emits config, package, html, transport, generated echo types, main, kipc facade, gitignore"
        );
    }

    #[test]
    fn template_main_import_requires_emitted_kipc_transport() {
        let main = HELLO_TEMPLATE
            .iter()
            .find(|file| file.path == "src/main.ts")
            .expect("hello main");
        assert!(
            main.contents.contains("from \"./kipc-transport.ts\""),
            "generated main must import the sidecar"
        );
        assert!(
            HELLO_TEMPLATE
                .iter()
                .any(|file| file.path == "src/kipc-transport.ts"),
            "keld create must emit src/kipc-transport.ts whenever main.ts imports it"
        );
    }

    #[test]
    fn template_emits_the_checked_in_generated_echo_types() {
        let generated = HELLO_TEMPLATE
            .iter()
            .find(|file| file.path == "src/echo.generated.ts")
            .expect("keld create must emit generated echo declarations");
        assert_eq!(
            generated.contents,
            include_str!("../templates/hello/src/echo.generated.ts"),
            "template output must be byte-identical to the freshness-checked artifact"
        );
        assert!(
            generated.contents.contains("export interface EchoRequest")
                && generated.contents.contains("export interface EchoResponse"),
            "generated artifact must own both echo payload declarations"
        );
        assert!(
            !generated.contents.contains("EchoClient"),
            "generated artifact must not invent a second client interface"
        );
    }
}
