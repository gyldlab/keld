//! Reserved and unknown `keld` verbs (KEL-29 / arch/07 §2).
//!
//! README and spec 06 name `build` / `migrate` / `gen` / `ext` before they are
//! live. A bare `unknown command` with exit 1 lets agents think the verb does
//! not exist. Reserved verbs are `KELD-CLI-045` (exit 2) with a tracking issue
//! and the Phase 2 workaround. Garbage verbs are `KELD-CLI-046` (exit 2).

use std::fmt;

/// Code for a spec-named verb that is not implemented in this binary.
pub const RESERVED_VERB_CODE: &str = "KELD-CLI-045";

/// Code for a token that is not a live or reserved verb.
pub const UNKNOWN_VERB_CODE: &str = "KELD-CLI-046";

/// Live `keld` verbs in dispatch order (usage text / unknown-verb fix).
pub const LIVE_VERBS: &str = "create, dev, doctor, mcp, hello, ipc-echo, ipc-client";

/// Spec 06 verbs that exist in the product vocabulary but are not in this binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservedVerb {
    /// `keld build` — packaging (`keld-pack`).
    Build,
    /// `keld migrate` — Electron analyzer + compat config.
    Migrate,
    /// `keld gen` — schema → TS/Rust codegen.
    Gen,
    /// `keld ext` — plugin scaffolding.
    Ext,
}

impl ReservedVerb {
    /// Parses a reserved verb token. Live verbs are not reserved.
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "build" => Some(Self::Build),
            "migrate" => Some(Self::Migrate),
            "gen" => Some(Self::Gen),
            "ext" => Some(Self::Ext),
            _ => None,
        }
    }

    /// Canonical CLI token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Migrate => "migrate",
            Self::Gen => "gen",
            Self::Ext => "ext",
        }
    }

    /// Linear issue that owns the missing implementation.
    #[must_use]
    pub const fn tracking(self) -> &'static str {
        match self {
            Self::Build | Self::Ext => "KEL-19",
            Self::Migrate => "KEL-17",
            Self::Gen => "KEL-13",
        }
    }

    /// Imperative next step for Phase 2.
    #[must_use]
    pub const fn fix(self) -> &'static str {
        match self {
            Self::Build => {
                "Packaging is not in Phase 2 (KEL-19 / keld-pack). \
                 Use `keld create <name>` then `keld dev`."
            }
            Self::Migrate => {
                "Electron migrate is not in Phase 2 (KEL-17). \
                 Scaffold a hello app with `keld create <name>` then `keld dev`."
            }
            Self::Gen => {
                "Schema codegen is parked (KEL-13 / KEL-30). \
                 The echo demo uses in-process postcard types."
            }
            Self::Ext => {
                "Plugin scaffolding is not in Phase 2 (KEL-19). \
                 Use `keld create <name>`."
            }
        }
    }
}

/// Misuse: a reserved verb that this binary does not implement (arch/07 §7 exit 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservedVerbError {
    /// The reserved verb.
    pub verb: ReservedVerb,
}

impl fmt::Display for ReservedVerbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{RESERVED_VERB_CODE}: `keld {}` is not implemented. {}",
            self.verb.as_str(),
            self.verb.fix()
        )
    }
}

impl std::error::Error for ReservedVerbError {}

/// Misuse: a token that is neither live nor reserved (arch/07 §7 exit 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownVerbError {
    /// The rejected token.
    pub verb: String,
}

impl fmt::Display for UnknownVerbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{UNKNOWN_VERB_CODE}: unknown command `{}`. \
             Live verbs: {LIVE_VERBS}.",
            self.verb
        )
    }
}

impl std::error::Error for UnknownVerbError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_tokens_are_045_with_tracking_and_phase2_fix() {
        for token in ["build", "migrate", "gen", "ext"] {
            let verb = ReservedVerb::parse(token).expect(token);
            assert_eq!(verb.as_str(), token);
            let msg = ReservedVerbError { verb }.to_string();
            assert!(msg.contains(RESERVED_VERB_CODE), "{msg}");
            assert!(msg.contains(&format!("keld {token}")), "{msg}");
            assert!(msg.contains("not implemented"), "{msg}");
            assert!(msg.contains(verb.tracking()), "{msg}");
            assert!(
                msg.contains("keld create") || msg.contains("postcard"),
                "{msg}"
            );
        }
    }

    #[test]
    fn live_verbs_are_not_reserved() {
        for token in LIVE_VERBS.split(", ") {
            assert_eq!(ReservedVerb::parse(token), None, "{token}");
        }
        assert_eq!(ReservedVerb::parse("--version"), None);
        assert_eq!(ReservedVerb::parse("Create"), None);
        assert_eq!(ReservedVerb::parse("Migrate"), None);
    }

    #[test]
    fn unknown_verb_is_046_and_lists_live_verbs() {
        let err = UnknownVerbError {
            verb: "bogus".to_owned(),
        };
        let msg = err.to_string();
        assert!(msg.contains(UNKNOWN_VERB_CODE), "{msg}");
        assert!(msg.contains("`bogus`"), "{msg}");
        assert!(msg.contains("create"), "{msg}");
        assert!(msg.contains("ipc-client"), "{msg}");
        assert!(!msg.contains(RESERVED_VERB_CODE), "{msg}");
    }

    #[test]
    fn migrate_fix_names_kel17_not_packaging() {
        let msg = ReservedVerbError {
            verb: ReservedVerb::Migrate,
        }
        .to_string();
        assert!(msg.contains("KEL-17"), "{msg}");
        assert!(!msg.contains("keld-pack"), "{msg}");
    }
}
