//! Pure receipt corruptions; these component controls grant no native acceptance.
use super::{CANARY, assert_complete};
use serde_json::{Value, json};

fn row(end: u64, value: Value) -> Value {
    let mut fields = serde_json::Map::new();
    fields.insert("end".into(), end.into());
    fields.insert("value".into(), value);
    Value::Object(fields)
}
fn state(generation: Option<u64>, commit: Option<u64>, depth: u64) -> Value {
    json!({"view":7,"generation":generation,"native_commit":commit,
        "callback_depth":depth,"posting":false,"reentry_failed":false})
}
fn nav(
    phase: &str,
    sequence: u64,
    generation: Option<u64>,
    commit: Option<u64>,
    depth: u64,
) -> Value {
    json!({"phase":phase,"sequence":sequence,"state":state(generation,commit,depth),
        "native_id":commit,"admitted":null,"call_active":false,"actual_api_calls":3,"invalid":false,"consumed":null})
}
fn owner(phase: &str, posted: bool) -> Value {
    json!({"phase":phase,"view":7,"navigation":11,"message":2,
        "post_phase":if posted {"returned"} else {"cancelled"},
        "retirement":if posted {Value::Null} else {json!("not-dispatched")},
        "retirement_sent":!posted,"released":matches!(phase,"release-observed" | "native-storage-dropped"),"actual_closed":false,"native_ok":null})
}
fn post(generation: u64, commit: u64, delivery: u64, sequence: u64, synthetic: bool) -> Value {
    let mut result = json!({
        "phase":"post-attempt","attempt_sequence":sequence,"decision_sequence":sequence+4,
        "target":state(Some(generation),Some(commit),0),
        "carrier_kind":"event","route_ready":null,
        "document_epoch":if generation == 11 {"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"} else {"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
        "event_storage":{"delivery":delivery,"payload_id":100,"payload_capacity":24,"wide_id":200,"wide_capacity":256},
        "role_live_before":true,"calls_before":3,"calls_after":4,
        "synthetic_enter":null,"synthetic_leave":null,
        "final_gate":{"sequence":sequence+1,"state":state(Some(generation),Some(commit),0)},
        "entry_sequence":sequence+2,"return_sequence":sequence+3,"unwind_sequence":null,
        "result":"returned","native_ok":true,"in_call_records":0,"in_call_max_depth":0,
        "first_in_call_sequence":null,"last_in_call_sequence":null,"call_active":false,"invalid":false,
    });
    if synthetic {
        result["calls_after"] = json!(3);
        result["entry_sequence"] = Value::Null;
        result["return_sequence"] = Value::Null;
        result["result"] = json!("not-admitted");
        result["native_ok"] = Value::Null;
        result["synthetic_enter"] =
            json!({"sequence":sequence+1,"state":state(Some(generation),Some(commit),1)});
        result["final_gate"] =
            json!({"sequence":sequence+2,"state":state(Some(generation),Some(commit),1)});
        result["synthetic_leave"] =
            json!({"sequence":sequence+5,"state":state(Some(generation),Some(commit),0)});
    }
    result
}
fn marker_post() -> Value {
    let mut result = post(12, 102, 3, 30, false);
    result["carrier_kind"] = json!("route-state");
    result["route_ready"] = json!(true);
    result["event_storage"] = Value::Null;
    result
}
fn native_string(value: &Value) -> Value {
    json!({"sourceIsNativePort":true,"text":value.to_string()})
}
fn b_page() -> Value {
    let document = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let terminal = json!({"v":2,"kind":"send-result","document":document,"message":1,"disposition":{"status":"consumed"}});
    json!({"documentKind":"B","first":{"firstAuthorScript":true,"authorScriptCount":1,"readyState":"loading","top":true},
        "mode":"facade","registered":true,"ready":true,"issued":true,"expectedCall":2,
        "routeMarker":document,"routeMarkerKind":"route-state","rawChallenge":document,"routeDelivery":null,"entered":0,"rawACanaries":0,"targetDelivery":null,"error":null,
        "terminal":terminal,"rawRecords":[
            native_string(&json!({"v":2,"kind":"route-state","document":document,"ready":true})),
            native_string(&json!({"v":2,"kind":"app-call","document":document,"call":2,"method":"sendMessage"})),
            native_string(&terminal)]})
}
fn page_readback(page: &Value, end: u64) -> Value {
    row(end, json!({"originalProbe":page.to_string()}))
}
fn fixture(case: &str) -> Value {
    let entered = case == "after-return";
    let a_commit = nav("content-committed", 2, Some(11), Some(101), 1);
    let mut b_start = nav("navigation-starting", 21, None, None, 1);
    b_start["native_id"] = json!(102);
    b_start["admitted"] = json!(true);
    let b_commit = nav("content-committed", 24, Some(12), Some(102), 1);
    let page = b_page();
    let mut receipt = json!({"schema":1,"case":case,
        "key":{"window":4,"incarnation":8,"view":7,"navigation":11,"message":2,"native_release_id":91,"retained":256},
        "page_a":{"documentKind":"A","routeDelivery":1,"routeMarkerKind":"event","routeMarker":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
        "page_b":page,"page_readbacks":[page_readback(&page,480)],
        "admission":row(100,json!({"phase":"event-admitted","window":4,"incarnation":8,"offers_closed":false,
            "scope":{"unknown_native_data":0,"unknown_native_controls":0,"native_storage_quiescent":false,
            "data":[{"webview":7,"navigation":11,"message":2,"native_release_id":91,"retained":256,"native_released":false}],"controls":[]}})),
        "a_commit":row(20,a_commit.clone()),"b_start":row(300,b_start.clone()),"b_commit":row(360,b_commit.clone()),
        "callbacks":[row(10,nav("callback-enter",1,Some(11),Some(101),1)),row(20,a_commit),
            row(30,nav("callback-leave",3,Some(11),Some(101),0)),
            row(290,nav("callback-enter",20,Some(11),Some(101),1)),row(300,b_start),
            row(310,nav("callback-leave",22,None,None,0)),row(350,nav("callback-enter",23,None,None,1)),
            row(360,b_commit),row(370,nav("callback-leave",25,Some(12),Some(102),0))],
        "target_posts":[],"marker_posts":[row(450,marker_post())],"consumed":[],"owner_posts":[],"hold":[],
        "ui_deliveries":[row(if case == "before-entry" {395} else {110},json!({"phase":"ui-delivery","kind":"event","view":7,"navigation":11,"message":2,"hold_active":entered}))],
        "retirements":[row(400,owner("retirement-handed-off",entered))],
        "releases":[row(410,owner("release-observed",entered))],"drops":[row(420,owner("native-storage-dropped",entered))],
        "controller_closes":[row(500,json!({"phase":"controller-closed","view":7,"actual_closed":true,"native_storage_quiescent":true}))],
        "view_drains":[row(520,json!({"phase":"view-drain","view":7,"drained":true}))],
        "quiescent_scopes":[row(510,json!({"phase":"storage-polled","window":4,"incarnation":8,"offers_closed":true,
            "scope":{"native_storage_quiescent":true,"unknown_native_data":0,"unknown_native_controls":0,"data":[],"controls":[]}}))],
    });
    if case == "synthetic-before-entry" {
        receipt["target_posts"] = json!([row(200, post(11, 101, 2, 9, true))]);
    } else {
        let mode = if entered {
            "after-next-event"
        } else {
            "before-intake"
        };
        receipt["hold"] = json!([
            row(
                if entered { 250 } else { 90 },
                json!({"phase":"held","mode":mode,"webview":7,"navigation":11,"delivery":2})
            ),
            row(390, json!({"phase":"released","mode":mode}))
        ]);
    }
    if entered {
        add_return(&mut receipt, false);
    }
    receipt
}
fn add_return(receipt: &mut Value, retry: bool) {
    let post = row(
        if retry { 240 } else { 200 },
        post(11, 101, 2, if retry { 15 } else { 10 }, false),
    );
    receipt["target_posts"]
        .as_array_mut()
        .expect("fixture posts")
        .push(post);
    let mut returned = owner("post-return", true);
    returned["native_ok"] = json!(true);
    receipt["owner_posts"] = json!([row(245, returned)]);
    let mut consumed = nav("consumer-observed", 27, Some(12), Some(102), 0);
    consumed["consumed"] = json!({"view":7,"generation":11,"native_commit":101,"delivery":2});
    receipt["consumed"] = json!([row(380, consumed)]);
    receipt["retirements"] = json!([]);
    receipt["releases"] = json!([row(410, owner("release-observed", true))]);
    receipt["drops"] = json!([row(420, owner("native-storage-dropped", true))]);
}
fn rejected(receipt: &Value) {
    assert!(std::panic::catch_unwind(|| assert_complete(receipt)).is_err());
}

#[test]
fn navigation_oracle_accepts_coherent_receipts_and_actual_retry() {
    for case in ["before-entry", "after-return", "synthetic-before-entry"] {
        assert_complete(&fixture(case));
    }
    let mut retried = fixture("synthetic-before-entry");
    add_return(&mut retried, true);
    assert_complete(&retried);
}
#[test]
fn navigation_oracle_rejects_zero_only_synthetic_receipt() {
    let mut receipt = fixture("synthetic-before-entry");
    receipt["target_posts"][0]["value"]["role_live_before"] = json!(false);
    rejected(&receipt);
}
#[test]
fn navigation_oracle_rejects_foreign_native_lease() {
    for field in ["target_posts", "marker_posts"] {
        let mut receipt = fixture("synthetic-before-entry");
        receipt[field][0]["value"]["final_gate"]["state"]["native_commit"] = json!(999);
        rejected(&receipt);
    }
}
#[test]
fn navigation_oracle_rejects_missing_positive_b_bytes() {
    let mut receipt = fixture("synthetic-before-entry");
    receipt["page_b"]["rawRecords"]
        .as_array_mut()
        .expect("fixture raw")
        .remove(0);
    receipt["page_readbacks"] = json!([page_readback(&receipt["page_b"], 480)]);
    rejected(&receipt);
}
#[test]
fn navigation_oracle_rejects_wrong_b_carrier_ready_or_epoch() {
    for (field, invalid) in [
        ("carrier_kind", json!("invocation")),
        ("route_ready", json!(false)),
        ("document_epoch", json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")),
    ] {
        let mut receipt = fixture("synthetic-before-entry");
        receipt["marker_posts"][0]["value"][field] = invalid;
        rejected(&receipt);
    }
}
#[test]
fn navigation_oracle_rejects_foreign_epoch_a_bytes_in_later_b_snapshot() {
    let mut receipt = fixture("synthetic-before-entry");
    let mut later = receipt["page_b"].clone();
    later["rawRecords"]
        .as_array_mut()
        .expect("fixture raw")
        .push(native_string(&json!({
        "v":99,"kind":"foreign","document":"cccccccccccccccccccccccccccccccc","payload":CANARY})));
    receipt["page_readbacks"]
        .as_array_mut()
        .expect("fixture snapshots")
        .push(page_readback(&later, 490));
    rejected(&receipt);
}
#[test]
fn navigation_oracle_rejects_changed_allocation_on_retry() {
    let mut receipt = fixture("synthetic-before-entry");
    add_return(&mut receipt, true);
    receipt["target_posts"][1]["value"]["event_storage"]["wide_id"] = json!(999);
    rejected(&receipt);
}
#[test]
fn navigation_oracle_rejects_missing_actual_native_drop() {
    let mut receipt = fixture("synthetic-before-entry");
    receipt["drops"] = json!([]);
    rejected(&receipt);
}
#[test]
fn navigation_oracle_actual_call_mutant_fails_the_intended_first_assertion() {
    let mut receipt = fixture("synthetic-before-entry");
    receipt["target_posts"][0]["value"]["calls_after"] = json!(4);
    let panic = std::panic::catch_unwind(|| assert_complete(&receipt))
        .expect_err("actual API counter mutant must fail");
    let text = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("assertion text");
    assert!(text.starts_with(
        "assertion `left == right` failed: synthetic callback suppresses actual API entry"
    ));
}
