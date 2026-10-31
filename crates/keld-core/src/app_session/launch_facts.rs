//! Non-authority development-launch data retained across the private Guardian boundary.

use std::path::{Component, Path};

use objc2_foundation::{
    NSSearchPathDirectory, NSSearchPathDomainMask, NSSearchPathForDirectoriesInDomains,
};
use serde::{Deserialize, Serialize};

use super::HostAppError;

const MAX_RECORD_BYTES: usize = 16 * 1024;
const MAX_FIELD_BYTES: usize = 4096;

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
        deserialize_with = "deserialize_present_object"
    )]
    paths: Option<LaunchDirectories>,
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
        let facts = Self {
            schema: 2,
            kind: String::from("development"),
            name: name.to_owned(),
            root: root_text.to_owned(),
            version: version.map(str::to_owned),
            paths: Some(LaunchDirectories::capture(name)?),
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
        if !matches!(self.schema, 1 | 2)
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
            || (self.schema == 2 && self.paths.is_none())
        {
            return Err(invalid(
                "launch facts fields do not match their closed schema",
            ));
        }
        if let Some(version) = &self.version {
            crate::hello::validate_app_version(version)
                .map_err(|source| invalid(source.to_string()))?;
        }
        if let Some(paths) = &self.paths {
            paths.validate(&self.name)?;
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
}
