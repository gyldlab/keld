//! Current-process macOS signing facts for publisher and local bundle admission.
//!
//! Team Identifier and signed identifier are read only after Security.framework
//! validates the running code object. The profile identity is derived by the
//! caller from these returned facts.

#![allow(unsafe_code)] // Core AGENTS.md exact current-process SecCode/CF owner.
#![deny(unsafe_op_in_unsafe_fn)]

use core::ffi::c_void;
use core::ptr;
use std::path::PathBuf;

use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::{CFURL, CFURLRef};

type SecCodeRef = *mut c_void;
type SecRequirementRef = *const c_void;
type SecStaticCodeRef = *const c_void;
type CFDictionaryRef = *const c_void;
type OSStatus = i32;

const SEC_CS_DEFAULT_FLAGS: u32 = 0;
// CommandLineTools MacOSX26.2.sdk Security.framework/Headers/SecStaticCode.h.
const SEC_CS_STRICT_VALIDATE: u32 = 1 << 4;
const SEC_CS_CHECK_ALL_ARCHITECTURES: u32 = 1 << 0;
const SEC_CS_CHECK_NESTED_CODE: u32 = 1 << 3;
// CommandLineTools MacOSX26.2.sdk Security.framework/Headers/SecCode.h.
const SEC_CS_USE_ALL_ARCHITECTURES: u32 = 1 << 0;
const SEC_CS_SIGNING_INFORMATION: u32 = 1 << 1;
// CommandLineTools MacOSX26.2.sdk Security.framework/Headers/CSCommon.h.
const SEC_CODE_SIGNATURE_ADHOC: u32 = 0x0002;
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
    fn SecCodeCopyPath(code: SecStaticCodeRef, flags: u32, path: *mut CFURLRef) -> OSStatus;
    fn SecStaticCodeCheckValidityWithErrors(
        code: SecStaticCodeRef,
        flags: u32,
        requirement: SecRequirementRef,
        errors: *mut *const c_void,
    ) -> OSStatus;

    static kSecCodeInfoFlags: CFStringRef;
    static kSecCodeInfoIdentifier: CFStringRef;
    static kSecCodeInfoMainExecutable: CFStringRef;
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
    let running_code = current_process_code()?;

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

    let static_code = current_process_static_code(&running_code, SEC_CS_DEFAULT_FLAGS)?;
    let information = current_process_signing_information(&static_code)?;

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

/// Local integrity facts, without authenticated publisher or profile authority.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MacLocalSigningInfo {
    pub(crate) bundle_path: PathBuf,
    pub(crate) main_executable: PathBuf,
    pub(crate) signing_identifier: String,
}

