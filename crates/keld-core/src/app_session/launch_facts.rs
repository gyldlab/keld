//! Closed launch-fact data and exact installed bootstrap inputs (GH-1404 §4.5).
//!
//! The codec extends the GH-452 owner at PR1362 c2dd2ff5. Schemas 1-3 retain
//! their development meaning. Independent development work owns 4 and 5;
//! the installed `LocalDevelopment` variant is 6. This module grants no access.

use std::path::{Component, Path};

use objc2_foundation::{
    NSLocale, NSSearchPathDirectory, NSSearchPathDomainMask, NSSearchPathForDirectoriesInDomains,
};
use serde::{Deserialize, Serialize};

use super::HostAppError;
use super::macos_installed::ValidatedMacLocalBundle;
use super::original_webapp::PreparedOriginalSource;
use super::single_instance::SingleInstanceVerdict;

const MAX_RECORD_BYTES: usize = 16 * 1024;
const MAX_FIELD_BYTES: usize = 4096;
const ORIGINAL_RESOURCE_POLICY_CONTRACT: &str = "gh473-original-editor-offline/v1";

/// Fixed generated preload below the independently verified application root.
pub const MACOS_LOCAL_LAUNCH_PRELOAD_PATH: &str =
    "keld-launch-facts/electron/src/installed-launch-preload.ts";

/// Exact source inputs shared by the local producer and installed admission.
///
/// These are compiled owner bytes, not files or fingerprints supplied by the
/// application. The relative layout preserves the source modules' imports.
#[must_use]
pub const fn macos_local_launch_bootstrap_sources() -> [(&'static str, &'static [u8]); 3] {
    [
        (
            "keld-launch-facts/kipc/src/launch-facts.ts",
            include_bytes!("../../../../packages/@keld/kipc/src/launch-facts.ts"),
        ),
        (
            "keld-launch-facts/electron/src/process-facts.ts",
            include_bytes!("../../../../packages/@keld/electron/src/process-facts.ts"),
        ),
        (
            MACOS_LOCAL_LAUNCH_PRELOAD_PATH,
            include_bytes!("../../../../packages/@keld/electron/src/installed-launch-preload.ts"),
        ),
    ]
}

/// One closed Host record. Construction requires an independently admitted origin.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LaunchFacts {
    schema: u8,
    kind: String,
    name: String,
    root: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
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
        rename = "resourcesRoot",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    resources_root: Option<String>,
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SingleInstanceFact {
    verdict: SingleInstanceVerdict,
}

/// A description of an independently prepared source, never a native grant.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct OriginalResourcePolicyDescriptor {
    contract: String,
    graph_sha256: String,
    policy_sha256: String,
    window_key: String,
    selector: String,
}

impl OriginalResourcePolicyDescriptor {
    fn from_verified(graph: [u8; 32], policy: &keld_guard::ResolvedWindowCsp) -> Self {
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
            graph_sha256: digest_text(graph),
            policy_sha256: digest_text(policy.verified_sha256()),
            window_key: policy.window_key().to_owned(),
            selector: policy.profile().as_str().to_owned(),
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
            || ![
                keld_guard::WindowCspProfile::OriginalEditorOffline.as_str(),
                keld_guard::WindowCspProfile::OriginalEditorOfflineEmptyStyle.as_str(),
            ]
            .contains(&self.selector.as_str())
            || !digest(&self.graph_sha256)
            || !digest(&self.policy_sha256)
            || !valid_text(&self.window_key)
        {
            return Err(invalid("invalid original resource policy descriptor"));
        }
        Ok(())
    }
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
    // Omission is absence. A present JSON null cannot erase a fact.
    T::deserialize(deserializer).map(Some)
}

fn deserialize_object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ObjectVisitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a closed Host launch-facts JSON object")
        }

        fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    // Derived structs alone also accept positional arrays. Require an object.
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

fn valid_native_path(value: &str) -> bool {
    valid_path(value)
        && (value == "/"
            || value.strip_prefix('/').is_some_and(|rest| {
                rest.split('/')
                    .all(|part| !part.is_empty() && part != "." && part != "..")
            }))
}

pub(super) fn require_safe_installed_name(name: &str) -> Result<(), HostAppError> {
    let mut parts = Path::new(name).components();
    if !valid_text(name)
        || !matches!(parts.next(), Some(Component::Normal(part)) if part == name)
        || parts.next().is_some()
    {
        return Err(invalid(
            "installed application name must be one normal filename component",
        ));
    }
    Ok(())
}

