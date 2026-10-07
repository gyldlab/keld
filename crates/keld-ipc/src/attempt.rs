//! `keld-attempt` endpoints and records (KEL-53 §4 "Candidate connect-back"
//! and "Machine-UAC bootstrap" item 1; owner decisions D2 and D5).
//!
//! An attempt endpoint name is a locator that carries no authority. On
//! Windows this module owns the closed set of endpoint descriptors, the
//! owner's first-instance creation with descriptor readback, the client's
//! readback of the server descriptor before it sends anything, and the
//! purpose-`1` endpoint locator. Comparing a readback with a form is not
//! repeated here: it is the named-pipe owner's single comparison, which the
//! app-link and lifecycle pipes share. The claim and health record codec is
//! pure bytes and builds on every platform; KEL-53 owns its byte layouts
//! (approved: KEL-270 owner decision `eff8e2fb`).
//!
//! The claim (`KELD-AH1` to `KELD-AR1`) runs inside the endpoint and the
//! client, which never expose their stream (`claim.rs`): the owner's endpoint
//! derives its name from the IDs that its `KELD-AC1` carries, and the client
//! runs the locator check before it sends `KELD-AA1`.

#[cfg(windows)]
mod channel;
#[cfg(windows)]
mod claim;
#[cfg(windows)]
mod locator;
mod records;
#[cfg(any(windows, test))]
mod window;

#[cfg(windows)]
pub use channel::{
    WindowsAttemptClaimantChannel, WindowsAttemptCloseWait, WindowsAttemptExchangeError,
    WindowsAttemptOwnerChannel, WindowsAttemptRollBack,
};
#[cfg(windows)]
pub(crate) use locator::ATTEMPT_ENDPOINT_PREFIX;
#[cfg(windows)]
pub use locator::{
    WindowsAttemptLocatorError, WindowsAttemptLocatorInput, windows_attempt_connect_back_endpoint,
};
pub use records::{
    AttemptBootAcknowledgement, AttemptChallenge, AttemptClaim, AttemptFailureClass,
    AttemptHealthResult, AttemptReadPosition, AttemptRecord, AttemptRecordError, AttemptRecordKind,
    AttemptTranscript,
};
#[cfg(windows)]
pub use window::{ATTEMPT_HEALTH_WINDOW, AttemptHealthWindowFailure};

#[cfg(windows)]
use std::fmt::{self, Write as _};
#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::time::Instant;

#[cfg(windows)]
use windows_permissions::{LocalBox, SecurityDescriptor, Sid};
#[cfg(windows)]
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_PIPE_BUSY};

#[cfg(windows)]
use crate::bootstrap::WindowsNamedPipeBootstrapStream;
#[cfg(windows)]
use crate::windows_named_pipe::{
    PIPE_ACCESS_MASK, PipeDescriptorReadback, PipeSecuritySections, WindowsNamedPipeServer,
    WindowsNamedPipeStream, WindowsPipeSecurityFact, current_process_session_id,
};

/// Explicit Medium mandatory label with `SYSTEM_MANDATORY_LABEL_NO_WRITE_UP`,
/// which denies Low-integrity and `AppContainer` writers whatever the
/// creator's own integrity level.
#[cfg(windows)]
const MEDIUM_NO_WRITE_UP_LABEL: &str = "S:(ML;;NW;;;ME)";

/// One of the closed descriptor forms a `keld-attempt` endpoint may carry.
///
/// Every form has a protected DACL that grants only the landed `keld-ipc`
/// access mask `0x0012019B` (never `FILE_CREATE_PIPE_INSTANCE`, `WRITE_DAC`
/// or `WRITE_OWNER`) and an explicit Medium no-write-up label. The forms
/// differ only in owner and grantees. The same value builds the owner's
/// descriptor and checks the readback on both ends, so creation and
/// verification cannot drift apart.
#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsAttemptEndpointSecurity {
    form: DescriptorForm,
}

