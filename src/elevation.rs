//! UAC-limited ETW helper. The elevated process accepts no file paths or arbitrary commands.
//! Aggregates cross a local, ACL-restricted named pipe; the original user writes history.
use crate::{metrics::wide, model::Report, traffic};
use std::{
    fs::File,
    io::{BufRead, BufReader, Write},
    os::windows::io::FromRawHandle,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::{Com::CoCreateGuid, Pipes::*, Threading::*},
        UI::{Shell::*, WindowsAndMessaging::SW_HIDE},
    },
    core::{PCWSTR, w},
};

pub struct PipeWriter(pub Arc<Mutex<File>>);
impl Write for PipeWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| std::io::Error::other("Pipe lock failed"))?
            .write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn send(pipe: &Mutex<File>, value: &serde_json::Value) -> std::io::Result<()> {
    let mut p = pipe
        .lock()
        .map_err(|_| std::io::Error::other("Pipe lock failed"))?;
    serde_json::to_writer(&mut *p, value)?;
    p.write_all(b"\n")
}
struct Disconnect(HANDLE);
impl Drop for Disconnect {
    fn drop(&mut self) {
        unsafe {
            let _ = DisconnectNamedPipe(self.0);
        }
    }
}
struct Owned(HANDLE);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn pipe_name(id: &str) -> Result<Vec<u16>, String> {
    if id.len() != 36 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err("Invalid private pipe identifier".into());
    }
    Ok(wide(&format!("\\\\.\\pipe\\SuperOpti.Network.{id}")))
}
pub fn record(
    seconds: u64,
    stop: &mpsc::Receiver<()>,
    notify: impl Fn(bool, Report),
) -> Result<(), String> {
    record_mode(seconds, stop, notify, false)
}
pub fn record_cores(notify: impl Fn(bool, Report)) -> Result<(), String> {
    let (_tx, rx) = mpsc::channel();
    record_mode(5, &rx, notify, true)
}
fn record_mode(
    seconds: u64,
    stop: &mpsc::Receiver<()>,
    notify: impl Fn(bool, Report),
    cores: bool,
) -> Result<(), String> {
    let id = format!(
        "{:?}",
        unsafe { CoCreateGuid() }.map_err(|e| e.to_string())?
    );
    let name = pipe_name(&id)?;
    let mut sd = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            w!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)"),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
    }
    .map_err(|e| e.to_string())?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: false.into(),
    };
    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65536,
            65536,
            0,
            Some(&attributes),
        )
    };
    unsafe {
        let _ = LocalFree(Some(HLOCAL(sd.0)));
    }
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!(
            "Cannot create private accounting pipe: {:?}",
            unsafe { GetLastError() }
        ));
    }
    let mut pipe = unsafe { File::from_raw_handle(handle.0) };
    let _disconnect = Disconnect(handle);
    let exe = wide(
        &std::env::current_exe()
            .map_err(|e| e.to_string())?
            .display()
            .to_string(),
    );
    let args = wide(&format!(
        "{} {} {id} {}",
        if cores {
            "--cores-elevated"
        } else {
            "--network-elevated"
        },
        std::process::id(),
        seconds.clamp(1, 900)
    ));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(exe.as_ptr()),
        lpParameters: PCWSTR(args.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        if unsafe { GetLastError() } == ERROR_CANCELLED {
            let mut r = Report::new("Recording cancelled", &["Status", "Capture"]);
            r.row(&["UAC declined", "No privileged recording started"]);
            notify(false, r);
            return Ok(());
        }
        return Err(format!("Could not request administrator approval: {e}"));
    }
    let process = Owned(info.hProcess);
    let helper_pid = unsafe { GetProcessId(process.0) };
    if helper_pid == 0 {
        return Err("Elevated helper identity unavailable".into());
    }
    let begin = Instant::now();
    loop {
        let connected = unsafe { ConnectNamedPipe(handle, None) };
        let error = unsafe { GetLastError() };
        if connected.is_ok() || error == ERROR_PIPE_CONNECTED {
            break;
        }
        if begin.elapsed() > Duration::from_secs(15)
            || unsafe { WaitForSingleObject(process.0, 0) } == WAIT_OBJECT_0
        {
            return Err("Elevated recorder did not connect".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
    let mut client = 0;
    unsafe { GetNamedPipeClientProcessId(handle, &mut client) }.map_err(|e| e.to_string())?;
    if client != helper_pid {
        return Err("Private pipe client identity mismatch".into());
    }
    let mode = PIPE_READMODE_BYTE | PIPE_WAIT;
    unsafe { SetNamedPipeHandleState(handle, Some(&mode), None, None) }
        .map_err(|e| e.to_string())?;
    let reader = pipe.try_clone().map_err(|e| e.to_string())?;
    let (frames_tx, frames) = mpsc::sync_channel(1024);
    let mut live = std::collections::VecDeque::new();
    let reader_thread = thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let limit = if cores { 4 * 1024 * 1024 } else { 65536 };
        loop {
            let mut line = Vec::new();
            // Bound every IPC frame, including errors, before JSON allocation.
            let read = std::io::Read::take(&mut reader, limit + 1).read_until(b'\n', &mut line);
            match read {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if line.len() as u64 > limit || frames_tx.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let folder = if cores {
        crate::actions::data_dir().join("core-observations")
    } else {
        traffic::dir()
    };
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = folder.join(format!("trace-{}-{id}.jsonl", crate::metrics::now()));
    let mut saved = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let mut size = 0usize;
    let mut stopping = None;
    let started = Instant::now();
    let mut complete = false;
    let mut metadata = false;
    let result = (|| -> Result<(), String> {
        loop {
            if stopping.is_none()
                && (stop.try_recv().is_ok() || started.elapsed() > Duration::from_secs(seconds + 3))
            {
                let _ = pipe.write_all(b"STOP\n");
                stopping = Some(Instant::now());
            }
            if stopping.is_some_and(|at: Instant| at.elapsed() > Duration::from_secs(6)) {
                return Err("Recorder cleanup timed out; private watchdog will stop the session. Capture marked incomplete.".into());
            }
            match frames.recv_timeout(Duration::from_millis(100)) {
                Ok(line) => {
                    let value: serde_json::Value = serde_json::from_slice(&line)
                        .map_err(|e| format!("Invalid recorder frame: {e}"))?;
                    if value.get("key").is_some() {
                        let row: traffic::Row =
                            serde_json::from_value(value).map_err(|e| e.to_string())?;
                        let data = serde_json::to_vec(&row).map_err(|e| e.to_string())?;
                        live.push_back(row);
                        if live.len() > 500 {
                            live.pop_front();
                        }
                        if size + data.len() + 1 > 64 * 1024 * 1024 {
                            return Err("64 MiB history limit reached".into());
                        }
                        saved
                            .write_all(&data)
                            .and_then(|_| saved.write_all(b"\n"))
                            .map_err(|e| e.to_string())?;
                        size += data.len() + 1;
                    } else if let Some(coverage) = value.get("coverage") {
                        let coverage: traffic::Coverage =
                            serde_json::from_value(coverage.clone()).map_err(|e| e.to_string())?;
                        std::fs::write(
                            path.with_extension("meta.json"),
                            serde_json::to_vec(&coverage).map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?;
                        metadata = true;
                    } else if let Some(notice) = value.get("notice") {
                        let active = notice
                            .get("active")
                            .and_then(|v| v.as_bool())
                            .ok_or("Missing recording state")?;
                        let mut report: Report = serde_json::from_value(notice["report"].clone())
                            .map_err(|e| e.to_string())?;
                        if !cores {
                            traffic::populate_live(&mut report, &live);
                        }
                        if !active {
                            if cores {
                                serde_json::to_writer(&mut saved, &report)
                                    .map_err(|e| e.to_string())?;
                            }
                            report
                                .debug
                                .push_str(&format!("\nSaved locally: {}", path.display()));
                            complete = true;
                        }
                        notify(active, report);
                        if complete {
                            break;
                        }
                    } else {
                        return Err("Unexpected recorder frame".into());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => {
                    if complete {
                        break;
                    }
                    return Err("Elevated recorder disconnected; history is incomplete".into());
                }
            }
        }
        Ok(())
    })();
    let _ = pipe.write_all(b"STOP\n");
    unsafe {
        let _ = DisconnectNamedPipe(handle);
    }
    drop(frames);
    let _ = reader_thread.join();
    if !cores && (result.is_err() || !metadata) {
        let coverage = serde_json::json!({"schema":1,"complete":false,"events_lost":0,"buffers_lost":0,"dropped":0,"unsupported":0,"failure":result.as_ref().err().cloned().unwrap_or("Recorder did not provide coverage".into())});
        let _ = std::fs::write(
            path.with_extension("meta.json"),
            serde_json::to_vec(&coverage).unwrap(),
        );
    }
    result
}

pub fn helper(parent: u32, id: &str, seconds: u64) -> Result<(), String> {
    helper_mode(parent, id, seconds, false)
}
pub fn helper_cores(parent: u32, id: &str) -> Result<(), String> {
    helper_mode(parent, id, 5, true)
}
fn helper_mode(parent: u32, id: &str, seconds: u64, cores: bool) -> Result<(), String> {
    let name = pipe_name(id)?;
    let h = unsafe {
        CreateFileW(
            PCWSTR(name.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|e| e.to_string())?;
    let mut reader = unsafe { File::from_raw_handle(h.0) };
    let mut server = 0;
    unsafe { GetNamedPipeServerProcessId(h, &mut server) }.map_err(|e| e.to_string())?;
    if server != parent {
        return Err("Accounting pipe server identity mismatch".into());
    }
    let output = Arc::new(Mutex::new(reader.try_clone().map_err(|e| e.to_string())?));
    if cores {
        let report = crate::core_trace::collect().unwrap_or_else(Report::error);
        return send(
            &output,
            &serde_json::json!({"notice":{"active":false,"report":report}}),
        )
        .map_err(|e| e.to_string());
    }
    let (stop_tx, stop_rx) = mpsc::channel();
    let ended = stop_tx.clone();
    let messages = output.clone();
    // All writes are IPC. This elevated helper cannot be asked to write arbitrary files.
    let recording = traffic::start_inner(
        seconds,
        None,
        Some(output.clone()),
        move |active, mut report| {
            // Flow rows were already streamed individually. Keep notices below the
            // IPC frame bound; the parent reconstructs its bounded live table.
            if report.columns.first().map(String::as_str) == Some("Process") {
                report.rows.clear();
            }
            let _ = send(
                &messages,
                &serde_json::json!({"notice":{"active":active,"report":report}}),
            );
            if !active {
                let _ = ended.send(());
            }
        },
    );
    let recording = match recording {
        Ok(r) => r,
        Err(e) => {
            send(
                &output,
                &serde_json::json!({"notice":{"active":false,"report":Report::error(e)}}),
            )
            .map_err(|e| e.to_string())?;
            return Ok(());
        }
    };
    let mut r = Report::new("TCP / UDP recording", &["Status", "Scope"]);
    r.row(&["Recording", "IPv4 + IPv6 · 2-minute deadline"]);
    send(
        &output,
        &serde_json::json!({"notice":{"active":true,"report":r}}),
    )
    .map_err(|e| e.to_string())?;
    thread::spawn(move || {
        let mut byte = [0u8; 1];
        let _ = std::io::Read::read(&mut reader, &mut byte);
        let _ = stop_tx.send(());
    });
    let _ = stop_rx.recv_timeout(Duration::from_secs(seconds.clamp(1, 900) + 2));
    drop(recording);
    Ok(())
}
