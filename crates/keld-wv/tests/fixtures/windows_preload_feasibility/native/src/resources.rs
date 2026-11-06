//! RAII owners for this fixture only; no shared window or process registry.
use crate::pump::hr;
use serde_json::{Value, json};
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindow, RegisterClassW,
    UnregisterClassW, WINDOW_EX_STYLE, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::core::w;
pub(crate) struct Apartment;
impl Apartment {
    pub(crate) fn create() -> Result<Self, String> {
        // SAFETY: this is the process main thread and has not initialized another COM model.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|e| format!("CoInitializeEx: {e}"))?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: balances successful CoInitializeEx after all owned COM references drop.
        unsafe {
            CoUninitialize();
        }
    }
}
unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: forward this ordinary fixture HWND's message to Win32 default processing.
    unsafe { DefWindowProcW(hwnd, message, wp, lp) }
}
pub(crate) struct Window {
    pub(crate) hwnd: Option<HWND>,
    instance: HINSTANCE,
    class_registered: bool,
}
impl Window {
    pub(crate) fn create(visible: bool) -> Result<Self, String> {
        // SAFETY: obtain this executable's live module handle.
        let module = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?;
        let instance = HINSTANCE(module.0);
        let class = w!("KeldW1BoundedNativeProbe");
        let descriptor = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        };
        // SAFETY: descriptor and static class name remain valid for the registration call.
        if unsafe { RegisterClassW(&descriptor) } == 0 {
            return Err(format!(
                "RegisterClassW: {}",
                windows::core::Error::from_win32()
            ));
        }
        let mut owner = Self {
            hwnd: None,
            instance,
            class_registered: true,
        };
        let style = if visible {
            WS_OVERLAPPEDWINDOW | WS_VISIBLE
        } else {
            WS_OVERLAPPEDWINDOW
        };
        // SAFETY: creates one owned window on this thread; no parent or external HWND.
        owner.hwnd = Some(
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    w!("KELD W1 feasibility fixture"),
                    style,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    900,
                    650,
                    None,
                    None,
                    Some(instance),
                    None,
                )
            }
            .map_err(|e| e.to_string())?,
        );
        Ok(owner)
    }
    pub(crate) fn close(&mut self) -> Value {
        let mut destroyed = true;
        let mut destroy_status = "already-closed".to_owned();
        if let Some(hwnd) = self.hwnd {
            // SAFETY: this exact HWND belongs to this owner and this thread.
            let result = unsafe { DestroyWindow(hwnd) };
            destroy_status = hr(&result);
            // SAFETY: observational handle validity check after explicit destruction.
            destroyed = !unsafe { IsWindow(Some(hwnd)) }.as_bool();
            if destroyed {
                self.hwnd = None;
            }
        }
        let mut class_status = "already-unregistered".to_owned();
        if destroyed && self.class_registered {
            // SAFETY: no owned window remains for this process-local class.
            let result =
                unsafe { UnregisterClassW(w!("KeldW1BoundedNativeProbe"), Some(self.instance)) };
            class_status = hr(&result);
            if result.is_ok() {
                self.class_registered = false;
            }
        }
        json!({"windowDestroyed": destroyed, "destroyStatus": destroy_status,
            "classUnregistered": !self.class_registered, "classStatus": class_status})
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
pub(crate) struct ProcessHandle {
    pub(crate) pid: u32,
    pub(crate) kind: i32,
    pub(crate) handle: Option<HANDLE>,
}
impl ProcessHandle {
    pub(crate) fn signaled(&self) -> bool {
        self.handle.is_some_and(|handle| {
            // SAFETY: nonblocking wait on an owned live process HANDLE, never a reused PID lookup.
            (unsafe { WaitForSingleObject(handle, 0) }) == WAIT_OBJECT_0
        })
    }
    pub(crate) fn release(&mut self) -> String {
        if let Some(handle) = self.handle.take() {
            // SAFETY: close this owner’s acquired HANDLE exactly once.
            hr(&unsafe { CloseHandle(handle) })
        } else {
            "already-closed".to_owned()
        }
    }
}
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
