//! GH-1189 isolated Windows TaskDialog lifecycle and one-fault controls.
//! Native execution requires independent exact-target review and a fresh RAMANI slot.
#![deny(unsafe_op_in_unsafe_fn)]
#[cfg(target_os = "windows")]
mod native {
    //! REVIEW CANDIDATE ONLY. Not authorized to build/run or integrate.
    //! Revised single-process candidate. Exactly one invocation per dedicated process.
    //! One Tao thread, one TaskDialog, controller signal via an atomic. No WebView2.

    use std::cell::{Cell, RefCell};
    use std::io::{Read, Write};
    use std::rc::Rc;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::{Duration, Instant};
    use tao::event::Event;
    use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
    use tao::platform::run_return::EventLoopExtRunReturn;
    use tao::platform::windows::{
        EventLoopBuilderExtWindows, WindowBuilderExtWindows, WindowExtWindows,
    };
    use tao::window::WindowBuilder;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Controls::{
        TASKDIALOG_NOTIFICATIONS, TASKDIALOGCONFIG, TDCBF_CANCEL_BUTTON, TDF_CALLBACK_TIMER,
        TDM_CLICK_BUTTON, TDN_CREATED, TDN_DESTROYED, TDN_TIMER, TaskDialogIndirect,
    };
    use windows::Win32::UI::WindowsAndMessaging::{IDCANCEL, IsWindowVisible, SendMessageW};
    use windows::core::{HRESULT, w};

    #[derive(Clone, Copy, Debug)]
    enum Wake {
        Open,
        Finish,
        RetireParent,
        Ack,
    }

