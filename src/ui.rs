//! Event-driven native dashboard. Logical layout units are scaled at system DPI.
use super::*;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;

pub unsafe fn icon(id: usize) -> HICON {
    LoadIconW(
        GetModuleHandleW(None).ok().map(Into::into),
        PCWSTR(id as *const u16),
    )
    .unwrap_or_default()
}

pub unsafe fn tray_icon(id: usize) -> HICON {
    LoadImageW(
        GetModuleHandleW(None).ok().map(Into::into),
        PCWSTR(id as *const u16),
        IMAGE_ICON,
        GetSystemMetrics(SM_CXSMICON),
        GetSystemMetrics(SM_CYSMICON),
        LR_SHARED,
    )
    .map(|image| HICON(image.0))
    .unwrap_or_default()
}

unsafe fn button(app: &mut App, hwnd: HWND, id: usize, label: &str, rect: (i32, i32, i32, i32)) {
    add_control(
        app,
        hwnd,
        w!("BUTTON"),
        label,
        id,
        WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
        rect,
    );
}

pub unsafe fn init(app: &mut App, hwnd: HWND) {
    let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_LISTVIEW_CLASSES,
    });
    for (id, label, x, width) in [
        (197, "Minimize window", 0, 46),
        (198, "Maximize window", 0, 46),
        (199, "Close to tray", 0, 46),
        (308, "Processes", 24, 140),
        (194, "2 min", 24, 46),
        (195, "5 min", 24, 46),
        (196, "15 min", 24, 48),
        (301, "Overview", 24, 140),
        (302, "System health", 172, 150),
        (303, "Settings", 330, 130),
        (305, "Devices", 24, 144),
        (306, "Traffic history", 24, 144),
    ] {
        button(app, hwnd, id, label, (x, 64, width, 32));
    }
    for (id, label, x, width) in [
        (101, "Start capture", 24, 126),
        (102, "Capture 5 min", 158, 126),
        (103, "Capture 15 min", 292, 130),
        (104, "Stop", 430, 76),
        (105, "Export", 514, 116),
    ] {
        button(app, hwnd, id, label, (x, 112, width, 34));
    }
    add_control(
        app,
        hwnd,
        w!("EDIT"),
        "",
        192,
        WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        (0, 0, 174, 28),
    );
    if let Ok(search) = GetDlgItem(Some(hwnd), 192) {
        let cue = wide("Search process or PID");
        SendMessageW(
            search,
            EM_SETCUEBANNER,
            Some(WPARAM(1)),
            Some(LPARAM(cue.as_ptr() as isize)),
        );
    }
    let tabs = add_control(
        app,
        hwnd,
        w!("SysTabControl32"),
        "Process detail views",
        280,
        WS_TABSTOP | WINDOW_STYLE(TCS_OWNERDRAWFIXED | TCS_FIXEDWIDTH),
        (0, 0, 840, 36),
    );
    for (index, label) in [
        "Summary",
        "Threads",
        "Connections",
        "Open files",
        "Child processes",
        "Services",
    ]
    .iter()
    .enumerate()
    {
        let mut label = wide(label);
        let tab = TCITEMW {
            mask: TCIF_TEXT,
            pszText: windows::core::PWSTR(label.as_mut_ptr()),
            ..Default::default()
        };
        SendMessageW(
            tabs,
            TCM_INSERTITEMW,
            Some(WPARAM(index)),
            Some(LPARAM(&tab as *const _ as isize)),
        );
    }
    app.slow = add_control(
        app,
        hwnd,
        w!("BUTTON"),
        "5s sample interval",
        108,
        WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        (774, 112, 176, 34),
    );
    app.sort = add_control(
        app,
        hwnd,
        w!("COMBOBOX"),
        "Rank contributors",
        109,
        WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        (24, 380, 214, 220),
    );
    for label in [
        "Pressure score",
        "CPU usage",
        "RAM usage",
        "I/O rate",
        "GPU usage",
        "Handle count",
        "Thread count",
    ] {
        let value = wide(label);
        SendMessageW(
            app.sort,
            CB_ADDSTRING,
            None,
            Some(LPARAM(value.as_ptr() as isize)),
        );
    }
    SendMessageW(app.sort, CB_SETCURSEL, Some(WPARAM(0)), None);
    app.table = add_control(
        app,
        hwnd,
        w!("SysListView32"),
        "Top ten contributors",
        201,
        WS_TABSTOP | WINDOW_STYLE(LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS),
        (24, 420, 1080, 220),
    );
    SendMessageW(
        app.table,
        LVM_SETEXTENDEDLISTVIEWSTYLE,
        None,
        Some(LPARAM(
            (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | LVS_EX_LABELTIP | LVS_EX_INFOTIP)
                as isize,
        )),
    );
    for (index, (label, width)) in [
        ("Process", 185),
        ("PID", 65),
        ("CPU %", 72),
        ("RAM MiB", 88),
        ("Commit MiB", 96),
        ("I/O MiB/s", 88),
        ("GPU %", 72),
        ("Threads", 72),
        ("Handles", 80),
        ("Score", 66),
        ("Observed cores", 124),
        ("Swap", 108),
    ]
    .iter()
    .enumerate()
    {
        let mut label = wide(label);
        let col = LVCOLUMNW {
            mask: LVCF_TEXT | LVCF_WIDTH | LVCF_FMT,
            fmt: if index == 0 || index == 10 {
                LVCFMT_LEFT
            } else {
                LVCFMT_RIGHT
            },
            cx: (*width as f64 * app.scale) as i32,
            pszText: windows::core::PWSTR(label.as_mut_ptr()),
            ..Default::default()
        };
        SendMessageW(
            app.table,
            LVM_INSERTCOLUMNW,
            Some(WPARAM(index)),
            Some(LPARAM((&col as *const LVCOLUMNW) as isize)),
        );
    }
    add_control(
        app,
        hwnd,
        w!("STATIC"),
        "Process ID",
        210,
        WINDOW_STYLE(0),
        (24, 648, 76, 28),
    );
    app.pid = add_control(
        app,
        hwnd,
        w!("EDIT"),
        "",
        211,
        WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_NUMBER as u32),
        (104, 644, 95, 30),
    );
    button(app, hwnd, 110, "Inspect threads (2s)", (208, 644, 166, 30));
    button(app, hwnd, 116, "Open process details", (382, 644, 170, 30));
    for (id, label, rect) in [
        (106, "Run system checks", (24, 218, 182, 36)),
        (132, "Exclude optional checks", (594, 90, 182, 34)),
        (121, "Reduce animations", (24, 304, 170, 34)),
        (122, "Use Balanced power", (206, 304, 182, 34)),
        (130, "Set fixed pagefile", (400, 304, 182, 34)),
        (123, "Fix all (3 settings)", (594, 304, 182, 34)),
        (124, "Undo fixes", (788, 304, 130, 34)),
        (125, "Storage", (24, 396, 128, 34)),
        (126, "Startup apps", (164, 396, 142, 34)),
        (127, "Windows Update", (318, 396, 164, 34)),
        (128, "Virtual memory", (494, 396, 152, 34)),
        (129, "Power settings", (658, 396, 148, 34)),
        (111, "Enable autostart", (24, 244, 170, 36)),
        (112, "Disable autostart", (206, 244, 170, 36)),
        (113, "Install for this user", (24, 362, 182, 36)),
        (114, "Open captures folder", (218, 362, 182, 36)),
        (304, "Back to overview", (24, 174, 160, 32)),
        (117, "Threads", (198, 174, 238, 32)),
        (118, "Record traffic (2 min)", (448, 174, 208, 32)),
        (119, "Connections", (448, 174, 132, 32)),
        (120, "Debug view", (20, 400, 144, 34)),
        (150, "Refresh devices", (208, 96, 144, 34)),
        (151, "Discover LAN", (364, 96, 150, 34)),
        (153, "Changes", (526, 96, 126, 34)),
        (160, "Record 2 min", (208, 96, 130, 34)),
        (161, "Stop recording", (350, 96, 122, 34)),
        (162, "Load history", (484, 96, 122, 34)),
        (166, "Apply filters", (668, 260, 136, 32)),
        (167, "Export filtered", (618, 96, 138, 34)),
        (171, "Previous", (740, 600, 110, 34)),
        (172, "Next", (862, 600, 110, 34)),
        (173, "Clear", (862, 600, 80, 34)),
        (174, "Traffic history", (0, 0, 150, 34)),
        (175, "Observe cores (5s)", (0, 0, 170, 34)),
        (176, "?", (0, 0, 30, 30)),
        (181, "Pin", (0, 0, 60, 30)),
        (182, "Compact", (0, 0, 90, 30)),
        (184, "Open executable folder", (0, 0, 190, 32)),
        (185, "Open file folder", (0, 0, 140, 32)),
        (186, "Open files", (0, 0, 112, 32)),
        (187, "Child processes", (0, 0, 140, 32)),
        (188, "Services", (0, 0, 96, 32)),
        (190, "Dismiss result", (0, 0, 132, 32)),
        (191, "Follow live", (0, 0, 140, 32)),
        (189, "CPU details", (0, 0, 132, 32)),
    ] {
        button(app, hwnd, id, label, rect);
    }
    for (id, label) in [
        (263, "Search process, address or port"),
        (264, "From UTC · blank = all"),
        (265, "To UTC · blank = all"),
        (268, "Protocol"),
        (269, "Group results by"),
    ] {
        add_control(
            app,
            hwnd,
            w!("STATIC"),
            label,
            id,
            WINDOW_STYLE(0),
            (208, 184, 200, 20),
        );
    }
    for (id, cue) in [
        (163, "Process name, IP address or port"),
        (164, "YYYY-MM-DDTHH:MM:SSZ"),
        (165, "YYYY-MM-DDTHH:MM:SSZ"),
    ] {
        let edit = add_control(
            app,
            hwnd,
            w!("EDIT"),
            "",
            id,
            WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            (208, 206, 210, 30),
        );
        let cue = wide(cue);
        SendMessageW(
            edit,
            EM_SETCUEBANNER,
            Some(WPARAM(1)),
            Some(LPARAM(cue.as_ptr() as isize)),
        );
        SendMessageW(
            edit,
            EM_SETLIMITTEXT,
            Some(WPARAM(if id == 163 { 256 } else { 32 })),
            None,
        );
    }
    for (id, items) in [
        (180, vec!["2 min", "5 min", "15 min"]),
        (183, vec!["100%", "85%", "70%", "50%"]),
        (168, vec!["All protocols", "TCP", "UDP"]),
        (
            169,
            vec!["Process + endpoint", "Process", "Remote endpoint"],
        ),
    ] {
        if id == 180 {
            add_control(
                app,
                hwnd,
                w!("STATIC"),
                "Duration",
                270,
                WINDOW_STYLE(0),
                (170, 50, 116, 16),
            );
        }
        let combo = add_control(
            app,
            hwnd,
            w!("COMBOBOX"),
            "",
            id,
            WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            (208, 206, 210, 150),
        );
        for item in items {
            let item = wide(item);
            SendMessageW(
                combo,
                CB_ADDSTRING,
                None,
                Some(LPARAM(item.as_ptr() as isize)),
            );
        }
        SendMessageW(combo, CB_SETCURSEL, Some(WPARAM(0)), None);
    }
    add_control(
        app,
        hwnd,
        w!("EDIT"),
        "",
        205,
        WS_TABSTOP
            | WS_VSCROLL
            | WS_HSCROLL
            | WINDOW_STYLE(
                ES_MULTILINE as u32
                    | ES_READONLY as u32
                    | ES_AUTOVSCROLL as u32
                    | ES_AUTOHSCROLL as u32,
            ),
        (208, 470, 720, 100),
    );
    app.report = add_control(
        app,
        hwnd,
        w!("SysListView32"),
        "Results",
        202,
        WS_TABSTOP | WINDOW_STYLE(LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS),
        (24, 694, 1080, 72),
    );
    app.telemetry = add_control(
        app,
        hwnd,
        w!("STATIC"),
        "Disk, network, paging and scheduler metrics appear here during capture.",
        204,
        WINDOW_STYLE(0),
        (24, 674, 1080, 36),
    );
    app.detail = add_control(
        app,
        hwnd,
        w!("SysListView32"),
        "Process details",
        203,
        WS_TABSTOP | WINDOW_STYLE(LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS),
        (24, 232, 1080, 500),
    );
    for table in [app.report, app.detail] {
        SendMessageW(
            table,
            LVM_SETEXTENDEDLISTVIEWSTYLE,
            None,
            Some(LPARAM(
                (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | LVS_EX_LABELTIP) as isize,
            )),
        );
    }
    update_report(app);
    app.taskbar = RegisterWindowMessageW(w!("TaskbarCreated"));
    name_accessible_controls(hwnd);
    tray(hwnd, NIM_ADD, false);
    layout(app, hwnd);
}

