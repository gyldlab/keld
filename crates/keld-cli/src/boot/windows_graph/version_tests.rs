use super::{APP, Fixture, write};

#[test]
fn real_graph_stage_carries_explicit_version_and_never_uses_package_version() {
    for version in [None, Some("31.7.0-selected")] {
        let f = Fixture::new();
        if let Some(version) = version {
            write(
                &f.project,
                "keld.config.ts",
                APP.replace(
                    "initialWindow:'none',",
                    &format!("initialWindow:'none',version:'{version}',"),
                )
                .as_bytes(),
            );
        }
        let stage = f.stage().expect("existing internal provider fixture only");
        let bytes =
            std::fs::read(stage.root().join("keld.boot.json")).expect("actual staged descriptor");
        let document: serde_json::Value = serde_json::from_slice(&bytes).expect("closed JSON");
        assert_eq!(
            document.get("version").and_then(serde_json::Value::as_str),
            version
        );
        assert_eq!(document["schema"], 8);
        assert_eq!(document["entry"], "main/app.mjs");
        assert_eq!(
            document.as_object().expect("object").len(),
            if version.is_some() { 8 } else { 7 }
        );
        drop(stage);
    }
}

#[test]
fn invalid_literal_versions_refuse_before_any_stage_creation() {
    for declaration in [
        "version:''".to_owned(),
        "version:null".to_owned(),
        "version:process.env.APP_VERSION".to_owned(),
        "version:'first',version:'second'".to_owned(),
        "version:'nul\\0value'".to_owned(),
        format!("version:'{}'", "x".repeat(4097)),
        format!("version:'{}'", "λ".repeat(2049)),
    ] {
        let f = Fixture::new();
        write(
            &f.project,
            "keld.config.ts",
            APP.replace(
                "initialWindow:'none',",
                &format!("initialWindow:'none',{declaration},"),
            )
            .as_bytes(),
        );
        assert!(f.stage().is_err(), "accepted {declaration}");
        assert!(!f.project.join(".keld").exists());
    }
}
