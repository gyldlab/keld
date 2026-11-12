//! Host-lifecycle kipc channel (KEL-72).
//!
//! Generic host session control — not Electron-named (`keld-compat` /
//! `@keld/electron` map these onto `app.whenReady` / `app.quit` /
//! `window-all-closed`). Its id comes from the [`crate::channel_table`] entry
//! `lifecycle`, like echo and fs; handshake channel-table exchange is later work.
//!
//! This channel is **not** routed through [`crate::guard_dispatch`]: ready /
//! last-window-closed / quit are session control on the app-link the host
//! already minted, not an OS-authority operation (`fs.read` / media).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::frame::ChannelId;

/// Channel handle for host lifecycle `Event` / `Call` frames: the
/// [`crate::channel_table::LIFECYCLE`] entry.
pub const LIFECYCLE_CHANNEL: ChannelId = crate::channel_table::LIFECYCLE.id();

/// Host → app-process notifications on [`LIFECYCLE_CHANNEL`].
///
/// Encoded as a postcard unit enum (varint discriminant). Pinned in tests:
/// `Ready` is `0x00`, `LastWindowClosed` is `0x01`, scoped `QuitRequested` is `0x02`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleEvent {
    /// The host is ready for the app process to create windows / run.
    Ready,
    /// The last host-owned window closed.
    LastWindowClosed,
    /// An already-authorized native menu Quit intent for the qualified original
    /// composition. The app still owns veto/preflight; this is not termination.
    QuitRequested,
}

/// App-process → host requests on [`LIFECYCLE_CHANNEL`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleRequest {
    /// End this app-link session. The host replies, then the serve loop
    /// returns — that return is the process-lifecycle oracle for quit.
    Quit,
    /// Subscribe this authenticated link to an admitted host-event channel.
    /// The payload never selects a principal (GH-508 criterion 16).
    Subscribe {
        /// Generated table id of the requested event-bearing channel.
        channel: u16,
    },
    /// Publish the qualified preload's immutable runtime facts before app entry.
    /// Only the trusted Host-selected admission worker accepts this first CALL.
    Init {
        /// Non-authority metadata captured in the role main realm before app code.
        runtime_process_facts: RuntimeProcessFacts,
    },
}

/// Closed subscription refusal vocabulary (GH-508 §4.4); order is wire data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscribeRefusal {
    /// The host build has no entry with this id.
    UnknownChannel,
    /// This entry does not support explicit host-event subscription.
    NotSubscribable,
    /// Guarded event subscription requires a future guard-owned contract.
    GuardedNotSupported,
    /// The host's authenticated role declaration does not admit this channel.
    RoleNotAdmitted,
}

/// Host → app-process reply to [`LifecycleRequest::Quit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleResponse {
    /// The host accepted quit and will end the session.
    Quit,
    /// Subscription and any replay have completed on this same link.
    Subscribed,
    /// No subscription state changed.
    SubscribeRefused {
        /// Typed reason for refusal; the peer matches this enum, not prose.
        reason: SubscribeRefusal,
    },
    /// The generation admission owner retained its qualified runtime facts.
    /// This acknowledgment is not native UI readiness or a permission grant.
    Initialized,
}

/// Maximum encoded body for the private first post-HELLO Init CALL.
///
/// The 8192 data-byte bound plus at most 32 key/value postcard lengths and the
/// Init/map prefixes fit below this conservative bound. Applied before allocation.
pub const MAX_RUNTIME_PROCESS_FACTS_PAYLOAD: usize = 8448;
const MAX_RUNTIME_VERSION_ENTRIES: usize = 32;
const MAX_RUNTIME_VERSION_BYTES: usize = 8192;

/// Actual runtime strings captured before application dependencies, never authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProcessFacts {
    /// Own data fields of the real role-main `process.versions` object.
    #[serde(deserialize_with = "deserialize_runtime_versions")]
    pub versions: BTreeMap<String, String>,
}

fn runtime_version_entry_bytes(key: &str, value: &str) -> Option<usize> {
    if key.is_empty() || value.is_empty() || key.contains('\0') || value.contains('\0') {
        return None;
    }
    key.len().checked_add(value.len())
}