fn display_report(app: &App) -> std::borrow::Cow<'_, model::Report> {
    if app.page == 3 && app.detail_pending {
        let mut report = model::Report::new(
            format!(
                "Loading {}",
                [
                    "Summary",
                    "Threads",
                    "Connections",
                    "Open files",
                    "Child processes",
                    "Services"
                ]
                .get(app.process_tab)
                .unwrap_or(&"process view")
            ),
            &["Status", "Details"],
        );
        report.row(&["Loading", "Waiting for the requested process view"]);
        return std::borrow::Cow::Owned(report);
    }

    if app.page == 1 && app.exclude_health_checks && !app.presentation.rows.is_empty() {
        let mut report = app.presentation.clone();
        report
            .rows
            .retain(|row| !matches!(row.first().map(String::as_str), Some("Optional" | "Review")));
        if let Some(metric) = report
            .metrics
            .iter_mut()
            .find(|m| m.label == "System health")
        {
            let penalty: i32 = report
                .rows
                .iter()
                .map(|row| match row.first().map(String::as_str) {
                    Some("Critical") => 25,
                    Some("Blocked") => 20,
                    Some("Warning") => 10,
                    Some("Unknown") => 5,
                    _ => 0,
                })
                .sum();
            metric.value = format!("{}%", (100 - penalty).clamp(0, 100));
        }
        return std::borrow::Cow::Owned(report);
    }
    if !app.presentation.title.is_empty()
        || !app.presentation.columns.is_empty()
        || !app.presentation.rows.is_empty()
        || !app.presentation.metrics.is_empty()
    {
        return std::borrow::Cow::Borrowed(&app.presentation);
    }
    let mut report = match app.page {
        0 => model::Report::new("Capture status", &["Status", "Action"]),
        1 => model::Report::new(
            "System checks",
            &[
                "Status",
                "Check",
                "Value",
                "Target / context",
                "Action",
                "Impact",
            ],
        ),
        2 => model::Report::new("Preferences", &["Setting", "Status", "Action"]),
        3 => model::Report::new(
            "Process snapshot",
            &["Category", "Metric", "Value", "Scope"],
        ),
        4 => model::Report::new(
            "Network inventory",
            &[
                "IP address",
                "Hostname",
                "MAC address",
                "Status",
                "Last seen",
            ],
        ),
        _ => model::Report::new(
            "Traffic history",
            &[
                "Process",
                "Protocol",
                "Local endpoint",
                "Remote endpoint",
                "Received",
                "Sent",
            ],
        ),
    };
    match app.page {
        0 => report.row(&["Not captured", "Start a short capture"]),
        1 => {
            for check in [
                "Disk space",
                "Pagefile target",
                "Power plan",
                "Startup apps",
            ] {
                report.row(&["Not checked", check, "Not captured", "Run system checks"]);
            }
        }
        2 => {
            report.row(&["Autostart", "Not queried", "Enable or disable above"]);
            report.row(&["Installation", "Not queried", "Install for this user"]);
        }
        3 => {
            for label in ["CPU", "Memory", "Threads", "Handles"] {
                report.metric(label, "Not captured");
            }
        }
        4 => {
            report.metric("Devices", "Not captured");
            report.metric("Last refresh", "Not captured");
            report.metric("Discovery", "Not run");
        }
        _ => {
            for label in ["Received", "Sent", "Flows"] {
                report.metric(label, "Not captured");
            }
        }
    }
    std::borrow::Cow::Owned(report)
}

fn report_top(app: &App, _height: i32) -> i32 {
    match app.page {
        0 => overview_table_bottom(_height) + 16,
        1 => 386,
        2 => 366,
        3 => 338,
        4 => 292,
        5 if !app.traffic_error.is_empty() => 498,
        _ => 446,
    }
}

fn geometry(width: i32) -> (i32, i32, i32) {
    let shell = width.min(1800);
    let origin = (width - shell) / 2;
    (origin, origin + 156, shell - 180)
}

