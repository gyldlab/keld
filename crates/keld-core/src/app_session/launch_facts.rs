//! Non-authority development-launch data retained across the private Guardian boundary.

use std::path::{Component, Path};

use objc2_foundation::{
    NSLocale, NSSearchPathDirectory, NSSearchPathDomainMask, NSSearchPathForDirectoriesInDomains,
};
use serde::{Deserialize, Serialize};

use super::HostAppError;
use super::original_webapp::PreparedOriginalSource;

const MAX_RECORD_BYTES: usize = 16 * 1024;
const MAX_FIELD_BYTES: usize = 4096;
const ORIGINAL_RESOURCE_POLICY_CONTRACT: &str = "gh473-original-editor-offline/v1";

/// Closed development-launch facts. This record never carries a grant or identity.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentLaunchFacts {
    schema: u8,
    kind: String,
    name: String,
    root: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::hello::deserialize_optional_app_version"
    )]
    version: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    locale: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present_object"
    )]
    paths: Option<LaunchDirectories>,
    #[serde(
        default,
        rename = "singleInstance",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present_object"
    )]
    single_instance: Option<SingleInstanceFact>,
    #[serde(
        default,
        rename = "originalResourcePolicy",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present_object"
    )]
    original_resource_policy: Option<OriginalResourcePolicyDescriptor>,
}

// A source-owner observation, not a native grant or a physical-test receipt.
// Private fields and constructor: only successful opaque source preparation mints it.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct OriginalResourcePolicyDescriptor {
    contract: String,
    graph_sha256: String,
    policy_sha256: String,
    window_key: String,
    selector: String,
}

impl OriginalResourcePolicyDescriptor {
    fn from_prepared_source(prepared: &PreparedOriginalSource) -> Self {
        fn digest_text(digest: [u8; 32]) -> String {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut text = String::from("sha256:");
            for byte in digest {
                text.push(char::from(HEX[usize::from(byte >> 4)]));
                text.push(char::from(HEX[usize::from(byte & 15)]));
            }
            text
        }
        Self {
            contract: String::from(ORIGINAL_RESOURCE_POLICY_CONTRACT),
            graph_sha256: digest_text(prepared.graph_sha256()),
            policy_sha256: digest_text(prepared.policy_sha256()),
            window_key: prepared.window_key().to_owned(),
            selector: keld_guard::WindowCspProfile::OriginalEditorOffline
                .as_str()
                .to_owned(),
        }
    }

    fn validate(&self) -> Result<(), HostAppError> {
        let digest = |value: &str| {
            value.strip_prefix("sha256:").is_some_and(|hex| {
                hex.len() == 64
                    && hex
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        };
        if self.contract != ORIGINAL_RESOURCE_POLICY_CONTRACT
            || self.selector != keld_guard::WindowCspProfile::OriginalEditorOffline.as_str()
            || !digest(&self.graph_sha256)
            || !digest(&self.policy_sha256)
            || !valid_text(&self.window_key)
        {
            return Err(invalid(
                "invalid prepared original-resource policy descriptor; use the same verified source/key and qualified composition",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SingleInstanceFact {
    verdict: super::single_instance::SingleInstanceVerdict,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LaunchDirectories {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    app_data: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    user_data: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    documents: Option<String>,
}

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    // Only an omitted field is absent. JSON null cannot erase a present fact.
    T::deserialize(deserializer).map(Some)
}

// Reuse each derived closed-field visitor, but require an object at both JSON
// object boundaries. Derived structs alone also accept positional sequences.
fn deserialize_object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ObjectVisitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a development-launch JSON object")
        }

        fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(std::marker::PhantomData))
}

fn deserialize_present_object<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    deserialize_object(deserializer).map(Some)
}

fn valid_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_FIELD_BYTES && !value.contains('\0')
}

fn valid_path(value: &str) -> bool {
    valid_text(value) && Path::new(value).is_absolute()
}

fn user_data_path(app_data: Option<&str>, name: &str) -> Option<String> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(component)) if component == name)
        || components.next().is_some()
    {
        return None;
    }
    let joined = Path::new(app_data?).join(name);
    joined
        .to_str()
        .filter(|path| valid_path(path))
        .map(str::to_owned)
}

