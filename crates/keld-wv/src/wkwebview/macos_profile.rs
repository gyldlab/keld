//! macOS identified-store ownership and Keld metadata for KEL-135/T3.
//!
//! `WebKit` owns the physical website-data-store layout. Keld retains only the
//! identity/UUID binding, lifecycle and purge records under Foundation's
//! per-user Application Support directory.

#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use std::sync::Arc;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use nix::fcntl::OFlag;
use nix::unistd::Uid;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject};
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use objc2_app_kit::NSWindowWillBeginSheetNotification;
use objc2_core_foundation::{CFRunLoop, kCFRunLoopDefaultMode};
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol, NSString};
use objc2_foundation::{
    NSProcessInfo, NSSearchPathDirectory, NSSearchPathDomainMask,
    NSSearchPathForDirectoriesInDomains, NSUUID,
};
use objc2_web_kit::{WKWebViewConfiguration, WKWebsiteDataStore};
use tao::event_loop::EventLoop;
use wry::WebViewExtDarwin;

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use crate::profile::PurgePhase;
use crate::profile::{
    AppleStoreUuid, BindingPhase, BootIdentity, MarkerAction, MarkerObservation, ProfileError,
    ProfileErrorKind, ProfileIdentity, ProfileLifecycleAction, ProfileLifecyclePhase,
    ProfileLifecycleRecord, ProfileLockLevel, ProfileLockOrder, ProfileMarker, ProfilePlatform,
    ProfileProcessIdentity, ProfilePurgePhase, ProfilePurgeRecord, ProfileRootRole,
    RecordedProcessObservation, RegistryAction, RegistryIntent, RegistryIntentPhase,
    RegistryRecord, RegistryRequest, RegistrySnapshot, StoreBinding, StoreObservation,
    WebProfileSelection, next_lifecycle_action, next_marker_action, next_registry_action,
};

use super::WkUserEvent;
use crate::WvError;

const METADATA_MAX_BYTES: usize = 16 * 1024;
const METADATA_DIRECTORY_MODE: u32 = 0o700;
const METADATA_FILE_MODE: u32 = 0o600;
const STORE_ENUMERATION_DEADLINE: Duration = Duration::from_secs(15);
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

// macOS 26.5 SDK sys/acl.h: ACL_TYPE_EXTENDED, ACL_FIRST/NEXT_ENTRY,
// ACL_EXTENDED_ALLOW/DENY. Retrieval returns an independent ACL that acl_free
// releases: https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/acl_get.3.html
const ACL_TYPE_EXTENDED: i32 = 0x0000_0100;
const ACL_FIRST_ENTRY: i32 = 0;
const ACL_NEXT_ENTRY: i32 = -1;
const ACL_LAST_ENTRY: i32 = -2;
const ACL_EXTENDED_ALLOW: i32 = 1;
const ACL_EXTENDED_DENY: i32 = 2;

#[allow(unsafe_code)] // exact sanctioned read-only metadata/ancestor ACL C API
unsafe extern "C" {
    fn acl_get_fd_np(fd: i32, acl_type: i32) -> *mut c_void;
    fn acl_get_entry(acl: *mut c_void, entry_id: i32, entry: *mut *mut c_void) -> i32;
    fn acl_get_tag_type(entry: *mut c_void, tag: *mut i32) -> i32;
    fn acl_free(acl: *mut c_void) -> i32;
}

// SAFETY: These are public AVFoundation NSString constants. The framework is
// linked only for the debug macOS acceptance hook, and each read occurs on the
// signed host's AppKit thread while AVFoundation remains loaded.
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[allow(unsafe_code)]
#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {
    static AVMediaTypeVideo: &'static NSString;
    static AVMediaTypeAudio: &'static NSString;
}