    // One explicit mode per disposable child; no combination is accepted.
    // The default path keeps the independently verified positive fixture behavior.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Fault {
        None,
        TrailingByte,
        OwnedDecoy,
        SuppressDismissal,
        HoldParent,
    }

    impl Fault {
        fn from_args() -> Self {
            let args: Vec<String> = std::env::args().skip(1).collect();
            match args.as_slice() {
                [] => Self::None,
                [flag, mode] if flag == "--gh1189-fault" => match mode.as_str() {
                    "trailing-byte" => Self::TrailingByte,
                    "owned-decoy" => Self::OwnedDecoy,
                    "suppress-dismissal" => Self::SuppressDismissal,
                    "hold-parent" => Self::HoldParent,
                    _ => panic!("unrecognized GH1189 test-only fault"),
                },
                _ => panic!("expected either no arguments or --gh1189-fault <one mode>"),
            }
        }
    }

    struct State {
        fault: Fault,
        cancel: Arc<AtomicBool>,
        visible_tx: mpsc::SyncSender<()>,
        visible_sent: AtomicBool,
        created: AtomicUsize,
        destroyed: AtomicUsize,
        requested: AtomicUsize,
        timeout: AtomicBool,
        callback_panicked: AtomicBool,
        trace_fault: AtomicBool,
        trace_tx: mpsc::SyncSender<[u8; 57]>,
        nonce: [u8; 16],
        started: u64,
        generation: u64,
        pid: u32,
        ui_thread: AtomicU32,
        ui_hwnd: AtomicUsize,
        parent_hwnd: AtomicUsize,
    }

    impl State {
        fn record(&self, tag: u8, hwnd: usize, thread: u32) {
            let mut frame = [0_u8; 57];
            frame[0] = tag;
            frame[1..17].copy_from_slice(&self.nonce);
            frame[17..21].copy_from_slice(&self.pid.to_le_bytes());
            frame[21..29].copy_from_slice(&self.started.to_le_bytes());
            frame[29..37].copy_from_slice(&self.generation.to_le_bytes());
            frame[37..41].copy_from_slice(&thread.to_le_bytes());
            frame[41..49].copy_from_slice(&(hwnd as u64).to_le_bytes());
            frame[49..57]
                .copy_from_slice(&(self.parent_hwnd.load(Ordering::SeqCst) as u64).to_le_bytes());
            if self.trace_tx.try_send(frame).is_err() {
                self.trace_fault.store(true, Ordering::SeqCst);
            }
        }
    }

    #[allow(unsafe_code)]
    unsafe extern "system" fn dialog_callback(
        hwnd: HWND,
        notification: TASKDIALOG_NOTIFICATIONS,
        elapsed: WPARAM,
        _unused: LPARAM,
        context: isize,
    ) -> HRESULT {
        // SAFETY: the fixture passes a pointer to its process-lifetime State; it retains
        // that allocation without reclaiming it for the life of this single-use process.
        // This shared reference is used only for atomic operations and try_send; it
        // is not retained across the reentrant SendMessageW below.
        let do_click = std::panic::catch_unwind(|| {
            let state = unsafe { &*(context as *const State) };
            if notification == TDN_CREATED {
                state.created.fetch_add(1, Ordering::SeqCst);
                // SAFETY: identifies only the calling UI thread; no handle is opened.
                let tid = unsafe { GetCurrentThreadId() };
                if state
                    .ui_hwnd
                    .compare_exchange(0, hwnd.0 as usize, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    state.trace_fault.store(true, Ordering::SeqCst);
                }
                state.ui_thread.store(tid, Ordering::SeqCst);
                state.record(b'D', hwnd.0 as usize, tid);
            } else if notification == TDN_DESTROYED {
                state.destroyed.fetch_add(1, Ordering::SeqCst);
                // SAFETY: identifies only the calling UI thread.
                let tid = unsafe { GetCurrentThreadId() };
                if state.ui_hwnd.load(Ordering::SeqCst) != hwnd.0 as usize
                    || state.ui_thread.load(Ordering::SeqCst) != tid
                {
                    state.trace_fault.store(true, Ordering::SeqCst);
                }
                state.record(b'X', hwnd.0 as usize, tid);
            } else if notification == TDN_TIMER {
                // SAFETY: hwnd is the live dialog handle supplied to this callback;
                // it is used synchronously on the thread receiving that callback.
                let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
                if visible && !state.visible_sent.swap(true, Ordering::SeqCst) {
                    let _ = state.visible_tx.try_send(());
                }
                let cancelled = state.cancel.load(Ordering::SeqCst);
                let timed_out = elapsed.0 >= 10_000;
                if timed_out {
                    state.timeout.store(true, Ordering::SeqCst);
                }
                if (cancelled || timed_out) && state.requested.fetch_add(1, Ordering::SeqCst) == 0 {
                    // One-fault control: the observer sent cancel, but this UI callback
                    // deliberately withholds the physical TDM_CLICK_BUTTON.
                    return state.fault != Fault::SuppressDismissal;
                }
            }
            false
        });
        match do_click {
            Ok(true) => {
                // SAFETY: callback hwnd is current and belongs to this dialog. The
                // shared State borrow has ended. The synchronous send may reenter
                // this callback, whose state operations are atomic.
                unsafe {
                    SendMessageW(
                        hwnd,
                        TDM_CLICK_BUTTON.0 as u32,
                        Some(WPARAM(IDCANCEL.0 as usize)),
                        Some(LPARAM(0)),
                    );
                }
            }
            Ok(false) => {}
            Err(_) => {
                // SAFETY: same retained allocation as above. Never unwind across ABI.
                unsafe { &*(context as *const State) }
                    .callback_panicked
                    .store(true, Ordering::SeqCst);
            }
        }
        HRESULT(0)
    }

    #[allow(unsafe_code)]
    pub(super) fn run() {
        let fault = Fault::from_args();
        // The observer must first install its process-filtered hook, then write R.
        let mut ready = [0_u8; 1];
        std::io::stdin()
            .read_exact(&mut ready)
            .expect("observer readiness");
        assert_eq!(ready[0], b'R');
        let mut context = [0_u8; 32];
        std::io::stdin()
            .read_exact(&mut context)
            .expect("observer context");
        let mut nonce = [0_u8; 16];
        nonce.copy_from_slice(&context[..16]);
        let started = u64::from_le_bytes(context[16..24].try_into().expect("start bytes"));
        let generation = u64::from_le_bytes(context[24..32].try_into().expect("generation bytes"));
        let cancel = Arc::new(AtomicBool::new(false));
        let (visible_tx, _visible_rx) = mpsc::sync_channel(1);
        let controller_cancel = Arc::clone(&cancel);
        let (control_proxy_tx, control_proxy_rx) = mpsc::sync_channel::<EventLoopProxy<Wake>>(1);
        // The child remains alive until the observer acknowledges the parent fence.
        // A failed or missing command produces no Ack; the UI deadline then fails.
        let _controller = std::thread::spawn(move || {
            let Ok(wake) = control_proxy_rx.recv_timeout(Duration::from_secs(15)) else {
                return;
            };
            let mut command = [0_u8; 1];
            if std::io::stdin().read_exact(&mut command).is_err() || command[0] != b'C' {
                return;
            }
            controller_cancel.store(true, Ordering::SeqCst);
            if std::io::stdin().read_exact(&mut command).is_err() || command[0] != b'A' {
                return;
            }
            let _ = wake.send_event(Wake::Ack);
        });
        // Callback uses nonblocking bounded sends. The one writer sends at most
        // three fixed 57-byte phase records; its acknowledgement is independently bounded.
        let (trace_tx, trace_rx) = mpsc::sync_channel::<[u8; 57]>(3);
        let (trace_done_tx, trace_done_rx) = mpsc::sync_channel::<bool>(1);
        let _trace_writer = std::thread::spawn(move || {
            let mut output = std::io::stdout().lock();
            let mut ok = true;
            for _ in 0..3 {
                match trace_rx.recv_timeout(Duration::from_secs(15)) {
                    Ok(frame) => {
                        if output
                            .write_all(&frame)
                            .and_then(|_| output.flush())
                            .is_err()
                        {
                            ok = false;
                            break;
                        }
                    }
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && fault == Fault::TrailingByte {
                // One-fault control: the three valid phase records remain unchanged.
                // The independent reader must reject this single surplus byte.
                ok = output.write_all(b"!").and_then(|_| output.flush()).is_ok();
            }
            let _ = trace_done_tx.try_send(ok);
        });
        // Deliberately retain one bounded State allocation until process termination.
        // No destructor or reclamation races a late callback in this process.
        let state: &'static State = Box::leak(Box::new(State {
            fault,
            cancel,
            visible_tx,
            visible_sent: AtomicBool::new(false),
            created: AtomicUsize::new(0),
            destroyed: AtomicUsize::new(0),
            requested: AtomicUsize::new(0),
            timeout: AtomicBool::new(false),
            callback_panicked: AtomicBool::new(false),
            trace_fault: AtomicBool::new(false),
            trace_tx,
            nonce,
            started,
            generation,
            pid: std::process::id(),
            ui_thread: AtomicU32::new(0),
            ui_hwnd: AtomicUsize::new(0),
            parent_hwnd: AtomicUsize::new(0),
        }));
        let state_ptr = (state as *const State) as isize;
        let pending = Rc::new(Cell::new(false));
        let hook_pending = Rc::clone(&pending);
        let parent = Rc::new(Cell::new(0_isize));
        let hook_parent = Rc::clone(&parent);
        let result = Rc::new(RefCell::new(None));
        let hook_result = Rc::clone(&result);
        let proxy: Rc<RefCell<Option<EventLoopProxy<Wake>>>> = Rc::new(RefCell::new(None));
        let hook_proxy = Rc::clone(&proxy);
        let mut builder = EventLoopBuilder::<Wake>::with_user_event();
        builder.with_any_thread(true).with_msg_hook(move |_| {
            if !hook_pending.replace(false) {
                return false;
            }
            let config = TASKDIALOGCONFIG {
                cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
                hwndParent: HWND(hook_parent.get() as *mut _),
                dwFlags: TDF_CALLBACK_TIMER,
                dwCommonButtons: TDCBF_CANCEL_BUTTON,
                pszWindowTitle: w!("Keld GH1189 test-only fixture"),
                pszMainInstruction: w!("Test-owned modal cancellation"),
                pfCallback: Some(dialog_callback),
                lpCallbackData: state_ptr,
                ..Default::default()
            };
            let mut button = 0;
            // SAFETY: the parent is the fixture-owned live Tao window on this UI
            // thread; config, strings, state and button remain valid through return.
            // There is no RefCell or mutable State borrow across this native call.
            let outcome = unsafe { TaskDialogIndirect(&config, Some(&mut button), None, None) };
            // SAFETY: state_ptr denotes the process-lifetime State retained above.
            let state = unsafe { &*(state_ptr as *const State) };
            state.record(
                b'M',
                state.ui_hwnd.load(Ordering::SeqCst),
                state.ui_thread.load(Ordering::SeqCst),
            );
            *hook_result.borrow_mut() = Some((outcome.is_ok(), button));
            let finish = hook_proxy.borrow().clone();
            if let Some(finish) = finish {
                let _ = finish.send_event(Wake::Finish);
            }
            false
        });
        let mut event_loop = builder.build();
        let window = WindowBuilder::new()
            .with_title("Keld GH1189 fixture owner")
            .build(&event_loop)
            .expect("fixture window");
        parent.set(window.hwnd());
        state
            .parent_hwnd
            .store(window.hwnd() as usize, Ordering::SeqCst);
        *proxy.borrow_mut() = Some(event_loop.create_proxy());
        let event_proxy = event_loop.create_proxy();
        control_proxy_tx
            .send(event_loop.create_proxy())
            .expect("controller proxy");
        let mut owned_window = Some(window);
        let mut owned_decoy = None;
        let mut acknowledged = false;
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut armed = false;
        event_loop.run_return(|event, target, flow| {
            *flow = ControlFlow::WaitUntil(deadline);
            if Instant::now() >= deadline {
                *flow = ControlFlow::Exit;
                return;
            }
            match event {
                Event::MainEventsCleared if !armed => {
                    armed = true;
                    pending.set(true);
                    event_proxy.send_event(Wake::Open).expect("fixture wake");
                }
                Event::UserEvent(Wake::Finish) => match fault {
                    Fault::OwnedDecoy => {
                        // A second *fixture-owned* HWND appears only after the real
                        // TaskDialog has returned. Tao owns creation/destruction;
                        // the independent WinEvent hook must reject its SHOW.
                        let owner = owned_window.as_ref().expect("live parent").hwnd();
                        owned_decoy = Some(
                            WindowBuilder::new()
                                .with_title("GH1189 owned late decoy")
                                .with_owner_window(owner)
                                .with_visible(true)
                                .build(target)
                                .expect("owned test decoy"),
                        );
                        event_proxy
                            .send_event(Wake::RetireParent)
                            .expect("retire after decoy SHOW");
                    }
                    Fault::HoldParent => {
                        // One-fault control: do not retire this owned parent when
                        // the dialog is finished. Observer's parent-fence bound fails.
                    }
                    _ => {
                        // Tao posts destruction; keep the loop alive to dispatch it.
                        drop(owned_window.take());
                    }
                },
                Event::UserEvent(Wake::RetireParent) => {
                    drop(owned_decoy.take());
                    drop(owned_window.take());
                }
                Event::UserEvent(Wake::Ack) => {
                    if owned_window.is_none() {
                        acknowledged = true;
                        *flow = ControlFlow::Exit;
                    }
                }
                _ => {}
            }
        });
        drop(owned_decoy);
        drop(owned_window);
        drop(event_loop);
        assert!(acknowledged, "observer fence acknowledgement missing");
        assert!(
            state.cancel.load(Ordering::SeqCst),
            "controller cancel missing"
        );
        assert_eq!(*result.borrow(), Some((true, IDCANCEL.0)));
        assert_eq!(state.created.load(Ordering::SeqCst), 1);
        assert_eq!(state.destroyed.load(Ordering::SeqCst), 1);
        assert_eq!(state.requested.load(Ordering::SeqCst), 1);
        assert!(!state.timeout.load(Ordering::SeqCst));
        assert!(!state.callback_panicked.load(Ordering::SeqCst));
        assert!(
            !state.trace_fault.load(Ordering::SeqCst),
            "trace enqueue failure"
        );
        assert_eq!(
            trace_done_rx.recv_timeout(Duration::from_secs(3)),
            Ok(true),
            "trace writer failed"
        );
    }
}
#[cfg(target_os = "windows")]
fn main() {
    native::run();
}
#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("GH-1189 native fixture requires Windows; no acceptance ran");
    std::process::exit(2);
}