fn native_directory(directory: NSSearchPathDirectory) -> Result<Option<String>, HostAppError> {
    let paths = NSSearchPathForDirectoriesInDomains(
        directory,
        NSSearchPathDomainMask::UserDomainMask,
        true,
    );
    if paths.is_empty() {
        return Ok(None);
    }
    if paths.len() != 1 {
        return Err(invalid(
            "native user-directory query returned ambiguous results",
        ));
    }
    let value = paths
        .firstObject()
        .ok_or_else(|| invalid("native directory query lost its selected value"))?
        .to_string();
    if !valid_path(&value) {
        return Err(invalid(
            "native directory is not a bounded absolute UTF-8 path",
        ));
    }
    Ok(Some(value))
}

impl LaunchDirectories {
    fn capture(name: &str) -> Result<Self, HostAppError> {
        let app_data = native_directory(NSSearchPathDirectory::ApplicationSupportDirectory)?;
        let documents = native_directory(NSSearchPathDirectory::DocumentDirectory)?;
        let user_data = user_data_path(app_data.as_deref(), name);
        Ok(Self {
            app_data,
            user_data,
            documents,
        })
    }

    fn validate(&self, name: &str) -> Result<(), HostAppError> {
        if [&self.app_data, &self.user_data, &self.documents]
            .into_iter()
            .flatten()
            .any(|path| !valid_path(path))
        {
            return Err(invalid(
                "native directory fact is not a bounded absolute path",
            ));
        }
        if self.user_data.is_some()
            && self.user_data != user_data_path(self.app_data.as_deref(), name)
        {
            return Err(invalid("userData is not the admitted appData/name value"));
        }
        Ok(())
    }
}

impl DevelopmentLaunchFacts {
    /// Bind data eligibility only from the same successful cold source owner.
    /// This neither grants native authority nor qualifies physical effects.
    pub(super) fn set_original_resource_policy(
        &mut self,
        prepared: &PreparedOriginalSource,
    ) -> Result<(), HostAppError> {
        if self.original_resource_policy.is_some() {
            return Err(invalid(
                "original-resource policy descriptor is already bound",
            ));
        }
        if Path::new(&self.root) != prepared.selected_stage() {
            return Err(invalid(
                "prepared original-resource source belongs to another selected stage",
            ));
        }
        let descriptor = OriginalResourcePolicyDescriptor::from_prepared_source(prepared);
        descriptor.validate()?;
        self.original_resource_policy = Some(descriptor);
        self.schema = 5;
        self.private_argument()?;
        Ok(())
    }
    pub(super) fn application_support(&self) -> Option<&str> {
        self.paths.as_ref()?.app_data.as_deref()
    }
    pub(super) fn set_single_instance(
        &mut self,
        verdict: super::single_instance::SingleInstanceVerdict,
    ) -> Result<(), HostAppError> {
        self.single_instance = Some(SingleInstanceFact { verdict });
        self.private_argument()?;
        Ok(())
    }

    pub(super) fn from_validated_development(
        name: &str,
        root: &Path,
        version: Option<&str>,
        has_dev_lease: bool,
    ) -> Result<Option<Self>, HostAppError> {
        if !has_dev_lease {
            return Ok(None);
        }
        let root_text = root
            .to_str()
            .ok_or_else(|| invalid("development launch root is not UTF-8"))?;
        // GH452's bounded macOS contract captures the Host process's first
        // preferred language. This is not Chromium/WebKit locale negotiation.
        // Capture once here; the retained record is reused after Bun restarts.
        let locale = NSLocale::preferredLanguages()
            .firstObject()
            .map(|language| language.to_string());
        let facts = Self {
            schema: 4,
            kind: String::from("development"),
            name: name.to_owned(),
            root: root_text.to_owned(),
            version: version.map(str::to_owned),
            locale,
            paths: Some(LaunchDirectories::capture(name)?),
            single_instance: None,
            original_resource_policy: None,
        };
        facts.validate(root)?;
        facts.private_argument()?;
        Ok(Some(facts))
    }

    /// Decode the Host's private argument and bind its root to the validated stage.
    ///
    /// # Errors
    /// Rejects oversized, malformed, unknown, duplicate, unsupported or foreign-root data.
    pub fn from_private_argument(text: &str, expected_root: &Path) -> Result<Self, HostAppError> {
        if text.len() > MAX_RECORD_BYTES {
            return Err(invalid("development launch facts exceed 16 KiB"));
        }
        let mut decoder = serde_json::Deserializer::from_str(text);
        let facts: Self = deserialize_object(&mut decoder)
            .map_err(|source| invalid(format!("development launch facts JSON: {source}")))?;
        decoder.end().map_err(|source| {
            invalid(format!("development launch facts trailing input: {source}"))
        })?;
        facts.validate(expected_root)?;
        Ok(facts)
    }

