//! macOS HID sender classification via IOHIDEventSystemClient SPI.
//!
//! Universal Control injects remote keystrokes through user-space virtual HID
//! services, which WindowServer delivers with `pid == 0` and `tag == 0` just
//! like a local physical keyboard. CoreGraphics still copies the originating
//! service's registry ID into undocumented `CGEventField(87)`, so we maintain a
//! non-blocking cache mapping registry IDs to `Physical` vs `Virtual`.

use objc2_core_foundation::{CFArray, CFNumber, CFRetained, CFString, CFType};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Once, OnceLock, RwLock};
use std::time::{Duration, Instant};

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    // Create rule: returns +1 retained IOHIDEventSystemClientRef (CFType) or null.
    fn IOHIDEventSystemClientCreateSimpleClient(allocator: *const c_void) -> *mut c_void;
    // Copy rule: returns +1 retained CFArrayRef or null.
    fn IOHIDEventSystemClientCopyServices(client: *const c_void) -> *mut c_void;
    // Get rule: returns borrowed CFTypeRef (CFNumberRef) or null. Do NOT release.
    fn IOHIDServiceClientGetRegistryID(service: *const c_void) -> *const c_void;
    // Copy rule: returns +1 retained CFTypeRef or null.
    fn IOHIDServiceClientCopyProperty(service: *const c_void, key: *const c_void) -> *mut c_void;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SenderKind {
    Physical,
    Virtual,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RefreshSummary {
    pub total: usize,
    pub physical: usize,
    pub virtual_count: usize,
}

/// Pure classification of an HID service from its metadata properties.
///
/// Only services without a `V-` product prefix, not using `UniversalControl`
/// transport, and backed by a non-empty IOKit class are considered `Physical`.
pub(super) fn classify_service(
    product: Option<&str>,
    transport: Option<&str>,
    io_class: Option<&str>,
) -> SenderKind {
    let has_virtual_product = product.is_some_and(|p| p.starts_with("V-"));
    let is_universal_control = transport == Some("UniversalControl");
    let has_io_class = io_class.is_some_and(|c| !c.is_empty());

    if !has_virtual_product && !is_universal_control && has_io_class {
        SenderKind::Physical
    } else {
        SenderKind::Virtual
    }
}

struct SenderCache {
    entries: RwLock<HashMap<u64, SenderKind>>,
    refresh_requested: AtomicBool,
}

impl SenderCache {
    fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            refresh_requested: AtomicBool::new(false),
        }
    }

    /// Non-blocking, non-allocating lookup used inside the CGEventTap callback.
    ///
    /// Returns `None` on `sender == 0`, cache miss (recording a refresh
    /// request), or lock contention.
    fn lookup(&self, sender: u64) -> Option<SenderKind> {
        if sender == 0 {
            return None;
        }
        let Ok(guard) = self.entries.try_read() else {
            return None;
        };
        if let Some(&kind) = guard.get(&sender) {
            Some(kind)
        } else {
            if !self.refresh_requested.swap(true, Ordering::AcqRel)
                && let Some(thread) = REFRESH_THREAD_HANDLE.get()
            {
                thread.unpark();
            }
            None
        }
    }

    fn replace(&self, fresh: HashMap<u64, SenderKind>) {
        let mut guard = self
            .entries
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = fresh;
    }
}

static CACHE: LazyLock<SenderCache> = LazyLock::new(SenderCache::new);
static REFRESH_THREAD: Once = Once::new();
static REFRESH_THREAD_HANDLE: OnceLock<std::thread::Thread> = OnceLock::new();
static WARN_LOGGED: AtomicBool = AtomicBool::new(false);

fn log_refresh_failure_once(error: &str) {
    if !WARN_LOGGED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "macOS HID sender cache unavailable; physical provenance will fail closed to Unknown: {error}"
        );
    }
}

fn copy_string_property(service: *const c_void, key: &CFString) -> Option<String> {
    // SAFETY: `service` is a valid IOHIDServiceClientRef element borrowed from
    // the CFArray returned by `IOHIDEventSystemClientCopyServices`, and `key`
    // is a valid `CFString`. `IOHIDServiceClientCopyProperty` follows the Copy
    // rule and returns a +1 retained CFTypeRef or null.
    let raw = unsafe { IOHIDServiceClientCopyProperty(service, std::ptr::from_ref(key).cast()) };
    let ptr = NonNull::new(raw)?;
    // SAFETY: `ptr` is non-null and +1 retained per the Copy rule.
    let cf_val: CFRetained<CFType> = unsafe { CFRetained::from_raw(ptr.cast()) };
    let cf_str = cf_val.downcast::<CFString>().ok()?;
    Some(cf_str.to_string())
}

