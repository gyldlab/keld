//! Read target identity from `DisplayConfig`. HMONITOR, enumeration order and GDI
//! source names alone are not stable display identities across topology changes.

use super::{os_error, unavailable};
use crate::WvError;
use windows_sys::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};

#[derive(Eq, PartialEq)]
pub(super) struct Target {
    adapter_high: i32,
    adapter_low: u32,
    display_config_id: u32,
    device_path: String,
}

fn active_path() -> Result<DISPLAYCONFIG_PATH_INFO, WvError> {
    // An intervening display change can enlarge the buffers. Retry that exact
    // documented error only, and never wait for an unstable desktop.
    for _ in 0..4 {
        let (mut np, mut nm) = (0, 0);
        // SAFETY: both writable counts live for the complete synchronous call.
        let status =
            unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &raw mut np, &raw mut nm) };
        if status != ERROR_SUCCESS {
            return Err(os_error("GetDisplayConfigBufferSizes", status));
        }
        // Bound temporary allocation before consulting the untrusted counts. The
        // API may overestimate, so validate the actual path count after the query.
        if !(1..=32).contains(&np) || !(1..=256).contains(&nm) {
            return Err(unavailable(
                "active display configuration exceeds capture bounds",
            ));
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
        // SAFETY: counts equal their initialized buffer lengths; QDC_ONLY_ACTIVE_PATHS
        // takes a null topology id. No array or output pointer is retained by Windows.
        let status = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &raw mut np,
                paths.as_mut_ptr(),
                &raw mut nm,
                modes.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        if status == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if status != ERROR_SUCCESS {
            return Err(os_error("QueryDisplayConfig", status));
        }
        if np != 1 || nm as usize > modes.len() || paths[0].targetInfo.targetAvailable == 0 {
            return Err(unavailable("requires a single available physical target"));
        }
        return Ok(paths[0]);
    }
    Err(unavailable("display topology did not settle"))
}

fn terminated(value: &[u16]) -> Result<&[u16], WvError> {
    let end = value
        .iter()
        .position(|&v| v == 0)
        .filter(|&i| i > 0)
        .ok_or_else(|| unavailable("missing native display identity"))?;
    Ok(&value[..end])
}

pub(super) fn read(gdi_name: &[u16]) -> Result<Target, WvError> {
    let path = active_path()?;
    let mut src = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: u32::try_from(size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>())
                .map_err(|_| unavailable("source packet size"))?,
            adapterId: path.sourceInfo.adapterId,
            id: path.sourceInfo.id,
        },
        ..Default::default()
    };
    // SAFETY: the API header describes the complete initialized repr(C) packet;
    // derive the pointer from the whole packet, not the smaller header subobject.
    let status = unsafe { DisplayConfigGetDeviceInfo((&raw mut src).cast()) };
    if status != 0 {
        return Err(os_error("DisplayConfigGetDeviceInfo(source)", status));
    }
    if terminated(&src.viewGdiDeviceName)? != gdi_name {
        return Err(unavailable(
            "monitor and active display source do not match",
        ));
    }
    let mut out = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: u32::try_from(size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>())
                .map_err(|_| unavailable("target packet size"))?,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        },
        ..Default::default()
    };
    // SAFETY: same whole-packet rule as above, this time for GET_TARGET_NAME.
    let status = unsafe { DisplayConfigGetDeviceInfo((&raw mut out).cast()) };
    if status != 0 {
        return Err(os_error("DisplayConfigGetDeviceInfo(target)", status));
    }
    let device_path = String::from_utf16(terminated(&out.monitorDevicePath)?)
        .map_err(|_| unavailable("invalid UTF-16 display target path"))?
        .to_lowercase();
    Ok(Target {
        adapter_high: path.targetInfo.adapterId.HighPart,
        adapter_low: path.targetInfo.adapterId.LowPart,
        display_config_id: path.targetInfo.id,
        device_path,
    })
}

#[cfg(test)]
mod tests;