    /// Serialize one bounded immutable argument for the private Guardian invocation.
    ///
    /// # Errors
    /// Returns a typed startup error if serialization or the record-size bound fails.
    pub fn private_argument(&self) -> Result<String, HostAppError> {
        let text = serde_json::to_string(self)
            .map_err(|source| invalid(format!("development launch facts encoding: {source}")))?;
        if text.len() > MAX_RECORD_BYTES {
            return Err(invalid("development launch facts exceed 16 KiB"));
        }
        Ok(text)
    }

    /// Build one argv element for Bun's parse-time constant, not a shell expression.
    ///
    /// # Errors
    /// Returns the same bounded-serialization errors as [`Self::private_argument`].
    pub fn bun_define_argument(&self) -> Result<String, HostAppError> {
        Ok(format!(
            "--define=__KELD_HOST_LAUNCH_FACTS__:{}",
            self.private_argument()?
        ))
    }

    fn validate(&self, expected_root: &Path) -> Result<(), HostAppError> {
        if !matches!(self.schema, 1..=5)
            || self.kind != "development"
            || !valid_text(&self.name)
            || !valid_path(&self.root)
            || Path::new(&self.root) != expected_root
        {
            return Err(invalid(
                "invalid development launch facts or mismatched stage root",
            ));
        }
        if (self.schema == 1 && (self.version.is_some() || self.paths.is_some()))
            || (matches!(self.schema, 2..=5) && self.paths.is_none())
            || (!matches!(self.schema, 3..=5) && self.locale.is_some())
            || (!matches!(self.schema, 4 | 5) && self.single_instance.is_some())
            || ((self.schema == 5) != self.original_resource_policy.is_some())
        {
            return Err(invalid(
                "launch facts fields do not match their closed schema",
            ));
        }
        if let Some(version) = &self.version {
            crate::hello::validate_app_version(version)
                .map_err(|source| invalid(source.to_string()))?;
        }
        if self
            .locale
            .as_ref()
            .is_some_and(|locale| !valid_text(locale))
        {
            return Err(invalid(
                "native preferred language is not a bounded nonempty string",
            ));
        }
        if let Some(paths) = &self.paths {
            paths.validate(&self.name)?;
        }
        if let Some(policy) = &self.original_resource_policy {
            policy.validate()?;
        }
        Ok(())
    }
}

