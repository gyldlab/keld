//! GH-1189 independent WinEvent observer for the isolated TaskDialog fixture.
//! Native execution requires independent exact-target review and a fresh RAMANI slot.
#![deny(unsafe_op_in_unsafe_fn)]
#[cfg(target_os = "windows")]
mod native {
    //! REVIEW CANDIDATE ONLY. Never build/run without independent unsafe review,
    //! owner allowance, isolated operator slot and the paired fixture manifest.

    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::os::windows::io::AsRawHandle;
    use std::process::{Child, Command, Stdio};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    };
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{
        FILETIME, HANDLE, HWND, LPARAM, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows::Win32::System::Threading::{GetProcessTimes, WaitForSingleObject};
    use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_SHOW,
        EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible, MSG,
        MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, OBJID_WINDOW, PM_REMOVE, PeekMessageW,
        QS_ALLINPUT, TranslateMessage, WINEVENT_OUTOFCONTEXT,
    };
    use windows::core::BOOL;

    #[derive(Clone, Copy)]
    struct Notice {
        kind: u32,
        hwnd: usize,
        thread: u32,
        observed: Instant,
    }
    thread_local! { static SENDER: RefCell<Option<SyncSender<Notice>>> = const { RefCell::new(None) }; }
    static CALLBACK_FAULT: AtomicBool = AtomicBool::new(false);

    #[allow(unsafe_code)]
    unsafe extern "system" fn on_event(
        _hook: HWINEVENTHOOK,
        kind: u32,
        hwnd: HWND,
        object: i32,
        child: i32,
        thread: u32,
        _time: u32,
    ) {
        let result = std::panic::catch_unwind(|| {
            if object != OBJID_WINDOW.0 || child != 0 || hwnd.0.is_null() {
                return;
            }
            let sent = SENDER
                .try_with(|slot| {
                    let Ok(slot) = slot.try_borrow() else {
                        return false;
                    };
                    match slot.as_ref() {
                        Some(tx) => tx
                            .try_send(Notice {
                                kind,
                                hwnd: hwnd.0 as usize,
                                thread,
                                observed: Instant::now(),
                            })
                            .is_ok(),
                        None => false,
                    }
                })
                .unwrap_or(false);
            if !sent {
                CALLBACK_FAULT.store(true, Ordering::SeqCst);
            }
        });
        if result.is_err() {
            CALLBACK_FAULT.store(true, Ordering::SeqCst);
        }
    }

    #[allow(unsafe_code)]
    fn start_time(handle: HANDLE) -> Result<u64, &'static str> {
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // SAFETY: borrowed live Child process handle; all outputs are valid locals.
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) }
            .map_err(|_| "GetProcessTimes failed")?;
        Ok(((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
    }

    #[allow(unsafe_code)]
    fn pid_tid(hwnd: HWND) -> (u32, u32) {
        let mut pid = 0;
        // SAFETY: query only; pid points to a live local.
        let tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        (pid, tid)
    }
    #[allow(unsafe_code)]
    fn matches_dialog(hwnd: HWND, pid: u32, thread: u32, parent: usize) -> bool {
        if pid_tid(hwnd) != (pid, thread) {
            return false;
        }
        // SAFETY: read-only owner query on the already PID-filtered window.
        let Ok(owner) = (unsafe { GetWindow(hwnd, GW_OWNER) }) else {
            return false;
        };
        !owner.0.is_null()
            && owner != hwnd
            && owner.0 as usize == parent
            && pid_tid(owner) == (pid, thread)
    }
    struct Census {
        pid: u32,
        thread: u32,
        parent: usize,
        matches: u32,
    }
    #[allow(unsafe_code)]
    unsafe extern "system" fn enum_window(hwnd: HWND, data: LPARAM) -> BOOL {
        let result = std::panic::catch_unwind(|| {
            // SAFETY: EnumWindows invokes synchronously while census owns this local.
            let c = unsafe { &mut *(data.0 as *mut Census) };
            if matches_dialog(hwnd, c.pid, c.thread, c.parent) {
                c.matches = c.matches.saturating_add(1);
            }
        });
        if result.is_err() {
            CALLBACK_FAULT.store(true, Ordering::SeqCst);
        }
        BOOL(1)
    }
    #[allow(unsafe_code)]
    fn census(pid: u32, thread: u32, parent: usize) -> Result<u32, &'static str> {
        let mut c = Census {
            pid,
            thread,
            parent,
            matches: 0,
        };
        // SAFETY: callback and context remain valid for this synchronous call.
        unsafe { EnumWindows(Some(enum_window), LPARAM((&mut c as *mut Census) as isize)) }
            .map_err(|_| "EnumWindows failed")?;
        Ok(c.matches)
    }
    // A bounded batch prevents an always-ready queue from hiding the deadline.
    #[allow(unsafe_code)]
    fn pump_once(deadline: Instant) -> Result<(), &'static str> {
        let mut msg = MSG::default();
        for _ in 0..32 {
            if Instant::now() >= deadline {
                return Err("observation deadline");
            }
            // SAFETY: MSG is local and the queue belongs to this observer thread.
            if !unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() } {
                break;
            }
            // SAFETY: MSG was produced from this thread's queue.
            unsafe {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }
        }
        let left = deadline
            .checked_duration_since(Instant::now())
            .ok_or("observation deadline")?;
        let ms = left.as_millis().min(25) as u32;
        // SAFETY: no handles; wait on this thread's message queue for at most ms.
        if unsafe { MsgWaitForMultipleObjectsEx(None, ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE) }
            == WAIT_FAILED
        {
            return Err("observer queue wait failed");
        }
        Ok(())
    }

    #[allow(unsafe_code)]
    fn observe(child: &mut Child, deadline: Instant) -> Result<(), &'static str> {
        let pid = child.id();
        let process = HANDLE(child.as_raw_handle());
        let started = start_time(process)?;
        eprintln!("GH1189 fixture pid={pid} creation={started}");
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).map_err(|_| "nonce unavailable")?;
        let generation: u64 = 1; // One dialog generation per fresh child process.
        let mut output = child.stdout.take().ok_or("child stdout unavailable")?;
        enum PhaseRead {
            Frame([u8; 57], Instant),
            Eof,
            Extra,
            Error,
        }
        let (phase_tx, phase_rx) = mpsc::sync_channel::<PhaseRead>(4);
        let _phase_reader = std::thread::spawn(move || {
            for _ in 0..3 {
                let mut frame = [0_u8; 57];
                if output.read_exact(&mut frame).is_err() {
                    let _ = phase_tx.try_send(PhaseRead::Error);
                    return;
                }
                if phase_tx
                    .try_send(PhaseRead::Frame(frame, Instant::now()))
                    .is_err()
                {
                    return;
                }
            }
            let mut extra = [0_u8; 1];
            let final_read = match output.read(&mut extra) {
                Ok(0) => PhaseRead::Eof,
                Ok(_) => PhaseRead::Extra,
                Err(_) => PhaseRead::Error,
            };
            let _ = phase_tx.try_send(final_read);
        });
        let (tx, rx) = mpsc::sync_channel::<Notice>(64);
        SENDER.with(|slot| *slot.borrow_mut() = Some(tx));
        if Instant::now() >= deadline {
            return Err("observation deadline");
        }
        // SAFETY: this out-of-context hook filters the exact nonzero child PID.
        // Callback and thread-local sender remain valid through process shutdown.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_SHOW,
                None,
                Some(on_event),
                pid,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        if hook.0.is_null() {
            return Err("hook registration failed");
        }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), &'static str> {
        let mut input = child.stdin.take().ok_or("child stdin unavailable")?;
        if Instant::now() >= deadline { return Err("observation deadline"); }
        input.write_all(b"R").map_err(|_| "readiness write failed")?;
        input.write_all(&nonce).map_err(|_| "nonce write failed")?;
        input.write_all(&started.to_le_bytes()).map_err(|_| "start identity write failed")?;
        input.write_all(&generation.to_le_bytes()).map_err(|_| "generation write failed")?;
        input.flush().map_err(|_| "readiness flush failed")?;
        let mut phases = 0_usize;
        const EXPECTED: [u8; 3] = [b'D', b'X', b'M'];
        let mut binding: Option<(usize, usize, u32)> = None; // dialog, parent, UI thread
        let mut pending = Vec::<Notice>::with_capacity(128);
        let mut creates = 0_u32;
        let mut shows = 0_u32;
        let mut destroys = 0_u32;
        let mut exited = false;
        let mut stream_eof = false;
        let mut parent_fence = false;
        let mut ack_sent = false;
        let mut cancel_at: Option<Instant> = None;
        let mut destroy_at: Option<Instant> = None;
        let mut return_at: Option<Instant> = None;
        let cleanup_bound = Duration::from_secs(5);
        loop {
            pump_once(deadline)?;
            for _ in 0..4 {
                let Ok(message) = phase_rx.try_recv() else { break; };
                let (frame, received) = match message {
                    PhaseRead::Frame(frame, received) => (frame, received),
                    PhaseRead::Eof => {
                        if !ack_sent || stream_eof { return Err("unexpected phase EOF"); }
                        stream_eof = true;
                        continue;
                    }
                    PhaseRead::Extra => return Err("extra phase bytes"),
                    PhaseRead::Error => return Err("phase stream failure"),
                };
                if phases >= EXPECTED.len() || frame[0] != EXPECTED[phases]
                    || frame[1..17] != nonce || frame[17..21] != pid.to_le_bytes()
                    || frame[21..29] != started.to_le_bytes()
                    || frame[29..37] != generation.to_le_bytes() {
                    return Err("phase or process correlation mismatch");
                }
                let thread = u32::from_le_bytes(frame[37..41].try_into().map_err(|_| "thread frame")?);
                let dialog = usize::try_from(u64::from_le_bytes(frame[41..49].try_into().map_err(|_| "dialog frame")?)).map_err(|_| "dialog width")?;
                let parent = usize::try_from(u64::from_le_bytes(frame[49..57].try_into().map_err(|_| "parent frame")?)).map_err(|_| "parent width")?;
                if thread == 0 || dialog == 0 || parent == 0 || dialog == parent { return Err("invalid fixture identity"); }
                if let Some(old) = binding {
                    if old != (dialog, parent, thread) { return Err("fixture identity changed"); }
                } else { binding = Some((dialog, parent, thread)); }
                if frame[0] == b'X' || frame[0] == b'M' {
                    let start = cancel_at.ok_or("terminal receipt preceded cancellation")?;
                    let elapsed = received.checked_duration_since(start).ok_or("terminal receipt before cancellation")?;
                    if elapsed > cleanup_bound { return Err("terminal receipt cleanup bound"); }
                    if frame[0] == b'M' { return_at = Some(received); }
                }
                phases += 1;
            }            // Preserve ordered notices received before the first identity frame.
            for _ in 0..64 {
                if Instant::now() >= deadline { return Err("observation deadline"); }
                let Ok(e) = rx.try_recv() else { break; };
                if pending.len() == 128 { return Err("notice buffer overflow"); }
                pending.push(e);
            }
            if let Some((dialog, parent, thread)) = binding {
                for e in pending.drain(..) {
                    if Instant::now() >= deadline { return Err("observation deadline"); }
                    if e.thread != thread { continue; }
                    if e.hwnd == parent && e.kind == EVENT_OBJECT_DESTROY {
                        if parent_fence || destroys != 1 || cancel_at.is_none() {
                            return Err("invalid parent destruction fence");
                        }
                        parent_fence = true;
                        continue;
                    }
                    if parent_fence && (e.hwnd == dialog || e.hwnd == parent) {
                        return Err("late event after parent fence");
                    }
                    if e.kind == EVENT_OBJECT_SHOW && e.hwnd != dialog && e.hwnd != parent {
                        return Err("unexpected UI-thread SHOW; possible decoy");
                    }
                    if e.hwnd != dialog { continue; }
                    if e.kind == EVENT_OBJECT_CREATE {
                        creates += 1;
                        if creates != 1 { return Err("reused dialog HWND generation"); }
                    } else if e.kind == EVENT_OBJECT_SHOW {
                        shows += 1;
                        if creates != 1 || shows != 1 { return Err("invalid dialog SHOW sequence"); }
                        let hwnd = HWND(dialog as *mut _);
                        // SAFETY: metadata queries only, never an observer-side write.
                        if !matches_dialog(hwnd, pid, thread, parent) || !unsafe { IsWindowVisible(hwnd).as_bool() } {
                            return Err("dialog ownership or visibility missing");
                        }
                        if census(pid, thread, parent)? != 1 { return Err("ambiguous owned-window census"); }
                        if Instant::now() >= deadline { return Err("observation deadline"); }
                        let sent = Instant::now();
                        input.write_all(b"C").map_err(|_| "cancel write failed")?;
                        input.flush().map_err(|_| "cancel flush failed")?;
                        cancel_at = Some(sent);
                    } else if e.kind == EVENT_OBJECT_DESTROY {
                        destroys += 1;
                        if creates != 1 || shows != 1 || destroys != 1 { return Err("invalid dialog DESTROY sequence"); }
                        let sent = cancel_at.ok_or("dialog destruction before cancellation")?;
                        let elapsed = e.observed.checked_duration_since(sent).ok_or("destruction observation before cancellation")?;
                        if elapsed > cleanup_bound { return Err("dialog destruction cleanup bound"); }
                        destroy_at = Some(e.observed);
                    }
                }
            }
            if CALLBACK_FAULT.load(Ordering::SeqCst) { return Err("callback overflow or fault"); }
            if let Some(sent) = cancel_at {
                if sent.elapsed() > cleanup_bound && (destroy_at.is_none() || return_at.is_none()) {
                    return Err("cleanup observation timeout");
                }
            }
            if let Some(returned) = return_at {
                if !parent_fence && returned.elapsed() > cleanup_bound {
                    // Independently bound the HWND retirement after the modal
                    // return; a successful cancel/return cannot mask a live owner.
                    return Err("parent destruction fence timeout");
                }
            }
            if parent_fence && phases == EXPECTED.len() && !ack_sent {
                if destroy_at.is_none() || return_at.is_none() { return Err("missing terminal observation"); }
                input.write_all(b"A").map_err(|_| "fence acknowledgement write failed")?;
                input.flush().map_err(|_| "fence acknowledgement flush failed")?;
                ack_sent = true;
            }
            if !exited {
                if let Some(status) = child.try_wait().map_err(|_| "child wait failed")? {
                    if !ack_sent || !status.success() { return Err("premature or unsuccessful fixture exit"); }
                    exited = true;
                }
            }
            if exited && stream_eof && ack_sent && parent_fence && phases == EXPECTED.len()
                && creates == 1 && shows == 1 && destroys == 1 {
                let Some((_, parent, thread)) = binding else { return Err("identity missing"); };
                if start_time(process)? != started { return Err("process identity changed"); }
                if census(pid, thread, parent)? != 0 { return Err("owned window remains"); }
                if Instant::now() >= deadline { return Err("observation deadline"); }
                let sent = cancel_at.ok_or("cancel timing missing")?;
                let destroyed = destroy_at.ok_or("destroy timing missing")?;
                let returned = return_at.ok_or("return timing missing")?;
                eprintln!("GH1189 timing cancel_to_destroy_observation_ms={} cancel_to_return_receipt_ms={}",
                    destroyed.duration_since(sent).as_millis(), returned.duration_since(sent).as_millis());
                return Ok(());
            }        }
    })).unwrap_or(Err("observer panic"));
        // SAFETY: called on the installing thread, with TLS callback state retained.
        // Unhook success does not establish queued-event drain or identity safety.
        let unhooked = unsafe { UnhookWinEvent(hook).as_bool() };
        if !unhooked {
            return Err("UnhookWinEvent failed");
        }
        outcome
    }
    // Rescue acts only on the original Child handle; no PID is reopened and no
    // HWND is acted on. Rescue results are always separate from fixture outcome.
    #[allow(unsafe_code)]
    fn rescue_and_hold_until_reaped(child: &mut Child) {
        let pid = child.id();
        match child.try_wait() {
            Ok(Some(status)) => {
                eprintln!("GH1189 FAIL rescue=already_exited pid={pid} status={status}");
                return;
            }
            Ok(None) => {}
            Err(e) => eprintln!("GH1189 FAIL rescue=initial_wait_error pid={pid} error={e}"),
        }
        match child.kill() {
            Ok(()) => eprintln!("GH1189 FAIL rescue=termination_requested pid={pid}"),
            Err(e) => eprintln!("GH1189 FAIL rescue=termination_error pid={pid} error={e}"),
        }
        let process = HANDLE(child.as_raw_handle());
        // SAFETY: this borrowed process handle belongs to the still-live Child;
        // Child cannot be dropped or its handle closed during the bounded wait.
        let wait = unsafe { WaitForSingleObject(process, 5_000) };
        if wait == WAIT_OBJECT_0 {
            match child.try_wait() {
                Ok(Some(status)) => {
                    eprintln!("GH1189 FAIL rescue=reaped pid={pid} status={status}");
                    return;
                }
                Ok(None) => eprintln!("GH1189 FAIL rescue=signaled_not_reaped pid={pid}"),
                Err(e) => eprintln!("GH1189 FAIL rescue=reap_error pid={pid} error={e}"),
            }
        } else if wait == WAIT_TIMEOUT {
            eprintln!("GH1189 FAIL rescue=wait_timeout pid={pid}");
        } else {
            eprintln!("GH1189 FAIL rescue=wait_failed pid={pid} result={}", wait.0);
        }
        // No automatic successful handoff exists. Retain the exact Child handle;
        // inform the operator and wait for that same process without a sleep loop.
        eprintln!(
            "GH1189 BLOCKED retaining child handle pid={pid}; operator must investigate this exact process; do not target a PID alone"
        );
        match child.wait() {
            Ok(status) => {
                eprintln!("GH1189 FAIL rescue=eventually_reaped pid={pid} status={status}")
            }
            Err(e) => {
                eprintln!(
                    "GH1189 BLOCKED reap_error pid={pid} error={e}; observer retains handle until externally resolved"
                );
                // No verified transfer of custody exists: hold Child rather than
                // silently exit and drop it. No timed sleep is used as a signal.
                loop {
                    std::thread::park();
                }
            }
        }
    }

    fn run() -> Result<(), &'static str> {
        let mut args = std::env::args_os().skip(1);
        let path = args.next().ok_or("fixture executable path missing")?;
        let forwarded: Vec<_> = args.collect();
        // The fixture is the sole owner of valid fault names. The observer
        // forwards at most one explicitly selected test-only fault.
        if !forwarded.is_empty()
            && (forwarded.len() != 2 || forwarded[0].to_str() != Some("--gh1189-fault"))
        {
            return Err("invalid fault control arguments");
        }
        let mut child = Command::new(path)
            .args(&forwarded)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "spawn failed")?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let deadline = Instant::now() + Duration::from_secs(20);
            observe(&mut child, deadline)
        }))
        .unwrap_or(Err("observer panic before or after hook"));
        if result.is_err() {
            let rescued = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                rescue_and_hold_until_reaped(&mut child);
            }));
            if rescued.is_err() {
                // In-process reporting may itself have failed. Preserve custody of
                // the exact handle until external operator intervention.
                loop {
                    std::thread::park();
                }
            }
        }
        result
    }

    pub(super) fn entry() {
        if let Err(reason) = run() {
            eprintln!("GH1189 observer FAIL: {reason}; see separate rescue evidence");
            std::process::exit(1);
        }
        println!("GH1189 observer candidate observed show, destroy, zero census, child exit");
    }
}
#[cfg(target_os = "windows")]
fn main() {
    native::entry();
}
#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("GH-1189 native fixture requires Windows; no acceptance ran");
    std::process::exit(2);
}
