use crate::error::{ProbeError, ProbeResult, utf16};
use serde_json::{Value, json};
use windows_sys::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LUID};

pub(crate) struct NativePath {
    pub(crate) source_name: String,
    pub(crate) facts: Value,
}

fn buffer<T: Default>(count: u32) -> ProbeResult<Vec<T>> {
    let mut values = Vec::new();
    values.try_reserve_exact(count as usize).map_err(|_| {
        ProbeError::invalid(
            "QueryDisplayConfig",
            "Unable to allocate OS-reported buffer size",
        )
    })?;
    values.resize_with(count as usize, T::default);
    Ok(values)
}

fn active_paths() -> ProbeResult<Vec<DISPLAYCONFIG_PATH_INFO>> {
    // Query sizes can race a topology change. Retry the documented
    // insufficient-buffer result only; no fallback identity is fabricated.
    for _ in 0..4 {
        let (mut path_count, mut mode_count) = (0, 0);
        // SAFETY: both output count pointers are valid for one u32.
        let status = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        };
        if status != ERROR_SUCCESS {
            return Err(ProbeError::native(
                "GetDisplayConfigBufferSizes",
                i64::from(status),
            ));
        }
        let mut paths = buffer::<DISPLAYCONFIG_PATH_INFO>(path_count)?;
        let mut modes = buffer::<DISPLAYCONFIG_MODE_INFO>(mode_count)?;
        // SAFETY: initialized buffers have the sizes supplied through the counts.
        // Active-path queries require a null current-topology pointer.
        let status = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        if status == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if status != ERROR_SUCCESS {
            return Err(ProbeError::native("QueryDisplayConfig", i64::from(status)));
        }
        if path_count as usize > paths.len() || mode_count as usize > modes.len() {
            return Err(ProbeError::invalid(
                "QueryDisplayConfig",
                "Returned count exceeded allocated buffer",
            ));
        }
        paths.truncate(path_count as usize);
        if paths.is_empty() {
            return Err(ProbeError::invalid(
                "QueryDisplayConfig",
                "No active display paths",
            ));
        }
        return Ok(paths);
    }
    Err(ProbeError::native(
        "QueryDisplayConfig",
        i64::from(ERROR_INSUFFICIENT_BUFFER),
    ))
}

fn luid(value: LUID) -> Value {
    json!({"lowPart": value.LowPart, "highPart": value.HighPart})
}

fn describe(path: DISPLAYCONFIG_PATH_INFO) -> ProbeResult<NativePath> {
    let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: path.sourceInfo.adapterId,
            id: path.sourceInfo.id,
        },
        ..Default::default()
    };
    // SAFETY: the correctly sized repr(C) packet starts with the header.
    // Derive the pointer from the whole packet to permit all output fields.
    let status = unsafe { DisplayConfigGetDeviceInfo((&raw mut source).cast()) };
    if status != 0 {
        return Err(ProbeError::native(
            "DisplayConfigGetDeviceInfo(source)",
            i64::from(status),
        ));
    }
    let source_name = utf16(
        &source.viewGdiDeviceName,
        "DisplayConfigGetDeviceInfo(source)",
    )?;
    if source_name.is_empty() {
        return Err(ProbeError::invalid(
            "DisplayConfigGetDeviceInfo(source)",
            "Empty GDI source name",
        ));
    }
    let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        },
        ..Default::default()
    };
    // SAFETY: the correctly sized repr(C) packet starts with the header.
    // Derive the pointer from the whole packet to permit all output fields.
    let status = unsafe { DisplayConfigGetDeviceInfo((&raw mut target).cast()) };
    if status != 0 {
        return Err(ProbeError::native(
            "DisplayConfigGetDeviceInfo(target)",
            i64::from(status),
        ));
    }
    let device_path = utf16(
        &target.monitorDevicePath,
        "DisplayConfigGetDeviceInfo(target)",
    )?;
    let friendly_name = utf16(
        &target.monitorFriendlyDeviceName,
        "DisplayConfigGetDeviceInfo(target)",
    )?;
    Ok(NativePath {
        source_name,
        facts: json!({
            "sourceAdapterLuid": luid(path.sourceInfo.adapterId), "sourceId": path.sourceInfo.id,
            "targetAdapterLuid": luid(path.targetInfo.adapterId), "targetId": path.targetInfo.id,
            "targetDevicePath": device_path, "friendlyName": friendly_name,
            "targetAvailable": path.targetInfo.targetAvailable != 0,
            "outputTechnology": path.targetInfo.outputTechnology,
            "pathFlags": path.flags,
        }),
    })
}

pub(crate) fn sample() -> ProbeResult<Vec<NativePath>> {
    active_paths()?.into_iter().map(describe).collect()
}

pub(crate) fn for_source(paths: &[NativePath], name: &str) -> ProbeResult<Vec<Value>> {
    let matching: Vec<_> = paths
        .iter()
        .filter(|path| path.source_name == name)
        .map(|path| path.facts.clone())
        .collect();
    if matching.is_empty() {
        return Err(ProbeError::invalid(
            "QueryDisplayConfig/source join",
            format!("No active path for {name}; topology may have changed"),
        ));
    }
    // Clone mode may expose multiple targets for one GDI source. Keep all of
    // them rather than silently choosing a synthetic or first-match identity.
    Ok(matching)
}