/// Counts site sheets in the requesting signed process and reads its TCC state.
///
/// The observer is process-wide because it is installed before the fixture
/// creates its only window. Any unrelated sheet is a conservative false
/// positive; there is no unobserved interval before navigation.
/// Apple's public `NSWindowWillBeginSheetNotification` reports sheet starts;
/// `WebKit`'s public-Prompt path presents an `NSAlert` with `beginSheetModalForWindow`.
/// See <https://developer.apple.com/documentation/appkit/nswindowwillbeginsheetnotification>
/// and <https://github.com/WebKit/WebKit/blob/f10733ef7744ff20f5cb4bbe94d430bf100bb62d/Source/WebKit/UIProcess/Cocoa/MediaPermissionUtilities.mm#L204-L217>.
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
pub(super) struct MacMediaPromptProbe {
    _main_thread: MainThreadMarker,
    center: Retained<NSNotificationCenter>,
    observer: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    _block: block2::RcBlock<dyn Fn(std::ptr::NonNull<NSNotification>)>,
    sheet_count: Arc<AtomicUsize>,
    camera_tcc: isize,
    microphone_tcc: isize,
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
impl MacMediaPromptProbe {
    /// Registers before the first fixture navigation, on the `AppKit` thread.
    ///
    /// # Errors
    ///
    /// Returns an error if this is not the main thread or `AVFoundation` is absent.
    /// The read-only authorization API is documented at
    /// <https://developer.apple.com/documentation/avfoundation/avcapturedevice/authorizationstatus(for:)>.
    #[allow(unsafe_code)] // exact public AVFoundation status and AppKit notification calls
    pub(super) fn new() -> Result<Self, WvError> {
        let main_thread = MainThreadMarker::new().ok_or_else(|| {
            WvError::Webview(String::from("media probe requires the AppKit main thread"))
        })?;
        let capture_device = AnyClass::get(c"AVCaptureDevice").ok_or_else(|| {
            WvError::Webview(String::from("AVCaptureDevice class is unavailable"))
        })?;
        // SAFETY: AVMediaTypeVideo/Audio and authorizationStatusForMediaType:
        // are public AVFoundation API. The two constants are valid NSStrings,
        // and the receiver is the live AVCaptureDevice class in this host.
        let (camera_tcc, microphone_tcc) = unsafe {
            (
                objc2::msg_send![capture_device, authorizationStatusForMediaType: AVMediaTypeVideo],
                objc2::msg_send![capture_device, authorizationStatusForMediaType: AVMediaTypeAudio],
            )
        };
        let center = NSNotificationCenter::defaultCenter();
        let sheet_count = Arc::new(AtomicUsize::new(0));
        let count_for_block = Arc::clone(&sheet_count);
        let block =
            block2::RcBlock::new(move |_notification: std::ptr::NonNull<NSNotification>| {
                count_for_block.fetch_add(1, Ordering::Relaxed);
            });
        // SAFETY: The public notification is emitted for NSWindow sheet
        // presentation. The owned block captures only a Send + Sync atomic;
        // the retained observer token and block outlive registration, and the
        // token is removed on the same AppKit thread before either is dropped.
        let observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSWindowWillBeginSheetNotification),
                None,
                None,
                &block,
            )
        };
        Ok(Self {
            _main_thread: main_thread,
            center,
            observer: Some(observer),
            _block: block,
            sheet_count,
            camera_tcc,
            microphone_tcc,
        })
    }

    pub(super) fn finish(mut self) {
        self.unregister();
        eprintln!(
            "KELD_KEL135_MEDIA_PROBE camera_tcc={} microphone_tcc={} sheet_count={} scope=process-wide",
            self.camera_tcc,
            self.microphone_tcc,
            self.sheet_count.load(Ordering::Acquire),
        );
    }

    #[allow(unsafe_code)] // exact public NSNotificationCenter removal on the AppKit thread
    fn unregister(&mut self) {
        if let Some(observer) = self.observer.take() {
            let object: &AnyObject = AsRef::<AnyObject>::as_ref(&*observer);
            // SAFETY: This is the retained token returned by this center's
            // registration, removed on the same main thread before the owned
            // callback block and its captured state are released.
            unsafe { self.center.removeObserver(object) };
        }
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
impl Drop for MacMediaPromptProbe {
    fn drop(&mut self) {
        self.unregister();
    }
}

/// One engine-owned store. Every per-view configuration receives this same
/// retained object, including the nonpersistent dev store.
pub(super) struct MacProfileOwner {
    store: Option<Retained<WKWebsiteDataStore>>,
    persistent: Option<PersistentProfile>,
}

struct PersistentProfile {
    root: MetadataRoot,
    identity: ProfileIdentity,
    binding: StoreBinding,
    lifecycle: ProfileLifecycleRecord,
    _package_lock: File,
    _profile_lock: File,
    lock_order: ProfileLockOrder,
}

struct MetadataRoot {
    path: PathBuf,
    uid: u32,
}

impl MacProfileOwner {
    pub(super) fn mode_name(&self) -> &'static str {
        if self.persistent.is_some() {
            "persistent"
        } else {
            "ephemeral-dev"
        }
    }

    /// Opens the single store selected by the authenticated host profile mode.
    pub(super) fn new(
        selection: WebProfileSelection,
        event_loop: &mut EventLoop<WkUserEvent>,
    ) -> Result<Self, WvError> {
        let main_thread = MainThreadMarker::new().ok_or_else(|| {
            WvError::EventLoop(String::from(
                "macOS profile stores must be selected on the AppKit main thread",
            ))
        })?;
        match selection {
            WebProfileSelection::Persistent(identity) => {
                let major = NSProcessInfo::processInfo()
                    .operatingSystemVersion()
                    .majorVersion;
                if major < 14 {
                    return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
                }
                // WebKit's store registry must be initialized before its
                // identifier enumeration callback is requested. This
                // short-lived memory-only store creates no persistent state.
                let _bootstrap = bootstrap_webkit_registry(main_thread)?;
                let (persistent, store) =
                    PersistentProfile::open(identity, main_thread, event_loop)?;
                let owner = Self {
                    store: Some(store),
                    persistent: Some(persistent),
                };
                report_store_selection("persistent", Some(identity), owner.store.as_ref());
                Ok(owner)
            }
            WebProfileSelection::EphemeralDev(_launch) => {
                // SAFETY: `main_thread` proves this WebKit call runs on the
                // required AppKit thread. The engine retains this one object
                // and shares it with every view configuration it creates.
                let store = bootstrap_webkit_registry(main_thread)?;
                let owner = Self {
                    store: Some(store),
                    persistent: None,
                };
                report_store_selection("ephemeral-dev", None, owner.store.as_ref());
                Ok(owner)
            }
        }
    }

    /// Creates a per-view configuration bound to the engine's selected store.
    #[allow(unsafe_code)] // exact main-thread WKWebsiteDataStore configuration calls
    pub(super) fn configuration_for_view(
        &self,
    ) -> Result<Retained<WKWebViewConfiguration>, WvError> {
        let main_thread = MainThreadMarker::new().ok_or_else(|| {
            WvError::EventLoop(String::from(
                "macOS profile configuration must be created on the AppKit main thread",
            ))
        })?;
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| profile_error(ProfileErrorKind::LifecycleUnproven))?;
        // SAFETY: the configuration is created and used on the AppKit main
        // thread. The setter retains the provided live WKWebsiteDataStore.
        let configuration = unsafe { WKWebViewConfiguration::new(main_thread) };
        // SAFETY: `configuration` and `store` are valid WebKit objects on the
        // main thread, and the API requires the store before WKWebView creation.
        unsafe { configuration.setWebsiteDataStore(store) };
        // SAFETY: reading the just-assigned property is valid on the main thread.
        let observed = unsafe { configuration.websiteDataStore() };
        if !std::ptr::eq(std::ptr::addr_of!(*observed), std::ptr::addr_of!(**store)) {
            return Err(profile_error(ProfileErrorKind::RegistryCorruption));
        }
        if let Some(persistent) = &self.persistent {
            verify_store(&observed, Some(persistent.binding.store_uuid()), true)?;
        } else {
            verify_store(&observed, None, false)?;
        }
        Ok(configuration)
    }

    /// Verifies the actual `WKWebView` configuration returned by Wry.
    #[allow(unsafe_code)] // exact main-thread WKWebView configuration read-back
    pub(super) fn verify_webview(&self, webview: &wry::WryWebView) -> Result<(), WvError> {
        // SAFETY: the handle is a live WKWebView returned by Wry and is read on
        // the same AppKit main thread that created it.
        let configuration = unsafe { webview.configuration() };
        // SAFETY: the configuration belongs to the live WebKit view.
        let observed = unsafe { configuration.websiteDataStore() };
        let selected = self
            .store
            .as_ref()
            .ok_or_else(|| profile_error(ProfileErrorKind::LifecycleUnproven))?;
        if !std::ptr::eq(
            std::ptr::addr_of!(*observed),
            std::ptr::addr_of!(**selected),
        ) {
            return Err(profile_error(ProfileErrorKind::RegistryCorruption));
        }
        if let Some(persistent) = &self.persistent {
            verify_store(&observed, Some(persistent.binding.store_uuid()), true)?;
        } else {
            verify_store(&observed, None, false)?;
        }
        if let Some(persistent) = &self.persistent {
            persistent.mark_running()?;
        }
        Ok(())
    }

    /// Commits the clean release chain after all Wry views have been dropped.
    pub(super) fn clean_shutdown(&mut self) -> Result<(), WvError> {
        let Some(persistent) = self.persistent.as_mut() else {
            self.store = None;
            return Ok(());
        };
        if self.store.is_none() && persistent.lifecycle.phase() == ProfileLifecyclePhase::Idle {
            return Ok(());
        }
        persistent
            .lock_order
            .acquire(ProfileLockLevel::EngineStoreIntent)
            .map_err(WvError::from)?;
        persistent.begin_stopping()?;
        // All Wry Views are destroyed by the event-loop exit path; release the
        // engine's final direct store reference before committing idle. Purge
        // uses WebKit's async removal callback plus enumeration as its stronger
        // destructive barrier.
        self.store = None;
        persistent.finish_idle_record()?;
        persistent
            .lock_order
            .release(ProfileLockLevel::EngineStoreIntent)
            .map_err(WvError::from)?;
        // Keep package/profile file locks in `PersistentProfile` until the
        // engine owner drops after Core finishes child and guardian teardown.
        Ok(())
    }

    /// Starts exact-identity purge in a fresh host process and waits for `WebKit`.
    pub(super) fn purge_persistent_profile(
        identity: ProfileIdentity,
        event_loop: &mut EventLoop<WkUserEvent>,
    ) -> Result<(), WvError> {
        if NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion
            < 14
        {
            return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
        }
        let main_thread = MainThreadMarker::new().ok_or_else(|| {
            WvError::EventLoop(String::from(
                "macOS profile purge must run on the AppKit main thread",
            ))
        })?;
        let _bootstrap = bootstrap_webkit_registry(main_thread)?;
        PersistentProfile::purge(identity, event_loop, main_thread)
    }

    /// Reads the current app's `WebKit` store registry without creating a persistent store.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn persistent_store_identifier_present_for_test(
        identifier: [u8; 16],
        event_loop: &mut EventLoop<WkUserEvent>,
    ) -> Result<bool, WvError> {
        if NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion
            < 14
        {
            return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
        }
        let main_thread = MainThreadMarker::new().ok_or_else(|| {
            WvError::EventLoop(String::from(
                "macOS profile store enumeration must run on the AppKit main thread",
            ))
        })?;
        let _bootstrap = bootstrap_webkit_registry(main_thread)?;
        Ok(fetch_data_store_identifiers(event_loop)?
            .iter()
            .any(|observed| observed == &identifier))
    }
}

impl PersistentProfile {
    fn open(
        identity: ProfileIdentity,
        main_thread: MainThreadMarker,
        event_loop: &mut EventLoop<WkUserEvent>,
    ) -> Result<(Self, Retained<WKWebsiteDataStore>), WvError> {
        let root = MetadataRoot::open()?;
        root.ensure_layout()?;
        let binding = StoreBinding::for_identity(identity);
        let mut lock_order = ProfileLockOrder::default();
        lock_order
            .acquire(ProfileLockLevel::PackageLifecycle)
            .map_err(WvError::from)?;
        let package_lock = root.open_identity_lock(identity, "package.lock")?;
        lock_order
            .acquire(ProfileLockLevel::PlatformRegistry)
            .map_err(WvError::from)?;
        let registry_lock = root.open_lock(&root.path.join("registry.lock"))?;
        root.reject_pending_purge(identity)?;
        root.prepare_binding_metadata(binding)?;
        drop(registry_lock);
        lock_order
            .release(ProfileLockLevel::PlatformRegistry)
            .map_err(WvError::from)?;

        let profile_lock = root.open_identity_lock(identity, "profile.lock")?;
        lock_order
            .acquire(ProfileLockLevel::ProfileLease)
            .map_err(WvError::from)?;
        let boot = current_boot_identity()?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if std::env::var_os("KELD_PROFILE_ACCEPTANCE_REPORT").is_some() {
            let mut boot_hex = String::with_capacity(32);
            for &byte in boot.as_bytes() {
                boot_hex.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
                boot_hex.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
            }
            eprintln!("KELD_KEL135_LIFECYCLE boot_uuid_hex={boot_hex}");
        }
        let process = current_process_identity()?;
        let mut lifecycle = root.begin_lifecycle(identity, boot, process)?;
        lock_order
            .acquire(ProfileLockLevel::EngineStoreIntent)
            .map_err(WvError::from)?;
        let store = root.bind_store(binding, main_thread, event_loop)?;
        lifecycle = lifecycle
            .advance(ProfileLifecyclePhase::Running)
            .map_err(WvError::from)?;
        root.write_lifecycle(identity, lifecycle)?;
        lock_order
            .release(ProfileLockLevel::EngineStoreIntent)
            .map_err(WvError::from)?;

        Ok((
            Self {
                root,
                identity,
                binding,
                lifecycle,
                _package_lock: package_lock,
                _profile_lock: profile_lock,
                lock_order,
            },
            store,
        ))
    }