pub unsafe fn layout(app: &App, hwnd: HWND) {
    theme::set_page(app.page);
    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    let width = (r.right as f64 / app.scale) as i32;
    let height = (r.bottom as f64 / app.scale) as i32 - 28;
    let (origin, x, content) = if app.compact {
        (0, 16, width - 32)
    } else {
        geometry(width)
    };
    let table_y = if app.compact { 286 } else { 284 };
    let widths = [160, 62, 68, 88, 96, 94, 68, 76, 84, 68, 124, 108];
    let table_was_visible = IsWindowVisible(app.table).as_bool();
    let sizing_header = HWND(SendMessageW(app.table, LVM_GETHEADER, None, None).0 as *mut _);
    SendMessageW(app.table, WM_SETREDRAW, Some(WPARAM(0)), None);
    SendMessageW(sizing_header, WM_SETREDRAW, Some(WPARAM(0)), None);
    for (column, width) in widths.into_iter().enumerate() {
        let width = if app.compact {
            match column {
                0 => (content - 148).max(140),
                2 => 70,
                9 => 70,
                _ => 0,
            }
        } else {
            width
        };
        SendMessageW(
            app.table,
            LVM_SETCOLUMNWIDTH,
            Some(WPARAM(column)),
            Some(LPARAM((width as f64 * app.scale) as isize)),
        );
    }
    SendMessageW(sizing_header, WM_SETREDRAW, Some(WPARAM(1)), None);
    SendMessageW(app.table, WM_SETREDRAW, Some(WPARAM(1)), None);
    if !table_was_visible {
        let _ = ShowWindow(app.table, SW_HIDE);
    }
    for &(h, _, _, _, _) in &app.controls {
        let id = GetDlgCtrlID(h);
        let visible = if app.compact {
            matches!(id, 101 | 104 | 181 | 182 | 183 | 194..=199)
                || (id == 201 && !app.history.is_empty())
        } else {
            match id {
                102 | 103 | 107 | 115 | 117 | 119 | 186..=188 | 204 => false,
                194..=196 | 192 => app.page == 0,
                180 | 270 => false,
                101 | 104 | 105 | 108 => app.page == 0,
                109 | 176 => app.page == 0,
                210 | 211 | 110 | 116 => false,
                201 => app.page == 0 && !app.history.is_empty(),
                106 | 121..=130 | 132 => app.page == 1,
                111..=114 => app.page == 2,
                203 => app.page == 3 && !app.debug_view,
                304 => matches!(app.page, 3 | 6),
                118 | 174 | 184 | 185 | 280 => app.page == 3,
                175 => matches!(app.page, 3 | 6),
                202 => {
                    (app.page == 0 && !app.presentation.rows.is_empty())
                        || (app.page > 0 && !matches!(app.page, 3 | 6) && !app.debug_view)
                }
                190 => app.page == 0 && !app.presentation.rows.is_empty(),
                189 | 308 => true,
                205 => app.debug_view && app.page > 0,
                120 => app.page > 0,
                150 | 151 | 153 => app.page == 4,
                160..=169 | 171..=173 | 191 | 263..=265 | 268 | 269 => app.page == 5,
                _ => true,
            }
        };
        let _ = ShowWindow(h, if visible { SW_SHOW } else { SW_HIDE });
        let rect = if app.compact {
            match id {
                197 => (width - 138, 0, 46, 28),
                198 => (width - 92, 0, 46, 28),
                199 => (width - 46, 0, 46, 28),
                181 => (width - 160, 18, 28, 28),
                182 => (width - 124, 18, 28, 28),
                183 => (width - 84, 18, 68, 28),
                101 => (16, 64, 104, 32),
                194 => (132, 64, 46, 32),
                195 => (178, 64, 46, 32),
                196 => (224, 64, 48, 32),
                104 => (width - 116, 64, 100, 32),
                180 => (170, 66, 116, 140),
                270 => (170, 50, 116, 16),
                201 => (17, table_y, content - 2, (height - table_y - 66).max(100)),
                190 => (140, height - 52, 132, 32),
                _ => continue,
            }
        } else {
            match id {
                197 => (width - 138, 0, 46, 28),
                198 => (width - 92, 0, 46, 28),
                199 => (width - 46, 0, 46, 28),
                301 => (origin + 10, 88, 120, 36),
                189 => (origin + 10, 132, 120, 36),
                308 => (origin + 10, 176, 120, 36),
                302 => (origin + 10, height - 132, 120, 36),
                305 => (origin + 10, 264, 120, 36),
                306 => (origin + 10, 220, 120, 36),
                303 => (origin + 10, height - 88, 120, 36),
                120 => (origin + 10, height - 44, 120, 28),
                107 => (origin + 10, height - 88, 120, 32),
                115 => (origin + 10, height - 48, 120, 32),
                101 => (x, 84, 116, 32),
                194 => (x + 126, 84, 46, 32),
                195 => (x + 172, 84, 46, 32),
                196 => (x + 218, 84, 48, 32),
                192 => (x + content - 190, 245, 174, 28),
                180 => (x + 218, 90, 116, 160),
                104 => (x + 336, 84, 64, 32),
                105 => (x + content - 82, 84, 82, 32),
                108 => (x + 410, 84, 190, 32),
                109 => (x + 248, 245, 160, 180),
                176 => (x + 418, 245, 30, 28),
                201 => (
                    x + 1,
                    table_y,
                    content - 2,
                    (overview_table_bottom(height) - table_y - 8).max(140),
                ),
                210 => (x, height - 60, 72, 22),
                211 => (x + 76, height - 66, 82, 30),
                110 => (x + 170, height - 66, 164, 32),
                116 => (x + 346, height - 66, 178, 32),
                202 | 203 | 205 => {
                    let y = report_top(app, height);
                    if app.page == 0 {
                        (x + 1, y, content - 2, 126)
                    } else {
                        (x + 12, y, content - 24, report_body_height(app, height))
                    }
                }
                106 => (x, 90, 170, 34),
                132 => (x + 560, 90, 182, 34),
                123 => (x + 182, 90, 174, 34),
                124 => (x + 368, 90, 112, 34),
                121 => (x + 16, 218, (content - 24) / 3 - 32, 32),
                122 => (
                    x + (content + 12) / 3 + 16,
                    218,
                    (content - 24) / 3 - 32,
                    32,
                ),
                130 => (
                    x + 2 * (content + 12) / 3 + 16,
                    218,
                    (content - 24) / 3 - 32,
                    32,
                ),
                125 => (x, 302, 106, 32),
                126 => (x + 114, 302, 126, 32),
                127 => (x + 248, 302, 154, 32),
                128 => (x + 410, 302, 142, 32),
                129 => (x + 560, 302, 142, 32),
                111 => (x + 18, 214, 154, 34),
                112 => (x + 182, 214, 154, 34),
                113 => (x + content / 2 + 24, 214, 160, 34),
                114 => (x + content / 2 + 24, 260, 174, 34),
                280 => (x, 142, content, 36),
                304 => (x, 90, 148, 34),
                117 => (x + 160, 90, 96, 34),
                119 => (x + 268, 90, 124, 34),
                118 => (x + 160, 90, 184, 34),
                190 => (x + 536, height - 66, 132, 32),
                181 => (x + content - 148, 28, 28, 28),
                182 => (x + content - 112, 28, 28, 28),
                183 => (x + content - 76, 28, 90, 28),
                174 => (x, height - 52, 140, 32),
                184 => (x + 338, height - 52, 186, 32),
                185 => (x + 536, height - 52, 144, 32),
                186 => (x + 16, 252, 110, 32),
                187 => (x + 138, 252, 140, 32),
                188 => (x + 290, 252, 96, 32),
                175 => {
                    if app.page == 6 {
                        (x + 160, 90, 178, 34)
                    } else {
                        (x + 152, height - 52, 174, 32)
                    }
                }
                150 => (x, 90, 146, 34),
                151 => (x + 158, 90, 144, 34),
                153 => (x + 314, 90, 112, 34),
                160 => (x, 90, 142, 34),
                161 => (x + 154, 90, 130, 34),
                162 => (x + 296, 90, 124, 34),
                167 => (x + 432, 90, 142, 34),
                191 => (x + 586, 90, 140, 34),
                263 => (x + 16, 146, 300, 20),
                268 => (x + 338, 146, 120, 20),
                269 => (x + 478, 146, content - 496, 20),
                163 => (x + 16, 170, 306, 30),
                168 => (x + 338, 170, 124, 160),
                169 => (x + 478, 170, content - 496, 160),
                264 => (x + 16, 212, 210, 20),
                265 => (x + 242, 212, 210, 20),
                164 => (x + 16, 236, 210, 30),
                165 => (x + 242, 236, 210, 30),
                166 => (x + 468, 236, 132, 30),
                173 => (x + 612, 236, 82, 30),
                171 => (x + content - 224, height - 52, 106, 32),
                172 => (x + content - 106, height - 52, 106, 32),
                _ => continue,
            }
        };
        let (a, mut b, c, d) = rect;
        if !matches!(id, 197..=199) {
            b += 28;
        }
        if id == 280 {
            SendMessageW(h, TCM_SETCURSEL, Some(WPARAM(app.process_tab)), None);
            let tw = (((content - 12) / 6) as f64 * app.scale).round() as usize;
            let th = (34. * app.scale).round() as usize;
            SendMessageW(
                h,
                TCM_SETITEMSIZE,
                None,
                Some(LPARAM((tw | (th << 16)) as isize)),
            );
            let _ = EnableWindow(h, app.detail_pid.is_some() && !app.detail_pending);
        }
        if id == 198 {
            set_text(
                h,
                if IsZoomed(hwnd).as_bool() {
                    "Restore window"
                } else {
                    "Maximize window"
                },
            );
        }
        let scale = |v: i32| (v as f64 * app.scale).round() as i32;
        let _ = MoveWindow(h, scale(a), scale(b), scale(c), scale(d), false);
        if matches!(id, 101 | 108 | 180 | 194..=196) {
            let _ = EnableWindow(h, !app.active);
        }
        if id == 104 {
            let _ = EnableWindow(
                h,
                app.active || (app.compact && (app.net_active || app.net_pending)),
            );
        }
        if id == 105 {
            let _ = EnableWindow(h, !app.history.is_empty());
        }
        if matches!(id, 110 | 116 | 117 | 118 | 119) {
            let _ = EnableWindow(h, !app.detail_pending);
        }
        if id == 160 {
            let _ = EnableWindow(h, !app.net_active && !app.net_pending);
        }
        if id == 161 {
            let _ = EnableWindow(h, app.net_active);
        }
        if id == 191 {
            let _ = EnableWindow(h, !app.net_pending && !app.traffic_live.columns.is_empty());
            set_text(
                h,
                if app.traffic_follow && app.net_active {
                    "Following live"
                } else if !app.net_active {
                    "Latest capture"
                } else {
                    "Follow live"
                },
            );
        }
        if id == 171 {
            let _ = EnableWindow(h, app.traffic_page > 0);
        }
        if id == 172 {
            let _ = EnableWindow(h, app.traffic_page + 1 < app.traffic_pages);
        }
        if id == 104 {
            set_text(h, if app.compact { "Stop all" } else { "Stop" });
        }
        if id == 181 {
            set_text(h, if app.pinned { "Pinned" } else { "Pin" });
        }
        if id == 182 {
            set_text(h, if app.compact { "Full view" } else { "Compact" });
        }
        if id == 183 {
            let _ = EnableWindow(h, app.pinned && !theme::palette().high_contrast);
            SendMessageW(
                h,
                CB_SETCURSEL,
                Some(WPARAM(match app.opacity {
                    85 => 1,
                    70 => 2,
                    50 => 3,
                    _ => 0,
                })),
                None,
            );
        }
        if matches!(id, 175 | 184 | 186..=188) {
            let _ = EnableWindow(h, !app.detail_pending);
        }
        if id == 120 {
            set_text(
                h,
                if app.debug_view {
                    "Hide debug"
                } else {
                    "Debug view"
                },
            );
        }
        if id == 132 {
            let _ = SendMessageW(
                h,
                BM_SETCHECK,
                Some(WPARAM(if app.exclude_health_checks { 1 } else { 0 })),
                None,
            );
        }
    }
    let max_scroll = core_scroll_max(app, hwnd);
    let _ = ShowScrollBar(
        hwnd,
        SB_VERT,
        app.page == 6 && !app.compact && max_scroll > 0,
    );
    if app.page == 6 {
        let info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: max_scroll as i32,
            nPage: 1,
            nPos: app.core_scroll.min(max_scroll) as i32,
            ..Default::default()
        };
        SetScrollInfo(hwnd, SB_VERT, &info, true);
    }
    update_file_actions(app);
    update_process_tooltip(app, hwnd);
    theme::size_rows(app);
    size_report_columns(app);
    for table in [app.table, app.report, app.detail] {
        let header = HWND(SendMessageW(table, LVM_GETHEADER, None, None).0 as *mut _);
        let _ = InvalidateRect(Some(header), None, false);
    }
    let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_ALLCHILDREN);
}

