//! Desktop facts use the shared Window postcard corpus.
use crate::codec::{decode, encode_into};
use crate::window::{DESKTOP_EVENT_STORAGE_BYTES, DesktopFact, DesktopFacts, Rect, WindowEvent};

pub(super) fn literal_cases() -> [(&'static str, WindowEvent); 3] {
    [
        (
            "desktop-captured",
            WindowEvent::DesktopSnapshot {
                epoch: 300,
                snapshot: Some(DesktopFacts {
                    primary_id: 17,
                    displays: vec![DesktopFact {
                        id: 17,
                        bounds: Rect {
                            x: 0,
                            y: 0,
                            width: 1440,
                            height: 900,
                        },
                        work_area: Rect {
                            x: 0,
                            y: 24,
                            width: 1440,
                            height: 876,
                        },
                        scale_factor: 2.0,
                    }],
                    should_use_dark_colors: false,
                }),
            },
        ),
        (
            "desktop-unavailable",
            WindowEvent::DesktopSnapshot {
                epoch: 301,
                snapshot: None,
            },
        ),
        (
            "desktop-epoch-maximum",
            WindowEvent::DesktopSnapshot {
                epoch: u64::MAX,
                snapshot: None,
            },
        ),
    ]
}

#[test]
fn desktop_literals_fit_the_desktop_storage() -> Result<(), Box<dyn std::error::Error>> {
    let vectors = super::vectors("event");
    for (name, fact) in literal_cases() {
        let mut storage = [0_u8; DESKTOP_EVENT_STORAGE_BYTES];
        assert_eq!(encode_into(&fact, &mut storage)?, vectors[name]);
    }
    Ok(())
}

#[test]
fn desktop_rejects_unknown_tag_and_invalid_optional_payload() {
    // A complete invalidation with an unknown tag fails.
    assert!(decode::<WindowEvent>(&[11, 1, 0]).is_err());
    assert!(decode::<WindowEvent>(&[7, 1, 2]).is_err());
}

#[test]
fn maximum_qualified_desktop_fits_its_exact_fixed_storage() -> Result<(), Box<dyn std::error::Error>>
{
    let rect = Rect {
        x: i32::MIN,
        y: i32::MAX,
        width: u32::MAX,
        height: u32::MAX,
    };
    let fact = WindowEvent::DesktopSnapshot {
        epoch: u64::MAX,
        snapshot: Some(DesktopFacts {
            primary_id: u32::MAX,
            displays: (0..32)
                .map(|offset| DesktopFact {
                    id: u32::MAX - offset,
                    bounds: rect,
                    work_area: rect,
                    scale_factor: f64::MAX,
                })
                .collect(),
            should_use_dark_colors: true,
        }),
    };
    let mut storage = [0_u8; DESKTOP_EVENT_STORAGE_BYTES];
    let bytes = encode_into(&fact, &mut storage)?;
    assert_eq!(
        bytes.len(),
        1715,
        "independent maximum-width postcard byte count"
    );
    assert_eq!(decode::<WindowEvent>(bytes)?, fact);
    assert!(
        encode_into(&fact, &mut [0_u8; 1714]).is_err(),
        "one byte less cannot encode the maximum"
    );
    Ok(())
}
