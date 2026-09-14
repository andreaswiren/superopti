use crate::{metrics::wide, model::Report, native::ProcessIdentity};
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ProcessMetadata {
    pub path: String,
    pub company: String,
    pub version: String,
    pub description: String,
    pub size_bytes: Option<u64>,
    pub error: Option<String>,
}
impl ProcessMetadata {
    pub fn tooltip(&self) -> String {
        let known = |s: &str| {
            if s.is_empty() {
                "Unavailable".to_string()
            } else {
                s.to_string()
            }
        };
        format!(
            "{}\nManufacturer: {} (file metadata)\nVersion: {}\nFile size: {}\nLocation: {}{}",
            known(&self.description),
            known(&self.company),
            known(&self.version),
            self.size_bytes
                .map(|s| format!("{s} bytes ({:.2} MiB)", s as f64 / 1048576.))
                .unwrap_or_else(|| "Unavailable".into()),
            known(&self.path),
            self.error
                .as_ref()
                .map(|e| format!("\n{e}"))
                .unwrap_or_default()
        )
    }
}
pub fn metadata(pid: u32, expected: Option<u64>) -> ProcessMetadata {
    let mut out = ProcessMetadata::default();
    let result = (|| -> Result<(), String> {
        let identity = ProcessIdentity::open(pid, expected)?;
        out.path = identity.image_path()?;
        out.size_bytes = std::fs::metadata(&out.path).ok().map(|m| m.len());
        unsafe {
            use windows::Win32::Storage::FileSystem::*;
            let path = wide(&out.path);
            let size = GetFileVersionInfoSizeW(PCWSTR(path.as_ptr()), None);
            if size == 0 {
                return Ok(());
            }
            if size > 4 * 1024 * 1024 {
                return Err("Version resource exceeds inspection limit".into());
            }
            let mut bytes = vec![0u8; size as usize];
            GetFileVersionInfoW(PCWSTR(path.as_ptr()), None, size, bytes.as_mut_ptr().cast())
                .map_err(|e| e.to_string())?;
            let mut pointer = std::ptr::null_mut();
            let mut length = 0;
            let translation = wide("\\VarFileInfo\\Translation");
            let pair = if VerQueryValueW(
                bytes.as_ptr().cast(),
                PCWSTR(translation.as_ptr()),
                &mut pointer,
                &mut length,
            )
            .as_bool()
                && length >= 4
                && !pointer.is_null()
            {
                let words = std::slice::from_raw_parts(pointer.cast::<u16>(), 2);
                (words[0], words[1])
            } else {
                (0x0409, 0x04b0)
            };
            let get = |field: &str| {
                let key = wide(&format!(
                    "\\StringFileInfo\\{:04x}{:04x}\\{field}",
                    pair.0, pair.1
                ));
                let mut text = std::ptr::null_mut();
                let mut count = 0;
                if VerQueryValueW(
                    bytes.as_ptr().cast(),
                    PCWSTR(key.as_ptr()),
                    &mut text,
                    &mut count,
                )
                .as_bool()
                    && !text.is_null()
                    && count > 0
                    && count < 32768
                {
                    String::from_utf16_lossy(std::slice::from_raw_parts(
                        text.cast::<u16>(),
                        count as usize,
                    ))
                    .trim_end_matches('\0')
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(1024)
                    .collect()
                } else {
                    String::new()
                }
            };
            out.company = get("CompanyName");
            out.version = get("FileVersion");
            out.description = get("FileDescription");
        }
        Ok(())
    })();
    out.error = result.err();
    out
}
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, Diagnostics::ToolHelp::*, Services::*},
        UI::Shell::*,
    },
    core::PCWSTR,
};

pub fn open_folder(path: &str) -> Result<(), String> {
    // Shell namespaces, URLs and device handles are not filesystem locations.
    if path.contains('\0')
        || !(path.as_bytes().get(1) == Some(&b':') || path.starts_with("\\\\"))
        || path.starts_with("\\\\.\\")
    {
        return Err("This item has no navigable filesystem path".into());
    }
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        let mut pidl = std::ptr::null_mut();
        let name = wide(path);
        let result = SHParseDisplayName(PCWSTR(name.as_ptr()), None, &mut pidl, 0, None)
            .and_then(|_| SHOpenFolderAndSelectItems(pidl, None, 0));
        if !pidl.is_null() {
            CoTaskMemFree(Some(pidl.cast()));
        }
        CoUninitialize();
        result.map_err(|e| format!("Explorer could not select this item: {e}"))
    }
}

