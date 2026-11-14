//! Oracles from complete actual native and fixture streams, after physical exit.
use serde_json::Value;

fn one<'a>(rows: &'a [(usize, Value)], phase: &str) -> (usize, &'a Value) {
    let selected: Vec<_> = rows
        .iter()
        .filter(|(_, value)| value["phase"] == phase)
        .collect();
    assert_eq!(
        selected.len(),
        1,
        "one actual {phase} transition: {selected:?}"
    );
    (selected[0].0, &selected[0].1)
}

fn native_records(trace: &str, view: u64) -> Vec<(usize, Value)> {
    let mut rows = Vec::new();
    for (line_number, line) in trace.lines().enumerate() {
        if let Some(text) = line.strip_prefix("KELD_ORIGINAL_STARTUP ") {
            assert!(rows.len() < 16, "bounded actual startup transitions");
            assert!(text.len() <= 4096, "bounded actual startup scalar record");
            let value: Value = serde_json::from_str(text).expect("actual startup JSON");
            rows.push((line_number, value));
        }
    }
    let (qualified_line, qualified) = one(&rows, "qualified");
    let generation = qualified["generation"].as_u64().expect("actual generation");
    let navigation = qualified["navigation"]
        .as_u64()
        .expect("actual native navigation");
    assert!(generation > 0 && navigation > 0);
    for key in ["qualifiedAt", "deadline"] {
        let text = qualified[key].as_str().expect("actual stored Instant");
        assert!(
            !text.is_empty() && text.len() <= 128,
            "bounded actual {key}"
        );
    }
    for (line_number, row) in &rows {
        assert_eq!(row["view"], view, "same Registry-associated native view");
        assert_eq!(row["generation"], generation, "same native generation");
        assert_eq!(row["navigation"], navigation, "same actual navigation");
        assert_eq!(row["current"], true, "actual current exact binding");
        assert_eq!(row["registered"], false, "no actual registration receipt");
        assert_eq!(row["retired"], false, "no earlier view retirement");
        assert!(
            row["unavailable"].is_null(),
            "actual observation unavailable"
        );
        // The fixed-clock falsifier compares actual stored fields, not elapsed
        // test time or a copied timer. Five seconds is the accepted contract.
        assert_eq!(
            row["deadlineAfterQualificationNs"], 5_000_000_000_u64,
            "X5 fixed qualification deadline (actual stored field)"
        );
        assert_eq!(
            row["qualifiedAt"], qualified["qualifiedAt"],
            "qualification never renewed"
        );
        assert_eq!(
            row["deadline"], qualified["deadline"],
            "same absolute stored deadline"
        );
        if row["phase"] != "qualified" {
            assert!(
                *line_number > qualified_line,
                "qualification precedes its observations"
            );
        }
        assert_eq!(row["faulted"], row["phase"] == "fault_latched");
        if row["phase"] != "dom_callback" {
            assert!(row["nativeNavigation"].is_null());
        }
        if row["phase"] != "bind_intake" {
            assert!(row["inputSameDocument"].is_null());
        }
    }
    rows
}

fn assert_native_order(trace: &str, rows: &[(usize, Value)]) {
    let (_, qualified) = one(rows, "qualified");
    let (initial_line, initial) = one(rows, "initial_returned");
    let (dom_line, dom) = one(rows, "dom_callback");
    let (retry_line, retry) = one(rows, "dom_retry_returned");
    let (expired_line, expired) = one(rows, "expired");
    let (fault_line, fault) = one(rows, "fault_latched");
    let binds: Vec<_> = rows
        .iter()
        .filter(|(_, value)| value["phase"] == "bind_intake")
        .collect();
    assert_eq!(binds.len(), 2, "two actual duplicate-bind intakes");
    assert_eq!(
        rows.len(),
        8,
        "only six native milestones and two bind intakes"
    );
    for key in ["initialEntered", "domObserved", "retryEntered"] {
        assert_eq!(qualified[key], false, "fresh receipt {key}");
    }
    assert_eq!(
        initial["initialEntered"], true,
        "actual initial native entry"
    );
    assert_eq!(
        initial["retryEntered"], false,
        "initial return is not a retry"
    );
    assert_eq!(
        dom["nativeNavigation"], qualified["navigation"],
        "actual DOM NavigationId"
    );
    assert_eq!(dom["domObserved"], true);
    assert_eq!(retry["initialEntered"], true);
    assert_eq!(retry["domObserved"], true);
    assert_eq!(retry["retryEntered"], true, "actual DOM retry native entry");
    assert!(retry_line > initial_line && retry_line > dom_line);
    for (line_number, row) in &binds {
        assert_eq!(
            row["inputSameDocument"], true,
            "actual captured same-document input"
        );
        assert_eq!(row["initialEntered"], true);
        assert!(*line_number < expired_line, "intake before actual expiry");
    }
    assert!(expired_line > retry_line && fault_line > expired_line);
    assert_eq!(expired["retryEntered"], true);
    assert_eq!(fault["retryEntered"], true);
    let retained_line = trace
        .lines()
        .position(|line| line == "KELD_T2_MAIN_CUSTODY retained=true")
        .expect("actual retained-run assignment");
    assert!(
        retained_line > fault_line,
        "custody recorded after the real failure"
    );
    assert!(!trace.contains("KELD_T2_RELEASE_WITNESS completed=true"));
}

