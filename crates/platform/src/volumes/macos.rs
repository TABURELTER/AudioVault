//! macOS volume enumeration: `getmntinfo` for mounts + DiskArbitration for the
//! stable volume UUID, label and internal/removable flags.

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::PathBuf;

use core_foundation_sys::base::{CFAllocatorRef, CFGetTypeID, CFRelease, CFTypeRef, kCFAllocatorDefault};
use core_foundation_sys::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
use core_foundation_sys::number::{CFBooleanGetTypeID, CFBooleanGetValue, CFBooleanRef};
use core_foundation_sys::string::{CFStringGetCString, CFStringGetTypeID, CFStringRef, kCFStringEncodingUTF8};
use core_foundation_sys::uuid::{CFUUIDCreateString, CFUUIDGetTypeID, CFUUIDRef};

use super::{MediaClass, VolumeInfo};

#[repr(C)]
struct OpaqueSession(c_void);
#[repr(C)]
struct OpaqueDisk(c_void);
type DASessionRef = *const OpaqueSession;
type DADiskRef = *const OpaqueDisk;

#[link(name = "DiskArbitration", kind = "framework")]
unsafe extern "C" {
    fn DASessionCreate(allocator: CFAllocatorRef) -> DASessionRef;
    fn DADiskCreateFromBSDName(allocator: CFAllocatorRef, session: DASessionRef, name: *const c_char) -> DADiskRef;
    fn DADiskCopyDescription(disk: DADiskRef) -> CFDictionaryRef;
    static kDADiskDescriptionVolumeUUIDKey: CFStringRef;
    static kDADiskDescriptionVolumeNameKey: CFStringRef;
    static kDADiskDescriptionMediaRemovableKey: CFStringRef;
    static kDADiskDescriptionDeviceInternalKey: CFStringRef;
}

fn cfstring_to_string(s: CFStringRef) -> Option<String> {
    let mut buf = [0 as c_char; 1024];
    // SAFETY: `s` is a valid CFString (checked by caller) and the buffer is writable.
    let ok = unsafe { CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as _, kCFStringEncodingUTF8) };
    if ok == 0 {
        return None;
    }
    // SAFETY: CFStringGetCString NUL-terminates on success.
    Some(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned())
}

struct DiskInfo {
    uuid: Option<String>,
    name: Option<String>,
    removable: bool,
    internal: Option<bool>,
}

fn disk_info(session: DASessionRef, bsd_name: &str) -> Option<DiskInfo> {
    let c = CString::new(bsd_name).ok()?;
    // SAFETY: valid session and NUL-terminated BSD name; returned objects are released below.
    unsafe {
        let disk = DADiskCreateFromBSDName(kCFAllocatorDefault, session, c.as_ptr());
        if disk.is_null() {
            return None;
        }
        let desc = DADiskCopyDescription(disk);
        CFRelease(disk as CFTypeRef);
        if desc.is_null() {
            return None;
        }
        let get = |key: CFStringRef| -> CFTypeRef { CFDictionaryGetValue(desc, key as *const c_void) };
        let uuid_ref = get(kDADiskDescriptionVolumeUUIDKey);
        let uuid = if !uuid_ref.is_null() && CFGetTypeID(uuid_ref) == CFUUIDGetTypeID() {
            let s = CFUUIDCreateString(kCFAllocatorDefault, uuid_ref as CFUUIDRef);
            let out = if s.is_null() { None } else { cfstring_to_string(s) };
            if !s.is_null() {
                CFRelease(s as CFTypeRef);
            }
            out
        } else {
            None
        };
        let name_ref = get(kDADiskDescriptionVolumeNameKey);
        let name = if !name_ref.is_null() && CFGetTypeID(name_ref) == CFStringGetTypeID() {
            cfstring_to_string(name_ref as CFStringRef)
        } else {
            None
        };
        let boolean = |r: CFTypeRef| -> Option<bool> {
            (!r.is_null() && CFGetTypeID(r) == CFBooleanGetTypeID()).then(|| CFBooleanGetValue(r as CFBooleanRef))
        };
        let removable = boolean(get(kDADiskDescriptionMediaRemovableKey)).unwrap_or(false);
        let internal = boolean(get(kDADiskDescriptionDeviceInternalKey));
        CFRelease(desc as CFTypeRef);
        Some(DiskInfo { uuid, name, removable, internal })
    }
}