    fn mark_running(&self) -> Result<(), WvError> {
        if self.lifecycle.phase() != ProfileLifecyclePhase::Running {
            return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(())
    }

    fn begin_stopping(&mut self) -> Result<(), WvError> {
        if self.lifecycle.phase() == ProfileLifecyclePhase::Running {
            self.lifecycle = self
                .lifecycle
                .advance(ProfileLifecyclePhase::Stopping)
                .map_err(WvError::from)?;
            self.root.write_lifecycle(self.identity, self.lifecycle)?;
        }
        Ok(())
    }

    fn finish_idle_record(&mut self) -> Result<(), WvError> {
        if self.lifecycle.phase() == ProfileLifecyclePhase::Stopping {
            self.lifecycle = self
                .lifecycle
                .advance(ProfileLifecyclePhase::Idle)
                .map_err(WvError::from)?;
            self.root.write_lifecycle(self.identity, self.lifecycle)?;
        }
        Ok(())
    }

    fn purge(
        identity: ProfileIdentity,
        event_loop: &mut EventLoop<WkUserEvent>,
        main_thread: MainThreadMarker,
    ) -> Result<(), WvError> {
        // Purge never constructs or opens a normal WKWebView. The package and
        // profile locks exclude a live Keld owner; if a crashed WebKit network
        // process still holds the store, the exact removal callback below fails
        // and the durable purge intent remains resumable while quarantine blocks
        // ordinary startup.
        let root = MetadataRoot::open()?;
        root.ensure_layout()?;
        let binding = StoreBinding::for_identity(identity);
        let mut lock_order = ProfileLockOrder::default();
        lock_order
            .acquire(ProfileLockLevel::PackageLifecycle)
            .map_err(WvError::from)?;
        let _package_lock = root.open_identity_lock(identity, "package.lock")?;
        lock_order
            .acquire(ProfileLockLevel::PlatformRegistry)
            .map_err(WvError::from)?;
        let _registry_lock = root.open_lock(&root.path.join("registry.lock"))?;
        root.validate_purge_authority(binding)?;
        let _profile_lock = root.open_identity_lock(identity, "profile.lock")?;
        lock_order
            .acquire(ProfileLockLevel::ProfileLease)
            .map_err(WvError::from)?;
        lock_order
            .acquire(ProfileLockLevel::EngineStoreIntent)
            .map_err(WvError::from)?;
        root.resume_purge(binding, event_loop, main_thread)?;
        lock_order
            .release(ProfileLockLevel::EngineStoreIntent)
            .map_err(WvError::from)?;
        lock_order
            .release(ProfileLockLevel::ProfileLease)
            .map_err(WvError::from)?;
        lock_order
            .release(ProfileLockLevel::PlatformRegistry)
            .map_err(WvError::from)?;
        lock_order
            .release(ProfileLockLevel::PackageLifecycle)
            .map_err(WvError::from)?;
        Ok(())
    }
}

impl MetadataRoot {
    fn open() -> Result<Self, WvError> {
        let uid = Uid::effective().as_raw();
        let support = application_support_path()?;
        validate_parent_chain(&support, uid)?;
        let root = support.join("Keld").join("profiles").join("v1");
        for path in [
            support.join("Keld"),
            support.join("Keld/profiles"),
            root.clone(),
        ] {
            ensure_private_directory(&path, uid)?;
        }
        for child in ["identities", "store-uuids", "intents", "locks"] {
            ensure_private_directory(&root.join(child), uid)?;
        }
        Ok(Self { path: root, uid })
    }

    fn ensure_layout(&self) -> Result<(), WvError> {
        validate_private_directory(&self.path, self.uid)?;
        for child in ["identities", "store-uuids", "intents", "locks"] {
            validate_private_directory(&self.path.join(child), self.uid)?;
        }
        Ok(())
    }

    fn identity_dir(&self, identity: ProfileIdentity) -> PathBuf {
        self.path
            .join("identities")
            .join(identity.namespace_segment())
    }

    fn uuid_dir(&self, uuid: AppleStoreUuid) -> PathBuf {
        self.path.join("store-uuids").join(uuid.to_string())
    }

    fn binding_intent_path(&self, identity: ProfileIdentity) -> PathBuf {
        self.path
            .join("intents")
            .join(format!("{}.binding.v1", identity.namespace_segment()))
    }

    fn purge_intent_path(&self, identity: ProfileIdentity) -> PathBuf {
        self.path
            .join("intents")
            .join(format!("{}.purge.v1", identity.namespace_segment()))
    }

    fn open_lock(&self, path: &Path) -> Result<File, WvError> {
        let file = open_metadata_file(path, true, true, self.uid)?;
        match file.try_lock() {
            Ok(()) => Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => {
                Err(profile_error(ProfileErrorKind::ProfileInUse))
            }
            Err(std::fs::TryLockError::Error(_)) => {
                Err(profile_error(ProfileErrorKind::LifecycleUnproven))
            }
        }
    }

    fn open_identity_lock(&self, identity: ProfileIdentity, name: &str) -> Result<File, WvError> {
        let path =
            self.path
                .join("locks")
                .join(format!("{}.{}", identity.namespace_segment(), name));
        self.open_lock(&path)
    }

    fn prepare_binding_metadata(&self, binding: StoreBinding) -> Result<(), WvError> {
        let identity = binding.identity();
        let mut store = StoreObservation::Unknown;
        loop {
            let snapshot = self.registry_snapshot(binding, store)?;
            let action = next_registry_action(RegistryRequest::Bind(binding), snapshot)
                .map_err(WvError::from)?;
            match action {
                RegistryAction::WriteBindingIntent(intent) => {
                    write_record(
                        &self.binding_intent_path(identity),
                        &intent.to_record_bytes().map_err(WvError::from)?,
                        true,
                        self.uid,
                    )?;
                }
                RegistryAction::WriteReverseRecord(binding) => {
                    self.write_binding_direction(binding, true)?;
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    crash_after_test_binding_reverse_record();
                }
                RegistryAction::WriteForwardRecord(binding) => {
                    self.write_binding_direction(binding, false)?;
                }
                RegistryAction::AdvanceBindingIntent(phase) => {
                    let path = self.binding_intent_path(identity);
                    let current = self
                        .read_intent(&path)?
                        .ok_or_else(|| profile_error(ProfileErrorKind::RegistryCorruption))?;
                    let next = current.with_binding_phase(phase);
                    write_record(
                        &path,
                        &next.to_record_bytes().map_err(WvError::from)?,
                        false,
                        self.uid,
                    )?;
                }
                RegistryAction::EnumerateStores => return Ok(()),
                RegistryAction::ReuseStore
                | RegistryAction::ConstructStore(_)
                | RegistryAction::MarkBindingActive
                | RegistryAction::ClearIntent
                | RegistryAction::WritePurgeIntent(_)
                | RegistryAction::AdvancePurgeIntent(_)
                | RegistryAction::RemoveStore(_)
                | RegistryAction::RemoveReverseRecord
                | RegistryAction::RemoveForwardRecord
                | RegistryAction::ClearActiveStatus => {
                    return Err(profile_error(ProfileErrorKind::RegistryCorruption));
                }
            }
            store = StoreObservation::Unknown;
        }
    }

    fn write_binding_direction(&self, binding: StoreBinding, reverse: bool) -> Result<(), WvError> {
        let directory = if reverse {
            self.uuid_dir(binding.store_uuid())
        } else {
            self.identity_dir(binding.identity())
        };
        self.ensure_profile_directory(&directory, binding.identity(), true)?;
        let filename = if reverse { "reverse.v1" } else { "forward.v1" };
        write_record(
            &directory.join(filename),
            &binding.to_record_bytes().map_err(WvError::from)?,
            true,
            self.uid,
        )
    }

    fn ensure_profile_directory(
        &self,
        directory: &Path,
        identity: ProfileIdentity,
        binding_intent_authorized: bool,
    ) -> Result<(), WvError> {
        let created = create_private_directory_if_missing(directory, self.uid)?;
        let marker =
            ProfileMarker::new(identity, ProfilePlatform::Macos, ProfileRootRole::Metadata);
        let marker_path = directory.join("profile.owner.v1");
        if let Some(bytes) = read_record(&marker_path, self.uid)? {
            let observed = ProfileMarker::from_record_bytes(&bytes).map_err(WvError::from)?;
            next_marker_action(marker, MarkerObservation::ExistingMarked(observed))
                .map_err(WvError::from)?;
        } else {
            cleanup_temporary_files(directory, self.uid)?;
            let empty = fs::read_dir(directory)
                .map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?
                .next()
                .is_none();
            let observation = if created || (binding_intent_authorized && empty) {
                MarkerObservation::NewlyCreatedEmpty
            } else if empty {
                MarkerObservation::ExistingEmptyUnmarked
            } else {
                MarkerObservation::ExistingNonemptyUnmarked
            };
            let action = next_marker_action(marker, observation).map_err(WvError::from)?;
            let MarkerAction::WriteNew(expected) = action else {
                return Err(profile_error(ProfileErrorKind::MarkerMismatch));
            };
            write_record(
                &marker_path,
                &expected.to_record_bytes().map_err(WvError::from)?,
                true,
                self.uid,
            )?;
        }
        Ok(())
    }

    fn registry_snapshot(
        &self,
        binding: StoreBinding,
        store: StoreObservation,
    ) -> Result<RegistrySnapshot, WvError> {
        let identity = binding.identity();
        let intent = self.read_intent(&self.binding_intent_path(identity))?;
        let binding_authorized = intent.is_some_and(|record| {
            record.store_binding() == binding
                && matches!(record.phase(), RegistryIntentPhase::Binding(_))
        });
        let identity_dir = self.identity_dir(identity);
        let uuid_dir = self.uuid_dir(binding.store_uuid());
        if path_exists(&identity_dir)? {
            self.ensure_profile_directory(&identity_dir, identity, binding_authorized)?;
        }
        if path_exists(&uuid_dir)? {
            self.ensure_profile_directory(&uuid_dir, identity, binding_authorized)?;
        }
        let forward = self.read_binding(&identity_dir.join("forward.v1"))?;
        let reverse = self.read_binding(&uuid_dir.join("reverse.v1"))?;
        let active_path = identity_dir.join("active.v1");
        let active_binding = self.read_binding(&active_path)?;
        let active = match active_binding {
            RegistryRecord::Missing => false,
            RegistryRecord::Present(actual) if actual == binding => true,
            RegistryRecord::Present(_) => {
                return Err(profile_error(ProfileErrorKind::RegistryCorruption));
            }
        };
        Ok(RegistrySnapshot::new(
            intent, forward, reverse, store, active,
        ))
    }

