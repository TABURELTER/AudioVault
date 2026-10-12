//! Windows volume enumeration through Win32 volume management APIs.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    BusType1394, BusTypeMmc, BusTypeSd, BusTypeUsb, CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, FindFirstVolumeW,
    FindNextVolumeW, FindVolumeClose, GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
    GetVolumeNameForVolumeMountPointW, GetVolumePathNamesForVolumeNameW, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_DEVICE_DESCRIPTOR,
    STORAGE_PROPERTY_QUERY, StorageDeviceProperty, StorageDeviceSeekPenaltyProperty,
};

use super::{MediaClass, VolumeInfo};

const FILE_READ_ONLY_VOLUME: u32 = 0x0008_0000;
// GetDriveTypeW results (winbase.h).
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;
const DRIVE_RAMDISK: u32 = 6;

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

fn from_wide_nul(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    OsString::from_wide(&buf[..end]).to_string_lossy().into_owned()
}

/// Split a double-NUL-terminated multi-string.
fn multi_sz(buf: &[u16]) -> Vec<String> {
    buf.split(|&c| c == 0).filter(|s| !s.is_empty()).map(|s| OsString::from_wide(s).to_string_lossy().into_owned()).collect()
}

pub fn list() -> Vec<VolumeInfo> {
    let mut out: Vec<VolumeInfo> = Vec::new();
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();

    // 1. Every local volume, including ones mounted into folders.
    let mut name = [0u16; 512];
    // SAFETY: buffer pointer/length describe a valid writable array.
    let handle = unsafe { FindFirstVolumeW(name.as_mut_ptr(), name.len() as u32) };
    if handle != INVALID_HANDLE_VALUE {
        loop {
            let guid_path = from_wide_nul(&name);
            for mount in mount_paths(&guid_path) {
                if let Some(v) = describe(&mount, Some(guid_path.clone()), &system_drive) {
                    out.push(v);
                }
            }
            // SAFETY: valid find handle and writable buffer.
            if unsafe { FindNextVolumeW(handle, name.as_mut_ptr(), name.len() as u32) } == 0 {
                break;
            }
        }
        // SAFETY: handle came from FindFirstVolumeW.
        unsafe { FindVolumeClose(handle) };
    }

    // 2. Mapped network drives (not returned by FindFirstVolume).
    let mut drives = [0u16; 512];
    // SAFETY: valid writable buffer.
    let n = unsafe { GetLogicalDriveStringsW(drives.len() as u32, drives.as_mut_ptr()) } as usize;
    if n > 0 && n < drives.len() {
        for root in multi_sz(&drives[..n]) {
            let w = wide(std::ffi::OsStr::new(&root));
            // SAFETY: NUL-terminated wide string.
            if unsafe { GetDriveTypeW(w.as_ptr()) } == DRIVE_REMOTE
                && !out.iter().any(|v| v.mount_path == Path::new(&root))
                && let Some(v) = describe(&root, None, &system_drive)
            {
                out.push(v);
            }
        }
    }
    out.sort_by(|a, b| a.mount_path.cmp(&b.mount_path));
    out
}

fn mount_paths(guid_path: &str) -> Vec<String> {
    let w = wide(std::ffi::OsStr::new(guid_path));
    let mut buf = vec![0u16; 1024];
    let mut needed: u32 = 0;
    // SAFETY: valid input string and output buffer; `needed` receives the required length.
    let ok = unsafe { GetVolumePathNamesForVolumeNameW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32, &mut needed) };
    if ok == 0 {
        if needed as usize > buf.len() && needed < 65536 {
            buf = vec![0u16; needed as usize];
            // SAFETY: as above with a buffer of the requested size.
            let ok2 = unsafe { GetVolumePathNamesForVolumeNameW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32, &mut needed) };
            if ok2 == 0 {
                return Vec::new();
            }
        } else {
            return Vec::new();
        }
    }
    multi_sz(&buf)
}

