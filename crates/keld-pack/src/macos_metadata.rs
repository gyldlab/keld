//! Closed XML metadata for the first local macOS bundle.
//!
//! Producer and installed admission share this codec. It deliberately accepts
//! only the XML subset emitted here, with no entity declarations, binary plist,
//! implicit type conversion or duplicate-key normalization.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;

/// Only target admitted by the initial local macOS package contract.
pub const MACOS_LOCAL_TARGET: &str = "macos-arm64";
/// Fixed main executable name recorded in the bundle metadata.
pub const MACOS_HOST_EXECUTABLE: &str = "keld-host";
/// Fixed bundle-relative Bun location; metadata cannot select another runtime.
pub const MACOS_BUN_RELATIVE_PATH: &str = "Contents/Helpers/bun";
/// Fixed bundle-relative application input root.
pub const MACOS_APP_RELATIVE_PATH: &str = "Contents/Resources/app";
/// Maximum encoded local Info.plist size accepted before any parsing.
pub const MAX_MACOS_METADATA_BYTES: usize = 64 * 1024;

const XML_HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>";
const XML_DOCTYPE: &str = "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">";

/// Metadata sealed by the outer application signature.
///
/// These values establish package consistency only. The caller separately
/// validates the canonical application ID through its existing identity owner;
/// neither this codec nor a valid digest authenticates a publisher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacosBundleMetadata {
    /// Application identifier, subject to the existing canonical-ID predicate.
    pub app_id: String,
    /// User-visible application name.
    pub name: String,
    /// Three-component numeric release version.
    pub version: String,
    /// One to three numeric build-version components.
    pub bundle_version: String,
    /// One to three numeric minimum-macOS components.
    pub minimum_system_version: String,
    /// Whether the fixed app.icns resource is present.
    pub icon: bool,
    /// Expected exact bytes of the separately qualified bundled Bun executable.
    pub bun_sha256: [u8; 32],
}

/// A bounded local bundle metadata refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacosMetadataError {
    detail: &'static str,
}

impl MacosMetadataError {
    /// Stable diagnostic identifier.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "KELD-PACK-013"
    }
}

impl fmt::Display for MacosMetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "KELD-PACK-013: invalid local macOS bundle metadata ({}). Rebuild the application with the supported macOS packager; do not edit its sealed Info.plist.",
            self.detail
        )
    }
}

impl std::error::Error for MacosMetadataError {}

fn invalid(detail: &'static str) -> MacosMetadataError {
    MacosMetadataError { detail }
}

impl MacosBundleMetadata {
    /// Validates the representation owned by Apple's metadata fields.
    ///
    /// Canonical KELD application identity and executable deployment floors are
    /// separate caller-owned predicates and cannot be inferred from this check.
    ///
    /// # Errors
    ///
    /// Refuses invalid strings, Apple identifier characters or numeric versions.
    pub fn validate(&self) -> Result<(), MacosMetadataError> {
        if !valid_text(&self.name, 255)
            || !valid_text(&self.app_id, 255)
            || !self
                .app_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'))
        {
            return Err(invalid("invalid application name or bundle identifier"));
        }
        // Current Apple Bundle Resources documents one to three numeric build
        // components and exactly three short-version components. No suffix or
        // extra component is silently removed.
        if !numeric_version(&self.version, 3, 3)
            || !numeric_version(&self.bundle_version, 1, 3)
            || !numeric_version(&self.minimum_system_version, 1, 3)
        {
            return Err(invalid("invalid release, build or minimum-system version"));
        }
        Ok(())
    }

    /// Emits the bounded XML representation consumed by installed admission.
    ///
    /// # Errors
    ///
    /// Refuses metadata that fails the representation rules in `Self::validate`.
    pub fn to_plist(&self) -> Result<Vec<u8>, MacosMetadataError> {
        self.validate()?;
        let mut xml = format!("{XML_HEADER}\n{XML_DOCTYPE}\n<plist version=\"1.0\">\n<dict>\n");
        for (key, value) in [
            ("CFBundlePackageType", "APPL"),
            ("CFBundleExecutable", MACOS_HOST_EXECUTABLE),
            ("CFBundleIdentifier", self.app_id.as_str()),
            ("CFBundleName", self.name.as_str()),
            ("CFBundleShortVersionString", self.version.as_str()),
            ("CFBundleVersion", self.bundle_version.as_str()),
            (
                "LSMinimumSystemVersion",
                self.minimum_system_version.as_str(),
            ),
        ] {
            emit_string(&mut xml, key, value);
        }
        if self.icon {
            emit_string(&mut xml, "CFBundleIconFile", "app.icns");
        }
        xml.push_str("<key>KELDLocalBundle</key><dict>\n<key>schema</key><integer>1</integer>\n");
        emit_string(&mut xml, "target", MACOS_LOCAL_TARGET);
        let mut digest = String::with_capacity(64);
        for byte in &self.bun_sha256 {
            // Formatting into a String cannot fail.
            let _ = write!(&mut digest, "{byte:02x}");
        }
        emit_string(&mut xml, "bunSha256", &digest);
        xml.push_str("</dict>\n</dict>\n</plist>\n");
        if xml.len() > MAX_MACOS_METADATA_BYTES {
            return Err(invalid("Info.plist exceeds its byte bound"));
        }
        Ok(xml.into_bytes())
    }