    fn read_intent(&self, path: &Path) -> Result<Option<RegistryIntent>, WvError> {
        read_record(path, self.uid)?
            .map(|bytes| RegistryIntent::from_record_bytes(&bytes).map_err(WvError::from))
            .transpose()
    }

    fn read_binding(&self, path: &Path) -> Result<RegistryRecord, WvError> {
        let Some(bytes) = read_record(path, self.uid)? else {
            return Ok(RegistryRecord::Missing);
        };
        StoreBinding::from_record_bytes(&bytes)
            .map(RegistryRecord::Present)
            .map_err(WvError::from)
    }

    fn bind_store(
        &self,
        binding: StoreBinding,
        main_thread: MainThreadMarker,
        event_loop: &mut EventLoop<WkUserEvent>,
    ) -> Result<Retained<WKWebsiteDataStore>, WvError> {
        let mut observation = StoreObservation::Unknown;
        let mut selected_store = None;
        loop {
            let snapshot = self.registry_snapshot(binding, observation)?;
            let action = next_registry_action(RegistryRequest::Bind(binding), snapshot)
                .map_err(WvError::from)?;
            match action {
                RegistryAction::EnumerateStores => {
                    let identifiers = fetch_data_store_identifiers(event_loop)?;
                    if let Some(store) =
                        self.recovered_store_if_authorized(binding, main_thread, &identifiers)?
                    {
                        selected_store = Some(store);
                        observation = StoreObservation::Present {
                            uuid: binding.store_uuid(),
                            persistent: true,
                        };
                    } else if identifiers.contains(binding.store_uuid().as_bytes()) {
                        observation = StoreObservation::Present {
                            uuid: binding.store_uuid(),
                            persistent: true,
                        };
                    } else {
                        observation = StoreObservation::Absent;
                    }
                }
                RegistryAction::ConstructStore(binding) => {
                    let store = construct_identified_store(binding.store_uuid(), main_thread)?;
                    selected_store = Some(store);
                    observation = StoreObservation::Present {
                        uuid: binding.store_uuid(),
                        persistent: true,
                    };
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    crash_after_test_binding_phase("store-created");
                }
                RegistryAction::ReuseStore => {
                    let store = construct_identified_store(binding.store_uuid(), main_thread)?;
                    return Ok(store);
                }
                RegistryAction::WriteBindingIntent(intent) => {
                    write_record(
                        &self.binding_intent_path(binding.identity()),
                        &intent.to_record_bytes().map_err(WvError::from)?,
                        true,
                        self.uid,
                    )?;
                }
                RegistryAction::AdvanceBindingIntent(phase) => {
                    let path = self.binding_intent_path(binding.identity());
                    let current = self
                        .read_intent(&path)?
                        .ok_or_else(|| profile_error(ProfileErrorKind::RegistryCorruption))?;
                    let next = current.with_binding_phase(phase);
                    write_record(
                        &path,
                        &next.to_record_bytes().map_err(WvError::from)?,
                        false,
                        self.uid,
                    )?;
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    if phase == BindingPhase::StoreVerified {
                        crash_after_test_binding_phase("store-verified");
                    }
                }
                RegistryAction::MarkBindingActive => {
                    write_record(
                        &self.identity_dir(binding.identity()).join("active.v1"),
                        &binding.to_record_bytes().map_err(WvError::from)?,
                        true,
                        self.uid,
                    )?;
                }
                RegistryAction::ClearIntent => {
                    let store = selected_store
                        .take()
                        .ok_or_else(|| profile_error(ProfileErrorKind::RegistryCorruption))?;
                    remove_file_if_exists(&self.binding_intent_path(binding.identity()), self.uid)?;
                    return Ok(store);
                }
                RegistryAction::WriteReverseRecord(binding) => {
                    self.write_binding_direction(binding, true)?;
                }
                RegistryAction::WriteForwardRecord(binding) => {
                    self.write_binding_direction(binding, false)?;
                }
                RegistryAction::WritePurgeIntent(_)
                | RegistryAction::AdvancePurgeIntent(_)
                | RegistryAction::RemoveStore(_)
                | RegistryAction::RemoveReverseRecord
                | RegistryAction::RemoveForwardRecord
                | RegistryAction::ClearActiveStatus => {
                    return Err(profile_error(ProfileErrorKind::ActiveIntent));
                }
            }
        }
    }

    fn begin_lifecycle(
        &self,
        identity: ProfileIdentity,
        current_boot: BootIdentity,
        process: ProfileProcessIdentity,
    ) -> Result<ProfileLifecycleRecord, WvError> {
        let path = self.identity_dir(identity).join("lifecycle.v1");
        let mut lifecycle = match read_record(&path, self.uid)? {
            Some(bytes) => ProfileLifecycleRecord::from_boot_scoped_record_bytes(&bytes)
                .map_err(WvError::from)?,
            None => ProfileLifecycleRecord::idle(current_boot),
        };
        loop {
            match next_lifecycle_action(lifecycle, RecordedProcessObservation::Dead, current_boot)
                .map_err(WvError::from)?
            {
                ProfileLifecycleAction::BeginStartup => {
                    lifecycle = lifecycle.begin_startup(process).map_err(WvError::from)?;
                    self.write_lifecycle(identity, lifecycle)?;
                    return Ok(lifecycle);
                }
                ProfileLifecycleAction::WriteQuarantined => {
                    lifecycle = lifecycle.quarantine().map_err(WvError::from)?;
                    self.write_lifecycle(identity, lifecycle)?;
                }
                ProfileLifecycleAction::RestoreIdleAfterBoot => {
                    lifecycle = lifecycle
                        .restore_idle_after_boot(current_boot)
                        .map_err(WvError::from)?;
                    self.write_lifecycle(identity, lifecycle)?;
                }
                ProfileLifecycleAction::RunWindowsExclusiveUdfRecovery => {
                    return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
                }
            }
        }
    }

    fn recovered_store_if_authorized(
        &self,
        binding: StoreBinding,
        main_thread: MainThreadMarker,
        identifiers: &[[u8; 16]],
    ) -> Result<Option<Retained<WKWebsiteDataStore>>, WvError> {
        if !identifiers.contains(binding.store_uuid().as_bytes()) {
            return Ok(None);
        }
        let intent = self.read_intent(&self.binding_intent_path(binding.identity()))?;
        let authorized = intent.is_some_and(|intent| {
            matches!(
                intent.phase(),
                RegistryIntentPhase::Binding(
                    BindingPhase::StoreCreationAuthorized | BindingPhase::StoreVerified
                )
            )
        });
        authorized
            .then(|| construct_identified_store(binding.store_uuid(), main_thread))
            .transpose()
    }

    fn write_lifecycle(
        &self,
        identity: ProfileIdentity,
        lifecycle: ProfileLifecycleRecord,
    ) -> Result<(), WvError> {
        write_record(
            &self.identity_dir(identity).join("lifecycle.v1"),
            &lifecycle.to_record_bytes().map_err(WvError::from)?,
            false,
            self.uid,
        )
    }

    fn read_purge_record(
        &self,
        identity: ProfileIdentity,
    ) -> Result<Option<ProfilePurgeRecord>, WvError> {
        read_record(&self.purge_intent_path(identity), self.uid)?
            .map(|bytes| {
                let record =
                    ProfilePurgeRecord::from_record_bytes(&bytes).map_err(WvError::from)?;
                if record.identity() != identity || record.platform() != ProfilePlatform::Macos {
                    return Err(profile_error(ProfileErrorKind::RegistryCorruption));
                }
                Ok(record)
            })
            .transpose()
    }

    fn reject_pending_purge(&self, identity: ProfileIdentity) -> Result<(), WvError> {
        if self.read_purge_record(identity)?.is_some() {
            return Err(profile_error(ProfileErrorKind::ActiveIntent));
        }
        if let Some(intent) = self.read_intent(&self.binding_intent_path(identity))?
            && matches!(intent.phase(), RegistryIntentPhase::Purging(_))
        {
            return Err(profile_error(ProfileErrorKind::ActiveIntent));
        }
        Ok(())
    }

    fn validate_purge_authority(&self, binding: StoreBinding) -> Result<(), WvError> {
        let purge_record = self.read_purge_record(binding.identity())?;
        let intent = self.read_intent(&self.binding_intent_path(binding.identity()))?;
        if let Some(intent) = intent {
            if intent.store_binding() != binding {
                return Err(profile_error(ProfileErrorKind::RegistryCorruption));
            }
            if matches!(intent.phase(), RegistryIntentPhase::Binding(_)) {
                return Err(profile_error(ProfileErrorKind::ActiveIntent));
            }
            if purge_record.is_some() {
                return Ok(());
            }
            return Err(profile_error(ProfileErrorKind::RegistryCorruption));
        }
        if let Some(record) = purge_record
            && matches!(
                record.phase(),
                ProfilePurgePhase::DataRemoved | ProfilePurgePhase::Completed
            )
        {
            return Ok(());
        }
        let identity_dir = self.identity_dir(binding.identity());
        let uuid_dir = self.uuid_dir(binding.store_uuid());
        self.ensure_profile_directory(&identity_dir, binding.identity(), false)?;
        self.ensure_profile_directory(&uuid_dir, binding.identity(), false)?;
        let forward = self.read_binding(&identity_dir.join("forward.v1"))?;
        let reverse = self.read_binding(&uuid_dir.join("reverse.v1"))?;
        let active = self.read_binding(&identity_dir.join("active.v1"))?;
        if forward == RegistryRecord::Present(binding)
            && reverse == RegistryRecord::Present(binding)
            && active == RegistryRecord::Present(binding)
        {
            Ok(())
        } else {
            Err(profile_error(ProfileErrorKind::RegistryCorruption))
        }
    }

