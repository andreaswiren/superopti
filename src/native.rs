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
                        let mut mem = PROCESS_MEMORY_COUNTERS {
                            cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                            ..Default::default()
                        };
                        if GetProcessMemoryInfo(
                            h.0,
                            &mut mem,
                            size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                        )
                        .is_ok()
                        {
                            p.ram_mb = Some(mem.WorkingSetSize as f64 / 1048576.0);
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

pub fn thread_report(pid: u32) -> Result<String, String> {
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
        let mut report = format!(
            "THREAD SNAPSHOT / PID {pid}\r\n2-second native sample. CPU is % of ONE logical processor. Top 40 readable threads.\r\nThreads created after snapshot are not included. Protected threads may be unavailable.\r\n\r\n       TID   CPU %   Kernel %   User %  Base priority  Exited\r\n"
        );
        if rows.is_empty() {
            report.push_str("No readable threads: process exited, PID was not found, or access was restricted.\r\n");
        }
        for (tid, priority, k, u, exited) in rows.iter().take(40) {
            report.push_str(&format!(
                "{tid:>10} {:>7.2} {k:>10.2} {u:>8.2} {priority:>14}  {exited}\r\n",
                k + u
            ));
        }
        Ok(report)
    }
}
