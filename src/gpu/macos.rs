//! macOS GPU/NPU collection.
//!
//! GPU usage comes from a public, documented-enough IOKit registry entry
//! (`IOAccelerator`'s "PerformanceStatistics" dictionary — the same data
//! `ioreg -rc IOAccelerator` prints) and needs no elevated privileges.
//!
//! NPU (Apple Neural Engine) usage has no public, sudo-less counter at all —
//! `powermetrics` reports it but requires root. The only sudo-less path is
//! Apple's private, undocumented `IOReport.framework`, loaded here via
//! `dlopen`/`dlsym`, reading the "Energy Model" channel group's "ANE0 Energy"
//! channel. This mirrors the technique used by community tools such as
//! asitop and Stats.app. It is unsupported API: if Apple ever removes these
//! symbols, `load_ioreport()` simply returns `None` and the NPU gauge shows
//! "n/a" — this must never crash the app.

use std::ffi::{CString, c_void};
use std::sync::OnceLock;
use std::time::Instant;

use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{CFGetTypeID, CFRelease, CFTypeRef, kCFAllocatorDefault};
use core_foundation_sys::dictionary::{
    CFDictionaryGetValue, CFDictionaryRef, CFMutableDictionaryRef,
};
use core_foundation_sys::number::{
    CFNumberGetTypeID, CFNumberGetValue, CFNumberRef, kCFNumberDoubleType,
};
use core_foundation_sys::string::{
    CFStringCreateWithCString, CFStringGetCString, CFStringGetLength,
    CFStringGetMaximumSizeForEncoding, CFStringRef, kCFStringEncodingUTF8,
};
use io_kit_sys::types::{io_iterator_t, io_object_t};
use io_kit_sys::{
    IOIteratorNext, IOObjectRelease, IORegistryEntryCreateCFProperties, IOServiceGetMatchingServices,
    IOServiceMatching, kIOMasterPortDefault,
};

use super::{GpuStat, GpuVendor, NpuStat};

/// Rough Apple Neural Engine max power draw, used only to turn a watts
/// reading into an approximate percentage — IOReport exposes energy, not a
/// utilization counter. Deliberately one conservative constant rather than a
/// per-chip-generation table: I don't have a verified authoritative figure
/// per chip, and the UI always shows the raw watts alongside the percent so
/// this approximation is never the only number on screen.
const ANE_MAX_POWER_W: f64 = 10.0;

pub struct Backend {
    ane: Option<AneReporter>,
}

impl Backend {
    pub fn new() -> Self {
        Self {
            ane: AneReporter::new(),
        }
    }

    pub fn sample(&mut self) -> (Vec<GpuStat>, Option<NpuStat>) {
        let gpus = sample_gpu();
        let npu = self.ane.as_mut().map(AneReporter::sample);
        (gpus, npu)
    }
}

// ---------- GPU, via the public IOAccelerator registry ----------

fn sample_gpu() -> Vec<GpuStat> {
    unsafe {
        let name = CString::new("IOAccelerator").unwrap();
        let matching = IOServiceMatching(name.as_ptr());
        if matching.is_null() {
            return Vec::new();
        }

        let mut iterator: io_iterator_t = 0;
        let result =
            IOServiceGetMatchingServices(kIOMasterPortDefault, matching as CFDictionaryRef, &mut iterator);
        if result != 0 {
            return Vec::new();
        }

        let mut stats = Vec::new();
        loop {
            let service: io_object_t = IOIteratorNext(iterator);
            if service == 0 {
                break;
            }
            if let Some(stat) = read_accelerator(service, stats.len()) {
                stats.push(stat);
            }
            IOObjectRelease(service);
        }
        IOObjectRelease(iterator);
        stats
    }
}

