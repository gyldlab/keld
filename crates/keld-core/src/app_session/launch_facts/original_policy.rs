// Same private prepared-source descriptor on each platform; data, never a grant.
use serde::{Deserialize, Serialize};

use super::{HostAppError, invalid, valid_text};

const ORIGINAL_RESOURCE_POLICY_CONTRACT: &str = "gh473-original-editor-offline/v1";

/// A description of an independently prepared source, never a native grant.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct OriginalResourcePolicyDescriptor {
    contract: String,
    graph_sha256: String,
    policy_sha256: String,
    window_key: String,
    selector: String,
}

impl OriginalResourcePolicyDescriptor {
    pub(super) fn from_verified(graph: [u8; 32], policy: &keld_guard::ResolvedWindowCsp) -> Self {
        fn digest_text(digest: [u8; 32]) -> String {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut text = String::from("sha256:");
            for byte in digest {
                text.push(char::from(HEX[usize::from(byte >> 4)]));
                text.push(char::from(HEX[usize::from(byte & 15)]));
            }
            text
        }
        Self {
            contract: String::from(ORIGINAL_RESOURCE_POLICY_CONTRACT),
            graph_sha256: digest_text(graph),
            policy_sha256: digest_text(policy.verified_sha256()),
            window_key: policy.window_key().to_owned(),
            selector: policy.profile().as_str().to_owned(),
        }
    }

    pub(super) fn validate(&self) -> Result<(), HostAppError> {
        let digest = |value: &str| {
            value.strip_prefix("sha256:").is_some_and(|hex| {
                hex.len() == 64
                    && hex
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        };
        if self.contract != ORIGINAL_RESOURCE_POLICY_CONTRACT
            || ![
                keld_guard::WindowCspProfile::OriginalEditorOffline.as_str(),
                keld_guard::WindowCspProfile::OriginalEditorOfflineEmptyStyle.as_str(),
            ]
            .contains(&self.selector.as_str())
            || !digest(&self.graph_sha256)
            || !digest(&self.policy_sha256)
            || !valid_text(&self.window_key)
        {
            return Err(invalid("invalid original resource policy descriptor"));
        }
        Ok(())
    }
}
