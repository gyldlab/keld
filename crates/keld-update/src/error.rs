use std::fmt;

/// Why protected installation provenance was unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceUnavailable {
    /// The installer commit record is absent.
    Missing,
    /// The platform loader could read the record but could not prove it protected.
    Unprotected,
}

/// Whether immutable-version publication may have completed before the refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionPublicationOutcome {
    /// The candidate remains under its diagnostic staging name and was not published.
    StageRetained,
    /// The final version rename may have completed, but readback did not confirm it.
    DestinationUnconfirmed,
}

/// What an activation-transaction refusal leaves behind, and therefore what may happen next.
///
/// The enum is deliberately exhaustive: a caller that matches it must handle every state,
/// so adding a variant is a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationEffect {
    /// No activation journal exists for this refusal: the installation selects exactly
    /// what it selected before the call, and any version the refused attempt published
    /// was retired again under the same writer lease.
    ProtectedStateUnchanged,
    /// An activation journal exists and is authoritative, whether or not this call wrote.
    /// Only journal-bound recovery under the writer lease may continue the attempt.
    JournalBoundRecoveryRequired,
    /// The attempt is resolved and its journal removed; only never-selectable leftovers
    /// (a renamed journal or retired version trees) could not be deleted yet.
    ResolvedWithLeftovers,
    /// No journal exists, but a published version that no journal references could not
    /// be retired; every later writer halts until the explicit unjournaled-version repair
    /// retires it under the writer lease.
    UnjournaledVersionRetained,
    /// A conflicting handle holds the installation's `activation.lock`, normally the
    /// updater's exclusive writer lease, so this call read and wrote nothing. Startup
    /// selects no package while that conflict lasts.
    ///
    /// It is also every refusal of the connect-back claimant's candidate-boot read (KEL-53
    /// criterion 20, "Candidate connect-back"), the one reader exception to that lease: the
    /// read took no lease and wrote nothing, but it read the mutable records and found
    /// that they do not name the attempt whose owner accepted this process, or could not
    /// read them. The candidate starts no application code.
    WriterActive,
    /// An ordinary startup of a `MachineUacDirect` installation found a pending activation
    /// journal, or no journal with an absent or undecodable `current` beside a valid
    /// last-known-good: one that holds with previous-known-good and the floor and whose
    /// version census, completion records and package policies pass, as the per-user
    /// startup repair requires. A last-known-good that fails any of those checks refuses
    /// with that check's own error instead. In that mode only the elevated
    /// `keld-updater-helper.exe` writes, so the ordinary process inferred and wrote nothing:
    /// it assumed neither health nor process-family retirement and preserved any activation
    /// journal, the pointers and the versions. Startup selects no package; only the
    /// helper's recovery-only role resolves this state, and while this release does not
    /// provide that role the guidance says so.
    MachineRecoveryRequired(MachineRecoveryGuidance),
}

/// The one supported way out of [`ActivationEffect::MachineRecoveryRequired`].
///
/// A closed set: each variant has exactly one fix-guidance text, which every rendering of
/// the refusal carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineRecoveryGuidance {
    /// This release does not provide the recovery-only role, so no supported resolution
    /// exists other than administrator action.
    RecoveryDisabled,
    /// An unlaunched `publish-pending` attempt or an invalid `current`: run the
    /// recovery-only role now; no restart is needed.
    RecoverNow,
    /// A launched attempt: restart Windows, then run the recovery-only role.
    RestartFirst,
}

impl MachineRecoveryGuidance {
    const fn fix_guidance(self) -> &'static str {
        match self {
            Self::RecoveryDisabled => {
                "This MachineUacDirect installation needs recovery that only the recovery-only role of the elevated keld-updater-helper.exe may perform, and this release does not provide that role: no supported resolution exists other than administrator action. Start nothing from this state; any activation journal, the pointers and the versions are preserved, and no ordinary process repairs them."
            }
            Self::RecoverNow => {
                "This MachineUacDirect installation needs recovery that only the recovery-only role of the elevated keld-updater-helper.exe may perform: run that role now and approve its UAC prompt; no restart is needed. Start nothing from this state; any activation journal, the pointers and the versions are preserved, and no ordinary process repairs them."
            }
            Self::RestartFirst => {
                "This MachineUacDirect installation needs recovery that only the recovery-only role of the elevated keld-updater-helper.exe may perform: restart Windows first, then run that role and approve its UAC prompt. Start nothing from this state; the activation journal, the pointers and the versions are preserved, and no ordinary process repairs them."
            }
        }
    }
}

