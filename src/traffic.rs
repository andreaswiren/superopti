//! Bounded, opt-in ETW network accounting. No payloads or DNS lookups.
use crate::{actions, metrics::wide, model::Report};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    os::windows::process::CommandExt,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::CoCreateGuid, Diagnostics::Etw::*, Threading::*, Time::*},
    },
    core::{GUID, PCWSTR, PWSTR},
};

const TCP: GUID = GUID::from_u128(0x9a280ac0_c8e0_11d1_84e2_00c04fb998a2);
const UDP: GUID = GUID::from_u128(0xbf3a50c5_a9c9_4988_a005_2df0b7c80f80);
const EPOCH: u64 = 11644473600;
const MAX_PENDING: usize = 20000;
const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_HISTORY: u64 = 256 * 1024 * 1024;
const PAGE: usize = 500;

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub search: String,
    pub from: Option<u64>,
    pub to: Option<u64>,
    pub protocol: usize,
    pub group: usize,
    pub page: usize,
}
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
struct Key {
    second: u64,
    pid: u32,
    protocol: String,
    local: IpAddr,
    local_port: u16,
    remote: IpAddr,
    remote_port: u16,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Count {
    first_tick: u64,
    last_tick: u64,
    received: u64,
    sent: u64,
    events: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Row {
    key: Key,
    count: Count,
    process: String,
    created: Option<u64>,
}
#[derive(Default)]
struct Pending {
    rows: HashMap<Key, Count>,
    dropped: u64,
    unsupported: u64,
    only_pid: Option<u32>,
}
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Coverage {
    schema: u32,
    complete: bool,
    events_lost: u32,
    buffers_lost: u32,
    dropped: u64,
    unsupported: u64,
    failure: Option<String>,
}

pub fn parse_time(text: &str) -> Result<Option<u64>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let s = text.trim().as_bytes();
    if s.len() != 20
        || s[4] != b'-'
        || s[7] != b'-'
        || s[10] != b'T'
        || s[13] != b':'
        || s[16] != b':'
        || s[19] != b'Z'
    {
        return Err("Use UTC YYYY-MM-DDTHH:MM:SSZ, or leave the time blank.".into());
    }
    let part = |a: usize, b: usize| {
        std::str::from_utf8(&s[a..b])
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .ok_or("Invalid UTC date")
    };
    let system = SYSTEMTIME {
        wYear: part(0, 4)?,
        wMonth: part(5, 7)?,
        wDay: part(8, 10)?,
        wHour: part(11, 13)?,
        wMinute: part(14, 16)?,
        wSecond: part(17, 19)?,
        ..Default::default()
    };
    let mut file = FILETIME::default();
    unsafe {
        SystemTimeToFileTime(&system, &mut file).map_err(|_| "Invalid calendar date")?;
    }
    let value = ((file.dwHighDateTime as u64) << 32) | file.dwLowDateTime as u64;
    Ok(Some(
        (value / 10_000_000)
            .checked_sub(EPOCH)
            .ok_or("Use dates from 1970 onward")?,
    ))
}
pub fn utc(second: u64) -> String {
    let value = second.saturating_add(EPOCH).saturating_mul(10_000_000);
    let file = FILETIME {
        dwLowDateTime: value as u32,
        dwHighDateTime: (value >> 32) as u32,
    };
    let mut s = SYSTEMTIME::default();
    if unsafe { FileTimeToSystemTime(&file, &mut s) }.is_err() {
        return "Unknown".into();
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        s.wYear, s.wMonth, s.wDay, s.wHour, s.wMinute, s.wSecond
    )
}
fn decode(
    provider: GUID,
    version: u8,
    opcode: u8,
    time: u64,
    data: &[u8],
) -> Option<(Key, u64, bool)> {
    if version != 2 || ![10, 11, 26, 27].contains(&opcode) || (provider != TCP && provider != UDP) {
        return None;
    }
    let ipv6 = opcode >= 26;
    if data.len() < if ipv6 { 44 } else { 20 } {
        return None;
    }
    let pid = u32::from_le_bytes(data[0..4].try_into().ok()?);
    let size = u32::from_le_bytes(data[4..8].try_into().ok()?) as u64;
    let (dest, source, port) = if ipv6 {
        (
            IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(&data[8..24]).ok()?)),
            IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(&data[24..40]).ok()?)),
            40,
        )
    } else {
        (
            IpAddr::V4(Ipv4Addr::new(data[8], data[9], data[10], data[11])),
            IpAddr::V4(Ipv4Addr::new(data[12], data[13], data[14], data[15])),
            16,
        )
    };
    let dp = u16::from_be_bytes([data[port], data[port + 1]]);
    let sp = u16::from_be_bytes([data[port + 2], data[port + 3]]);
    let incoming = opcode == 11 || opcode == 27;
    let (local, local_port, remote, remote_port) = if incoming {
        (dest, dp, source, sp)
    } else {
        (source, sp, dest, dp)
    };
    Some((
        Key {
            second: (time / 10_000_000).checked_sub(EPOCH)?,
            pid,
            protocol: if provider == TCP { "TCP" } else { "UDP" }.into(),
            local,
            local_port,
            remote,
            remote_port,
        },
        size,
        incoming,
    ))
}
unsafe extern "system" fn event(record: *mut EVENT_RECORD) {
    if record.is_null() {
        return;
    }
    let r = &*record;
    if r.UserContext.is_null() || r.UserData.is_null() {
        return;
    }
    let h = &r.EventHeader;
    if (h.ProviderId != TCP && h.ProviderId != UDP)
        || ![10, 11, 26, 27].contains(&h.EventDescriptor.Opcode)
    {
        return;
    }
    let context = &*(r.UserContext as *const Mutex<Pending>);
    let Ok(mut p) = context.lock() else {
        return;
    };
    let payload = std::slice::from_raw_parts(r.UserData as *const u8, r.UserDataLength as usize);
    if let Some((key, size, incoming)) = decode(
        h.ProviderId,
        h.EventDescriptor.Version,
        h.EventDescriptor.Opcode,
        h.TimeStamp as u64,
        payload,
    ) {
        if p.only_pid.is_some_and(|pid| pid != key.pid) {
            return;
        }
        if p.rows.len() >= MAX_PENDING && !p.rows.contains_key(&key) {
            p.dropped += 1;
            return;
        }
        let count = p.rows.entry(key).or_default();
        count.events += 1;
        let tick = h.TimeStamp as u64;
        if count.first_tick == 0 || tick < count.first_tick {
            count.first_tick = tick;
        }
        count.last_tick = count.last_tick.max(tick);
        if incoming {
            count.received += size;
        } else {
            count.sent += size;
        }
    } else {
        p.unsupported += 1;
    }
}
struct Properties(Vec<u64>);
impl Properties {
    fn new(name: &[u16], guid: GUID) -> Self {
        let bytes = size_of::<EVENT_TRACE_PROPERTIES>() + name.len() * 2;
        let mut p = Self(vec![0; bytes.div_ceil(8)]);
        unsafe {
            let v = &mut *p.ptr();
            v.Wnode.BufferSize = bytes as u32;
            v.Wnode.Guid = guid;
            v.Wnode.ClientContext = 2;
            v.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
            v.BufferSize = 64;
            v.MinimumBuffers = 4;
            v.MaximumBuffers = 64;
            v.FlushTimer = 1;
            v.LogFileMode = EVENT_TRACE_REAL_TIME_MODE
                | EVENT_TRACE_SYSTEM_LOGGER_MODE
                | EVENT_TRACE_NO_PER_PROCESSOR_BUFFERING;
            v.EnableFlags = EVENT_TRACE_FLAG_NETWORK_TCPIP;
            v.LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                (p.ptr() as *mut u8)
                    .add(size_of::<EVENT_TRACE_PROPERTIES>())
                    .cast(),
                name.len(),
            );
        }
        p
    }
    fn ptr(&mut self) -> *mut EVENT_TRACE_PROPERTIES {
        self.0.as_mut_ptr().cast()
    }
}
pub struct Recorder {
    stop: mpsc::Sender<()>,
    handle: Option<thread::JoinHandle<()>>,
}
impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
pub(crate) fn dir() -> PathBuf {
    actions::data_dir().join("network-history")
}
pub(crate) fn files() -> Result<Vec<PathBuf>, String> {
    let root = dir();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut list = Vec::new();
    let mut total = 0u64;
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().extension().is_some_and(|v| v == "jsonl") {
            let meta = entry.metadata().map_err(|e| e.to_string())?;
            if meta.len() > MAX_FILE {
                return Err(
                    "History file exceeds 64 MiB safety limit. Archive it before querying.".into(),
                );
            }
            total = total.saturating_add(meta.len());
            list.push(entry.path());
        }
    }
    if total > MAX_HISTORY || list.len() > 1000 {
        return Err("History limit reached (256 MiB / 1000 captures). Archive old local captures before continuing.".into());
    }
    list.sort();
    Ok(list)
}
fn identity(pid: u32, first_tick: u64) -> (String, Option<u64>) {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return ("Unknown / exited".into(), None);
        };
        let mut created = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let times = GetProcessTimes(h, &mut created, &mut exit, &mut kernel, &mut user);
        let stamp = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
        let mut name = [0u16; 32768];
        let mut len = name.len() as u32;
        let queried =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(name.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        // A process born after this bucket cannot own it: never attribute a recycled PID.
        if times.is_err() || stamp > first_tick {
            return ("Unknown / exited".into(), None);
        }
        let label = if queried.is_ok() {
            String::from_utf16_lossy(&name[..len as usize])
                .rsplit('\\')
                .next()
                .unwrap_or("Unknown")
                .to_string()
        } else {
            "Protected process".into()
        };
        (label, Some(stamp))
    }
}
pub fn start(
    seconds: u64,
    notify: impl Fn(bool, Report) + Send + Sync + 'static,
) -> Result<Recorder, String> {
    let previous = files()?;
    let total: u64 = previous
        .iter()
        .filter_map(|p| p.metadata().ok())
        .map(|m| m.len())
        .sum();
    if total + MAX_FILE > MAX_HISTORY || previous.len() >= 1000 {
        return Err("Archive old captures to reserve 64 MiB for recording".into());
    }
    let notify = Arc::new(notify);
    let (stop, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ended = done.clone();
        let events = notify.clone();
        match start_inner(seconds, None, None, move |active, r| {
            if !active {
                ended.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            events(active, r);
        }) {
            Ok(recording) => {
                notify(true, live_report(0, 0, 0, 0, seconds, &VecDeque::new()));
                while !done.load(std::sync::atomic::Ordering::Relaxed) {
                    if rx.recv_timeout(Duration::from_millis(100)).is_ok() {
                        break;
                    }
                }
                drop(recording);
            }
            Err(e) if e.starts_with("Administrator permission required") => {
                if let Err(e) =
                    crate::elevation::record(seconds, &rx, |active, r| notify(active, r))
                {
                    notify(false, Report::error(e));
                }
            }
            Err(e) => notify(false, Report::error(e)),
        }
    });
    Ok(Recorder {
        stop,
        handle: Some(handle),
    })
}
pub(crate) fn start_inner(
    seconds: u64,
    only_pid: Option<u32>,
    remote: Option<Arc<Mutex<std::fs::File>>>,
    notify: impl Fn(bool, Report) + Send + 'static,
) -> Result<Recorder, String> {
    let previous = if remote.is_none() {
        files()?
    } else {
        Vec::new()
    };
    let total: u64 = previous
        .iter()
        .filter_map(|p| p.metadata().ok())
        .map(|m| m.len())
        .sum();
    if total + MAX_FILE > MAX_HISTORY || previous.len() >= 1000 {
        return Err(
            "Reserve 64 MiB for a capture: archive old network-history files first.".into(),
        );
    }
    let guid = unsafe { CoCreateGuid() }.map_err(|e| e.to_string())?;
    let name = format!("SuperOpti.Network.{}.{guid:?}", std::process::id());
    let name_w = wide(&name);
    let mut props = Properties::new(&name_w, guid);
    let mut control = CONTROLTRACE_HANDLE::default();
    let status = unsafe { StartTraceW(&mut control, PCWSTR(name_w.as_ptr()), props.ptr()) };
    if status == ERROR_ACCESS_DENIED {
        return Err("Administrator permission required for ETW network recording".into());
    }
    status
        .ok()
        .map_err(|e| format!("ETW recording unavailable: {e}"))?;
    let pending = Arc::new(Mutex::new(Pending {
        only_pid,
        ..Default::default()
    }));
    let mut logfile = EVENT_TRACE_LOGFILEW {
        LoggerName: PWSTR(name_w.as_ptr() as *mut _),
        Context: Arc::as_ptr(&pending) as *mut _,
        ..Default::default()
    };
    logfile.Anonymous1.ProcessTraceMode =
        PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
    logfile.Anonymous2.EventRecordCallback = Some(event);
    let trace = unsafe { OpenTraceW(&mut logfile) };
    if trace.Value == u64::MAX {
        unsafe {
            let _ = ControlTraceW(
                control,
                PCWSTR::null(),
                props.ptr(),
                EVENT_TRACE_CONTROL_STOP,
            );
        }
        return Err(format!("OpenTrace failed: {:?}", unsafe { GetLastError() }));
    }
    // Independent deadline helper also cleans up if the UI crashes or is force-terminated.
    let watchdog = std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args([
            "--network-watchdog",
            &std::process::id().to_string(),
            &name,
            &seconds.clamp(1, 900).to_string(),
        ])
        .creation_flags(0x08000000)
        .spawn();
    let mut watchdog = match watchdog {
        Ok(child) => child,
        Err(e) => {
            unsafe {
                let _ = ControlTraceW(
                    control,
                    PCWSTR::null(),
                    props.ptr(),
                    EVENT_TRACE_CONTROL_STOP,
                );
                let _ = CloseTrace(trace);
            }
            return Err(format!("Cannot start network cleanup watchdog: {e}"));
        }
    };
    let trace_value = trace.Value;
    let keep = pending.clone();
    let (finished_tx, finished_rx) = mpsc::channel();
    let consumer = thread::spawn(move || {
        let _keep = keep;
        let result =
            unsafe { ProcessTrace(&[PROCESSTRACE_HANDLE { Value: trace_value }], None, None) };
        let _ = finished_tx.send(());
        result
    });
    let (stop, rx) = mpsc::channel();
    let path = if remote.is_none() {
        dir().join(format!("traffic-{}-{guid:?}.jsonl", crate::metrics::now()))
    } else {
        PathBuf::from("User local network history")
    };
    let handle = thread::spawn(move || {
        let begin = Instant::now();
        let mut coverage = Coverage {
            schema: 1,
            ..Default::default()
        };
        let mut size = 0u64;
        let opened: std::io::Result<Box<dyn Write + Send>> = if let Some(pipe) = remote.as_ref() {
            Ok(Box::new(crate::elevation::PipeWriter(pipe.clone())))
        } else {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map(|f| Box::new(f) as Box<dyn Write + Send>)
        };
        let mut file = match opened {
            Ok(f) => Some(f),
            Err(e) => {
                coverage.failure = Some(e.to_string());
                None
            }
        };
        let mut received = 0u64;
        let mut sent = 0u64;
        let mut records = 0u64;
        let mut live = VecDeque::new();
        while let Some(current_file) = file.as_mut() {
            if coverage.failure.is_some() {
                break;
            }
            let ending = rx.recv_timeout(Duration::from_secs(2)).is_ok()
                || begin.elapsed() >= Duration::from_secs(seconds.clamp(1, 900));
            if ending {
                break;
            }
            if let Err(e) = flush(
                &pending,
                current_file,
                &mut size,
                &mut received,
                &mut sent,
                &mut records,
                &mut live,
            ) {
                coverage.failure = Some(e);
                break;
            }
            notify(
                true,
                live_report(
                    received,
                    sent,
                    records,
                    begin.elapsed().as_secs(),
                    seconds,
                    &live,
                ),
            );
        }
        let stopped = unsafe {
            ControlTraceW(
                control,
                PCWSTR::null(),
                props.ptr(),
                EVENT_TRACE_CONTROL_STOP,
            )
        };
        if finished_rx.recv_timeout(Duration::from_secs(3)).is_err() {
            unsafe {
                let _ = CloseTrace(PROCESSTRACE_HANDLE { Value: trace_value });
            }
            coverage.failure = Some(
                "ETW consumer drain exceeded three seconds; final events may be missing".into(),
            );
        }
        let consumed = consumer.join();
        unsafe {
            let _ = CloseTrace(PROCESSTRACE_HANDLE { Value: trace_value });
        }
        // The watchdog is killed only after our own session has stopped.
        if stopped == ERROR_SUCCESS || stopped == ERROR_WMI_INSTANCE_NOT_FOUND {
            let _ = watchdog.kill();
            let _ = watchdog.wait();
        } else {
            coverage.failure = Some(format!(
                "ETW stop status {}. Deadline watchdog remains active.",
                stopped.0
            ));
        }
        if let Some(f) = file.as_mut()
            && let Err(e) = flush(
                &pending,
                f,
                &mut size,
                &mut received,
                &mut sent,
                &mut records,
                &mut live,
            )
        {
            coverage.failure = Some(e);
        }
        let p = pending.lock().unwrap();
        coverage.dropped = p.dropped;
        coverage.unsupported = p.unsupported;
        unsafe {
            coverage.events_lost = (*props.ptr()).EventsLost;
            coverage.buffers_lost =
                (*props.ptr()).RealTimeBuffersLost + (*props.ptr()).LogBuffersLost;
        }
        if let Ok(code) = consumed {
            if code != ERROR_SUCCESS && code != ERROR_CANCELLED {
                coverage.failure = Some(format!("ETW consumer status {}", code.0));
            }
        } else {
            coverage.failure = Some("ETW consumer failed".into());
        }
        coverage.complete = coverage.failure.is_none()
            && coverage.events_lost == 0
            && coverage.buffers_lost == 0
            && coverage.dropped == 0
            && coverage.unsupported == 0;
        let metadata_result = if let Some(pipe) = remote.as_ref() {
            crate::elevation::send(pipe, &serde_json::json!({"coverage":coverage}))
        } else {
            fs::write(
                path.with_extension("meta.json"),
                serde_json::to_vec(&coverage).unwrap_or_default(),
            )
        };
        if let Err(e) = metadata_result {
            coverage.failure = Some(e.to_string());
            coverage.complete = false;
        }
        let mut report = live_report(
            received,
            sent,
            records,
            begin.elapsed().as_secs(),
            seconds,
            &live,
        );
        report.title = if coverage.complete {
            "Capture saved · latest 500 buckets · load history for all rows"
        } else {
            "Capture incomplete · inspect coverage before interpreting totals"
        }
        .into();
        report.metrics.pop();
        report.metric(
            "Coverage",
            if coverage.complete {
                "No reported loss".into()
            } else {
                format!(
                    "{} events / {} buffers / {} drops / {} unsupported",
                    coverage.events_lost,
                    coverage.buffers_lost,
                    coverage.dropped,
                    coverage.unsupported
                )
            },
        );
        report.debug = serde_json::to_string_pretty(&coverage).unwrap_or_default();
        if let Some(error) = &coverage.failure {
            report.metric("Capture error", error);
        }
        notify(false, report);
    });
    Ok(Recorder {
        stop,
        handle: Some(handle),
    })
}
fn live_report(
    received: u64,
    sent: u64,
    records: u64,
    elapsed: u64,
    seconds: u64,
    live: &VecDeque<Row>,
) -> Report {
    let mut r = Report::new(
        "Live traffic · latest 500 one-second flow buckets · updates every 2 s",
        &[
            "Process",
            "PID",
            "Protocol",
            "Local endpoint",
            "Remote endpoint",
            "Received bytes",
            "Sent bytes",
            "Events",
            "First UTC",
            "Last UTC",
        ],
    );
    r.metric("Received", bytes(received));
    r.metric("Sent", bytes(sent));
    r.metric("Flow buckets", records);
    r.metric(
        "Remaining",
        format!("{} s", seconds.saturating_sub(elapsed)),
    );
    populate_live(&mut r, live);
    r
}
pub(crate) fn populate_live(report: &mut Report, live: &VecDeque<Row>) {
    if report.columns.first().map(String::as_str) != Some("Process") {
        return;
    }
    report.rows.clear();
    for row in live.iter().rev() {
        report.row(&[
            &row.process,
            &row.key.pid.to_string(),
            &row.key.protocol,
            &endpoint(row.key.local, row.key.local_port),
            &endpoint(row.key.remote, row.key.remote_port),
            &row.count.received.to_string(),
            &row.count.sent.to_string(),
            &row.count.events.to_string(),
            &utc(row.key.second),
            &utc(row.key.second),
        ]);
    }
}
fn flush(
    pending: &Mutex<Pending>,
    file: &mut (impl Write + ?Sized),
    size: &mut u64,
    received: &mut u64,
    sent: &mut u64,
    records: &mut u64,
    live: &mut VecDeque<Row>,
) -> Result<(), String> {
    let rows = std::mem::take(
        &mut pending
            .lock()
            .map_err(|_| "Traffic buffer unavailable")?
            .rows,
    );
    let mut names = HashMap::new();
    let mut rows: Vec<_> = rows.into_iter().collect();
    rows.sort_by_key(|(key, count)| (key.second, count.last_tick, key.pid));
    for (key, count) in rows {
        let (process, created) = names
            .entry((key.pid, count.first_tick))
            .or_insert_with(|| identity(key.pid, count.first_tick))
            .clone();
        let row = Row {
            key,
            count,
            process,
            created,
        };
        let line = serde_json::to_vec(&row).map_err(|e| e.to_string())?;
        if *size + line.len() as u64 + 1 > MAX_FILE {
            return Err("64 MiB capture limit reached; remaining events were not retained".into());
        }
        file.write_all(&line)
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|e| e.to_string())?;
        *size += line.len() as u64 + 1;
        *received += row.count.received;
        *sent += row.count.sent;
        *records += 1;
        live.push_back(row);
        if live.len() > PAGE {
            live.pop_front();
        }
    }
    file.flush().map_err(|e| e.to_string())
}
pub fn watchdog(pid: u32, name: &str, seconds: u64) {
    if !name.starts_with(&format!("SuperOpti.Network.{pid}.")) || name.len() > 140 {
        return;
    }
    unsafe {
        if let Ok(parent) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            let _ = WaitForSingleObject(parent, ((seconds.clamp(1, 900) + 5) * 1000) as u32);
            let _ = CloseHandle(parent);
        }
        let wide = wide(name);
        let mut props = Properties::new(&wide, GUID::zeroed());
        let _ = ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            PCWSTR(wide.as_ptr()),
            props.ptr(),
            EVENT_TRACE_CONTROL_STOP,
        );
    }
}
fn bytes(n: u64) -> String {
    format!("{:.2} MiB", n as f64 / 1048576.)
}
fn endpoint(ip: IpAddr, port: u16) -> String {
    match ip {
        IpAddr::V4(_) => format!("{ip}:{port}"),
        IpAddr::V6(_) => format!("[{ip}]:{port}"),
    }
}
fn matches(row: &Row, f: &Filter) -> bool {
    if f.from.is_some_and(|t| row.key.second < t) || f.to.is_some_and(|t| row.key.second > t) {
        return false;
    }
    if (f.protocol == 1 && row.key.protocol != "TCP")
        || (f.protocol == 2 && row.key.protocol != "UDP")
    {
        return false;
    }
    let query = f.search.trim().to_lowercase();
    query.is_empty()
        || format!(
            "{} {} {} {} {} {} {}",
            row.process,
            row.key.pid,
            row.key.protocol,
            row.key.local,
            row.key.local_port,
            row.key.remote,
            row.key.remote_port
        )
        .to_lowercase()
        .contains(&query)
}
pub fn history(filter: &Filter) -> Result<Report, String> {
    if filter.from.zip(filter.to).is_some_and(|(a, b)| a > b) {
        return Err("From must be earlier than To".into());
    }
    let mut grouped: HashMap<String, (Row, u64)> = HashMap::new();
    let mut incomplete = 0;
    let mut malformed = 0u64;
    for path in files()? {
        let coverage = fs::read(path.with_extension("meta.json"))
            .ok()
            .and_then(|s| serde_json::from_slice::<Coverage>(&s).ok());
        if !coverage.is_some_and(|c| c.complete) {
            incomplete += 1;
        }
        let reader = BufReader::new(std::fs::File::open(path).map_err(|e| e.to_string())?);
        for line in reader.split(b'\n') {
            let line = line.map_err(|e| e.to_string())?;
            if line.is_empty() {
                continue;
            }
            let Ok(row) = serde_json::from_slice::<Row>(&line) else {
                malformed += 1;
                continue;
            };
            if !matches(&row, filter) {
                continue;
            }
            let k = &row.key;
            let group = match filter.group {
                1 => format!("{}:{:?}:{}", k.pid, row.created, row.process),
                2 => format!("{}:{}:{}", k.protocol, k.remote, k.remote_port),
                _ => format!(
                    "{}:{:?}:{}:{}:{}:{}:{}",
                    k.pid, row.created, k.protocol, k.local, k.local_port, k.remote, k.remote_port
                ),
            };
            if let Some((existing, last)) = grouped.get_mut(&group) {
                existing.count.received += row.count.received;
                existing.count.sent += row.count.sent;
                existing.count.events += row.count.events;
                *last = (*last).max(k.second);
                existing.key.second = existing.key.second.min(k.second);
            } else {
                if grouped.len() >= 100000 {
                    return Err(
                        "Query exceeds 100,000 groups. Narrow the time range or search.".into(),
                    );
                }
                let at = k.second;
                grouped.insert(group, (row, at));
            }
        }
    }
    let mut rows: Vec<_> = grouped.into_values().collect();
    rows.sort_by_key(|(r, _)| std::cmp::Reverse(r.count.received + r.count.sent));
    let received: u64 = rows.iter().map(|(r, _)| r.count.received).sum();
    let sent: u64 = rows.iter().map(|(r, _)| r.count.sent).sum();
    let pages = rows.len().max(1).div_ceil(PAGE);
    let page = filter.page.min(pages - 1);
    let mut report = Report::new(
        format!(
            "Traffic history · page {} / {pages} · {incomplete} archive capture(s) incomplete or active · {malformed} unreadable rows",
            page + 1
        ),
        &[
            "Process",
            "PID",
            "Protocol",
            "Local endpoint",
            "Remote endpoint",
            "Received bytes",
            "Sent bytes",
            "Events",
            "First UTC",
            "Last UTC",
        ],
    );
    report.metric("Received", bytes(received));
    report.metric("Sent", bytes(sent));
    report.metric("Groups", rows.len());
    report.metric("Page", format!("{} / {pages}", page + 1));
    for (r, last) in rows.into_iter().skip(page * PAGE).take(PAGE) {
        let process = if filter.group == 2 {
            "All processes"
        } else {
            &r.process
        };
        let pid = if filter.group == 2 {
            "All".into()
        } else {
            r.key.pid.to_string()
        };
        let protocol = if filter.group == 1 {
            match filter.protocol {
                1 => "TCP",
                2 => "UDP",
                _ => "Combined",
            }
        } else {
            &r.key.protocol
        };
        let local = if filter.group != 0 {
            "Multiple".into()
        } else {
            endpoint(r.key.local, r.key.local_port)
        };
        let remote = if filter.group == 1 {
            "Multiple".into()
        } else {
            endpoint(r.key.remote, r.key.remote_port)
        };
        report.row(&[
            process,
            &pid,
            protocol,
            &local,
            &remote,
            &r.count.received.to_string(),
            &r.count.sent.to_string(),
            &r.count.events.to_string(),
            &utc(r.key.second),
            &utc(last),
        ]);
    }
    Ok(report)
}

