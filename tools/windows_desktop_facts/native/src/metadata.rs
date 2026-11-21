use crate::error::{ProbeError, ProbeResult};
use serde_json::{Value, json};
use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows_sys::Win32::System::WindowsProgramming::GetComputerNameW;

pub(crate) fn sample() -> ProbeResult<Value> {
    let mut name = [0u16; 256];
    let mut count = name.len() as u32;
    // SAFETY: name is writable for count UTF-16 code units; count is in/out.
    if unsafe { GetComputerNameW(name.as_mut_ptr(), &mut count) } == 0 {
        // SAFETY: no pointer arguments, immediately follows the failed call.
        return Err(ProbeError::native(
            "GetComputerNameW",
            i64::from(unsafe { GetLastError() }),
        ));
    }
    let name_slice = name.get(..count as usize).ok_or_else(|| {
        ProbeError::invalid("GetComputerNameW", "Returned length exceeded buffer")
    })?;
    let name = String::from_utf16(name_slice)
        .map_err(|_| ProbeError::invalid("GetComputerNameW", "Invalid UTF-16 computer name"))?;
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: version is an initialized, correctly sized writable structure.
    let status = unsafe { RtlGetVersion(&mut version) };
    if status < 0 {
        return Err(ProbeError::native("RtlGetVersion", i64::from(status)));
    }
    Ok(json!({
        "name": name, "architecture": std::env::consts::ARCH,
        "os": {"source": "RtlGetVersion", "major": version.dwMajorVersion,
               "minor": version.dwMinorVersion, "build": version.dwBuildNumber,
               "platformId": version.dwPlatformId}
    }))
}