    /// Parses the closed producer representation without normalizing duplicates.
    ///
    /// Only string, unsigned integer and dictionary values are representable.
    /// No DTD is loaded; only the exact inert Apple declaration is recognized.
    ///
    /// # Errors
    ///
    /// Refuses oversized, malformed, duplicate, unknown, missing or wrongly typed
    /// metadata, unsupported local schema/target, or invalid field representations.
    pub fn from_plist(bytes: &[u8]) -> Result<Self, MacosMetadataError> {
        if bytes.len() > MAX_MACOS_METADATA_BYTES {
            return Err(invalid("Info.plist exceeds its byte bound"));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| invalid("Info.plist is not UTF-8"))?;
        let mut parser = Parser { rest: text };
        parser.consume(XML_HEADER);
        parser.consume(XML_DOCTYPE);
        parser.expect("<plist version=\"1.0\">")?;
        let mut fields = parser.dictionary(0)?;
        parser.expect("</plist>")?;
        if !parser.rest.trim().is_empty() {
            return Err(invalid("unexpected content after plist"));
        }
        if take_text(&mut fields, "CFBundlePackageType")? != "APPL"
            || take_text(&mut fields, "CFBundleExecutable")? != MACOS_HOST_EXECUTABLE
        {
            return Err(invalid(
                "bundle kind or executable is not the fixed application shape",
            ));
        }
        let Some(Value::Dictionary(mut local)) = fields.remove("KELDLocalBundle") else {
            return Err(invalid(
                "local bundle record is missing or is not a dictionary",
            ));
        };
        if !matches!(local.remove("schema"), Some(Value::Integer(1)))
            || take_text(&mut local, "target")? != MACOS_LOCAL_TARGET
        {
            return Err(invalid("unsupported local schema or target"));
        }
        let bun_sha256 = parse_digest(&take_text(&mut local, "bunSha256")?)?;
        if !local.is_empty() {
            return Err(invalid("unknown local bundle field"));
        }
        let icon = match fields.remove("CFBundleIconFile") {
            None => false,
            Some(Value::Text(value)) if value == "app.icns" => true,
            _ => return Err(invalid("icon must use the fixed app.icns resource")),
        };
        let metadata = Self {
            app_id: take_text(&mut fields, "CFBundleIdentifier")?,
            name: take_text(&mut fields, "CFBundleName")?,
            version: take_text(&mut fields, "CFBundleShortVersionString")?,
            bundle_version: take_text(&mut fields, "CFBundleVersion")?,
            minimum_system_version: take_text(&mut fields, "LSMinimumSystemVersion")?,
            icon,
            bun_sha256,
        };
        if !fields.is_empty() {
            return Err(invalid("unknown top-level bundle field"));
        }
        metadata.validate()?;
        Ok(metadata)
    }
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.chars().all(|character| {
            !character.is_control() && character != '\u{fffe}' && character != '\u{ffff}'
        })
}

fn numeric_version(value: &str, minimum: usize, maximum: usize) -> bool {
    if value.is_empty() || value.len() > 128 {
        return false;
    }
    let components = value.split('.');
    let count = components.clone().count();
    (minimum..=maximum).contains(&count)
        && components.clone().all(|component| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn emit_string(xml: &mut String, key: &str, value: &str) {
    xml.push_str("<key>");
    xml.push_str(key);
    xml.push_str("</key><string>");
    for character in value.chars() {
        match character {
            '&' => xml.push_str("&amp;"),
            '<' => xml.push_str("&lt;"),
            '>' => xml.push_str("&gt;"),
            '"' => xml.push_str("&quot;"),
            '\'' => xml.push_str("&apos;"),
            _ => xml.push(character),
        }
    }
    xml.push_str("</string>\n");
}

fn parse_digest(value: &str) -> Result<[u8; 32], MacosMetadataError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "runtime digest must be 64 lowercase hexadecimal digits",
        ));
    }
    let mut digest = [0_u8; 32];
    for (index, chunk) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let text =
            std::str::from_utf8(chunk).map_err(|_| invalid("invalid runtime digest encoding"))?;
        digest[index] =
            u8::from_str_radix(text, 16).map_err(|_| invalid("invalid runtime digest encoding"))?;
    }
    Ok(digest)
}

