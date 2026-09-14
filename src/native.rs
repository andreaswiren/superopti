use crate::metrics::{Process, Sample, now};
use std::{collections::HashMap, time::Instant};
use windows::Win32::{
    Foundation::*,
    System::{Diagnostics::ToolHelp::*, ProcessStatus::*, SystemInformation::*, Threading::*},
};
struct Owned(HANDLE);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn ticks(t: FILETIME) -> u64 {
    ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64
}
#[derive(Clone, Copy)]
struct Reading {
    created: u64,
    cpu: u64,
    io: Option<u64>,
}
pub struct NativeMonitor {
    previous: HashMap<u32, Reading>,
    system: Option<(u64, u64)>,
    last: Instant,
    cpus: f64,
}
fn system_times() -> Option<(u64, u64)> {
    unsafe {
        let (mut idle, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()?;
        Some((ticks(idle), ticks(kernel) + ticks(user)))
    }
}
impl NativeMonitor {
    pub fn new() -> Result<Self, String> {
        let mut m = Self {
            previous: HashMap::new(),
            system: None,
            last: Instant::now(),
            cpus: unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) }.max(1) as f64,
        };
        m.sample(0.0)?;
        Ok(m)
    }
    pub fn sample(&mut self, elapsed: f64) -> Result<Sample, String> {
        unsafe {
            let start = Instant::now();
            let seconds = start.duration_since(self.last).as_secs_f64().max(0.001);
            self.last = start;
            let sys = system_times();
            let cpu = sys
                .zip(self.system)
                .and_then(|((idle, total), (old_idle, old_total))| {
                    let dt = total.checked_sub(old_total)?;
                    let di = idle.checked_sub(old_idle)?;
                    (dt > 0).then_some(
                        (100.0 * (dt.saturating_sub(di)) as f64 / dt as f64).clamp(0.0, 100.0),
                    )
                });
            self.system = sys;
            let mut memory = MEMORYSTATUSEX {
                dwLength: size_of::<MEMORYSTATUSEX>() as u32,
                ..Default::default()
            };
            let memory_ok = GlobalMemoryStatusEx(&mut memory).is_ok();
            let mut perf = PERFORMANCE_INFORMATION::default();
            let perf_ok =
                GetPerformanceInfo(&mut perf, size_of::<PERFORMANCE_INFORMATION>() as u32).is_ok();
            let mut sample = Sample {
                unix_seconds: now(),
                elapsed,
                cpu: if GetActiveProcessorGroupCount() > 1 {
                    None
                } else {
                    cpu
                },
                ram: memory_ok.then_some(memory.dwMemoryLoad as f64),
                total_ram_mb: memory.ullTotalPhys as f64 / 1048576.0,
                commit_limit_mb: perf_ok
                    .then_some(perf.CommitLimit as f64 * perf.PageSize as f64 / 1048576.),
                commit: (perf_ok && perf.CommitLimit > 0)
                    .then(|| 100.0 * perf.CommitTotal as f64 / perf.CommitLimit as f64),
                ..Default::default()
            };
            let snapshot =
                Owned(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?);
            let mut entry = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut next = HashMap::new();
            let mut valid = Process32FirstW(snapshot.0, &mut entry).is_ok();
            while valid {
                let pid = entry.th32ProcessID;
                if pid != 0 {
                    let end = entry
                        .szExeFile
                        .iter()
                        .position(|v| *v == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let mut p = Process {
                        pid,
                        parent_pid: entry.th32ParentProcessID,
                        name: String::from_utf16_lossy(&entry.szExeFile[..end]),
                        threads: Some(entry.cntThreads as f64),
                        ..Default::default()
                    };
                    let handle =
                        OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
                            .or_else(|_| {
                                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                            });
                    if let Ok(handle) = handle {
                        let h = Owned(handle);
                        let (mut created, mut exit, mut kernel, mut user) = (
                            FILETIME::default(),
                            FILETIME::default(),
                            FILETIME::default(),
                            FILETIME::default(),
                        );
                        let mut io = IO_COUNTERS::default();
                        let io = GetProcessIoCounters(h.0, &mut io)
                            .is_ok()
                            .then_some(io.ReadTransferCount.saturating_add(io.WriteTransferCount));
                        if GetProcessTimes(h.0, &mut created, &mut exit, &mut kernel, &mut user)
                            .is_ok()
                        {
                            p.created_ticks = Some(ticks(created));
                            let reading = Reading {
                                created: ticks(created),
                                cpu: ticks(kernel) + ticks(user),
                                io,
                            };
                            if let Some(old) = self
                                .previous
                                .get(&pid)
                                .filter(|old| old.created == reading.created)
                            {
                                p.cpu = reading.cpu.checked_sub(old.cpu).map(|dt| {
                                    (dt as f64 / 1e7 / seconds / self.cpus * 100.0)
                                        .clamp(0.0, 100.0)
                                });
                                p.io_mb = reading
                                    .io
                                    .zip(old.io)
                                    .and_then(|(a, b)| a.checked_sub(b))
                                    .map(|bytes| bytes as f64 / 1048576.0 / seconds);
                            }
                            next.insert(pid, reading);
                        }
                        let mut mem = PROCESS_MEMORY_COUNTERS_EX {
                            cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                            ..Default::default()
                        };
                        if GetProcessMemoryInfo(
                            h.0,
                            (&mut mem as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                            size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                        )
                        .is_ok()
                        {
                            p.ram_mb = Some(mem.WorkingSetSize as f64 / 1048576.0);
                            p.commit_mb = Some(mem.PrivateUsage as f64 / 1048576.);
                        }
                        let mut count = 0;
                        if GetProcessHandleCount(h.0, &mut count).is_ok() {
                            p.handles = Some(count as f64);
                        }
                    }
                    sample.processes.push(p);
                }
                valid = Process32NextW(snapshot.0, &mut entry).is_ok();
            }
            self.previous = next;
            sample.collection_ms = start.elapsed().as_secs_f64() * 1000.0;
            Ok(sample)
        }
    }
}

pub fn thread_report(pid: u32) -> Result<crate::model::Report, String> {
    unsafe {
        let snapshot =
            Owned(CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).map_err(|e| e.to_string())?);
        let mut entry = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut threads = Vec::new();
        let mut valid = Thread32First(snapshot.0, &mut entry).is_ok();
        while valid {
            if entry.th32OwnerProcessID == pid
                && let Ok(handle) =
                    OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, entry.th32ThreadID)
            {
                let h = Owned(handle);
                let (mut created, mut exit, mut kernel, mut user) = (
                    FILETIME::default(),
                    FILETIME::default(),
                    FILETIME::default(),
                    FILETIME::default(),
                );
                if GetThreadTimes(h.0, &mut created, &mut exit, &mut kernel, &mut user).is_ok() {
                    threads.push((
                        entry.th32ThreadID,
                        entry.tpBasePri,
                        h,
                        ticks(kernel),
                        ticks(user),
                    ));
                }
            }
            valid = Thread32Next(snapshot.0, &mut entry).is_ok();
        }
        let start = Instant::now();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let seconds = start.elapsed().as_secs_f64();
        let mut rows = Vec::new();
        for (tid, priority, h, old_kernel, old_user) in threads {
            let (mut created, mut exit, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            if GetThreadTimes(h.0, &mut created, &mut exit, &mut kernel, &mut user).is_ok() {
                let k = ticks(kernel).saturating_sub(old_kernel) as f64 / 1e7 / seconds * 100.0;
                let u = ticks(user).saturating_sub(old_user) as f64 / 1e7 / seconds * 100.0;
                rows.push((tid, priority, k, u, ticks(exit) != 0));
            }
        }
        rows.sort_by(|a, b| (b.2 + b.3).total_cmp(&(a.2 + a.3)));
        let mut report = crate::model::Report::new(
            format!("Threads · PID {pid}"),
            &[
                "Thread ID",
                "CPU %",
                "Kernel %",
                "User %",
                "Base priority",
                "State",
            ],
        );
        report.metric("Readable threads", rows.len());
        report.metric("Sample", format!("{seconds:.1} s"));
        report.metric("CPU scale", "1 logical core");
        report.metric("Displayed", rows.len().min(40));
        for (tid, priority, k, u, exited) in rows.iter().take(40) {
            report.row(&[
                &tid.to_string(),
                &format!("{:.2}", k + u),
                &format!("{k:.2}"),
                &format!("{u:.2}"),
                &priority.to_string(),
                if *exited { "Exited" } else { "Running" },
            ]);
        }
        Ok(report)
    }
}