impl ProvenanceUnavailable {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Unprotected => "not OS-protected",
        }
    }
}

/// The executable image whose own path locates its installation (KEL-254 §4
/// executable-located rule; KEL-53 "Helper launch and self-anchor").
///
/// A closed choice of the two installed images, never a free-form name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsLocatedImage {
    /// `keld-host.exe`, the installed host.
    Host,
    /// `keld-updater-helper.exe`, the elevated updater helper (KEL-53 T4d).
    UpdaterHelper,
}

impl WindowsLocatedImage {
    /// The image's exact file name in a version tree. The helper's is `keld-pack`'s
    /// canonical member path, because the package carries it at the tree root.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Host => "keld-host.exe",
            Self::UpdaterHelper => keld_pack::UPDATER_HELPER_PATH,
        }
    }

    /// The `KELD-UPDATE-018` correction for a refusal of this image.
    const fn binding_fix(self) -> &'static str {
        match self {
            Self::Host => {
                "Launch keld-host.exe from its installation's selected version tree; repair or reinstall through the trusted installer if the layout is damaged."
            }
            Self::UpdaterHelper => {
                "Start keld-updater-helper.exe only from its installation's protected version tree, as keld-host.exe or an administrator starts it; repair or reinstall through the trusted installer if the layout is damaged."
            }
        }
    }
}

/// Provenance identity field that disagreed with the running host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceField {
    /// Explicit installer-selected Windows mode.
    InstallMode,
    /// Canonical application id.
    AppId,
    /// Requested update channel.
    Channel,
    /// Compiled target triple.
    Target,
    /// Direct installation root.
    InstallRoot,
    /// Protected update-state root.
    UpdateRoot,
    /// Identity of the compiled-in signing key.
    SigningKey,
    /// Installer baseline artifact.
    Baseline,
    /// Strict-profile identity.
    Profile,
    /// Distinct-OS-principal model.
    PrincipalModel,
    /// Persisted semantic-version floor relative to the installer baseline.
    VersionFloor,
}

impl ProvenanceField {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::InstallMode => "installMode",
            Self::AppId => "app.id",
            Self::Channel => "channel",
            Self::Target => "target",
            Self::InstallRoot => "installRoot",
            Self::UpdateRoot => "updateRoot",
            Self::SigningKey => "signingKey",
            Self::Baseline => "baseline",
            Self::Profile => "securityProfile",
            Self::PrincipalModel => "principalModel",
            Self::VersionFloor => "versionFloor",
        }
    }
}

/// Signed-manifest identity field that disagreed with the admitted installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestIdentityField {
    /// Canonical application id.
    AppId,
    /// Requested update channel.
    Channel,
    /// Compiled target triple.
    Target,
}

impl ManifestIdentityField {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AppId => "app.id",
            Self::Channel => "channel",
            Self::Target => "target",
        }
    }
}

/// Byte domain whose declared size or digest failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactDomain {
    /// Downloaded zstd-compressed artifact bytes.
    Compressed,
    /// Decompressed canonical package bytes.
    Content,
}

impl ArtifactDomain {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Compressed => "compressed artifact",
            Self::Content => "decompressed content",
        }
    }
}