pub unsafe fn update_report(app: &App) {
    let report = display_report(app);
    if app.debug_view
        && let Some(&(debug, _, _, _, _)) = app
            .controls
            .iter()
            .find(|entry| GetDlgCtrlID(entry.0) == 205)
    {
        set_text(
            debug,
            &serde_json::to_string_pretty(&app.presentation)
                .unwrap_or_else(|error| error.to_string()),
        );
    }
    for table in [if app.page == 3 {
        app.detail
    } else {
        app.report
    }] {
        if table.0.is_null() {
            continue;
        }
        SendMessageW(table, WM_SETREDRAW, Some(WPARAM(0)), None);
        SendMessageW(table, LVM_DELETEALLITEMS, None, None);
        while SendMessageW(table, LVM_DELETECOLUMN, Some(WPARAM(0)), None).0 != 0 {}
        for (index, label) in report.columns.iter().enumerate() {
            let numeric = matches!(
                label.as_str(),
                "PID" | "TID" | "Threads" | "Handles" | "Events" | "Local port" | "Remote port"
            ) || label.contains("bytes")
                || label.contains("MiB")
                || label.contains("KiB")
                || label.contains("%");
            let mut label = wide(label);
            let column = LVCOLUMNW {
                mask: LVCF_TEXT | LVCF_WIDTH | LVCF_FMT,
                fmt: if numeric { LVCFMT_RIGHT } else { LVCFMT_LEFT },
                cx: 160,
                pszText: windows::core::PWSTR(label.as_mut_ptr()),
                ..Default::default()
            };
            SendMessageW(
                table,
                LVM_INSERTCOLUMNW,
                Some(WPARAM(index)),
                Some(LPARAM((&column as *const LVCOLUMNW) as isize)),
            );
        }
        for (row, cells) in report.rows.iter().enumerate() {
            for (column, cell) in cells.iter().take(report.columns.len()).enumerate() {
                let mut value = wide(cell);
                let item = LVITEMW {
                    mask: LVIF_TEXT,
                    iItem: row as i32,
                    iSubItem: column as i32,
                    pszText: windows::core::PWSTR(value.as_mut_ptr()),
                    ..Default::default()
                };
                SendMessageW(
                    table,
                    if column == 0 {
                        LVM_INSERTITEMW
                    } else {
                        LVM_SETITEMW
                    },
                    None,
                    Some(LPARAM((&item as *const LVITEMW) as isize)),
                );
            }
        }
        SendMessageW(table, WM_SETREDRAW, Some(WPARAM(1)), None);
        let _ = InvalidateRect(Some(table), None, false);
    }
    size_report_columns(app);
}

unsafe fn size_report_columns(app: &App) {
    let report = display_report(app);
    let columns = &report.columns;
    if columns.is_empty() {
        return;
    }
    let weights: Vec<i32> = columns
        .iter()
        .map(|name| match name.as_str() {
            "Local endpoint" | "Remote endpoint" | "Hostname" | "Host name" => 3,
            "Scope" | "Details" | "Activity" | "Value" => 3,
            "Result" => 4,
            "Metric" | "Check" | "Action" | "Operation" | "Process" | "IP address"
            | "MAC address" | "First seen" | "Last seen" => 2,
            _ => 1,
        })
        .collect();
    let total: i32 = weights.iter().sum();
    for table in [app.report, app.detail] {
        if table.0.is_null() {
            continue;
        }
        let mut rect = RECT::default();
        let _ = GetClientRect(table, &mut rect);
        let width = (rect.right - (18.0 * app.scale) as i32).max(100);
        for (column, weight) in weights.iter().enumerate() {
            if app.page == 1 {
                let logical = (width as f64 / app.scale) as i32;
                let desired = match columns[column].as_str() {
                    "Status" => 96,
                    "Check" => 190,
                    "Current" => 190,
                    "Target" => (logical - 626).max(190),
                    "Action" => 150,
                    _ => 150,
                };
                SendMessageW(
                    table,
                    LVM_SETCOLUMNWIDTH,
                    Some(WPARAM(column)),
                    Some(LPARAM((desired as f64 * app.scale) as isize)),
                );
                continue;
            }
            SendMessageW(
                table,
                LVM_SETCOLUMNWIDTH,
                Some(WPARAM(column)),
                Some(LPARAM((width * weight / total).max(
                    ((if columns[column].contains("endpoint")
                        || columns[column].contains("seen")
                        || columns[column].contains("UTC")
                    {
                        210.0
                    } else {
                        100.0
                    }) * app.scale) as i32,
                ) as isize)),
            );
        }
    }
}

pub unsafe fn update_table(app: &mut App) {
    let Some(s) = app.history.back() else { return };
    let selected = selected_identity(app);
    let sort = SendMessageW(app.sort, CB_GETCURSEL, None, None).0;
    let mut rows: Vec<_> = s
        .processes
        .iter()
        .filter(|p| {
            app.process_search.is_empty()
                || p.name.to_lowercase().contains(&app.process_search)
                || p.pid.to_string().contains(&app.process_search)
        })
        .collect();
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
    SendMessageW(app.table, WM_SETREDRAW, Some(WPARAM(0)), None);
    SendMessageW(app.table, LVM_DELETEALLITEMS, None, None);
    for (index, p) in rows
        .iter()
        .take(if app.compact { 3 } else { 10 })
        .enumerate()
    {
        let whole = |v: Option<f64>| v.map(|v| format!("{v:.0}")).unwrap_or_else(|| "N/A".into());
        let cells = [
            p.name.clone(),
            p.pid.to_string(),
            fmt(p.cpu, ""),
            fmt(p.ram_mb, ""),
            fmt(p.commit_mb, ""),
            fmt(p.io_mb, ""),
            fmt(p.gpu, ""),
            whole(p.threads),
            whole(p.handles),
            format!("{:.1}", p.score),
            "Not captured".into(),
            "Unavailable".into(),
        ];
        for (column, cell) in cells.iter().enumerate() {
            let mut text = wide(cell);
            let item = LVITEMW {
                mask: LVIF_TEXT
                    | if column == 0 {
                        LVIF_PARAM | LVIF_STATE
                    } else {
                        LIST_VIEW_ITEM_FLAGS(0)
                    },
                iItem: index as i32,
                iSubItem: column as i32,
                pszText: windows::core::PWSTR(text.as_mut_ptr()),
                lParam: LPARAM(p.pid as isize),
                state: if selected == Some((p.pid, p.created_ticks)) {
                    LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0)
                } else {
                    LIST_VIEW_ITEM_STATE_FLAGS(0)
                },
                stateMask: LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0),
                ..Default::default()
            };
            SendMessageW(
                app.table,
                if column == 0 {
                    LVM_INSERTITEMW
                } else {
                    LVM_SETITEMW
                },
                None,
                Some(LPARAM((&item as *const LVITEMW) as isize)),
            );
        }
    }
    app.displayed_rows = rows
        .iter()
        .take(if app.compact { 3 } else { 10 })
        .map(|p| (p.pid, p.created_ticks))
        .collect();
    set_text(
        app.pid,
        &selected
            .filter(|identity| app.displayed_rows.contains(identity))
            .map(|(pid, _)| pid.to_string())
            .unwrap_or_default(),
    );
    SendMessageW(app.table, WM_SETREDRAW, Some(WPARAM(1)), None);
    let _ = InvalidateRect(Some(app.table), None, false);
}

pub unsafe fn selected_pid(app: &App) -> Option<u32> {
    let row = SendMessageW(
        app.table,
        LVM_GETNEXTITEM,
        Some(WPARAM(usize::MAX)),
        Some(LPARAM(LVNI_SELECTED as isize)),
    )
    .0;
    if row < 0 {
        return None;
    }
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
    Some(item.lParam.0 as u32)
}

pub unsafe fn details(app: &mut App, hwnd: HWND) {
    if app.detail_pending {
        return;
    }
    let Some((pid, created)) = selected_identity(app) else {
        message(hwnd, "Select a contributor in the table first.", MB_OK);
        return;
    };
    app.process_tab = 0;
    let process = app
        .history
        .back()
        .and_then(|s| s.processes.iter().find(|p| p.pid == pid));
    let name = process.map(|p| p.name.as_str()).unwrap_or("Process");
    app.presentation = model::Report {
        title: format!("{name} · PID {pid}"),
        metrics: Vec::new(),
        columns: vec!["Status".into(), "Activity".into()],
        rows: vec![vec![
            "Collecting".into(),
            "Requesting a fresh process snapshot".into(),
        ]],
        debug: String::new(),
    };
    app.reports[3] = app.presentation.clone();
    app.detail_pid = Some(pid);
    app.detail_created = created;
    app.detail_pending = true;
    app.page = 3;
    update_report(app);
    layout(app, hwnd);
    let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(app.detail));
    let _ = app.tx.send(Command::Details(pid, created));
}

unsafe fn selected_identity(app: &App) -> Option<(u32, Option<u64>)> {
    let pid = selected_pid(app)?;
    app.displayed_rows
        .iter()
        .find(|identity| identity.0 == pid)
        .copied()
}

unsafe fn label(dc: HDC, x: i32, y: i32, value: &str) {
    text(dc, x, y, value, theme::palette().muted);
}

fn tint(base: COLORREF, accent: COLORREF, weight: u32) -> COLORREF {
    let channel = |shift: u32| {
        (((base.0 >> shift) & 255) * (100 - weight) + ((accent.0 >> shift) & 255) * weight) / 100
    };
    color(channel(0), channel(8), channel(16))
}

unsafe fn panel(dc: HDC, rect: RECT, background: COLORREF, border: COLORREF) {
    let mut logical = SIZE::default();
    let mut device = SIZE::default();
    let _ = GetWindowExtEx(dc, &mut logical);
    let _ = GetViewportExtEx(dc, &mut device);
    let scale = if logical.cx != 0 {
        device.cx as f32 / logical.cx as f32
    } else {
        1.
    };
    graphs::rounded_rect(dc, scale, rect, background, border, 8.);
}