fn native_directory(directory: NSSearchPathDirectory) -> Result<Option<String>, HostAppError> {
    let paths = NSSearchPathForDirectoriesInDomains(
        directory,
        NSSearchPathDomainMask::UserDomainMask,
        true,
    );
    select_native_directory(paths.len(), || {
        paths.firstObject().map(|path| path.to_string())
    })
}

fn select_native_directory(
    count: usize,
    selected: impl FnOnce() -> Option<String>,
) -> Result<Option<String>, HostAppError> {
    match count {
        0 => return Ok(None),
        1 => {}
        _ => {
            return Err(invalid(
                "native directory query returned multiple candidates",
            ));
        }
    }
    let value =
        selected().ok_or_else(|| invalid("native directory query lost its selected value"))?;
    if !valid_native_path(&value) {
        return Err(invalid(
            "native directory is not a canonical bounded absolute path",
        ));
    }
    Ok(Some(value))
}

fn user_data_path(app_data: Option<&str>, name: &str) -> Option<String> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(component)) if component == name)
        || components.next().is_some()
    {
        return None;
    }
    Path::new(app_data?)
        .join(name)
        .to_str()
        .filter(|path| valid_path(path))
        .map(str::to_owned)
}

impl LaunchDirectories {
    fn capture(name: &str) -> Result<Self, HostAppError> {
        require_safe_installed_name(name)?;
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

impl LaunchFacts {
    pub(super) fn from_validated_local(
        origin: &ValidatedMacLocalBundle,
    ) -> Result<Self, HostAppError> {
        let path_text = |path: &Path| {
            path.to_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("verified local launch path is not UTF-8"))
        };
        let facts = Self {
            schema: 6,
            kind: String::from("local-development"),
            name: origin.app_name().to_owned(),
            root: path_text(origin.app_root())?,
            version: Some(origin.app_version().to_owned()),
            locale: None,
            paths: None,
            resources_root: Some(path_text(origin.resources_root())?),
            single_instance: None,
            original_resource_policy: None,
        };
        // Use the one closed codec even for the trusted local constructor.
        Self::from_private_argument(&facts.private_argument()?, origin.app_root())
    }

    pub(super) fn from_validated_original(
        origin: &ValidatedMacLocalBundle,
        prepared: &PreparedOriginalSource,
    ) -> Result<Self, HostAppError> {
        require_safe_installed_name(origin.app_name())?;
        if prepared.selected_stage() != origin.app_root() {
            return Err(invalid(
                "prepared original source belongs to a different installed root",
            ));
        }
        let mut facts = Self::from_validated_local(origin)?;
        facts.schema = 7;
        facts.paths = Some(LaunchDirectories::capture(origin.app_name())?);
        facts.locale = NSLocale::preferredLanguages()
            .firstObject()
            .map(|language| language.to_string());
        facts.original_resource_policy = Some(OriginalResourcePolicyDescriptor::from_verified(
            prepared.graph_sha256(),
            prepared.source().resolved_window_policy(),
        ));
        facts.validate(origin.app_root())?;
        Ok(facts)
    }

    pub(super) fn application_support(&self) -> Option<&str> {
        self.paths.as_ref()?.app_data.as_deref()
    }

    pub(super) fn set_single_instance(
        &mut self,
        verdict: SingleInstanceVerdict,
    ) -> Result<(), HostAppError> {
        if self.schema != 7 || self.single_instance.is_some() {
            return Err(invalid(
                "single-instance verdict requires one installed original facts owner",
            ));
        }
        self.single_instance = Some(SingleInstanceFact { verdict });
        self.validate(Path::new(&self.root))?;
        self.private_argument()?;
        Ok(())
    }

    /// Called only by the authenticated Guardian's current-process admission.
    pub(super) fn from_authenticated_installed(
        text: &str,
        origin: &ValidatedMacLocalBundle,
        graph_sha256: [u8; 32],
        policy: &keld_guard::ResolvedWindowCsp,
        single_instance_declared: bool,
    ) -> Result<Self, HostAppError> {
        let facts = Self::from_private_argument(text, origin.app_root())?;
        let expected = OriginalResourcePolicyDescriptor::from_verified(graph_sha256, policy);
        if facts.schema != 7
            || facts.name != origin.app_name()
            || facts.version.as_deref() != Some(origin.app_version())
            || facts.resources_root.as_deref().map(Path::new) != Some(origin.resources_root())
            || facts.original_resource_policy.as_ref() != Some(&expected)
        {
            return Err(invalid(
                "authenticated Host facts differ from the Guardian's independently verified installed source",
            ));
        }
        facts.validate_single_instance_declaration(single_instance_declared)?;
        Ok(facts)
    }

    fn validate_single_instance_declaration(&self, declared: bool) -> Result<(), HostAppError> {
        if self.single_instance.is_some() != declared {
            return Err(invalid(
                "authenticated Host single-instance verdict differs from the sealed declaration",
            ));
        }
        Ok(())
    }

    fn from_private_argument(text: &str, expected_root: &Path) -> Result<Self, HostAppError> {
        if text.len() > MAX_RECORD_BYTES {
            return Err(invalid("Host launch facts exceed 16 KiB"));
        }
        let mut decoder = serde_json::Deserializer::from_str(text);
        let facts: Self = deserialize_object(&mut decoder)
            .map_err(|source| invalid(format!("Host launch facts JSON: {source}")))?;
        decoder
            .end()
            .map_err(|source| invalid(format!("Host launch facts trailing input: {source}")))?;
        facts.validate(expected_root)?;
        Ok(facts)
    }

    pub(super) fn private_argument(&self) -> Result<String, HostAppError> {
        let text = serde_json::to_string(self)
            .map_err(|source| invalid(format!("Host launch facts encoding: {source}")))?;
        if text.len() > MAX_RECORD_BYTES {
            return Err(invalid("Host launch facts exceed 16 KiB"));
        }
        Ok(text)
    }

    pub(super) fn bun_define_argument(&self) -> Result<String, HostAppError> {
        Ok(format!(
            "--define=__KELD_HOST_LAUNCH_FACTS__:{}",
            self.private_argument()?
        ))
    }

    fn validate(&self, expected_root: &Path) -> Result<(), HostAppError> {
        if !valid_text(&self.name)
            || !valid_path(&self.root)
            || Path::new(&self.root) != expected_root
            || self
                .version
                .as_ref()
                .is_some_and(|value| !valid_text(value))
            || self.locale.as_ref().is_some_and(|value| !valid_text(value))
        {
            return Err(invalid("invalid launch facts or mismatched verified root"));
        }
        match self.schema {
            1..=3 => {
                if self.kind != "development"
                    || self.resources_root.is_some()
                    || self.single_instance.is_some()
                    || self.original_resource_policy.is_some()
                    || (self.schema == 1 && (self.version.is_some() || self.paths.is_some()))
                    || (matches!(self.schema, 2 | 3) && self.paths.is_none())
                    || (self.schema != 3 && self.locale.is_some())
                {
                    return Err(invalid(
                        "development fields do not match their closed schema",
                    ));
                }
                if let Some(paths) = &self.paths {
                    paths.validate(&self.name)?;
                }
            }
            6 => {
                let resources = self.resources_root.as_deref().ok_or_else(|| {
                    invalid("installed launch facts require the verified resources root")
                })?;
                if self.kind != "local-development"
                    || self.version.is_none()
                    || self.locale.is_some()
                    || self.paths.is_some()
                    || self.single_instance.is_some()
                    || self.original_resource_policy.is_some()
                    || !valid_path(resources)
                    || Path::new(resources).join("app") != expected_root
                {
                    return Err(invalid("installed fields do not match their closed schema"));
                }
            }
            7 => {
                require_safe_installed_name(&self.name)?;
                let resources = self
                    .resources_root
                    .as_deref()
                    .ok_or_else(|| invalid("installed original facts require resourcesRoot"))?;
                let paths = self
                    .paths
                    .as_ref()
                    .ok_or_else(|| invalid("installed original facts require paths"))?;
                let policy = self.original_resource_policy.as_ref().ok_or_else(|| {
                    invalid("installed original facts require originalResourcePolicy")
                })?;
                if self.kind != "local-development"
                    || self.version.is_none()
                    || !valid_native_path(&self.root)
                    || !valid_native_path(resources)
                    || Path::new(resources).join("app") != expected_root
                    || [&paths.app_data, &paths.user_data, &paths.documents]
                        .into_iter()
                        .flatten()
                        .any(|value| !valid_native_path(value))
                    || (self.single_instance.is_some() && paths.app_data.is_none())
                {
                    return Err(invalid(
                        "installed original fields do not match their closed schema",
                    ));
                }
                paths.validate(&self.name)?;
                policy.validate()?;
            }
            _ => return Err(invalid("unsupported Host launch-facts schema")),
        }
        Ok(())
    }
}

fn invalid(detail: impl Into<String>) -> HostAppError {
    HostAppError::new(
        "KELD-CORE-037",
        "Host launch facts",
        detail,
        "Launch an intact local bundle through its validated Host; do not forge startup facts.",
    )
}

#[cfg(test)]
mod tests {
    use super::LaunchFacts;
    use std::path::Path;

