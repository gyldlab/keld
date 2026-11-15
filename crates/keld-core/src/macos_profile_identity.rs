//! Current-process macOS signing facts for KEL-135/T3.
//!
//! Team Identifier and signed identifier are read only after Security.framework
//! validates the running code object. The profile identity is derived by the
//! caller from these returned facts.

#![allow(unsafe_code)] // Core AGENTS.md KEL-135/T3 exact SecCode owner only.
#![deny(unsafe_op_in_unsafe_fn)]

use core::ffi::c_void;
use core::ptr;

use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};

type SecCodeRef = *mut c_void;
type SecRequirementRef = *const c_void;
type SecStaticCodeRef = *const c_void;
type CFDictionaryRef = *const c_void;
type OSStatus = i32;

const SEC_CS_DEFAULT_FLAGS: u32 = 0;
// Xcode 26.5.1 SDK Security.framework/Headers/SecStaticCode.h.
const SEC_CS_STRICT_VALIDATE: u32 = 1 << 4;
// Xcode 26.5.1 SDK Security.framework/Headers/SecCode.h.
const SEC_CS_SIGNING_INFORMATION: u32 = 1 << 1;
const APPLE_DEVELOPER_REQUIREMENT: &str =
    "anchor apple generic and certificate leaf[subject.OU] exists";

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecCodeCopySelf(flags: u32, self_code: *mut SecCodeRef) -> OSStatus;
    fn SecCodeCheckValidity(code: SecCodeRef, flags: u32, requirement: *const c_void) -> OSStatus;
    fn SecRequirementCreateWithString(
        requirement: CFStringRef,
        flags: u32,
        output: *mut SecRequirementRef,
    ) -> OSStatus;
    fn SecCodeCopyStaticCode(
        code: SecCodeRef,
        flags: u32,
        static_code: *mut SecStaticCodeRef,
    ) -> OSStatus;
    fn SecCodeCopySigningInformation(
        code: SecStaticCodeRef,
        flags: u32,
        information: *mut CFDictionaryRef,
    ) -> OSStatus;

    static kSecCodeInfoIdentifier: CFStringRef;
    static kSecCodeInfoTeamIdentifier: CFStringRef;
}

/// Signing information copied from a successfully validated running process.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MacSigningInfo {
    /// Apple Team Identifier from the validated code signature.
    pub(crate) team_identifier: String,
    /// Code signing identifier from the validated code signature.
    pub(crate) signing_identifier: String,
}

