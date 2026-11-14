//! Independent first-author-script snapshot and actual native-listener receipts.
use serde_json::Value;

pub(super) fn assert_first_author_script(page: &Value, hello: &Value) {
    assert!(page["error"].is_null(), "page failure: {page}");
    assert_first_author_snapshot(&page["first"], hello);
}

pub(super) fn assert_first_author_snapshot(first: &Value, hello: &Value) {
    assert_eq!(first["readyState"], "loading");
    assert_eq!(first["firstAuthorScript"], true);
    assert_eq!(first["authorScriptCount"], 1);
    assert_eq!(first["top"], true);
    assert_eq!(first["processType"], "renderer");
    for name in [
        "request",
        "registerMsgListener",
        "sendMessage",
        "listenOnce",
    ] {
        assert_eq!(
            first["methods"][name], "function",
            "first author script {name}"
        );
    }
    // The unchanged preload consumes its lexical argv. It does not publish argv;
    // the observed one-shot localStorage effect is the independent argv oracle.
    assert_eq!(first["adaptiveColors"], "auto");
    assert!(first["configurationError"].is_null());
    let versions = first["versions"]
        .as_object()
        .expect("first-author-script versions");
    assert!(!versions.is_empty());
    assert_eq!(versions.get("bun"), hello["versions"].get("bun"));
    assert!(
        versions
            .get("bun")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
    );
    for (key, value) in versions {
        assert_eq!(value, &hello["versions"][key], "actual Bun version {key}");
    }
}

pub(super) fn assert_native_terminal(page: &Value, mode: &str, permitted: bool) {
    assert_eq!(page["mode"], mode);
    assert_eq!(page["ready"], true);
    assert_eq!(page["issued"], true);
    let records = page["rawRecords"]
        .as_array()
        .expect("actual port listener records");
    assert!(!records.is_empty() && records.len() <= 12);
    let actual: Vec<Value> = records
        .iter()
        .map(|record| {
            assert_eq!(record["sourceIsNativePort"], true);
            serde_json::from_str(record["text"].as_str().expect("raw native text"))
                .expect("actual Host JSON")
        })
        .collect();
    let terminal = &page["terminal"];
    assert!(
        actual.iter().any(|record| record == terminal),
        "terminal must be present in independent raw listener: {page}"
    );
    let document = terminal["document"].as_str().expect("actual document");
    assert_eq!(document.len(), 32);
    assert!(
        document
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    if mode == "raw" {
        assert_eq!(page["rawChallenge"], document);
        assert!(
            actual.iter().any(|record| record["kind"] == "app-call"
                && record["call"] == 1
                && record["method"] == "registerMsgListener"
                && record["document"] == document),
            "actual challenge missing"
        );
    }
    if permitted {
        assert_eq!(terminal["kind"], "send-result");
        assert_eq!(terminal["message"], 1);
        assert_eq!(terminal["disposition"]["status"], "consumed");
        assert!(
            actual.iter().any(|record| record["kind"] == "app-call"
                && record["call"] == page["expectedCall"]
                && record["method"] == "sendMessage"
                && record["document"] == document),
            "actual original Function entry missing"
        );
    } else {
        let preflight =
            terminal["kind"] == "app-refused" && terminal["call"] == page["expectedCall"];
        let send = terminal["kind"] == "send-result"
            && terminal["message"] == 1
            && terminal["disposition"]["status"] == "refused";
        assert!(
            preflight || send,
            "actual typed native refusal required: {terminal}"
        );
    }
}
