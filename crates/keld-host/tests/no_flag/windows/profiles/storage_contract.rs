//! Native pure storage-report completeness and error contracts.

use crate::support::profile_response::profile_state_response;

#[test]
fn profile_state_report_requires_the_complete_storage_census() {
    let fields = [
        "case=a-seed",
        "nonce=run1",
        "before=",
        "after=value-a",
        "cookie_before=",
        "cookie_after=value-a",
        "indexed_db_before=",
        "indexed_db_after=value-a",
        "cache_before=",
        "cache_after=value-a",
        "worker_before=",
        "worker_after=value-a",
    ];
    let complete = format!("/state?{}", fields.join("&"));
    assert!(profile_state_response(&complete, "a-seed", None).is_ok());
    for omitted in 2..fields.len() {
        let query = fields
            .iter()
            .enumerate()
            .filter_map(|(index, field)| (index != omitted).then_some(*field))
            .collect::<Vec<_>>()
            .join("&");
        assert!(
            profile_state_response(&format!("/state?{query}"), "a-seed", None).is_err(),
            "missing storage observation must fail: {}",
            fields[omitted]
        );
    }
}

#[test]
fn profile_state_browser_failure_cannot_become_an_empty_success() {
    let result = profile_state_response(
        "/state?case=a-seed&nonce=run1&error=indexedDB-write%3AAbortError",
        "a-seed",
        None,
    );
    assert_eq!(
        result.err().as_deref(),
        Some("browser storage case `a-seed` failed: indexedDB-write%3AAbortError")
    );
}

#[test]
fn profile_state_report_retains_each_store_observation() {
    let response = profile_state_response(
        "/state?case=a-restart&nonce=run1&before=local-a&after=local-b&cookie_before=cookie-a&cookie_after=cookie-b&indexed_db_before=idb-a&indexed_db_after=idb-b&cache_before=cache-a&cache_after=cache-b&worker_before=worker-a&worker_after=worker-b",
        "a-restart",
        None,
    )
    .expect("complete storage observation");
    let observed = response
        .observation
        .expect("semantic observation, not a resource reply");
    assert_eq!(
        observed.before.values(),
        [
            ("cookie", "cookie-a"),
            ("localStorage", "local-a"),
            ("IndexedDB", "idb-a"),
            ("CacheStorage", "cache-a"),
            ("serviceWorker", "worker-a"),
        ]
    );
    assert_eq!(
        observed.after.values(),
        [
            ("cookie", "cookie-b"),
            ("localStorage", "local-b"),
            ("IndexedDB", "idb-b"),
            ("CacheStorage", "cache-b"),
            ("serviceWorker", "worker-b"),
        ]
    );
}