/// Typed updater refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// Exact baseline authentication or native bootstrap failed.
    Baseline {
        /// Boundary that refused the operation.
        step: &'static str,
        /// Non-secret failure detail.
        detail: String,
    },
    /// A local installer record is malformed, noncanonical or unsupported.
    LocalRecordInvalid {
        /// Codec refusal detail.
        detail: String,
    },
    /// Protected provenance is absent or its protection could not be established.
    ProvenanceUnavailable {
        /// The unavailable state observed by the trusted platform loader.
        reason: ProvenanceUnavailable,
    },
    /// Another package/store mechanism owns this installation.
    ManagedInstall {
        /// Owning mechanism named by protected provenance.
        mechanism: String,
    },
    /// Protected provenance disagrees with the host or baseline contract.
    ProvenanceMismatch {
        /// Field that failed exact comparison.
        field: ProvenanceField,
        /// Expected value safe for diagnostics.
        expected: String,
        /// Observed value safe for diagnostics.
        found: String,
    },
    /// The compiled key or detached signature cannot authenticate the manifest bytes.
    ManifestAuthentication {
        /// Non-secret reason for the authentication refusal.
        detail: String,
    },
    /// The authenticated JSON is not one unambiguous v0 manifest.
    ManifestInvalid {
        /// Parser or closed-schema failure detail.
        detail: String,
    },
    /// The authenticated manifest is for another app, channel, or target.
    ManifestIdentityMismatch {
        /// Field that failed exact comparison.
        field: ManifestIdentityField,
        /// Admitted host value.
        expected: String,
        /// Signed manifest value.
        found: String,
    },
    /// The protected semantic-version floor is missing or malformed.
    VersionFloorInvalid {
        /// Failure detail without protected path contents.
        detail: String,
    },
    /// A compressed or decompressed byte count did not equal its signed bound.
    ArtifactSizeMismatch {
        /// Byte domain whose count failed.
        domain: ArtifactDomain,
        /// Signed exact count.
        expected: u64,
        /// Exact count or a lower-bound description when the stream was too long.
        observed: String,
    },
    /// Transport or canonical-content BLAKE3 did not match the signed digest.
    ArtifactDigestMismatch {
        /// Byte domain whose digest failed.
        domain: ArtifactDomain,
        /// Signed lowercase hexadecimal digest.
        expected: String,
        /// Computed lowercase hexadecimal digest.
        actual: String,
    },
    /// Stream seek/read/write or zstd decoding failed.
    ArtifactProcessing {
        /// Processing stage safe for diagnostics.
        stage: &'static str,
        /// Underlying failure detail.
        detail: String,
    },
    /// Decompressed package bytes do not match the canonical Windows v0 archive profile.
    ArchiveInvalid {
        /// Stable parser reason that does not include untrusted path bytes.
        detail: &'static str,
    },
    /// Windows root admission or unpublished extraction failed.
    Extraction {
        /// Diagnostic stage name after creation was attempted; not proof of ownership or existence.
        incomplete_stage: Option<String>,
        /// Failed boundary or I/O operation.
        step: &'static str,
        /// Underlying refusal, without archive member contents.
        detail: String,
    },
    /// Immutable version publication failed with an effect that callers must inspect.
    VersionPublication {
        /// Exact candidate version; never sufficient by itself to select the package.
        version: String,
        /// Generated diagnostic staging leaf.
        stage_name: String,
        /// Whether the final version name may now exist.
        outcome: VersionPublicationOutcome,
        /// Non-secret failure detail.
        detail: String,
    },
    /// The running executable is not bound to the installation it locates.
    ExecutableBinding {
        /// The image the executable runs as, which names the correction.
        image: WindowsLocatedImage,
        /// Selection boundary that refused the executable.
        step: &'static str,
        /// Non-secret failure detail.
        detail: String,
    },
    /// The build-time expected app identity payload is not a valid expectation.
    ExpectedIdentityInvalid {
        /// Failing part: a keld-pack payload detail, the channel or the public key.
        detail: String,
    },
    /// The verified executable image does not carry exactly one readable, canonical
    /// expected-identity container.
    ExpectedIdentityContainer {
        /// The keld-pack container-reader code, such as `KELD-PACK-007`.
        pack_code: &'static str,
        /// The keld-pack refusal message, beginning with that code.
        detail: String,
    },
    /// The verified signer of an installed executable is not the publisher or the app that
    /// the installation's protected provenance records.
    RecordedSignerMismatch {
        /// Which recorded fact differs from the verified signer.
        detail: String,
    },
    /// The updater helper refused to anchor itself to its installation before any lease
    /// or write, or its image was not derived for a launch.
    UpdaterHelper {
        /// Boundary that refused.
        step: &'static str,
        /// Non-secret failure detail.
        detail: String,
    },
    /// The common journaled activation transaction refused or could not confirm a step.
    Activation {
        /// Transaction boundary that refused the operation.
        step: &'static str,
        /// Whether protected state may have changed before the refusal.
        effect: ActivationEffect,
        /// Non-secret failure detail.
        detail: String,
    },
}