impl RuntimeProcessFacts {
    fn validate(&self) -> Result<(), crate::IpcError> {
        let invalid = || crate::IpcError::Protocol {
            detail: "runtime facts require 1..32 nonempty string entries within 8192 UTF-8 bytes",
        };
        if self.versions.is_empty() || self.versions.len() > MAX_RUNTIME_VERSION_ENTRIES {
            return Err(invalid());
        }
        let mut bytes = 0_usize;
        for (key, value) in &self.versions {
            bytes = bytes
                .checked_add(runtime_version_entry_bytes(key, value).ok_or_else(invalid)?)
                .filter(|total| *total <= MAX_RUNTIME_VERSION_BYTES)
                .ok_or_else(invalid)?;
        }
        Ok(())
    }
}

fn deserialize_runtime_versions<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct VersionsVisitor;
    impl<'de> serde::de::Visitor<'de> for VersionsVisitor {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter.write_str("1..32 unique runtime-version strings within 8192 UTF-8 bytes")
        }
        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: serde::de::MapAccess<'de>,
        {
            use serde::de::Error as _;
            if map
                .size_hint()
                .is_some_and(|size| size == 0 || size > MAX_RUNTIME_VERSION_ENTRIES)
            {
                return Err(M::Error::custom("runtime versions count is outside 1..32"));
            }
            let mut versions = BTreeMap::new();
            let mut bytes = 0_usize;
            while let Some((key, value)) = map.next_entry::<String, String>()? {
                if versions.len() == MAX_RUNTIME_VERSION_ENTRIES {
                    return Err(M::Error::custom("runtime versions exceed 32 entries"));
                }
                bytes = bytes
                    .checked_add(
                        runtime_version_entry_bytes(&key, &value)
                            .ok_or_else(|| M::Error::custom("empty or NUL runtime version text"))?,
                    )
                    .filter(|total| *total <= MAX_RUNTIME_VERSION_BYTES)
                    .ok_or_else(|| M::Error::custom("runtime versions exceed 8192 UTF-8 bytes"))?;
                if versions.insert(key, value).is_some() {
                    return Err(M::Error::custom("duplicate runtime version key"));
                }
            }
            if versions.is_empty() {
                return Err(M::Error::custom("runtime versions are empty"));
            }
            Ok(versions)
        }
    }
    deserializer.deserialize_map(VersionsVisitor)
}

