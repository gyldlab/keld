//! Unknown-flag rejection for Phase 2 CLI verbs.
//!
//! `keld doctor --permissions` must not succeed as a silent no-op (arch/03 §3).
//! `keld hello` extra args must fail before `run_hello_window` (tao `EventLoop::run`
//! never returns). `keld create --template` and `keld dev --watch` must not
//! scaffold or open a window as if those flags were live.

use std::fmt;

/// Code for an unknown flag on a verb that has a closed flag set.
pub const UNKNOWN_FLAG_CODE: &str = "KELD-CLI-044";

/// Fix text for unknown `keld doctor` flags.
pub const DOCTOR_UNKNOWN_FLAG_FIX: &str = "Use `keld doctor` or `keld doctor --json`. \
     `--permissions`, `--web-compat`, and `--attack` are not implemented.";

/// Fix text for extra `keld hello` arguments.
pub const HELLO_UNKNOWN_FLAG_FIX: &str =
    "`keld hello` takes no flags. Run `keld hello` with no arguments.";

/// Fix text for extra `keld create` tokens.
pub const CREATE_UNKNOWN_FLAG_FIX: &str = "Pass a single project name: `keld create <name>`. \
     `--template` is not implemented (vanilla-ts hello only).";

/// Fix text for extra `keld dev` arguments.
pub const DEV_UNKNOWN_FLAG_FIX: &str = "`keld dev` takes no flags. `--watch` and `--inspect-ipc` are not implemented. \
     Run `keld dev` with no arguments in a scaffolded project.";

/// Misuse: a flag this verb does not accept (arch/07 §7 exit 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownFlagError {
    /// CLI verb (`create`, `dev`, `doctor`, `hello`).
    pub verb: &'static str,
    /// The rejected token, including leading dashes.
    pub flag: String,
    /// Imperative next step.
    pub fix: &'static str,
}

impl UnknownFlagError {
    fn new(verb: &'static str, flag: String, fix: &'static str) -> Self {
        Self { verb, flag, fix }
    }
}

impl fmt::Display for UnknownFlagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{UNKNOWN_FLAG_CODE}: unknown {} flag `{}`. {}",
            self.verb, self.flag, self.fix
        )
    }
}

impl std::error::Error for UnknownFlagError {}

/// Parses `keld doctor` flags. The only live flag is `--json`.
///
/// # Errors
///
/// Returns [`UnknownFlagError`] (`KELD-CLI-044`) for any other token, including
/// planned-but-unimplemented `--permissions` / `--web-compat` / `--attack`.
pub fn parse_doctor_flags(args: &[String]) -> Result<bool, UnknownFlagError> {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            other => {
                return Err(UnknownFlagError::new(
                    "doctor",
                    other.to_owned(),
                    DOCTOR_UNKNOWN_FLAG_FIX,
                ));
            }
        }
    }
    Ok(json)
}

/// `keld hello` accepts no flags or positional arguments.
///
/// # Errors
///
/// Returns [`UnknownFlagError`] (`KELD-CLI-044`) when `args` is non-empty.
pub fn parse_hello_flags(args: &[String]) -> Result<(), UnknownFlagError> {
    reject_any_args("hello", HELLO_UNKNOWN_FLAG_FIX, args)
}

/// `keld dev` accepts no flags or positional arguments.
///
/// # Errors
///
/// Returns [`UnknownFlagError`] (`KELD-CLI-044`) when `args` is non-empty,
/// including planned-but-unimplemented `--watch` / `--inspect-ipc`.
pub fn parse_dev_flags(args: &[String]) -> Result<(), UnknownFlagError> {
    reject_any_args("dev", DEV_UNKNOWN_FLAG_FIX, args)
}

/// Parses `keld create` arguments: exactly one positional name, no flags.
///
/// Empty `args` is `Ok(None)` so the caller can emit `KELD-CLI-020` (missing
/// name). A token that looks like a flag, or a second positional, is
/// `KELD-CLI-044` — `--template` must not be mistaken for a project name.
///
/// # Errors
///
/// Returns [`UnknownFlagError`] (`KELD-CLI-044`) for extra tokens.
pub fn parse_create_args(args: &[String]) -> Result<Option<&str>, UnknownFlagError> {
    match args {
        [] => Ok(None),
        [name] if looks_like_flag(name) => Err(UnknownFlagError::new(
            "create",
            name.clone(),
            CREATE_UNKNOWN_FLAG_FIX,
        )),
        [name] => Ok(Some(name.as_str())),
        [_, extra, ..] => {
            let flag = args
                .iter()
                .find(|token| looks_like_flag(token))
                .unwrap_or(extra);
            Err(UnknownFlagError::new(
                "create",
                flag.clone(),
                CREATE_UNKNOWN_FLAG_FIX,
            ))
        }
    }
}

fn reject_any_args(
    verb: &'static str,
    fix: &'static str,
    args: &[String],
) -> Result<(), UnknownFlagError> {
    match args.first() {
        None => Ok(()),
        Some(flag) => Err(UnknownFlagError::new(verb, flag.clone(), fix)),
    }
}