/// Validates the current process, then copies only the two identity fields T3 uses.
///
/// # Errors
///
/// Returns a diagnostic when Security.framework rejects the running signature,
/// or when the validated signing-information dictionary lacks either field.
pub(crate) fn verified_current_process_signing_info() -> Result<MacSigningInfo, String> {
    let mut running_code = ptr::null_mut();
    // SAFETY: `running_code` is a valid out-pointer. On success, Security.framework
    // returns one retained SecCodeRef for this process, which is wrapped below.
    let status = unsafe { SecCodeCopySelf(SEC_CS_DEFAULT_FLAGS, &raw mut running_code) };
    if status != 0 {
        return Err(format!("SecCodeCopySelf failed with OSStatus {status}"));
    }
    if running_code.is_null() {
        return Err(String::from(
            "SecCodeCopySelf returned no current-process code object",
        ));
    }
    // SAFETY: SecCodeCopySelf succeeded and returned a retained Core Foundation
    // object, so this wrapper owns exactly that create-rule reference.
    let running_code = unsafe { CFType::wrap_under_create_rule(running_code.cast()) };

    let requirement_text = CFString::new(APPLE_DEVELOPER_REQUIREMENT);
    let mut requirement = ptr::null();
    // SAFETY: `requirement_text` is a valid CFString and `requirement` is a
    // writable output pointer for the Security-owned requirement object.
    let requirement_status = unsafe {
        SecRequirementCreateWithString(
            requirement_text.as_concrete_TypeRef(),
            SEC_CS_DEFAULT_FLAGS,
            &raw mut requirement,
        )
    };
    if requirement_status != 0 || requirement.is_null() {
        return Err(format!(
            "SecRequirementCreateWithString failed with OSStatus {requirement_status}"
        ));
    }
    // SAFETY: `running_code` remains retained and represents only this process;
    // the explicit requirement accepts Apple-issued developer certificates
    // with a certificate Team Identifier before any identity fields are read.
    let status = unsafe {
        SecCodeCheckValidity(
            running_code.as_CFTypeRef().cast_mut().cast(),
            SEC_CS_STRICT_VALIDATE,
            requirement.cast(),
        )
    };
    // SAFETY: SecRequirementCreateWithString returned one retained CF object.
    let _requirement = unsafe { CFType::wrap_under_create_rule(requirement.cast()) };
    if status != 0 {
        return Err(format!(
            "SecCodeCheckValidity rejected the running signature with OSStatus {status}"
        ));
    }

    let mut static_code = ptr::null();
    // SAFETY: the validated SecCode remains retained, and `static_code` is a
    // valid out-pointer. Success returns one retained SecStaticCodeRef.
    let status = unsafe {
        SecCodeCopyStaticCode(
            running_code.as_CFTypeRef().cast_mut().cast(),
            SEC_CS_DEFAULT_FLAGS,
            &raw mut static_code,
        )
    };
    if status != 0 {
        return Err(format!(
            "SecCodeCopyStaticCode failed after validation with OSStatus {status}"
        ));
    }
    if static_code.is_null() {
        return Err(String::from(
            "SecCodeCopyStaticCode returned no validated static code object",
        ));
    }
    // SAFETY: SecCodeCopyStaticCode succeeded and returned a retained CF object.
    let static_code = unsafe { CFType::wrap_under_create_rule(static_code.cast()) };

    let mut information = ptr::null();
    // SAFETY: `static_code` is the disk representation of the already validated
    // current process. `information` is a valid out-pointer and the signing flag
    // requests the Team Identifier and code identifier keys below.
    let status = unsafe {
        SecCodeCopySigningInformation(
            static_code.as_CFTypeRef().cast(),
            SEC_CS_SIGNING_INFORMATION,
            &raw mut information,
        )
    };
    if status != 0 {
        return Err(format!(
            "SecCodeCopySigningInformation failed after validation with OSStatus {status}"
        ));
    }
    if information.is_null() {
        return Err(String::from(
            "validated code has no signing-information dictionary",
        ));
    }
    // SAFETY: Security.framework returned this retained dictionary on success.
    let information = unsafe { CFType::wrap_under_create_rule(information.cast()) };
    let information = information
        .downcast_into::<CFDictionary>()
        .ok_or_else(|| String::from("validated signing information is not a CFDictionary"))?;

    let team_identifier = verified_signing_string(
        &information,
        // SAFETY: this Security.framework global is a static CFStringRef key.
        unsafe { kSecCodeInfoTeamIdentifier },
        "Team Identifier",
    )?;
    let signing_identifier = verified_signing_string(
        &information,
        // SAFETY: this Security.framework global is a static CFStringRef key.
        unsafe { kSecCodeInfoIdentifier },
        "signing identifier",
    )?;

    Ok(MacSigningInfo {
        team_identifier,
        signing_identifier,
    })
}

#[allow(unsafe_code)] // Core AGENTS.md KEL-135/T3 permits retained CF key/value reads only.
fn verified_signing_string(
    information: &CFDictionary,
    raw_key: CFStringRef,
    field: &'static str,
) -> Result<String, String> {
    if raw_key.is_null() {
        return Err(format!("Security.framework omitted its {field} key"));
    }
    // SAFETY: the Security.framework key is a static, non-null CFStringRef.
    let key = unsafe { CFString::wrap_under_get_rule(raw_key) };
    let Some(raw_value) = information.find(key.as_CFTypeRef()) else {
        return Err(format!("validated signature has no {field}"));
    };
    let raw_value = *raw_value;
    if raw_value.is_null() {
        return Err(format!("validated signature has a null {field}"));
    }
    // SAFETY: SecCodeCopySigningInformation defines the identifier and Team
    // Identifier dictionary values as retained CFString values; the dictionary
    // remains alive for this lookup and wrapping adds one owned reference.
    let value = unsafe { CFType::wrap_under_get_rule(raw_value) };
    let value = value
        .downcast_into::<CFString>()
        .ok_or_else(|| format!("validated {field} is not a CFString"))?;
    let value = value.to_string();
    if value.is_empty() || !value.is_ascii() {
        return Err(format!("validated {field} is empty or not ASCII"));
    }
    Ok(value)
}