#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum DescriptorForm {
    PerUserConnectBack { user: String },
    MachineUacConnectBack { initiating_user: String },
    Bootstrap { host_user: String },
}

#[cfg(windows)]
impl WindowsAttemptEndpointSecurity {
    /// `PerUserDirect` connect-back endpoint: owned by the user, whose SID is
    /// also the only grantee (the owner and the initiating user are the same
    /// account in that mode).
    ///
    /// # Errors
    ///
    /// Returns [`WindowsAttemptEndpointError::InvalidSid`] when `user_sid` is
    /// not a valid binary Windows SID.
    pub fn per_user_connect_back(user_sid: &[u8]) -> Result<Self, WindowsAttemptEndpointError> {
        Ok(Self {
            form: DescriptorForm::PerUserConnectBack {
                user: sid_text(user_sid)?,
            },
        })
    }

    /// `MachineUacDirect` connect-back endpoint that the elevated helper
    /// creates: owned by BUILTIN Administrators (`O:BA`, owner decision D5),
    /// with the initiating user, not the helper's own account, as the only
    /// grantee.
    ///
    /// # Errors
    ///
    /// Returns [`WindowsAttemptEndpointError::InvalidSid`] when
    /// `initiating_user_sid` is not a valid binary Windows SID.
    pub fn machine_uac_connect_back(
        initiating_user_sid: &[u8],
    ) -> Result<Self, WindowsAttemptEndpointError> {
        Ok(Self {
            form: DescriptorForm::MachineUacConnectBack {
                initiating_user: sid_text(initiating_user_sid)?,
            },
        })
    }

    /// D2 bootstrap endpoint that the ordinary host creates: owned by the
    /// host's own user SID (a Medium token cannot assign BUILTIN
    /// Administrators), granting exactly that SID and BUILTIN Administrators.
    ///
    /// # Errors
    ///
    /// Returns [`WindowsAttemptEndpointError::InvalidSid`] when `host_user_sid`
    /// is not a valid binary Windows SID.
    pub fn bootstrap(host_user_sid: &[u8]) -> Result<Self, WindowsAttemptEndpointError> {
        Ok(Self {
            form: DescriptorForm::Bootstrap {
                host_user: sid_text(host_user_sid)?,
            },
        })
    }

    fn sddl(&self) -> String {
        let grant = |sid: &str| format!("(A;;0x{PIPE_ACCESS_MASK:08x};;;{sid})");
        match &self.form {
            DescriptorForm::PerUserConnectBack { user } => {
                format!("O:{user}D:P{}{MEDIUM_NO_WRITE_UP_LABEL}", grant(user))
            }
            DescriptorForm::MachineUacConnectBack { initiating_user } => {
                format!(
                    "O:BAD:P{}{MEDIUM_NO_WRITE_UP_LABEL}",
                    grant(initiating_user)
                )
            }
            DescriptorForm::Bootstrap { host_user } => format!(
                "O:{host_user}D:P{}{}{MEDIUM_NO_WRITE_UP_LABEL}",
                grant(host_user),
                grant("BA")
            ),
        }
    }

    fn descriptor(&self) -> Result<LocalBox<SecurityDescriptor>, WindowsAttemptEndpointError> {
        self.sddl()
            .parse()
            .map_err(|source| WindowsAttemptEndpointError::Os {
                operation: "build the endpoint descriptor",
                source,
            })
    }

    /// Requires `readback` to be exactly this form: a non-inheritable handle,
    /// a pipe that rejects remote clients, this owner, this protected DACL
    /// and this label, through the one pipe-form comparison
    /// (`PipeDescriptorReadback::first_mismatch`) that the app-link and
    /// lifecycle pipes also use.
    pub(crate) fn verify(
        &self,
        readback: &PipeDescriptorReadback,
    ) -> Result<(), WindowsAttemptEndpointError> {
        let expected = self.descriptor()?;
        let mismatch = readback
            .first_mismatch(&expected, PipeSecuritySections::OwnerDaclLabel)
            .map_err(|source| WindowsAttemptEndpointError::Os {
                operation: "render the endpoint descriptor",
                source,
            })?;
        match mismatch {
            Some(fact) => Err(WindowsAttemptEndpointError::SecurityMismatch { fact }),
            None => Ok(()),
        }
    }
}

