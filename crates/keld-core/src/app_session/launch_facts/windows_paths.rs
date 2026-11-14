//! Lexical observation paths only; no file access or authority is derived here.

pub(super) fn retained_drive_tail(value: &str) -> Option<&str> {
    value.strip_prefix(r"\\?\").and_then(drive_tail)
}

pub(super) fn retained_root(value: &str) -> bool {
    retained_drive_tail(value).is_some_and(valid_tail)
}

pub(super) fn observed_absolute(value: &str) -> bool {
    if let Some(tail) = value.strip_prefix(r"\\?\UNC\") {
        return unc_tail(tail);
    }
    if let Some(local) = value.strip_prefix(r"\\?\") {
        return drive_tail(local).is_some_and(valid_tail);
    }
    if let Some(tail) = value.strip_prefix(r"\\") {
        return unc_tail(tail);
    }
    drive_tail(value).is_some_and(valid_tail)
}

fn drive_tail(value: &str) -> Option<&str> {
    let bytes = value.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || &bytes[1..3] != b":\\" {
        return None;
    }
    Some(&value[3..])
}

fn unc_tail(value: &str) -> bool {
    let mut parts = value.split('\\');
    matches!((parts.next(), parts.next()), (Some(server), Some(share))
        if valid_component(server) && valid_component(share))
        && parts.all(valid_component)
}

fn valid_tail(value: &str) -> bool {
    value.is_empty() || value.split('\\').all(valid_component)
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value
            .chars()
            .any(|ch| ch <= '\u{1f}' || matches!(ch, '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

pub(super) fn user_data(app_data: Option<&str>, name: &str) -> Option<String> {
    // Reuse Guard's existing one-component grammar, including Windows device
    // basenames, trailing dot/space and separators. This check grants no access.
    keld_guard::validate_fs_component(name).ok()?;
    let app_data = app_data?;
    Some(if app_data.ends_with('\\') {
        format!("{app_data}{name}")
    } else {
        format!("{app_data}\\{name}")
    })
}