unsafe fn read_accelerator(service: io_object_t, index: usize) -> Option<GpuStat> {
    let mut props: CFMutableDictionaryRef = std::ptr::null_mut();
    let result =
        unsafe { IORegistryEntryCreateCFProperties(service, &mut props, kCFAllocatorDefault, 0) };
    if result != 0 || props.is_null() {
        return None;
    }
    let props = props as CFDictionaryRef;

    let perf = unsafe { cf_dict_get(props, "PerformanceStatistics") } as CFDictionaryRef;
    let stat = if !perf.is_null() {
        let util = unsafe { cf_number_f64(perf, "Device Utilization %") };
        let mem_used = unsafe { cf_number_f64(perf, "In use system memory") }.map(|v| v as u64);
        Some(GpuStat {
            name: format!("Apple GPU {index}"),
            vendor: GpuVendor::Apple,
            util_pct: util,
            mem_used,
            mem_total: None, // unified memory — no fixed "VRAM total" to report
        })
    } else {
        None
    };

    unsafe { CFRelease(props as CFTypeRef) };
    stat
}

// ---------- NPU, via the private IOReport framework ----------

struct AneReporter {
    channel: CFDictionaryRef,
    subscription: *const c_void,
    prev_sample: CFDictionaryRef,
    prev_time: Instant,
}

// SAFETY: these CF/IOReport objects are only ever touched from the single
// collector task that owns this `Backend`; exclusive access is already
// enforced by the `RwLock<GpuMonitor>` the backend lives behind.
unsafe impl Send for AneReporter {}
unsafe impl Sync for AneReporter {}

impl AneReporter {
    fn new() -> Option<Self> {
        let fns = get_ioreport()?;
        unsafe {
            let group = cfstring("Energy Model");
            let channel = (fns.copy_channels)(group, std::ptr::null(), 0, 0, 0);
            CFRelease(group as CFTypeRef);
            if channel.is_null() {
                return None;
            }

            let mut desired: CFDictionaryRef = std::ptr::null();
            let subscription = (fns.create_subscription)(
                std::ptr::null(),
                channel,
                &mut desired,
                0,
                std::ptr::null(),
            );
            if subscription.is_null() {
                CFRelease(channel as CFTypeRef);
                return None;
            }
            if !desired.is_null() {
                CFRelease(desired as CFTypeRef);
            }

            let prev_sample = (fns.create_samples)(subscription, channel, std::ptr::null());
            if prev_sample.is_null() {
                CFRelease(channel as CFTypeRef);
                return None;
            }

            Some(Self {
                channel,
                subscription,
                prev_sample,
                prev_time: Instant::now(),
            })
        }
    }

    fn sample(&mut self) -> NpuStat {
        let unavailable = || NpuStat {
            name: "Apple Neural Engine".to_string(),
            power_w: None,
            util_pct_approx: None,
        };

        let Some(fns) = get_ioreport() else {
            return unavailable();
        };

        unsafe {
            let new_sample = (fns.create_samples)(self.subscription, self.channel, std::ptr::null());
            if new_sample.is_null() {
                return unavailable();
            }

            let now = Instant::now();
            let elapsed_s = now.duration_since(self.prev_time).as_secs_f64();

            let delta = (fns.create_samples_delta)(self.prev_sample, new_sample, std::ptr::null());
            CFRelease(self.prev_sample as CFTypeRef);
            self.prev_sample = new_sample;
            self.prev_time = now;

            if delta.is_null() || elapsed_s < 1e-6 {
                return unavailable();
            }

            let joules = parse_ane_joules(fns, delta);
            CFRelease(delta as CFTypeRef);

            let Some(joules) = joules else {
                return unavailable();
            };

            let watts = joules / elapsed_s;
            let pct = (watts / ANE_MAX_POWER_W * 100.0).clamp(0.0, 100.0);
            NpuStat {
                name: "Apple Neural Engine".to_string(),
                power_w: Some(watts),
                util_pct_approx: Some(pct),
            }
        }
    }
}