/// The minted IDs that a connect-back endpoint's name derives from and that
/// its `KELD-AC1` carries.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConnectBackIds {
    /// The provenance-derived installation ID, the owner's own.
    pub(crate) installation: [u8; 32],
    /// The attempt ID that `keld-update` minted.
    pub(crate) attempt: [u8; 32],
    /// The health-channel ID that `keld-update` minted.
    pub(crate) health_channel: [u8; 32],
}

/// The owner-held `keld-attempt` endpoint: the only instance of its name,
/// created first and read back before the owner reveals the name to anyone.
///
/// Dropping it closes the instance and releases the name.
#[cfg(windows)]
#[derive(Debug)]
pub struct WindowsAttemptEndpoint {
    server: WindowsNamedPipeServer,
    endpoint: String,
    ids: ConnectBackIds,
}

#[cfg(windows)]
impl WindowsAttemptEndpoint {
    /// Creates the purpose-`1` connect-back endpoint of one minted attempt.
    /// Its name is [`windows_attempt_connect_back_endpoint`] over exactly the
    /// IDs that this endpoint's `KELD-AC1` then carries, so the name and the
    /// challenge cannot drift apart (KEL-53 §4 "Candidate connect-back",
    /// *Locator*). It is created as its name's only, first instance under
    /// `security`, with remote clients rejected and a non-inheritable handle,
    /// and its descriptor is read back and required to be exactly `security`.
    ///
    /// # Errors
    ///
    /// - [`WindowsAttemptEndpointError::Locator`] before any creation when the
    ///   locator refuses the IDs (an all-zero or a repeated ID);
    /// - [`WindowsAttemptEndpointError::NameInUse`] when the name already
    ///   exists, whoever created it;
    /// - [`WindowsAttemptEndpointError::SecurityMismatch`] when the readback
    ///   differs from `security` (the instance is closed);
    /// - [`WindowsAttemptEndpointError::Os`] for any other Windows failure,
    ///   including `ERROR_INVALID_OWNER` when the creating token cannot assign
    ///   the form's owner.
    pub fn create_connect_back(
        installation_id: &[u8; 32],
        attempt_id: &[u8; 32],
        health_channel_id: &[u8; 32],
        security: &WindowsAttemptEndpointSecurity,
    ) -> Result<Self, WindowsAttemptEndpointError> {
        let endpoint =
            windows_attempt_connect_back_endpoint(installation_id, attempt_id, health_channel_id)
                .map_err(WindowsAttemptEndpointError::Locator)?;
        Self::create_named(
            &endpoint,
            ConnectBackIds {
                installation: *installation_id,
                attempt: *attempt_id,
                health_channel: *health_channel_id,
            },
            security,
        )
    }

    /// Creates `endpoint`, bound to `ids`, as [`Self::create_connect_back`]
    /// does. Production passes the name derived from `ids`; a test passes
    /// another to stand in for an owner whose challenge does not derive its
    /// name.
    ///
    /// # Errors
    ///
    /// [`WindowsAttemptEndpointError::EndpointShape`] before any creation when
    /// `endpoint` is not an exact `keld-attempt` name, and otherwise as
    /// [`Self::create_connect_back`].
    pub(crate) fn create_named(
        endpoint: &str,
        ids: ConnectBackIds,
        security: &WindowsAttemptEndpointSecurity,
    ) -> Result<Self, WindowsAttemptEndpointError> {
        if !WindowsNamedPipeBootstrapStream::is_attempt_endpoint(endpoint) {
            return Err(WindowsAttemptEndpointError::EndpointShape);
        }
        Self::create_from(endpoint, ids, security, &security.descriptor()?)
    }

