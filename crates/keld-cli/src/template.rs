//! Embedded hello-world template (KEL-29).

/// Files written by `keld create`.
#[derive(Debug)]
pub struct TemplateFile {
    /// Relative path within the project directory.
    pub path: &'static str,
    /// File contents (`{{name}}` replaced with the project name).
    pub contents: &'static str,
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

    /// `kipc.test.ts`, the wire client source, and `main-body.ts` are
    /// scaffold-internal sources, not separately copied app files.
    /// `HELLO_TEMPLATE` is an explicit allow-list (not a directory glob) so
    /// the client and app body can be composed into one staged entry while the
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
}
