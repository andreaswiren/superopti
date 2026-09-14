use crate::{
    Command, EVENT, Event, actions,
    metrics::{self, Sample},
    native::NativeMonitor,
};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    os::windows::{io::AsRawHandle, process::CommandExt},
    process::{Child, Command as ProcessCommand, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};
use windows::Win32::{Foundation::*, System::JobObjects::*, UI::WindowsAndMessaging::*};
struct Job(HANDLE);
impl Job {
    fn new() -> Result<Self, String> {
        unsafe {
            let job = Self(CreateJobObjectW(None, None).map_err(|e| e.to_string())?);
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .map_err(|e| e.to_string())?;
            Ok(job)
        }
    }
    fn attach(&self, child: &Child) -> Result<(), String> {
        unsafe {
            AssignProcessToJobObject(self.0, HANDLE(child.as_raw_handle()))
                .map_err(|e| e.to_string())
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct Capture {
    children: Vec<Child>,
    cancel: Sender<()>,
    start: Instant,
    latest: HashMap<String, (Instant, Sample)>,
    step: u64,
    _job: Job,
}
impl Drop for Capture {
    fn drop(&mut self) {
        metrics::trace("Capture drop begin");
        let _ = self.cancel.send(());
        for child in &mut self.children {
            metrics::trace(&format!("Kill {}", child.id()));
            let _ = child.kill();
        }
        // TerminateProcess stops every thread immediately but Windows may take seconds
        // cancelling a driver's pending I/O. Do not block the command coordinator on
        // process-object retirement. Dropping Child releases its Windows handles;
        // kill-on-close Job ownership supplies an additional termination boundary.
        metrics::trace("Capture drop end");
    }
}
fn emit(hwnd: usize, tx: &Sender<Event>, event: Event) {
    if tx.send(event).is_ok() && hwnd != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), EVENT, WPARAM(0), LPARAM(0));
        }
    }
}
fn launch(
    group: &str,
    seconds: u64,
    step: u64,
    generation: u64,
    commands: Sender<Command>,
    job: &Job,
) -> Result<Child, String> {
    let mut child = ProcessCommand::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args([
            "--collector",
            group,
            &seconds.to_string(),
            &step.to_string(),
        ])
        .creation_flags(0x08000000)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Err(e) = job.attach(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    let pipe = child.stdout.take().ok_or("Collector has no output pipe")?;
    let group = group.to_string();
    std::thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            let event = line
                .ok()
                .and_then(|v| serde_json::from_str::<Event>(&v).ok());
            match event {
                Some(event) => {
                    if commands
                        .send(Command::Collected(generation, group.clone(), event))
                        .is_err()
                    {
                        break;
                    }
                }
                None => break,
            }
        }
        let _ = commands.send(Command::CollectorEnded(generation, group));
    });
    Ok(child)
}
pub fn worker(
    hwnd: usize,
    rx: Receiver<Command>,
    commands: Sender<Command>,
    events: Sender<Event>,
) {
    let (action_tx, action_rx) = mpsc::channel();
    let action_events = events.clone();
    let action_handle = std::thread::spawn(move || {
        while let Ok(command) = action_rx.recv() {
            let report = match command {
                Command::Checks => actions::checks(),
                Command::Fix(n) => actions::fix(n).unwrap_or_else(|e| e),
                Command::Undo => actions::undo().unwrap_or_else(|e| e),
                Command::Threads(pid, created) => {
                    emit(
                        hwnd,
                        &action_events,
                        Event::ProcessReport(pid, thread_probe(pid, created)),
                    );
                    continue;
                }
                Command::Details(pid, created) => {
                    emit(
                        hwnd,
                        &action_events,
                        Event::ProcessReport(
                            pid,
                            crate::native::process_report(pid, created).unwrap_or_else(|e| e),
                        ),
                    );
                    continue;
                }
                Command::Network(pid, created) => {
                    emit(
                        hwnd,
                        &action_events,
                        Event::ProcessReport(
                            pid,
                            crate::network::measure(pid, created).unwrap_or_else(|e| e),
                        ),
                    );
                    continue;
                }
                Command::Auto(b) => actions::autostart(b).unwrap_or_else(|e| e),
                Command::Install => actions::install().unwrap_or_else(|e| e),
                Command::Quit => break,
                _ => continue,
            };
            emit(hwnd, &action_events, Event::Report(report));
        }
    });
    let mut generation = 0u64;
    let mut capture: Option<Capture> = None;
    while let Ok(command) = rx.recv() {
        match command {
            Command::Quit => break,
            Command::Start(seconds, step) => {
                capture = None;
                generation += 1;
                let result = (|| -> Result<Capture, String> {
                    let job = Job::new()?;
                    let (cancel, timer) = mpsc::channel();
                    let mut c = Capture {
                        children: Vec::new(),
                        cancel,
                        start: Instant::now(),
                        latest: HashMap::new(),
                        step,
                        _job: job,
                    };
                    metrics::trace("Capture launch begin");
                    for group in ["core", "extra", "disk", "gpu", "network"] {
                        c.children.push(launch(
                            group,
                            seconds,
                            step,
                            generation,
                            commands.clone(),
                            &c._job,
                        )?);
                    }
                    metrics::trace("Capture launch end");
                    let notify = commands.clone();
                    let id = generation;
                    let remaining = Duration::from_secs(seconds).saturating_sub(c.start.elapsed());
                    std::thread::spawn(move || {
                        if timer.recv_timeout(remaining).is_err() {
                            let _ = notify.send(Command::Deadline(id));
                        }
                    });
                    Ok(c)
                })();
                match result {
                    Ok(c) => {
                        capture = Some(c);
                        emit(
                            hwnd,
                            &events,
                            Event::Started(format!(
                                "CAPTURING  /  {} min  /  every {step}s  /  warming up native counters",
                                seconds / 60
                            )),
                        );
                    }
                    Err(e) => emit(
                        hwnd,
                        &events,
                        Event::Stopped(format!("Capture could not start: {e}")),
                    ),
                }
            }
            Command::Stop => {
                capture = None;
                generation += 1;
                emit(
                    hwnd,
                    &events,
                    Event::Stopped(
                        "IDLE  /  Sampling stopped; Windows may finish pending I/O cleanup.".into(),
                    ),
                );
            }
            Command::Deadline(id) if id == generation => {
                metrics::trace("Deadline received");
                capture = None;
                generation += 1;
                emit(
                    hwnd,
                    &events,
                    Event::Stopped(
                        "IDLE  /  Capture completed automatically. No background sampling.".into(),
                    ),
                );
            }
            Command::Collected(id, group, event) if id == generation => {
                if let Some(c) = capture.as_mut() {
                    match event {
                        Event::Sample(mut s) if group == "core" => {
                            s.elapsed = c.start.elapsed().as_secs_f64();
                            for (name, (at, extra)) in &c.latest {
                                if at.elapsed() > Duration::from_secs((c.step * 3).max(15)) {
                                    continue;
                                }
                                match name.as_str() {
                                    "disk" => {
                                        s.disk = extra.disk;
                                        s.disk_mb = extra.disk_mb;
                                        s.disk_latency_ms = extra.disk_latency_ms;
                                        s.disk_queue = extra.disk_queue;
                                    }
                                    "gpu" => {
                                        s.gpu = extra.gpu;
                                        for p in &mut s.processes {
                                            p.gpu = extra
                                                .gpu_processes
                                                .get(&p.pid)
                                                .map(|v| v.min(100.0));
                                        }
                                    }
                                    "network" => s.network_mb = extra.network_mb,
                                    "extra" => {
                                        s.swap = extra.swap;
                                        s.page_reads = extra.page_reads;
                                        s.cpu_queue = extra.cpu_queue;
                                        s.dpc = extra.dpc;
                                        s.context_switches = extra.context_switches;
                                    }
                                    _ => {}
                                }
                            }
                            for i in 0..s.processes.len() {
                                let mut p = std::mem::take(&mut s.processes[i]);
                                metrics::rank(&mut p, &s);
                                s.processes[i] = p;
                            }
                            s.processes.sort_by(|a, b| {
                                b.score.total_cmp(&a.score).then(a.pid.cmp(&b.pid))
                            });
                            s.processes.truncate(10);
                            emit(hwnd, &events, Event::Sample(s));
                        }
                        Event::Sample(s) => {
                            c.latest.insert(group, (Instant::now(), *s));
                        }
                        Event::Report(s) => emit(
                            hwnd,
                            &events,
                            Event::Report(format!("{group} collector: {s}")),
                        ),
                        _ => {}
                    }
                }
            }
            Command::CollectorEnded(id, group) if id == generation && group == "core" => {
                // The deadline owns normal completion. An early core exit is a failed capture.
                if let Some(c) = &capture {
                    emit(
                        hwnd,
                        &events,
                        Event::Report(format!(
                            "Core collector ended after {:.0}s. Optional providers may still be completing; Stop remains available.",
                            c.start.elapsed().as_secs_f64()
                        )),
                    );
                }
            }
            Command::Collected(..) | Command::CollectorEnded(..) | Command::Deadline(_) => {}
            other => {
                let _ = action_tx.send(other);
            }
        }
    }
    drop(capture);
    let _ = action_tx.send(Command::Quit);
    let _ = action_handle.join();
}
pub fn collector(group: &str, seconds: u64, step: u64) {
    let mut output = std::io::BufWriter::new(std::io::stdout());
    let mut send = |event: Event| -> Result<(), String> {
        serde_json::to_writer(&mut output, &event).map_err(|e| e.to_string())?;
        writeln!(output).map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())
    };
    let start = Instant::now();
    let result = (|| -> Result<(), String> {
        let mut native = if group == "core" {
            Some(NativeMonitor::new()?)
        } else {
            None
        };
        let pdh = if group != "core" {
            Some(metrics::Monitor::new(group)?)
        } else {
            None
        };
        while start.elapsed() < Duration::from_secs(seconds) {
            std::thread::sleep(Duration::from_secs(step));
            if start.elapsed() >= Duration::from_secs(seconds) {
                break;
            }
            let sample = if let Some(m) = native.as_mut() {
                m.sample(start.elapsed().as_secs_f64())?
            } else {
                pdh.as_ref()
                    .unwrap()
                    .sample(start.elapsed().as_secs_f64())?
            };
            send(Event::Sample(Box::new(sample)))?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = send(Event::Report(e));
    }
}
fn thread_probe(pid: u32, expected: Option<u64>) -> String {
    let result = (|| -> Result<String, String> {
        let _identity = crate::native::ProcessIdentity::open(pid, expected)?;
        let job = Job::new()?;
        let mut child = ProcessCommand::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args(["--thread-probe", &pid.to_string()])
            .creation_flags(0x08000000)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        job.attach(&child)?;
        let pipe = child.stdout.take().ok_or("No thread snapshot output")?;
        let reader = std::thread::spawn(move || {
            use std::io::Read;
            let mut text = String::new();
            let _ = BufReader::new(pipe).read_to_string(&mut text);
            text
        });
        let start = Instant::now();
        while child.try_wait().map_err(|e| e.to_string())?.is_none() {
            if start.elapsed() > Duration::from_secs(12) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "Thread provider timed out after 12 seconds. Monitoring remains responsive."
                        .into(),
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        reader.join().map_err(|_| "Thread reader failed".into())
    })();
    result.unwrap_or_else(|e| e)
}

// Exercises real helper startup, automatic deadline, cancellation and idle behavior.
pub fn smoke_test() -> Result<serde_json::Value, String> {
    let (tx, rx) = mpsc::channel();
    let (events, received) = mpsc::channel();
    let commands = tx.clone();
    let handle = std::thread::spawn(move || worker(0, rx, commands, events));
    let result = (|| -> Result<serde_json::Value, String> {
        tx.send(Command::Start(10, 2)).map_err(|e| e.to_string())?;
        let start = Instant::now();
        let mut samples = Vec::new();
        loop {
            match received
                .recv_timeout(Duration::from_secs(15))
                .map_err(|e| e.to_string())?
            {
                Event::Sample(s) => samples.push(*s),
                Event::Stopped(_) => break,
                _ => {}
            }
            if start.elapsed() > Duration::from_secs(15) {
                return Err("Automatic deadline failed".into());
            }
        }
        let auto_stop_ms = start.elapsed().as_millis();
        if auto_stop_ms > 12000 || samples.len() < 2 {
            return Err(format!(
                "Expected responsive core samples and deadline: {} samples in {auto_stop_ms}ms",
                samples.len()
            ));
        }
        tx.send(Command::Start(60, 2)).map_err(|e| e.to_string())?;
        while !matches!(
            received
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?,
            Event::Started(_)
        ) {}
        let start = Instant::now();
        tx.send(Command::Stop).map_err(|e| e.to_string())?;
        while !matches!(
            received
                .recv_timeout(Duration::from_secs(3))
                .map_err(|e| e.to_string())?,
            Event::Stopped(_)
        ) {}
        let stop_ms = start.elapsed().as_millis();
        if stop_ms > 2000 {
            return Err(format!("Stop took {stop_ms}ms"));
        }
        if received.recv_timeout(Duration::from_millis(500)).is_ok() {
            return Err("Unexpected events while idle".into());
        }
        Ok(
            serde_json::json!({"auto_stop_ms":auto_stop_ms,"manual_stop_ms":stop_ms,"no_idle_events":true,"samples":samples}),
        )
    })();
    let _ = tx.send(Command::Quit);
    let _ = handle.join();
    result
}