    /// Creates the instance under `descriptor` and admits it only if the
    /// readback is exactly `security`. Production passes `security`'s own
    /// descriptor; a test passes a deviating one to stand in for Windows
    /// assigning something other than what was requested.
    fn create_from(
        endpoint: &str,
        ids: ConnectBackIds,
        security: &WindowsAttemptEndpointSecurity,
        descriptor: &LocalBox<SecurityDescriptor>,
    ) -> Result<Self, WindowsAttemptEndpointError> {
        // Both codes mean that the name exists. FILE_FLAG_FIRST_PIPE_INSTANCE
        // refuses with ERROR_ACCESS_DENIED, but a squatter at its instance
        // limit that grants FILE_CREATE_PIPE_INSTANCE is refused first with
        // ERROR_PIPE_BUSY; the tests reach both.
        let server = WindowsNamedPipeServer::bind_with_descriptor(endpoint, descriptor).map_err(
            |source| match source.raw_os_error() {
                Some(code)
                    if code == ERROR_ACCESS_DENIED.cast_signed()
                        || code == ERROR_PIPE_BUSY.cast_signed() =>
                {
                    WindowsAttemptEndpointError::NameInUse { source }
                }
                _ => WindowsAttemptEndpointError::Os {
                    operation: "create the endpoint",
                    source,
                },
            },
        )?;
        let readback =
            server
                .descriptor_readback()
                .map_err(|source| WindowsAttemptEndpointError::Os {
                    operation: "read the endpoint descriptor back",
                    source,
                })?;
        security.verify(&readback)?;
        Ok(Self {
            server,
            endpoint: endpoint.to_owned(),
            ids,
        })
    }

    /// The exact endpoint name this owner holds.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

/// A client connection to a `keld-attempt` endpoint whose server was found in
/// this process's session, with its descriptor read back and matched, before
/// this value existed. Nothing has been sent on it; [`Self::claim`] runs the
/// claim on it.
#[cfg(windows)]
#[derive(Debug)]
pub struct WindowsAttemptClient {
    stream: WindowsNamedPipeStream,
    /// The rendezvous name this client opened: the claim's locator check
    /// requires the offered IDs to derive exactly this name.
    endpoint: String,
}

#[cfg(windows)]
impl WindowsAttemptClient {
    /// Refuses any name outside the `keld-attempt` namespace before opening
    /// it, opens the endpoint so the server can at most identify this client
    /// (`SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`), and, before
    /// returning, requires `GetNamedPipeServerSessionId` to equal this
    /// process's own session and the server's owner, DACL, label and
    /// remote-client rejection to be exactly `expected` (KEL-53 §4
    /// "Candidate connect-back" and "Machine-UAC bootstrap" item 3).
    ///
    /// # Errors
    ///
    /// - [`WindowsAttemptEndpointError::EndpointShape`] before any open;
    /// - [`WindowsAttemptEndpointError::ServerSession`] when the server runs
    ///   in another session (the connection is closed, nothing was sent);
    /// - [`WindowsAttemptEndpointError::SecurityMismatch`] when the server is
    ///   not exactly `expected` (the connection is closed, nothing was sent);
    /// - [`WindowsAttemptEndpointError::Os`] when this process's session query,
    ///   the open (including its wait for a busy instance until `deadline`),
    ///   the server session query or the readback fails.
    pub fn connect_until(
        endpoint: &str,
        expected: &WindowsAttemptEndpointSecurity,
        deadline: Instant,
    ) -> Result<Self, WindowsAttemptEndpointError> {
        if !WindowsNamedPipeBootstrapStream::is_attempt_endpoint(endpoint) {
            return Err(WindowsAttemptEndpointError::EndpointShape);
        }
        let own_session =
            current_process_session_id().map_err(|source| WindowsAttemptEndpointError::Os {
                operation: "query this process's session",
                source,
            })?;
        Self::connect_in_session(endpoint, expected, deadline, own_session)
    }

