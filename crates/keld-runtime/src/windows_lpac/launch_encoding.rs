//! Windows suspended-launch command-line and environment admission.
//!
//! Both zero-capability LPAC and same-token suspended launches consume these
//! exact UTF-16 encoders. Process creation, access tokens, handle inheritance,
//! and all Windows authority decisions remain with the parent.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStrExt as _;

use super::WindowsLpacError;

pub(super) fn encode_command_line(
    program: &OsStr,
    args: &[OsString],
) -> Result<Vec<u16>, WindowsLpacError> {
    let mut encoded = Vec::new();
    append_quoted_arg(&mut encoded, program)?;
    for arg in args {
        encoded.push(u16::from(b' '));
        append_quoted_arg(&mut encoded, arg)?;
    }
    encoded.push(0);
    Ok(encoded)
}

fn append_quoted_arg(output: &mut Vec<u16>, arg: &OsStr) -> Result<(), WindowsLpacError> {
    let units: Vec<u16> = arg.encode_wide().collect();
    if units.contains(&0) {
        return Err(WindowsLpacError::contract(
            "command-line encoding",
            "argument contains NUL",
        ));
    }
    let needs_quotes =
        units.is_empty() || units.iter().any(|unit| matches!(*unit, 0x20 | 0x09 | 0x22));
    if !needs_quotes {
        output.extend(units);
        return Ok(());
    }

    output.push(u16::from(b'"'));
    let mut slashes = 0_usize;
    for unit in units {
        if unit == u16::from(b'\\') {
            slashes += 1;
            continue;
        }
        if unit == u16::from(b'"') {
            output.extend(std::iter::repeat_n(u16::from(b'\\'), slashes * 2 + 1));
        } else {
            output.extend(std::iter::repeat_n(u16::from(b'\\'), slashes));
        }
        slashes = 0;
        output.push(unit);
    }
    output.extend(std::iter::repeat_n(u16::from(b'\\'), slashes * 2));
    output.push(u16::from(b'"'));
    Ok(())
}

pub(super) fn encode_environment(
    environment: &[(OsString, OsString)],
) -> Result<Vec<u16>, WindowsLpacError> {
    let mut ordered = Vec::with_capacity(environment.len());
    let mut keys = BTreeSet::new();
    for (key, value) in environment {
        let key_units: Vec<u16> = key.encode_wide().collect();
        let value_units: Vec<u16> = value.encode_wide().collect();
        if key_units.is_empty()
            || key_units.contains(&0)
            || key_units.contains(&u16::from(b'='))
            || value_units.contains(&0)
        {
            return Err(WindowsLpacError::contract(
                "environment encoding",
                "key is empty/contains '=' or key/value contains NUL",
            ));
        }
        let folded = key.to_string_lossy().to_uppercase();
        if !keys.insert(folded.clone()) {
            return Err(WindowsLpacError::contract(
                "environment encoding",
                format!("duplicate case-insensitive key {folded}"),
            ));
        }
        ordered.push((folded, key_units, value_units));
    }
    ordered.sort_by(|left, right| left.0.cmp(&right.0));

    let mut encoded = Vec::new();
    for (_, key, value) in ordered {
        encoded.extend(key);
        encoded.push(u16::from(b'='));
        encoded.extend(value);
        encoded.push(0);
    }
    encoded.push(0);
    if environment.is_empty() {
        encoded.push(0);
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};

    use super::{encode_command_line, encode_environment};

    #[test]
    fn command_line_quotes_exact_windows_arguments() {
        let args = [
            OsString::from("bare"),
            OsString::from("two words"),
            OsString::from("a\"b"),
            OsString::from(""),
        ];
        let encoded =
            encode_command_line(OsStr::new("program.exe"), &args).expect("valid command line");
        let expected: Vec<u16> = "program.exe bare \"two words\" \"a\\\"b\" \"\"\0"
            .encode_utf16()
            .collect();
        assert_eq!(encoded, expected, "literal Windows UTF-16 launch bytes");
    }

    #[test]
    fn command_line_doubles_terminal_backslash_inside_quotes() {
        let encoded = encode_command_line(
            OsStr::new("program.exe"),
            &[OsString::from("folder with space\\")],
        )
        .expect("quoted trailing slash");
        let expected: Vec<u16> = "program.exe \"folder with space\\\\\"\0"
            .encode_utf16()
            .collect();
        assert_eq!(
            encoded, expected,
            "two slashes must precede the closing quote"
        );
    }

    #[test]
    fn command_line_refuses_embedded_nul() {
        for text in ["a\0b", "\0", "value\0"] {
            assert!(
                encode_command_line(OsStr::new("program.exe"), &[OsString::from(text)]).is_err(),
                "interior NUL must not truncate a Win32 command line"
            );
        }
        assert!(
            encode_command_line(OsStr::new("bad\0.exe"), &[]).is_err(),
            "program NUL must also be rejected"
        );
    }

    #[test]
    fn environment_encodes_sorted_pairs_with_double_nul() {
        let entries = [
            (OsString::from("Z_KEY"), OsString::from("last")),
            (OsString::from("A_KEY"), OsString::from("first")),
        ];
        let actual = encode_environment(&entries).expect("valid environment");
        let expected: Vec<u16> = "A_KEY=first\0Z_KEY=last\0\0".encode_utf16().collect();
        assert_eq!(actual, expected, "sorted literal UTF-16 environment block");
        assert_eq!(
            encode_environment(&[]).expect("empty environment"),
            vec![0, 0],
            "an empty environment still ends in two NULs"
        );
    }

    #[test]
    fn environment_refuses_duplicate_or_malformed_keys_and_values() {
        assert!(
            encode_environment(&[
                (OsString::from("Path"), OsString::from("one")),
                (OsString::from("PATH"), OsString::from("two")),
            ])
            .is_err(),
            "case-insensitive duplicates must not reach the child"
        );
        for (key, value) in [
            ("", "anything"),
            ("A=B", "anything"),
            ("A\0B", "anything"),
            ("VALID", "with\0nul"),
        ] {
            assert!(
                encode_environment(&[(OsString::from(key), OsString::from(value))]).is_err(),
                "invalid key/value {key:?} {value:?} must be refused"
            );
        }
    }
}