impl Drop for AneReporter {
    fn drop(&mut self) {
        unsafe {
            if !self.prev_sample.is_null() {
                CFRelease(self.prev_sample as CFTypeRef);
            }
            if !self.channel.is_null() {
                CFRelease(self.channel as CFTypeRef);
            }
        }
    }
}

unsafe fn parse_ane_joules(fns: &IOReportFns, delta: CFDictionaryRef) -> Option<f64> {
    let items = unsafe { cf_dict_get(delta, "IOReportChannels") } as CFArrayRef;
    if items.is_null() {
        return None;
    }
    let count = unsafe { CFArrayGetCount(items) };
    if count <= 0 {
        return None;
    }

    let mut ane_joules = 0.0_f64;
    for i in 0..count {
        let item = unsafe { CFArrayGetValueAtIndex(items, i) };
        if item.is_null() {
            continue;
        }
        let item = item as CFDictionaryRef;

        let name_cf = unsafe { (fns.channel_get_channel_name)(item) };
        if name_cf.is_null() {
            continue;
        }
        let name = unsafe { cfstring_to_string(name_cf) };
        if !name.to_uppercase().contains("ANE") {
            continue;
        }

        let mut err: i32 = 0;
        let value = unsafe { (fns.simple_get_integer_value)(item, &mut err) };
        if err != 0 || value <= 0 {
            continue;
        }

        let unit_cf = unsafe { (fns.channel_get_unit_label)(item) };
        let unit = if unit_cf.is_null() {
            String::new()
        } else {
            unsafe { cfstring_to_string(unit_cf) }
        };

        ane_joules += match unit.as_str() {
            "uJ" => value as f64 / 1e6,
            "mJ" => value as f64 / 1e3,
            _ => value as f64 / 1e9, // nJ default
        };
    }

    Some(ane_joules)
}

// ---------- IOReport FFI (private framework, loaded via dlopen) ----------

type FnCopyChannelsInGroup =
    unsafe extern "C" fn(CFStringRef, CFStringRef, u64, u64, u64) -> CFDictionaryRef;
type FnCreateSubscription = unsafe extern "C" fn(
    *const c_void,
    CFDictionaryRef,
    *mut CFDictionaryRef,
    u64,
    *const c_void,
) -> *const c_void;
type FnCreateSamples =
    unsafe extern "C" fn(*const c_void, CFDictionaryRef, *const c_void) -> CFDictionaryRef;
type FnCreateSamplesDelta =
    unsafe extern "C" fn(CFDictionaryRef, CFDictionaryRef, *const c_void) -> CFDictionaryRef;
type FnChannelGetChannelName = unsafe extern "C" fn(CFDictionaryRef) -> CFStringRef;
type FnSimpleGetIntegerValue = unsafe extern "C" fn(CFDictionaryRef, *mut i32) -> i64;
type FnChannelGetUnitLabel = unsafe extern "C" fn(CFDictionaryRef) -> CFStringRef;

struct IOReportFns {
    copy_channels: FnCopyChannelsInGroup,
    create_subscription: FnCreateSubscription,
    create_samples: FnCreateSamples,
    create_samples_delta: FnCreateSamplesDelta,
    channel_get_channel_name: FnChannelGetChannelName,
    simple_get_integer_value: FnSimpleGetIntegerValue,
    channel_get_unit_label: FnChannelGetUnitLabel,
}

// SAFETY: these are function pointers into a shared library, valid for the
// life of the process and safe to call from any thread.
unsafe impl Send for IOReportFns {}
unsafe impl Sync for IOReportFns {}

static IOREPORT_FNS: OnceLock<Option<IOReportFns>> = OnceLock::new();

fn get_ioreport() -> Option<&'static IOReportFns> {
    IOREPORT_FNS.get_or_init(load_ioreport).as_ref()
}