fn enumerate_services() -> std::result::Result<(HashMap<u64, SenderKind>, RefreshSummary), String> {
    // SAFETY: Passing null uses the default CoreFoundation allocator; Create
    // rule returns a +1 retained IOHIDEventSystemClientRef or null.
    let client_raw = unsafe { IOHIDEventSystemClientCreateSimpleClient(std::ptr::null()) };
    let client_ptr = NonNull::new(client_raw)
        .ok_or_else(|| "IOHIDEventSystemClientCreateSimpleClient returned null".to_string())?;
    // SAFETY: `client_ptr` is non-null and +1 retained per the Create rule.
    let client: CFRetained<CFType> = unsafe { CFRetained::from_raw(client_ptr.cast()) };

    // SAFETY: `client` is a valid retained IOHIDEventSystemClientRef; Copy
    // rule returns a +1 retained CFArrayRef or null.
    let services_raw =
        unsafe { IOHIDEventSystemClientCopyServices(std::ptr::from_ref(&*client).cast()) };
    let services_ptr = NonNull::new(services_raw)
        .ok_or_else(|| "IOHIDEventSystemClientCopyServices returned null".to_string())?;
    // SAFETY: `services_ptr` is a non-null +1 retained CFArrayRef of
    // IOHIDServiceClientRef elements per the Copy rule.
    let services: CFRetained<CFArray> = unsafe { CFRetained::from_raw(services_ptr.cast()) };

    let product_key = CFString::from_static_str("Product");
    let transport_key = CFString::from_static_str("Transport");
    let io_class_key = CFString::from_static_str("IOClass");

    let count = services.count();
    let mut map = HashMap::with_capacity(usize::try_from(count).unwrap_or(0));

    for idx in 0..count {
        // SAFETY: `idx` is in `0..services.count()`, and `services` is a
        // CFArray containing untyped IOHIDServiceClientRef pointers.
        let service = unsafe { services.value_at_index(idx) };
        if service.is_null() {
            continue;
        }

        // SAFETY: `service` is a valid IOHIDServiceClientRef borrowed from
        // `services`. `IOHIDServiceClientGetRegistryID` follows the Get rule
        // and returns a borrowed CFNumberRef (or null) valid while `services`
        // keeps the element alive.
        let reg_raw = unsafe { IOHIDServiceClientGetRegistryID(service) };
        let Some(reg_ptr) = NonNull::new(reg_raw.cast_mut()) else {
            continue;
        };
        // SAFETY: `reg_ptr` is a non-null borrowed CFTypeRef valid for this
        // loop iteration; we do NOT take ownership or release it.
        let reg_type: &CFType = unsafe { reg_ptr.cast::<CFType>().as_ref() };
        let Some(reg_number) = reg_type.downcast_ref::<CFNumber>() else {
            continue;
        };
        let Some(reg_id_signed) = reg_number.as_i64() else {
            continue;
        };
        let sender_id = reg_id_signed as u64;
        if sender_id == 0 {
            continue;
        }

        let product = copy_string_property(service, &product_key);
        let transport = copy_string_property(service, &transport_key);
        let io_class = copy_string_property(service, &io_class_key);

        let kind = classify_service(
            product.as_deref(),
            transport.as_deref(),
            io_class.as_deref(),
        );
        map.insert(sender_id, kind);
    }

    let mut summary = RefreshSummary {
        total: map.len(),
        physical: 0,
        virtual_count: 0,
    };
    for &kind in map.values() {
        match kind {
            SenderKind::Physical => summary.physical += 1,
            SenderKind::Virtual => summary.virtual_count += 1,
        }
    }

    Ok((map, summary))
}

pub(super) fn lookup(sender: u64) -> Option<SenderKind> {
    CACHE.lookup(sender)
}

pub(super) fn refresh() -> std::result::Result<RefreshSummary, String> {
    let (map, summary) = enumerate_services()?;
    CACHE.replace(map);
    log::debug!(
        "refreshed macOS HID sender cache: total={}, physical={}, virtual={}",
        summary.total,
        summary.physical,
        summary.virtual_count
    );
    Ok(summary)
}