pub unsafe fn paint(app: &App, hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let target = BeginPaint(hwnd, &mut ps);
    let mut bounds = RECT::default();
    let _ = GetClientRect(hwnd, &mut bounds);
    if bounds.right <= 0 || bounds.bottom <= 0 {
        let _ = EndPaint(hwnd, &ps);
        return;
    }
    let dc = CreateCompatibleDC(Some(target));
    if dc.0.is_null() {
        fill(target, &bounds, theme::palette().bg);
        let _ = EndPaint(hwnd, &ps);
        return;
    }
    let bitmap = CreateCompatibleBitmap(target, bounds.right, bounds.bottom);
    if bitmap.0.is_null() {
        fill(target, &bounds, theme::palette().bg);
        let _ = DeleteDC(dc);
        let _ = EndPaint(hwnd, &ps);
        return;
    }
    let original_bitmap = SelectObject(dc, bitmap.into());
    let width = (bounds.right as f64 / app.scale) as i32;
    let height = (bounds.bottom as f64 / app.scale) as i32;
    let palette = theme::palette();
    let report = display_report(app);
    SetMapMode(dc, MM_ANISOTROPIC);
    let _ = SetWindowExtEx(dc, width, height, None);
    let _ = SetViewportExtEx(dc, bounds.right, bounds.bottom, None);
    fill(
        dc,
        &RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        },
        palette.bg,
    );
    if !app.compact {
        let (origin, _, _) = geometry(width);
        fill(
            dc,
            &RECT {
                left: origin,
                top: 0,
                right: origin + 140,
                bottom: 28,
            },
            if palette.dark {
                color(28, 30, 40)
            } else {
                color(248, 250, 254)
            },
        );
        // Keep the rail/content divider continuous through the custom caption.
        fill(
            dc,
            &RECT {
                left: origin + 139,
                top: 0,
                right: origin + 140,
                bottom: 28,
            },
            palette.border,
        );
    }
    let _ = OffsetViewportOrgEx(dc, 0, (28.0 * app.scale).round() as i32, None);
    let height = height - 28;
    let (origin, x, content) = geometry(width);
    if !app.compact {
        let rail = if palette.dark {
            color(28, 30, 40)
        } else {
            color(248, 250, 254)
        };
        fill(
            dc,
            &RECT {
                left: origin,
                top: 0,
                right: origin + 140,
                bottom: height,
            },
            rail,
        );
        fill(
            dc,
            &RECT {
                left: origin + 139,
                top: 0,
                right: origin + 140,
                bottom: height,
            },
            palette.border,
        );
    }
    SetBkMode(dc, TRANSPARENT);
    let make_font = |size, weight| {
        CreateFontW(
            size,
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
            theme::font_face(),
        )
    };
    let font = make_font(-11, 400);
    let heading = make_font(-20, 600);
    let value_font = make_font(-23, 600);
    let semibold = make_font(-12, 600);
    let old = SelectObject(dc, semibold.into());
    let (state, state_color) = if app.net_pending {
        ("Permission pending", theme::metric_colors()[3])
    } else if app.net_active {
        ("Network live", theme::metric_colors()[1])
    } else if app.active {
        ("Capturing", theme::metric_colors()[0])
    } else if !app.traffic_error.is_empty() {
        ("Needs attention", theme::metric_colors()[3])
    } else {
        match app.capture_end {
            Some(CaptureEnd::Completed) => ("Capture complete", theme::metric_colors()[1]),
            Some(CaptureEnd::Stopped) => ("Stopped", theme::metric_colors()[1]),
            Some(CaptureEnd::Failed) => ("Capture failed", theme::metric_colors()[3]),
            None => ("Idle", palette.surface),
        }
    };
    // Paint in caption coordinates while the content viewport starts below it.
    graphs::rounded_rect(
        dc,
        app.scale as f32,
        RECT {
            left: width - 276,
            top: -24,
            right: width - 148,
            bottom: -4,
        },
        state_color,
        state_color,
        5.,
    );
    let mut status = wide(state);
    let mut sr = RECT {
        left: width - 274,
        top: -24,
        right: width - 150,
        bottom: -4,
    };
    SetTextColor(
        dc,
        if state == "Idle" {
            palette.muted
        } else {
            color(22, 24, 30)
        },
    );
    SetBkMode(dc, TRANSPARENT);
    DrawTextW(
        dc,
        &mut status,
        &mut sr,
        DT_SINGLELINE | DT_VCENTER | DT_CENTER,
    );

    if !app.compact {
        icons::draw(
            dc,
            "audio-lines",
            RECT {
                left: origin + 20,
                top: 8,
                right: origin + 36,
                bottom: 24,
            },
            theme::metric_colors()[0],
        );
        SelectObject(dc, font.into());
        text(dc, origin + 44, 8, "SuperOpti", palette.text);
        label(dc, x, 8, "Workspace");
        SelectObject(dc, heading.into());
        text(
            dc,
            x,
            30,
            match app.page {
                0 => "Performance overview",
                1 => "System health",
                2 => "Preferences",
                3 => "Process explorer",
                4 => "Network devices",
                6 => "CPU details",
                _ => "Traffic history",
            },
            palette.text,
        );
        SelectObject(dc, font.into());
        label(
            dc,
            x,
            62,
            match app.page {
                0 => "Find the pressure behind a slowdown",
                1 => "Review your system. Make targeted improvements.",
                2 => "Configure how SuperOpti works for you",
                3 => "Inspect resource use and connection activity",
                4 => "Local IPv4 + IPv6 neighbors and network changes",
                6 => "Logical processor activity · last 120 seconds",
                _ => "Explore recorded TCP and UDP activity",
            },
        );
        if app.page == 3
            && let Some(pid) = app.detail_pid
            && let Some(name) = cached_process_name(pid, app.detail_created)
        {
            SelectObject(dc, font.into());
            let mut label = wide(&format!("{name} · PID {pid}"));
            let mut rect = RECT {
                left: x,
                top: 60,
                right: x + content - 260,
                bottom: 80,
            };
            fill(dc, &rect, palette.bg);
            if draw_cached_icon(
                app,
                dc,
                pid,
                app.detail_created,
                RECT {
                    left: x,
                    top: 62,
                    right: x + 16,
                    bottom: 78,
                },
            ) {
                rect.left += 24;
            }
            SetTextColor(dc, palette.muted);
            DrawTextW(dc, &mut label, &mut rect, DT_SINGLELINE | DT_END_ELLIPSIS);
        }
        label(dc, x + content - 76, 14, "Opacity");
    }
    let accents = theme::metric_colors();
    let metric_card = |left: i32, top: i32, w: i32, label0: &str, value: &str, index: usize| {
        let accent = if palette.high_contrast {
            palette.text
        } else {
            accents[index % 4]
        };
        panel(
            dc,
            RECT {
                left,
                top,
                right: left + w,
                bottom: top + 96,
            },
            palette.surface,
            palette.border,
        );
        if !palette.high_contrast {
            glow(
                dc,
                RECT {
                    left: left + w - 96,
                    top: top + 2,
                    right: left + w - 2,
                    bottom: top + 70,
                },
                palette.surface,
                accent,
            );
        }
        SelectObject(dc, font.into());
        text(dc, left + 14, top + 12, label0, palette.muted);
        SelectObject(
            dc,
            if value.len() > 15 {
                semibold
            } else {
                value_font
            }
            .into(),
        );
        let mut value = wide(value);
        let mut rect = RECT {
            left: left + 16,
            top: top + 34,
            right: left + w - 14,
            bottom: top + 69,
        };
        SetTextColor(dc, palette.text);
        DrawTextW(
            dc,
            &mut value,
            &mut rect,
            DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        SelectObject(dc, font.into());
        if app.page == 0 {
            let peak = app
                .history
                .iter()
                .filter_map(|s| match index {
                    0 => s.cpu,
                    1 => s.ram,
                    2 => s.gpu,
                    _ => s.disk,
                })
                .reduce(f64::max);
            label(
                dc,
                left + 14,
                top + 74,
                &if index == 1
                    && let Some(s) = app.history.back()
                {
                    format!(
                        "{} of {:.0} GiB installed",
                        fmt(s.ram, "%"),
                        s.total_ram_mb / 1024.
                    )
                } else if let Some(p) = peak {
                    format!("Peak {p:.1}% · capture")
                } else {
                    "Waiting for capture".into()
                },
            );
        }
    };
    if app.compact {
        paint_compact(app, dc, width, height, font, semibold, value_font);
    } else {
        if matches!(app.page, 1..=5) {
            panel(
                dc,
                RECT {
                    left: x,
                    top: report_top(app, height) - 42,
                    right: x + content,
                    bottom: report_top(app, height) + report_body_height(app, height) + 12,
                },
                palette.surface,
                palette.border,
            );
        }
        match app.page {
            0 => {
                panel(
                    dc,
                    RECT {
                        left: x + 124,
                        top: 82,
                        right: x + 268,
                        bottom: 118,
                    },
                    palette.surface,
                    palette.border,
                );
                label(dc, x + 274, 91, "Duration");
                let latest = app.history.back();
                let card = (content - 36) / 4;
                for (i, name) in ["CPU", "Memory", "GPU", "Disk busy"].iter().enumerate() {
                    let v = latest.and_then(|s| match i {
                        0 => s.cpu,
                        1 => s.ram,
                        2 => s.gpu,
                        _ => s.disk,
                    });
                    metric_card(
                        x + i as i32 * (card + 12),
                        124,
                        card,
                        name,
                        &if latest.is_none() {
                            "—".into()
                        } else if i == 1 {
                            latest
                                .and_then(|s| {
                                    s.ram.map(|r| {
                                        format!("{:.1} GiB", r / 100. * s.total_ram_mb / 1024.)
                                    })
                                })
                                .unwrap_or_else(|| "Unavailable".into())
                        } else {
                            fmt(v, "%")
                        },
                        i,
                    );
                }
                let chart_top = overview_table_bottom(height) + 16;
                let bottom = (chart_top + 220).min(height - 40);
                panel(
                    dc,
                    RECT {
                        left: x,
                        top: chart_top,
                        right: x + content - 190,
                        bottom,
                    },
                    palette.surface,
                    palette.border,
                );
                SelectObject(dc, semibold.into());
                text(dc, x + 16, chart_top + 12, "Resource history", palette.text);
                SelectObject(dc, font.into());
                label(dc, x + 180, chart_top + 15, "Last 120 seconds · %");
                let chart_right = x + content - 210;
                panel(
                    dc,
                    RECT {
                        left: x + content - 178,
                        top: chart_top,
                        right: x + content,
                        bottom,
                    },
                    palette.surface,
                    palette.border,
                );
                for (i, name) in ["CPU", "RAM", "GPU", "Disk"].iter().enumerate() {
                    let lx = x + 16 + i as i32 * 70;
                    fill(
                        dc,
                        &RECT {
                            left: lx,
                            top: chart_top + 43,
                            right: lx + 8,
                            bottom: chart_top + 51,
                        },
                        accents[i],
                    );
                    text(dc, lx + 13, chart_top + 39, name, palette.muted);
                }
                let graph = RECT {
                    left: x + 46,
                    top: chart_top + 62,
                    right: chart_right,
                    bottom: bottom - 32,
                };
                for (fraction, value) in [(0, "100"), (1, "50"), (2, "0")] {
                    let y = graph.top + (graph.bottom - graph.top) * fraction / 2;
                    label(dc, x + 12, y - 7, value);
                    fill(
                        dc,
                        &RECT {
                            left: graph.left,
                            top: y,
                            right: graph.right,
                            bottom: y + 1,
                        },
                        palette.border,
                    );
                }
                let axis_start = latest.map(|s| (s.elapsed - 120.).max(0.)).unwrap_or(0.);
                for tick in 0..=4 {
                    let px = graph.left + (graph.right - graph.left) * tick / 4;
                    let mut value = wide(&format!("{:.0}s", axis_start + tick as f64 * 30.));
                    let mut rect = RECT {
                        left: px - 20,
                        top: bottom - 23,
                        right: px + 28,
                        bottom: bottom - 6,
                    };
                    SetTextColor(dc, palette.muted);
                    DrawTextW(dc, &mut value, &mut rect, DT_SINGLELINE | DT_CENTER);
                }
                cpu_area(dc, app, graph, accents[0]);
                for (i, accent) in accents.iter().enumerate() {
                    series(dc, app, graph, *accent, |sample| match i {
                        0 => sample.cpu,
                        1 => sample.ram,
                        2 => sample.gpu,
                        _ => sample.disk,
                    });
                }
                let rx = x + content - 164;
                label(dc, rx, chart_top + 14, "Capture insight");
                SelectObject(dc, semibold.into());
                text(
                    dc,
                    rx,
                    chart_top + 39,
                    if latest.is_some() {
                        "Resource peaks"
                    } else {
                        "Awaiting capture"
                    },
                    palette.text,
                );
                SelectObject(dc, font.into());
                for (i, name) in ["CPU", "Memory", "Disk busy"].iter().enumerate() {
                    let peak = app
                        .history
                        .iter()
                        .filter_map(|s| match i {
                            0 => s.cpu,
                            1 => s.ram,
                            _ => s.disk,
                        })
                        .reduce(f64::max);
                    let y = chart_top + 72 + i as i32 * 27;
                    if y + 20 < bottom {
                        label(dc, rx, y, name);
                        fill(
                            dc,
                            &RECT {
                                left: rx + 72,
                                top: y + 7,
                                right: rx + 117,
                                bottom: y + 11,
                            },
                            palette.border,
                        );
                        if let Some(v) = peak {
                            fill(
                                dc,
                                &RECT {
                                    left: rx + 72,
                                    top: y + 7,
                                    right: rx + 72 + (v.clamp(0., 100.) * 0.45) as i32,
                                    bottom: y + 11,
                                },
                                accents[if i == 2 { 3 } else { i }],
                            );
                        }
                        text(dc, rx + 122, y, &fmt(peak, "%"), palette.muted);
                    }
                }
                if latest.is_none() {
                    text(
                        dc,
                        graph.left + 30,
                        graph.top + 8,
                        "Start a capture to reveal resource history",
                        palette.muted,
                    );
                }
                SelectObject(dc, semibold.into());
                text(dc, x + 14, 251, "Top contributors", palette.text);
                SelectObject(dc, font.into());
                panel(
                    dc,
                    RECT {
                        left: x,
                        top: 236,
                        right: x + content,
                        bottom: overview_table_bottom(height),
                    },
                    palette.surface,
                    palette.border,
                );
                SelectObject(dc, semibold.into());
                text(dc, x + 14, 251, "Top contributors", palette.text);
                SelectObject(dc, font.into());
                label(
                    dc,
                    x + 162,
                    252,
                    &format!("{} processes", app.displayed_rows.len()),
                );
                panel(
                    dc,
                    RECT {
                        left: x + content - 192,
                        top: 243,
                        right: x + content - 14,
                        bottom: 275,
                    },
                    palette.bg,
                    palette.border,
                );
                if latest.is_none() {
                    SelectObject(dc, semibold.into());
                    text(
                        dc,
                        x + 24,
                        308,
                        "Ready when your system slows down",
                        palette.text,
                    );
                    SelectObject(dc, font.into());
                    label(
                        dc,
                        x + 24,
                        339,
                        "Start a short capture. Your ten busiest processes will appear here.",
                    );
                    label(
                        dc,
                        x + 24,
                        365,
                        "Double-click a row or press Enter to investigate a process.",
                    );
                }
                SelectObject(dc, font.into());
                label(
                    dc,
                    x,
                    height - 28,
                    if app.active {
                        "Live pressure ranking · indicative, not proof of causation"
                    } else {
                        if app.net_active || app.net_pending {
                            "Performance capture off · network recording requested"
                        } else {
                            "Local diagnostics · no background sampling"
                        }
                    },
                );
            }
            1 => {
                let card = (content - 24) / 3;
                for (i, (title, description)) in [
                    ("Visual effects", "Reduce desktop animations"),
                    ("Power profile", "Restore Balanced power"),
                    ("Fixed pagefile", "50% RAM · minimum 16 GiB"),
                ]
                .iter()
                .enumerate()
                {
                    let left = x + i as i32 * (card + 12);
                    panel(
                        dc,
                        RECT {
                            left,
                            top: 142,
                            right: left + card,
                            bottom: 266,
                        },
                        palette.surface,
                        palette.border,
                    );
                    SelectObject(dc, semibold.into());
                    text(dc, left + 16, 158, title, palette.text);
                    SelectObject(dc, font.into());
                    label(dc, left + 16, 188, description);
                }
                label(dc, x, 278, "Review in Windows Settings");
                let count = |statuses: &[&str]| {
                    report
                        .rows
                        .iter()
                        .filter(|row| row.first().is_some_and(|v| statuses.contains(&v.as_str())))
                        .count()
                };
                label(
                    dc,
                    x + 16,
                    356,
                    &if app.reports[1].columns.is_empty() {
                        "Not checked · Run system checks".into()
                    } else {
                        format!(
                            "Results · {} deviations · {} blocked · {} unavailable",
                            count(&["Warning", "Critical"]),
                            count(&["Blocked"]),
                            count(&["Unknown", "Unavailable"])
                        )
                    },
                );
            }
            2 => {
                SelectObject(dc, semibold.into());
                text(
                    dc,
                    x + 16,
                    report_top(app, height) - 28,
                    "Settings results",
                    palette.text,
                );
                SelectObject(dc, font.into());
                let half = (content - 16) / 2;
                for (left, title, description) in [
                    (x, "Start with Windows", "Launch quietly in the system tray"),
                    (
                        x + half + 16,
                        "Installation & data",
                        "Per-user setup. Captures stay local.",
                    ),
                ] {
                    panel(
                        dc,
                        RECT {
                            left,
                            top: 130,
                            right: left + half,
                            bottom: 312,
                        },
                        palette.surface,
                        palette.border,
                    );
                    SelectObject(dc, semibold.into());
                    text(dc, left + 18, 148, title, palette.text);
                    SelectObject(dc, font.into());
                    label(dc, left + 18, 182, description);
                }
            }
            6 => {
                paint_cores(app, dc, width, height, font, semibold);
            }
            _ => {
                if app.page == 5 {
                    panel(
                        dc,
                        RECT {
                            left: x,
                            top: 136,
                            right: x + content,
                            bottom: 280,
                        },
                        palette.surface,
                        palette.border,
                    );
                }
                let top = if app.page == 5 {
                    296
                } else if app.page == 3 {
                    188
                } else if app.page == 4 {
                    142
                } else {
                    130
                };
                let count = report.metrics.len().clamp(1, 4);
                let card = (content - (count as i32 - 1) * 12) / count as i32;
                for (i, m) in report.metrics.iter().take(4).enumerate() {
                    metric_card(x + i as i32 * (card + 12), top, card, &m.label, &m.value, i);
                }
                SelectObject(dc, font.into());
                let mut title = wide(&report.title);
                let mut rect = RECT {
                    left: x + 16,
                    top: report_top(app, height) - 28,
                    right: x + content,
                    bottom: report_top(app, height) - 4,
                };
                SetTextColor(dc, palette.muted);
                DrawTextW(dc, &mut title, &mut rect, DT_SINGLELINE | DT_END_ELLIPSIS);
                if app.page == 5 {
                    if !app.traffic_error.is_empty() {
                        let mut error = wide(&app.traffic_error);
                        let mut rect = RECT {
                            left: x + 12,
                            top: 400,
                            right: x + content - 12,
                            bottom: 454,
                        };
                        SetTextColor(dc, accents[3]);
                        DrawTextW(dc, &mut error, &mut rect, DT_WORDBREAK | DT_END_ELLIPSIS);
                    }
                    let state = if app.net_pending {
                        "Preparing recorder · awaiting permission"
                    } else if app.net_active && app.traffic_follow {
                        "LIVE · updates every 2 seconds"
                    } else if app.net_active {
                        "HISTORY · following paused"
                    } else {
                        "HISTORY · recorder stopped"
                    };
                    label(dc, x, height - 44, state);
                    if app.net_active && !app.traffic_follow {
                        let value = |name: &str| {
                            app.traffic_live
                                .metrics
                                .iter()
                                .find(|m| m.label == name)
                                .map(|m| m.value.as_str())
                                .unwrap_or("Unavailable")
                        };
                        let mut text = wide(&format!(
                            "Live session Rx {} / Tx {}",
                            value("Received"),
                            value("Sent")
                        ));
                        let mut rect = RECT {
                            left: x + 290,
                            top: height - 44,
                            right: x + content - 236,
                            bottom: height - 22,
                        };
                        SetTextColor(dc, accents[1]);
                        DrawTextW(dc, &mut text, &mut rect, DT_SINGLELINE | DT_END_ELLIPSIS);
                    }
                }
            }
        }
    }
    if matches!(app.page, 3 | 5) && !app.compact && !app.action_status.is_empty() {
        SelectObject(dc, font.into());
        let mut status = wide(&app.action_status);
        let mut rect = RECT {
            left: x,
            top: height - 19,
            right: x + content,
            bottom: height - 2,
        };
        SetTextColor(dc, palette.muted);
        DrawTextW(dc, &mut status, &mut rect, DT_SINGLELINE | DT_END_ELLIPSIS);
    }
    SelectObject(dc, old);
    for font in [font, heading, value_font, semibold] {
        let _ = DeleteObject(font.into());
    }
    SetMapMode(dc, MM_TEXT);
    let _ = SetViewportOrgEx(dc, 0, 0, None);
    let _ = BitBlt(
        target,
        0,
        0,
        bounds.right,
        bounds.bottom,
        Some(dc),
        0,
        0,
        SRCCOPY,
    );
    SelectObject(dc, original_bitmap);
    let _ = DeleteObject(bitmap.into());
    let _ = DeleteDC(dc);
    let _ = EndPaint(hwnd, &ps);
}