    /// Opens `endpoint` and admits it only for a server in `own_session` whose
    /// descriptor is exactly `expected`. Production passes this process's own
    /// session; a test passes another to stand in for a server that runs in a
    /// different session.
    fn connect_in_session(
        endpoint: &str,
        expected: &WindowsAttemptEndpointSecurity,
        deadline: Instant,
        own_session: u32,
    ) -> Result<Self, WindowsAttemptEndpointError> {
        let stream =
            WindowsNamedPipeServer::connect_identification_client_until(endpoint, deadline)
                .map_err(|source| WindowsAttemptEndpointError::Os {
                    operation: "open the endpoint",
                    source,
                })?;
        let server_session =
            stream
                .peer_session_id()
                .map_err(|source| WindowsAttemptEndpointError::Os {
                    operation: "query the server session ID",
                    source,
                })?;
        if server_session != own_session {
            return Err(WindowsAttemptEndpointError::ServerSession {
                server_session,
                own_session,
            });
        }
        let readback =
            stream
                .descriptor_readback()
                .map_err(|source| WindowsAttemptEndpointError::Os {
                    operation: "read the server descriptor back",
                    source,
                })?;
        expected.verify(&readback)?;
        Ok(Self {
            stream,
            endpoint: endpoint.to_owned(),
        })
    }

    /// Process ID of the server end, from `GetNamedPipeServerProcessId`. It
    /// names the process only until that process exits; the caller pins the
    /// process object before relying on it.
    ///
    /// # Errors
    ///
    /// Returns [`WindowsAttemptEndpointError::Os`] if Windows cannot report it.
    pub fn server_process_id(&self) -> Result<u32, WindowsAttemptEndpointError> {
        self.stream
            .peer_process_id()
            .map_err(|source| WindowsAttemptEndpointError::Os {
                operation: "query the server process ID",
                source,
            })
    }

    /// Session ID of the server end, from `GetNamedPipeServerSessionId`.
    ///
    /// # Errors
    ///
    /// Returns [`WindowsAttemptEndpointError::Os`] if Windows cannot report it.
    pub fn server_session_id(&self) -> Result<u32, WindowsAttemptEndpointError> {
        self.stream
            .peer_session_id()
            .map_err(|source| WindowsAttemptEndpointError::Os {
                operation: "query the server session ID",
                source,
            })
    }
}

/// Typed failure of a `keld-attempt` endpoint operation.
#[cfg(windows)]
#[derive(Debug)]
pub enum WindowsAttemptEndpointError {
    /// `KELD-IPC-008`: the name is not exactly
    /// `\\.\pipe\keld-attempt-<64 lowercase hex>`; refused before any open
    /// or creation.
    EndpointShape,
    /// `KELD-IPC-009`: a pipe with this name already exists, so this process
    /// cannot create its first instance.
    NameInUse {
        /// The `CreateNamedPipeW` failure (`ERROR_ACCESS_DENIED` or
        /// `ERROR_PIPE_BUSY`).
        source: io::Error,
    },
    /// `KELD-IPC-010`: the descriptor or pipe state read back from the live
    /// handle is not the expected form.
    SecurityMismatch {
        /// The first fact that differed.
        fact: WindowsPipeSecurityFact,
    },
    /// `KELD-IPC-011`: a caller-supplied SID is not a valid binary Windows SID.
    InvalidSid,
    /// `KELD-IPC-012`: a Windows call on the endpoint failed.
    Os {
        /// What the call was doing.
        operation: &'static str,
        /// The Windows failure.
        source: io::Error,
    },
    /// `KELD-IPC-013`: the server end of the endpoint runs in another Windows
    /// session than this client; refused before anything is sent.
    ServerSession {
        /// The server's session, from `GetNamedPipeServerSessionId`.
        server_session: u32,
        /// This client process's own session.
        own_session: u32,
    },
    /// `KELD-IPC-014`: the locator refused the connect-back IDs before any
    /// creation.
    Locator(WindowsAttemptLocatorError),
}

#[cfg(windows)]
impl fmt::Display for WindowsAttemptEndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndpointShape => f.write_str(
                "KELD-IPC-008: keld-attempt endpoint name refused before any open. \
                 Pass exactly one local `\\\\.\\pipe\\keld-attempt-<64 lowercase hex>` name; \
                 UNC, `\\\\?\\`, other `keld-*` namespaces, uppercase hex and other \
                 lengths are refused.",
            ),
            Self::NameInUse { source } => write!(
                f,
                "KELD-IPC-009: keld-attempt endpoint name already exists ({source}). \
                 Refuse this attempt before any protected write; never reuse, wait for \
                 or connect to a name that another process created."
            ),
            Self::SecurityMismatch { fact } => write!(
                f,
                "KELD-IPC-010: keld-attempt endpoint security readback mismatch ({fact}). \
                 Refuse the endpoint without sending anything: only the owner's exact \
                 form (owner, protected DACL, Medium no-write-up label, remote clients \
                 rejected, non-inheritable handle) is admitted."
            ),
            Self::InvalidSid => f.write_str(
                "KELD-IPC-011: SID is not a valid binary Windows SID. \
                 Pass the exact TokenUser SID bytes from query_windows_peer_token_facts.",
            ),
            Self::Os { operation, source } => write!(
                f,
                "KELD-IPC-012: Windows failed to {operation} for a keld-attempt endpoint \
                 ({source}). Check that the endpoint exists, and that the creating token \
                 can assign the form's owner (a Medium token cannot assign BUILTIN \
                 Administrators)."
            ),
            Self::ServerSession {
                server_session,
                own_session,
            } => write!(
                f,
                "KELD-IPC-013: keld-attempt server runs in session {server_session}, not this \
                 process's session {own_session}. Refuse the endpoint without sending \
                 anything: only an owner in the client's own session is admitted."
            ),
            Self::Locator(source) => write!(f, "{source}"),
        }
    }
}