/// Holds an opened process across a detail request, preventing PID reuse from changing its identity.
pub struct ProcessIdentity {
    handle: Owned,
    pub created_ticks: u64,
}
impl ProcessIdentity {
    pub fn image_path(&self) -> Result<String, String> {
        unsafe {
            let mut value = vec![0u16; 32768];
            let mut size = value.len() as u32;
            QueryFullProcessImageNameW(
                self.handle.0,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(value.as_mut_ptr()),
                &mut size,
            )
            .map_err(|e| e.to_string())?;
            Ok(String::from_utf16_lossy(&value[..size as usize]))
        }
    }
    pub fn open(pid: u32, expected: Option<u64>) -> Result<Self, String> {
        unsafe {
            let handle = Owned(
                OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
                    .or_else(|_| OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid))
                    .map_err(|e| format!("Cannot inspect PID {pid}: {e}"))?,
            );
            let (mut created, mut exit, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            GetProcessTimes(handle.0, &mut created, &mut exit, &mut kernel, &mut user)
                .map_err(|e| e.to_string())?;
            let created_ticks = ticks(created);
            if expected.is_some_and(|value| value != created_ticks) {
                return Err("That process has exited and its PID was reused. Select the new process from a fresh capture.".into());
            }
            Ok(Self {
                handle,
                created_ticks,
            })
        }
    }
    pub fn running(&self) -> bool {
        unsafe {
            let mut code = 0;
            GetExitCodeProcess(self.handle.0, &mut code).is_ok() && code == 259
        }
    }
}
#[derive(Default)]
struct ProcessStats {
    kernel: u64,
    user: u64,
    io: Option<IO_COUNTERS>,
    memory: Option<PROCESS_MEMORY_COUNTERS_EX>,
    handles: Option<u32>,
}
fn process_stats(h: HANDLE) -> Result<ProcessStats, String> {
    unsafe {
        let (mut created, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        GetProcessTimes(h, &mut created, &mut exit, &mut kernel, &mut user)
            .map_err(|e| e.to_string())?;
        let mut io = IO_COUNTERS::default();
        let io = GetProcessIoCounters(h, &mut io).is_ok().then_some(io);
        let mut memory = PROCESS_MEMORY_COUNTERS_EX {
            cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        let memory = GetProcessMemoryInfo(
            h,
            (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
            size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        )
        .is_ok()
        .then_some(memory);
        let mut count = 0;
        let handles = GetProcessHandleCount(h, &mut count)
            .is_ok()
            .then_some(count);
        Ok(ProcessStats {
            kernel: ticks(kernel),
            user: ticks(user),
            io,
            memory,
            handles,
        })
    }
}
pub fn process_report(pid: u32, expected: Option<u64>) -> Result<crate::model::Report, String> {
    unsafe {
        let identity = ProcessIdentity::open(pid, expected)?;
        let mut name = vec![0u16; 32768];
        let mut count = name.len() as u32;
        let image = if QueryFullProcessImageNameW(
            identity.handle.0,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(name.as_mut_ptr()),
            &mut count,
        )
        .is_ok()
        {
            String::from_utf16_lossy(&name[..count as usize])
        } else {
            "N/A - executable path could not be read".into()
        };
        let start = Instant::now();
        let first = process_stats(identity.handle.0)?;
        std::thread::sleep(std::time::Duration::from_secs(2));
        let last = process_stats(identity.handle.0)?;
        let duration = start.elapsed().as_secs_f64();
        let cpus = GetActiveProcessorCount(ALL_PROCESSOR_GROUPS).max(1) as f64;
        let kernel =
            last.kernel.saturating_sub(first.kernel) as f64 / 1e7 / duration / cpus * 100.0;
        let user = last.user.saturating_sub(first.user) as f64 / 1e7 / duration / cpus * 100.0;
        let created = identity.created_ticks.saturating_sub(116444736000000000) / 10000000;
        let snapshot =
            Owned(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?);
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut valid = Process32FirstW(snapshot.0, &mut entry).is_ok();
        let mut parent = None;
        let mut thread_count = None;
        while valid {
            if entry.th32ProcessID == pid {
                parent = Some(entry.th32ParentProcessID);
                thread_count = Some(entry.cntThreads);
                break;
            }
            valid = Process32NextW(snapshot.0, &mut entry).is_ok();
        }
        let priority = GetPriorityClass(identity.handle.0);
        let mut report = crate::model::Report::new(
            format!("Process · PID {pid}"),
            &["Category", "Metric", "Value", "Scope"],
        );
        let mib = |v: usize| format!("{:.1} MiB", v as f64 / 1048576.0);
        report.metric("CPU", format!("{:.2}%", (kernel + user).clamp(0.0, 100.0)));
        report.metric(
            "Memory",
            last.memory
                .map(|v| mib(v.WorkingSetSize))
                .unwrap_or_else(|| "N/A".into()),
        );
        report.metric(
            "Threads",
            thread_count
                .map(|v| v.to_string())
                .unwrap_or_else(|| "N/A".into()),
        );
        report.metric(
            "Handles",
            last.handles
                .map(|v| v.to_string())
                .unwrap_or_else(|| "N/A".into()),
        );
        report.row(&[
            "Identity",
            "PID",
            &pid.to_string(),
            if identity.running() {
                "Running"
            } else {
                "Exited"
            },
        ]);
        report.row(&["Identity", "Executable", &image, "Windows process image"]);
        report.row(&[
            "Identity",
            "Parent PID",
            &parent
                .map(|v| v.to_string())
                .unwrap_or_else(|| "N/A".into()),
            "Snapshot",
        ]);
        report.row(&[
            "Identity",
            "Created",
            &created.to_string(),
            "Unix UTC seconds",
        ]);
        report.row(&[
            "CPU",
            "Kernel",
            &format!("{kernel:.2}%"),
            "Whole-machine capacity",
        ]);
        report.row(&[
            "CPU",
            "User",
            &format!("{user:.2}%"),
            "Whole-machine capacity",
        ]);
        report.row(&[
            "CPU",
            "Kernel lifetime",
            &format!("{:.2} s", last.kernel as f64 / 1e7),
            "Lifetime",
        ]);
        report.row(&[
            "CPU",
            "User lifetime",
            &format!("{:.2} s", last.user as f64 / 1e7),
            "Lifetime",
        ]);
        report.row(&[
            "CPU",
            "Priority class",
            &format!("0x{priority:04x}"),
            "Windows class",
        ]);
        if let Some(memory) = last.memory {
            for (label, value) in [
                ("Working set", memory.WorkingSetSize),
                ("Peak working set", memory.PeakWorkingSetSize),
                ("Private commit", memory.PrivateUsage),
                ("Peak commit", memory.PeakPagefileUsage),
            ] {
                report.row(&["Memory", label, &mib(value), "Process"]);
            }
            report.row(&[
                "Memory",
                "Page faults",
                &memory.PageFaultCount.to_string(),
                "Soft + hard; lifetime",
            ]);
        }
        if let Some(io) = last.io {
            for (label, bytes, ops) in [
                ("Read / IN", io.ReadTransferCount, io.ReadOperationCount),
                ("Write / OUT", io.WriteTransferCount, io.WriteOperationCount),
                ("Other", io.OtherTransferCount, io.OtherOperationCount),
            ] {
                report.row(&[
                    "I/O",
                    label,
                    &format!("{bytes} bytes"),
                    &format!("{ops} operations; files + network + devices"),
                ]);
            }
            if let Some(old) = first.io {
                for (label, bytes) in [
                    (
                        "Read / IN rate",
                        io.ReadTransferCount.saturating_sub(old.ReadTransferCount),
                    ),
                    (
                        "Write / OUT rate",
                        io.WriteTransferCount.saturating_sub(old.WriteTransferCount),
                    ),
                ] {
                    report.row(&[
                        "I/O",
                        label,
                        &format!("{:.3} MiB/s", bytes as f64 / 1048576.0 / duration),
                        &format!("{duration:.1} s sample"),
                    ]);
                }
            }
        }
        match crate::inspect::services(pid, Some(identity.created_ticks)) {
            Ok(services) if services.rows.is_empty() => report.row(&[
                "Identity",
                "Windows services",
                "None associated",
                "Service Control Manager",
            ]),
            Ok(services) => {
                for service in services.rows {
                    report.row(&["Service", &service[0], &service[1], &service[3]]);
                }
            }
            Err(_) => report.row(&[
                "Identity",
                "Windows services",
                "Unavailable",
                "Service Control Manager query failed",
            ]),
        }
        Ok(report)
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn rejects_reused_process_identity() {
        let pid = std::process::id();
        let identity = ProcessIdentity::open(pid, None).unwrap();
        assert!(identity.running());
        assert!(ProcessIdentity::open(pid, Some(identity.created_ticks)).is_ok());
        assert!(ProcessIdentity::open(pid, Some(identity.created_ticks + 1)).is_err());
    }
    #[test]
    fn focused_report_reads_current_process() {
        let pid = std::process::id();
        let identity = ProcessIdentity::open(pid, None).unwrap();
        let stats = process_stats(identity.handle.0).unwrap();
        assert!(stats.memory.is_some());
        assert!(stats.io.is_some());
        assert!(stats.handles.is_some_and(|n| n > 0));
    }
}
