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
        path: ".gitignore",
        contents: include_str!("../templates/hello/.gitignore"),
    },
];