pub fn services(pid: u32, expected: Option<u64>) -> Result<Report, String> {
    let _identity = ProcessIdentity::open(pid, expected)?;
    let mut report = Report::new(
        format!("Windows services · PID {pid}"),
        &["Service name", "Display name", "PID", "State"],
    );
    unsafe {
        let scm =
            OpenSCManagerW(None, None, SC_MANAGER_ENUMERATE_SERVICE).map_err(|e| e.to_string())?;
        let result = (|| {
            let mut memory = vec![0u64; 256 * 1024 / 8];
            let buffer =
                std::slice::from_raw_parts_mut(memory.as_mut_ptr().cast::<u8>(), memory.len() * 8);
            let (mut needed, mut count, mut resume) = (0, 0, 0);
            loop {
                let status = EnumServicesStatusExW(
                    scm,
                    SC_ENUM_PROCESS_INFO,
                    SERVICE_WIN32,
                    SERVICE_STATE_ALL,
                    Some(buffer),
                    &mut needed,
                    &mut count,
                    Some(&mut resume),
                    None,
                );
                if let Err(e) = &status
                    && e.code() != ERROR_MORE_DATA.to_hresult()
                {
                    return Err(e.to_string());
                }
                if count as usize * size_of::<ENUM_SERVICE_STATUS_PROCESSW>() > buffer.len() {
                    return Err("Invalid service enumeration size".into());
                }
                for item in std::slice::from_raw_parts(
                    buffer.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>(),
                    count as usize,
                ) {
                    if item.ServiceStatusProcess.dwProcessId == pid {
                        report.row(&[
                            &item.lpServiceName.to_string().unwrap_or_default(),
                            &item.lpDisplayName.to_string().unwrap_or_default(),
                            &pid.to_string(),
                            match item.ServiceStatusProcess.dwCurrentState {
                                SERVICE_RUNNING => "Running",
                                SERVICE_STOPPED => "Stopped",
                                SERVICE_START_PENDING => "Starting",
                                SERVICE_STOP_PENDING => "Stopping",
                                SERVICE_PAUSED => "Paused",
                                SERVICE_PAUSE_PENDING => "Pausing",
                                SERVICE_CONTINUE_PENDING => "Resuming",
                                _ => "Unknown",
                            },
                        ]);
                    }
                }
                if status.is_ok() {
                    break;
                }
            }
            Ok(())
        })();
        let _ = CloseServiceHandle(scm);
        result?;
    }
    report.metric("Associated services", report.rows.len().to_string());
    report.metric("Source", "Windows Service Control Manager");
    Ok(report)
}

pub fn children(pid: u32, expected: Option<u64>) -> Result<Report, String> {
    let identity = ProcessIdentity::open(pid, expected)?;
    let mut report = Report::new(
        format!("Process tree · PID {pid}"),
        &["Process", "PID", "Parent PID", "Executable", "Identity"],
    );
    let mut entries = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut valid = Process32FirstW(snap, &mut entry).is_ok();
        while valid && entries.len() < 32768 {
            entries.push(entry);
            valid = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    let mut pending = vec![(pid, identity.created_ticks, 0usize)];
    let mut seen = std::collections::HashSet::new();
    while let Some((current, created, depth)) = pending.pop() {
        if !seen.insert(current) || depth > 64 {
            continue;
        }
        if let Some(entry) = entries.iter().find(|e| e.th32ProcessID == current) {
            let instance = ProcessIdentity::open(current, Some(created));
            let path = instance
                .as_ref()
                .ok()
                .and_then(|p| p.image_path().ok())
                .unwrap_or_else(|| "Unavailable".into());
            let end = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = format!(
                "{}{}",
                "  ".repeat(depth),
                String::from_utf16_lossy(&entry.szExeFile[..end])
            );
            report.row(&[
                &name,
                &current.to_string(),
                &entry.th32ParentProcessID.to_string(),
                &path,
                if instance.is_ok() {
                    "Verified"
                } else {
                    "Exited / inaccessible"
                },
            ]);
        }
        for child in entries
            .iter()
            .filter(|e| e.th32ParentProcessID == current && e.th32ProcessID != current)
        {
            // A recycled parent PID is not evidence of a parent-child relation.
            if let Ok(child_instance) = ProcessIdentity::open(child.th32ProcessID, None)
                && child_instance.created_ticks >= created
            {
                pending.push((child.th32ProcessID, child_instance.created_ticks, depth + 1));
            }
        }
    }
    report.metric("Processes", report.rows.len().to_string());
    report.metric(
        "Coverage",
        "Accessible live descendants; snapshot relationships",
    );
    Ok(report)
}

#[cfg(test)]
mod metadata_tests {
    #[test]
    fn inspects_real_executable_without_treating_missing_fields_as_facts() {
        let result = super::metadata(std::process::id(), None);
        assert!(result.error.is_none(), "{:?}", result.error);
        assert!(std::path::Path::new(&result.path).is_absolute());
        assert!(result.size_bytes.is_some_and(|size| size > 0));
        let empty = super::ProcessMetadata::default().tooltip();
        assert!(empty.contains("Manufacturer: Unavailable"));
        assert!(empty.contains("Version: Unavailable"));
        assert!(empty.contains("File size: Unavailable"));
    }
}
