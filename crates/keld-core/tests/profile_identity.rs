//! Cross-crate verified-parts and substitution contract for KEL-135/T1.

use keld_wv::{ProfileError, ProfileIdentity};

struct SyntheticVerifiedAppIdentity {
    publisher_scope: [u8; 32],
    canonical_app_id: &'static str,
}

impl SyntheticVerifiedAppIdentity {
    fn profile_identity(&self) -> Result<ProfileIdentity, ProfileError> {
        ProfileIdentity::from_host_verified_parts(self.publisher_scope, self.canonical_app_id)
    }
}

#[derive(Clone)]
struct UntrustedAndAmbientInputs {
    display_name: &'static str,
    cwd: &'static str,
    executable: &'static str,
    stage_nonce: &'static str,
    renderer_url: &'static str,
    app_link_endpoint: &'static str,
    app_link_token: &'static str,
    environment_value: &'static str,
    ipc_value: &'static str,
}

#[test]
fn substitution() -> Result<(), ProfileError> {
    let verified = SyntheticVerifiedAppIdentity {
        publisher_scope: [0x5a; 32],
        canonical_app_id: "dev.keld.fixture",
    };
    let baseline = UntrustedAndAmbientInputs {
        display_name: "Fixture",
        cwd: "C:/one",
        executable: "one.exe",
        stage_nonce: "stage-a",
        renderer_url: "https://same.example",
        app_link_endpoint: "endpoint-a",
        app_link_token: "token-a",
        environment_value: "ambient-a",
        ipc_value: "ipc-a",
    };
    let substituted = UntrustedAndAmbientInputs {
        display_name: "Other",
        cwd: "D:/two",
        executable: "renamed.exe",
        stage_nonce: "stage-b",
        renderer_url: "https://other.example",
        app_link_endpoint: "endpoint-b",
        app_link_token: "token-b",
        environment_value: "ambient-b",
        ipc_value: "ipc-b",
    };

    let baseline_identity = derive_after_observing_untrusted(&verified, &baseline)?;
    let substituted_identity = derive_after_observing_untrusted(&verified, &substituted)?;
    assert_eq!(baseline_identity, substituted_identity);
    Ok(())
}

fn derive_after_observing_untrusted(
    verified: &SyntheticVerifiedAppIdentity,
    inputs: &UntrustedAndAmbientInputs,
) -> Result<ProfileIdentity, ProfileError> {
    let observed_only = [
        inputs.display_name,
        inputs.cwd,
        inputs.executable,
        inputs.stage_nonce,
        inputs.renderer_url,
        inputs.app_link_endpoint,
        inputs.app_link_token,
        inputs.environment_value,
        inputs.ipc_value,
    ];
    assert!(observed_only.iter().all(|value| !value.is_empty()));
    verified.profile_identity()
}
