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
        contents: include_str!("../templates/hello/src/main.ts"),
    },
    TemplateFile {
        path: "src/kipc.ts",
        contents: include_str!("../templates/hello/src/kipc.ts"),
    },
    TemplateFile {
        path: ".gitignore",
        contents: include_str!("../templates/hello/.gitignore"),
    },
];

#[cfg(test)]
mod tests {
    use super::HELLO_TEMPLATE;

    /// `kipc.test.ts` is scaffold-internal test coverage, not app code a
    /// created project should carry. `HELLO_TEMPLATE` is an explicit
    /// allow-list (not a directory glob) specifically so files like this can
    /// live beside `kipc.ts` without shipping.
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
}