enum Value {
    Text(String),
    Integer(u64),
    Dictionary(BTreeMap<String, Value>),
}

fn take_text(
    fields: &mut BTreeMap<String, Value>,
    key: &str,
) -> Result<String, MacosMetadataError> {
    match fields.remove(key) {
        Some(Value::Text(value)) => Ok(value),
        _ => Err(invalid(
            "required string field is missing or has another type",
        )),
    }
}

struct Parser<'a> {
    rest: &'a str,
}

impl Parser<'_> {
    fn consume(&mut self, token: &str) -> bool {
        self.rest = self.rest.trim_start_matches([' ', '\t', '\r', '\n']);
        if let Some(rest) = self.rest.strip_prefix(token) {
            self.rest = rest;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: &str) -> Result<(), MacosMetadataError> {
        if self.consume(token) {
            Ok(())
        } else {
            Err(invalid("malformed or unsupported XML structure"))
        }
    }

    fn element_text(&mut self, opening: &str, closing: &str) -> Result<String, MacosMetadataError> {
        self.expect(opening)?;
        let end = self
            .rest
            .find(closing)
            .ok_or_else(|| invalid("unterminated XML value"))?;
        let raw = &self.rest[..end];
        if raw.contains('<') || raw.contains("]]>") {
            return Err(invalid("nested markup in text is forbidden"));
        }
        let decoded = decode_entities(raw)?;
        self.rest = &self.rest[end + closing.len()..];
        Ok(decoded)
    }

    fn dictionary(&mut self, depth: usize) -> Result<BTreeMap<String, Value>, MacosMetadataError> {
        if depth > 1 {
            return Err(invalid("metadata nesting exceeds the closed schema"));
        }
        self.expect("<dict>")?;
        let mut fields = BTreeMap::new();
        loop {
            if self.consume("</dict>") {
                return Ok(fields);
            }
            if fields.len() == 16 {
                return Err(invalid("too many metadata fields"));
            }
            let key = self.element_text("<key>", "</key>")?;
            if !valid_text(&key, 128) || fields.contains_key(&key) {
                return Err(invalid("empty, invalid or duplicate metadata key"));
            }
            let value = if self.rest.trim_start().starts_with("<string>") {
                Value::Text(self.element_text("<string>", "</string>")?)
            } else if self.rest.trim_start().starts_with("<integer>") {
                let text = self.element_text("<integer>", "</integer>")?;
                if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(invalid("invalid unsigned metadata integer"));
                }
                Value::Integer(
                    text.parse()
                        .map_err(|_| invalid("metadata integer overflow"))?,
                )
            } else if self.rest.trim_start().starts_with("<dict>") {
                Value::Dictionary(self.dictionary(depth + 1)?)
            } else {
                return Err(invalid("unsupported metadata value type"));
            };
            fields.insert(key, value);
        }
    }
}