fn load_ioreport() -> Option<IOReportFns> {
    unsafe {
        let paths = [
            c"/System/Library/PrivateFrameworks/IOReport.framework/IOReport",
            c"/usr/lib/libIOReport.dylib",
        ];

        let mut handle: *mut c_void = std::ptr::null_mut();
        for path in paths {
            handle = libc::dlopen(path.as_ptr(), libc::RTLD_LAZY);
            if !handle.is_null() {
                break;
            }
        }
        if handle.is_null() {
            return None;
        }

        macro_rules! sym {
            ($name:literal, $ty:ty) => {{
                let p = libc::dlsym(handle, $name.as_ptr() as *const i8);
                if p.is_null() {
                    return None;
                }
                std::mem::transmute::<*mut c_void, $ty>(p)
            }};
        }

        Some(IOReportFns {
            copy_channels: sym!(b"IOReportCopyChannelsInGroup\0", FnCopyChannelsInGroup),
            create_subscription: sym!(b"IOReportCreateSubscription\0", FnCreateSubscription),
            create_samples: sym!(b"IOReportCreateSamples\0", FnCreateSamples),
            create_samples_delta: sym!(b"IOReportCreateSamplesDelta\0", FnCreateSamplesDelta),
            channel_get_channel_name: sym!(
                b"IOReportChannelGetChannelName\0",
                FnChannelGetChannelName
            ),
            simple_get_integer_value: sym!(
                b"IOReportSimpleGetIntegerValue\0",
                FnSimpleGetIntegerValue
            ),
            channel_get_unit_label: sym!(b"IOReportChannelGetUnitLabel\0", FnChannelGetUnitLabel),
        })
    }
}

// ---------- shared CoreFoundation helpers ----------

unsafe fn cfstring(s: &str) -> CFStringRef {
    let cstr = CString::new(s).unwrap_or_default();
    unsafe { CFStringCreateWithCString(kCFAllocatorDefault, cstr.as_ptr(), kCFStringEncodingUTF8) }
}

/// Converts a CFString to a Rust String. The caller retains ownership of
/// `cf` (this never releases it — several IOReport accessors return borrowed
/// "Get rule" references that must not be released by the caller).
unsafe fn cfstring_to_string(cf: CFStringRef) -> String {
    let len = unsafe { CFStringGetLength(cf) };
    if len <= 0 {
        return String::new();
    }
    let max_size = unsafe { CFStringGetMaximumSizeForEncoding(len, kCFStringEncodingUTF8) };
    if max_size <= 0 {
        return String::new();
    }
    let mut buf = vec![0u8; max_size as usize + 1];
    let ok = unsafe {
        CFStringGetCString(
            cf,
            buf.as_mut_ptr() as *mut i8,
            buf.len() as isize,
            kCFStringEncodingUTF8,
        )
    };
    if ok == 0 {
        return String::new();
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr() as *const i8) };
    cstr.to_string_lossy().to_string()
}

/// Looks up `key` in `dict` without transferring ownership of the result
/// (CF's "Get rule" — caller must not `CFRelease` the returned pointer).
unsafe fn cf_dict_get(dict: CFDictionaryRef, key: &str) -> *const c_void {
    let cfkey = unsafe { cfstring(key) };
    let value = unsafe { CFDictionaryGetValue(dict, cfkey as *const c_void) };
    unsafe { CFRelease(cfkey as CFTypeRef) };
    value
}

unsafe fn cf_number_f64(dict: CFDictionaryRef, key: &str) -> Option<f64> {
    let value = unsafe { cf_dict_get(dict, key) };
    if value.is_null() {
        return None;
    }
    if unsafe { CFGetTypeID(value as CFTypeRef) } != unsafe { CFNumberGetTypeID() } {
        return None;
    }
    let number = value as CFNumberRef;
    let mut out: f64 = 0.0;
    let ok = unsafe {
        CFNumberGetValue(
            number,
            kCFNumberDoubleType,
            &mut out as *mut f64 as *mut c_void,
        )
    };
    if ok { Some(out) } else { None }
}