pub fn smoke_test() -> Result<serde_json::Value, String> {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream, UdpSocket};
    let before = files()?;
    let (tx, rx) = mpsc::channel();
    let recording = start_inner(12, Some(std::process::id()), None, move |active, report| {
        if !active {
            let _ = tx.send(report);
        }
    })?;
    thread::sleep(Duration::from_secs(1));
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let server = thread::spawn(move || -> Result<(), String> {
        let (mut socket, _) = listener.accept().map_err(|e| e.to_string())?;
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;
        let mut bytes = [0u8; 8192];
        socket.read_exact(&mut bytes).map_err(|e| e.to_string())?;
        socket.write_all(&bytes).map_err(|e| e.to_string())
    });
    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).map_err(|e| e.to_string())?;
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    client.write_all(&[7; 8192]).map_err(|e| e.to_string())?;
    let mut reply = [0u8; 8192];
    client.read_exact(&mut reply).map_err(|e| e.to_string())?;
    server.join().map_err(|_| "TCP server failed")??;
    let a = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let b = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    b.set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    let udp_port = b.local_addr().map_err(|e| e.to_string())?.port();
    let mut datagram = [0u8; 1024];
    for _ in 0..10 {
        a.send_to(&[9; 1024], b.local_addr().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        b.recv_from(&mut datagram).map_err(|e| e.to_string())?;
    }
    thread::sleep(Duration::from_secs(3));
    drop(recording);
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let new: Vec<_> = files()?
        .into_iter()
        .filter(|p| !before.contains(p))
        .collect();
    let mut tcp = Count::default();
    let mut udp = Count::default();
    for path in new {
        let coverage: Coverage = serde_json::from_slice(
            &fs::read(path.with_extension("meta.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if !coverage.complete {
            return Err(format!(
                "Controlled capture incomplete: {}",
                serde_json::to_string(&coverage).unwrap()
            ));
        }
        for line in BufReader::new(std::fs::File::open(path).map_err(|e| e.to_string())?).lines() {
            let r: Row = serde_json::from_str(&line.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if r.key.pid != std::process::id() {
                return Err("Process filter leaked unrelated traffic".into());
            }
            let c = if r.key.protocol == "TCP"
                && (r.key.local_port == port || r.key.remote_port == port)
            {
                &mut tcp
            } else if r.key.protocol == "UDP"
                && (r.key.local_port == udp_port || r.key.remote_port == udp_port)
            {
                &mut udp
            } else {
                continue;
            };
            c.received += r.count.received;
            c.sent += r.count.sent;
        }
    }
    if tcp.received < 16384 || tcp.sent < 16384 || udp.received < 10240 || udp.sent < 10240 {
        return Err(format!(
            "Missing controlled ETW bytes: TCP in/out {}/{}; UDP in/out {}/{}",
            tcp.received, tcp.sent, udp.received, udp.sent
        ));
    }
    Ok(
        serde_json::json!({"tcp_received":tcp.received,"tcp_sent":tcp.sent,"udp_received":udp.received,"udp_sent":udp.sent,"only_test_process":true,"reported_loss":0,"manual_stop":true}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_window_is_bounded_without_losing_archived_bytes() {
        let mut pending = Pending::default();
        for second in 1..=620 {
            pending.rows.insert(
                Key {
                    second,
                    pid: 0,
                    protocol: "UDP".into(),
                    local: Ipv6Addr::LOCALHOST.into(),
                    local_port: 12000,
                    remote: Ipv6Addr::LOCALHOST.into(),
                    remote_port: 53,
                },
                Count {
                    received: 12,
                    sent: 34,
                    events: 2,
                    ..Default::default()
                },
            );
        }
        let mut archive = Vec::new();
        let (mut size, mut received, mut sent, mut records) = (0, 0, 0, 0);
        let mut live = VecDeque::new();
        flush(
            &Mutex::new(pending),
            &mut archive,
            &mut size,
            &mut received,
            &mut sent,
            &mut records,
            &mut live,
        )
        .unwrap();
        assert_eq!((records, received, sent), (620, 620 * 12, 620 * 34));
        assert_eq!(archive.iter().filter(|b| **b == b'\n').count(), 620);
        assert_eq!(live.len(), 500);
        let report = live_report(received, sent, records, 2, 120, &live);
        assert_eq!(report.rows[0][8], utc(620));
        assert_eq!(report.rows[499][8], utc(121));
        assert_eq!(report.rows[0][3], "[::1]:12000");
        assert!(report.rows.iter().all(|r| r.len() == report.columns.len()));
        let mut parent_report = report.clone();
        parent_report.rows.clear();
        populate_live(&mut parent_report, &live);
        assert_eq!(parent_report.rows, report.rows);
    }
    #[test]
    fn utc_round_trip() {
        for text in [
            "1970-01-01T00:00:00Z",
            "2024-02-29T23:59:59Z",
            "2026-09-14T12:30:01Z",
        ] {
            assert_eq!(utc(parse_time(text).unwrap().unwrap()), text);
        }
        assert!(parse_time("2025-02-29T00:00:00Z").is_err());
        assert!(parse_time("2026-01-01").is_err());
    }
    #[test]
    fn decode_direction_and_ports() {
        let mut p = vec![0u8; 20];
        p[0..4].copy_from_slice(&42u32.to_le_bytes());
        p[4..8].copy_from_slice(&1000u32.to_le_bytes());
        p[8..12].copy_from_slice(&[8, 8, 8, 8]);
        p[12..16].copy_from_slice(&[10, 0, 0, 1]);
        p[16..18].copy_from_slice(&53u16.to_be_bytes());
        p[18..20].copy_from_slice(&50123u16.to_be_bytes());
        let time = (EPOCH + 100) * 10_000_000;
        let (out, n, incoming) = decode(UDP, 2, 10, time, &p).unwrap();
        assert_eq!(n, 1000);
        assert!(!incoming);
        assert_eq!(out.local_port, 50123);
        assert_eq!(out.remote_port, 53);
        assert_eq!(out.pid, 42);
        let (recv, _, incoming) = decode(TCP, 2, 11, time, &p).unwrap();
        assert!(incoming);
        assert_eq!(recv.local_port, 53);
        assert!(decode(UDP, 0, 10, time, &p).is_none());
        assert!(decode(UDP, 2, 10, time, &p[..19]).is_none());
        assert!(decode(TCP, 2, 14, time, &p).is_none());
    }
    #[test]
    fn ipv6_and_filters() {
        let mut p = vec![0u8; 44];
        p[8..24].copy_from_slice(&Ipv6Addr::LOCALHOST.octets());
        p[24..40].copy_from_slice(&Ipv6Addr::LOCALHOST.octets());
        p[40..42].copy_from_slice(&443u16.to_be_bytes());
        let (key, _, incoming) = decode(TCP, 2, 27, (EPOCH + 100) * 10_000_000, &p).unwrap();
        assert!(incoming);
        assert_eq!(key.local_port, 443);
        let r = Row {
            key,
            count: Count::default(),
            process: "Browser.exe".into(),
            created: None,
        };
        assert!(matches(
            &r,
            &Filter {
                search: "browser".into(),
                from: Some(100),
                to: Some(100),
                ..Default::default()
            }
        ));
        assert!(!matches(
            &r,
            &Filter {
                protocol: 2,
                ..Default::default()
            }
        ));
        assert!(!matches(
            &r,
            &Filter {
                from: Some(101),
                ..Default::default()
            }
        ));
    }
}