impl UpdateError {
    /// Builds the one activation-transaction refusal shape.
    #[cfg(windows)]
    pub(crate) fn activation(
        step: &'static str,
        effect: ActivationEffect,
        detail: impl fmt::Display,
    ) -> Self {
        Self::Activation {
            step,
            effect,
            detail: detail.to_string(),
        }
    }

    /// The failed step and detail of this refusal, without its code or guidance, for
    /// re-labelling under an enclosing activation refusal.
    #[cfg(windows)]
    pub(crate) fn step_and_detail(&self) -> (&'static str, String) {
        match self {
            Self::Activation { step, detail, .. } | Self::Baseline { step, detail } => {
                (step, detail.clone())
            }
            Self::LocalRecordInvalid { detail } => ("local record", detail.clone()),
            other => ("update", other.to_string()),
        }
    }

    /// Stable `KELD-UPDATE-*` code for this refusal.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Baseline { .. } => "KELD-UPDATE-013",
            Self::LocalRecordInvalid { .. } => "KELD-UPDATE-014",
            Self::ProvenanceUnavailable { .. } => "KELD-UPDATE-001",
            Self::ManagedInstall { .. } => "KELD-UPDATE-002",
            Self::ProvenanceMismatch { .. } => "KELD-UPDATE-003",
            Self::ManifestAuthentication { .. } => "KELD-UPDATE-004",
            Self::ManifestInvalid { .. } => "KELD-UPDATE-005",
            Self::ManifestIdentityMismatch { .. } => "KELD-UPDATE-006",
            Self::VersionFloorInvalid { .. } => "KELD-UPDATE-007",
            Self::ArtifactSizeMismatch { .. } => "KELD-UPDATE-008",
            Self::ArtifactDigestMismatch { .. } => "KELD-UPDATE-009",
            Self::ArtifactProcessing { .. } => "KELD-UPDATE-010",
            Self::ArchiveInvalid { .. } => "KELD-UPDATE-011",
            Self::Extraction { .. } => "KELD-UPDATE-012",
            Self::VersionPublication { .. } => "KELD-UPDATE-015",
            Self::Activation { .. } => "KELD-UPDATE-016",
            Self::ExpectedIdentityInvalid { .. } => "KELD-UPDATE-017",
            Self::ExecutableBinding { .. } => "KELD-UPDATE-018",
            Self::ExpectedIdentityContainer { .. } => "KELD-UPDATE-019",
            Self::RecordedSignerMismatch { .. } => "KELD-UPDATE-020",
            Self::UpdaterHelper { .. } => "KELD-UPDATE-021",
        }
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Baseline { step, detail } => write!(
                f,
                "KELD-UPDATE-013: baseline {step} refused ({detail}). Preserve incomplete state and repair the trusted installer configuration or protected scaffold; do not reseed an existing installation."
            ),
            Self::LocalRecordInvalid { detail } => write!(
                f,
                "KELD-UPDATE-014: invalid local installer record ({detail}). Preserve the record for diagnosis and repair or reinstall through the trusted installer; never infer protected state from its presence."
            ),
            Self::ProvenanceUnavailable { reason } => write!(
                f,
                "KELD-UPDATE-001: direct-update provenance is {}. Reinstall with a supported direct installer and restore its OS-protected commit record before checking for updates.",
                reason.as_str()
            ),
            Self::ManagedInstall { mechanism } => write!(
                f,
                "KELD-UPDATE-002: `{mechanism}` owns this installation. Use that package/store mechanism to update it; Keld will not mutate its files directly."
            ),
            Self::ProvenanceMismatch {
                field,
                expected,
                found,
            } => write!(
                f,
                "KELD-UPDATE-003: protected provenance `{}` mismatch: expected `{expected}`, found `{found}`. Stop before feed access and repair or reinstall the direct package.",
                field.as_str()
            ),
            Self::ManifestAuthentication { detail } => write!(
                f,
                "KELD-UPDATE-004: detached manifest authentication failed ({detail}). Do not parse or activate the feed; publish bytes signed by the compiled-in release key."
            ),
            Self::ExecutableBinding {
                image,
                step,
                detail,
            } => fmt_binding_error(f, *image, step, detail),
            Self::ExpectedIdentityInvalid { detail } => fmt_expectation_error(f, detail),
            Self::ExpectedIdentityContainer { detail, .. } => fmt_container_error(f, detail),
            Self::RecordedSignerMismatch { detail } => fmt_signer_error(f, detail),
            Self::UpdaterHelper { step, detail } => fmt_helper_error(f, step, detail),
            Self::ManifestInvalid { detail } => write!(
                f,
                "KELD-UPDATE-005: authenticated update manifest is not valid v0 ({detail}). Publish one closed, duplicate-free v0 manifest with canonical fields."
            ),
            Self::ManifestIdentityMismatch {
                field,
                expected,
                found,
            } => write!(
                f,
                "KELD-UPDATE-006: signed manifest `{}` mismatch: expected `{expected}`, found `{found}`. Fetch the feed for the admitted app, channel, and target.",
                field.as_str()
            ),
            Self::VersionFloorInvalid { detail } => write!(
                f,
                "KELD-UPDATE-007: protected semantic-version floor is unavailable or invalid ({detail}). Restore the protected updater state from the installer/known-good record before polling."
            ),
            Self::ArtifactSizeMismatch {
                domain,
                expected,
                observed,
            } => write!(
                f,
                "KELD-UPDATE-008: {} length mismatch: expected exactly {expected} bytes, observed {observed}. Discard the candidate and fetch the signed full artifact again.",
                domain.as_str()
            ),
            Self::ArtifactDigestMismatch {
                domain,
                expected,
                actual,
            } => write!(
                f,
                "KELD-UPDATE-009: {} BLAKE3 mismatch: expected `{expected}`, computed `{actual}`. Discard the candidate and fetch the signed full artifact again.",
                domain.as_str()
            ),
            Self::ArtifactProcessing { stage, detail } => write!(
                f,
                "KELD-UPDATE-010: full-artifact {stage} failed ({detail}). Preserve the current installation, repair the stream or staging sink, and retry."
            ),
            Self::ArchiveInvalid { detail } => write!(
                f,
                "KELD-UPDATE-011: verified full-package bytes are not a canonical Windows v0 archive ({detail}). Discard the candidate and publish a canonical package signed by the release key."
            ),
            Self::Extraction {
                incomplete_stage,
                step,
                detail,
            } => fmt_extraction_error(f, incomplete_stage.as_deref(), step, detail),
            Self::VersionPublication {
                version,
                stage_name,
                outcome,
                detail,
            } => fmt_version_publication_error(f, version, stage_name, *outcome, detail),
            Self::Activation {
                step,
                effect,
                detail,
            } => fmt_activation_error(f, step, *effect, detail),
        }
    }
}

