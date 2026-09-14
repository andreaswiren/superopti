//! Process-owned handles, not invented per-thread file ownership.
use crate::{model::Report, native::ProcessIdentity};
use windows::{
    Win32::{
        Foundation::*,
        Storage::FileSystem::QueryDosDeviceW,
        System::{Diagnostics::ProcessSnapshotting::*, Threading::*},
    },
    core::PCWSTR,
};
struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct Snapshot(HPSS);
impl Drop for Snapshot {
    fn drop(&mut self) {
        unsafe {
            PssFreeSnapshot(GetCurrentProcess(), self.0);
        }
    }
}
struct Marker(HPSSWALK);
impl Drop for Marker {
    fn drop(&mut self) {
        unsafe {
            PssWalkMarkerFree(self.0);
        }
    }
}
unsafe fn string(pointer: PCWSTR, bytes: u16) -> String {
    if pointer.is_null() || bytes == 0 {
        return String::new();
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(pointer.0, bytes as usize / 2))
}
fn drive_map() -> Vec<(String, String)> {
    let mut map = Vec::new();
    for letter in b'A'..=b'Z' {
        let name = format!("{}:", letter as char);
        let wide = crate::metrics::wide(&name);
        let mut buffer = [0u16; 1024];
        let length = unsafe { QueryDosDeviceW(PCWSTR(wide.as_ptr()), Some(&mut buffer)) };
        if length > 0 {
            let end = buffer
                .iter()
                .position(|v| *v == 0)
                .unwrap_or(length as usize);
            map.push((String::from_utf16_lossy(&buffer[..end]), name));
        }
    }
    map.sort_by_key(|(device, _)| std::cmp::Reverse(device.len()));
    map
}
fn display_path(name: &str, map: &[(String, String)]) -> String {
    for (device, drive) in map {
        if let Some(rest) = name.strip_prefix(device)
            && (rest.is_empty() || rest.starts_with('\\'))
        {
            return format!("{drive}{rest}");
        }
    }
    if let Some(rest) = name.strip_prefix("\\Device\\Mup\\") {
        return format!("\\\\{rest}");
    }
    name.into()
}
// Call through the bounded probe helper: a driver can delay handle-name queries.
pub fn snapshot(pid: u32, expected: Option<u64>) -> Result<Report, String> {
    let identity = ProcessIdentity::open(pid, expected)?;
    unsafe {
        let process = Process(
            OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_DUP_HANDLE,
                false,
                pid,
            )
            .map_err(|e| format!("Cannot query file handles: {e}"))?,
        );
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user)
            .map_err(|e| e.to_string())?;
        if (((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
            != identity.created_ticks
        {
            return Err("The selected process identity changed".into());
        }
        let mut raw = HPSS::default();
        let status = PssCaptureSnapshot(
            process.0,
            PSS_CAPTURE_HANDLES
                | PSS_CAPTURE_HANDLE_NAME_INFORMATION
                | PSS_CAPTURE_HANDLE_BASIC_INFORMATION,
            None,
            &mut raw,
        );
        if status != 0 {
            return Err(format!(
                "Handle snapshot unavailable: Windows error {status}"
            ));
        }
        let snapshot = Snapshot(raw);
        let mut raw_marker = HPSSWALK::default();
        let status = PssWalkMarkerCreate(None, &mut raw_marker);
        if status != 0 {
            return Err(format!(
                "Handle iterator unavailable: Windows error {status}"
            ));
        }
        let marker = Marker(raw_marker);
        let map = drive_map();
        let mut visited = 0;
        let mut named = 0;
        let mut file_count = 0;
        let mut report = Report::new(
            "Open files · process-shared handles",
            &[
                "Handle",
                "Object type",
                "File / object path",
                "Access mask",
                "Name status",
            ],
        );
        loop {
            let mut entry = PSS_HANDLE_ENTRY::default();
            let bytes = std::slice::from_raw_parts_mut(
                (&mut entry as *mut PSS_HANDLE_ENTRY).cast::<u8>(),
                size_of::<PSS_HANDLE_ENTRY>(),
            );
            let status = PssWalkSnapshot(snapshot.0, PSS_WALK_HANDLES, marker.0, Some(bytes));
            if status == ERROR_NO_MORE_ITEMS.0 {
                break;
            }
            if status != 0 {
                return Err(format!("Handle iteration failed: Windows error {status}"));
            }
            visited += 1;
            let kind = if entry.Flags.contains(PSS_HANDLE_HAVE_TYPE) {
                string(entry.TypeName, entry.TypeNameLength)
            } else {
                String::new()
            };
            if kind.eq_ignore_ascii_case("File") {
                file_count += 1;
                let path = if entry.Flags.contains(PSS_HANDLE_HAVE_NAME) {
                    string(entry.ObjectName, entry.ObjectNameLength)
                } else {
                    String::new()
                };
                if !path.is_empty() {
                    named += 1;
                }
                let mask = if entry.Flags.contains(PSS_HANDLE_HAVE_BASIC_INFORMATION) {
                    format!("0x{:08X}", entry.GrantedAccess)
                } else {
                    "Unavailable".into()
                };
                report.row(&[
                    &format!("0x{:X}", entry.Handle.0 as usize),
                    &kind,
                    &display_path(&path, &map),
                    &mask,
                    if path.is_empty() {
                        "Unavailable"
                    } else {
                        "Captured"
                    },
                ]);
            }
            if visited >= 20000 {
                report.title = "Open files · first 20,000 handles examined (partial)".into();
                break;
            }
        }
        report.metric("File handles", file_count);
        report.metric("Named", named);
        report.metric("Handles examined", visited);
        report.metric("Ownership", "Process shared");
        Ok(report)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_respect_volume_boundaries() {
        let map = vec![("\\Device\\HarddiskVolume1".into(), "C:".into())];
        assert_eq!(
            display_path("\\Device\\HarddiskVolume1\\data.txt", &map),
            "C:\\data.txt"
        );
        assert_eq!(
            display_path("\\Device\\HarddiskVolume10\\data.txt", &map),
            "\\Device\\HarddiskVolume10\\data.txt"
        );
        assert_eq!(
            display_path("\\Device\\Mup\\server\\share\\file", &map),
            "\\\\server\\share\\file"
        );
    }
}
