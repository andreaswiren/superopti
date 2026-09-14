#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(unsafe_op_in_unsafe_fn)]
mod actions;
mod engine;
mod metrics;
mod native;
use metrics::{Sample, fmt, wide};
use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{LibraryLoader::GetModuleHandleW, Threading::*},
        UI::{Controls::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};
const EVENT: u32 = WM_APP + 1;
const TRAY: u32 = WM_APP + 2;
const CLASS: PCWSTR = w!("SuperOptiWindow");
enum Command {
    Start(u64, u64),
    Stop,
    Checks,
    Fix(u32),
    Undo,
    Threads(u32),
    Auto(bool),
    Install,
    Quit,
    Collected(u64, String, Event),
    CollectorEnded(u64, String),
    Deadline(u64),
}
#[derive(serde::Serialize, serde::Deserialize)]
enum Event {
    Started(String),
    Sample(Box<Sample>),
    Stopped(String),
    Report(String),
}
struct App {
    tx: Sender<Command>,
    rx: Receiver<Event>,
    history: VecDeque<Sample>,
    active: bool,
    status: String,
    report: HWND,
    table: HWND,
    pid: HWND,
    slow: HWND,
    sort: HWND,
    controls: Vec<(HWND, i32, i32, i32, i32)>,
    font: HFONT,
    mono: HFONT,
    heading: HFONT,
    taskbar: u32,
}
struct WindowState {
    app: std::cell::RefCell<App>,
    deferred_event: std::cell::Cell<bool>,
}
unsafe fn set_text(hwnd: HWND, s: &str) {
    let t = wide(s);
    let _ = SetWindowTextW(hwnd, PCWSTR(t.as_ptr()));
}
unsafe fn message(hwnd: HWND, text: &str, flags: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let t = wide(text);
    MessageBoxW(Some(hwnd), PCWSTR(t.as_ptr()), w!("SuperOpti"), flags)
}
unsafe fn open(hwnd: HWND, target: &str) {
    let t = wide(target);
    let result = ShellExecuteW(
        Some(hwnd),
        w!("open"),
        PCWSTR(t.as_ptr()),
        None,
        None,
        SW_SHOWNORMAL,
    );
    if result.0 as isize <= 32 {
        message(
            hwnd,
            "Windows could not open this location. It may be restricted by system policy.",
            MB_OK | MB_ICONERROR,
        );
    }
}
unsafe fn add_control(
    app: &mut App,
    parent: HWND,
    class: PCWSTR,
    text: &str,
    id: usize,
    style: WINDOW_STYLE,
    rect: (i32, i32, i32, i32),
) -> HWND {
    let t = wide(text);
    let (x, y, width, height) = rect;
    let h = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        class,
        PCWSTR(t.as_ptr()),
        WS_CHILD | WS_VISIBLE | style,
        x,
        y,
        width,
        height,
        Some(parent),
        Some(HMENU(id as *mut _)),
        None,
        None,
    )
    .expect("create control");
    SendMessageW(
        h,
        WM_SETFONT,
        Some(WPARAM(app.font.0 as usize)),
        Some(LPARAM(1)),
    );
    app.controls.push((h, x, y, width, height));
    h
}
unsafe fn init(app: &mut App, hwnd: HWND) {
    for (id, text, x, width) in [
        (101, "Capture 2 min", 20, 126),
        (102, "Capture 5 min", 154, 126),
        (103, "Capture 15 min", 288, 130),
        (104, "Stop", 426, 70),
        (105, "Export JSON", 504, 112),
        (106, "System checks", 624, 128),
        (107, "Hide to tray", 760, 112),
    ] {
        add_control(
            app,
            hwnd,
            w!("BUTTON"),
            text,
            id,
            WS_TABSTOP,
            (x, 56, width, 30),
        );
    }
    app.slow = add_control(
        app,
        hwnd,
        w!("BUTTON"),
        "5s interval",
        108,
        WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        (890, 56, 122, 30),
    );
    app.sort = add_control(
        app,
        hwnd,
        w!("COMBOBOX"),
        "",
        109,
        WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        (940, 312, 210, 180),
    );
    for item in [
        "Top 10: pressure score",
        "Top 10: CPU",
        "Top 10: RAM",
        "Top 10: I/O",
        "Top 10: GPU",
        "Top 10: handles",
        "Top 10: threads",
    ] {
        let t = wide(item);
        SendMessageW(
            app.sort,
            CB_ADDSTRING,
            None,
            Some(LPARAM(t.as_ptr() as isize)),
        );
    }
    SendMessageW(app.sort, CB_SETCURSEL, Some(WPARAM(0)), None);
    app.table = add_control(
        app,
        hwnd,
        w!("EDIT"),
        "Start a capture while the system feels slow. The first sample arrives after the selected interval.",
        201,
        WS_VSCROLL
            | WS_HSCROLL
            | WINDOW_STYLE(ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOHSCROLL as u32),
        (20, 350, 1130, 236),
    );
    SendMessageW(
        app.table,
        WM_SETFONT,
        Some(WPARAM(app.mono.0 as usize)),
        Some(LPARAM(1)),
    );
    add_control(
        app,
        hwnd,
        w!("STATIC"),
        "Process ID:",
        210,
        WINDOW_STYLE(0),
        (20, 601, 85, 26),
    );
    app.pid = add_control(
        app,
        hwnd,
        w!("EDIT"),
        "",
        211,
        WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_NUMBER as u32),
        (106, 596, 95, 28),
    );
    for (id, text, x, width) in [
        (110, "Inspect threads (2s)", 210, 168),
        (111, "Startup on", 390, 98),
        (112, "Startup off", 496, 98),
        (113, "Install for me", 606, 118),
        (114, "Open captures", 734, 130),
        (115, "Exit", 876, 70),
    ] {
        add_control(
            app,
            hwnd,
            w!("BUTTON"),
            text,
            id,
            WS_TABSTOP,
            (x, 596, width, 28),
        );
    }
    for (id, text, x, width) in [
        (121, "Fix animations", 20, 125),
        (122, "Fix power saver", 153, 137),
        (123, "Fix all (2 fixes)", 298, 136),
        (124, "Undo fixes", 442, 105),
        (125, "Storage", 555, 86),
        (126, "Startup apps", 649, 110),
        (127, "Updates", 767, 85),
        (128, "Virtual memory", 860, 136),
        (129, "Power settings", 1004, 126),
    ] {
        add_control(
            app,
            hwnd,
            w!("BUTTON"),
            text,
            id,
            WS_TABSTOP,
            (x, 641, width, 29),
        );
    }
    app.report = add_control(
        app,
        hwnd,
        w!("EDIT"),
        "Run System checks to review disk space, paging policy, startup entries, power plan, animations and pending restart.\r\n\r\nFix all offers two reversible settings changes: reduce client-area animations and switch the standard Power saver plan to Balanced.\r\nOther buttons open Windows settings for review. Captures stay in memory until explicitly exported.",
        202,
        WS_TABSTOP
            | WS_VSCROLL
            | WINDOW_STYLE(ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32),
        (20, 684, 1130, 150),
    );
    app.taskbar = RegisterWindowMessageW(w!("TaskbarCreated"));
    tray(hwnd, NIM_ADD, false);
}
unsafe fn tray(hwnd: HWND, action: NOTIFY_ICON_MESSAGE, active: bool) {
    let mut icon = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY,
        hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
        ..Default::default()
    };
    let tip = wide(if active {
        "SuperOpti - monitoring active"
    } else {
        "SuperOpti - idle (no sampling)"
    });
    icon.szTip[..tip.len()].copy_from_slice(&tip);
    if !Shell_NotifyIconW(action, &icon).as_bool() && action == NIM_ADD {
        let _ = ShowWindow(hwnd, SW_SHOW);
        message(
            hwnd,
            "Could not add the tray icon. The window will remain accessible.",
            MB_OK | MB_ICONWARNING,
        );
    }
}
unsafe fn update_table(app: &App) {
    let Some(s) = app.history.back() else {
        return;
    };
    let sort = SendMessageW(app.sort, CB_GETCURSEL, None, None).0;
    let mut rows: Vec<_> = s.processes.iter().collect();
    let key = |p: &metrics::Process| {
        match sort {
            1 => p.cpu,
            2 => p.ram_mb,
            3 => p.io_mb,
            4 => p.gpu,
            5 => p.handles,
            6 => p.threads,
            _ => Some(p.score),
        }
        .unwrap_or(-1.0)
    };
    rows.sort_by(|a, b| key(b).total_cmp(&key(a)));
    let mut text = String::from(
        " PROCESS / INSTANCE                  PID    CPU %    RAM MiB  I/O MiB/s  GPU %  THREADS  HANDLES   SCORE  LIKELY CONTRIBUTION\r\n",
    );
    for p in rows.iter().take(10) {
        let name: String = p.name.chars().take(33).collect();
        text.push_str(&format!(
            " {:<33} {:>7} {:>8} {:>10} {:>10} {:>6} {:>8} {:>8} {:>7.1}  {}\r\n",
            name,
            p.pid,
            fmt(p.cpu, ""),
            fmt(p.ram_mb, ""),
            fmt(p.io_mb, ""),
            fmt(p.gpu, ""),
            p.threads
                .map(|v| format!("{v:.0}"))
                .unwrap_or_else(|| "N/A".into()),
            p.handles
                .map(|v| format!("{v:.0}"))
                .unwrap_or_else(|| "N/A".into()),
            p.score,
            p.reason
        ));
    }
    text.push_str(&format!("\r\nDisk {} MiB/s | latency {} ms | queue {} | page-ins {}/s | CPU queue {} | DPC {}%\r\nNetwork {} MiB/s | context switches {}/s | capture elapsed {:.0}s | core collection {:.1}ms",fmt(s.disk_mb,""),fmt(s.disk_latency_ms,""),fmt(s.disk_queue,""),fmt(s.page_reads,""),fmt(s.cpu_queue,""),fmt(s.dpc,""),fmt(s.network_mb,""),fmt(s.context_switches,""),s.elapsed,s.collection_ms));
    set_text(app.table, &text);
}
unsafe fn command(app: &mut App, hwnd: HWND, id: usize) {
    let cmd = match id {
        101..=103 => {
            if !app.history.is_empty()
                && message(
                    hwnd,
                    "Start a new capture? The previous in-memory capture will be replaced. Export it first if you want to keep it.",
                    MB_YESNO | MB_ICONQUESTION,
                ) != IDYES
            {
                return;
            }
            let step =
                if SendMessageW(app.slow, BM_GETCHECK, None, None).0 == BST_CHECKED.0 as isize {
                    5
                } else {
                    2
                };
            Some(Command::Start(
                match id {
                    101 => 120,
                    102 => 300,
                    _ => 900,
                },
                step,
            ))
        }
        104 => Some(Command::Stop),
        105 => {
            export(app);
            None
        }
        106 => Some(Command::Checks),
        107 => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            None
        }
        109 => {
            update_table(app);
            None
        }
        110 => {
            let mut buf = [0u16; 32];
            let n = GetWindowTextW(app.pid, &mut buf);
            match String::from_utf16_lossy(&buf[..n as usize]).parse::<u32>() {
                Ok(pid) if pid > 0 => Some(Command::Threads(pid)),
                _ => {
                    message(hwnd, "Enter a process ID from the top-10 table.", MB_OK);
                    None
                }
            }
        }
        111 => Some(Command::Auto(true)),
        112 => Some(Command::Auto(false)),
        113 => Some(Command::Install),
        114 => {
            let dir = actions::data_dir().join("captures");
            let _ = std::fs::create_dir_all(&dir);
            open(hwnd, &dir.to_string_lossy());
            None
        }
        115 => {
            let _ = DestroyWindow(hwnd);
            None
        }
        121..=123 => {
            let description = match id {
                121 => {
                    "Disable client-area animations? Some transitions will lose motion. This is optional and may offer only a small performance benefit."
                }
                122 => {
                    "Switch the standard Power saver plan to Balanced, if active? This can increase energy use. Custom plans are left as configured."
                }
                _ => {
                    "Apply both available fixes?\n\n1. Disable client-area animations.\n2. Switch the standard Power saver plan to Balanced, if active (may use more energy).\n\nOriginal settings are saved for Undo. Storage, startup, update and pagefile reviews require your choices and are not applied by Fix all."
                }
            };
            if message(hwnd, description, MB_YESNO | MB_ICONQUESTION) == IDYES {
                Some(Command::Fix((id - 120) as u32))
            } else {
                None
            }
        }
        124 => {
            if message(
                hwnd,
                "Restore animation and power settings saved before SuperOpti fixes? This may replace settings changed since then.",
                MB_YESNO | MB_ICONQUESTION,
            ) == IDYES
            {
                Some(Command::Undo)
            } else {
                None
            }
        }
        125 => {
            open(hwnd, "ms-settings:storagesense");
            None
        }
        126 => {
            open(hwnd, "ms-settings:startupapps");
            None
        }
        127 => {
            open(hwnd, "ms-settings:windowsupdate");
            None
        }
        128 => {
            open(hwnd, "SystemPropertiesAdvanced.exe");
            None
        }
        129 => {
            open(hwnd, "ms-settings:powersleep");
            None
        }
        _ => None,
    };
    if let Some(c) = cmd {
        set_text(app.report, "Working on your request...");
        let _ = app.tx.send(c);
    }
}
fn export(app: &App) {
    let result = (|| -> Result<String, String> {
        if app.history.is_empty() {
            return Err("No samples to export. Start a capture first.".into());
        }
        let dir = actions::data_dir().join("captures");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!(
            "capture-{}-{}.json",
            metrics::now(),
            std::process::id()
        ));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let data = serde_json::json!({"schema_version":1,"app_version":env!("CARGO_PKG_VERSION"),"notes":"Rankings are heuristic, not proof of causation. Process I/O includes non-disk I/O. Missing counters are null. GPU is busiest physical engine; per-process GPU sums engines capped at 100. RAM is working set. Disk busy is aggregate. Timestamps are UTC Unix seconds.","samples":app.history});
        serde_json::to_writer(file, &data).map_err(|e| e.to_string())?;
        Ok(format!(
            "Exported {} samples to {}\r\nExports contain process names and PIDs. Nothing is uploaded.",
            app.history.len(),
            path.display()
        ))
    })();
    unsafe {
        set_text(app.report, &result.unwrap_or_else(|e| e));
    }
}
fn color(r: u32, g: u32, b: u32) -> COLORREF {
    COLORREF(r | (g << 8) | (b << 16))
}
unsafe fn fill(dc: HDC, rect: &RECT, c: COLORREF) {
    let brush = CreateSolidBrush(c);
    FillRect(dc, rect, brush);
    let _ = DeleteObject(brush.into());
}
unsafe fn text(dc: HDC, x: i32, y: i32, s: &str, c: COLORREF) {
    SetTextColor(dc, c);
    let v: Vec<_> = s.encode_utf16().collect();
    let _ = TextOutW(dc, x, y, &v);
}
unsafe fn paint(app: &App, hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let dc = BeginPaint(hwnd, &mut ps);
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    fill(dc, &rect, color(245, 247, 250));
    SetBkMode(dc, TRANSPARENT);
    let old = SelectObject(dc, app.heading.into());
    text(dc, 20, 14, "SuperOpti", color(20, 34, 52));
    SelectObject(dc, app.font.into());
    text(
        dc,
        190,
        24,
        "ON-DEMAND WINDOWS DIAGNOSTICS",
        color(84, 104, 124),
    );
    text(
        dc,
        20,
        99,
        &app.status,
        if app.active {
            color(0, 118, 103)
        } else {
            color(84, 104, 124)
        },
    );
    let gap = 12;
    let width = (rect.right - 40 - 5 * gap) / 6;
    let latest = app.history.back();
    for (i, label) in ["CPU", "RAM", "GPU", "DISK BUSY", "COMMIT", "PAGEFILE"]
        .iter()
        .enumerate()
    {
        let x = 20 + i as i32 * (width + gap);
        let y = 136;
        fill(
            dc,
            &RECT {
                left: x,
                top: y,
                right: x + width,
                bottom: 294,
            },
            color(255, 255, 255),
        );
        let value = |s: &Sample| match i {
            0 => s.cpu,
            1 => s.ram,
            2 => s.gpu,
            3 => s.disk,
            4 => s.commit,
            _ => s.swap,
        };
        text(dc, x + 12, y + 10, label, color(84, 104, 124));
        SelectObject(dc, app.heading.into());
        text(
            dc,
            x + 12,
            y + 33,
            &fmt(latest.and_then(value), "%"),
            color(20, 34, 52),
        );
        SelectObject(dc, app.font.into());
        let graph = RECT {
            left: x + 12,
            top: y + 80,
            right: x + width - 12,
            bottom: y + 142,
        };
        let grid = CreatePen(PS_SOLID, 1, color(226, 233, 240));
        let prev = SelectObject(dc, grid.into());
        for row in 0..3 {
            let gy = graph.top + row * (graph.bottom - graph.top) / 2;
            let _ = MoveToEx(dc, graph.left, gy, None);
            let _ = LineTo(dc, graph.right, gy);
        }
        SelectObject(dc, prev);
        let _ = DeleteObject(grid.into());
        let pen = CreatePen(PS_SOLID, 2, color(0, 150, 136));
        let prev = SelectObject(dc, pen.into());
        let end = latest.map(|s| s.elapsed).unwrap_or(0.0);
        let begin = (end - 120.0).max(0.0);
        let span = (end - begin).max(2.0);
        let mut connected = false;
        for s in app.history.iter().filter(|s| s.elapsed >= begin) {
            if let Some(v) = value(s) {
                let px = graph.left
                    + ((s.elapsed - begin) / span * (graph.right - graph.left) as f64) as i32;
                let py = graph.bottom
                    - (v.clamp(0.0, 100.0) / 100.0 * (graph.bottom - graph.top) as f64) as i32;
                if connected {
                    let _ = LineTo(dc, px, py);
                } else {
                    let _ = MoveToEx(dc, px, py, None);
                    connected = true;
                }
            } else {
                connected = false;
            }
        }
        SelectObject(dc, prev);
        let _ = DeleteObject(pen.into());
    }
    text(
        dc,
        20,
        315,
        "TOP 10  /  Likely contributors, not proven causes. Trends: last 2 min, 0-100%.",
        color(20, 34, 52),
    );
    SelectObject(dc, old);
    let _ = EndPaint(hwnd, &ps);
}
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_DESTROY {
        tray(hwnd, NIM_DELETE, false);
        PostQuitMessage(0);
        return LRESULT(0);
    }
    if msg == WM_NCCREATE {
        let cs = &*(lparam.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &*ptr;
    // Modal dialogs and synchronous Win32 calls can re-enter the window procedure.
    // Never manufacture aliased mutable references; deliver deferred worker events
    // when the outer callback releases its borrow.
    let Ok(mut guard) = state.app.try_borrow_mut() else {
        if msg == EVENT {
            state.deferred_event.set(true);
            return LRESULT(0);
        }
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };
    let app = &mut *guard;
    if app.taskbar != 0 && msg == app.taskbar {
        tray(hwnd, NIM_ADD, app.active);
        return LRESULT(0);
    }
    let result = match msg {
        WM_CREATE => {
            init(app, hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            command(app, hwnd, wparam.0 & 0xffff);
            LRESULT(0)
        }
        EVENT => {
            while let Ok(event) = app.rx.try_recv() {
                match event {
                    Event::Started(s) => {
                        app.history.clear();
                        app.active = true;
                        app.status = s;
                        set_text(app.table, "Warming up performance counters...");
                        tray(hwnd, NIM_MODIFY, true);
                    }
                    Event::Sample(s) => {
                        app.status = format!(
                            "CAPTURING  /  {:.0}s elapsed  /  core collection {:.1}ms  /  automatic stop enabled",
                            s.elapsed, s.collection_ms
                        );
                        app.history.push_back(*s);
                        if app.history.len() > 900 {
                            app.history.pop_front();
                        }
                        update_table(app);
                    }
                    Event::Stopped(s) => {
                        app.active = false;
                        app.status = s;
                        tray(hwnd, NIM_MODIFY, false);
                    }
                    Event::Report(s) => set_text(app.report, &s),
                }
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_PAINT => {
            paint(app, hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            if wparam.0 == SIZE_MINIMIZED as usize {
                let _ = ShowWindow(hwnd, SW_HIDE);
                return LRESULT(0);
            }
            let mut r = RECT::default();
            let _ = GetClientRect(hwnd, &mut r);
            for &(h, x, y, width, height) in &app.controls {
                let (width, height) = if h == app.table {
                    (r.right - 40, height)
                } else if h == app.report {
                    (r.right - 40, (r.bottom - y - 20).max(70))
                } else {
                    (width, height)
                };
                let _ = MoveWindow(h, x, y, width, height, true);
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let m = &mut *(lparam.0 as *mut MINMAXINFO);
            m.ptMinTrackSize = POINT { x: 1190, y: 850 };
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        TRAY => {
            match lparam.0 as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                }
                WM_RBUTTONUP => {
                    if let Ok(menu) = CreatePopupMenu() {
                        let _ = AppendMenuW(menu, MF_STRING, 1070, w!("Show SuperOpti"));
                        let _ = AppendMenuW(menu, MF_STRING, 102, w!("Capture 5 minutes"));
                        let _ = AppendMenuW(menu, MF_STRING, 104, w!("Stop monitoring"));
                        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
                        let _ = AppendMenuW(menu, MF_STRING, 115, w!("Exit"));
                        let mut p = POINT::default();
                        let _ = GetCursorPos(&mut p);
                        let _ = SetForegroundWindow(hwnd);
                        let choice = TrackPopupMenu(
                            menu,
                            TPM_RETURNCMD | TPM_RIGHTBUTTON,
                            p.x,
                            p.y,
                            Some(0),
                            hwnd,
                            None,
                        )
                        .0 as usize;
                        let _ = DestroyMenu(menu);
                        if choice == 1070 {
                            let _ = ShowWindow(hwnd, SW_RESTORE);
                        } else if choice != 0 {
                            command(app, hwnd, choice);
                        }
                        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    };
    drop(guard);
    if state.deferred_event.replace(false) {
        let _ = PostMessageW(Some(hwnd), EVENT, WPARAM(0), LPARAM(0));
    }
    result
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--collector") {
        if let (Some(group), Some(seconds), Some(step)) = (
            args.get(2),
            args.get(3).and_then(|v| v.parse::<u64>().ok()),
            args.get(4).and_then(|v| v.parse::<u64>().ok()),
        ) {
            engine::collector(group, seconds.clamp(2, 900), step.clamp(1, 5));
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--thread-probe") {
        if let Some(pid) = args.get(2).and_then(|v| v.parse::<u32>().ok()) {
            println!("{}", native::thread_report(pid).unwrap_or_else(|e| e));
        }
        return;
    }
    if std::env::args().any(|v| v == "--smoke-test") {
        smoke();
        return;
    }
    unsafe {
        // One UI per interactive session, including portable/installed copies.
        let mutex = CreateMutexW(None, false, w!("Local\\SuperOpti.SingleInstance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(h) = FindWindowW(CLASS, None) {
                let _ = ShowWindow(h, SW_RESTORE);
                let _ = SetForegroundWindow(h);
            }
            return;
        }
        let _mutex = mutex.expect("single-instance mutex");
        let _ = SetProcessDPIAware();
        let instance = GetModuleHandleW(None).expect("module");
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&class);
        let (tx, rx_worker) = mpsc::channel();
        let (tx_worker, rx) = mpsc::channel();
        let make_font = |height, weight, name| {
            CreateFontW(
                height,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                DEFAULT_QUALITY,
                DEFAULT_PITCH.0 as u32,
                name,
            )
        };
        let app = App {
            tx,
            rx,
            history: VecDeque::new(),
            active: false,
            status:
                "IDLE  /  No background sampling. Start a short capture when the system feels slow."
                    .into(),
            report: HWND::default(),
            table: HWND::default(),
            pid: HWND::default(),
            slow: HWND::default(),
            sort: HWND::default(),
            controls: Vec::new(),
            font: make_font(-16, 400, w!("Segoe UI")),
            mono: make_font(-14, 400, w!("Consolas")),
            heading: make_font(-27, 600, w!("Segoe UI")),
            taskbar: 0,
        };
        let state = Box::new(WindowState {
            app: std::cell::RefCell::new(app),
            deferred_event: std::cell::Cell::new(false),
        });
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            w!("SuperOpti | On-demand performance diagnostics"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1210,
            940,
            None,
            None,
            Some(instance.into()),
            Some((&*state as *const WindowState).cast()),
        )
        .expect("main window");
        let address = hwnd.0 as usize;
        let commands = state.app.borrow().tx.clone();
        let handle =
            std::thread::spawn(move || engine::worker(address, rx_worker, commands, tx_worker));
        if !std::env::args().any(|v| v == "--tray") {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let app = state.app.borrow();
        let _ = app.tx.send(Command::Quit);
        let _ = handle.join();
        for f in [app.font, app.mono, app.heading] {
            let _ = DeleteObject(f.into());
        }
        let _ = CloseHandle(_mutex);
    }
}
fn smoke() {
    let result = (|| -> Result<serde_json::Value, String> {
        let mut m = native::NativeMonitor::new()?;
        std::thread::sleep(Duration::from_secs(2));
        let s = m.sample(2.0)?;
        let count = s.processes.len();
        if (s.cpu.is_none() && unsafe { GetActiveProcessorGroupCount() } == 1)
            || s.ram.is_none()
            || count == 0
        {
            return Err(format!(
                "Core counters unavailable: cpu={:?}, ram={:?}, processes={count}",
                s.cpu, s.ram
            ));
        }
        let lifecycle = engine::smoke_test()?;
        let threads = native::thread_report(std::process::id())?;
        Ok(serde_json::json!({"native_sample":s,"capture_lifecycle":lifecycle,"threads":threads}))
    })();
    let path = std::env::args()
        .skip_while(|v| v != "--smoke-test")
        .nth(1)
        .unwrap_or_else(|| "smoke-result.json".into());
    let failed = result.is_err();
    let _ = std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap());
    if failed {
        std::process::exit(1);
    }
}
