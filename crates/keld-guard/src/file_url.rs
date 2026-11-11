//! The single Windows file-URL grammar used by resource and native identity owners.
//! Parsed names and root spellings are data; only a retained source can grant a read.

#[cfg(windows)]
use std::path::Path;

/// First refusal in the approved W3 file-URL grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileUrlRefusal {
    /// Another URI scheme.
    NotFileScheme,
    /// A nonempty URL authority, including a network host.
    Authority,
    /// No absolute local drive-letter path.
    NoDrive,
    /// A Windows device or UNC spelling.
    DevicePrefix,
    /// Malformed encoding, invalid UTF-8, or an encoded separator/NUL.
    ForbiddenEscape,
    /// A decoded component contains a separator or NUL.
    DecodedSeparator,
    /// A dot segment or trailing directory selector.
    DotSegment,
    /// The requested drive/root does not match the admitted locator.
    OutsideRoot,
    /// A filename component violates the existing Windows namespace grammar.
    Component(String),
    /// A bounded URL or relative resource path is too long.
    TooLong,
    /// A shell-link leaf could redirect Chromium to a second object.
    ShellLink,
}

/// Equality of the requested root spelling and the retained source's locator.
#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootSpelling {
    /// Exact root code units; the drive letter may differ only in ASCII case.
    Exact,
    /// Windows ordinal root equality with another component spelling.
    CaseVariant,
}

/// A Host-admitted locator derived from a retained handle's normalized DOS path.
/// This data is never filesystem custody or permission to reopen that path.
#[cfg(windows)]
#[derive(Clone, Debug)]
pub struct AdmittedRootUrl {
    drive: u8,
    components: Vec<String>,
}

/// Canonical, bounded resource components validated by the one Windows grammar.
#[cfg(windows)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootRelativeComponents(Vec<String>);

/// Parsed native URL data. The raw query is preserved and grants no identity.
#[cfg(windows)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedFileUrl {
    /// The requested relative filename components.
    pub components: RootRelativeComponents,
    /// The exact serialized query, distinguishing omission from an empty query.
    pub raw_query: Option<String>,
    /// Whether the absolute root used its admitted spelling.
    pub root_spelling: RootSpelling,
}

const MAX_URL_BYTES: usize = 4 * 32_768;
#[cfg(windows)]
const MAX_RELATIVE_BYTES: usize = 100;

#[cfg(windows)]
impl AdmittedRootUrl {
    /// Validate the normalized local DOS path obtained from a retained handle.
    /// The caller must keep that handle; this constructor grants no authority.
    ///
    /// # Errors
    /// Refuses network/device locators and invalid filesystem components.
    pub fn from_normalized_drive_path(path: &Path) -> Result<Self, FileUrlRefusal> {
        let text = path.to_str().ok_or(FileUrlRefusal::ForbiddenEscape)?;
        if text.len() > MAX_URL_BYTES {
            return Err(FileUrlRefusal::TooLong);
        }
        let text = text.strip_prefix(r"\\?\").unwrap_or(text);
        let bytes = text.as_bytes();
        if bytes.len() < 3
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || bytes[2] != b'\\'
        {
            return Err(FileUrlRefusal::NoDrive);
        }
        let tail = text.get(3..).ok_or(FileUrlRefusal::NoDrive)?;
        let mut components = Vec::new();
        if !tail.is_empty() {
            for component in tail.split('\\') {
                super::validate_fs_component(component)
                    .map_err(|_| FileUrlRefusal::Component(component.to_owned()))?;
                components.push(component.to_owned());
            }
        }
        Ok(Self {
            drive: bytes[0].to_ascii_uppercase(),
            components,
        })
    }

    /// Spell a source-qualified relative selection as a real local file URL.
    /// The supplied query remains byte-exact and is never decoded.
    ///
    /// # Errors
    /// Refuses query fragments/controls or an overlong resulting URL.
    pub fn url_for(
        &self,
        relative: &RootRelativeComponents,
        raw_query: Option<&str>,
    ) -> Result<String, FileUrlRefusal> {
        if raw_query.is_some_and(|query| query.contains('#') || query.chars().any(char::is_control))
        {
            return Err(FileUrlRefusal::ForbiddenEscape);
        }
        let mut url = format!("file:///{}:/", char::from(self.drive));
        for (index, component) in self.components.iter().chain(relative.0.iter()).enumerate() {
            if index != 0 {
                url.push('/');
            }
            encode_component(component, &mut url);
        }
        if let Some(query) = raw_query {
            url.push('?');
            url.push_str(query);
        }
        if url.len() > MAX_URL_BYTES {
            return Err(FileUrlRefusal::TooLong);
        }
        Ok(url)
    }
}

#[cfg(windows)]
impl RootRelativeComponents {
    /// Validate an already canonical graph path using the same resource grammar.
    ///
    /// # Errors
    /// Refuses aliases, non-NFC names, shell links, and the 100-byte path ceiling.
    pub fn from_graph_path(path: &str) -> Result<Self, FileUrlRefusal> {
        let parts = path.split('/').map(str::to_owned).collect();
        validate_relative_components(parts)
    }

