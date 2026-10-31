//! Strict process-argument decoding shared by the `keld` and `keld-host` binaries
//! and keld-core's feature-gated `native_renderer_joint_main` acceptance harness.
//!
//! Native argv is not guaranteed to be Unicode: Unix argv is arbitrary bytes and
//! Windows argv may hold unpaired UTF-16 surrogates. `std::env::args()` panics on
//! such input, and lossy conversion would turn an invalid argument into a
//! different command, flag or path. All three callers therefore read
//! `std::env::args_os()` and decode it here, on every OS, refusing with one
//! registered code instead. Windows private helper roles dispatch on the raw
//! `OsString` values before this conversion.

use std::ffi::OsString;
use std::fmt;

/// A process argument that is not valid Unicode (`KELD-CLI-051`).
///
/// The message names only the argument's position, never its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonUnicodeArgument {
    index: usize,
}

impl NonUnicodeArgument {
    /// Position of the first non-Unicode argument; `0` is the program path.
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }
}

impl fmt::Display for NonUnicodeArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "KELD-CLI-051: process argument {} is not valid Unicode. \
             Pass every argument, including the program path, as valid Unicode text; \
             rename or re-quote the offending path or name.",
            self.index
        )
    }
}

impl std::error::Error for NonUnicodeArgument {}

/// Decodes every process argument as Unicode, without loss.
///
/// # Errors
///
/// Returns [`NonUnicodeArgument`] for the first argument that is not valid
/// Unicode. No argument is replaced or skipped.
pub fn unicode_args<I>(args: I) -> Result<Vec<String>, NonUnicodeArgument>
where
    I: IntoIterator<Item = OsString>,
{
    args.into_iter()
        .enumerate()
        .map(|(index, arg)| arg.into_string().map_err(|_| NonUnicodeArgument { index }))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{NonUnicodeArgument, unicode_args};

    #[cfg(windows)]
    fn non_unicode() -> OsString {
        use std::os::windows::ffi::OsStringExt as _;
        // An unpaired high surrogate: valid native Windows argv, not Unicode.
        OsString::from_wide(&[0xd800])
    }

    #[cfg(unix)]
    fn non_unicode() -> OsString {
        use std::os::unix::ffi::OsStringExt as _;
        // A lone 0xFF byte: valid Unix argv, not UTF-8.
        OsString::from_vec(vec![0xff])
    }

    #[test]
    fn unicode_arguments_pass_through_unchanged_and_in_order() {
        let args = ["keld-host", "--hello", "--title=Ünïcode ✓", ""].map(OsString::from);
        assert_eq!(
            unicode_args(args),
            Ok(vec![
                String::from("keld-host"),
                String::from("--hello"),
                String::from("--title=Ünïcode ✓"),
                String::new(),
            ])
        );
    }

    #[test]
    fn first_non_unicode_argument_is_refused_by_position() {
        let args = vec![
            OsString::from("keld"),
            OsString::from("create"),
            non_unicode(),
            non_unicode(),
        ];
        let error = unicode_args(args).expect_err("non-Unicode argv must be refused");
        assert_eq!(error.index(), 2);
        assert_eq!(error, NonUnicodeArgument { index: 2 });
    }

    #[test]
    fn non_unicode_program_path_is_refused() {
        let error = unicode_args([non_unicode()]).expect_err("argv0 is decoded too");
        assert_eq!(error.index(), 0);
    }

    #[test]
    fn refusal_names_the_registered_code_and_fix_without_echoing_input() {
        let message = NonUnicodeArgument { index: 1 }.to_string();
        assert!(
            message.starts_with("KELD-CLI-051: process argument 1 is not valid Unicode."),
            "{message}"
        );
        assert!(
            message.contains("rename or re-quote the offending path or name"),
            "{message}"
        );
        assert!(!message.contains('\u{fffd}'), "{message}");
    }
}