    fn resume_purge(
        &self,
        binding: StoreBinding,
        event_loop: &mut EventLoop<WkUserEvent>,
        _main_thread: MainThreadMarker,
    ) -> Result<(), WvError> {
        let identity = binding.identity();
        let purge_record_path = self.purge_intent_path(identity);
        let mut purge_record = self.read_or_create_purge_record(identity, &purge_record_path)?;
        if self.resume_purge_after_cleared_intent(
            binding,
            &mut purge_record,
            &purge_record_path,
            event_loop,
        )? {
            return Ok(());
        }
        self.resume_purge_actions(binding, &mut purge_record, &purge_record_path, event_loop)
    }

    fn read_or_create_purge_record(
        &self,
        identity: ProfileIdentity,
        path: &Path,
    ) -> Result<ProfilePurgeRecord, WvError> {
        if let Some(record) = self.read_purge_record(identity)? {
            return Ok(record);
        }
        let prepared = ProfilePurgeRecord::prepared(identity, ProfilePlatform::Macos);
        write_record(
            path,
            &prepared.to_record_bytes().map_err(WvError::from)?,
            true,
            self.uid,
        )?;
        Ok(prepared)
    }

    fn resume_purge_after_cleared_intent(
        &self,
        binding: StoreBinding,
        purge_record: &mut ProfilePurgeRecord,
        purge_record_path: &Path,
        event_loop: &mut EventLoop<WkUserEvent>,
    ) -> Result<bool, WvError> {
        let identity = binding.identity();
        let intent_path = self.binding_intent_path(identity);
        if self.read_intent(&intent_path)?.is_some() {
            return Ok(false);
        }

        let forward = self.read_binding(&self.identity_dir(identity).join("forward.v1"))?;
        let reverse = self.read_binding(&self.uuid_dir(binding.store_uuid()).join("reverse.v1"))?;
        let active = self.read_binding(&self.identity_dir(identity).join("active.v1"))?;
        if forward != RegistryRecord::Missing
            || reverse != RegistryRecord::Missing
            || active != RegistryRecord::Missing
        {
            let snapshot = self.registry_snapshot(binding, StoreObservation::Unknown)?;
            let action = next_registry_action(RegistryRequest::Purge(binding), snapshot)
                .map_err(WvError::from)?;
            let RegistryAction::WritePurgeIntent(intent) = action else {
                return Err(profile_error(ProfileErrorKind::RegistryCorruption));
            };
            write_record(
                &intent_path,
                &intent.to_record_bytes().map_err(WvError::from)?,
                true,
                self.uid,
            )?;
            return Ok(false);
        }

        if purge_record.phase() == ProfilePurgePhase::Prepared {
            return Err(profile_error(ProfileErrorKind::RegistryCorruption));
        }
        let identifiers = fetch_data_store_identifiers(event_loop)?;
        if identifiers.contains(binding.store_uuid().as_bytes()) {
            return Err(profile_error(ProfileErrorKind::RegistryCorruption));
        }
        if purge_record.phase() == ProfilePurgePhase::DataRemoved {
            *purge_record = purge_record
                .advance(ProfilePurgePhase::Completed)
                .map_err(WvError::from)?;
            write_record(
                purge_record_path,
                &purge_record.to_record_bytes().map_err(WvError::from)?,
                false,
                self.uid,
            )?;
        }
        remove_reverse_directory(&self.uuid_dir(binding.store_uuid()), self.uid)?;
        remove_file_if_exists(purge_record_path, self.uid)?;
        Ok(true)
    }

    fn resume_purge_actions(
        &self,
        binding: StoreBinding,
        purge_record: &mut ProfilePurgeRecord,
        purge_record_path: &Path,
        event_loop: &mut EventLoop<WkUserEvent>,
    ) -> Result<(), WvError> {
        let identity = binding.identity();
        let intent_path = self.binding_intent_path(identity);
        let mut observation = StoreObservation::Unknown;
        loop {
            self.record_removed_store_if_needed(observation, purge_record, purge_record_path)?;
            let snapshot = self.registry_snapshot(binding, observation)?;
            let action = next_registry_action(RegistryRequest::Purge(binding), snapshot)
                .map_err(WvError::from)?;
            match action {
                RegistryAction::EnumerateStores => {
                    let identifiers = fetch_data_store_identifiers(event_loop)?;
                    observation = if identifiers.contains(binding.store_uuid().as_bytes()) {
                        StoreObservation::Present {
                            uuid: binding.store_uuid(),
                            persistent: true,
                        }
                    } else {
                        StoreObservation::Absent
                    };
                }
                RegistryAction::WritePurgeIntent(intent) => {
                    write_record(
                        &intent_path,
                        &intent.to_record_bytes().map_err(WvError::from)?,
                        true,
                        self.uid,
                    )?;
                }
                RegistryAction::AdvancePurgeIntent(phase) => {
                    let current = self
                        .read_intent(&intent_path)?
                        .ok_or_else(|| profile_error(ProfileErrorKind::RegistryCorruption))?;
                    let next = current.with_purge_phase(phase);
                    write_record(
                        &intent_path,
                        &next.to_record_bytes().map_err(WvError::from)?,
                        false,
                        self.uid,
                    )?;
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    crash_after_test_purge_phase(phase);
                }
                RegistryAction::RemoveStore(binding) => {
                    remove_data_store(binding.store_uuid(), event_loop)?;
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    crash_after_test_purge_barrier();
                    observation = StoreObservation::Unknown;
                }
                RegistryAction::RemoveReverseRecord => {
                    self.remove_binding_direction(binding, true)?;
                }
                RegistryAction::RemoveForwardRecord => {
                    self.remove_binding_direction(binding, false)?;
                }
                RegistryAction::ClearActiveStatus => {
                    remove_file_if_exists(
                        &self.identity_dir(identity).join("active.v1"),
                        self.uid,
                    )?;
                }
                RegistryAction::ClearIntent => {
                    remove_file_if_exists(&intent_path, self.uid)?;
                    break;
                }
                RegistryAction::WriteBindingIntent(_)
                | RegistryAction::AdvanceBindingIntent(_)
                | RegistryAction::WriteReverseRecord(_)
                | RegistryAction::WriteForwardRecord(_)
                | RegistryAction::ConstructStore(_)
                | RegistryAction::ReuseStore
                | RegistryAction::MarkBindingActive => {
                    return Err(profile_error(ProfileErrorKind::RegistryCorruption));
                }
            }
        }

        remove_reverse_directory(&self.uuid_dir(binding.store_uuid()), self.uid)?;
        if purge_record.phase() != ProfilePurgePhase::Completed {
            *purge_record = purge_record
                .advance(ProfilePurgePhase::Completed)
                .map_err(WvError::from)?;
            write_record(
                purge_record_path,
                &purge_record.to_record_bytes().map_err(WvError::from)?,
                false,
                self.uid,
            )?;
        }
        remove_file_if_exists(purge_record_path, self.uid)?;
        Ok(())
    }

    fn record_removed_store_if_needed(
        &self,
        observation: StoreObservation,
        purge_record: &mut ProfilePurgeRecord,
        purge_record_path: &Path,
    ) -> Result<(), WvError> {
        if observation == StoreObservation::Absent
            && purge_record.phase() == ProfilePurgePhase::Prepared
        {
            *purge_record = purge_record
                .advance(ProfilePurgePhase::DataRemoved)
                .map_err(WvError::from)?;
            write_record(
                purge_record_path,
                &purge_record.to_record_bytes().map_err(WvError::from)?,
                false,
                self.uid,
            )?;
        }
        Ok(())
    }