fn decode_entities(raw: &str) -> Result<String, MacosMetadataError> {
    let mut decoded = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(position) = rest.find('&') {
        decoded.push_str(&rest[..position]);
        rest = &rest[position..];
        let mut matched = false;
        for (entity, character) in [
            ("&amp;", '&'),
            ("&lt;", '<'),
            ("&gt;", '>'),
            ("&quot;", '"'),
            ("&apos;", '\''),
        ] {
            if let Some(suffix) = rest.strip_prefix(entity) {
                decoded.push(character);
                rest = suffix;
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(invalid("unsupported or unterminated XML entity"));
        }
    }
    decoded.push_str(rest);
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::{MAX_MACOS_METADATA_BYTES, MacosBundleMetadata};

    fn metadata() -> MacosBundleMetadata {
        MacosBundleMetadata {
            app_id: "org.example.local".to_owned(),
            name: "A & B <Local> \"quoted\" 'name'".to_owned(),
            version: "1.2.3".to_owned(),
            bundle_version: "4".to_owned(),
            minimum_system_version: "13.0".to_owned(),
            icon: true,
            bun_sha256: [0xab; 32],
        }
    }

    fn encoded() -> String {
        String::from_utf8(metadata().to_plist().expect("valid fixture")).expect("UTF-8")
    }

    #[test]
    fn emits_fixed_layout_and_exact_closed_runtime_record() {
        let xml = encoded();
        assert!(xml.contains("<key>CFBundleExecutable</key><string>keld-host</string>"));
        assert!(xml.contains("<key>schema</key><integer>1</integer>"));
        assert!(xml.contains("<key>target</key><string>macos-arm64</string>"));
        assert!(xml.contains("<key>bunSha256</key><string>abababababababababababababababababababababababababababababababab</string>"));
        assert!(xml.contains("A &amp; B &lt;Local&gt; &quot;quoted&quot; &apos;name&apos;"));
        assert_eq!(
            MacosBundleMetadata::from_plist(xml.as_bytes()),
            Ok(metadata())
        );
    }

    #[test]
    fn independently_written_metadata_parses_without_producer_oracle() {
        let bytes = br#"<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>org.example.fixture</string>
<key>CFBundleName</key><string>Fixture</string>
<key>CFBundleExecutable</key><string>keld-host</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>0</string>
<key>LSMinimumSystemVersion</key><string>13</string>
<key>KELDLocalBundle</key><dict>
<key>bunSha256</key><string>0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef</string>
<key>target</key><string>macos-arm64</string><key>schema</key><integer>1</integer>
</dict></dict></plist>"#;
        let parsed = MacosBundleMetadata::from_plist(bytes).expect("independent literal");
        assert_eq!(parsed.app_id, "org.example.fixture");
        assert_eq!(parsed.version, "0.1.0");
        assert_eq!(
            parsed.bun_sha256[0..8],
            [1, 35, 69, 103, 137, 171, 205, 239]
        );
        assert!(!parsed.icon);
    }

    #[test]
    fn single_fault_metadata_controls_refuse_before_facts() {
        let xml = encoded();
        for (from, to) in [
            ("<integer>1</integer>", "<string>1</string>"),
            ("<integer>1</integer>", "<integer>2</integer>"),
            (
                "<integer>1</integer>",
                "<integer>18446744073709551616</integer>",
            ),
            ("<string>macos-arm64</string>", "<string>macos-x64</string>"),
            ("<string>keld-host</string>", "<string>../other</string>"),
            ("<string>APPL</string>", "<string>BNDL</string>"),
            ("<string>app.icns</string>", "<string>../icon.icns</string>"),
            (
                "<key>schema</key>",
                "<key>schema</key><integer>1</integer><key>schema</key>",
            ),
            (
                "<key>schema</key>",
                "<key>extra</key><integer>1</integer><key>schema</key>",
            ),
            (
                "<key>CFBundleName</key>",
                "<key>CFBundleName</key><string>duplicate</string><key>CFBundleName</key>",
            ),
            ("<key>KELDLocalBundle</key>", "<key>OtherRecord</key>"),
            ("<string>1.2.3</string>", "<string>1.2.3-beta</string>"),
            ("<string>4</string>", "<string>4.0.0.1</string>"),
            ("&amp;", "&unknown;"),
        ] {
            let broken = xml.replacen(from, to, 1);
            assert_ne!(broken, xml, "mutation must change the fixture");
            let error = MacosBundleMetadata::from_plist(broken.as_bytes())
                .expect_err("single-fault mutation must refuse");
            assert_eq!(error.code(), "KELD-PACK-013", "{from} -> {to}");
        }
    }

    #[test]
    fn bounded_parser_rejects_truncation_trailing_data_entities_and_deep_values() {
        let xml = encoded();
        for length in [0, 1, xml.len() / 2, xml.len() - 10] {
            assert!(MacosBundleMetadata::from_plist(&xml.as_bytes()[..length]).is_err());
        }
        assert!(MacosBundleMetadata::from_plist(format!("{xml}junk").as_bytes()).is_err());
        assert!(
            MacosBundleMetadata::from_plist(&vec![b' '; MAX_MACOS_METADATA_BYTES + 1]).is_err()
        );
        assert!(MacosBundleMetadata::from_plist(b"<!DOCTYPE plist [<!ENTITY bad SYSTEM 'file:///etc/passwd'>]><plist version=\"1.0\"><dict></dict></plist>").is_err());
        let nested = xml.replace(
            "<integer>1</integer>",
            "<dict><key>a</key><dict></dict></dict>",
        );
        assert!(MacosBundleMetadata::from_plist(nested.as_bytes()).is_err());
    }

    #[test]
    fn invalid_authoring_values_cannot_be_emitted() {
        for name in ["", "bad\nname", "bad\0name"] {
            let mut value = metadata();
            value.name = name.to_owned();
            assert!(value.to_plist().is_err());
        }
        for version in ["1", "1.2", "1.2.3.4", "1.2.-3", "1.2.3a1", "1..3"] {
            let mut value = metadata();
            value.version = version.to_owned();
            assert!(value.to_plist().is_err());
        }
        let mut value = metadata();
        value.app_id = "org.example/escape".to_owned();
        assert!(value.to_plist().is_err());
    }
}