    const ORIGINAL_FIXTURE: &str = include_str!(
        "../../../../packages/@keld/kipc/test/fixtures/installed-launch-facts-v7.json"
    );

    fn original() -> serde_json::Value {
        serde_json::from_str(ORIGINAL_FIXTURE).expect("shared literal schema-7 fixture")
    }

    #[test]
    fn installed_original_facts_share_the_exact_codec_fixture_and_roundtrip() {
        let input = original();
        let root = Path::new(input["root"].as_str().expect("fixture root"));
        let facts =
            LaunchFacts::from_private_argument(ORIGINAL_FIXTURE, root).expect("closed schema7");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                &facts.private_argument().expect("bounded record")
            )
            .expect("encoded JSON"),
            input
        );
        assert_eq!(
            facts.application_support(),
            Some("/Users/native/Library/Application Support")
        );
        assert!(
            LaunchFacts::from_private_argument(ORIGINAL_FIXTURE, Path::new("/foreign/app"))
                .is_err()
        );
    }

    #[test]
    fn installed_original_descriptor_retains_the_exact_opt_in_selector() {
        let mut value = original();
        value["originalResourcePolicy"]["selector"] =
            serde_json::json!("original-editor-offline-empty-style");
        let root = Path::new(value["root"].as_str().expect("fixture root"));
        let captured = LaunchFacts::from_private_argument(&value.to_string(), root)
            .expect("closed installed original selector");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                &captured.private_argument().expect("encoded facts")
            )
            .expect("JSON"),
            value
        );
        for selector in [
            "default",
            "unsafe-inline",
            "original-editor-offline-empty-style-extra",
        ] {
            let mut changed = value.clone();
            changed["originalResourcePolicy"]["selector"] = serde_json::json!(selector);
            assert!(LaunchFacts::from_private_argument(&changed.to_string(), root).is_err());
        }
    }

    #[test]
    fn native_directory_cardinality_requires_one_unique_value() {
        assert_eq!(
            super::select_native_directory(0, || panic!("empty query must not select a value"))
                .expect("empty native query omits the optional fact"),
            None
        );
        assert_eq!(
            super::select_native_directory(1, || Some(String::from("/native/Application Support")))
                .expect("one canonical native value"),
            Some(String::from("/native/Application Support"))
        );
        for count in [2, usize::MAX] {
            let error = super::select_native_directory(count, || {
                panic!("ambiguous native query must not select the first value")
            })
            .expect_err("multiple native directory candidates must fail");
            assert_eq!(error.code(), "KELD-CORE-037");
        }
        assert!(super::select_native_directory(1, || None).is_err());
        assert!(super::select_native_directory(1, || Some(String::from("relative"))).is_err());
    }

    #[test]
    fn installed_original_locale_preserves_raw_utf8_and_ordinary_byte_bound() {
        let input = original();
        let root = Path::new(input["root"].as_str().expect("fixture root"));
        for locale in [
            String::from("en_US"),
            String::from("EN-gb"),
            String::from("é"),
            "x".repeat(129),
            "é".repeat(2048),
        ] {
            let mut changed = input.clone();
            changed["locale"] = serde_json::Value::String(locale.clone());
            let facts = LaunchFacts::from_private_argument(&changed.to_string(), root)
                .expect("bounded raw native locale remains data");
            assert_eq!(facts.locale.as_deref(), Some(locale.as_str()));
        }
    }

    #[test]
    fn authenticated_instance_verdict_requires_native_application_support() {
        let input = original();
        let root = Path::new(input["root"].as_str().expect("fixture root"));
        for verdict in ["acquired", "contended"] {
            let mut changed = input.clone();
            changed["singleInstance"]["verdict"] = serde_json::json!(verdict);
            let facts = LaunchFacts::from_private_argument(&changed.to_string(), root)
                .expect("native application support is available");
            facts
                .validate_single_instance_declaration(true)
                .expect("both actual acquisition outcomes carry the native namespace");
            assert!(facts.validate_single_instance_declaration(false).is_err());

            for paths in [
                serde_json::json!({}),
                serde_json::json!({"documents":"/native/Documents"}),
            ] {
                changed["paths"] = paths;
                let error = LaunchFacts::from_private_argument(&changed.to_string(), root)
                    .expect_err("the generic decoder must refuse a verdict without native appData");
                assert_eq!(error.code(), "KELD-CORE-037");
            }
        }
    }

    #[test]
    fn undeclared_instance_can_omit_unavailable_native_directories() {
        let mut input = original();
        input
            .as_object_mut()
            .expect("object")
            .remove("singleInstance");
        input["paths"] = serde_json::json!({});
        let root = Path::new(input["root"].as_str().expect("fixture root"));
        let facts = LaunchFacts::from_private_argument(&input.to_string(), root)
            .expect("no declaration requires no native lock namespace");
        facts
            .validate_single_instance_declaration(false)
            .expect("unavailable paths remain valid for undeclared locks");
        let error = facts
            .validate_single_instance_declaration(true)
            .expect_err("a sealed declaration cannot lose its acquisition verdict");
        assert_eq!(error.code(), "KELD-CORE-037");
    }

    #[test]
    fn installed_original_facts_reject_missing_mixed_and_noncanonical_fields() {
        let input = original();
        let root = Path::new(input["root"].as_str().expect("fixture root"));
        for field in [
            "name",
            "version",
            "root",
            "resourcesRoot",
            "paths",
            "originalResourcePolicy",
        ] {
            let mut changed = input.clone();
            changed.as_object_mut().expect("object").remove(field);
            assert!(
                LaunchFacts::from_private_argument(&changed.to_string(), root).is_err(),
                "missing {field}"
            );
        }
        for field in [
            "version",
            "paths",
            "locale",
            "singleInstance",
            "originalResourcePolicy",
        ] {
            let mut changed = input.clone();
            changed[field] = serde_json::Value::Null;
            assert!(
                LaunchFacts::from_private_argument(&changed.to_string(), root).is_err(),
                "null {field}"
            );
        }
        for name in [".", "..", "nested/name", "name/", ""] {
            let mut changed = input.clone();
            changed["name"] = serde_json::json!(name);
            assert!(
                LaunchFacts::from_private_argument(&changed.to_string(), root).is_err(),
                "name {name}"
            );
        }
        for path in ["relative", "//tmp", "/tmp/", "/tmp/./x", "/tmp/../x"] {
            let mut changed = input.clone();
            changed["paths"]["documents"] = serde_json::json!(path);
            assert!(
                LaunchFacts::from_private_argument(&changed.to_string(), root).is_err(),
                "path {path}"
            );
        }
        for locale in [
            String::new(),
            String::from("\u{0}"),
            "x".repeat(4097),
            "é".repeat(2049),
        ] {
            let mut changed = input.clone();
            changed["locale"] = serde_json::json!(locale);
            assert!(
                LaunchFacts::from_private_argument(&changed.to_string(), root).is_err(),
                "locale {locale:?}"
            );
        }
        let mut changed = input.clone();
        changed["singleInstance"]["verdict"] = serde_json::json!("failed");
        assert!(LaunchFacts::from_private_argument(&changed.to_string(), root).is_err());
        changed = input.clone();
        changed["originalResourcePolicy"]["graphSha256"] =
            serde_json::json!(format!("sha256:{}", "A".repeat(64)));
        assert!(LaunchFacts::from_private_argument(&changed.to_string(), root).is_err());
    }

    #[test]
    fn installed_original_facts_reject_duplicate_keys_objects_as_arrays_and_unknown_authority() {
        let input = original();
        let root = Path::new(input["root"].as_str().expect("fixture root"));
        let text = input.to_string();
        for malformed in [
            text.replacen("\"schema\":7", "\"schema\":7,\"schema\":7", 1),
            text.replacen(
                "\"verdict\":\"acquired\"",
                "\"verdict\":\"acquired\",\"verdict\":\"acquired\"",
                1,
            ),
            format!("{text} false"),
            " ".repeat(16_385),
        ] {
            assert!(LaunchFacts::from_private_argument(&malformed, root).is_err());
        }
        for field in ["paths", "singleInstance", "originalResourcePolicy"] {
            let mut changed = input.clone();
            changed[field] = serde_json::json!([]);
            assert!(LaunchFacts::from_private_argument(&changed.to_string(), root).is_err());
            changed = input.clone();
            changed[field]["grant"] = serde_json::json!("all");
            assert!(LaunchFacts::from_private_argument(&changed.to_string(), root).is_err());
        }
    }

    fn installed() -> serde_json::Value {
        serde_json::json!({
            "schema": 6,
            "kind": "local-development",
            "name": "Facts",
            "root": "/Applications/Facts.app/Contents/Resources/app",
            "version": "1.2.3",
            "resourcesRoot": "/Applications/Facts.app/Contents/Resources"
        })
    }

    const ROOT: &str = "/Applications/Facts.app/Contents/Resources/app";

    #[test]
    fn installed_launch_facts_roundtrip_the_closed_local_variant() {
        let input = installed();
        let facts = LaunchFacts::from_private_argument(&input.to_string(), Path::new(ROOT))
            .expect("the exact installed record");
        let encoded = facts.private_argument().expect("bounded record");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&encoded).expect("JSON"),
            input
        );
        assert_eq!(
            facts.bun_define_argument().expect("one argv element"),
            format!("--define=__KELD_HOST_LAUNCH_FACTS__:{encoded}")
        );
        assert!(LaunchFacts::from_private_argument(&encoded, Path::new("/foreign/app")).is_err());
    }

    #[test]
    fn installed_launch_facts_refuse_unknown_mixed_missing_and_forged_fields() {
        let input = installed();
        for field in ["name", "root", "version", "resourcesRoot"] {
            let mut missing = input.clone();
            missing.as_object_mut().expect("object").remove(field);
            assert!(
                LaunchFacts::from_private_argument(&missing.to_string(), Path::new(ROOT)).is_err(),
                "missing {field}"
            );
            let mut null = input.clone();
            null[field] = serde_json::Value::Null;
            assert!(
                LaunchFacts::from_private_argument(&null.to_string(), Path::new(ROOT)).is_err(),
                "null {field}"
            );
        }
        for (field, value) in [
            ("kind", serde_json::json!("development")),
            ("root", serde_json::json!("/foreign/app")),
            ("resourcesRoot", serde_json::json!("/foreign")),
            ("version", serde_json::json!("")),
            ("version", serde_json::json!("v".repeat(4097))),
            ("paths", serde_json::json!({})),
            ("locale", serde_json::json!("en")),
            ("grant", serde_json::json!("all")),
        ] {
            let mut changed = input.clone();
            changed[field] = value;
            assert!(
                LaunchFacts::from_private_argument(&changed.to_string(), Path::new(ROOT)).is_err(),
                "{field}"
            );
        }
        for schema in [0, 1, 2, 3, 4, 5, 7, 255] {
            let mut changed = input.clone();
            changed["schema"] = serde_json::json!(schema);
            assert!(
                LaunchFacts::from_private_argument(&changed.to_string(), Path::new(ROOT)).is_err(),
                "schema {schema}"
            );
        }
        let text = input.to_string();
        for malformed in [
            format!("{text} false"),
            text.replacen("\"schema\":6", "\"schema\":6,\"schema\":6", 1),
            String::from("[6,\"local-development\",\"Facts\",\"/app\"]"),
        ] {
            assert!(LaunchFacts::from_private_argument(&malformed, Path::new(ROOT)).is_err());
        }
    }

    #[test]
    fn installed_extension_preserves_legacy_development_codec_without_relabeling_it() {
        for schema in 1..=3 {
            let mut value = serde_json::json!({
                "schema": schema, "kind": "development", "name": "Facts", "root": "/stage"
            });
            if schema >= 2 {
                value["version"] = serde_json::json!("v1");
                value["paths"] =
                    serde_json::json!({"appData":"/native","userData":"/native/Facts"});
            }
            if schema == 3 {
                value["locale"] = serde_json::json!("fr-CA");
            }
            let facts = LaunchFacts::from_private_argument(&value.to_string(), Path::new("/stage"))
                .expect("unchanged development schema");
            assert_eq!(facts.kind, "development");
            assert!(facts.resources_root.is_none());
            value["resourcesRoot"] = serde_json::json!("/stage");
            assert!(
                LaunchFacts::from_private_argument(&value.to_string(), Path::new("/stage"))
                    .is_err()
            );
        }
    }
}