    fn remove_binding_direction(
        &self,
        binding: StoreBinding,
        reverse: bool,
    ) -> Result<(), WvError> {
        let directory = if reverse {
            self.uuid_dir(binding.store_uuid())
        } else {
            self.identity_dir(binding.identity())
        };
        let file = directory.join(if reverse { "reverse.v1" } else { "forward.v1" });
        let expected = self.read_binding(&file)?;
        if expected != RegistryRecord::Present(binding) {
            return Err(profile_error(ProfileErrorKind::RegistryCorruption));
        }
        remove_file_if_exists(&file, self.uid)
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
fn crash_after_test_purge_barrier() {
    if std::env::var_os("KELD_PROFILE_TEST_PURGE_CRASH_AFTER_CALLBACK").is_some() {
        eprintln!("KELD_KEL135_PURGE_CRASH after_removal_callback=true");
        std::process::exit(86);
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
fn crash_after_test_purge_phase(phase: PurgePhase) {
    let phase_name = match phase {
        PurgePhase::StoreAbsent => "StoreAbsent",
        PurgePhase::ReverseRemoved => "ReverseRemoved",
        PurgePhase::ForwardRemoved => "ForwardRemoved",
        PurgePhase::Inactive => "Inactive",
        PurgePhase::Prepared | PurgePhase::StoreRemovalAuthorized => return,
    };
    if std::env::var("KELD_PROFILE_TEST_PURGE_CRASH_AFTER_PHASE").as_deref() == Ok(phase_name) {
        eprintln!("KELD_KEL135_PURGE_CRASH after_fsynced_phase={phase_name}");
        std::process::exit(88);
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
fn crash_after_test_binding_reverse_record() {
    if std::env::var_os("KELD_PROFILE_TEST_BINDING_CRASH_AFTER_REVERSE").is_some() {
        eprintln!("KELD_KEL135_BINDING_CRASH after_reverse_record=true");
        std::process::exit(87);
    }
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
fn crash_after_test_binding_phase(phase: &str) {
    if std::env::var_os("KELD_PROFILE_TEST_BINDING_CRASH_AFTER")
        .as_deref()
        .is_some_and(|value| value == phase)
    {
        eprintln!("KELD_KEL135_BINDING_CRASH after={phase}");
        std::process::exit(if phase == "store-created" { 88 } else { 89 });
    }
}

fn profile_error(kind: ProfileErrorKind) -> WvError {
    WvError::from(ProfileError::platform_failure(kind))
}

fn application_support_path() -> Result<PathBuf, WvError> {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    if let Some(path) = std::env::var_os("KELD_PROFILE_TEST_ROOT") {
        let supplied = PathBuf::from(path);
        if !supplied.is_absolute() {
            return Err(profile_error(ProfileErrorKind::MarkerMismatch));
        }
        let temp = std::env::temp_dir()
            .canonicalize()
            .map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
        let canonical = supplied
            .canonicalize()
            .map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
        if !canonical.starts_with(temp) {
            return Err(profile_error(ProfileErrorKind::MarkerMismatch));
        }
        return Ok(canonical);
    }

    let directories = NSSearchPathForDirectoriesInDomains(
        NSSearchPathDirectory::ApplicationSupportDirectory,
        NSSearchPathDomainMask::UserDomainMask,
        true,
    );
    if directories.len() != 1 {
        return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
    }
    directories
        .firstObject()
        .map(|path| PathBuf::from(path.to_string()))
        .ok_or_else(|| profile_error(ProfileErrorKind::LifecycleUnproven))
}

fn validate_parent_chain(path: &Path, uid: u32) -> Result<(), WvError> {
    if !path.is_absolute() {
        return Err(profile_error(ProfileErrorKind::MarkerMismatch));
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => current.push("/"),
            Component::Normal(value) => current.push(value),
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(profile_error(ProfileErrorKind::MarkerMismatch));
            }
        }
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
        validate_parent_directory(&current, &metadata, uid)?;
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
    if metadata.uid() != uid {
        return Err(profile_error(ProfileErrorKind::MarkerMismatch));
    }
    Ok(())
}

fn validate_parent_directory(
    path: &Path,
    metadata: &fs::Metadata,
    uid: u32,
) -> Result<(), WvError> {
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || (metadata.uid() != 0 && metadata.uid() != uid)
    {
        return Err(profile_error(ProfileErrorKind::MarkerMismatch));
    }
    let mode = metadata.permissions().mode();
    // POSIX sticky directories such as root-owned /private/tmp prevent an
    // ordinary foreign user from renaming a root- or current-user-owned child.
    // Every next component's owner is checked before accepting that boundary.
    // https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/rename.2.html
    if mode & 0o022 != 0 && mode & 0o1000 == 0 {
        return Err(profile_error(ProfileErrorKind::MarkerMismatch));
    }
    validate_no_allow_acl(path, metadata)
}

fn create_private_directory_if_missing(path: &Path, uid: u32) -> Result<bool, WvError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            validate_private_directory(path, uid)?;
            Ok(false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            builder.mode(METADATA_DIRECTORY_MODE);
            match builder.create(path) {
                Ok(()) => {
                    validate_private_directory(path, uid)?;
                    Ok(true)
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    validate_private_directory(path, uid)?;
                    Ok(false)
                }
                Err(_) => Err(profile_error(ProfileErrorKind::MarkerMismatch)),
            }
        }
        Err(_) => Err(profile_error(ProfileErrorKind::MarkerMismatch)),
    }
}

fn ensure_private_directory(path: &Path, uid: u32) -> Result<(), WvError> {
    create_private_directory_if_missing(path, uid).map(|_| ())
}

fn validate_private_directory(path: &Path, uid: u32) -> Result<(), WvError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != uid
        || metadata.permissions().mode() & 0o777 != METADATA_DIRECTORY_MODE
    {
        return Err(profile_error(ProfileErrorKind::MarkerMismatch));
    }
    validate_no_allow_acl(path, &metadata)
}

fn validate_metadata_file(path: &Path, uid: u32) -> Result<(), WvError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.uid() != uid
        || metadata.permissions().mode() & 0o777 != METADATA_FILE_MODE
    {
        return Err(profile_error(ProfileErrorKind::MarkerMismatch));
    }
    validate_no_allow_acl(path, &metadata)
}

#[allow(unsafe_code)] // exact read-only ACL inspection of an opened metadata or ancestor node
fn validate_no_allow_acl(path: &Path, named: &fs::Metadata) -> Result<(), WvError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC).bits())
        .open(path)
        .map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
    let opened = file
        .metadata()
        .map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
    if opened.dev() != named.dev() || opened.ino() != named.ino() {
        return Err(profile_error(ProfileErrorKind::MarkerMismatch));
    }
    // SAFETY: `file` holds this exact non-symlink metadata or ancestor node open, and the
    // public API reads an independent extended ACL without retaining the fd.
    let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), ACL_TYPE_EXTENDED) };
    if acl.is_null() {
        // macOS 26.5.1 returns ENOENT for an existing valid fd with no extended
        // ACL (isolated runner control). The held fd excludes path disappearance;
        // all other read errors fail closed. Apple's acl_get(3) specifies the
        // NULL/errno contract at the URL above.
        return if std::io::Error::last_os_error().raw_os_error() == Some(nix::libc::ENOENT) {
            Ok(())
        } else {
            Err(profile_error(ProfileErrorKind::MarkerMismatch))
        };
    }
    let checked = {
        let mut last = std::ptr::null_mut();
        // SAFETY: `acl` is live and `last` is a writable out-pointer. The
        // Darwin ACL_LAST_ENTRY selector names the final entry in this copy.
        let last_status = unsafe { acl_get_entry(acl, ACL_LAST_ENTRY, &raw mut last) };
        if last_status != 0 || last.is_null() {
            Err(profile_error(ProfileErrorKind::MarkerMismatch))
        } else {
            let mut entry = std::ptr::null_mut();
            let mut which = ACL_FIRST_ENTRY;
            loop {
                // SAFETY: `acl` is the live result of acl_get_fd_np, `entry` is a
                // writable out-pointer, and the returned entry remains owned by acl.
                let status = unsafe { acl_get_entry(acl, which, &raw mut entry) };
                if status != 0 {
                    break Err(profile_error(ProfileErrorKind::MarkerMismatch));
                }
                let mut tag = 0;
                // SAFETY: status 0 produced a live entry and `tag` is writable.
                if unsafe { acl_get_tag_type(entry, &raw mut tag) } != 0 {
                    break Err(profile_error(ProfileErrorKind::MarkerMismatch));
                }
                if tag == ACL_EXTENDED_ALLOW {
                    break Err(profile_error(ProfileErrorKind::MarkerMismatch));
                }
                if tag != ACL_EXTENDED_DENY {
                    break Err(profile_error(ProfileErrorKind::MarkerMismatch));
                }
                if entry == last {
                    break Ok(());
                }
                which = ACL_NEXT_ENTRY;
            }
        }
    };
    // SAFETY: `acl` was returned by acl_get_fd_np and is released exactly once
    // after entry iteration, including rejection and read-error paths.
    if unsafe { acl_free(acl) } != 0 {
        return Err(profile_error(ProfileErrorKind::MarkerMismatch));
    }
    checked
}

fn open_metadata_file(path: &Path, write: bool, create: bool, uid: u32) -> Result<File, WvError> {
    if let Some(parent) = path.parent() {
        validate_private_directory(parent, uid)?;
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(write)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC).bits());
    if create {
        options.create(true).mode(METADATA_FILE_MODE);
    }
    let file = options
        .open(path)
        .map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
    validate_metadata_file(path, uid)?;
    let opened = file
        .metadata()
        .map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
    let named =
        fs::symlink_metadata(path).map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
    if opened.dev() != named.dev() || opened.ino() != named.ino() {
        return Err(profile_error(ProfileErrorKind::MarkerMismatch));
    }
    Ok(file)
}

fn read_record(path: &Path, uid: u32) -> Result<Option<Vec<u8>>, WvError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(profile_error(ProfileErrorKind::MarkerMismatch)),
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.uid() != uid
        || metadata.permissions().mode() & 0o777 != METADATA_FILE_MODE
    {
        return Err(profile_error(ProfileErrorKind::MarkerMismatch));
    }
    let mut file = open_metadata_file(path, false, false, uid)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(u64::try_from(METADATA_MAX_BYTES + 1).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .map_err(|_| profile_error(ProfileErrorKind::InvalidRecord))?;
    if bytes.len() > METADATA_MAX_BYTES {
        return Err(profile_error(ProfileErrorKind::InvalidRecord));
    }
    Ok(Some(bytes))
}