    /// Borrow the validated components without allocating or resolving a path.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// Return the canonical slash-separated relative spelling.
    #[must_use]
    pub fn to_wire_path(&self) -> String {
        self.0.join("/")
    }
}

/// Parse once, compare the admitted root ordinally, and preserve raw query data.
///
/// # Errors
/// Returns the first named refusal from W3's R1 through R10 rules.
#[cfg(windows)]
pub fn file_url_root_relative(
    url: &str,
    root: &AdmittedRootUrl,
) -> Result<ParsedFileUrl, FileUrlRefusal> {
    let decoded = decode_file_url(url)?;
    if !decoded.drive.eq_ignore_ascii_case(&root.drive)
        || decoded.components.len() <= root.components.len()
    {
        return Err(FileUrlRefusal::OutsideRoot);
    }
    let mut spelling = RootSpelling::Exact;
    for (actual, expected) in decoded.components.iter().zip(&root.components) {
        if !ordinal_equal(actual, expected)? {
            return Err(FileUrlRefusal::OutsideRoot);
        }
        if actual != expected {
            spelling = RootSpelling::CaseVariant;
        }
    }
    let relative = decoded
        .components
        .into_iter()
        .skip(root.components.len())
        .collect();
    Ok(ParsedFileUrl {
        components: validate_relative_components(relative)?,
        raw_query: decoded.raw_query,
        root_spelling: spelling,
    })
}

/// Compare previously validated components with the one Windows ordinal rule.
/// The query is intentionally absent from this filename comparison.
///
/// # Errors
/// Refuses a Windows NLS failure.
#[cfg(windows)]
pub fn file_components_equal(
    left: &RootRelativeComponents,
    right: &RootRelativeComponents,
) -> Result<bool, FileUrlRefusal> {
    if left.0.len() != right.0.len() {
        return Ok(false);
    }
    for (left, right) in left.0.iter().zip(&right.0) {
        if !ordinal_equal(left, right)? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(windows)]
fn ordinal_equal(left: &str, right: &str) -> Result<bool, FileUrlRefusal> {
    if left == right {
        return Ok(true);
    }
    let fail = || FileUrlRefusal::Component(left.to_owned());
    let left = super::utf16(left).map_err(|_| fail())?;
    let right = super::utf16(right).map_err(|_| fail())?;
    super::ordinal_order(&left, &right)
        .map(|order| order == std::cmp::Ordering::Equal)
        .map_err(|()| fail())
}

#[cfg(windows)]
fn validate_relative_components(
    parts: Vec<String>,
) -> Result<RootRelativeComponents, FileUrlRefusal> {
    if parts.is_empty() {
        return Err(FileUrlRefusal::OutsideRoot);
    }
    for part in &parts {
        super::validate_windows_package_component(part)
            .and_then(|()| super::ensure_nfc(part))
            .map_err(|_| FileUrlRefusal::Component(part.clone()))?;
    }
    let length = parts
        .iter()
        .try_fold(parts.len() - 1, |length, part| {
            length.checked_add(part.len())
        })
        .ok_or(FileUrlRefusal::TooLong)?;
    if length > MAX_RELATIVE_BYTES {
        return Err(FileUrlRefusal::TooLong);
    }
    if let Some(extension) = parts
        .last()
        .and_then(|leaf| leaf.rsplit_once('.'))
        .map(|(_, ext)| ext)
        && ordinal_equal(extension, "lnk")?
    {
        return Err(FileUrlRefusal::ShellLink);
    }
    Ok(RootRelativeComponents(parts))
}

#[cfg(windows)]
fn encode_component(component: &str, output: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in component.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct DecodedFileUrl {
    drive: u8,
    components: Vec<String>,
    raw_query: Option<String>,
}

fn decode_file_url(url: &str) -> Result<DecodedFileUrl, FileUrlRefusal> {
    let (scheme, rest) = url.split_once(':').ok_or(FileUrlRefusal::NotFileScheme)?;
    if !scheme.eq_ignore_ascii_case("file") {
        return Err(FileUrlRefusal::NotFileScheme);
    }
    if url.len() > MAX_URL_BYTES {
        return Err(FileUrlRefusal::TooLong);
    }
    let rest = rest.split_once('#').map_or(rest, |(path, _)| path);
    let (path, query) = rest
        .split_once('?')
        .map_or((rest, None), |(path, query)| (path, Some(query)));
    let path = if let Some(authority_path) = path.strip_prefix("//") {
        if authority_path.is_empty() {
            return Err(FileUrlRefusal::NoDrive);
        }
        if !authority_path.starts_with('/') {
            return Err(FileUrlRefusal::Authority);
        }
        authority_path
    } else {
        path
    };
    if path.starts_with("//") || path.starts_with('\\') {
        return Err(FileUrlRefusal::DevicePrefix);
    }
    let bytes = path.as_bytes();
    if bytes.len() < 4
        || bytes[0] != b'/'
        || !bytes[1].is_ascii_alphabetic()
        || bytes[2] != b':'
        || bytes[3] != b'/'
    {
        return Err(FileUrlRefusal::NoDrive);
    }
    let tail = path.get(4..).ok_or(FileUrlRefusal::NoDrive)?;
    if tail.is_empty() || tail.ends_with('/') {
        return Err(FileUrlRefusal::DotSegment);
    }
    let mut components = Vec::new();
    for raw in tail.split('/').filter(|part| !part.is_empty()) {
        let decoded = decode_component(raw)?;
        if decoded.contains(['/', '\\', '\0']) {
            return Err(FileUrlRefusal::DecodedSeparator);
        }
        if matches!(decoded.as_str(), "." | "..") {
            return Err(FileUrlRefusal::DotSegment);
        }
        components.push(decoded);
    }
    Ok(DecodedFileUrl {
        drive: bytes[1],
        components,
        raw_query: query.map(str::to_owned),
    })
}

fn decode_component(value: &str) -> Result<String, FileUrlRefusal> {
    let mut decoded = Vec::with_capacity(value.len());
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let (Some(high), Some(low)) = (bytes.next(), bytes.next()) else {
                return Err(FileUrlRefusal::ForbiddenEscape);
            };
            let high = hex_digit(high).ok_or(FileUrlRefusal::ForbiddenEscape)?;
            let low = hex_digit(low).ok_or(FileUrlRefusal::ForbiddenEscape)?;
            let byte = high * 16 + low;
            if matches!(byte, b'/' | b'\\' | 0) {
                return Err(FileUrlRefusal::ForbiddenEscape);
            }
            decoded.push(byte);
        } else {
            decoded.push(byte);
        }
    }
    String::from_utf8(decoded).map_err(|_| FileUrlRefusal::ForbiddenEscape)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Compare a parsed resource selection with an immutable canonical graph path.
/// Graph paths may be longer than the request ceiling; they still never acquire
/// authority through an alias. The caller retains the verified graph inventory.
///
/// # Errors
/// Refuses malformed graph components, non-NFC names or a Windows NLS failure.
#[cfg(windows)]
pub fn file_components_match_graph_path(
    components: &RootRelativeComponents,
    graph_path: &str,
) -> Result<bool, FileUrlRefusal> {
    if components.0.len() != graph_path.split('/').count() {
        return Ok(false);
    }
    for (requested, member) in components.0.iter().zip(graph_path.split('/')) {
        super::validate_windows_package_component(member)
            .and_then(|()| super::ensure_nfc(member))
            .map_err(|_| FileUrlRefusal::Component(member.to_owned()))?;
        if !ordinal_equal(requested, member)? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_file_url_rules_decode_once_and_preserve_query() {
        let parsed = decode_file_url("FILE:///d:/Project//assets/e%CC%81.txt?x=%252f&y=1#ignored")
            .expect("portable lexical parse");
        assert_eq!(parsed.drive, b'd');
        assert_eq!(parsed.components, ["Project", "assets", "e\u{301}.txt"]);
        assert_eq!(parsed.raw_query.as_deref(), Some("x=%252f&y=1"));
        assert_eq!(
            decode_file_url("file:///D:/root/%252e%252e")
                .expect("single decode")
                .components,
            ["root", "%2e%2e"]
        );
        assert_eq!(
            decode_file_url("file:///D:/root/file?")
                .expect("empty query")
                .raw_query,
            Some(String::new())
        );
        assert_eq!(
            decode_file_url("file:///D:/root/file")
                .expect("absent query")
                .raw_query,
            None
        );
    }

    #[test]
    fn lexical_file_url_refusals_have_independent_golden_classes() {
        for (url, refusal) in [
            ("https://example.test/a", FileUrlRefusal::NotFileScheme),
            ("file://host/share/a", FileUrlRefusal::Authority),
            ("file:///relative/a", FileUrlRefusal::NoDrive),
            ("file:////?/D:/root/a", FileUrlRefusal::DevicePrefix),
            ("file:///D:/root/%2f", FileUrlRefusal::ForbiddenEscape),
            ("file:///D:/root/%5c", FileUrlRefusal::ForbiddenEscape),
            ("file:///D:/root/%00", FileUrlRefusal::ForbiddenEscape),
            ("file:///D:/root/%ff", FileUrlRefusal::ForbiddenEscape),
            ("file:///D:/root/%zz", FileUrlRefusal::ForbiddenEscape),
            ("file:///D:/root/%2e%2e/a", FileUrlRefusal::DotSegment),
            ("file:///D:/root/.", FileUrlRefusal::DotSegment),
            ("file:///D:/root/", FileUrlRefusal::DotSegment),
        ] {
            assert_eq!(decode_file_url(url), Err(refusal), "{url}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn ordinal_root_and_resource_namespace_match_the_approved_windows_contract() {
        let root =
            AdmittedRootUrl::from_normalized_drive_path(Path::new(r"\\?\D:\Users\John Doe\App"))
                .expect("handle-derived long locator");
        let exact = file_url_root_relative(
            "file:///d:/Users/John%20Doe/App/nested/ENTRY.html?x=%252f",
            &root,
        )
        .expect("drive case does not change root spelling");
        assert_eq!(exact.root_spelling, RootSpelling::Exact);
        assert_eq!(exact.raw_query.as_deref(), Some("x=%252f"));
        let variant = file_url_root_relative(
            "file:///D:/users/John%20Doe/app/nested/entry.html?x=%252f",
            &root,
        )
        .expect("ordinal root variant");
        assert_eq!(variant.root_spelling, RootSpelling::CaseVariant);
        assert!(
            file_components_equal(&exact.components, &variant.components)
                .expect("ordinal components")
        );
        assert_eq!(
            root.url_for(&variant.components, variant.raw_query.as_deref())
                .expect("source URL"),
            "file:///D:/Users/John%20Doe/App/nested/entry.html?x=%252f"
        );
        for relative in [
            "name:stream",
            "CON",
            "x.",
            "x ",
            "LONGNA~1.JS",
            "e\u{301}.txt",
        ] {
            let url = format!("file:///D:/Users/John%20Doe/App/{relative}");
            assert!(
                matches!(
                    file_url_root_relative(&url, &root),
                    Err(FileUrlRefusal::Component(_))
                ),
                "{relative}"
            );
        }
        assert_eq!(
            file_url_root_relative("file:///D:/Users/John%20Doe/App/alias.LNK", &root),
            Err(FileUrlRefusal::ShellLink)
        );
        assert_eq!(
            file_url_root_relative("file:///D:/Users/John%20Doe/App-sibling/file", &root),
            Err(FileUrlRefusal::OutsideRoot)
        );
        assert_eq!(
            RootRelativeComponents::from_graph_path(&"x".repeat(101)),
            Err(FileUrlRefusal::TooLong)
        );
    }
}