fn looks_like_flag(token: &str) -> bool {
    token.starts_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn doctor_no_flags_is_human_mode() {
        assert!(!parse_doctor_flags(&[]).expect("empty"));
    }

    #[test]
    fn doctor_json_once_or_twice_is_ok() {
        assert!(parse_doctor_flags(&s(&["--json"])).expect("json"));
        assert!(parse_doctor_flags(&s(&["--json", "--json"])).expect("dup json"));
    }

    #[test]
    fn doctor_permissions_is_044_not_success() {
        let err = parse_doctor_flags(&s(&["--permissions"])).expect_err("must reject");
        let msg = err.to_string();
        assert!(msg.contains(UNKNOWN_FLAG_CODE), "{msg}");
        assert!(msg.contains("--permissions"), "{msg}");
        assert!(msg.contains("not implemented"), "{msg}");
        assert_eq!(err.fix, DOCTOR_UNKNOWN_FLAG_FIX);
        assert_eq!(err.verb, "doctor");
    }

    #[test]
    fn doctor_json_then_unknown_is_044() {
        let err = parse_doctor_flags(&s(&["--json", "--web-compat"])).expect_err("must reject");
        assert!(err.to_string().contains("--web-compat"), "{err}");
        assert!(err.to_string().contains(UNKNOWN_FLAG_CODE), "{err}");
    }

    #[test]
    fn doctor_attack_and_bare_token_are_044() {
        for flag in ["--attack", "--bogus", "json"] {
            let err = parse_doctor_flags(&s(&[flag])).expect_err(flag);
            let msg = err.to_string();
            assert!(msg.contains(UNKNOWN_FLAG_CODE), "{msg}");
            assert!(msg.contains(flag), "{msg}");
        }
    }

    #[test]
    fn hello_no_args_ok() {
        parse_hello_flags(&[]).expect("hello takes no flags");
    }

    #[test]
    fn hello_devtools_and_title_are_044() {
        for flag in ["--devtools", "--title", "Acme"] {
            let err = parse_hello_flags(&s(&[flag])).expect_err(flag);
            let msg = err.to_string();
            assert!(msg.contains(UNKNOWN_FLAG_CODE), "{msg}");
            assert!(msg.contains(flag), "{msg}");
            assert!(msg.contains("takes no flags"), "{msg}");
            assert_eq!(err.verb, "hello");
        }
    }

    #[test]
    fn create_one_name_is_ok() {
        assert_eq!(
            parse_create_args(&s(&["hello"])).expect("name"),
            Some("hello")
        );
        assert_eq!(parse_create_args(&[]).expect("missing"), None);
        assert_eq!(parse_create_args(&s(&[""])).expect("empty name"), Some(""));
    }

    #[test]
    fn create_template_flag_is_044_not_020_name() {
        let err = parse_create_args(&s(&["--template"])).expect_err("flag");
        let msg = err.to_string();
        assert!(msg.contains(UNKNOWN_FLAG_CODE), "{msg}");
        assert!(msg.contains("--template"), "{msg}");
        assert!(msg.contains("not implemented"), "{msg}");
        assert!(msg.contains("vanilla-ts"), "{msg}");
        assert_eq!(err.verb, "create");
        assert_eq!(err.fix, CREATE_UNKNOWN_FLAG_FIX);
    }

    #[test]
    fn create_template_after_name_is_044() {
        let err = parse_create_args(&s(&["hello", "--template", "react"])).expect_err("extra");
        let msg = err.to_string();
        assert!(msg.contains(UNKNOWN_FLAG_CODE), "{msg}");
        assert!(msg.contains("--template"), "{msg}");
        assert!(!msg.contains("`hello`"), "{msg}");
    }

    #[test]
    fn create_extra_positional_is_044() {
        let err = parse_create_args(&s(&["hello", "world"])).expect_err("extra");
        let msg = err.to_string();
        assert!(msg.contains(UNKNOWN_FLAG_CODE), "{msg}");
        assert!(msg.contains("`world`"), "{msg}");
        assert_eq!(err.verb, "create");
    }

    #[test]
    fn dev_watch_and_inspect_are_044() {
        for flag in ["--watch", "--inspect-ipc", "--devtools", "extra"] {
            let err = parse_dev_flags(&s(&[flag])).expect_err(flag);
            let msg = err.to_string();
            assert!(msg.contains(UNKNOWN_FLAG_CODE), "{msg}");
            assert!(msg.contains(flag), "{msg}");
            assert!(msg.contains("takes no flags"), "{msg}");
            assert!(msg.contains("--watch"), "{msg}");
            assert_eq!(err.verb, "dev");
            assert_eq!(err.fix, DEV_UNKNOWN_FLAG_FIX);
        }
    }

    #[test]
    fn dev_no_args_ok() {
        parse_dev_flags(&[]).expect("dev takes no flags");
    }
}