fn write_record(path: &Path, bytes: &[u8], create_new: bool, uid: u32) -> Result<(), WvError> {
    let parent = path
        .parent()
        .ok_or_else(|| profile_error(ProfileErrorKind::MarkerMismatch))?;
    validate_private_directory(parent, uid)?;
    let record_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| profile_error(ProfileErrorKind::MarkerMismatch))?;
    cleanup_temporary_record_files(parent, record_name, uid)?;
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".keld-profile-tmp-{record_name}-{}-{sequence}",
        std::process::id()
    ));
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(METADATA_FILE_MODE)
            .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC).bits())
            .open(&temporary)
            .map_err(|_| profile_error(ProfileErrorKind::LifecycleUnproven))?;
        file.write_all(bytes)
            .map_err(|_| profile_error(ProfileErrorKind::LifecycleUnproven))?;
        file.sync_all()
            .map_err(|_| profile_error(ProfileErrorKind::LifecycleUnproven))?;
        drop(file);

        if create_new {
            match fs::hard_link(&temporary, path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Err(profile_error(ProfileErrorKind::RegistryCorruption));
                }
                Err(_) => return Err(profile_error(ProfileErrorKind::LifecycleUnproven)),
            }
            fs::remove_file(&temporary)
                .map_err(|_| profile_error(ProfileErrorKind::LifecycleUnproven))?;
        } else {
            match fs::symlink_metadata(path) {
                Ok(_) => validate_metadata_file(path, uid)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(profile_error(ProfileErrorKind::MarkerMismatch)),
            }
            fs::rename(&temporary, path)
                .map_err(|_| profile_error(ProfileErrorKind::LifecycleUnproven))?;
        }
        sync_directory(parent)?;
        Ok(())
    })();
    if write_result.is_err() {
        match fs::remove_file(&temporary) {
            Ok(()) => {
                let _ = sync_directory(parent);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(profile_error(ProfileErrorKind::LifecycleUnproven)),
        }
    }
    write_result
}

fn cleanup_temporary_files(directory: &Path, uid: u32) -> Result<(), WvError> {
    cleanup_temporary_files_with_prefix(directory, ".keld-profile-tmp-", uid)
}

fn cleanup_temporary_record_files(
    directory: &Path,
    record_name: &str,
    uid: u32,
) -> Result<(), WvError> {
    cleanup_temporary_files_with_prefix(
        directory,
        &format!(".keld-profile-tmp-{record_name}-"),
        uid,
    )
}

fn cleanup_temporary_files_with_prefix(
    directory: &Path,
    prefix: &str,
    uid: u32,
) -> Result<(), WvError> {
    let entries =
        fs::read_dir(directory).map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
    let mut removed = false;
    for entry in entries {
        let entry = entry.map_err(|_| profile_error(ProfileErrorKind::MarkerMismatch))?;
        if !entry.file_name().to_string_lossy().starts_with(prefix) {
            continue;
        }
        validate_metadata_file(&entry.path(), uid)?;
        fs::remove_file(entry.path())
            .map_err(|_| profile_error(ProfileErrorKind::LifecycleUnproven))?;
        removed = true;
    }
    if removed {
        sync_directory(directory)?;
    }
    Ok(())
}

fn sync_directory(directory: &Path) -> Result<(), WvError> {
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| profile_error(ProfileErrorKind::LifecycleUnproven))
}

fn remove_file_if_exists(path: &Path, uid: u32) -> Result<(), WvError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            validate_metadata_file(path, uid)?;
            fs::remove_file(path)
                .map_err(|_| profile_error(ProfileErrorKind::LifecycleUnproven))?;
            if let Some(parent) = path.parent() {
                sync_directory(parent)?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(profile_error(ProfileErrorKind::MarkerMismatch)),
    }
}

fn remove_reverse_directory(path: &Path, uid: u32) -> Result<(), WvError> {
    if !path_exists(path)? {
        return Ok(());
    }
    validate_private_directory(path, uid)?;
    cleanup_temporary_files(path, uid)?;
    remove_file_if_exists(&path.join("profile.owner.v1"), uid)?;
    fs::remove_dir(path).map_err(|_| profile_error(ProfileErrorKind::RegistryCorruption))?;
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

fn path_exists(path: &Path) -> Result<bool, WvError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(profile_error(ProfileErrorKind::MarkerMismatch)),
    }
}

#[allow(unsafe_code)] // exact WKWebsiteDataStore persistence and macOS 14 UUID getters
fn verify_store(
    store: &WKWebsiteDataStore,
    expected_uuid: Option<AppleStoreUuid>,
    persistent: bool,
) -> Result<(), WvError> {
    let major = NSProcessInfo::processInfo()
        .operatingSystemVersion()
        .majorVersion;
    if persistent && major < 14 {
        return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
    }
    // SAFETY: callers hold a live WKWebsiteDataStore on the AppKit main thread.
    let observed_persistent = unsafe { store.isPersistent() };
    let observed_uuid = if major >= 14 {
        // SAFETY: the identifier property is read on the AppKit main thread and
        // is available starting in macOS 14.
        unsafe { store.identifier() }.map(|uuid| uuid.as_bytes())
    } else {
        None
    };
    let expected_bytes = expected_uuid.map(|uuid| *uuid.as_bytes());
    if observed_persistent != persistent || observed_uuid != expected_bytes {
        return Err(profile_error(ProfileErrorKind::RegistryCorruption));
    }
    Ok(())
}

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
#[allow(unsafe_code)] // acceptance-only WKWebsiteDataStore facts on the AppKit thread
fn report_store_selection(
    mode: &str,
    identity: Option<ProfileIdentity>,
    store: Option<&Retained<WKWebsiteDataStore>>,
) {
    use std::ffi::OsStr;

    if std::env::var_os("KELD_PROFILE_ACCEPTANCE_REPORT").as_deref() != Some(OsStr::new("1")) {
        return;
    }
    let Some(store) = store else { return };
    // SAFETY: this test-only report reads the live selected store on the AppKit
    // main thread after verify_store has already checked its expected state.
    let persistent = unsafe { store.isPersistent() };
    let identifier = if NSProcessInfo::processInfo()
        .operatingSystemVersion()
        .majorVersion
        >= 14
    {
        // SAFETY: WKWebsiteDataStore.identifier is read on the AppKit main
        // thread and this API exists only from macOS 14 onward.
        match unsafe { store.identifier() } {
            Some(uuid) => uuid.to_string().to_ascii_lowercase(),
            None => String::from("none"),
        }
    } else {
        String::from("unavailable-before-14")
    };
    let profile_identity = match identity {
        Some(value) => value.namespace_segment(),
        None => String::from("ephemeral"),
    };
    let store_uuid = match identity {
        Some(value) => value.apple_store_uuid().to_string(),
        None => String::from("none"),
    };
    eprintln!(
        "KELD_KEL135_STORE mode={mode} profile_identity={profile_identity} expected_store_uuid={store_uuid} actual_identifier={identifier} persistent={persistent}"
    );
}

#[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
fn report_store_selection(
    _mode: &str,
    _identity: Option<ProfileIdentity>,
    _store: Option<&Retained<WKWebsiteDataStore>>,
) {
}

#[allow(unsafe_code)] // exact macOS 14 identifier-addressed WKWebsiteDataStore factory
fn construct_identified_store(
    uuid: AppleStoreUuid,
    main_thread: MainThreadMarker,
) -> Result<Retained<WKWebsiteDataStore>, WvError> {
    // SAFETY: the deterministic UUID is validated by ProfileIdentity, the OS
    // version was checked before this call, and WebKit is called on the UI thread.
    let store = unsafe {
        WKWebsiteDataStore::dataStoreForIdentifier(
            &NSUUID::from_bytes(*uuid.as_bytes()),
            main_thread,
        )
    };
    verify_store(&store, Some(uuid), true)?;
    Ok(store)
}

#[allow(unsafe_code)] // exact memory-only WKWebsiteDataStore factory on the AppKit thread
fn bootstrap_webkit_registry(
    main_thread: MainThreadMarker,
) -> Result<Retained<WKWebsiteDataStore>, WvError> {
    // SAFETY: this memory-only store is created on the AppKit main thread to
    // initialize WebKit before registry enumeration; it carries no disk state.
    let store = unsafe { WKWebsiteDataStore::nonPersistentDataStore(main_thread) };
    verify_store(&store, None, false)?;
    Ok(store)
}

fn fetch_data_store_identifiers(
    _event_loop: &mut EventLoop<super::WkUserEvent>,
) -> Result<Vec<[u8; 16]>, WvError> {
    wait_for_profile_event(|sender| {
        <wry::WebView as WebViewExtDarwin>::fetch_data_store_identifiers(move |identifiers| {
            let _ = sender.send(Ok(identifiers));
        })
        .map_err(|error| WvError::EventLoop(error.to_string()))
    })
}

fn remove_data_store(
    uuid: AppleStoreUuid,
    _event_loop: &mut EventLoop<super::WkUserEvent>,
) -> Result<(), WvError> {
    wait_for_profile_event(|sender| {
        <wry::WebView as WebViewExtDarwin>::remove_data_store(uuid.as_bytes(), move |result| {
            let result = result.map_err(|error| error.to_string());
            let _ = sender.send(Ok(result));
        });
        Ok(())
    })?
    .map_err(|_| profile_error(ProfileErrorKind::ProfileInUse))
}