#[cfg(windows)]
impl std::error::Error for WindowsAttemptEndpointError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NameInUse { source } | Self::Os { source, .. } => Some(source),
            Self::Locator(source) => Some(source),
            Self::EndpointShape
            | Self::SecurityMismatch { .. }
            | Self::InvalidSid
            | Self::ServerSession { .. } => None,
        }
    }
}

/// Renders a binary Windows SID as SDDL text. Only a structurally valid SID
/// (revision 1, at most 15 subauthorities, exact length) that the Windows SID
/// parser reads back unchanged is accepted.
#[cfg(windows)]
fn sid_text(binary: &[u8]) -> Result<String, WindowsAttemptEndpointError> {
    const MAX_SUB_AUTHORITIES: usize = 15;
    let [1, count, rest @ ..] = binary else {
        return Err(WindowsAttemptEndpointError::InvalidSid);
    };
    let count = usize::from(*count);
    if count > MAX_SUB_AUTHORITIES || rest.len() != 6 + 4 * count {
        return Err(WindowsAttemptEndpointError::InvalidSid);
    }
    let (authority_bytes, sub_bytes) = rest.split_at(6);
    let authority = authority_bytes
        .iter()
        .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte));
    let sub_authorities: Vec<u32> = sub_bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    let mut text = String::from("S-1-");
    let rendered = if authority >> 32 == 0 {
        write!(text, "{authority}")
    } else {
        write!(text, "0x{authority:012X}")
    };
    rendered.map_err(|_| WindowsAttemptEndpointError::InvalidSid)?;
    for value in &sub_authorities {
        write!(text, "-{value}").map_err(|_| WindowsAttemptEndpointError::InvalidSid)?;
    }
    let parsed: LocalBox<Sid> = text
        .parse()
        .map_err(|_| WindowsAttemptEndpointError::InvalidSid)?;
    if parsed.id_authority().as_slice() != authority_bytes
        || parsed.sub_authorities() != sub_authorities
    {
        return Err(WindowsAttemptEndpointError::InvalidSid);
    }
    Ok(text)
}

#[cfg(test)]
#[cfg(windows)]
mod tests;
