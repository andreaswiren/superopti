#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(unsafe_op_in_unsafe_fn)]
mod actions;
mod engine;
mod metrics;
mod native;
mod network;
mod ui;
use metrics::{Sample, fmt, wide};
use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};
use ui::{init, paint, update_table};
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
    Threads(u32, Option<u64>),
    Details(u32, Option<u64>),
    Network(u32, Option<u64>),
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
    ProcessReport(u32, String),
}
struct App {
    tx: Sender<Command>,
    rx: Receiver<Event>,
    history: VecDeque<Sample>,
    active: bool,
    status: String,
    report: HWND,
    table: HWND,
    telemetry: HWND,
    detail: HWND,
    detail_pid: Option<u32>,
    detail_created: Option<u64>,
    detail_pending: bool,
    displayed_rows: Vec<(u32, Option<u64>)>,
    pid: HWND,
    slow: HWND,
    sort: HWND,
    controls: Vec<(HWND, i32, i32, i32, i32)>,
    font: HFONT,
    page: usize,
    scale: f64,
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
unsafe fn tray(hwnd: HWND, action: NOTIFY_ICON_MESSAGE, active: bool) {
    let mut icon = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY,
        hIcon: ui::tray_icon(if active { 102 } else { 101 }),
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
unsafe fn command(app: &mut App, hwnd: HWND, id: usize) {
    if app.detail_pending && matches!(id, 110 | 116 | 117 | 118) {
        return;
    }
    if id == 304 {
        app.page = 0;
        ui::layout(app, hwnd);
        let _ =
            windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(GetDlgItem(Some(hwnd), 301).ok());
        return;
    }
    if (301..=303).contains(&id) {
        app.page = id - 301;
        ui::layout(app, hwnd);
        return;
    }
    let cmd = match id {
        116 => {
            ui::details(app, hwnd);
            None
        }
        117 => app
            .detail_pid
            .map(|pid| Command::Threads(pid, app.detail_created)),
        118 => {
            if message(
                hwnd,
                "Measure TCP traffic for 10 seconds? This temporarily enables Windows per-connection byte counters and may need administrator rights. SuperOpti will not elevate itself. Established TCP connections are checked once per second, so brief flows may be missed. UDP/QUIC remote peers and byte counts are unavailable. Counters enabled by SuperOpti are restored afterward where Windows permits.",
                MB_YESNO | MB_ICONQUESTION,
            ) == IDYES
            {
                app.detail_pid
                    .map(|pid| Command::Network(pid, app.detail_created))
            } else {
                None
            }
        }
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
            app.page = 0;
            ui::layout(app, hwnd);
            None
        }
        106 => {
            app.page = 1;
            ui::layout(app, hwnd);
            Some(Command::Checks)
        }
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
                Ok(pid) if pid > 0 => {
                    app.detail_pid = Some(pid);
                    app.detail_created = app
                        .history
                        .back()
                        .and_then(|s| s.processes.iter().find(|p| p.pid == pid))
                        .and_then(|p| p.created_ticks);
                    app.page = 3;
                    ui::layout(app, hwnd);
                    Some(Command::Threads(pid, app.detail_created))
                }
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
        121..=123 | 130 => {
            let description = match id {
                121 => {
                    "Disable client-area animations? Some transitions will lose motion. This is optional and may offer only a small performance benefit."
                }
                122 => {
                    "Switch the standard Power saver plan to Balanced, if active? This can increase energy use. Custom plans are left as configured."
                }
                130 => {
                    "Apply the fixed pagefile policy?\n\nInitial size = maximum size = the larger of 50% of installed physical RAM or 16 GiB. Run System checks to preview the exact target for this computer.\n\nThis requires administrator rights and a Windows restart. A disk-space guard must pass before the change is applied. SuperOpti will not restart Windows or elevate itself automatically. Original settings are saved for Undo."
                }
                _ => {
                    "Apply all three available fixes?\n\n1. Disable client-area animations.\n2. Switch the standard Power saver plan to Balanced, if active (may use more energy).\n3. Set a fixed pagefile: initial = maximum = the larger of 50% of installed physical RAM or 16 GiB. Run System checks to preview the exact target.\n\nThe pagefile change requires administrator rights, sufficient disk space and a Windows restart. SuperOpti does not automatically elevate or restart. Original settings are saved for Undo. Storage, startup and update reviews remain manual."
                }
            };
            if message(hwnd, description, MB_YESNO | MB_ICONQUESTION) == IDYES {
                Some(Command::Fix(if id == 130 { 4 } else { (id - 120) as u32 }))
            } else {
                None
            }
        }
        124 => {
            if message(
                hwnd,
                "Restore animation, power and pagefile settings saved before SuperOpti fixes? This may replace settings changed since then. Restoring pagefile settings requires administrator rights and a Windows restart. SuperOpti will not elevate or restart automatically.",
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
        if matches!(c, Command::Threads(_, _) | Command::Network(_, _)) {
            app.detail_pending = true;
            ui::layout(app, hwnd);
        }
        set_text(
            if app.page == 3 {
                app.detail
            } else {
                app.report
            },
            "Working on your request...",
        );
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
        WM_NOTIFY => {
            let notification = &*(lparam.0 as *const NMHDR);
            if notification.hwndFrom == app.table && notification.code == NM_DBLCLK {
                ui::details(app, hwnd);
            }
            if notification.hwndFrom == app.table && notification.code == LVN_KEYDOWN {
                let key = &*(lparam.0 as *const NMLVKEYDOWN);
                if key.wVKey == 13 {
                    ui::details(app, hwnd);
                }
            }
            if notification.hwndFrom == app.table && notification.code == LVN_ITEMCHANGED {
                let row = SendMessageW(
                    app.table,
                    LVM_GETNEXTITEM,
                    Some(WPARAM(usize::MAX)),
                    Some(LPARAM(LVNI_SELECTED as isize)),
                )
                .0;
                if row >= 0 {
                    let mut item = LVITEMW {
                        mask: LVIF_PARAM,
                        iItem: row as i32,
                        ..Default::default()
                    };
                    SendMessageW(
                        app.table,
                        LVM_GETITEMW,
                        None,
                        Some(LPARAM((&mut item as *mut LVITEMW) as isize)),
                    );
                    set_text(app.pid, &item.lParam.0.to_string());
                }
            }
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
                        app.displayed_rows.clear();
                        set_text(app.pid, "");
                        set_text(
                            app.telemetry,
                            "Warming up counters. The first actual sample arrives after the selected interval.",
                        );
                        app.page = 0;
                        app.active = true;
                        app.status = s;
                        SendMessageW(app.table, LVM_DELETEALLITEMS, None, None);
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
                    Event::ProcessReport(pid, s) => {
                        if app.detail_pid == Some(pid) {
                            app.detail_pending = false;
                            set_text(app.detail, &s);
                        }
                    }
                }
            }
            ui::layout(app, hwnd);
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
            ui::layout(app, hwnd);
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let m = &mut *(lparam.0 as *mut MINMAXINFO);
            m.ptMinTrackSize = POINT {
                x: (1020.0 * app.scale) as i32,
                y: (680.0 * app.scale) as i32,
            };
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
            hIcon: ui::icon(101),
            ..Default::default()
        };
        RegisterClassW(&class);
        let (tx, rx_worker) = mpsc::channel();
        let (tx_worker, rx) = mpsc::channel();
        let screen = GetDC(None);
        let scale = GetDeviceCaps(Some(screen), LOGPIXELSY) as f64 / 96.0;
        ReleaseDC(None, screen);
        let make_font = |height: i32, weight, name| {
            CreateFontW(
                (height as f64 * scale) as i32,
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
                CLEARTYPE_QUALITY,
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
            telemetry: HWND::default(),
            detail: HWND::default(),
            detail_pid: None,
            detail_created: None,
            detail_pending: false,
            displayed_rows: Vec::new(),
            pid: HWND::default(),
            slow: HWND::default(),
            sort: HWND::default(),
            controls: Vec::new(),
            font: make_font(-14, 400, w!("Segoe UI")),
            page: 0,
            scale,
            heading: make_font(-27, 600, w!("Segoe UI")),
            taskbar: 0,
        };
        let state = Box::new(WindowState {
            app: std::cell::RefCell::new(app),
            deferred_event: std::cell::Cell::new(false),
        });
        let mut work_area = RECT::default();
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&mut work_area as *mut RECT).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let initial_width =
            ((1160.0 * scale) as i32).min((work_area.right - work_area.left).max(800));
        let initial_height =
            ((850.0 * scale) as i32).min((work_area.bottom - work_area.top).max(600));
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            w!("SuperOpti | On-demand performance diagnostics"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            initial_width,
            initial_height,
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
            if msg.message == WM_KEYDOWN
                && msg.wParam.0 == 13
                && msg.hwnd == state.app.borrow().table
            {
                let _ = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(116), LPARAM(0));
                continue;
            }
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let app = state.app.borrow();
        let _ = app.tx.send(Command::Quit);
        let _ = handle.join();
        for f in [app.font, app.heading] {
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
