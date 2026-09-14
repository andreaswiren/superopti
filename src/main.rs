#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(unsafe_op_in_unsafe_fn)]
mod actions;
mod devices;
mod elevation;
mod engine;
mod frame;
mod graphs;
mod icons;
mod inspect;
mod metrics;
mod model;
mod native;
mod network;
mod open_files;
mod theme;
mod traffic;
mod ui;
use metrics::{Sample, fmt, wide};
use std::{
    collections::{HashMap, HashSet, VecDeque},
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
    Metadata(u32, Option<u64>),
    Network(u32, Option<u64>),
    Connections(u32, Option<u64>),
    Files(u32, Option<u64>),
    Children(u32, Option<u64>),
    Services(u32, Option<u64>),
    Reveal(u32, Option<u64>, Option<String>),
    TrafficStart,
    TrafficStop,
    TrafficHistory(u64, traffic::Filter),
    Devices(bool),
    DeviceChanges,
    Auto(bool),
    Install,
    Quit,
    Collected(u64, String, Event),
    CollectorEnded(u64, String),
    Deadline(u64),
}
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq)]
enum CaptureEnd {
    Completed,
    Stopped,
    Failed,
}
#[derive(serde::Serialize, serde::Deserialize)]
enum Event {
    Started(String),
    Sample(Box<Sample>),
    Stopped(CaptureEnd, String),
    Report(String),
    ProcessReport(u32, model::Report),
    Metadata(u32, Option<u64>, inspect::ProcessMetadata),
    Structured(usize, model::Report),
    TrafficState(bool),
    TrafficUpdate(model::Report),
    TrafficHistory(u64, model::Report),
    ActionStatus(String),
}
struct App {
    tx: Sender<Command>,
    rx: Receiver<Event>,
    history: VecDeque<Sample>,
    active: bool,
    capture_end: Option<CaptureEnd>,
    status: String,
    report: HWND,
    table: HWND,
    telemetry: HWND,
    detail: HWND,
    detail_pid: Option<u32>,
    detail_created: Option<u64>,
    detail_pending: bool,
    process_tab: usize,
    displayed_rows: Vec<(u32, Option<u64>)>,
    metadata_cache: HashMap<(u32, Option<u64>), inspect::ProcessMetadata>,
    metadata_pending: HashSet<(u32, Option<u64>)>,
    process_search: String,
    pid: HWND,
    slow: HWND,
    sort: HWND,
    controls: Vec<(HWND, i32, i32, i32, i32)>,
    font: HFONT,
    page: usize,
    presentation: model::Report,
    net_active: bool,
    net_pending: bool,
    traffic_pages: usize,
    traffic_page: usize,
    traffic_follow: bool,
    traffic_live: model::Report,
    traffic_request: u64,
    traffic_error: String,
    debug_view: bool,
    pinned: bool,
    compact: bool,
    opacity: u8,
    full_rect: Option<WINDOWPLACEMENT>,
    action_status: String,
    exclude_health_checks: bool,
    core_scroll: usize,
    reports: [model::Report; 7],
    scale: f64,
    heading: HFONT,
    taskbar: u32,
}
struct WindowState {
    app: std::cell::RefCell<App>,
    deferred_event: std::cell::Cell<bool>,
}
fn request_metadata(app: &mut App, key: (u32, Option<u64>)) {
    if !app.metadata_cache.contains_key(&key)
        && app.metadata_pending.len() < 16
        && app.metadata_pending.insert(key)
    {
        let _ = app.tx.send(Command::Metadata(key.0, key.1));
    }
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
    if matches!(id, 197..=199) {
        match id {
            197 => {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
            198 => {
                let _ = ShowWindow(
                    hwnd,
                    if IsZoomed(hwnd).as_bool() {
                        SW_RESTORE
                    } else {
                        SW_MAXIMIZE
                    },
                );
            }
            _ => {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }
    if matches!(id, 194..=196) {
        if let Ok(combo) = GetDlgItem(Some(hwnd), 180) {
            SendMessageW(combo, CB_SETCURSEL, Some(WPARAM(id - 194)), None);
        }
        let _ = InvalidateRect(Some(hwnd), None, false);
        return;
    }
    if id == 192 {
        if let Ok(search) = GetDlgItem(Some(hwnd), 192) {
            let mut value = [0u16; 256];
            let length = GetWindowTextW(search, &mut value);
            app.process_search = String::from_utf16_lossy(&value[..length as usize])
                .trim()
                .to_lowercase();
            update_table(app);
        }
        return;
    }
    if id == 308 {
        if app.detail_pending || (app.page != 0 && app.detail_pid.is_some()) {
            app.page = 3;
            app.presentation = app.reports[3].clone();
            ui::update_report(app);
            ui::layout(app, hwnd);
        } else if ui::selected_pid(app).is_some() {
            ui::details(app, hwnd);
        } else {
            app.page = 3;
            app.presentation = if app.detail_pid.is_some() {
                app.reports[3].clone()
            } else {
                model::Report::outcome(
                    "Process explorer",
                    "Select a process in Overview and double-click to inspect it.".into(),
                )
            };
            ui::update_report(app);
            ui::layout(app, hwnd);
        }
        return;
    }
    if matches!(id, 181..=183) {
        if id == 181 {
            app.pinned = !app.pinned;
            if !app.pinned {
                app.opacity = 100;
            }
            let _ = SetWindowPos(
                hwnd,
                Some(if app.pinned {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                }),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        } else if id == 182 {
            app.compact = !app.compact;
            if app.compact {
                let mut placement = WINDOWPLACEMENT {
                    length: size_of::<WINDOWPLACEMENT>() as u32,
                    ..Default::default()
                };
                if GetWindowPlacement(hwnd, &mut placement).is_ok() {
                    app.full_rect = Some(placement);
                }
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let compact_rect = RECT {
                    right: (480. * app.scale) as i32,
                    bottom: (508. * app.scale) as i32,
                    ..Default::default()
                };

                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    compact_rect.right - compact_rect.left,
                    compact_rect.bottom - compact_rect.top,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            } else if let Some(placement) = app.full_rect.take() {
                let _ = SetWindowPlacement(hwnd, &placement);
            }
        } else if let Ok(combo) = GetDlgItem(Some(hwnd), 183) {
            let selected = SendMessageW(combo, CB_GETCURSEL, None, None).0;
            app.opacity = [100, 85, 70, 50]
                .get(selected as usize)
                .copied()
                .unwrap_or(100);
            if app.opacity < 100 && !app.pinned {
                app.pinned = true;
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
        if theme::palette().high_contrast {
            app.opacity = 100;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if app.pinned && app.opacity < 100 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
            let _ = SetLayeredWindowAttributes(
                hwnd,
                COLORREF(0),
                (u16::from(app.opacity) * 255 / 100) as u8,
                LWA_ALPHA,
            );
        } else {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED.0 as isize));
        }
        ui::layout(app, hwnd);
        ui::update_table(app);
        let _ = InvalidateRect(Some(hwnd), None, false);
        return;
    }
    if id == 176 {
        message(
            hwnd,
            "Pressure score (0–100)\n\nA relative ranking clue, not proof that a process caused the slowdown. The score is the largest of:\n\n• CPU percentage\n• GPU percentage\n• RAM share × memory weight\n• Private commit / system commit limit × 100 × commit weight\n• min(100, I/O MiB/s × 2) × disk weight\n\nMemory and commit weights are 1 when their system usage reaches 85%, otherwise 0.2. Disk weight is 1 at 80% disk busy, otherwise 0.2. Missing measurements do not contribute; incomplete evidence can understate pressure.\n\nRAM is resident working-set memory. Commit is private committed memory, backed by RAM or the pagefile. Windows PagefileUsage reports commit, not actual swapped-out bytes. Per-process swap bytes are unavailable in this capture; commit minus RAM is not a valid swap measurement.",
            MB_OK | MB_ICONINFORMATION,
        );
        return;
    }
    if id == 132 {
        app.exclude_health_checks = !app.exclude_health_checks;
        ui::layout(app, hwnd);
        ui::update_report(app);
        let _ = InvalidateRect(Some(hwnd), None, false);
        return;
    }
    if id == 190 {
        app.reports[0] = model::Report::default();
        app.presentation = model::Report::default();
        ui::update_report(app);
        ui::layout(app, hwnd);
        return;
    }
    if app.detail_pending && matches!(id, 110 | 116 | 117 | 118 | 119 | 186..=188) {
        return;
    }
    if let Some(index) = [116, 117, 119, 186, 187, 188]
        .iter()
        .position(|&tab| tab == id)
    {
        app.process_tab = index;
    }
    if id == 304 {
        app.page = 0;
        app.presentation = app.reports[0].clone();
        ui::update_report(app);
        ui::layout(app, hwnd);
        let _ =
            windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(GetDlgItem(Some(hwnd), 301).ok());
        return;
    }
    if (301..=303).contains(&id) {
        app.page = id - 301;
        app.presentation = app.reports[app.page].clone();
        ui::update_report(app);
        ui::layout(app, hwnd);
        return;
    }
    if id == 305 || id == 306 {
        app.page = id - 301;
        app.presentation = app.reports[app.page].clone();
        ui::update_report(app);
        ui::layout(app, hwnd);
        return;
    }
    if id == 120 {
        app.debug_view = !app.debug_view;
        ui::update_report(app);
        ui::layout(app, hwnd);
        return;
    }
    if id == 173 {
        for id in [163, 164, 165] {
            if let Ok(h) = GetDlgItem(Some(hwnd), id) {
                set_text(h, "");
            }
        }
        for id in [168, 169] {
            if let Ok(h) = GetDlgItem(Some(hwnd), id) {
                SendMessageW(h, CB_SETCURSEL, Some(WPARAM(0)), None);
            }
        }
    }
    let cmd = match id {
        150 => Some(Command::Devices(false)),
        151 => Some(Command::Devices(true)),
        153 => Some(Command::DeviceChanges),
        160 => {
            app.traffic_follow = true;
            app.traffic_request += 1;
            app.traffic_error.clear();
            app.traffic_live = model::Report::new("Starting network capture", &[]);
            app.net_pending = true;
            ui::layout(app, hwnd);
            Some(Command::TrafficStart)
        }
        161 => {
            let _ = app.tx.send(Command::TrafficStop);
            None
        }
        162 | 166 | 171 | 172 | 173 => {
            app.traffic_follow = false;
            app.traffic_request += 1;
            match traffic_filter(hwnd) {
                Ok(mut filter) => {
                    app.traffic_follow = false;
                    app.traffic_page = match id {
                        171 => app.traffic_page.saturating_sub(1),
                        172 => app.traffic_page.saturating_add(1),
                        _ => 0,
                    };
                    filter.page = app.traffic_page;
                    Some(Command::TrafficHistory(app.traffic_request, filter))
                }
                Err(e) => {
                    app.presentation = model::Report::error(e);
                    ui::update_report(app);
                    None
                }
            }
        }
        167 => {
            export_report(app);
            let _ = InvalidateRect(Some(hwnd), None, false);
            None
        }
        191 => {
            app.traffic_follow = true;
            app.traffic_request += 1;
            app.reports[5] = app.traffic_live.clone();
            app.presentation = app.traffic_live.clone();
            ui::update_report(app);
            ui::layout(app, hwnd);
            None
        }
        189 => {
            app.page = 6;
            app.core_scroll = 0;
            ui::layout(app, hwnd);
            let _ = InvalidateRect(Some(hwnd), None, false);
            None
        }
        116 if app.page == 3 => app
            .detail_pid
            .map(|pid| Command::Details(pid, app.detail_created)),
        116 => {
            ui::details(app, hwnd);
            None
        }
        117 => app
            .detail_pid
            .map(|pid| Command::Threads(pid, app.detail_created)),
        119 => app
            .detail_pid
            .map(|pid| Command::Connections(pid, app.detail_created)),
        184 => app
            .detail_pid
            .map(|pid| Command::Reveal(pid, app.detail_created, None)),
        185 => {
            let selected = SendMessageW(
                app.detail,
                LVM_GETNEXTITEM,
                Some(WPARAM(usize::MAX)),
                Some(LPARAM(LVNI_SELECTED as isize)),
            )
            .0;
            let column = app
                .presentation
                .columns
                .iter()
                .position(|c| c == "Path" || c == "Executable");
            let path = usize::try_from(selected)
                .ok()
                .and_then(|row| app.presentation.rows.get(row))
                .and_then(|row| column.and_then(|col| row.get(col)))
                .cloned();
            app.detail_pid
                .zip(path)
                .map(|(pid, path)| Command::Reveal(pid, app.detail_created, Some(path)))
        }
        186 => app
            .detail_pid
            .map(|pid| Command::Files(pid, app.detail_created)),
        187 => app
            .detail_pid
            .map(|pid| Command::Children(pid, app.detail_created)),
        188 => app
            .detail_pid
            .map(|pid| Command::Services(pid, app.detail_created)),
        118 => {
            if message(
                hwnd,
                "Record TCP and UDP traffic for 2 minutes? This capture records process and endpoint byte totals for all processes locally. Windows will request administrator approval if required. No packet payloads are stored. Use the process filter in Traffic history to focus the results.",
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
                    102 => 300,
                    103 => 900,
                    _ => {
                        let selected = SendMessageW(
                            GetDlgItem(Some(hwnd), 180).unwrap_or_default(),
                            CB_GETCURSEL,
                            None,
                            None,
                        )
                        .0;
                        match selected {
                            1 => 300,
                            2 => 900,
                            _ => 120,
                        }
                    }
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
        if matches!(c, Command::Network(..)) {
            app.traffic_follow = true;
            app.traffic_request += 1;
            app.traffic_error.clear();
            app.traffic_live = model::Report::new("Starting network capture", &[]);
            app.page = 5;
            app.net_pending = true;
            app.presentation = app.reports[5].clone();
            ui::update_report(app);
            ui::layout(app, hwnd);
            let _ = app.tx.send(c);
            return;
        }
        if matches!(c, Command::Reveal(..)) {
            app.action_status = "Opening containing folder…".into();
            let _ = app.tx.send(c);
            let _ = InvalidateRect(Some(hwnd), None, false);
            return;
        }
        let focused_request = matches!(
            c,
            Command::Details(_, _)
                | Command::Threads(_, _)
                | Command::Network(_, _)
                | Command::Connections(_, _)
                | Command::Files(_, _)
                | Command::Children(_, _)
                | Command::Services(_, _)
                | Command::Reveal(_, _, _)
        );
        if focused_request {
            app.detail_pending = true;
            ui::layout(app, hwnd);
        }
        app.presentation = model::Report::new("Collecting", &["Status", "Operation"]);
        app.presentation.row(&["Working", "On-demand request"]);
        if focused_request {
            app.reports[3] = app.presentation.clone();
        }
        ui::update_report(app);
        let _ = app.tx.send(c);
    }
}
unsafe fn traffic_filter(hwnd: HWND) -> Result<traffic::Filter, String> {
    let value = |id| {
        let h = GetDlgItem(Some(hwnd), id).unwrap_or_default();
        let mut b = [0u16; 1024];
        let len = GetWindowTextW(h, &mut b);
        String::from_utf16_lossy(&b[..len as usize])
    };
    let selected = |id| {
        SendMessageW(
            GetDlgItem(Some(hwnd), id).unwrap_or_default(),
            CB_GETCURSEL,
            None,
            None,
        )
        .0
        .max(0) as usize
    };
    let from = traffic::parse_time(&value(164))?;
    let to = traffic::parse_time(&value(165))?;
    if from.zip(to).is_some_and(|(a, b)| a > b) {
        return Err("From must be earlier than To".into());
    }
    Ok(traffic::Filter {
        search: value(163),
        from,
        to,
        protocol: selected(168),
        group: selected(169),
        page: 0,
    })
}
fn export_report(app: &mut App) {
    let result = (|| -> Result<String, String> {
        let folder = actions::data_dir().join("exports");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let path = folder.join(format!(
            "view-{}-{}.json",
            metrics::now(),
            std::process::id()
        ));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(file, &app.presentation).map_err(|e| e.to_string())?;
        Ok(path.display().to_string())
    })();
    app.action_status = match result {
        Ok(path) => format!("Current page exported: {path}"),
        Err(e) => format!("Export failed: {e}"),
    };
}
fn export(app: &mut App) {
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
        app.presentation = model::Report::outcome("Export", result.unwrap_or_else(|e| e));
        app.reports[0] = app.presentation.clone();
        ui::update_report(app);
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
    if let Some(result) = frame::handle(hwnd, msg, wparam, lparam) {
        return result;
    }
    if msg == WM_DESTROY {
        ui::clear_accessible_names();
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
    if let Some(result) = theme::control_color(msg, wparam) {
        return result;
    }
    if msg == WM_DRAWITEM && lparam.0 != 0 {
        theme::draw_button(&*(lparam.0 as *const DRAWITEMSTRUCT));
        return LRESULT(1);
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
        tray(hwnd, NIM_ADD, app.active || app.net_active);
        return LRESULT(0);
    }
    let result = match msg {
        WM_CREATE => {
            app.scale = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) as f64 / 96.0;
            init(app, hwnd);
            theme::apply(app, hwnd);
            frame::install(hwnd);
            LRESULT(0)
        }
        WM_NOTIFY => {
            if let Some(result) = theme::custom_draw(app, lparam) {
                return result;
            }
            let notification = &*(lparam.0 as *const NMHDR);
            if notification.idFrom == 280 {
                if notification.code == TCN_SELCHANGING {
                    return LRESULT(app.detail_pending as isize);
                }
                if notification.code == TCN_SELCHANGE {
                    let index = SendMessageW(notification.hwndFrom, TCM_GETCURSEL, None, None).0;
                    if let Some(&id) = [116, 117, 119, 186, 187, 188].get(index as usize) {
                        command(app, hwnd, id);
                    }
                    return LRESULT(0);
                }
            }
            if notification.hwndFrom == app.table && notification.code == LVN_GETINFOTIPW {
                let tip = &mut *(lparam.0 as *mut NMLVGETINFOTIPW);
                if let Some(key) = app.displayed_rows.get(tip.iItem as usize).copied() {
                    let value = app
                        .metadata_cache
                        .get(&key)
                        .map(|m| m.tooltip())
                        .unwrap_or_else(|| "Loading executable details…".into());
                    request_metadata(app, key);
                    if !tip.pszText.is_null() && tip.cchTextMax > 0 {
                        let text = wide(&value);
                        let n = text
                            .len()
                            .saturating_sub(1)
                            .min(tip.cchTextMax as usize - 1);
                        std::ptr::copy_nonoverlapping(text.as_ptr(), tip.pszText.0, n);
                        *tip.pszText.0.add(n) = 0;
                    }
                }
                return LRESULT(0);
            }
            if notification.hwndFrom == app.report
                && notification.code == LVN_BEGINSCROLL
                && app.page == 5
            {
                app.traffic_follow = false;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            if notification.hwndFrom == app.detail && notification.code == LVN_ITEMCHANGED {
                ui::update_file_actions(app);
            }
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
            // Opening, focusing and closing a combo must not relayout it.
            if matches!(wparam.0 & 0xffff, 109 | 168 | 169 | 180 | 183)
                && (wparam.0 >> 16) as u32 != CBN_SELCHANGE
            {
                return LRESULT(0);
            }
            command(app, hwnd, wparam.0 & 0xffff);
            LRESULT(0)
        }
        EVENT => {
            while let Ok(event) = app.rx.try_recv() {
                match event {
                    Event::Started(s) => {
                        app.capture_end = None;
                        app.reports[0] = model::Report::default();
                        app.presentation = model::Report::default();
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
                    Event::Stopped(reason, s) => {
                        app.active = false;
                        app.capture_end = Some(reason);
                        app.status = s;
                        tray(hwnd, NIM_MODIFY, app.net_active);
                    }
                    Event::Report(s) => {
                        app.reports[0] = model::Report::error(s);
                        if app.page == 0 {
                            app.presentation = app.reports[0].clone();
                            ui::update_report(app);
                        }
                    }
                    Event::ActionStatus(status) => {
                        app.action_status = status;
                    }
                    Event::TrafficState(active) => {
                        app.net_active = active;
                        app.net_pending = false;
                        tray(hwnd, NIM_MODIFY, app.active || app.net_active);
                        ui::layout(app, hwnd);
                    }
                    Event::Metadata(pid, created, metadata) => {
                        let key = (pid, created);
                        app.metadata_pending.remove(&key);
                        ui::cache_process_icon(pid, created, &metadata.path);
                        if app.metadata_cache.len() >= 512 {
                            app.metadata_cache.retain(|k, _| {
                                app.displayed_rows.contains(k) || Some(k.0) == app.detail_pid
                            });
                        }
                        app.metadata_cache.insert(key, metadata);
                        ui::update_process_tooltip(app, hwnd);
                        update_table(app);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    Event::TrafficUpdate(report) => {
                        if report.title == "Request failed" {
                            app.traffic_error = report
                                .rows
                                .iter()
                                .filter_map(|r| r.last())
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(" · ");
                            app.traffic_live.title = "Capture incomplete".into();
                        } else {
                            if let Some(error) =
                                report.metrics.iter().find(|m| m.label == "Capture error")
                            {
                                app.traffic_error = error.value.clone();
                            }
                            app.traffic_live = report;
                        }
                        if app.traffic_follow {
                            app.reports[5] = app.traffic_live.clone();
                            if app.page == 5 {
                                app.presentation = app.traffic_live.clone();
                                ui::update_report(app);
                            }
                        }
                    }
                    Event::TrafficHistory(request, report) => {
                        if request == app.traffic_request && !app.traffic_follow {
                            if let Some(metric) = report.metrics.iter().find(|m| m.label == "Page")
                                && let Some((current, pages)) = metric.value.split_once('/')
                            {
                                app.traffic_page = current
                                    .trim()
                                    .parse::<usize>()
                                    .unwrap_or(1)
                                    .saturating_sub(1);
                                app.traffic_pages = pages.trim().parse::<usize>().unwrap_or(1);
                            }
                            app.reports[5] = report;
                            if app.page == 5 {
                                app.presentation = app.reports[5].clone();
                                ui::update_report(app);
                            }
                        }
                    }
                    Event::Structured(page, report) => {
                        if page == 5
                            && let Some(metric) = report.metrics.iter().find(|m| m.label == "Page")
                            && let Some((current, pages)) = metric.value.split_once('/')
                        {
                            app.traffic_page = current
                                .trim()
                                .parse::<usize>()
                                .unwrap_or(1)
                                .saturating_sub(1);
                            app.traffic_pages = pages.trim().parse::<usize>().unwrap_or(1);
                        }
                        app.reports[page] = report;
                        if app.page == page {
                            app.presentation = app.reports[page].clone();
                            ui::update_report(app);
                        }
                    }
                    Event::ProcessReport(pid, s) => {
                        if app.detail_pid == Some(pid) {
                            app.detail_pending = false;
                            app.reports[3] = s;
                            if app.page == 3 {
                                app.presentation = app.reports[3].clone();
                                ui::update_report(app);
                            }
                        }
                    }
                }
            }
            ui::layout(app, hwnd);
            for key in app.displayed_rows.clone() {
                request_metadata(app, key);
            }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK if ui::cpu_hit(app, hwnd, lparam) => {
            app.page = 6;
            app.presentation = model::Report::new(
                "Logical processor usage",
                &["Processor group / logical CPU", "Usage %"],
            );
            if let Some(sample) = app.history.back() {
                let mut rows: Vec<_> = sample.cores.iter().collect();
                rows.sort_by(|a, b| a.0.cmp(b.0));
                for (core, usage) in rows {
                    app.presentation.row(&[core, &format!("{usage:.1}")]);
                }
            }
            app.reports[6] = app.presentation.clone();
            ui::update_report(app);
            ui::layout(app, hwnd);
            LRESULT(0)
        }
        WM_MOUSEWHEEL | WM_VSCROLL if app.page == 6 && !app.compact => {
            let max = ui::core_scroll_max(app, hwnd);
            if msg == WM_MOUSEWHEEL {
                let delta = (wparam.0 >> 16) as u16 as i16;
                app.core_scroll = if delta > 0 {
                    app.core_scroll.saturating_sub(1)
                } else {
                    (app.core_scroll + 1).min(max)
                };
            } else {
                let code = (wparam.0 & 0xffff) as u32;
                app.core_scroll = match code {
                    0 => app.core_scroll.saturating_sub(1),
                    1 => (app.core_scroll + 1).min(max),
                    2 => app.core_scroll.saturating_sub(3),
                    3 => (app.core_scroll + 3).min(max),
                    4 | 5 => {
                        let mut info = SCROLLINFO {
                            cbSize: size_of::<SCROLLINFO>() as u32,
                            fMask: SIF_TRACKPOS,
                            ..Default::default()
                        };
                        let _ = GetScrollInfo(hwnd, SB_VERT, &mut info);
                        (info.nTrackPos.max(0) as usize).min(max)
                    }
                    6 => 0,
                    7 => max,
                    _ => app.core_scroll,
                };
            }
            ui::layout(app, hwnd);
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_PAINT => {
            paint(app, hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_SYSCOLORCHANGE => {
            let _ = PostMessageW(Some(hwnd), WM_APP + 10, WPARAM(0), LPARAM(0));
            LRESULT(0)
        }
        value if value == WM_APP + 10 => {
            theme::apply(app, hwnd);
            if theme::palette().high_contrast {
                app.opacity = 100;
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED.0 as isize));
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let dpi = (wparam.0 & 0xffff) as u32;
            let previous = app.scale;
            app.scale = dpi as f64 / 96.0;
            let new_font = CreateFontW(
                (-11.0 * app.scale).round() as i32,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                DEFAULT_PITCH.0 as u32,
                theme::font_face(),
            );
            for &(h, ..) in &app.controls {
                SendMessageW(
                    h,
                    WM_SETFONT,
                    Some(WPARAM(new_font.0 as usize)),
                    Some(LPARAM(0)),
                );
            }
            let _ = DeleteObject(app.font.into());
            app.font = new_font;
            for index in 0..10 {
                let width =
                    SendMessageW(app.table, LVM_GETCOLUMNWIDTH, Some(WPARAM(index)), None).0;
                SendMessageW(
                    app.table,
                    LVM_SETCOLUMNWIDTH,
                    Some(WPARAM(index)),
                    Some(LPARAM(
                        (width as f64 * app.scale / previous).round() as isize
                    )),
                );
            }
            let r = &*(lparam.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                None,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            ui::layout(app, hwnd);
            LRESULT(0)
        }
        WM_SIZE => {
            if wparam.0 == SIZE_MINIMIZED as usize {
                return LRESULT(0);
            }
            ui::layout(app, hwnd);
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let m = &mut *(lparam.0 as *mut MINMAXINFO);
            let rect = RECT {
                left: 0,
                top: 0,
                right: ((if app.compact { 420.0 } else { 1020.0 }) * app.scale) as i32,
                bottom: ((if app.compact { 488.0 } else { 680.0 }) * app.scale) as i32,
            };
            m.ptMinTrackSize = POINT {
                x: rect.right - rect.left,
                y: rect.bottom - rect.top,
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
        _ => {
            // DefWindowProc enters nested move/resize loops and synchronously sends
            // WM_SIZE. Release the borrow so those messages can paint and lay out.
            drop(guard);
            if state.deferred_event.replace(false) {
                let _ = PostMessageW(Some(hwnd), EVENT, WPARAM(0), LPARAM(0));
            }
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
    };
    drop(guard);
    if state.deferred_event.replace(false) {
        let _ = PostMessageW(Some(hwnd), EVENT, WPARAM(0), LPARAM(0));
    }
    result
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--network-elevated") {
        if let (Some(pid), Some(id), Some(seconds)) = (
            args.get(2).and_then(|s| s.parse().ok()),
            args.get(3),
            args.get(4).and_then(|s| s.parse().ok()),
        ) {
            let _ = elevation::helper(pid, id, seconds);
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--network-watchdog") {
        if let (Some(pid), Some(name), Some(seconds)) = (
            args.get(2).and_then(|s| s.parse().ok()),
            args.get(3),
            args.get(4).and_then(|s| s.parse().ok()),
        ) {
            traffic::watchdog(pid, name, seconds);
        }
        return;
    }
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
    if args.get(1).map(String::as_str) == Some("--metadata-probe") {
        if let Some(pid) = args.get(2).and_then(|v| v.parse::<u32>().ok()) {
            let data = inspect::metadata(pid, args.get(3).and_then(|s| s.parse().ok()));
            println!("{}", serde_json::to_string(&data).unwrap_or_default());
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--open-files-probe") {
        if let Some(pid) = args.get(2).and_then(|v| v.parse::<u32>().ok()) {
            println!(
                "{}",
                serde_json::to_string(
                    &open_files::snapshot(pid, args.get(3).and_then(|s| s.parse().ok()))
                        .unwrap_or_else(model::Report::error)
                )
                .unwrap()
            );
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--thread-probe") {
        if let Some(pid) = args.get(2).and_then(|v| v.parse::<u32>().ok()) {
            println!(
                "{}",
                serde_json::to_string(
                    &native::thread_report(pid).unwrap_or_else(model::Report::error)
                )
                .unwrap()
            );
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--network-smoke-test") {
        let result = traffic::smoke_test();
        let value = match &result {
            Ok(v) => v.clone(),
            Err(e) => serde_json::json!({"error":e}),
        };
        if let Some(path) = args.get(2) {
            let _ = std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap());
        }
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    if std::env::args().any(|v| v == "--smoke-test") {
        smoke();
        return;
    }
    unsafe {
        // One UI per interactive session, including portable/installed copies.
        let test = cfg!(debug_assertions) && args.iter().any(|v| v.starts_with("--ui-test"));
        let mutex_text = if test {
            format!("Local\\SuperOpti.UITest.{}", std::process::id())
        } else {
            "Local\\SuperOpti.SingleInstance".into()
        };
        let mutex_name = wide(&mutex_text);
        let mutex = CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr()));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(h) = FindWindowW(CLASS, None) {
                let _ = ShowWindow(h, SW_RESTORE);
                let _ = SetForegroundWindow(h);
            }
            return;
        }
        let _mutex = mutex.expect("single-instance mutex");

        let instance = GetModuleHandleW(None).expect("module");
        let class = WNDCLASSW {
            style: CS_DBLCLKS,
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
            capture_end: None,
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
            process_tab: 0,
            displayed_rows: Vec::new(),
            metadata_cache: HashMap::new(),
            metadata_pending: HashSet::new(),
            process_search: String::new(),
            pid: HWND::default(),
            slow: HWND::default(),
            sort: HWND::default(),
            controls: Vec::new(),
            font: make_font(-11, 400, theme::font_face()),
            page: 0,
            presentation: model::Report::default(),
            net_active: false,
            net_pending: false,
            traffic_follow: true,
            traffic_live: model::Report::default(),
            traffic_request: 0,
            traffic_error: String::new(),
            traffic_pages: 1,
            traffic_page: 0,
            debug_view: false,
            pinned: false,
            compact: false,
            opacity: 100,
            full_rect: None,
            action_status: String::new(),
            exclude_health_checks: false,
            core_scroll: 0,
            reports: std::array::from_fn(|_| model::Report::default()),
            scale,
            heading: make_font(-20, 600, theme::font_face()),
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
            ((1380.0 * scale) as i32).min((work_area.right - work_area.left).max(800));
        let initial_height =
            ((980.0 * scale) as i32).min((work_area.bottom - work_area.top).max(600));
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            if test {
                w!("SuperOpti | Signal design validation")
            } else {
                w!("SuperOpti | On-demand performance diagnostics")
            },
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