fn invalid(detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-037",
        "private development launch facts",
        detail,
        "Restage and launch through the validated no-flag Host; never supply private Guardian arguments.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_one_stays_readable_but_cannot_carry_schema_two_metadata() {
        let legacy = r#"{"schema":1,"kind":"development","name":"n","root":"/stage"}"#;
        let decoded = DevelopmentLaunchFacts::from_private_argument(legacy, Path::new("/stage"))
            .expect("existing schema one");
        assert_eq!(decoded.private_argument().expect("legacy encoding"), legacy);
        for extra in [r#", "version":"1""#, r#", "paths":{}"#] {
            let payload = format!("{}{extra}}}", &legacy[..legacy.len() - 1]);
            assert!(
                DevelopmentLaunchFacts::from_private_argument(&payload, Path::new("/stage"))
                    .is_err()
            );
        }
    }

    #[test]
    fn private_schema_three_preserves_locale_and_omission_without_defaults() {
        let root = Path::new("/stage");
        for locale in [
            "fr-CA",
            "zh-Hant-TW",
            "es-419",
            &"a".repeat(MAX_FIELD_BYTES),
        ] {
            let payload = serde_json::json!({
                "schema": 3, "kind": "development", "name": "n", "root": "/stage",
                "paths": {}, "locale": locale
            });
            let parsed = DevelopmentLaunchFacts::from_private_argument(&payload.to_string(), root)
                .expect("schema-three locale");
            let encoded: serde_json::Value =
                serde_json::from_str(&parsed.private_argument().expect("encode")).expect("JSON");
            assert_eq!(encoded["locale"], locale);
            assert_eq!(encoded["schema"], 3);
        }
        let absent = r#"{"schema":3,"kind":"development","name":"n","root":"/stage","paths":{}}"#;
        let parsed = DevelopmentLaunchFacts::from_private_argument(absent, root)
            .expect("missing native preference is data absence");
        assert_eq!(parsed.private_argument().expect("encode absent"), absent);
    }

    #[test]
    fn private_locale_requires_schema_three_and_rejects_malformed_present_values() {
        let root = Path::new("/stage");
        for payload in [
            r#"{"schema":1,"kind":"development","name":"n","root":"/stage","locale":"fr-CA"}"#.to_owned(),
            r#"{"schema":2,"kind":"development","name":"n","root":"/stage","paths":{},"locale":"fr-CA"}"#.to_owned(),
            r#"{"schema":3,"kind":"development","name":"n","root":"/stage","locale":"fr-CA"}"#.to_owned(),
            r#"{"schema":3,"kind":"development","name":"n","root":"/stage","paths":{},"locale":"fr-CA","locale":"es-419"}"#.to_owned(),
        ] {
            let error = DevelopmentLaunchFacts::from_private_argument(&payload, root)
                .expect_err("wrong schema, missing paths or duplicate locale");
            assert_eq!(error.code(), "KELD-CORE-037");
        }
        for locale in [
            serde_json::Value::Null,
            serde_json::json!(""),
            serde_json::json!(false),
            serde_json::json!(1),
            serde_json::json!({}),
            serde_json::json!([]),
            serde_json::json!("fr\0CA"),
            serde_json::json!("a".repeat(MAX_FIELD_BYTES + 1)),
            serde_json::json!("é".repeat(MAX_FIELD_BYTES / 2 + 1)),
        ] {
            let payload = serde_json::json!({
                "schema": 3, "kind": "development", "name": "n", "root": "/stage",
                "paths": {}, "locale": locale
            });
            let error = DevelopmentLaunchFacts::from_private_argument(&payload.to_string(), root)
                .expect_err("malformed present locale");
            assert_eq!(error.code(), "KELD-CORE-037");
        }
    }

    #[test]
    fn schema_two_native_paths_are_closed_partial_data_not_a_creation_or_grant() {
        let root = Path::new("/stage");
        let base = r#""schema":2,"kind":"development","name":"n","root":"/stage""#;
        for paths in [
            "{}",
            r#"{"documents":"/Users/test/Documents"}"#,
            r#"{"appData":"/Users/test/Library/Application Support","userData":"/Users/test/Library/Application Support/n"}"#,
        ] {
            let payload = format!("{{{base},\"paths\":{paths}}}");
            let parsed = DevelopmentLaunchFacts::from_private_argument(&payload, root)
                .expect("partial paths");
            assert!(parsed.version.is_none());
        }
        for extra in [
            r#""paths":null"#,
            r#""paths":[]"#,
            r#""paths":{"documents":null}"#,
            r#""paths":{"documents":"relative"}"#,
            r#""paths":{"unknown":"/path"}"#,
            r#""paths":{"documents":"/one","documents":"/two"}"#,
            r#""paths":{"userData":"/not-derived"}"#,
            r#""paths":{},"version":null"#,
            r#""paths":{},"version":"""#,
            r#""paths":{},"version":"one","version":"two""#,
        ] {
            let payload = format!("{{{base},{extra}}}");
            assert!(
                DevelopmentLaunchFacts::from_private_argument(&payload, root).is_err(),
                "{payload}"
            );
        }
        for name in [
            ".",
            "..",
            "../escape",
            "/absolute",
            "nested/name",
            "trailing/",
        ] {
            assert!(
                user_data_path(Some("/Users/test/Library/Application Support"), name).is_none(),
                "{name}"
            );
        }
        assert!(user_data_path(None, "Name").is_none());
        assert_eq!(
            user_data_path(Some("/Users/test/Library/Application Support"), "Name λ"),
            Some(String::from(
                "/Users/test/Library/Application Support/Name λ"
            ))
        );
        let temp = tempfile::tempdir().expect("no-create observer");
        let absent = temp.path().join("not-created");
        let derived = user_data_path(absent.to_str(), "Name").expect("data-only path");
        assert_eq!(Path::new(&derived), absent.join("Name"));
        assert!(!absent.exists(), "deriving a path must not create it");
    }

    #[test]
    fn verified_development_values_roundtrip_without_environment_or_defaults() {
        let root = Path::new("/private/test-stage");
        let facts = DevelopmentLaunchFacts::from_validated_development(
            "quoted \"name\" λ",
            root,
            Some("29.1.7"),
            true,
        )
        .expect("validated values")
        .expect("development provenance");
        let encoded = facts.private_argument().expect("bounded encode");
        let decoded =
            DevelopmentLaunchFacts::from_private_argument(&encoded, root).expect("decode");
        assert_eq!(decoded.name, "quoted \"name\" λ");
        assert_eq!(decoded.root, "/private/test-stage");
        assert_eq!(decoded.kind, "development");
        assert_eq!(decoded.version.as_deref(), Some("29.1.7"));
        assert_eq!(
            decoded.bun_define_argument().expect("define"),
            format!("--define=__KELD_HOST_LAUNCH_FACTS__:{encoded}")
        );
        assert!(
            DevelopmentLaunchFacts::from_validated_development("name", root, None, false)
                .expect("unknown provenance")
                .is_none()
        );
    }

    #[test]
    fn closed_private_record_refuses_malformed_duplicate_foreign_and_oversized_input() {
        let root = Path::new("/stage");
        for text in [
            "null",
            "[]",
            r#"[1,"development","n","/stage"]"#,
            "{}",
            "{broken",
            r#"{"schema":1,"schema":1,"kind":"development","name":"n","root":"/stage"}"#,
            r#"{"schema":2,"kind":"development","name":"n","root":"/stage"}"#,
            r#"{"schema":1,"kind":"installed","name":"n","root":"/stage"}"#,
            r#"{"schema":1,"kind":"development","name":"n","root":"/other"}"#,
            r#"{"schema":1,"kind":"development","name":"n","root":"/stage","grant":"all"}"#,
            r#"{"schema":1,"kind":"development","name":null,"root":"/stage"}"#,
            r#"{"schema":1,"kind":"development","name":"n","root":"/stage"} false"#,
        ] {
            let error = DevelopmentLaunchFacts::from_private_argument(text, root).expect_err(text);
            assert_eq!(error.code(), "KELD-CORE-037");
        }
        assert!(
            DevelopmentLaunchFacts::from_private_argument(&" ".repeat(MAX_RECORD_BYTES + 1), root)
                .is_err()
        );
        assert!(
            DevelopmentLaunchFacts::from_validated_development(
                &"a".repeat(MAX_FIELD_BYTES + 1),
                root,
                None,
                true
            )
            .is_err()
        );
        assert!(
            DevelopmentLaunchFacts::from_validated_development(
                "n",
                Path::new("relative"),
                None,
                true
            )
            .is_err()
        );
    }

    #[test]
    fn private_schema_four_roundtrips_both_data_verdicts_and_preserves_absence() {
        let root = Path::new("/stage");
        for verdict in ["acquired", "contended"] {
            let payload = format!(
                r#"{{"schema":4,"kind":"development","name":"n","root":"/stage","paths":{{}},"singleInstance":{{"verdict":"{verdict}"}}}}"#
            );
            let parsed = DevelopmentLaunchFacts::from_private_argument(&payload, root)
                .expect("one schema-four data verdict");
            assert_eq!(
                parsed.private_argument().expect("same private bytes"),
                payload
            );
        }
        let absent = r#"{"schema":4,"kind":"development","name":"n","root":"/stage","paths":{}}"#;
        let parsed =
            DevelopmentLaunchFacts::from_private_argument(absent, root).expect("undeclared");
        assert_eq!(
            parsed.private_argument().expect("absence preserved"),
            absent
        );
        for schema in 1..=3 {
            let paths = if schema == 1 { "" } else { r#","paths":{}"# };
            let payload = format!(
                r#"{{"schema":{schema},"kind":"development","name":"n","root":"/stage"{paths},"singleInstance":{{"verdict":"acquired"}}}}"#
            );
            let error = DevelopmentLaunchFacts::from_private_argument(&payload, root)
                .expect_err("old records cannot forge a new service fact");
            assert_eq!(error.code(), "KELD-CORE-037");
        }
    }

    #[test]
    fn private_single_instance_record_rejects_malformed_duplicate_and_authority_fields() {
        let root = Path::new("/stage");
        for field in [
            r"null",
            r"[]",
            r#"["acquired"]"#,
            r"{}",
            r#"{"verdict":null}"#,
            r#"{"verdict":true}"#,
            r#"{"verdict":"ACQUIRED"}"#,
            r#"{"verdict":"other"}"#,
            r#"{"verdict":"acquired","id":"org.keld.app"}"#,
            r#"{"verdict":"acquired","fd":3}"#,
            r#"{"verdict":"acquired","verdict":"contended"}"#,
        ] {
            let payload = format!(
                r#"{{"schema":4,"kind":"development","name":"n","root":"/stage","paths":{{}},"singleInstance":{field}}}"#
            );
            let error = DevelopmentLaunchFacts::from_private_argument(&payload, root)
                .expect_err("closed data-only result");
            assert_eq!(error.code(), "KELD-CORE-037", "{field}");
        }
        for payload in [
            r#"{"schema":4,"kind":"development","name":"n","root":"/stage","singleInstance":{"verdict":"acquired"}}"#,
            r#"{"schema":4,"kind":"development","name":"n","root":"/stage","paths":{},"singleInstance":{"verdict":"acquired"},"singleInstance":{"verdict":"contended"}}"#,
        ] {
            let error = DevelopmentLaunchFacts::from_private_argument(payload, root)
                .expect_err("missing paths or duplicate result");
            assert_eq!(error.code(), "KELD-CORE-037");
        }
    }
    fn policy_descriptor_fixture() -> serde_json::Value {
        serde_json::json!({
            "contract": ORIGINAL_RESOURCE_POLICY_CONTRACT,
            "graphSha256": format!("sha256:{}", "1".repeat(64)),
            "policySha256": format!("sha256:{}", "2".repeat(64)),
            "windowKey": "editor",
            "selector": keld_guard::WindowCspProfile::OriginalEditorOffline.as_str(),
        })
    }

    #[test]
    fn schema_five_roundtrips_only_closed_policy_data_and_rejects_older_schema_insertion() {
        let descriptor = policy_descriptor_fixture();
        let payload = serde_json::json!({
            "schema":5,"kind":"development","name":"n","root":"/stage","paths":{},
            "singleInstance":{"verdict":"acquired"},"originalResourcePolicy":descriptor,
        });
        let parsed = DevelopmentLaunchFacts::from_private_argument(
            &payload.to_string(),
            Path::new("/stage"),
        )
        .expect("closed private data fixture, not native proof");
        let observed: serde_json::Value =
            serde_json::from_str(&parsed.private_argument().expect("encode")).expect("JSON");
        assert_eq!(observed, payload);
        for schema in 1..=4 {
            let mut old = payload.clone();
            old["schema"] = serde_json::json!(schema);
            assert!(
                DevelopmentLaunchFacts::from_private_argument(
                    &old.to_string(),
                    Path::new("/stage")
                )
                .is_err(),
                "schema{schema} cannot carry eligibility data"
            );
        }
        assert!(
            DevelopmentLaunchFacts::from_private_argument(
                &payload.to_string(),
                Path::new("/foreign")
            )
            .is_err(),
            "private record root must bind the actual admitted stage"
        );
    }

    #[test]
    fn schema_five_refuses_missing_malformed_or_forged_descriptor_fields() {
        let base = serde_json::json!({"schema":5,"kind":"development","name":"n","root":"/stage","paths":{}});
        assert!(
            DevelopmentLaunchFacts::from_private_argument(&base.to_string(), Path::new("/stage"))
                .is_err(),
            "schema5 requires the source descriptor"
        );
        let mut variants = vec![
            serde_json::Value::Null,
            serde_json::json!([]),
            serde_json::json!({}),
            serde_json::json!(true),
        ];
        for (key, value) in [
            ("contract", serde_json::json!("forged")),
            ("selector", serde_json::json!("default")),
            ("graphSha256", serde_json::json!("sha256:0")),
            (
                "policySha256",
                serde_json::json!(format!("sha256:{}", "A".repeat(64))),
            ),
            ("windowKey", serde_json::json!("")),
            ("grant", serde_json::json!("all")),
        ] {
            let mut descriptor = policy_descriptor_fixture();
            descriptor[key] = value;
            variants.push(descriptor);
        }
        for descriptor in variants {
            let mut payload = base.clone();
            payload["originalResourcePolicy"] = descriptor;
            let error = DevelopmentLaunchFacts::from_private_argument(
                &payload.to_string(),
                Path::new("/stage"),
            )
            .expect_err("invalid private claim");
            assert_eq!(error.code(), "KELD-CORE-037");
        }
    }
}
