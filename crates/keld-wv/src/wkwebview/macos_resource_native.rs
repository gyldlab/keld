//! GH473 original webapp custom-scheme delivery on the real macOS WK owner.
//!
//! Direct native request *shape* admission occurs before Wry's request/body
//! bridge. Bytes are read and authenticated by one source worker from GH499's
//! retained webapp directory FD, returned to Tao, then delivered to the exact
//! native `WKURLSchemeTask` only if its ticket, actual view, and GH542 pending or
//! committed resource lease are still live. No renderer channel is granted.

#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    mpsc::{SyncSender, TrySendError, sync_channel},
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dispatch2::{DispatchQueue, DispatchQueueGlobalPriority, GlobalQueueIdentifier};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send,
};
use objc2_foundation::{
    NSData, NSError, NSHTTPURLResponse, NSMutableDictionary, NSObject, NSObjectProtocol, NSString,
    NSURL, NSURLRequest, ns_string,
};
use objc2_web_kit::{WKURLSchemeHandler, WKURLSchemeTask, WKWebView, WKWebViewConfiguration};
use tao::event_loop::EventLoopProxy;

use super::WkUserEvent;
use super::macos_document::{NativeDocumentOwner, ResourceLease};
use super::macos_resource_intake::{
    IntakeReject, NativeRequestShape, ORIGINAL_WEBAPP_LIMITS, TaskLedger, TaskTicket, check_shape,
    pending_metadata_charge, worst_case_charge,
};
use super::macos_resource_source::{
    AdmittedWebAppSource, ResourceSourceError, SelectedRead, VerifiedBytes,
};
use crate::WebviewId;
use crate::error::WvError;

/// Only the exact native view and its Host-signed source descriptor can select
/// an initial load; this is a private UI/worker result, not KIPC/app data.
pub(super) struct ResourceCompleted {
    pub(super) view: WebviewId,
    pub(super) ticket: TaskTicket,
    pub(super) lease: ResourceLease,
    pub(super) result: Result<VerifiedBytes, ResourceSourceError>,
    /// Remains charged while this owned payload is queued inside Tao, even if
    /// the native window is destroyed and the consumer is never called.
    _credit: RetainedByteCredit,
}

struct ReadWork {
    ticket: TaskTicket,
    lease: ResourceLease,
    selected: SelectedRead,
    credit: RetainedByteCredit,
}

// A regular-file read and Rust TLS destruction may both block. Only this
// shutdown-time OS work item joins; the UI retains native/source custody and
// observes the same completion cell through its exact private Tao nudge.
const RESOURCE_READER_SHUTDOWN_DEADLINE: Duration = Duration::from_secs(5);

struct ReaderCompletion(AtomicU8);

impl ReaderCompletion {
    const PENDING: u8 = 0;
    const JOINED: u8 = 1;
    const PANICKED: u8 = 2;
    const LOOP_LOST: u8 = 3;
}

pub(super) struct ReaderTerminated {
    pub(super) view: WebviewId,
    completion: Arc<ReaderCompletion>,
}

// The production work item captures no WK object, source clone, ledger,
// document identity, byte credit or arbitrary application callback.
enum ReaderWake {
    Tao {
        view: WebviewId,
        proxy: EventLoopProxy<WkUserEvent>,
    },
    #[cfg(test)]
    Test {
        view: WebviewId,
        sender: std::sync::mpsc::Sender<ReaderTerminated>,
    },
}

impl ReaderWake {
    fn send(self, completion: Arc<ReaderCompletion>) -> bool {
        match self {
            Self::Tao { view, proxy } => proxy
                .send_event(WkUserEvent::ReaderTerminated(ReaderTerminated {
                    view,
                    completion,
                }))
                .is_ok(),
            #[cfg(test)]
            Self::Test { view, sender } => {
                sender.send(ReaderTerminated { view, completion }).is_ok()
            }
        }
    }
}

struct ReaderCustody<T> {
    cancelled: Arc<AtomicBool>,
    submit: Option<SyncSender<T>>,
    // NeverStarted: both fields absent. Running: unique worker. Joining:
    // unique handle transferred to GCD, same outcome cell retained here.
    // Joined requires successful outcome AND its exact wake observed before
    // the original deadline. This proof stays valid while other native
    // delivery or renderer owners finish retirement.
    worker: Option<JoinHandle<()>>,
    completion: Option<Arc<ReaderCompletion>>,
    joined_proved: bool,
    shutdown_at: Option<Instant>,
    // Unproved is sticky and retains the concrete completion/native owner.
    unproved: bool,
}

impl<T> ReaderCustody<T> {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            submit: None,
            worker: None,
            completion: None,
            joined_proved: false,
            shutdown_at: None,
            unproved: false,
        }
    }

    fn install(&mut self, sender: SyncSender<T>, worker: JoinHandle<()>) {
        self.submit = Some(sender);
        self.worker = Some(worker);
    }

    fn submit(&self, work: T) -> Result<(), TrySendError<T>> {
        match self.submit.as_ref() {
            Some(sender) => sender.try_send(work),
            None => Err(TrySendError::Disconnected(work)),
        }
    }

    fn request_shutdown(&mut self, wake: ReaderWake) {
        if self.shutdown_at.is_some() {
            return;
        }
        self.shutdown_at = Some(Instant::now());
        self.cancelled.store(true, Ordering::Release);
        // Also makes recv() return when it has no work.
        self.submit.take();
        let Some(worker) = self.worker.take() else {
            return;
        };
        let completion = Arc::new(ReaderCompletion(AtomicU8::new(ReaderCompletion::PENDING)));
        self.completion = Some(Arc::clone(&completion));
        DispatchQueue::global_queue(GlobalQueueIdentifier::Priority(
            DispatchQueueGlobalPriority::Default,
        ))
        .exec_async(move || {
            // This is the only production join. The OS owns this pool work;
            // no second Rust JoinHandle needs a recursive termination proof.
            let outcome = if worker.join().is_ok() {
                ReaderCompletion::JOINED
            } else {
                ReaderCompletion::PANICKED
            };
            completion.0.store(outcome, Ordering::Release);
            if !wake.send(Arc::clone(&completion)) {
                completion
                    .0
                    .store(ReaderCompletion::LOOP_LOST, Ordering::Release);
            }
        });
    }

    fn is_shutdown_requested(&self) -> bool {
        self.shutdown_at.is_some()
    }

    fn shutdown_deadline(&self) -> Option<Instant> {
        if self.joined_proved || self.completion.is_none() {
            // No reader ever started, or the exact joined reader was proved.
            // Neither can supply a later reader-failure wake deadline.
            None
        } else {
            self.shutdown_at
                .map(|started| started + RESOURCE_READER_SHUTDOWN_DEADLINE)
        }
    }

    fn observe_termination(&mut self, event: &ReaderTerminated) -> bool {
        if self
            .completion
            .as_ref()
            .is_some_and(|completion| Arc::ptr_eq(completion, &event.completion))
        {
            if !self.unproved
                && self
                    .shutdown_deadline()
                    .is_some_and(|deadline| Instant::now() < deadline)
                && event.completion.0.load(Ordering::Acquire) == ReaderCompletion::JOINED
            {
                self.joined_proved = true;
            }
            true
        } else {
            false
        }
    }

    fn try_finish_shutdown(&mut self) -> Result<bool, WvError> {
        if self.unproved {
            return Err(WvError::EventLoop(String::from(
                "resource reader failed; native source/window custody is unproved",
            )));
        }
        if self.joined_proved {
            return Ok(true);
        }
        let Some(started) = self.shutdown_at else {
            return Ok(false);
        };
        let Some(completion) = self.completion.as_ref() else {
            // No source reader was ever installed.
            return Ok(true);
        };
        let deadline = started + RESOURCE_READER_SHUTDOWN_DEADLINE;
        if Instant::now() >= deadline {
            self.unproved = true;
            return Err(WvError::EventLoop(String::from(
                "resource reader failed to quiesce by deadline; retain native custody",
            )));
        }
        match completion.0.load(Ordering::Acquire) {
            ReaderCompletion::JOINED | ReaderCompletion::PENDING => Ok(false),
            _ => {
                self.unproved = true;
                Err(WvError::EventLoop(String::from(
                    "resource reader panicked or its completion wake was lost; retain native custody",
                )))
            }
        }
    }
}

