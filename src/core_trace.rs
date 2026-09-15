//! Explicit five-second scheduler observation; no affinity inference or idle sampling.
use crate::{metrics::wide, model::Report, native::NativeMonitor, traffic::Properties};
use std::{
    collections::{BTreeSet, HashMap},
    os::windows::process::CommandExt,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::*,
        System::{
            Com::CoCreateGuid,
            Diagnostics::{Etw::*, ToolHelp::*},
            Threading::*,
        },
    },
    core::{GUID, PCWSTR, PWSTR},
};

const THREAD: GUID = GUID::from_u128(0x3d6fa8d1_fe05_11d0_9dda_00c04fd7ba7c);
#[derive(Default)]
struct State {
    // Thread creation timestamp prevents attributing buffered events to reused IDs.
    threads: HashMap<u32, (u32, u64)>,
    observed: HashMap<u32, BTreeSet<u16>>,
    events: u64,
    unknown: u64,
}
impl State {
    fn decode(&mut self, opcode: u8, version: u8, timestamp: u64, cpu: u16, bytes: &[u8]) {
        if bytes.len() < 8 {
            self.unknown += 1;
            return;
        }
        let a = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        let b = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        // Microsoft TraceEvent's kernel parser keeps these leading IDs across
        // newer versions; CSwitch v2+ has at least 24 bytes. Thread v1+ starts
        // with ProcessId/TThreadId. Do not apply v0's different layout.
        if (opcode == 36 && (version < 2 || bytes.len() < 24))
            || (matches!(opcode, 1..=4) && version < 1)
        {
            if matches!(opcode, 1..=4) {
                self.threads.remove(&b);
            }
            self.unknown += 1;
            return;
        }
        match opcode {
            1 | 3 if self.threads.len() < 100_000 => {
                self.threads.insert(b, (a, timestamp));
            }
            2 | 4 => {
                self.threads.remove(&b);
            }
            36 => {
                self.events += 1;
                if let Some(&(pid, born)) = self.threads.get(&a) {
                    if timestamp >= born && pid != 0 && self.observed.len() < 10_000 {
                        self.observed.entry(pid).or_default().insert(cpu);
                    }
                } else {
                    self.unknown += 1;
                }
            }
            _ => {}
        }
    }
}
unsafe extern "system" fn event(record: *mut EVENT_RECORD) {
    let Some(r) = (unsafe { record.as_ref() }) else {
        return;
    };
    if r.EventHeader.ProviderId != THREAD || r.UserData.is_null() || r.UserContext.is_null() {
        return;
    }
    if !matches!(r.EventHeader.EventDescriptor.Opcode, 1..=4 | 36) {
        return;
    }
    let cpu = if r.EventHeader.Flags as u32 & EVENT_HEADER_FLAG_PROCESSOR_INDEX != 0 {
        unsafe { r.BufferContext.Anonymous.ProcessorIndex }
    } else {
        unsafe { r.BufferContext.Anonymous.Anonymous.ProcessorNumber as u16 }
    };
    let state = unsafe { &*(r.UserContext as *const Mutex<State>) };
    if let Ok(mut state) = state.lock() {
        state.decode(
            r.EventHeader.EventDescriptor.Opcode,
            r.EventHeader.EventDescriptor.Version,
            r.EventHeader.TimeStamp as u64,
            cpu,
            unsafe {
                std::slice::from_raw_parts(r.UserData.cast::<u8>(), r.UserDataLength as usize)
            },
        );
    }
}
pub fn collect() -> Result<Report, String> {
    let mut monitor = NativeMonitor::new()?;
    let initial = monitor.sample(0.)?;
    let state = Arc::new(Mutex::new(State::default()));
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).map_err(|e| e.to_string())?;
        let mut entry = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        if Thread32First(snap, &mut entry).is_ok() {
            loop {
                if let Ok(handle) =
                    OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, entry.th32ThreadID)
                {
                    let (mut born, mut exit, mut kernel, mut user) = (
                        FILETIME::default(),
                        FILETIME::default(),
                        FILETIME::default(),
                        FILETIME::default(),
                    );
                    if GetThreadTimes(handle, &mut born, &mut exit, &mut kernel, &mut user).is_ok()
                    {
                        let stamp =
                            ((born.dwHighDateTime as u64) << 32) | born.dwLowDateTime as u64;
                        state
                            .lock()
                            .unwrap()
                            .threads
                            .insert(entry.th32ThreadID, (GetProcessIdOfThread(handle), stamp));
                    }
                    let _ = CloseHandle(handle);
                }
                if Thread32Next(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    let guid = unsafe { CoCreateGuid() }.map_err(|e| e.to_string())?;
    // Same private namespace lets the existing strictly scoped watchdog clean up.
    let name = format!("SuperOpti.Network.{}.{guid:?}", std::process::id());
    let name_w = wide(&name);
    let mut properties = Properties::new(&name_w, guid);
    unsafe {
        (*properties.ptr()).EnableFlags = EVENT_TRACE_FLAG_CSWITCH | EVENT_TRACE_FLAG_THREAD;
        (*properties.ptr()).BufferSize = 256;
        (*properties.ptr()).MinimumBuffers = 64;
        (*properties.ptr()).MaximumBuffers = 256;
    }
    let mut session = CONTROLTRACE_HANDLE::default();
    let status = unsafe { StartTraceW(&mut session, PCWSTR(name_w.as_ptr()), properties.ptr()) };
    if status == ERROR_ACCESS_DENIED {
        return Err("Administrator permission required for scheduler tracing".into());
    }
    status
        .ok()
        .map_err(|e| format!("Scheduler trace unavailable: {e}"))?;
    let stop = |properties: &mut Properties| unsafe {
        ControlTraceW(
            session,
            PCWSTR::null(),
            properties.ptr(),
            EVENT_TRACE_CONTROL_STOP,
        )
    };
    let mut logfile = EVENT_TRACE_LOGFILEW {
        LoggerName: PWSTR(name_w.as_ptr().cast_mut()),
        Context: Arc::as_ptr(&state).cast_mut().cast(),
        ..Default::default()
    };
    logfile.Anonymous1.ProcessTraceMode =
        PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
    logfile.Anonymous2.EventRecordCallback = Some(event);
    let trace = unsafe { OpenTraceW(&mut logfile) };
    if trace.Value == u64::MAX {
        let _ = stop(&mut properties);
        return Err("Could not open scheduler trace".into());
    }
    let watchdog = std::env::current_exe()
        .map_err(|e| e.to_string())
        .and_then(|exe| {
            std::process::Command::new(exe)
                .args([
                    "--network-watchdog",
                    &std::process::id().to_string(),
                    &name,
                    "10",
                ])
                .creation_flags(0x08000000)
                .spawn()
                .map_err(|e| e.to_string())
        });
    let mut watchdog = match watchdog {
        Ok(p) => p,
        Err(e) => {
            let _ = stop(&mut properties);
            unsafe {
                let _ = CloseTrace(trace);
            }
            return Err(e);
        }
    };
    let trace_value = trace.Value;
    let keep = state.clone();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let consumer = thread::spawn(move || {
        let _keep = keep;
        let result =
            unsafe { ProcessTrace(&[PROCESSTRACE_HANDLE { Value: trace_value }], None, None) };
        let _ = done_tx.send(());
        result
    });
    thread::sleep(Duration::from_secs(5));
    let stopped = stop(&mut properties);
    let drained = done_rx.recv_timeout(Duration::from_secs(3)).is_ok();
    unsafe {
        let _ = CloseTrace(trace);
    }
    if !drained {
        return Err("Scheduler trace drain timed out; watchdog will clean up".into());
    }
    let consumed = consumer.join().map_err(|_| "Scheduler consumer failed")?;
    if stopped == ERROR_SUCCESS || stopped == ERROR_WMI_INSTANCE_NOT_FOUND {
        let _ = watchdog.kill();
        let _ = watchdog.wait();
    }
    stopped
        .ok()
        .map_err(|e| format!("Stopping scheduler session: {e}"))?;
    consumed
        .ok()
        .map_err(|e| format!("Reading scheduler session: {e}"))?;
    let lost = unsafe { (*properties.ptr()).EventsLost + (*properties.ptr()).RealTimeBuffersLost };
    // Lost lifecycle events make identity attribution unsafe, not just incomplete.
    if lost != 0 {
        return Err(format!(
            "Scheduler trace lost {lost} events/buffers; attribution discarded. Retry."
        ));
    }
    let final_sample = monitor.sample(5.)?;
    let state = state.lock().map_err(|_| "Scheduler state unavailable")?;
    if state.events == 0 {
        return Err(format!(
            "No supported context switches received ({0} unmapped events)",
            state.unknown
        ));
    }
    let mut report = Report::new(
        "Observed logical CPU indexes · 5-second trace",
        &["Process", "PID", "Created ticks", "Observed cores"],
    );
    report.metric("Window", "5 seconds");
    report.metric("Context switches", state.events.to_string());
    report.metric("Unmapped events", state.unknown.to_string());
    report.metric("Completed UTC", crate::traffic::utc(crate::metrics::now()));
    for p in initial.processes.iter().take(1000) {
        if p.created_ticks.is_none()
            || !final_sample
                .processes
                .iter()
                .any(|q| q.pid == p.pid && q.created_ticks == p.created_ticks)
        {
            continue;
        }
        let cores = state
            .observed
            .get(&p.pid)
            .map(ranges)
            .unwrap_or_else(|| "Not observed".into());
        report.row(&[
            &p.name,
            &p.pid.to_string(),
            &p.created_ticks.unwrap().to_string(),
            &cores,
        ]);
    }
    let mapped = report
        .rows
        .iter()
        .filter(|row| row[3] != "Not observed")
        .count();
    if mapped == 0 {
        return Err("Context switches arrived, but no process identities could be verified".into());
    }
    report.metric("Mapped processes", mapped.to_string());
    report.metric("Process limit", "1000 stable snapshot identities");
    Ok(report)
}
fn ranges(values: &BTreeSet<u16>) -> String {
    let mut parts = Vec::new();
    let mut it = values.iter().copied().peekable();
    while let Some(start) = it.next() {
        let mut end = start;
        while it.peek().is_some_and(|v| *v as u32 == end as u32 + 1) {
            end = it.next().unwrap();
        }
        parts.push(if start == end {
            start.to_string()
        } else {
            format!("{start}–{end}")
        });
    }
    parts.join(", ")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduler_attributes_incoming_thread_and_rejects_reused_identity() {
        let mut s = State::default();
        s.threads.insert(42, (12, 100));
        let mut bytes = [42u32.to_le_bytes(), 99u32.to_le_bytes()].concat();
        bytes.resize(24, 0);
        s.decode(36, 2, 99, 2, &bytes);
        assert!(s.observed.is_empty());
        s.decode(36, 4, 101, 257, &bytes);
        assert_eq!(s.observed[&12], BTreeSet::from([257]));
        s.decode(
            2,
            2,
            102,
            0,
            &[12u32.to_le_bytes(), 42u32.to_le_bytes()].concat(),
        );
        s.decode(36, 2, 103, 3, &bytes);
        assert_eq!(s.observed[&12].len(), 1);
        assert_eq!(ranges(&BTreeSet::from([0, 1, 2, 65, 257])), "0–2, 65, 257");
    }
}
