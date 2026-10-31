//! Exact profile lifecycle identity, durable record and recovery action policy.
//! Platform engine actions, profile purge and registry ownership stay parent-local.

use serde::{Deserialize, Deserializer, Serialize};

use super::{ProfileError, ProfileErrorKind, decode_lower_hex, encode_lower_hex};

/// Strict boot-scoped identity bytes supplied by a future platform parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BootIdentity([u8; 16]);

impl BootIdentity {
    /// Accepts nonzero bytes after the platform owner validated its UUID source.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when the boot identity is absent.
    pub fn from_host_verified_bytes(bytes: [u8; 16]) -> Result<Self, ProfileError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self(bytes))
    }

    #[cfg(all(target_os = "macos", feature = "profile-test-hooks", debug_assertions))]
    pub(crate) const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for BootIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = <[u8; 16]>::deserialize(deserializer)?;
        Self::from_host_verified_bytes(bytes).map_err(serde::de::Error::custom)
    }
}

/// Exact OS process identity retained in non-idle lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProfileProcessIdentity {
    pid: u32,
    process_birth: u64,
}

impl ProfileProcessIdentity {
    /// Creates a process identity only when both OS observations are present.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when PID or process birth is absent.
    pub fn from_host_observation(pid: u32, process_birth: u64) -> Result<Self, ProfileError> {
        if pid == 0 || process_birth == 0 {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self { pid, process_birth })
    }
}

impl<'de> Deserialize<'de> for ProfileProcessIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let document = ProcessIdentityDocument::deserialize(deserializer)?;
        Self::from_host_observation(document.pid, document.process_birth)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessIdentityDocument {
    pid: u32,
    process_birth: u64,
}

/// Durable common lifecycle phase for one persistent profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileLifecyclePhase {
    /// No durable engine state remains and restart is admissible.
    Idle,
    /// Startup was committed before constructing the engine.
    Starting,
    /// The selected store is attached to the running engine.
    Running,
    /// Teardown began but its platform release barrier is incomplete.
    Stopping,
    /// A dead non-idle predecessor requires platform recovery.
    Quarantined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleRecovery {
    BootScoped(BootIdentity),
    WindowsExclusiveUdf,
}

/// Durable lifecycle record interpreted by the common phase policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileLifecycleRecord {
    phase: ProfileLifecyclePhase,
    recovery: LifecycleRecovery,
    owner: Option<ProfileProcessIdentity>,
}

impl ProfileLifecycleRecord {
    /// Creates an admissible idle record for the current boot.
    #[must_use]
    pub const fn idle(boot_identity: BootIdentity) -> Self {
        Self {
            phase: ProfileLifecyclePhase::Idle,
            recovery: LifecycleRecovery::BootScoped(boot_identity),
            owner: None,
        }
    }

    /// Creates an admissible Windows record recovered only through the
    /// exclusive-UDF browser-process release oracle.
    #[must_use]
    pub const fn windows_idle() -> Self {
        Self {
            phase: ProfileLifecyclePhase::Idle,
            recovery: LifecycleRecovery::WindowsExclusiveUdf,
            owner: None,
        }
    }

    /// Begins startup by committing the exact process owner before engine work.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` unless this record is valid idle state.
    pub fn begin_startup(self, owner: ProfileProcessIdentity) -> Result<Self, ProfileError> {
        if self.phase != ProfileLifecyclePhase::Idle || self.owner.is_some() {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self {
            phase: ProfileLifecyclePhase::Starting,
            owner: Some(owner),
            ..self
        })
    }

    /// Advances the clean startup/teardown chain by exactly one phase.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for a skipped, reversed, or recovery-only phase.
    pub fn advance(self, next: ProfileLifecyclePhase) -> Result<Self, ProfileError> {
        let allowed = matches!(
            (self.phase, next),
            (
                ProfileLifecyclePhase::Starting,
                ProfileLifecyclePhase::Running
            ) | (
                ProfileLifecyclePhase::Running,
                ProfileLifecyclePhase::Stopping
            ) | (ProfileLifecyclePhase::Stopping, ProfileLifecyclePhase::Idle)
        );
        if !allowed {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self {
            phase: next,
            owner: if next == ProfileLifecyclePhase::Idle {
                None
            } else {
                self.owner
            },
            ..self
        })
    }

    /// Marks a dead non-idle predecessor quarantined before returning an error.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` unless a non-idle record retains an owner.
    pub fn quarantine(self) -> Result<Self, ProfileError> {
        if !matches!(
            self.phase,
            ProfileLifecyclePhase::Starting
                | ProfileLifecyclePhase::Running
                | ProfileLifecyclePhase::Stopping
        ) || self.owner.is_none()
        {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self {
            phase: ProfileLifecyclePhase::Quarantined,
            ..self
        })
    }

