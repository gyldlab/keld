//! GH481 candidate wire contract, separate from the eight existing event vectors.
use crate::codec::{decode, encode, encode_into};
use crate::window::{DESKTOP_EVENT_STORAGE_BYTES, DesktopFact, DesktopFacts, Rect, WindowEvent};

#[test]
fn desktop_literals_pin_optional_facts_and_full_epoch() -> Result<(), Box<dyn std::error::Error>> {
    let expected = [
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
    ];
    let rows: Vec<_> = include_str!("../../../tests/fixtures/desktop-v0.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    assert_eq!(rows.len(), expected.len());
    for (line, (name, fact)) in rows.into_iter().zip(expected) {
        let (actual_name, hex) = line.split_once('\t').ok_or("malformed desktop literal")?;
        assert_eq!(actual_name, name);
        assert_eq!(hex.len() % 2, 0);
        let mut bytes = Vec::new();
        for pair in hex.as_bytes().as_chunks::<2>().0 {
            bytes.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
        }
        assert_eq!(encode(&fact)?, bytes);
        let mut scratch = [0_u8; DESKTOP_EVENT_STORAGE_BYTES];
        assert_eq!(encode_into(&fact, &mut scratch)?, bytes);
        assert_eq!(decode::<WindowEvent>(&bytes)?, fact);
        if name == "desktop-captured" {
            let mut invalid_option = bytes.clone();
            invalid_option[3] = 2;
            assert!(decode::<WindowEvent>(&invalid_option).is_err());
        }
        for end in 0..bytes.len() {
            assert!(decode::<WindowEvent>(&bytes[..end]).is_err());
        }
        bytes.push(0);
        assert!(decode::<WindowEvent>(&bytes).is_err());
    }
    Ok(())
}

#[test]
fn desktop_rejects_unknown_tag_and_invalid_optional_payload() {
    // A complete otherwise-valid invalidation distinguishes unknown tag 10 from truncation.
    assert!(decode::<WindowEvent>(&[10, 1, 0]).is_err());
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