pub fn list() -> Vec<VolumeInfo> {
    let mut mounts: *mut libc::statfs = std::ptr::null_mut();
    // SAFETY: getmntinfo returns a pointer to an internal static buffer valid until the next call.
    let n = unsafe { libc::getmntinfo(&mut mounts, libc::MNT_NOWAIT) };
    if n <= 0 || mounts.is_null() {
        return Vec::new();
    }
    // SAFETY: `mounts` points to `n` initialized statfs structs.
    let entries = unsafe { std::slice::from_raw_parts(mounts, n as usize) };
    // SAFETY: plain session creation; released at the end.
    let session = unsafe { DASessionCreate(kCFAllocatorDefault) };
    let mut out = Vec::new();
    for st in entries {
        // SAFETY: statfs string fields are NUL-terminated C arrays.
        let mount = unsafe { CStr::from_ptr(st.f_mntonname.as_ptr()) }.to_string_lossy().into_owned();
        let from = unsafe { CStr::from_ptr(st.f_mntfromname.as_ptr()) }.to_string_lossy().into_owned();
        let fstype = unsafe { CStr::from_ptr(st.f_fstypename.as_ptr()) }.to_string_lossy().into_owned();
        let flags = st.f_flags as u32;
        let local = flags & (libc::MNT_LOCAL as u32) != 0;
        let is_network = matches!(fstype.as_str(), "smbfs" | "nfs" | "afpfs" | "webdav");
        if !(local || is_network) || matches!(fstype.as_str(), "devfs" | "autofs" | "nullfs") {
            continue;
        }
        // Hide APFS system-internal volumes (VM, Preboot, Update…) except the data volume and root.
        if mount.starts_with("/System/Volumes/") && mount != "/System/Volumes/Data" {
            continue;
        }
        if flags & (libc::MNT_DONTBROWSE as u32) != 0 && mount != "/System/Volumes/Data" {
            continue;
        }
        let bsize = st.f_bsize as u64;
        let total = st.f_blocks as u64 * bsize;
        let free = st.f_bavail as u64 * bsize;
        let info = if !session.is_null() && from.starts_with("/dev/") {
            disk_info(session, from.trim_start_matches("/dev/"))
        } else {
            None
        };
        let (uuid, name, removable, internal) = match info {
            Some(d) => (d.uuid, d.name, d.removable, d.internal),
            None => (None, None, false, None),
        };
        let external = removable || internal == Some(false);
        // Internal Apple storage is SSD on all supported Macs; external media is
        // reported as Unknown (conservative HDD-like scheduling) until the user sets it.
        let media = if is_network {
            MediaClass::Network
        } else if internal == Some(true) {
            MediaClass::Ssd
        } else if removable {
            MediaClass::Removable
        } else {
            MediaClass::Unknown
        };
        let label = name.or_else(|| {
            PathBuf::from(&mount).file_name().map(|s| s.to_string_lossy().into_owned())
        });
        out.push(VolumeInfo {
            mount_path: PathBuf::from(&mount),
            label,
            fs_type: Some(fstype),
            capacity_bytes: Some(total),
            free_bytes: Some(free),
            platform_id: uuid.or_else(|| if is_network { Some(format!("net:{from}")) } else { None }),
            serial: None,
            media,
            is_external: external,
            is_system: mount == "/" || mount == "/System/Volumes/Data",
            read_only: flags & (libc::MNT_RDONLY as u32) != 0,
        });
    }
    if !session.is_null() {
        // SAFETY: session was created above and is released once.
        unsafe { CFRelease(session as CFTypeRef) };
    }
    out
}