/// Validates the running process and all sealed code/resources at its disk origin.
///
/// This never accepts a caller-selected path. The filesystem admission owner must
/// compare the returned locations with retained, no-follow bundle handles and
/// revalidate them for each generation. Static validity is a check at this point
/// in time; it does not make a writable installation immutable.
pub(crate) fn verified_current_local_bundle_signing_info() -> Result<MacLocalSigningInfo, String> {
    let running_code = current_process_code()?;
    // SAFETY: the retained object came only from SecCodeCopySelf. A null requirement
    // asks for validity of this running code, without claiming an Apple publisher.
    let status = unsafe {
        SecCodeCheckValidity(
            running_code.as_CFTypeRef().cast_mut().cast(),
            SEC_CS_STRICT_VALIDATE,
            ptr::null(),
        )
    };
    if status != 0 {
        return Err(format!(
            "SecCodeCheckValidity rejected the local running signature with OSStatus {status}"
        ));
    }
    let static_code = current_process_static_code(&running_code, SEC_CS_USE_ALL_ARCHITECTURES)?;
    let mut validation_error = ptr::null();
    // SAFETY: this retained static object was derived from the validated current
    // process, using all architectures. The flags validate executable bytes,
    // sealed resources and nested code, including strict bundle structure. The
    // optional requirement is null; the writable error slot is owned below.
    let status = unsafe {
        SecStaticCodeCheckValidityWithErrors(
            static_code.as_CFTypeRef().cast(),
            SEC_CS_STRICT_VALIDATE | SEC_CS_CHECK_ALL_ARCHITECTURES | SEC_CS_CHECK_NESTED_CODE,
            ptr::null(),
            &raw mut validation_error,
        )
    };
    let _validation_error = if validation_error.is_null() {
        None
    } else {
        // SAFETY: Security.framework supplied one retained CFError reference in
        // the initialized output slot. Wrapping releases it on every return path.
        Some(unsafe { CFType::wrap_under_create_rule(validation_error.cast()) })
    };
    if status != 0 {
        return Err(format!(
            "full local bundle signature/resource validation failed with OSStatus {status}"
        ));
    }
    let information = current_process_signing_information(&static_code)?;
    let flags = verified_signing_value(
        &information,
        // SAFETY: this Security.framework global is a static CFStringRef key.
        unsafe { kSecCodeInfoFlags },
        "signature flags",
    )?
    .downcast_into::<CFNumber>()
    .and_then(|value| value.to_i64())
    .and_then(|value| u32::try_from(value).ok())
    .ok_or_else(|| String::from("validated signature flags are not an unsigned 32-bit CFNumber"))?;
    if flags & SEC_CODE_SIGNATURE_ADHOC == 0 {
        return Err(String::from(
            "LocalDevelopment admission requires an ad-hoc outer signature",
        ));
    }
    let signing_identifier = verified_signing_string(
        &information,
        // SAFETY: this Security.framework global is a static CFStringRef key.
        unsafe { kSecCodeInfoIdentifier },
        "signing identifier",
    )?;
    let main_executable = verified_signing_value(
        &information,
        // SAFETY: this Security.framework global is a static CFStringRef key.
        unsafe { kSecCodeInfoMainExecutable },
        "main executable",
    )?
    .downcast_into::<CFURL>()
    .and_then(|value| value.to_path())
    .ok_or_else(|| String::from("validated main executable is not a filesystem CFURL"))?;

    let mut bundle_url = ptr::null();
    // SAFETY: the static code object still represents only the validated current
    // process. The valid output slot receives a retained CFURL on success.
    let status = unsafe {
        SecCodeCopyPath(
            static_code.as_CFTypeRef().cast(),
            SEC_CS_DEFAULT_FLAGS,
            &raw mut bundle_url,
        )
    };
    if status != 0 || bundle_url.is_null() {
        return Err(format!(
            "SecCodeCopyPath failed for the validated local bundle with OSStatus {status}"
        ));
    }
    // SAFETY: SecCodeCopyPath returned a retained CFURL. This wrapper owns it, and
    // the typed downcast below refuses a mismatched object before URL conversion.
    let bundle_url = unsafe { CFType::wrap_under_create_rule(bundle_url.cast()) };
    let bundle_path = bundle_url
        .downcast_into::<CFURL>()
        .and_then(|value| value.to_path())
        .ok_or_else(|| String::from("validated bundle location is not a filesystem CFURL"))?;
    Ok(MacLocalSigningInfo {
        bundle_path,
        main_executable,
        signing_identifier,
    })
}

fn current_process_code() -> Result<CFType, String> {
    let mut running_code = ptr::null_mut();
    // SAFETY: the initialized out-pointer receives one retained SecCodeRef for
    // this process on success. It is wrapped under the create rule below.
    let status = unsafe { SecCodeCopySelf(SEC_CS_DEFAULT_FLAGS, &raw mut running_code) };
    if status != 0 || running_code.is_null() {
        return Err(format!("SecCodeCopySelf failed with OSStatus {status}"));
    }
    // SAFETY: successful SecCodeCopySelf returned a non-null retained CF object.
    Ok(unsafe { CFType::wrap_under_create_rule(running_code.cast()) })
}

fn current_process_static_code(running_code: &CFType, flags: u32) -> Result<CFType, String> {
    let mut static_code = ptr::null();
    // SAFETY: the validated SecCode remains retained, and `static_code` is a
    // valid out-pointer. Success returns one retained SecStaticCodeRef.
    let status = unsafe {
        SecCodeCopyStaticCode(
            running_code.as_CFTypeRef().cast_mut().cast(),
            flags,
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
    Ok(unsafe { CFType::wrap_under_create_rule(static_code.cast()) })
}

fn current_process_signing_information(static_code: &CFType) -> Result<CFDictionary, String> {
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
    information
        .downcast_into::<CFDictionary>()
        .ok_or_else(|| String::from("validated signing information is not a CFDictionary"))
}

fn verified_signing_string(
    information: &CFDictionary,
    raw_key: CFStringRef,
    field: &'static str,
) -> Result<String, String> {
    let value = verified_signing_value(information, raw_key, field)?
        .downcast_into::<CFString>()
        .ok_or_else(|| format!("validated {field} is not a CFString"))?
        .to_string();
    if value.is_empty() || !value.is_ascii() {
        return Err(format!("validated {field} is empty or not ASCII"));
    }
    Ok(value)
}

fn verified_signing_value(
    information: &CFDictionary,
    raw_key: CFStringRef,
    field: &'static str,
) -> Result<CFType, String> {
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
    // SAFETY: SecCodeCopySigningInformation returns a CF dictionary containing
    // CF object values. It remains alive here, and wrapping retains this non-null
    // value. Each caller checks the required CF type before accessing its data.
    Ok(unsafe { CFType::wrap_under_get_rule(raw_value) })
}