fn describe(root: &str, guid_path: Option<String>, system_drive: &str) -> Option<VolumeInfo> {
    let root_w = wide(std::ffi::OsStr::new(root));
    // SAFETY: NUL-terminated wide string.
    let drive_type = unsafe { GetDriveTypeW(root_w.as_ptr()) };
    if drive_type == DRIVE_CDROM || drive_type == DRIVE_RAMDISK {
        return None;
    }
    let mut label = [0u16; 261];
    let mut fs = [0u16; 261];
    let mut serial: u32 = 0;
    let mut max_comp: u32 = 0;
    let mut flags: u32 = 0;
    // SAFETY: all buffers are valid and sized as passed.
    let ok = unsafe {
        GetVolumeInformationW(
            root_w.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            &mut serial,
            &mut max_comp,
            &mut flags,
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    };
    if ok == 0 {
        // No media (empty card reader) or inaccessible.
        return None;
    }
    let (free, total) = space(Path::new(root)).unzip();
    let guid_path = guid_path.or_else(|| volume_guid(root));
    let (media, external) = if drive_type == DRIVE_REMOTE {
        (MediaClass::Network, false)
    } else {
        classify(root, drive_type)
    };
    let label = from_wide_nul(&label);
    let is_system = root.trim_end_matches('\\').eq_ignore_ascii_case(system_drive);
    Some(VolumeInfo {
        mount_path: PathBuf::from(root),
        label: if label.is_empty() { None } else { Some(label) },
        fs_type: Some(from_wide_nul(&fs)).filter(|s| !s.is_empty()),
        capacity_bytes: total,
        free_bytes: free,
        platform_id: guid_path,
        serial: if serial != 0 { Some(format!("{:04X}-{:04X}", serial >> 16, serial & 0xFFFF)) } else { None },
        media,
        is_external: external || drive_type == DRIVE_REMOVABLE,
        is_system,
        read_only: flags & FILE_READ_ONLY_VOLUME != 0,
    })
}

fn volume_guid(root: &str) -> Option<String> {
    let w = wide(std::ffi::OsStr::new(root));
    let mut buf = [0u16; 128];
    // SAFETY: valid NUL-terminated input and writable output buffer.
    let ok = unsafe { GetVolumeNameForVolumeMountPointW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    (ok != 0).then(|| from_wide_nul(&buf))
}

pub fn space(path: &Path) -> Option<(u64, u64)> {
    let mut p = path.as_os_str().to_os_string();
    if !p.to_string_lossy().ends_with('\\') {
        p.push("\\");
    }
    let w = wide(&p);
    let (mut avail, mut total, mut total_free) = (0u64, 0u64, 0u64);
    // SAFETY: valid NUL-terminated string and out-pointers to u64.
    let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut total_free) };
    (ok != 0).then_some((avail, total))
}

/// Seek-penalty (rotational) and bus type via IOCTL_STORAGE_QUERY_PROPERTY.
/// Works without administrator rights because the handle is opened with no access.
fn classify(root: &str, drive_type: u32) -> (MediaClass, bool) {
    let letter = root.trim_end_matches('\\');
    let fallback = if drive_type == DRIVE_REMOVABLE { MediaClass::Removable } else { MediaClass::Unknown };
    if !(letter.len() == 2 && letter.ends_with(':')) || !(drive_type == DRIVE_FIXED || drive_type == DRIVE_REMOVABLE) {
        return (fallback, drive_type == DRIVE_REMOVABLE);
    }
    let dev = wide(std::ffi::OsStr::new(&format!("\\\\.\\{letter}")));
    // SAFETY: valid device path; zero desired access only queries metadata.
    let h = unsafe {
        CreateFileW(dev.as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut())
    };
    if h == INVALID_HANDLE_VALUE {
        return (fallback, drive_type == DRIVE_REMOVABLE);
    }
    let mut media = fallback;
    let mut external = drive_type == DRIVE_REMOVABLE;

    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceSeekPenaltyProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    let mut seek = DEVICE_SEEK_PENALTY_DESCRIPTOR { Version: 0, Size: 0, IncursSeekPenalty: false };
    let mut returned: u32 = 0;
    // SAFETY: input/output buffers are valid, correctly sized POD structs; handle is open.
    let ok = unsafe {
        DeviceIoControl(
            h,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &query as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            &mut seek as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok != 0 && returned as usize >= std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() {
        media = if seek.IncursSeekPenalty { MediaClass::Hdd } else { MediaClass::Ssd };
    }

    let query = STORAGE_PROPERTY_QUERY { PropertyId: StorageDeviceProperty, QueryType: PropertyStandardQuery, AdditionalParameters: [0] };
    // Header only; variable-length strings beyond it are not needed.
    let mut buf = [0u8; 1024];
    // SAFETY: as above; output buffer is larger than STORAGE_DEVICE_DESCRIPTOR.
    let ok = unsafe {
        DeviceIoControl(
            h,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &query as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            buf.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok != 0 && returned as usize >= std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
        // SAFETY: the kernel filled at least size_of::<STORAGE_DEVICE_DESCRIPTOR>() bytes;
        // read_unaligned avoids alignment assumptions about the byte buffer.
        let desc: STORAGE_DEVICE_DESCRIPTOR = unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };
        if [BusTypeUsb, BusType1394, BusTypeSd, BusTypeMmc].contains(&desc.BusType) || desc.RemovableMedia {
            external = true;
        }
    }
    // SAFETY: handle was returned by CreateFileW and is closed exactly once.
    unsafe { CloseHandle(h) };
    if media == MediaClass::Unknown && drive_type == DRIVE_REMOVABLE {
        media = MediaClass::Removable;
    }
    (media, external)
}
