use super::*;
use std::collections::BTreeMap;

fn versions() -> BTreeMap<String, String> {
    BTreeMap::from([(String::from("bun"), String::from("captured"))])
}

fn image(
    budget: &AppPreloadBudget,
    source: &[u8],
    args: Vec<String>,
    facts: BTreeMap<String, String>,
) -> Result<AppPreloadImage, WvError> {
    budget.reserve_window()?.from_qualified_graph(
        source.to_vec(),
        Sha256::digest(source).into(),
        args,
        facts,
    )
}

#[test]
fn changed_empty_oversized_or_invalid_text_drops_the_reservation() -> Result<(), WvError> {
    let budget = AppPreloadBudget::default();
    let bytes = b"selected original bytes";
    let changed = budget.reserve_window()?.from_qualified_graph(
        bytes.to_vec(),
        Sha256::digest(b"different").into(),
        vec![],
        versions(),
    );
    assert!(matches!(changed, Err(WvError::AppPreloadSource { .. })));
    assert_eq!(budget.retained_bytes(), Some(0));
    for source in [vec![], vec![0xff], vec![b'a'; 512 * 1024 + 1]] {
        assert!(matches!(
            image(&budget, &source, vec![], versions()),
            Err(WvError::AppPreloadSource { .. })
        ));
        assert_eq!(budget.retained_bytes(), Some(0));
    }
    Ok(())
}

#[test]
fn argument_and_fact_limits_apply_to_the_retained_configuration() -> Result<(), WvError> {
    let budget = AppPreloadBudget::default();
    let bytes = b"preload\r\n";
    for args in [
        vec![String::new(); 17],
        vec!["a".repeat(257)],
        vec!["a".repeat(256); 9],
        vec!["x\0".to_owned()],
    ] {
        assert!(image(&budget, bytes, args, versions()).is_err());
    }
    let entries = |n, length| {
        (0..n)
            .map(|i| (format!("{i:02}"), "v".repeat(length)))
            .collect()
    };
    for facts in [
        BTreeMap::new(),
        entries(33, 1),
        entries(32, 255),
        BTreeMap::from([("k".repeat(129), "v".into())]),
        BTreeMap::from([("k".into(), "v".repeat(257))]),
        BTreeMap::from([(String::new(), "v".into())]),
        BTreeMap::from([("k".into(), String::new())]),
        BTreeMap::from([("bad\0key".into(), "v".into())]),
        BTreeMap::from([("k".into(), "bad\0value".into())]),
    ] {
        assert!(image(&budget, bytes, vec![], facts).is_err());
    }
    assert_eq!(budget.retained_bytes(), Some(0));
    let mut args = vec!["a".repeat(256); 8];
    args.resize(16, String::new());
    let facts = entries(32, 254);
    let selected = image(&budget, bytes, args.clone(), facts.clone())?;
    assert_eq!(selected.source().as_bytes(), bytes);
    assert_eq!(selected.additional_arguments(), args);
    assert_eq!(selected.versions(), &facts);
    assert!(selected.same_configuration(&image(&budget, bytes, args.clone(), facts.clone())?));
    assert!(!selected.same_configuration(&image(
        &budget,
        b"changed",
        args.clone(),
        facts.clone()
    )?));
    assert!(!selected.same_configuration(&image(&budget, bytes, args, versions())?));
    assert!(!selected.same_configuration(&image(&budget, bytes, vec![], facts)?));
    Ok(())
}

#[test]
fn clones_share_one_charge_until_the_last_drop_at_the_host_limit() -> Result<(), WvError> {
    let budget = AppPreloadBudget::default();
    let another_window = budget.clone();
    let selected = image(&budget, b"exact", vec![], versions())?;
    let surviving = selected.clone();
    drop(selected);
    assert_eq!(surviving.source_bytes(), 5);
    assert_eq!(another_window.retained_bytes(), Some(5));
    let mut reserved = Vec::new();
    for _ in 0..15 {
        reserved.push(another_window.reserve_window()?);
    }
    assert!(budget.reserve_window().is_err());
    drop(surviving);
    let final_image = image(&budget, &vec![b'a'; 512 * 1024], vec![], versions())?;
    assert_eq!(budget.retained_bytes(), Some(8 * 1024 * 1024));
    assert!(another_window.reserve_window().is_err());
    drop(reserved);
    assert_eq!(budget.retained_bytes(), Some(512 * 1024));
    drop(final_image);
    assert_eq!(another_window.retained_bytes(), Some(0));
    Ok(())
}