/// Consumes and acknowledges the qualified first runtime-facts CALL after HELLO.
///
/// The caller must select this through trusted Host launch state before spawn.
/// It must not run again after the bound generation is released. The map grants
/// no authority, names no identity, and never completes the native UI Init barrier.
///
/// # Errors
///
/// Returns the existing framing, codec, semantic or deadline error for a missing,
/// malformed or oversized Init. The stream is terminal after every error.
pub fn accept_runtime_process_facts_until<S>(
    stream: &mut S,
    deadline: Instant,
) -> Result<BTreeMap<String, String>, crate::IpcError>
where
    S: Read + Write + crate::AppLinkDeadlines,
{
    let old_read = stream.app_link_read_deadline()?;
    let old_write = stream.app_link_write_deadline()?;
    stream.set_app_link_read_deadline(Some(crate::APP_LINK_READER_POLL))?;
    let (_header, payload) = crate::link::read_runtime_process_facts_init_until(stream, deadline)?;
    let LifecycleRequest::Init {
        runtime_process_facts,
    } = crate::codec::decode(&payload)?
    else {
        return Err(crate::IpcError::Protocol {
            detail: "the first qualified post-HELLO CALL must publish runtime facts",
        });
    };
    runtime_process_facts.validate()?;
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(crate::IpcError::Timeout)?;
    stream.set_app_link_write_deadline(Some(remaining))?;
    let reply = crate::codec::encode(&LifecycleResponse::Initialized)?;
    crate::link::write_frame(
        stream,
        crate::FrameKind::Reply,
        0,
        LIFECYCLE_CHANNEL,
        crate::CorrelationId(1),
        &reply,
    )?;
    if Instant::now() >= deadline {
        return Err(crate::IpcError::Timeout);
    }
    stream.set_app_link_read_deadline(old_read)?;
    stream.set_app_link_write_deadline(old_write)?;
    Ok(runtime_process_facts.versions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{decode, encode};

    #[test]
    fn lifecycle_event_postcard_bytes_are_pinned() {
        assert_eq!(encode(&LifecycleEvent::Ready).expect("enc"), [0x00]);
        assert_eq!(
            encode(&LifecycleEvent::LastWindowClosed).expect("enc"),
            [0x01]
        );
        assert_eq!(
            decode::<LifecycleEvent>(&[0x00]).expect("dec"),
            LifecycleEvent::Ready
        );
        assert_eq!(
            decode::<LifecycleEvent>(&[0x01]).expect("dec"),
            LifecycleEvent::LastWindowClosed
        );
    }

    #[test]
    fn scoped_native_quit_requested_is_append_only_unit_two() {
        assert_eq!(encode(&LifecycleEvent::QuitRequested).expect("encode"), [2]);
        assert_eq!(
            decode::<LifecycleEvent>(&[2]).expect("decode"),
            LifecycleEvent::QuitRequested
        );
        for bytes in [&[][..], &[3][..], &[2, 0][..]] {
            assert!(
                decode::<LifecycleEvent>(bytes).is_err(),
                "strict unit payload: {bytes:?}"
            );
        }
    }

    #[test]
    fn lifecycle_quit_postcard_bytes_are_pinned() {
        assert_eq!(encode(&LifecycleRequest::Quit).expect("enc"), [0x00]);
        assert_eq!(encode(&LifecycleResponse::Quit).expect("enc"), [0x00]);
        assert_eq!(
            decode::<LifecycleRequest>(&[0x00]).expect("dec"),
            LifecycleRequest::Quit
        );
    }

    #[test]
    fn subscription_payloads_pin_append_only_discriminants_and_strict_bounds() {
        let request = LifecycleRequest::Subscribe { channel: 5 };
        assert_eq!(encode(&request).expect("encode Subscribe"), [1, 5]);
        assert_eq!(
            decode::<LifecycleRequest>(&[1, 5]).expect("decode"),
            request
        );
        assert_eq!(
            encode(&LifecycleRequest::Subscribe { channel: u16::MAX }).expect("encode max"),
            [1, 255, 255, 3]
        );
        assert_eq!(encode(&LifecycleResponse::Subscribed).expect("encode"), [1]);
        for (reason, byte) in [
            (SubscribeRefusal::UnknownChannel, 0),
            (SubscribeRefusal::NotSubscribable, 1),
            (SubscribeRefusal::GuardedNotSupported, 2),
            (SubscribeRefusal::RoleNotAdmitted, 3),
        ] {
            let response = LifecycleResponse::SubscribeRefused { reason };
            assert_eq!(encode(&response).expect("encode refusal"), [2, byte]);
            assert_eq!(
                decode::<LifecycleResponse>(&[2, byte]).expect("decode refusal"),
                response
            );
        }
        for bad in [&[1][..], &[1, 128], &[1, 255, 255, 7], &[1, 5, 0], &[2]] {
            assert!(
                decode::<LifecycleRequest>(bad).is_err(),
                "malformed request {bad:?}"
            );
        }
        for bad in [&[1, 0][..], &[2], &[2, 4], &[2, 0, 0], &[4]] {
            assert!(
                decode::<LifecycleResponse>(bad).is_err(),
                "malformed response {bad:?}"
            );
        }
    }

    #[test]
    fn runtime_init_and_ack_have_append_only_exact_bytes() {
        let request = LifecycleRequest::Init {
            runtime_process_facts: RuntimeProcessFacts {
                versions: BTreeMap::from([(String::from("bun"), String::from("v"))]),
            },
        };
        let wire = [2, 1, 3, b'b', b'u', b'n', 1, b'v'];
        assert_eq!(encode(&request).expect("encode Init"), wire);
        assert_eq!(
            decode::<LifecycleRequest>(&wire).expect("decode Init"),
            request
        );
        assert_eq!(
            encode(&LifecycleResponse::Initialized).expect("encode ACK"),
            [3]
        );
        assert_eq!(
            decode::<LifecycleResponse>(&[3]).expect("decode ACK"),
            LifecycleResponse::Initialized
        );
        assert!(decode::<LifecycleRequest>(&[2, 1, 3, b'b', b'u', b'n', 1]).is_err());
        assert!(decode::<LifecycleRequest>(&[2, 1, 3, b'b', b'u', b'n', 1, b'v', 0]).is_err());
    }

    #[test]
    fn runtime_versions_decode_rejects_duplicate_and_invalid_map_text() {
        // Exact postcard bytes: Init tag, map length, then UTF-8 key/value pairs.
        for bytes in [
            vec![2, 0],
            vec![2, 33],
            vec![2, 1, 0, 1, b'v'],
            vec![2, 1, 1, b'k', 0],
            vec![2, 1, 1, 0, 1, b'v'],
            vec![2, 2, 1, b'k', 1, b'a', 1, b'k', 1, b'b'],
        ] {
            assert!(
                decode::<LifecycleRequest>(&bytes).is_err(),
                "invalid Init {bytes:?}"
            );
        }
        let max = RuntimeProcessFacts {
            versions: BTreeMap::from([(String::from("bun"), "v".repeat(8189))]),
        };
        assert!(max.validate().is_ok());
        let max_wire = encode(&LifecycleRequest::Init {
            runtime_process_facts: max,
        })
        .expect("max wire");
        assert!(max_wire.len() <= MAX_RUNTIME_PROCESS_FACTS_PAYLOAD);
        assert!(decode::<LifecycleRequest>(&max_wire).is_ok());
        let too_large = LifecycleRequest::Init {
            runtime_process_facts: RuntimeProcessFacts {
                versions: BTreeMap::from([(String::from("bun"), "v".repeat(8190))]),
            },
        };
        assert!(decode::<LifecycleRequest>(&encode(&too_large).expect("over-bound wire")).is_err());
        let utf8_over = RuntimeProcessFacts {
            versions: BTreeMap::from([(String::from("bun"), "é".repeat(4095))]),
        };
        assert!(
            utf8_over.validate().is_err(),
            "UTF-8 byte count, not characters"
        );
        let max_entries = RuntimeProcessFacts {
            versions: (0..32)
                .map(|index| (format!("k{index}"), String::from("v")))
                .collect(),
        };
        let max_entries_wire = encode(&LifecycleRequest::Init {
            runtime_process_facts: max_entries,
        })
        .expect("32 entries");
        assert!(decode::<LifecycleRequest>(&max_entries_wire).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn runtime_admission_acknowledges_only_complete_validated_facts() {
        use crate::{AppLinkDeadlines, CorrelationId, FrameKind};
        use std::time::Duration;
        let (mut host, mut role) =
            std::os::unix::net::UnixStream::pair().expect("actual app-link pair");
        role.set_app_link_deadlines(Some(Duration::from_secs(2)))
            .expect("bounded role");
        let expected = BTreeMap::from([(String::from("bun"), String::from("actual-role-value"))]);
        let request = LifecycleRequest::Init {
            runtime_process_facts: RuntimeProcessFacts {
                versions: expected.clone(),
            },
        };
        crate::link::write_frame(
            &mut role,
            FrameKind::Call,
            0,
            LIFECYCLE_CHANNEL,
            CorrelationId(1),
            &encode(&request).expect("Init"),
        )
        .expect("publish complete Init");
        let versions =
            accept_runtime_process_facts_until(&mut host, Instant::now() + Duration::from_secs(2))
                .expect("complete valid Init");
        assert_eq!(versions, expected);
        let (ack, payload) = crate::link::read_frame(&mut role).expect("actual ACK frame");
        assert_eq!(ack.kind, FrameKind::Reply);
        assert_eq!(ack.channel, LIFECYCLE_CHANNEL);
        assert_eq!(ack.corr, CorrelationId(1));
        assert_eq!(payload, [3]);
        assert_eq!(host.app_link_read_deadline().expect("restored read"), None);
        assert_eq!(
            host.app_link_write_deadline().expect("restored write"),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn missing_or_invalid_runtime_init_never_receives_an_ack() {
        use crate::{CorrelationId, FrameKind};
        use std::io::Read as _;
        for payload in [vec![0], vec![2, 0], vec![2, 1, 1, b'k', 1]] {
            let (mut host, mut role) = std::os::unix::net::UnixStream::pair().expect("real pair");
            crate::link::write_frame(
                &mut role,
                FrameKind::Call,
                0,
                LIFECYCLE_CHANNEL,
                CorrelationId(1),
                &payload,
            )
            .expect("publish malformed Init");
            assert!(
                accept_runtime_process_facts_until(
                    &mut host,
                    Instant::now() + std::time::Duration::from_secs(1)
                )
                .is_err()
            );
            drop(host);
            let mut byte = [0_u8; 1];
            assert_eq!(role.read(&mut byte).expect("host closed without ACK"), 0);
        }
        let (mut host, _role) = std::os::unix::net::UnixStream::pair().expect("silent role");
        assert!(matches!(
            accept_runtime_process_facts_until(&mut host, Instant::now()),
            Err(crate::IpcError::Timeout)
        ));
    }

    #[test]
    fn lifecycle_channel_is_three() {
        assert_eq!(LIFECYCLE_CHANNEL, ChannelId(3));
        assert_ne!(LIFECYCLE_CHANNEL, crate::ECHO_CHANNEL);
    }
}