    /// Returns the durable phase.
    #[must_use]
    pub const fn phase(&self) -> ProfileLifecyclePhase {
        self.phase
    }

    /// Completes boot-scoped quarantine only after a different validated boot.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` unless a quarantined record with an owner names a
    /// different boot than `current_boot`.
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn restore_idle_after_boot(
        self,
        current_boot: BootIdentity,
    ) -> Result<Self, ProfileError> {
        match self.recovery {
            LifecycleRecovery::BootScoped(recorded_boot)
                if self.phase == ProfileLifecyclePhase::Quarantined
                    && self.owner.is_some()
                    && recorded_boot != current_boot =>
            {
                Ok(Self::idle(current_boot))
            }
            LifecycleRecovery::BootScoped(_) | LifecycleRecovery::WindowsExclusiveUdf => {
                Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven))
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) const fn owner(&self) -> Option<ProfileProcessIdentity> {
        self.owner
    }

    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn complete_windows_recovery(self) -> Result<Self, ProfileError> {
        if self.phase != ProfileLifecyclePhase::Quarantined
            || self.owner.is_none()
            || self.recovery != LifecycleRecovery::WindowsExclusiveUdf
        {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self::windows_idle())
    }

    /// Encodes the canonical strict lifecycle record for its recovery mode.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` if the record cannot be encoded.
    pub fn to_record_bytes(&self) -> Result<Vec<u8>, ProfileError> {
        let encoded = match self.recovery {
            LifecycleRecovery::BootScoped(boot_identity) => {
                serde_json::to_vec(&BootLifecycleDocument {
                    schema: 1,
                    phase: self.phase,
                    boot_identity: encode_lower_hex(&boot_identity.0),
                    owner: self.owner,
                })
            }
            LifecycleRecovery::WindowsExclusiveUdf => {
                serde_json::to_vec(&WindowsLifecycleDocument {
                    schema: 2,
                    recovery_mode: WindowsRecoveryMode::WindowsExclusiveUdf,
                    phase: self.phase,
                    owner: self.owner,
                })
            }
        };
        encoded.map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))
    }

    /// Decodes and validates one durable lifecycle record.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for malformed, noncanonical, wrong-schema, or
    /// phase/owner-inconsistent records.
    pub fn from_record_bytes(bytes: &[u8]) -> Result<Self, ProfileError> {
        let document: AnyLifecycleDocument = serde_json::from_slice(bytes)
            .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))?;
        let record = match document {
            AnyLifecycleDocument::Boot(document) if document.schema == 1 => Self {
                phase: document.phase,
                recovery: LifecycleRecovery::BootScoped(BootIdentity::from_host_verified_bytes(
                    decode_lower_hex::<16>(&document.boot_identity)?,
                )?),
                owner: document.owner,
            },
            AnyLifecycleDocument::Windows(document)
                if document.schema == 2
                    && document.recovery_mode == WindowsRecoveryMode::WindowsExclusiveUdf =>
            {
                Self {
                    phase: document.phase,
                    recovery: LifecycleRecovery::WindowsExclusiveUdf,
                    owner: document.owner,
                }
            }
            AnyLifecycleDocument::Boot(_) | AnyLifecycleDocument::Windows(_) => {
                return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
            }
        };
        let valid_owner = match record.phase {
            ProfileLifecyclePhase::Idle => record.owner.is_none(),
            ProfileLifecyclePhase::Starting
            | ProfileLifecyclePhase::Running
            | ProfileLifecyclePhase::Stopping
            | ProfileLifecyclePhase::Quarantined => record.owner.is_some(),
        };
        if !valid_owner {
            return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
        }
        Ok(record)
    }

    /// Decodes only a boot-scoped macOS/Linux lifecycle record.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for Windows-mode or malformed records.
    pub fn from_boot_scoped_record_bytes(bytes: &[u8]) -> Result<Self, ProfileError> {
        let record = Self::from_record_bytes(bytes)?;
        if matches!(record.recovery, LifecycleRecovery::BootScoped(_)) {
            Ok(record)
        } else {
            Err(ProfileError::new(ProfileErrorKind::InvalidRecord))
        }
    }

    /// Decodes only a Windows exclusive-UDF lifecycle record.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for boot-scoped or malformed records.
    pub fn from_windows_record_bytes(bytes: &[u8]) -> Result<Self, ProfileError> {
        let record = Self::from_record_bytes(bytes)?;
        if record.recovery == LifecycleRecovery::WindowsExclusiveUdf {
            Ok(record)
        } else {
            Err(ProfileError::new(ProfileErrorKind::InvalidRecord))
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootLifecycleDocument {
    schema: u8,
    phase: ProfileLifecyclePhase,
    boot_identity: String,
    owner: Option<ProfileProcessIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WindowsRecoveryMode {
    WindowsExclusiveUdf,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowsLifecycleDocument {
    schema: u8,
    recovery_mode: WindowsRecoveryMode,
    phase: ProfileLifecyclePhase,
    owner: Option<ProfileProcessIdentity>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AnyLifecycleDocument {
    Boot(BootLifecycleDocument),
    Windows(WindowsLifecycleDocument),
}

/// Host observation of the exact process recorded in durable state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordedProcessObservation {
    /// The exact PID and birth identity is still live.
    Live,
    /// The exact recorded process identity is no longer live.
    Dead,
    /// Liveness could not be proved either way.
    Unknown,
}

/// One common lifecycle recovery action; platform effects remain future proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileLifecycleAction {
    /// Commit `starting` for a newly admitted owner.
    BeginStartup,
    /// Commit `quarantined` and fail this startup attempt.
    WriteQuarantined,
    /// Revalidate metadata, commit `idle`, and retry startup afterward.
    RestoreIdleAfterBoot,
    /// Run the Windows exclusive-UDF recovery probe without app navigation.
    RunWindowsExclusiveUdfRecovery,
}

/// Selects the next safe lifecycle action from independently supplied facts.
///
/// # Errors
///
/// Returns `KELD-WV-009` for a live owner, same-boot unknown liveness,
/// malformed idle state, or same-boot quarantine.
pub fn next_lifecycle_action(
    record: ProfileLifecycleRecord,
    process: RecordedProcessObservation,
    current_boot: BootIdentity,
) -> Result<ProfileLifecycleAction, ProfileError> {
    let LifecycleRecovery::BootScoped(recorded_boot) = record.recovery else {
        return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
    };
    match record.phase {
        ProfileLifecyclePhase::Idle if record.owner.is_none() => {
            Ok(ProfileLifecycleAction::BeginStartup)
        }
        ProfileLifecyclePhase::Quarantined if recorded_boot != current_boot => {
            Ok(ProfileLifecycleAction::RestoreIdleAfterBoot)
        }
        ProfileLifecyclePhase::Quarantined => {
            Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven))
        }
        ProfileLifecyclePhase::Starting
        | ProfileLifecyclePhase::Running
        | ProfileLifecyclePhase::Stopping
            if recorded_boot != current_boot =>
        {
            Ok(ProfileLifecycleAction::WriteQuarantined)
        }
        ProfileLifecyclePhase::Starting
        | ProfileLifecyclePhase::Running
        | ProfileLifecyclePhase::Stopping => match process {
            RecordedProcessObservation::Live => {
                Err(ProfileError::new(ProfileErrorKind::ProfileInUse))
            }
            RecordedProcessObservation::Dead => Ok(ProfileLifecycleAction::WriteQuarantined),
            RecordedProcessObservation::Unknown => {
                Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven))
            }
        },
        ProfileLifecyclePhase::Idle => Err(ProfileError::new(ProfileErrorKind::InvalidRecord)),
    }
}

