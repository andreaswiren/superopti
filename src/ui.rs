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
    add_control(app, hwnd, w!("BUTTON"), label, id, WS_TABSTOP, rect);
}

pub unsafe fn init(app: &mut App, hwnd: HWND) {
    let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_LISTVIEW_CLASSES,
    });
    for (id, label, x, width) in [
        (301, "Overview", 24, 140),
        (302, "System checks", 172, 150),
        (303, "Settings", 330, 130),
    ] {
        button(app, hwnd, id, label, (x, 64, width, 32));
    }
    for (id, label, x, width) in [
        (101, "Capture 2 min", 24, 126),
        (102, "Capture 5 min", 158, 126),
        (103, "Capture 15 min", 292, 130),
        (104, "Stop", 430, 76),
        (105, "Export JSON", 514, 116),
        (107, "Hide to tray", 638, 116),
    ] {
        button(app, hwnd, id, label, (x, 112, width, 34));
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
        "Rank by pressure score",
        "Rank by CPU",
        "Rank by RAM",
        "Rank by I/O",
        "Rank by GPU",
        "Rank by handles",
        "Rank by threads",
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
        WS_TABSTOP | WS_BORDER | WINDOW_STYLE(LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS),
        (24, 420, 1080, 220),
    );
    SendMessageW(
        app.table,
        LVM_SETEXTENDEDLISTVIEWSTYLE,
        None,
        Some(LPARAM(
            (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | LVS_EX_LABELTIP) as isize,
        )),
    );
    for (index, (label, width)) in [
        ("Process", 185),
        ("PID", 65),
        ("CPU %", 72),
        ("RAM MiB", 88),
        ("I/O MiB/s", 88),
        ("GPU %", 72),
        ("Threads", 72),
        ("Handles", 80),
        ("Score", 66),
        ("Likely contribution", 240),
    ]
    .iter()
    .enumerate()
    {
        let mut label = wide(label);
        let col = LVCOLUMNW {
            mask: LVCF_TEXT | LVCF_WIDTH | LVCF_FMT,
            fmt: if index == 0 || index == 9 {
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
        (115, "Exit SuperOpti", (412, 362, 150, 36)),
        (304, "Back to overview", (24, 174, 160, 32)),
        (117, "Inspect this process's threads", (198, 174, 238, 32)),
        (118, "Measure TCP traffic (10s)", (448, 174, 208, 32)),
    ] {
        button(app, hwnd, id, label, rect);
    }
    app.report = add_control(
        app,
        hwnd,
        w!("EDIT"),
        "Ready when you need it. Start a short capture during slowness. No sampling runs while idle.\r\nSelect a contributor and press Enter, or double-click it, to investigate. N/A means a counter is unavailable.",
        202,
        WS_TABSTOP
            | WS_VSCROLL
            | WINDOW_STYLE(ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32),
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
        w!("EDIT"),
        "",
        203,
        WS_TABSTOP
            | WS_VSCROLL
            | WINDOW_STYLE(ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32),
        (24, 232, 1080, 500),
    );
    app.taskbar = RegisterWindowMessageW(w!("TaskbarCreated"));
    tray(hwnd, NIM_ADD, false);
    layout(app, hwnd);
}

pub unsafe fn layout(app: &App, hwnd: HWND) {
    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    let width = (r.right as f64 / app.scale) as i32;
    let height = (r.bottom as f64 / app.scale) as i32;
    let compact = if height < 740 { 40 } else { 0 };
    for &(h, x, y, w, height0) in &app.controls {
        let id = GetDlgCtrlID(h);
        let visible = match id {
            109 | 201 | 204 | 210 | 211 | 110 | 116 => app.page == 0,
            106 | 121..=130 => app.page == 1,
            111..=115 => app.page == 2,
            203 | 304 | 117 | 118 => app.page == 3,
            202 => app.page != 3,
            _ => true,
        };
        let _ = ShowWindow(h, if visible { SW_SHOW } else { SW_HIDE });
        let (x, y, w, height0) = match id {
            201 => (
                24,
                420 - compact,
                width - 48,
                (height - 584 + compact).max(90),
            ),
            210 | 211 | 110 | 116 => (x, height - 156 + (y - 644), w, height0),
            202 => {
                let y = match app.page {
                    0 => height - 80,
                    1 => 478,
                    _ => 448,
                };
                (24, y, width - 48, (height - y - 24).max(60))
            }
            203 => (24, 232, width - 48, (height - 256).max(100)),
            204 => (24, height - 120, width - 48, 36),
            109 => (width - 246, 380 - compact, 222, height0),
            _ => (x, y, w, height0),
        };
        let s = |v: i32| (v as f64 * app.scale).round() as i32;
        let _ = MoveWindow(h, s(x), s(y), s(w), s(height0), true);
        if (101..=103).contains(&id) || id == 108 {
            let _ = EnableWindow(h, !app.active);
        }
        if id == 104 {
            let _ = EnableWindow(h, app.active);
        }
        if id == 105 {
            let _ = EnableWindow(h, !app.history.is_empty());
        }
        if matches!(id, 110 | 116 | 117 | 118) {
            let _ = EnableWindow(h, !app.detail_pending);
        }
        if (301..=303).contains(&id) {
            SendMessageW(
                h,
                BM_SETSTYLE,
                Some(WPARAM(if (id - 301) as usize == app.page {
                    BS_DEFPUSHBUTTON as usize
                } else {
                    BS_PUSHBUTTON as usize
                })),
                Some(LPARAM(1)),
            );
        }
    }
    let _ = InvalidateRect(Some(hwnd), None, false);
}

pub unsafe fn update_table(app: &mut App) {
    let Some(s) = app.history.back() else { return };
    set_text(
        app.telemetry,
        &format!(
            "Disk {} MiB/s  ·  latency {} ms  ·  disk queue {}  ·  page-ins {}/s  ·  network {} MiB/s\r\nCPU queue {}  ·  DPC {}%  ·  context switches {}/s  ·  collection {:.1} ms",
            fmt(s.disk_mb, ""),
            fmt(s.disk_latency_ms, ""),
            fmt(s.disk_queue, ""),
            fmt(s.page_reads, ""),
            fmt(s.network_mb, ""),
            fmt(s.cpu_queue, ""),
            fmt(s.dpc, ""),
            fmt(s.context_switches, ""),
            s.collection_ms
        ),
    );
    let selected = selected_identity(app);
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
    SendMessageW(app.table, WM_SETREDRAW, Some(WPARAM(0)), None);
    SendMessageW(app.table, LVM_DELETEALLITEMS, None, None);
    for (index, p) in rows.iter().take(10).enumerate() {
        let whole = |v: Option<f64>| v.map(|v| format!("{v:.0}")).unwrap_or_else(|| "N/A".into());
        let cells = [
            p.name.clone(),
            p.pid.to_string(),
            fmt(p.cpu, ""),
            fmt(p.ram_mb, ""),
            fmt(p.io_mb, ""),
            fmt(p.gpu, ""),
            whole(p.threads),
            whole(p.handles),
            format!("{:.1}", p.score),
            p.reason.clone(),
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
        .take(10)
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
    let process = app
        .history
        .back()
        .and_then(|s| s.processes.iter().find(|p| p.pid == pid));
    let name = process.map(|p| p.name.as_str()).unwrap_or("Process");
    set_text(
        app.detail,
        &format!(
            "{name}  ·  PID {pid}\r\n\r\nCollecting process details once. This does not start continuous monitoring..."
        ),
    );
    app.detail_pid = Some(pid);
    app.detail_created = created;
    app.detail_pending = true;
    app.page = 3;
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

unsafe fn label(dc: HDC, x: i32, y: i32, s: &str) {
    text(dc, x, y, s, color(89, 103, 123));
}
pub unsafe fn paint(app: &App, hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let target = BeginPaint(hwnd, &mut ps);
    let mut bounds = RECT::default();
    let _ = GetClientRect(hwnd, &mut bounds);
    let dc = CreateCompatibleDC(Some(target));
    let bitmap = CreateCompatibleBitmap(target, bounds.right.max(1), bounds.bottom.max(1));
    let original_bitmap = SelectObject(dc, bitmap.into());
    let width = (bounds.right as f64 / app.scale) as i32;
    let height = (bounds.bottom as f64 / app.scale) as i32;
    let compact = if height < 740 { 40 } else { 0 };
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
        color(245, 247, 251),
    );
    SetBkMode(dc, TRANSPARENT);
    // Fonts already use physical pixels; map text back to logical size via temporary DC scale fonts.
    let font = CreateFontW(
        -14,
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
        w!("Segoe UI"),
    );
    let heading = CreateFontW(
        -26,
        0,
        0,
        0,
        600,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH.0 as u32,
        w!("Segoe UI"),
    );
    let old = SelectObject(dc, heading.into());
    let _ = DrawIconEx(dc, 24, 20, icon(101), 30, 30, 0, None, DI_NORMAL);
    text(dc, 66, 18, "SuperOpti", color(25, 40, 61));
    SelectObject(dc, font.into());
    label(dc, 212, 29, "On-demand performance diagnostics");
    text(
        dc,
        width - 232,
        28,
        if app.active {
            "●  CAPTURING"
        } else if app.history.is_empty() {
            "○  IDLE · NO SAMPLING"
        } else {
            "○  CAPTURE COMPLETE"
        },
        color(0, 118, 109),
    );
    label(dc, 24, 153, &app.status);
    match app.page {
        0 => {
            let gap = 12;
            let card = (width - 48 - gap * 5) / 6;
            let latest = app.history.back();
            for (i, title) in ["CPU", "Memory", "GPU", "Disk busy", "Commit", "Pagefile"]
                .iter()
                .enumerate()
            {
                let x = 24 + i as i32 * (card + gap);
                let y = 182;
                fill(
                    dc,
                    &RECT {
                        left: x,
                        top: y,
                        right: x + card,
                        bottom: 362 - compact,
                    },
                    color(255, 255, 255),
                );
                let colors = [
                    color(0, 133, 123),
                    color(64, 104, 211),
                    color(133, 78, 194),
                    color(196, 122, 31),
                    color(55, 126, 157),
                    color(151, 86, 111),
                ];
                fill(
                    dc,
                    &RECT {
                        left: x,
                        top: y,
                        right: x + card,
                        bottom: y + 3,
                    },
                    colors[i],
                );
                label(dc, x + 12, y + 13, title);
                let value = |s: &Sample| match i {
                    0 => s.cpu,
                    1 => s.ram,
                    2 => s.gpu,
                    3 => s.disk,
                    4 => s.commit,
                    _ => s.swap,
                };
                SelectObject(dc, heading.into());
                text(
                    dc,
                    x + 12,
                    y + 34,
                    &fmt(latest.and_then(value), "%"),
                    color(25, 40, 61),
                );
                SelectObject(dc, font.into());
                let graph = RECT {
                    left: x + 12,
                    top: y + 85 - compact / 4,
                    right: x + card - 12,
                    bottom: y + 146 - compact,
                };
                let grid = CreatePen(PS_SOLID, 1, color(231, 236, 244));
                let prev = SelectObject(dc, grid.into());
                for fraction in 0..=2 {
                    let gy = graph.top + (graph.bottom - graph.top) * fraction / 2;
                    let _ = MoveToEx(dc, graph.left, gy, None);
                    let _ = LineTo(dc, graph.right, gy);
                }
                SelectObject(dc, prev);
                let _ = DeleteObject(grid.into());
                let pen = CreatePen(PS_SOLID, 2, colors[i]);
                let prev = SelectObject(dc, pen.into());
                let end = latest.map(|s| s.elapsed).unwrap_or(0.0);
                let begin = (end - 120.0).max(0.0);
                let span = 120.0;
                let mut connected = false;
                for sample in app.history.iter().filter(|s| s.elapsed >= begin) {
                    if let Some(value) = value(sample).filter(|v| v.is_finite()) {
                        let px = graph.left
                            + ((sample.elapsed - begin) / span * (graph.right - graph.left) as f64)
                                as i32;
                        let py = graph.bottom
                            - (value.clamp(0.0, 100.0) / 100.0 * (graph.bottom - graph.top) as f64)
                                as i32;
                        if connected {
                            let _ = LineTo(dc, px, py);
                        } else {
                            let _ = MoveToEx(dc, px, py, None);
                            connected = true;
                            let _ = SetPixel(dc, px, py, colors[i]);
                        }
                    } else {
                        connected = false;
                    }
                }
                SelectObject(dc, prev);
                let _ = DeleteObject(pen.into());
                if latest.is_none() {
                    label(
                        dc,
                        x + 12,
                        y + 90,
                        if app.active {
                            "Warming up…"
                        } else {
                            "Awaiting capture"
                        },
                    );
                }
                label(dc, x + 12, y + 155 - compact, "0–100% · 2 min");
            }
            text(
                dc,
                24,
                379 - compact,
                "Top 10 contributors",
                color(25, 40, 61),
            );
            label(
                dc,
                24,
                399 - compact,
                "Live ranking is a clue, not proof of causation. Double-click a process to investigate.",
            );
        }
        1 => {
            text(
                dc,
                24,
                184,
                "Check first. Change deliberately.",
                color(25, 40, 61),
            );
            label(
                dc,
                220,
                228,
                "Review system settings and preview this computer's fixed pagefile target.",
            );
            text(
                dc,
                24,
                275,
                "Optional, reversible settings",
                color(25, 40, 61),
            );
            label(
                dc,
                24,
                348,
                "Pagefile: max(50% physical RAM, 16 GiB), initial = maximum. Admin + restart required; disk space is checked.",
            );
            text(dc, 24, 372, "Review in Windows Settings", color(25, 40, 61));
            label(
                dc,
                24,
                442,
                "Fix all applies the three settings above. Storage, startup and Windows Update remain manual reviews.",
            );
        }
        2 => {
            text(
                dc,
                24,
                188,
                "A quiet companion for your system tray",
                color(25, 40, 61),
            );
            label(
                dc,
                24,
                218,
                "Autostart opens SuperOpti in the tray. Monitoring still starts only when you request a capture.",
            );
            text(
                dc,
                24,
                306,
                "Installation and local captures",
                color(25, 40, 61),
            );
            label(
                dc,
                24,
                336,
                "Install for your Windows account. Exported diagnostics remain on this device; no telemetry is sent.",
            );
            label(
                dc,
                24,
                416,
                "Closing or minimizing hides the window. Use Exit SuperOpti to quit completely.",
            );
        }
        _ => {
            label(
                dc,
                24,
                212,
                "Process detail · requested once · refresh by opening the contributor again",
            );
        }
    }
    SelectObject(dc, old);
    let _ = DeleteObject(font.into());
    let _ = DeleteObject(heading.into());
    SetMapMode(dc, MM_TEXT);
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
