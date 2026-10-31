//! The comparison base of a repository ratchet, for standalone contract checkers.
//!
//! Hosted CI binds the PR base or the previous push tip, so a ratchet never compares a
//! commit with itself. A local run defaults to the way `tools/ci_changes.sh local` reads it.

use std::env;
use std::process::Output;

/// The variable hosted CI binds, and `tools/ci_changes.sh local` reads.
pub(crate) const BASE_ENV: &str = "KELD_CI_BASE_REF";
const DEFAULT_BASE: &str = "origin/main";

/// The configured base ref. An empty value stays empty so `merge_base` refuses it.
pub(crate) fn base_ref() -> String {
    env::var(BASE_ENV).unwrap_or_else(|_| DEFAULT_BASE.to_owned())
}

/// Why no comparison base exists.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BaseError {
    /// Git itself could not run; the text is the caller's own error.
    Git(String),
    /// Git ran and the base does not resolve; the text says why.
    Unresolved(&'static str),
}

/// The merge base of `base_ref` and `HEAD`. A missing base is an error, never a pass.
pub(crate) fn merge_base(
    git: &dyn Fn(&[&str]) -> Result<Output, String>,
    base_ref: &str,
) -> Result<String, BaseError> {
    if base_ref.is_empty() || base_ref.bytes().all(|byte| byte == b'0') {
        return Err(BaseError::Unresolved(
            "empty or all-zero; a first push has no comparison base",
        ));
    }
    let commit = format!("{base_ref}^{{commit}}");
    let verify = git(&["rev-parse", "--verify", "--quiet", &commit]).map_err(BaseError::Git)?;
    if !verify.status.success() {
        return Err(BaseError::Unresolved("not a commit in this repository"));
    }
    let found = git(&["merge-base", base_ref, "HEAD"]).map_err(BaseError::Git)?;
    if !found.status.success() {
        return Err(BaseError::Unresolved("no merge base with HEAD"));
    }
    let sha = String::from_utf8_lossy(&found.stdout).trim().to_owned();
    if sha.is_empty() {
        return Err(BaseError::Unresolved("empty merge base"));
    }
    Ok(sha)
}