/// Selects the next Windows lifecycle action without inventing a boot identity.
///
/// # Errors
///
/// Returns `KELD-WV-009` for a boot-scoped record, live/unknown predecessor,
/// or malformed idle state. A quarantined record can only start the platform
/// exclusive-UDF recovery probe; the backend commits idle after its exact
/// browser-process release event.
pub fn next_windows_lifecycle_action(
    record: ProfileLifecycleRecord,
    process: RecordedProcessObservation,
) -> Result<ProfileLifecycleAction, ProfileError> {
    if record.recovery != LifecycleRecovery::WindowsExclusiveUdf {
        return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
    }
    match record.phase {
        ProfileLifecyclePhase::Idle if record.owner.is_none() => {
            Ok(ProfileLifecycleAction::BeginStartup)
        }
        ProfileLifecyclePhase::Quarantined => {
            Ok(ProfileLifecycleAction::RunWindowsExclusiveUdfRecovery)
        }
        ProfileLifecyclePhase::Starting
        | ProfileLifecyclePhase::Running
        | ProfileLifecyclePhase::Stopping => match process {
            RecordedProcessObservation::Live => {
                Err(ProfileError::new(ProfileErrorKind::ProfileInUse))
            }
            RecordedProcessObservation::Dead => Ok(ProfileLifecycleAction::WriteQuarantined),
            RecordedProcessObservation::Unknown => {
                Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven))
            }
        },
        ProfileLifecyclePhase::Idle => Err(ProfileError::new(ProfileErrorKind::InvalidRecord)),
    }
}