fn fmt_binding_error(
    f: &mut fmt::Formatter<'_>,
    image: WindowsLocatedImage,
    step: &str,
    detail: &str,
) -> fmt::Result {
    write!(
        f,
        "KELD-UPDATE-018: installed executable {step} refused ({detail}). {}",
        image.binding_fix()
    )
}

fn fmt_expectation_error(f: &mut fmt::Formatter<'_>, detail: &str) -> fmt::Result {
    write!(
        f,
        "KELD-UPDATE-017: the executable's expected app identity is invalid ({detail}). Rebuild `keld-host.exe` or `keld-updater-helper.exe` so keld-pack embeds a supported channel and the release's valid Ed25519 public key; the installed image refuses to run until then."
    )
}

fn fmt_container_error(f: &mut fmt::Formatter<'_>, detail: &str) -> fmt::Result {
    write!(
        f,
        "KELD-UPDATE-019: the signed image's expected-identity container was refused ({detail}). Reinstall the signed package or rebuild `keld-host.exe` or `keld-updater-helper.exe` with `keld build`; the installed image refuses to run until it carries exactly one valid container."
    )
}

fn fmt_signer_error(f: &mut fmt::Formatter<'_>, detail: &str) -> fmt::Result {
    write!(
        f,
        "KELD-UPDATE-020: verified signer refused ({detail}). Reinstall the package from the publisher its installer recorded, signed for the app it recorded; an executable signed by another publisher or for another app never runs against this installation."
    )
}