pub unsafe fn cpu_hit(app: &App, hwnd: HWND, lparam: LPARAM) -> bool {
    if app.page != 0 || app.compact {
        return false;
    }
    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    let (_, left, content) = geometry((r.right as f64 / app.scale) as i32);
    let x = (lparam.0 as i16 as f64 / app.scale) as i32;
    let y = ((lparam.0 >> 16) as i16 as f64 / app.scale) as i32 - 28;
    x >= left && x < left + (content - 36) / 4 && (124..220).contains(&y)
}

fn chart_runs(
    app: &App,
    rect: RECT,
    value: impl Fn(&Sample) -> Option<f64>,
) -> Vec<Vec<(f32, f32)>> {
    let end = app.history.back().map(|s| s.elapsed).unwrap_or(0.);
    let begin = (end - 120.).max(0.);
    let mut runs = Vec::new();
    let mut points = Vec::new();
    for sample in app.history.iter().filter(|s| s.elapsed >= begin) {
        if let Some(v) = value(sample).filter(|v| v.is_finite()) {
            points.push((
                rect.left as f32
                    + ((sample.elapsed - begin) / 120. * (rect.right - rect.left) as f64) as f32,
                rect.bottom as f32
                    - (v.clamp(0., 100.) / 100. * (rect.bottom - rect.top) as f64) as f32,
            ));
        } else if !points.is_empty() {
            runs.push(std::mem::take(&mut points));
        }
    }
    if !points.is_empty() {
        runs.push(points);
    }
    runs
}
unsafe fn series(
    dc: HDC,
    app: &App,
    rect: RECT,
    accent: COLORREF,
    value: impl Fn(&Sample) -> Option<f64>,
) {
    for points in chart_runs(app, rect, value) {
        graphs::line(dc, app.scale as f32, &points, accent, 1.5);
    }
}
unsafe fn paint_compact(
    app: &App,
    dc: HDC,
    width: i32,
    height: i32,
    font: HFONT,
    heading: HFONT,
    value_font: HFONT,
) {
    let p = theme::palette();
    SelectObject(dc, heading.into());
    text(dc, 16, 20, "SuperOpti", p.text);
    SelectObject(dc, font.into());
    label(dc, width - 84, 3, "Opacity");
    label(dc, 132, 48, "Duration");
    panel(
        dc,
        RECT {
            left: 130,
            top: 62,
            right: 274,
            bottom: 98,
        },
        p.surface,
        p.border,
    );
    let card = (width - 42) / 2;
    for (i, name) in ["CPU", "Memory", "GPU", "Disk busy"].iter().enumerate() {
        let x = 16 + (i % 2) as i32 * (card + 10);
        let y = 104 + (i / 2) as i32 * 82;
        let accent = theme::metric_colors()[i];
        panel(
            dc,
            RECT {
                left: x,
                top: y,
                right: x + card,
                bottom: y + 76,
            },
            tint(p.surface, accent, 6),
            p.border,
        );
        SelectObject(dc, font.into());
        label(dc, x + 12, y + 10, name);
        SelectObject(dc, value_font.into());
        let v = app.history.back().and_then(|s| match i {
            0 => s.cpu,
            1 => s.ram,
            2 => s.gpu,
            _ => s.disk,
        });
        text(dc, x + 12, y + 34, &fmt(v, "%"), p.text);
        series(
            dc,
            app,
            RECT {
                left: x + 12,
                top: y + 62,
                right: x + card - 12,
                bottom: y + 70,
            },
            accent,
            |s| match i {
                0 => s.cpu,
                1 => s.ram,
                2 => s.gpu,
                _ => s.disk,
            },
        );
    }
    SelectObject(dc, heading.into());
    text(dc, 16, 267, "Top 3 contributors", p.text);
    SelectObject(dc, font.into());
    if app.history.is_empty() {
        panel(
            dc,
            RECT {
                left: 16,
                top: 286,
                right: width - 16,
                bottom: height - 66,
            },
            p.surface,
            p.border,
        );
        label(dc, 30, 318, "Start a capture to identify contributors.");
    }
    label(
        dc,
        16,
        height - 42,
        if app.net_pending {
            "Network permission pending"
        } else if app.net_active {
            "Network recording · automatic stop"
        } else if app.active {
            "Capturing · automatic stop enabled"
        } else {
            "Idle · no sampling"
        },
    );
}
fn core_keys(app: &App) -> Vec<String> {
    let mut keys: Vec<_> = app
        .history
        .back()
        .map(|s| s.cores.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort_by_key(|key| {
        let mut parts = key.split(',').filter_map(|x| x.parse::<u32>().ok());
        (parts.next().unwrap_or(0), parts.next().unwrap_or(0))
    });
    keys
}
pub unsafe fn core_scroll_max(app: &App, hwnd: HWND) -> usize {
    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    let width = (r.right as f64 / app.scale) as i32;
    let height = (r.bottom as f64 / app.scale) as i32 - 28;
    let (_, _, content) = geometry(width);
    let columns = (content / 130).clamp(4, 12) as usize;
    let visible = ((height - 162) / 110).max(1) as usize;
    core_keys(app)
        .len()
        .div_ceil(columns)
        .saturating_sub(visible)
}
unsafe fn paint_cores(app: &App, dc: HDC, width: i32, height: i32, font: HFONT, heading: HFONT) {
    let p = theme::palette();
    let (_, x, content) = geometry(width);
    let keys = core_keys(app);
    let columns = (content / 130).clamp(4, 12) as usize;
    let rows = ((height - 162) / 110).max(1) as usize;
    SelectObject(dc, font.into());
    label(
        dc,
        x,
        126,
        &format!(
            "{} logical processors · each graph 0–100% · scroll for more",
            keys.len()
        ),
    );
    if keys.is_empty() {
        panel(
            dc,
            RECT {
                left: x,
                top: 158,
                right: x + content,
                bottom: height - 28,
            },
            p.surface,
            p.border,
        );
        SelectObject(dc, heading.into());
        text(dc, x + 22, 184, "No processor samples yet", p.text);
        SelectObject(dc, font.into());
        label(
            dc,
            x + 22,
            216,
            "Start a performance capture from Overview.",
        );
        return;
    }
    let card = (content - ((columns - 1) * 10) as i32) / columns as i32;
    for (index, key) in keys
        .iter()
        .skip(
            app.core_scroll
                .min(keys.len().div_ceil(columns).saturating_sub(rows))
                * columns,
        )
        .take(rows * columns)
        .enumerate()
    {
        let left = x + (index % columns) as i32 * (card + 10);
        let top = 158 + (index / columns) as i32 * 110;
        panel(
            dc,
            RECT {
                left,
                top,
                right: left + card,
                bottom: top + 98,
            },
            p.surface,
            p.border,
        );
        SelectObject(dc, font.into());
        let label0 = key
            .split_once(',')
            .map(|(g, n)| format!("G{g} · CPU {n}"))
            .unwrap_or_else(|| key.clone());
        label(dc, left + 10, top + 10, &label0);
        SelectObject(dc, heading.into());
        text(
            dc,
            left + 10,
            top + 34,
            &fmt(
                app.history.back().and_then(|s| s.cores.get(key)).copied(),
                "%",
            ),
            p.text,
        );
        series(
            dc,
            app,
            RECT {
                left: left + 10,
                top: top + 65,
                right: left + card - 10,
                bottom: top + 87,
            },
            theme::metric_colors()[0],
            |s| s.cores.get(key).copied(),
        );
    }
    SelectObject(dc, font.into());
    label(
        dc,
        x,
        height - 24,
        "Actual sampled activity · process affinity does not prove execution on a core",
    );
}

pub unsafe fn update_file_actions(app: &App) {
    let Some(&(button, ..)) = app.controls.iter().find(|(h, ..)| GetDlgCtrlID(*h) == 185) else {
        return;
    };
    let selected = SendMessageW(
        app.detail,
        LVM_GETNEXTITEM,
        Some(WPARAM(usize::MAX)),
        Some(LPARAM(LVNI_SELECTED as isize)),
    )
    .0;
    let eligible = app
        .presentation
        .columns
        .iter()
        .position(|c| c == "Path" || c == "Executable")
        .and_then(|column| {
            app.presentation
                .rows
                .get(selected as usize)
                .and_then(|row| row.get(column))
        })
        .is_some_and(|path| path.as_bytes().get(1) == Some(&b':') || path.starts_with("\\\\"));
    let _ = EnableWindow(
        button,
        app.page == 3 && !app.detail_pending && selected >= 0 && eligible,
    );
}

struct CachedIcon {
    path: String,
    icon: HICON,
}
impl Drop for CachedIcon {
    fn drop(&mut self) {
        if !self.icon.is_invalid() {
            unsafe {
                let _ = DestroyIcon(self.icon);
            }
        }
    }
}
#[derive(Default)]
struct IconCache {
    icons: std::collections::HashMap<(u32, Option<u64>), CachedIcon>,
    indices: std::collections::HashMap<(u32, Option<u64>), i32>,
    list: isize,
}
thread_local! {static PROCESS_ICONS:std::cell::RefCell<IconCache>=std::cell::RefCell::new(IconCache::default());}
/// Called only after a verified metadata result; never during collection or every paint.
pub unsafe fn cache_process_icon(pid: u32, created: Option<u64>, path: &str) {
    if path.is_empty() {
        return;
    }
    PROCESS_ICONS.with_borrow_mut(|cache| {
        let key = (pid, created);
        if cache.icons.contains_key(&key) || cache.icons.len() >= 512 {
            return;
        }
        let mut icon = HICON::default();
        let path_w = wide(path);
        // Direct PE icon extraction avoids arbitrary Shell icon-handler extensions.
        windows::Win32::UI::Shell::ExtractIconExW(
            PCWSTR(path_w.as_ptr()),
            0,
            Some(&mut icon),
            None,
            1,
        );
        cache.icons.insert(
            key,
            CachedIcon {
                path: path.into(),
                icon,
            },
        );
    });
}
pub unsafe fn draw_cached_icon(
    app: &App,
    dc: HDC,
    pid: u32,
    created: Option<u64>,
    rect: RECT,
) -> bool {
    PROCESS_ICONS.with_borrow_mut(|cache| {
        let key = (pid, created);
        let Some(entry) = cache.icons.get(&key) else {
            return false;
        };
        if entry.icon.is_invalid() {
            return false;
        }
        let list = HIMAGELIST(
            SendMessageW(
                app.table,
                LVM_GETIMAGELIST,
                Some(WPARAM(LVSIL_SMALL as usize)),
                None,
            )
            .0,
        );
        if list.0 == 0 {
            return false;
        }
        if cache.list != list.0 {
            cache.list = list.0;
            cache.indices.clear();
        }
        let index = if let Some(&index) = cache.indices.get(&key) {
            index
        } else {
            let index = ImageList_ReplaceIcon(list, -1, entry.icon);
            if index < 0 {
                return false;
            }
            cache.indices.insert(key, index);
            index
        };
        ImageList_DrawEx(
            list,
            index,
            dc,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            COLORREF(CLR_NONE as u32),
            COLORREF(CLR_NONE as u32),
            ILD_TRANSPARENT,
        )
        .as_bool()
    })
}
fn cached_process_name(pid: u32, created: Option<u64>) -> Option<String> {
    PROCESS_ICONS.with_borrow(|cache| {
        cache.icons.get(&(pid, created)).and_then(|entry| {
            std::path::Path::new(&entry.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
    })
}

unsafe fn cpu_area(dc: HDC, app: &App, rect: RECT, color: COLORREF) {
    for points in chart_runs(app, rect, |s| s.cpu) {
        graphs::area(dc, app.scale as f32, &points, rect.bottom as f32, color, 30);
    }
}

// One owned native tooltip. Text storage outlives the common-control pointer.
thread_local! {
    static PROCESS_TOOLTIP: std::cell::RefCell<(HWND, Vec<u16>)> = std::cell::RefCell::new((HWND::default(), Vec::new()));
}
pub unsafe fn update_process_tooltip(app: &App, hwnd: HWND) {
    PROCESS_TOOLTIP.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.0.is_invalid() {
            state.0 = CreateWindowExW(
                WS_EX_TOPMOST,
                w!("tooltips_class32"),
                None,
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                0,
                0,
                0,
                0,
                Some(hwnd),
                None,
                None,
                None,
            )
            .unwrap_or_default();
            SendMessageW(
                state.0,
                TTM_SETMAXTIPWIDTH,
                None,
                Some(LPARAM((480. * app.scale) as isize)),
            );
        }
        let mut tool = TTTOOLINFOW {
            cbSize: size_of::<TTTOOLINFOW>() as u32,
            hwnd,
            uId: 1,
            ..Default::default()
        };
        SendMessageW(
            state.0,
            TTM_DELTOOLW,
            None,
            Some(LPARAM(&tool as *const _ as isize)),
        );
        if app.page != 3 || app.compact {
            return;
        }
        let Some(pid) = app.detail_pid else {
            return;
        };
        state.1 = wide(
            &app.metadata_cache
                .get(&(pid, app.detail_created))
                .map(|m| m.tooltip())
                .unwrap_or_else(|| "Loading executable details...".into()),
        );
        let mut bounds = RECT::default();
        let _ = GetClientRect(hwnd, &mut bounds);
        let (_, x, _) = geometry((bounds.right as f64 / app.scale) as i32);
        tool.uFlags = TTF_SUBCLASS;
        tool.rect = RECT {
            left: (x as f64 * app.scale) as i32,
            top: (46. * app.scale) as i32,
            right: ((x + 360) as f64 * app.scale) as i32,
            bottom: (94. * app.scale) as i32,
        };
        tool.lpszText = windows::core::PWSTR(state.1.as_mut_ptr());
        SendMessageW(
            state.0,
            TTM_ADDTOOLW,
            None,
            Some(LPARAM(&tool as *const _ as isize)),
        );
    });
}

fn overview_table_bottom(height: i32) -> i32 {
    (height - 200).clamp(424, 614)
}
unsafe fn glow(dc: HDC, rect: RECT, base: COLORREF, accent: COLORREF) {
    let saved = SaveDC(dc);
    IntersectClipRect(dc, rect.left, rect.top, rect.right, rect.bottom);
    for radius in (1..=46).rev() {
        let brush = CreateSolidBrush(tint(base, accent, ((46 - radius) * 25 / 46) as u32));
        let old = SelectObject(dc, brush.into());
        let pen = SelectObject(dc, GetStockObject(NULL_PEN));
        let cx = (rect.left + rect.right) / 2;
        let cy = rect.top + 20;
        let _ = Ellipse(dc, cx - radius, cy - radius, cx + radius, cy + radius);
        SelectObject(dc, pen);
        SelectObject(dc, old);
        let _ = DeleteObject(brush.into());
    }
    let _ = RestoreDC(dc, saved);
}

struct AccessibleNames {
    service: Option<windows::Win32::UI::Accessibility::IAccPropServices>,
    windows: Vec<HWND>,
    initialized: bool,
    tooltip: HWND,
    tip_text: Vec<Vec<u16>>,
}
impl Drop for AccessibleNames {
    fn drop(&mut self) {
        unsafe {
            if let Some(service) = self.service.take() {
                for &hwnd in &self.windows {
                    let _ = service.ClearHwndProps(
                        hwnd,
                        OBJID_CLIENT.0 as u32,
                        0,
                        &[windows::Win32::UI::Accessibility::PROPID_ACC_NAME],
                    );
                }
                drop(service);
            }
            if !self.tooltip.is_invalid() {
                let _ = DestroyWindow(self.tooltip);
            }
            if self.initialized {
                windows::Win32::System::Com::CoUninitialize();
            }
        }
    }
}
thread_local! {static ACCESSIBLE_NAMES:std::cell::RefCell<Option<AccessibleNames>>=const {std::cell::RefCell::new(None)};}
unsafe fn name_accessible_controls(hwnd: HWND) {
    use windows::Win32::{System::Com::*, UI::Accessibility::*};
    let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
    let service: Option<IAccPropServices> =
        CoCreateInstance(&CAccPropServices, None, CLSCTX_INPROC_SERVER).ok();
    let mut state = AccessibleNames {
        service,
        windows: Vec::new(),
        initialized,
        tooltip: CreateWindowExW(
            WS_EX_TOPMOST,
            w!("tooltips_class32"),
            None,
            WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
            0,
            0,
            0,
            0,
            Some(hwnd),
            None,
            None,
            None,
        )
        .unwrap_or_default(),
        tip_text: Vec::new(),
    };
    if let Some(service) = state.service.as_ref() {
        for (id, name) in [
            (109, "Rank contributors by"),
            (180, "Capture duration"),
            (183, "Pinned window opacity"),
            (181, "Always on top"),
            (182, "Toggle compact view"),
            (192, "Search processes by name or PID"),
            (168, "Traffic protocol"),
            (169, "Group traffic by"),
        ] {
            if let Ok(control) = GetDlgItem(Some(hwnd), id) {
                let name = wide(name);
                if service
                    .SetHwndPropStr(
                        control,
                        OBJID_CLIENT.0 as u32,
                        0,
                        PROPID_ACC_NAME,
                        PCWSTR(name.as_ptr()),
                    )
                    .is_ok()
                {
                    state.windows.push(control);
                }
            }
        }
    }
    for (id, label) in [
        (181, "Toggle always on top"),
        (182, "Switch compact / full view"),
    ] {
        if let Ok(control) = GetDlgItem(Some(hwnd), id) {
            state.tip_text.push(wide(label));
            let text = state.tip_text.last_mut().unwrap();
            let tool = TTTOOLINFOW {
                cbSize: size_of::<TTTOOLINFOW>() as u32,
                uFlags: TTF_IDISHWND | TTF_SUBCLASS,
                hwnd,
                uId: control.0 as usize,
                lpszText: windows::core::PWSTR(text.as_mut_ptr()),
                ..Default::default()
            };
            SendMessageW(
                state.tooltip,
                TTM_ADDTOOLW,
                None,
                Some(LPARAM(&tool as *const _ as isize)),
            );
        }
    }
    ACCESSIBLE_NAMES.with_borrow_mut(|value| *value = Some(state));
}
pub fn clear_accessible_names() {
    unsafe {
        theme::clear_fonts();
    }
    ACCESSIBLE_NAMES.with_borrow_mut(|value| *value = None);
}

fn report_body_height(app: &App, height: i32) -> i32 {
    let available =
        (height - report_top(app, height) - if matches!(app.page, 3 | 5) { 78 } else { 40 })
            .max(100);
    let rows = display_report(app).rows.len().clamp(4, 1000) as i32;
    available.min(32 + rows * 31)
}