pub(super) fn initialize() {
    if let Err(error) = refresh() {
        log_refresh_failure_once(&error);
    }

    REFRESH_THREAD.call_once(|| {
        let spawned = std::thread::Builder::new()
            .name("monio-hid-sender-refresh".to_string())
            .spawn(|| {
                let mut last_refresh = Instant::now();
                loop {
                    while !CACHE.refresh_requested.load(Ordering::Acquire) {
                        std::thread::park();
                    }
                    let elapsed = last_refresh.elapsed();
                    if elapsed < Duration::from_secs(1) {
                        std::thread::sleep(Duration::from_secs(1) - elapsed);
                    }
                    if CACHE.refresh_requested.swap(false, Ordering::AcqRel) {
                        if let Err(error) = refresh() {
                            log_refresh_failure_once(&error);
                        }
                        last_refresh = Instant::now();
                    }
                }
            });
        match spawned {
            Ok(handle) => {
                let _ = REFRESH_THREAD_HANDLE.set(handle.thread().clone());
                if CACHE.refresh_requested.load(Ordering::Acquire) {
                    handle.thread().unpark();
                }
            }
            Err(error) => {
                log_refresh_failure_once(&format!("failed to spawn HID sender refresh thread: {error}"));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};

    #[test]
    fn classify_service_universal_control_virtual_keyboard_is_virtual() {
        assert_eq!(
            classify_service(Some("V-K1_5.0"), Some("Bluetooth Low Energy"), None),
            SenderKind::Virtual
        );
    }

    #[test]
    fn classify_service_universal_control_automouse_is_virtual() {
        assert_eq!(
            classify_service(Some("V-UC Automouse"), Some("UniversalControl"), None),
            SenderKind::Virtual
        );
        assert_eq!(
            classify_service(
                Some("Automouse"),
                Some("UniversalControl"),
                Some("AppleUserHIDEventService")
            ),
            SenderKind::Virtual
        );
    }

    #[test]
    fn classify_service_without_io_class_is_virtual() {
        assert_eq!(
            classify_service(Some("Some Keyboard"), Some("USB"), None),
            SenderKind::Virtual
        );
        assert_eq!(
            classify_service(Some("Some Keyboard"), Some("USB"), Some("")),
            SenderKind::Virtual
        );
    }

    #[test]
    fn classify_service_bluetooth_physical_device_is_physical() {
        assert_eq!(
            classify_service(
                Some("M720 Triathlon"),
                Some("Bluetooth Low Energy"),
                Some("AppleUserHIDEventService")
            ),
            SenderKind::Physical
        );
    }

    #[test]
    fn classify_service_usb_keyboard_is_physical() {
        assert_eq!(
            classify_service(
                Some("USB Keyboard"),
                Some("USB"),
                Some("IOUSBHostHIDDevice")
            ),
            SenderKind::Physical
        );
    }

    #[test]
    fn classify_service_none_product_with_class_is_physical() {
        assert_eq!(
            classify_service(None, None, Some("AppleSMCKeysEndpoint")),
            SenderKind::Physical
        );
    }

    #[test]
    fn cache_miss_sets_refresh_request_and_zero_does_not() {
        let cache = SenderCache::new();

        assert_eq!(cache.lookup(0), None);
        assert!(!cache.refresh_requested.load(Ordering::Acquire));

        assert_eq!(cache.lookup(0x0001_0006_177d), None);
        assert!(cache.refresh_requested.load(Ordering::Acquire));
    }

    #[test]
    fn cache_hit_returns_kind_without_requesting_refresh() {
        let cache = SenderCache::new();
        let mut entries = HashMap::new();
        entries.insert(0x0001_0006_177d, SenderKind::Physical);
        entries.insert(0x007b_cad0_a800_0e4e, SenderKind::Virtual);
        cache.replace(entries);

        assert_eq!(cache.lookup(0x0001_0006_177d), Some(SenderKind::Physical));
        assert_eq!(cache.lookup(0x007b_cad0_a800_0e4e), Some(SenderKind::Virtual));
        assert!(!cache.refresh_requested.load(Ordering::Acquire));
    }

    #[test]
    fn cache_lookup_under_held_write_lock_returns_none_without_blocking() {
        let cache = Arc::new(SenderCache::new());
        let mut entries = HashMap::new();
        entries.insert(0x0001_0006_177d, SenderKind::Physical);
        cache.replace(entries);

        let guard = cache.entries.write().expect("write lock should succeed");
        let cache_clone = Arc::clone(&cache);
        let (tx, rx) = mpsc::channel();

        let handle = std::thread::spawn(move || {
            let result = cache_clone.lookup(0x0001_0006_177d);
            let _ = tx.send(result);
        });

        let result = rx
            .recv_timeout(Duration::from_millis(500))
            .expect("lookup should return immediately without blocking on write lock");
        assert_eq!(result, None);

        drop(guard);
        handle.join().expect("worker thread should join");
    }

    #[test]
    #[ignore = "reads live IOHIDEventSystemClient services on macOS"]
    fn lists_hid_services_live() {
        let summary = refresh().expect("IOHIDEventSystemClient service listing should succeed");
        println!(
            "lists_hid_services_live: total={} physical={} virtual={}",
            summary.total, summary.physical, summary.virtual_count
        );
        assert!(
            summary.physical > 0,
            "expected at least one physical HID service on this Mac, got {summary:?}"
        );
        assert_eq!(summary.total, summary.physical + summary.virtual_count);
    }
}