/// Counts all retained source/input/native copy headroom even across a
/// background worker result queued after the native UI handler shut down.
/// This RAII charge follows the actual last owned copy, unlike a task ledger
/// reset at stopURLSchemeTask, and has only one aggregate owner per view.
#[derive(Debug)]
pub(super) struct RetainedByteCredit {
    account: Arc<AtomicUsize>,
    bytes: usize,
}

impl RetainedByteCredit {
    pub(super) fn reserve_more(&self, bytes: usize) -> Result<Self, IntakeReject> {
        Self::reserve(&self.account, bytes)
    }

    pub(super) fn reserve(account: &Arc<AtomicUsize>, bytes: usize) -> Result<Self, IntakeReject> {
        account
            .try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|next| *next <= ORIGINAL_WEBAPP_LIMITS.max_retained_bytes)
            })
            .map_err(|_| IntakeReject::RetainedByteLimit)?;
        Ok(Self {
            account: Arc::clone(account),
            bytes,
        })
    }
}

impl Drop for RetainedByteCredit {
    fn drop(&mut self) {
        self.account.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

struct BoundedNativeRequest {
    url: Retained<NSURL>,
    raw_url: Retained<NSString>,
    url_units: usize,
    header_units: usize,
}

struct PendingNativeTask {
    ticket: TaskTicket,
    lease: ResourceLease,
    retained_task: Retained<ProtocolObject<dyn WKURLSchemeTask>>,
    response_url: Retained<NSURL>,
    // Metadata remains in the ONE native task map while the reader is busy.
    waiting_read: Option<(SelectedRead, usize)>,
    _metadata_credit: RetainedByteCredit,
}

/// Only `MainThreadOnly` code owns WK objects. The worker owns neither the
/// native task nor document state and uses the same fixed returned result slot.
pub(super) struct HandlerIvars {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    lifecycle_census: RefCell<Option<Rc<super::s1_resource_lifecycle::CallbackCensus>>>,
    view: WebviewId,
    expected_view: Cell<Option<usize>>,
    source: Arc<AdmittedWebAppSource>,
    document: Rc<RefCell<NativeDocumentOwner>>,
    ledger: RefCell<TaskLedger>,
    retained_bytes: Arc<AtomicUsize>,
    tasks: RefCell<BTreeMap<usize, PendingNativeTask>>,
    reader: RefCell<ReaderCustody<ReadWork>>,
    wake: EventLoopProxy<WkUserEvent>,
}

define_class!(
    // SAFETY: NSObject initialized by MainThreadMarker with owned Rust ivars;
    // WK callbacks and task references stay on the WK/AppKit UI owner.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = HandlerIvars]
    pub(super) struct KeldAppSchemeHandler;

    // SAFETY: NSObject implements the inherited ObjC NSObjectProtocol.
    unsafe impl NSObjectProtocol for KeldAppSchemeHandler {}

    // SAFETY: WKURLSchemeHandler selectors have exactly the required
    // (WKWebView, WKURLSchemeTask) native ObjC ABI and MainThreadOnly lifetime.
    unsafe impl WKURLSchemeHandler for KeldAppSchemeHandler {
        #[unsafe(method(webView:startURLSchemeTask:))]
        fn start(&self, webview: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            self.start_native(webview, task);
        }

        #[unsafe(method(webView:stopURLSchemeTask:))]
        fn stop(&self, webview: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            self.stop_native(webview, task);
        }
    }
);

impl KeldAppSchemeHandler {
    /// Install once into the SAME Host-selected `WKWebViewConfiguration` before
    /// Wry's guarded empty facade builder constructs its `WKWebView`.
    #[allow(unsafe_code)] // Exact GH473 NSObject init + selected scheme registration.
    pub(super) fn install(
        view: WebviewId,
        configuration: &WKWebViewConfiguration,
        source: Arc<AdmittedWebAppSource>,
        document: Rc<RefCell<NativeDocumentOwner>>,
        wake: EventLoopProxy<WkUserEvent>,
    ) -> Result<Retained<Self>, WvError> {
        let mtm = MainThreadMarker::new().ok_or_else(|| {
            WvError::Webview(String::from(
                "original resource handler needs the AppKit UI thread",
            ))
        })?;
        // No worker exists during a partially built native WebView. The one
        // reader starts on the first authenticated WK task after binding.
        let retained_bytes = source.retained_credit_account();
        let fresh = mtm.alloc::<Self>().set_ivars(HandlerIvars {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            lifecycle_census: RefCell::new(None),
            view,
            expected_view: Cell::new(None),
            source,
            document,
            ledger: RefCell::new(TaskLedger::new(ORIGINAL_WEBAPP_LIMITS)),
            retained_bytes,
            tasks: RefCell::new(BTreeMap::new()),
            reader: RefCell::new(ReaderCustody::new()),
            wake,
        });
        // SAFETY: the owning MainThreadOnly NSObject subclass is allocated,
        // its ivars set, and standard NSObject super init completes it.
        let instance: Retained<Self> = unsafe { msg_send![super(fresh), init] };
        let scheme = NSString::from_str(instance.ivars().source.scheme());
        let handler = ProtocolObject::from_ref(&*instance);
        // SAFETY: single verified private scheme on a new selected WK
        // configuration; the catch prevents an ObjC duplicate-registration
        // exception from escaping the Host's typed build path.
        let installed = objc2::exception::catch(AssertUnwindSafe(|| unsafe {
            configuration.setURLSchemeHandler_forURLScheme(Some(handler), &scheme);
        }));
        if installed.is_err() {
            instance.request_shutdown();
            return Err(WvError::Webview(String::from(
                "verified GH473 scheme handler registration refused",
            )));
        }
        Ok(instance)
    }

    /// Called only after the real profile/controller readback on the built view
    /// succeeded. Exact view binding is nonreplaceable for this lifetime.
    pub(super) fn bind_real_view(&self, actual: &WKWebView) -> Result<(), WvError> {
        let address = std::ptr::from_ref(actual) as usize;
        match self.ivars().expected_view.get() {
            None => {
                self.ivars().expected_view.set(Some(address));
                Ok(())
            }
            Some(old) if old == address => Ok(()),
            Some(_) => Err(WvError::Webview(String::from(
                "resource handler cannot rebind a different WKWebView",
            ))),
        }
    }

    fn owns(&self, actual: &WKWebView) -> bool {
        self.ivars().expected_view.get() == Some(std::ptr::from_ref(actual) as usize)
    }

    fn ensure_reader(&self) -> Result<(), WvError> {
        let mut reader = self.ivars().reader.borrow_mut();
        if reader.is_shutdown_requested() {
            return Err(WvError::EventLoop(String::from(
                "resource reader already stopped",
            )));
        }
        if reader.worker.is_some() {
            return Ok(());
        }
        let (submit, receiver) = sync_channel::<ReadWork>(1);
        let source = Arc::clone(&self.ivars().source);
        let wake = self.ivars().wake.clone();
        let view = self.ivars().view;
        let cancelled = Arc::clone(&reader.cancelled);
        let worker = std::thread::Builder::new()
            .name(String::from("keld-static-webapp"))
            .spawn(move || {
                while let Ok(read) = receiver.recv() {
                    if cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    // Only the retained descriptor, immutable asset facts and
                    // the cancellation flag reach this single I/O reader.
                    let result = source.verified_read(
                        &read.selected,
                        ORIGINAL_WEBAPP_LIMITS.max_asset_bytes,
                        &cancelled,
                    );
                    if cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    if wake
                        .send_event(WkUserEvent::Resource(ResourceCompleted {
                            view,
                            ticket: read.ticket,
                            lease: read.lease,
                            result,
                            _credit: read.credit,
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| {
                WvError::Webview(format!("cannot start GH473 resource reader: {error}"))
            })?;
        reader.install(submit, worker);
        Ok(())
    }

    pub(super) fn current_document(&self) -> &Rc<RefCell<NativeDocumentOwner>> {
        &self.ivars().document
    }

    pub(super) fn selected_source(&self) -> &Arc<AdmittedWebAppSource> {
        &self.ivars().source
    }

    fn key(task: &ProtocolObject<dyn WKURLSchemeTask>) -> usize {
        std::ptr::from_ref(task).cast::<()>() as usize
    }

    fn admit_native_ticket(
        &self,
        view: &WKWebView,
        task: &ProtocolObject<dyn WKURLSchemeTask>,
    ) -> Option<TaskTicket> {
        if self.ivars().reader.borrow().is_shutdown_requested() {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if Self::resource_trace_enabled() {
                Self::trace_resource("admission-shutdown", None, None, None, None, None);
            }
            // Destroy/Quit already tombstoned the native session. WebKit may
            // still race in with a start, but no callback may follow its stop.
            return None;
        }
        if !self.owns(view) {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if Self::resource_trace_enabled() {
                Self::trace_resource("admission-view-mismatch", None, None, None, None, None);
            }
            Self::send_error(
                task,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                self,
            );
            return None;
        }
        let key = Self::key(task);
        // End the ledger borrow before any native failure callback can reenter.
        let admission = self.ivars().ledger.borrow_mut().begin(key);
        let ticket = match admission {
            Ok(ticket) => ticket,
            Err(IntakeReject::DuplicateNativeTask) => {
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                if Self::resource_trace_enabled() {
                    Self::trace_resource("admission-duplicate-task", None, None, None, None, None);
                }
                return None;
            }
            Err(reason) => {
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                if Self::resource_trace_enabled() {
                    let category = match reason {
                        IntakeReject::NoTaskCapacity => "admission-task-capacity",
                        IntakeReject::TaskIncarnationExhausted => "admission-incarnation-exhausted",
                        _ => "admission-other-ledger-refusal",
                    };
                    Self::trace_resource(category, None, None, None, None, None);
                }
                let _ = reason;
                Self::send_error(
                    task,
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    self,
                );
                return None;
            }
        };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if Self::resource_trace_enabled() {
            Self::trace_resource("native-start", Some(ticket), None, None, None, None);
        }
        Some(ticket)
    }

    #[allow(unsafe_code)] // One exact current WKURLSchemeTask request getter.
    fn start_native(&self, view: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
        let Some(ticket) = self.admit_native_ticket(view, task) else {
            return;
        };
        // SAFETY: WebKit supplied this live task for the just-checked
        // receiving native view. No native object escapes the UI thread.
        let request = unsafe { task.request() };
        let Some(BoundedNativeRequest {
            url,
            raw_url,
            url_units,
            header_units,
        }) = Self::bounded_native_request(&request)
        else {
            self.reject_ticket(task, ticket);
            return;
        };
        let current = self.ivars().document.borrow();
        let Some(lease) = current.active_resource_lease(self.ivars().source.source_id()) else {
            drop(current);
            self.reject_ticket(task, ticket);
            return;
        };
        drop(current);
        // The native length, method, body, header count and total lengths have
        // been bounded BEFORE any Keld-owned string/URL/header copy.
        let Ok(selected) = self.ivars().source.qualify_url(&raw_url.to_string()) else {
            self.reject_ticket(task, ticket);
            return;
        };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if Self::resource_trace_enabled() {
            Self::trace_resource(
                "selected",
                Some(ticket),
                Some(self.ivars().document.borrow().is_current_lease(lease)),
                Some(selected.size),
                Some(selected.sha256),
                Some(selected.mime),
            );
        }
        let Some(units) = url_units.checked_add(header_units) else {
            self.reject_ticket(task, ticket);
            return;
        };
        self.queue_selected(task, ticket, lease, url, units, selected);
    }

    fn queue_selected(
        &self,
        task: &ProtocolObject<dyn WKURLSchemeTask>,
        ticket: TaskTicket,
        lease: ResourceLease,
        url: Retained<NSURL>,
        units: usize,
        selected: SelectedRead,
    ) {
        let Ok(body_charge) = worst_case_charge(selected.size, 0, 0, ORIGINAL_WEBAPP_LIMITS) else {
            self.reject_ticket(task, ticket);
            return;
        };
        // No body credit or read is admitted while only metadata is pending.
        // Include the owned selected path and the native-task map record.
        let Ok(metadata_charge) = pending_metadata_charge(
            units,
            selected.path.capacity(),
            std::mem::size_of::<PendingNativeTask>(),
            self.ivars().source.response_policy().len(),
            ORIGINAL_WEBAPP_LIMITS,
        ) else {
            self.reject_ticket(task, ticket);
            return;
        };
        let Ok(metadata_credit) =
            RetainedByteCredit::reserve(&self.ivars().retained_bytes, metadata_charge)
        else {
            self.reject_ticket(task, ticket);
            return;
        };
        let admitted = {
            let mut ledger = self.ivars().ledger.borrow_mut();
            ledger
                .reserve_bytes(ticket, metadata_charge)
                .and_then(|()| ledger.queue_read(ticket))
        };
        if admitted.is_err() {
            self.reject_ticket(task, ticket);
            return;
        }
        self.ivars().tasks.borrow_mut().insert(
            ticket.native_identity(),
            PendingNativeTask {
                ticket,
                lease,
                retained_task: task.retain(),
                response_url: url,
                waiting_read: Some((selected, body_charge)),
                _metadata_credit: metadata_credit,
            },
        );
        self.dispatch_waiting_read();
    }

    fn dispatch_waiting_read(&self) {
        if self.ivars().reader.borrow().is_shutdown_requested() {
            return;
        }
        loop {
            let next = self.ivars().ledger.borrow().next_queued_read();
            let Some(ticket) = next else {
                return;
            };
            let key = ticket.native_identity();
            let waiting = {
                let mut tasks = self.ivars().tasks.borrow_mut();
                tasks
                    .get_mut(&key)
                    .filter(|pending| pending.ticket == ticket)
                    .and_then(|pending| {
                        pending
                            .waiting_read
                            .take()
                            .map(|(selected, charge)| (pending.lease, selected, charge))
                    })
            };
            let Some((lease, selected, charge)) = waiting else {
                self.complete_without_delivery(ticket);
                continue;
            };
            if self.ivars().expected_view.get().is_none()
                || !self.ivars().document.borrow().is_current_lease(lease)
            {
                drop(selected);
                let removed = self.ivars().tasks.borrow_mut().remove(&key);
                drop(removed);
                self.complete_without_delivery(ticket);
                continue;
            }
            if self.submit_selected(ticket, lease, selected, charge) {
                return;
            }
        }
    }

    fn submit_selected(
        &self,
        ticket: TaskTicket,
        lease: ResourceLease,
        selected: SelectedRead,
        charge: usize,
    ) -> bool {
        let Ok(credit) = RetainedByteCredit::reserve(&self.ivars().retained_bytes, charge) else {
            drop(selected);
            self.reject_pending(ticket);
            return false;
        };
        let admitted = self
            .ivars()
            .ledger
            .borrow_mut()
            .reserve_bytes(ticket, charge);
        if admitted.is_err() || self.ensure_reader().is_err() {
            drop(selected);
            drop(credit);
            self.reject_pending(ticket);
            return false;
        }
        let started = self.ivars().ledger.borrow_mut().worker_started(ticket);
        if started.is_err() {
            drop(selected);
            drop(credit);
            self.reject_pending(ticket);
            return false;
        }
        let work = ReadWork {
            ticket,
            lease,
            selected,
            credit,
        };
        let queued = self.ivars().reader.borrow().submit(work);
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if Self::resource_trace_enabled() {
            Self::trace_resource(
                if queued.is_err() {
                    "submit-refused"
                } else {
                    "submit-accepted"
                },
                Some(ticket),
                Some(self.ivars().document.borrow().is_current_lease(lease)),
                None,
                None,
                None,
            );
        }
        if queued.is_ok() {
            return true;
        }
        // Drop the unobserved work before releasing either accounting owner.
        // There is no retry on Full or Disconnected.
        drop(queued);
        let _ = self.ivars().ledger.borrow_mut().worker_quiesced(ticket);
        self.reject_pending(ticket);
        false
    }

    fn reject_pending(&self, ticket: TaskTicket) {
        let pending = self
            .ivars()
            .tasks
            .borrow_mut()
            .remove(&ticket.native_identity());
        if let Some(pending) = pending {
            // The pending owner and metadata charge span the native callback.
            self.reject_ticket(&pending.retained_task, ticket);
        } else {
            self.complete_without_delivery(ticket);
        }
    }

    fn bounded_native_request(request: &NSURLRequest) -> Option<BoundedNativeRequest> {
        if !request
            .HTTPMethod()
            .is_some_and(|method| method.isEqualToString(ns_string!("GET")))
        {
            return None;
        }
        // Check body/stream PRESENCE, never touch read/bytes from the stream.
        if request.HTTPBody().is_some() || request.HTTPBodyStream().is_some() {
            return None;
        }
        let url = request.URL()?;
        let raw_url = url.absoluteString()?;
        let url_units = raw_url.length();
        if url_units == 0 || url_units > ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units {
            return None;
        }
        let headers = request.allHTTPHeaderFields();
        let count = headers.as_ref().map_or(0, |h| h.len());
        if count > ORIGINAL_WEBAPP_LIMITS.max_header_fields {
            return None;
        }
        let mut header_units = 0_usize;
        if let Some(headers) = headers {
            for key in headers.keys() {
                let value = headers.objectForKey(&key)?;
                let next = header_units
                    .checked_add(key.length())
                    .and_then(|n| n.checked_add(value.length()))?;
                if next > ORIGINAL_WEBAPP_LIMITS.max_header_utf16_units {
                    return None;
                }
                header_units = next;
            }
        }
        if check_shape(
            NativeRequestShape {
                is_get: true,
                has_body: false,
                has_body_stream: false,
                url_utf16_units: url_units,
                header_fields: count,
                header_utf16_units: header_units,
            },
            ORIGINAL_WEBAPP_LIMITS,
        )
        .is_err()
        {
            return None;
        }
        Some(BoundedNativeRequest {
            url,
            raw_url,
            url_units,
            header_units,
        })
    }

    /// Last request after stop/destruction receives no further native callback.
    /// Worker still owns its bytes until completion; do not release on stop.
    fn stop_native(&self, view: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
        if !self.owns(view) {
            return;
        }
        self.ivars().ledger.borrow_mut().stop(Self::key(task));
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if Self::resource_trace_enabled() {
            let observed = self
                .ivars()
                .tasks
                .borrow()
                .get(&Self::key(task))
                .map(|pending| (pending.ticket, pending.lease));
            let current =
                observed.map(|(_, lease)| self.ivars().document.borrow().is_current_lease(lease));
            Self::trace_resource(
                "native-stop",
                observed.map(|(ticket, _)| ticket),
                current,
                None,
                None,
                None,
            );
        }

        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(census) = self.ivars().lifecycle_census.borrow().as_ref() {
            census.stopped(Self::key(task));
        }
        let key = Self::key(task);
        let pending_only = self
            .ivars()
            .tasks
            .borrow()
            .get(&key)
            .is_some_and(|pending| pending.waiting_read.is_some());
        if pending_only {
            let removed = self.ivars().tasks.borrow_mut().remove(&key);
            if let Some(pending) = removed {
                let ticket = pending.ticket;
                drop(pending);
                self.complete_without_delivery(ticket);
            }
            self.dispatch_waiting_read();
        }
    }

    pub(super) fn deliver(&self, response: ResourceCompleted) {
        if response.view != self.ivars().view {
            return;
        }
        let ticket = response.ticket;
        let started = self.ivars().ledger.borrow_mut().delivery_started(ticket);
        if started.is_err() {
            return;
        }
        self.deliver_response(response);
        // Response credit and every local native reference have now dropped.
        let _ = self.ivars().ledger.borrow_mut().delivery_quiesced(ticket);
        self.complete_without_delivery(ticket);
        self.dispatch_waiting_read();
    }

    fn deliver_response(&self, response: ResourceCompleted) {
        if response.view != self.ivars().view {
            return;
        }
        let ticket = response.ticket;
        let key = ticket.native_identity();
        // The wrapper holds delivery custody until this function drops every
        // response buffer, native reference and source-wide byte credit.
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        self.trace_worker_result(&response);
        let Some(pending) = self.ivars().tasks.borrow_mut().remove(&key) else {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if Self::resource_trace_enabled() {
                Self::trace_resource("drop-no-pending-task", Some(ticket), None, None, None, None);
            }
            return;
        };
        if pending.ticket != ticket
            || pending.lease != response.lease
            || !self
                .ivars()
                .document
                .borrow()
                .is_current_lease(response.lease)
        {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if Self::resource_trace_enabled() {
                let category = if pending.ticket != ticket {
                    "drop-ticket-mismatch"
                } else if pending.lease != response.lease {
                    "drop-lease-mismatch"
                } else {
                    "drop-resource-lease-not-current"
                };
                let current = self
                    .ivars()
                    .document
                    .borrow()
                    .is_current_lease(response.lease);
                Self::trace_resource(category, Some(ticket), Some(current), None, None, None);
            }
            return;
        }
        if !self
            .ivars()
            .ledger
            .borrow_mut()
            .begin_terminal_callback(ticket)
        {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if Self::resource_trace_enabled() {
                Self::trace_resource(
                    "drop-terminal-slot-denied",
                    Some(ticket),
                    Some(
                        self.ivars()
                            .document
                            .borrow()
                            .is_current_lease(response.lease),
                    ),
                    None,
                    None,
                    None,
                );
            }
            return;
        }
        let task = &pending.retained_task;
        match response.result {
            Err(_) => {
                if self.can_deliver(ticket, response.lease) {
                    Self::send_error(
                        task,
                        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                        self,
                    );
                }
            }
            Ok(bytes) => self.send_verified_response(&pending, bytes),
        }
    }

    #[allow(unsafe_code)] // Existing exact live task response/data/finish operations.
    fn send_verified_response(&self, pending: &PendingNativeTask, bytes: VerifiedBytes) {
        let ticket = pending.ticket;
        let task = &pending.retained_task;
        let actual_url = &pending.response_url;

        let headers = self.response_headers(&bytes);
        let Some(response) = NSHTTPURLResponse::initWithURL_statusCode_HTTPVersion_headerFields(
            NSHTTPURLResponse::alloc(),
            actual_url,
            200,
            Some(ns_string!("HTTP/1.1")),
            Some(&headers),
        ) else {
            if self.can_deliver(ticket, pending.lease) {
                Self::send_error(
                    task,
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    self,
                );
            }
            return;
        };
        if !self.can_deliver(ticket, pending.lease) {
            return;
        }
        // SAFETY: real, still-live task has exactly one valid terminal
        // lease and matching current receiving native view/document.
        // Catch native WK stop/reentrancy exceptions rather than retry.
        let sent_response = objc2::exception::catch(AssertUnwindSafe(|| unsafe {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            self.record_lifecycle_callback(task);
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            self.trace_native_attempt("native-response-attempt", ticket, pending.lease);
            task.didReceiveResponse(&response);
        }))
        .is_ok();
        if sent_response && self.can_deliver(ticket, pending.lease) {
            let data = NSData::from_vec(bytes.bytes);
            if objc2::exception::catch(AssertUnwindSafe(|| unsafe {
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                self.record_lifecycle_callback(task);
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                self.trace_native_attempt("native-data-attempt", ticket, pending.lease);
                task.didReceiveData(&data);
            }))
            .is_ok()
                && self.can_deliver(ticket, pending.lease)
            {
                let _ = objc2::exception::catch(AssertUnwindSafe(|| unsafe {
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    self.record_lifecycle_callback(task);
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    self.trace_native_attempt("native-finish-attempt", ticket, pending.lease);
                    task.didFinish();
                }));
            }
        }
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fn trace_worker_result(&self, response: &ResourceCompleted) {
        if Self::resource_trace_enabled() {
            use sha2::Digest as _;
            let ticket = response.ticket;
            let current = self
                .ivars()
                .document
                .borrow()
                .is_current_lease(response.lease);
            match &response.result {
                Ok(bytes) => Self::trace_resource(
                    "worker-result-ok",
                    Some(ticket),
                    Some(current),
                    Some(bytes.bytes.len() as u64),
                    Some(sha2::Sha256::digest(&bytes.bytes).into()),
                    Some(bytes.mime),
                ),
                Err(_) => Self::trace_resource(
                    "worker-result-error",
                    Some(ticket),
                    Some(current),
                    None,
                    None,
                    None,
                ),
            }
        }
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fn trace_native_attempt(&self, phase: &'static str, ticket: TaskTicket, lease: ResourceLease) {
        if Self::resource_trace_enabled() {
            Self::trace_resource(
                phase,
                Some(ticket),
                Some(self.ivars().document.borrow().is_current_lease(lease)),
                None,
                None,
                None,
            );
        }
    }

    fn response_headers(
        &self,
        bytes: &VerifiedBytes,
    ) -> Retained<NSMutableDictionary<NSString, NSString>> {
        let headers = NSMutableDictionary::<NSString, NSString>::new();
        headers.insert(ns_string!("Content-Type"), &NSString::from_str(bytes.mime));
        headers.insert(
            ns_string!("Content-Length"),
            &NSString::from_str(&bytes.bytes.len().to_string()),
        );
        headers.insert(ns_string!("Cache-Control"), ns_string!("no-store"));
        headers.insert(ns_string!("X-Content-Type-Options"), ns_string!("nosniff"));
        headers.insert(
            ns_string!("Content-Security-Policy"),
            &NSString::from_str(self.ivars().source.response_policy()),
        );
        headers
    }

    fn can_deliver(&self, ticket: TaskTicket, lease: ResourceLease) -> bool {
        self.ivars().expected_view.get().is_some()
            && self.ivars().document.borrow().is_current_lease(lease)
            && self
                .ivars()
                .ledger
                .borrow()
                .may_continue_native_delivery(ticket)
    }

    fn complete_without_delivery(&self, ticket: TaskTicket) {
        let mut ledger = self.ivars().ledger.borrow_mut();
        // Stale qualified metadata has no callback to claim its terminal slot.
        // This transition is incarnation-checked and invokes no native method.
        ledger.begin_terminal_callback(ticket);
        let _ = ledger.release(ticket);
    }

    fn reject_ticket(&self, task: &ProtocolObject<dyn WKURLSchemeTask>, ticket: TaskTicket) {
        if self
            .ivars()
            .ledger
            .borrow_mut()
            .begin_terminal_callback(ticket)
        {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if Self::resource_trace_enabled() {
                Self::trace_resource("terminal-error-claim", Some(ticket), None, None, None, None);
            }
            Self::send_error(
                task,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                self,
            );
        }
        let _ = self.ivars().ledger.borrow_mut().release(ticket);
    }

    #[allow(unsafe_code)] // Error construction + one live WKURLSchemeTask failure.
    fn send_error(
        task: &ProtocolObject<dyn WKURLSchemeTask>,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))] owner: &Self,
    ) {
        // SAFETY: fixed Cocoa error; caller has checked stop/ticket state or
        // WebKit provided an untracked new task that must fail closed.
        let error = unsafe {
            NSError::errorWithDomain_code_userInfo(
                ns_string!("keld-gh473-resource-denied"),
                403,
                None,
            )
        };
        // SAFETY: no header/data is emitted; exactly one attempted final
        // callback to the original native WK task on the AppKit UI owner.
        let _ = objc2::exception::catch(AssertUnwindSafe(|| unsafe {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            owner.record_lifecycle_callback(task);
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            if Self::resource_trace_enabled() {
                Self::trace_resource("native-fail-attempt", None, None, None, None, None);
            }
            task.didFailWithError(&error);
        }));
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fn resource_trace_enabled() -> bool {
        std::env::var_os("KELD_WV_RESOURCE_TRACE").as_deref() == Some(std::ffi::OsStr::new("1"))
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fn trace_resource(
        phase: &'static str,
        ticket: Option<TaskTicket>,
        current: Option<bool>,
        size: Option<u64>,
        digest: Option<[u8; 32]>,
        mime: Option<&str>,
    ) {
        eprintln!(
            "KELD resource observation: {}",
            serde_json::json!({"phase":phase,"ticket":ticket.map(TaskTicket::diagnostic_incarnation),"resource_lease_current":current,"bytes":size,"sha256":digest,"mime":mime})
        );
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn attach_lifecycle_census(
        &self,
        census: Rc<super::s1_resource_lifecycle::CallbackCensus>,
    ) {
        *self.ivars().lifecycle_census.borrow_mut() = Some(census);
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fn record_lifecycle_callback(&self, task: &ProtocolObject<dyn WKURLSchemeTask>) {
        if let Some(census) = self.ivars().lifecycle_census.borrow().as_ref() {
            census.callback(Self::key(task));
        }
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn lifecycle_held_task(&self) -> Option<usize> {
        self.ivars()
            .tasks
            .borrow()
            .iter()
            .find_map(|(key, pending)| {
                pending
                    .response_url
                    .absoluteString()
                    .is_some_and(|url| url.to_string().ends_with("/leak.svg?lifecycle=held"))
                    .then_some(*key)
            })
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn lifecycle_task_present(&self, key: usize) -> bool {
        self.ivars().tasks.borrow().contains_key(&key)
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn lifecycle_status(&self) -> (bool, bool, usize, usize) {
        (
            self.ivars().reader.borrow().is_shutdown_requested(),
            self.ivars()
                .reader
                .borrow()
                .cancelled
                .load(Ordering::Acquire),
            self.ivars().retained_bytes.load(Ordering::Acquire),
            self.ivars().tasks.borrow().len(),
        )
    }

    /// Tombstones all tasks and cancels the one source reader immediately.
    /// The reader may be stuck in one OS read. This call never waits.
    pub(super) fn request_shutdown(&self) {
        self.ivars()
            .reader
            .borrow_mut()
            .request_shutdown(ReaderWake::Tao {
                view: self.ivars().view,
                proxy: self.ivars().wake.clone(),
            });
        // The ledger also owns delivery tickets temporarily removed from the
        // native task map while callbacks execute.
        self.ivars().ledger.borrow_mut().stop_all();
    }

    pub(super) fn shutdown_requested(&self) -> bool {
        self.ivars().reader.borrow().is_shutdown_requested()
    }

    pub(super) fn shutdown_deadline(&self) -> Option<Instant> {
        self.ivars().reader.borrow().shutdown_deadline()
    }

    pub(super) fn observe_reader_termination(&self, event: &ReaderTerminated) -> bool {
        event.view == self.ivars().view
            && self.ivars().reader.borrow_mut().observe_termination(event)
    }

    /// The caller MUST keep the entire View/handler/selected-root FD alive
    /// while this returns false or Err. No native release event is sent then.
    pub(super) fn try_finish_shutdown(&self) -> Result<bool, WvError> {
        if !self.ivars().reader.borrow_mut().try_finish_shutdown()?
            || self.ivars().ledger.borrow().has_active_delivery()
        {
            return Ok(false);
        }
        let mut current = self.ivars().tasks.borrow_mut();
        for pending in current.values() {
            let _ = self
                .ivars()
                .ledger
                .borrow_mut()
                .worker_quiesced(pending.ticket);
            let _ = self.ivars().ledger.borrow_mut().release(pending.ticket);
        }
        current.clear();
        self.ivars().expected_view.set(None);
        Ok(true)
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
    use std::thread;

    fn request<T>(custody: &mut ReaderCustody<T>) -> Receiver<ReaderTerminated> {
        let (sender, receiver) = mpsc::channel();
        custody.request_shutdown(ReaderWake::Test {
            view: WebviewId(1),
            sender,
        });
        receiver
    }

    fn completed<T>(custody: &mut ReaderCustody<T>, wakes: &Receiver<ReaderTerminated>) {
        let event = wakes
            .recv_timeout(Duration::from_secs(3))
            .expect("actual post-join wake");
        custody.observe_termination(&event);
    }

    // The deterministic gate models a regular-file read that ignores
    // O_NONBLOCK while the AppKit owner receives Destroy or Quit. It verifies
    // the real custody primitive required before native release.
    #[test]
    fn blocked_resource_read_preserves_retained_credit_until_destroy_or_quit_quiesces() {
        for terminal in ["DestroyWindow", "Quit"] {
            let account = Arc::new(AtomicUsize::new(0));
            let credit = RetainedByteCredit::reserve(&account, 1024 * 1024)
                .expect("within the approved retained-byte cap");
            let (sender, receiver) = sync_channel::<RetainedByteCredit>(1);
            let (started_tx, started_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::sync_channel::<()>(0);
            let mut custody = ReaderCustody::new();
            let stop = Arc::clone(&custody.cancelled);
            let worker = thread::spawn(move || {
                while let Ok(held_credit) = receiver.recv() {
                    started_tx.send(()).expect("barrier admission witness");
                    // A kernel read can remain stuck despite cancel intent.
                    // No native callbacks or UI objects are held on this thread.
                    release_rx.recv().expect("release held read");
                    drop(held_credit);
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                }
            });
            custody.install(sender, worker);
            custody.submit(credit).expect("admitted one resource");
            started_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("reader entered held source I/O");

            let ui_start = Instant::now();
            let wakes = request(&mut custody);
            assert!(
                !custody.try_finish_shutdown().expect("pending read"),
                "{terminal}"
            );
            assert!(
                ui_start.elapsed() < Duration::from_secs(1),
                "{terminal}: UI owner blocked on source I/O"
            );
            assert_eq!(
                account.load(Ordering::Acquire),
                1024 * 1024,
                "{terminal}: bytes released before I/O quiescence"
            );

            release_tx.send(()).expect("release actual read");
            completed(&mut custody, &wakes);
            assert!(
                custody
                    .try_finish_shutdown()
                    .expect("proved I/O quiescence")
            );
            assert_eq!(
                account.load(Ordering::Acquire),
                0,
                "{terminal}: credit survives after physical quiescence"
            );
            assert!(
                custody.worker.is_none(),
                "{terminal}: worker still retained"
            );
        }
    }

    #[test]
    fn expired_reader_does_not_claim_a_native_release_or_discard_worker() {
        let (sender, receiver) = sync_channel::<()>(1);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = sync_channel::<()>(0);
        let worker = thread::spawn(move || {
            receiver.recv().expect("held read admitted");
            started_tx.send(()).expect("barrier entered");
            let _ = release_rx.recv();
        });
        let mut custody = ReaderCustody::new();
        custody.install(sender, worker);
        custody.submit(()).expect("work admitted");
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(3)), Ok(()));
        let wakes = request(&mut custody);
        custody.shutdown_at = Some(
            Instant::now()
                .checked_sub(RESOURCE_READER_SHUTDOWN_DEADLINE)
                .expect("elapsed shutdown test clock"),
        );
        assert!(custody.try_finish_shutdown().is_err());
        assert!(
            custody.completion.is_some(),
            "unproved I/O must retain its completion custody"
        );
        release_tx.send(()).expect("finish test reader");
        // Actual GCD work owns the unique handle even after a failed deadline.
        completed(&mut custody, &wakes);
        assert!(custody.try_finish_shutdown().is_err(), "failure is sticky");
        assert!(matches!(
            started_rx.recv_timeout(Duration::from_millis(1)),
            Err(RecvTimeoutError::Disconnected)
        ));
    }

    // A Rust thread can finish its main function before its thread-local
    // destructors finish. This gate holds actual thread termination, not a
    // simulated exit marker or a delay. The outer runner bounds the child.
    struct HeldReaderThreadLocal {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        credit: Option<RetainedByteCredit>,
    }

    impl Drop for HeldReaderThreadLocal {
        fn drop(&mut self) {
            let _ = self.entered.send(());
            let _ = self.release.recv();
            drop(self.credit.take());
        }
    }

    thread_local! {
        static HELD_READER_THREAD_LOCAL: RefCell<Option<HeldReaderThreadLocal>> =
            const { RefCell::new(None) };
    }

    #[test]
    fn reader_custody_finish_remains_nonblocking_until_actual_thread_termination() {
        let deadline = Duration::from_secs(3);
        let account = Arc::new(AtomicUsize::new(0));
        let credit =
            RetainedByteCredit::reserve(&account, 1024 * 1024).expect("one retained source buffer");
        let (submit, receiver) = sync_channel::<RetainedByteCredit>(1);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = sync_channel::<()>(0);
        let worker = thread::spawn(move || {
            let held = receiver.recv().expect("real worker received its buffer");
            HELD_READER_THREAD_LOCAL.with(|slot| {
                *slot.borrow_mut() = Some(HeldReaderThreadLocal {
                    entered: entered_tx,
                    release: release_rx,
                    credit: Some(held),
                });
            });
        });
        submit.try_send(credit).expect("bounded work admitted");
        entered_rx
            .recv_timeout(deadline)
            .expect("real worker entered its thread-local destructor");
        // Record the exact precondition before transferring the unique handle
        // into custody. No production-private field or is_finished loop is used.
        let main_function_finished = worker.is_finished();
        let mut custody = ReaderCustody::new();
        custody.install(submit, worker);
        let (first_tx, first_rx) = mpsc::channel();
        let (outcome_tx, outcome_rx) = mpsc::channel();
        let (continue_tx, continue_rx) = sync_channel::<()>(0);
        let (final_tx, final_rx) = mpsc::channel();
        let ui = thread::spawn(move || {
            let wakes = request(&mut custody);
            let first = custody
                .try_finish_shutdown()
                .map_err(|error| error.to_string());
            let pending_before_join = custody.completion.as_ref().is_some_and(|completion| {
                completion.0.load(Ordering::Acquire) == ReaderCompletion::PENDING
            });
            let _ = outcome_tx.send(pending_before_join);
            let _ = first_tx.send(first);
            let _ = continue_rx.recv();
            // The GCD work item publishes only after real TLS destruction.
            completed(&mut custody, &wakes);
            let cleaned = custody.try_finish_shutdown().unwrap_or(false);
            let _ = final_tx.send(cleaned);
        });

        // A timeout is a failed contract/kill switch. It never supplies a
        // completion turn or a successful release. Always open the gate before
        // asserting, so even the unfixed UI join can return for child cleanup.
        let first = first_rx.recv_timeout(deadline);
        let pending_before_join = outcome_rx.recv_timeout(deadline);
        let retained_while_tls_held = account.load(Ordering::Acquire);
        release_tx
            .send(())
            .expect("allow actual thread termination");
        continue_tx
            .send(())
            .expect("one explicit post-release probe");
        let final_result = final_rx.recv_timeout(deadline);
        ui.join()
            .expect("test UI thread cleaned up after gate release");

        assert!(
            main_function_finished,
            "TLS witness did not establish the intended is_finished precondition"
        );
        assert_eq!(retained_while_tls_held, 1024 * 1024);
        assert_eq!(
            pending_before_join,
            Ok(true),
            "joined outcome published while actual TLS retains bytes"
        );
        assert_eq!(
            first,
            Ok(Ok(false)),
            "ReaderCustody blocked its caller in join after is_finished; pending reader termination must return without waiting on TLS"
        );
        assert_eq!(final_result, Ok(true));
        assert_eq!(account.load(Ordering::Acquire), 0);
        let mut never_started = ReaderCustody::<()>::new();
        let never_started_wakes = request(&mut never_started);
        assert!(
            never_started
                .try_finish_shutdown()
                .expect("no worker to retain")
        );
        assert_eq!(
            never_started.shutdown_deadline(),
            None,
            "never-started reader cannot arm an expired native window wake"
        );
        assert!(matches!(
            never_started_wakes.try_recv(),
            Err(TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn repeated_shutdown_keeps_one_join_one_deadline_and_rejects_stale_wake() {
        let (sender, receiver) = sync_channel::<()>(1);
        let (release_tx, release_rx) = sync_channel::<()>(0);
        let worker = thread::spawn(move || {
            receiver.recv().expect("read admitted");
            release_rx.recv().expect("release actual worker");
        });
        let mut custody = ReaderCustody::new();
        custody.install(sender, worker);
        custody.submit(()).expect("work admitted");
        let wakes = request(&mut custody);
        let deadline = custody.shutdown_deadline();
        let cell = Arc::clone(custody.completion.as_ref().expect("join cell"));
        let duplicate = request(&mut custody);
        assert_eq!(custody.shutdown_deadline(), deadline);
        assert!(Arc::ptr_eq(
            custody.completion.as_ref().expect("same cell"),
            &cell
        ));
        assert!(matches!(
            duplicate.try_recv(),
            Err(TryRecvError::Disconnected)
        ));
        let unrelated = ReaderTerminated {
            view: WebviewId(1),
            completion: Arc::new(ReaderCompletion(AtomicU8::new(ReaderCompletion::JOINED))),
        };
        release_tx.send(()).expect("finish worker");
        let actual = wakes
            .recv_timeout(Duration::from_secs(3))
            .expect("actual joined wake");
        assert_eq!(
            actual.completion.0.load(Ordering::Acquire),
            ReaderCompletion::JOINED
        );
        assert!(!custody.observe_termination(&unrelated));
        assert!(
            !custody
                .try_finish_shutdown()
                .expect("unrelated cell has no authority")
        );
        assert!(custody.observe_termination(&actual));
        assert!(custody.try_finish_shutdown().expect("exact completion"));
        assert!(matches!(
            wakes.recv_timeout(Duration::from_secs(3)),
            Err(RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn timely_reader_wake_remains_proved_while_window_retirement_waits() {
        // A native delivery or renderer evaluation can keep the window alive
        // after its reader joined. That independent owner must not make an
        // already-proved reader become unproved at the original reader bound.
        // The two cases cover a prior successful release probe and the first
        // release probe after the other owner has delayed it.
        for first_probe_before_deadline in [true, false] {
            let (submit, receiver) = sync_channel::<()>(1);
            let worker = thread::spawn(move || {
                assert!(
                    receiver.recv().is_err(),
                    "shutdown must close the actual reader channel"
                );
            });
            let mut custody = ReaderCustody::new();
            custody.install(submit, worker);
            let wakes = request(&mut custody);
            assert!(custody.shutdown_deadline().is_some());
            completed(&mut custody, &wakes);
            if first_probe_before_deadline {
                assert!(
                    custody
                        .try_finish_shutdown()
                        .expect("timely exact join and wake"),
                    "actual joined reader has quiesced"
                );
            }
            // Advance only the test's fixed deadline after the real join and
            // its exact wake were observed. No synthetic completion or sleep.
            custody.shutdown_at = Some(
                Instant::now()
                    .checked_sub(RESOURCE_READER_SHUTDOWN_DEADLINE)
                    .expect("elapsed reader deadline"),
            );
            assert!(
                custody
                    .try_finish_shutdown()
                    .expect("timely reader proof survives unrelated owner delay"),
                "first_probe_before_deadline={first_probe_before_deadline}"
            );
            assert_eq!(
                custody.shutdown_deadline(),
                None,
                "a proved reader cannot supply an expired window wake deadline"
            );
        }
    }

    #[test]
    fn reader_panic_retains_unproved_custody_after_actual_join_wake() {
        let (sender, _receiver) = sync_channel::<()>(1);
        let worker = thread::spawn(|| panic!("intentional source-reader panic"));
        let mut custody = ReaderCustody::new();
        custody.install(sender, worker);
        let wakes = request(&mut custody);
        completed(&mut custody, &wakes);
        assert!(custody.try_finish_shutdown().is_err());
        assert!(custody.completion.is_some(), "retain failed owner");
        assert!(custody.try_finish_shutdown().is_err(), "failure is sticky");
    }

    #[test]
    fn suppressed_completion_wake_cannot_release_even_a_joined_reader() {
        let (sender, receiver) = sync_channel::<()>(1);
        let worker = thread::spawn(move || {
            let _ = receiver.recv();
        });
        let mut custody = ReaderCustody::new();
        custody.install(sender, worker);
        let wakes = request(&mut custody);
        let held_event = wakes
            .recv_timeout(Duration::from_secs(3))
            .expect("real post-join wake");
        assert_eq!(
            held_event.completion.0.load(Ordering::Acquire),
            ReaderCompletion::JOINED
        );
        assert!(
            !custody
                .try_finish_shutdown()
                .expect("unobserved nudge is pending")
        );
        custody.shutdown_at = Some(
            Instant::now()
                .checked_sub(RESOURCE_READER_SHUTDOWN_DEADLINE)
                .expect("elapsed shutdown test clock"),
        );
        assert!(
            custody.try_finish_shutdown().is_err(),
            "fixed failure deadline"
        );
        custody.observe_termination(&held_event);
        assert!(
            custody.try_finish_shutdown().is_err(),
            "late event cannot revive custody"
        );
    }
}