fn actual_page(trace: &str, view: u64) -> Value {
    let prefix = format!("PAGE_READBACK {view} ");
    let mut latest = None;
    let mut page_records = 0_usize;
    for line in trace.lines() {
        let Some(text) = line.strip_prefix(&prefix) else {
            continue;
        };
        assert!(page_records < 512, "bounded selected native page readbacks");
        page_records += 1;
        assert!(text.len() <= 32768, "bounded fixed ExecuteScript result");
        let readback: Value = serde_json::from_str(text).expect("actual fixed page readback");
        assert_ne!(readback["originalProbeOversize"], true);
        if let Some(text) = readback["originalProbe"].as_str() {
            assert!(text.len() <= 4096, "bounded hostile probe");
            let page: Value = serde_json::from_str(text).expect("actual hostile page snapshot");
            assert!(page["error"].is_null(), "actual probe error: {page}");
            assert_eq!(page["appReady"], 0, "no page readiness receipt");
            latest = Some(page);
        }
    }
    let page = latest.expect("actual bounded page snapshot");
    assert_eq!(page["first"]["top"], true);
    assert_eq!(page["first"]["readyState"], "loading");
    assert_eq!(page["first"]["authorScriptCount"], 1);
    assert_eq!(page["first"]["firstAuthorScript"], true);
    assert_eq!(page["first"]["processType"], "renderer");
    for method in [
        "request",
        "registerMsgListener",
        "sendMessage",
        "listenOnce",
    ] {
        assert_eq!(
            page["first"]["methods"][method], "function",
            "canonical page facade"
        );
    }
    assert!(matches!(page["bounds"].as_u64(), Some(1 | 2)));
    assert_eq!(page["bindMessagesSent"], 2);
    assert_eq!(page["domObserved"], true);
    page
}

fn assert_fixture(control: &str, window: u64) {
    let mut created = 0_usize;
    let mut loads = 0_usize;
    for line in control.lines() {
        assert!(
            !line.starts_with("FIXTURE_ERROR "),
            "actual fixture failure: {line}"
        );
        for (prefix, count) in [("CREATED ", &mut created), ("LOAD_ENTERED ", &mut loads)] {
            if let Some(text) = line.strip_prefix(prefix) {
                assert!(text.len() <= 4096);
                let record: Value = serde_json::from_str(text).expect("actual fixture record");
                assert_eq!(record["id"], window);
                *count += 1;
            }
        }
    }
    assert_eq!(created, 1);
    assert_eq!(loads, 1);
}

pub(super) fn assert_missing_registration(trace: &str, control: &str, view: u64, window: u64) {
    let rows = native_records(trace, view);
    assert_native_order(trace, &rows);
    let page = actual_page(trace, view);
    assert_fixture(control, window);
    let (_, qualified) = one(&rows, "qualified");
    println!(
        "KELD_ORIGINAL_X5_NATIVE_CLOCK {}",
        serde_json::json!({
            "view":view,"window":window,"generation":qualified["generation"],
            "navigation":qualified["navigation"],"qualifiedAt":qualified["qualifiedAt"],
            "deadline":qualified["deadline"],
            "deadlineAfterQualificationNs":qualified["deadlineAfterQualificationNs"],
            "actualInitialReturns":1,"actualDomRetries":1,"actualBindIntakes":2,
            "registered":false,"retainedRun":true,"page":page,
        })
    );
}