#[allow(unsafe_code)] // read CoreFoundation's process-constant default run-loop mode
fn wait_for_profile_event<T>(
    start: impl FnOnce(mpsc::SyncSender<Result<T, WvError>>) -> Result<(), WvError>,
) -> Result<T, WvError> {
    let deadline = Instant::now() + STORE_ENUMERATION_DEADLINE;
    let (sender, receiver) = mpsc::sync_channel(1);
    start(sender)?;
    loop {
        match receiver.try_recv() {
            Ok(result) => return result,
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
            }
            Err(mpsc::TryRecvError::Empty) if Instant::now() >= deadline => {
                return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
            }
            Err(mpsc::TryRecvError::Empty) => {
                // SAFETY: CoreFoundation's exported default mode is a process
                // constant valid for the lifetime of this main-thread pump.
                let default_mode = unsafe { kCFRunLoopDefaultMode };
                let _ = CFRunLoop::run_in_mode(
                    default_mode,
                    Duration::from_millis(10).as_secs_f64(),
                    true,
                );
            }
        }
    }
}

#[allow(unsafe_code)] // exact kern.bootsessionuuid sysctl query for reboot-scoped recovery
fn current_boot_identity() -> Result<BootIdentity, WvError> {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    if let Some(boot_uuid) = std::env::var_os("KELD_PROFILE_TEST_BOOT_UUID") {
        if std::env::var_os("KELD_PROFILE_TEST_ROOT").is_none() {
            return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
        }
        let boot_uuid = boot_uuid
            .to_str()
            .and_then(parse_uuid_bytes)
            .ok_or_else(|| profile_error(ProfileErrorKind::LifecycleUnproven))?;
        return BootIdentity::from_host_verified_bytes(boot_uuid).map_err(WvError::from);
    }

    let key = c"kern.bootsessionuuid";
    let mut buffer = [0_u8; 64];
    let mut length = buffer.len();
    // SAFETY: `key` is NUL terminated, `buffer` is writable for `length` bytes,
    // and the output-length pointer is valid. No new sysctl value is supplied.
    let status = unsafe {
        nix::libc::sysctlbyname(
            key.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &raw mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || length == 0 || length > buffer.len() {
        return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
    }
    let uuid_text = std::str::from_utf8(&buffer[..length])
        .map_err(|_| profile_error(ProfileErrorKind::LifecycleUnproven))?
        .trim_end_matches('\0');
    let bytes = parse_uuid_bytes(uuid_text)
        .ok_or_else(|| profile_error(ProfileErrorKind::LifecycleUnproven))?;
    BootIdentity::from_host_verified_bytes(bytes).map_err(WvError::from)
}

#[allow(unsafe_code)] // exact self-PID proc_bsdinfo query for crash-liveness identity
fn current_process_identity() -> Result<ProfileProcessIdentity, WvError> {
    let pid = std::process::id();
    let pid_i32 =
        i32::try_from(pid).map_err(|_| profile_error(ProfileErrorKind::LifecycleUnproven))?;
    // SAFETY: `proc_bsdinfo` is a C ABI struct containing only integer scalars
    // and byte arrays; all-zero is a valid writable output buffer for proc_pidinfo.
    let mut info: nix::libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = i32::try_from(std::mem::size_of::<nix::libc::proc_bsdinfo>())
        .map_err(|_| profile_error(ProfileErrorKind::LifecycleUnproven))?;
    // SAFETY: `info` is a valid writable PROC_PIDTBSDINFO-sized buffer for the
    // current PID, and the process identity query does not retain its pointer.
    let observed = unsafe {
        nix::libc::proc_pidinfo(
            pid_i32,
            nix::libc::PROC_PIDTBSDINFO,
            0,
            (&raw mut info).cast(),
            size,
        )
    };
    if observed != size || info.pbi_pid != pid {
        return Err(profile_error(ProfileErrorKind::LifecycleUnproven));
    }
    let process_birth = info
        .pbi_start_tvsec
        .checked_mul(1_000_000)
        .and_then(|seconds| seconds.checked_add(info.pbi_start_tvusec))
        .ok_or_else(|| profile_error(ProfileErrorKind::LifecycleUnproven))?;
    ProfileProcessIdentity::from_host_observation(pid, process_birth).map_err(WvError::from)
}

fn parse_uuid_bytes(value: &str) -> Option<[u8; 16]> {
    let bytes = value.as_bytes();
    if bytes.len() != 36
        || [8, 13, 18, 23]
            .into_iter()
            .any(|index| bytes[index] != b'-')
    {
        return None;
    }
    let mut output = [0_u8; 16];
    let mut high = None;
    let mut cursor = 0;
    for byte in bytes.iter().copied().filter(|byte| *byte != b'-') {
        let nibble = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return None,
        };
        if let Some(upper) = high.take() {
            *output.get_mut(cursor)? = (upper << 4) | nibble;
            cursor += 1;
        } else {
            high = Some(nibble);
        }
    }
    (cursor == output.len() && high.is_none()).then_some(output)
}

#[cfg(test)]
mod acl_tests {
    use super::{METADATA_DIRECTORY_MODE, METADATA_FILE_MODE, TEMP_FILE_SEQUENCE};
    use super::{validate_metadata_file, validate_parent_chain, validate_private_directory};
    use std::fs::{self, DirBuilder, OpenOptions};
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::Ordering;

    struct ScratchRoot(PathBuf);

    impl Drop for ScratchRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn private_dir(path: &Path) {
        let mut builder = DirBuilder::new();
        builder.mode(METADATA_DIRECTORY_MODE);
        builder
            .create(path)
            .expect("create isolated ACL fixture directory");
    }

    fn add_acl(path: &Path, rule: &str) {
        let output = Command::new("/bin/chmod")
            .args(["+a", rule])
            .arg(path)
            .output()
            .expect("set isolated fixture ACL");
        assert!(output.status.success(), "chmod +a failed: {output:?}");
    }

    fn acl_listing(path: &Path) -> String {
        let output = Command::new("/bin/ls")
            .arg("-lde")
            .arg(path)
            .output()
            .expect("read isolated fixture ACL");
        assert!(output.status.success(), "ls -lde failed: {output:?}");
        String::from_utf8(output.stdout).expect("ACL listing is UTF-8")
    }

    #[test]
    fn keld_owned_metadata_rejects_allow_acl_and_preserves_deny_acl() {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = ScratchRoot(
            std::env::temp_dir().join(format!("keld-macos-acl-{}-{sequence}", std::process::id())),
        );
        private_dir(&root.0);
        let uid = nix::unistd::Uid::current().as_raw();
        assert!(validate_private_directory(&root.0, uid).is_ok());

        let explicit = root.0.join("explicit-allow");
        private_dir(&explicit);
        add_acl(&explicit, "everyone allow read,write");
        assert!(validate_private_directory(&explicit, uid).is_err());

        let inherited_parent = root.0.join("inherited-parent");
        private_dir(&inherited_parent);
        add_acl(
            &inherited_parent,
            "everyone allow read,write,execute,file_inherit,directory_inherit",
        );
        let inherited_child = inherited_parent.join("child");
        private_dir(&inherited_child);
        assert!(acl_listing(&inherited_child).contains("inherited allow"));
        assert!(validate_private_directory(&inherited_child, uid).is_err());

        let file = root.0.join("metadata.v1");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(METADATA_FILE_MODE)
            .open(&file)
            .expect("create isolated metadata file");
        add_acl(&file, "everyone allow read");
        assert!(validate_metadata_file(&file, uid).is_err());

        let mixed = root.0.join("deny-then-allow");
        private_dir(&mixed);
        add_acl(&mixed, "everyone deny writesecurity");
        add_acl(&mixed, "everyone allow read");
        let mixed_before = acl_listing(&mixed);
        assert!(validate_private_directory(&mixed, uid).is_err());
        assert_eq!(acl_listing(&mixed), mixed_before);

        let deny_only = root.0.join("deny-only");
        private_dir(&deny_only);
        add_acl(&deny_only, "everyone deny writesecurity");
        let before = acl_listing(&deny_only);
        assert!(validate_private_directory(&deny_only, uid).is_ok());
        assert_eq!(acl_listing(&deny_only), before, "validation changed an ACL");
    }

    #[test]
    fn parent_chain_rejects_foreign_mutation_rights_and_accepts_sticky_temp() {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = ScratchRoot(PathBuf::from("/private/tmp").join(format!(
            "keld-macos-parent-acl-{}-{sequence}",
            std::process::id()
        )));
        private_dir(&root.0);
        let uid = nix::unistd::Uid::current().as_raw();
        assert!(
            validate_parent_chain(&root.0, uid).is_ok(),
            "the root-owned sticky /private/tmp ancestor protects this owned child"
        );

        let writable_parent = root.0.join("writable-parent");
        private_dir(&writable_parent);
        fs::set_permissions(&writable_parent, fs::Permissions::from_mode(0o777))
            .expect("widen only isolated scratch parent mode");
        let writable_child = writable_parent.join("support");
        private_dir(&writable_child);
        assert!(
            validate_parent_chain(&writable_child, uid).is_err(),
            "foreign-writable parent can replace its Keld child"
        );

        let acl_parent = root.0.join("acl-parent");
        private_dir(&acl_parent);
        let acl_child = acl_parent.join("support");
        private_dir(&acl_child);
        add_acl(&acl_parent, "everyone allow read,write,execute");
        assert!(
            validate_parent_chain(&acl_child, uid).is_err(),
            "parent ACL Allow can mutate the Keld subtree despite mode 0700"
        );
    }
}