fn fmt_helper_error(f: &mut fmt::Formatter<'_>, step: &str, detail: &str) -> fmt::Result {
    write!(
        f,
        "KELD-UPDATE-021: updater helper {step} refused ({detail}). Only a MachineUacDirect installation runs keld-updater-helper.exe, and only its journaled image or the image of the version its role requires; start nothing in its place, and repair or reinstall through the trusted installer if the installation is damaged."
    )
}

fn fmt_activation_error(
    f: &mut fmt::Formatter<'_>,
    step: &str,
    effect: ActivationEffect,
    detail: &str,
) -> fmt::Result {
    let guidance = match effect {
        ActivationEffect::ProtectedStateUnchanged => {
            "No activation journal exists and the installation selects what it did before; correct the refused input or state before a new attempt."
        }
        ActivationEffect::JournalBoundRecoveryRequired => {
            "An activation journal remains authoritative; preserve it and the versions, and continue only through journal-bound recovery under the writer lease."
        }
        ActivationEffect::ResolvedWithLeftovers => {
            "The attempt is resolved; only never-selectable leftovers remain, and a later transaction retries their deletion."
        }
        ActivationEffect::UnjournaledVersionRetained => {
            "A published version is referenced by no journal and could not be retired; later writers halt until the explicit unjournaled-version repair retires it under the writer lease. If that repair refuses an unknown or damaged entry, that entry needs manual recovery."
        }
        ActivationEffect::WriterActive => {
            "A conflicting handle, normally the updater's exclusive writer lease, holds the installation's activation lock, or an accepted candidate's boot read found that the pending attempt is not the one whose owner accepted it. Nothing was written, and only that boot read read any record; start nothing from this state, and select again only after that handle is released."
        }
        ActivationEffect::MachineRecoveryRequired(guidance) => guidance.fix_guidance(),
    };
    write!(
        f,
        "KELD-UPDATE-016: activation {step} refused ({detail}). {guidance}"
    )
}

fn fmt_extraction_error(
    f: &mut fmt::Formatter<'_>,
    incomplete_stage: Option<&str>,
    step: &str,
    detail: &str,
) -> fmt::Result {
    write!(
        f,
        "KELD-UPDATE-012: Windows extraction {step} failed ({detail}). "
    )?;
    if let Some(name) = incomplete_stage {
        write!(
            f,
            "Preserve any incomplete stage at `{name}` for diagnosis; it is not a runnable version. "
        )?;
    }
    f.write_str("Keep the current installation and repair the protected staging root or artifact before retrying.")
}

fn fmt_version_publication_error(
    f: &mut fmt::Formatter<'_>,
    version: &str,
    stage_name: &str,
    outcome: VersionPublicationOutcome,
    detail: &str,
) -> fmt::Result {
    match outcome {
        VersionPublicationOutcome::StageRetained => write!(
            f,
            "KELD-UPDATE-015: immutable version `{version}` was not published ({detail}); stage `{stage_name}` remains diagnostic. This operation does not mutate current, floor or journal; preserve any transaction journal and do not treat the stage as runnable."
        ),
        VersionPublicationOutcome::DestinationUnconfirmed => write!(
            f,
            "KELD-UPDATE-015: immutable version `{version}` may exist but its final readback was not confirmed ({detail}); `{stage_name}` is the attempted stage name, not proof of the current filesystem state. This operation does not mutate current, floor or journal; preserve any transaction journal, and never select by directory presence."
        ),
    }
}

impl std::error::Error for UpdateError {}

pub(crate) fn hex_digest(digest: &[u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest {
        let _ = fmt::Write::write_fmt(&mut output, format_args!("{byte:02x}"));
    }
    output
}

/// Whether every byte of `text` is a lowercase hexadecimal digit, the alphabet
/// [`hex_digest`] writes. Callers check the length their format requires.
///
/// Compiled where its callers are: `records` and `windows_baseline`.
#[cfg(any(windows, test, feature = "fuzzing"))]
pub(crate) fn is_lowercase_hex(text: &str) -> bool {
    text.bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
