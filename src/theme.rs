//! Theme changes are event-driven; no background polling or rendering timer.
use super::*;
use std::cell::Cell;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::{
    Graphics::Dwm::*,
    System::Registry::*,
    UI::{Accessibility::*, HiDpi::*},
};

#[derive(Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub high_contrast: bool,
    pub bg: COLORREF,
    pub surface: COLORREF,
    pub text: COLORREF,
    pub muted: COLORREF,
    pub border: COLORREF,
    pub accent: COLORREF,
    pub selection: COLORREF,
}
thread_local! {
    static CURRENT: Cell<Option<Palette>> = const { Cell::new(None) };
    static PAGE: Cell<usize> = const { Cell::new(0) };
}
pub fn set_page(page: usize) {
    PAGE.set(page);
}
pub fn palette() -> Palette {
    CURRENT.get().unwrap_or_else(|| unsafe { detect() })
}
pub fn colors(dark: bool) -> Palette {
    if dark {
        Palette {
            dark,
            high_contrast: false,
            bg: color(18, 19, 25),
            surface: color(34, 36, 48),
            text: color(235, 240, 250),
            muted: color(169, 175, 195),
            border: color(52, 55, 70),
            accent: color(173, 67, 118),
            selection: color(72, 40, 61),
        }
    } else {
        Palette {
            dark,
            high_contrast: false,
            bg: color(238, 242, 249),
            surface: color(255, 255, 255),
            text: color(22, 33, 55),
            muted: color(83, 99, 123),
            border: color(210, 220, 235),
            accent: color(166, 58, 108),
            selection: color(246, 225, 238),
        }
    }
}
pub fn font_face() -> PCWSTR {
    w!("Segoe UI")
}
pub fn metric_colors() -> [COLORREF; 4] {
    let p = palette();
    if p.high_contrast {
        [p.text; 4]
    } else if p.dark {
        [
            color(232, 132, 182),
            color(127, 202, 216),
            color(169, 148, 231),
            color(217, 174, 113),
        ]
    } else {
        [
            color(171, 73, 123),
            color(33, 126, 140),
            color(118, 98, 183),
            color(147, 105, 35),
        ]
    }
}
unsafe fn detect() -> Palette {
    // Preview only in debug/test binaries; never alters the user's Windows theme.
    #[cfg(any(debug_assertions, test))]
    for argument in std::env::args() {
        match argument.as_str() {
            "--ui-test-dark" => return colors(true),
            "--ui-test-light" => return colors(false),
            _ => {}
        }
    }
    let mut hc = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    let _ = SystemParametersInfoW(
        SPI_GETHIGHCONTRAST,
        hc.cbSize,
        Some((&mut hc as *mut HIGHCONTRASTW).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    if hc.dwFlags.0 & HCF_HIGHCONTRASTON.0 != 0 {
        return Palette {
            dark: false,
            high_contrast: true,
            bg: COLORREF(GetSysColor(COLOR_WINDOW)),
            surface: COLORREF(GetSysColor(COLOR_WINDOW)),
            text: COLORREF(GetSysColor(COLOR_WINDOWTEXT)),
            muted: COLORREF(GetSysColor(COLOR_WINDOWTEXT)),
            border: COLORREF(GetSysColor(COLOR_WINDOWTEXT)),
            accent: COLORREF(GetSysColor(COLOR_HIGHLIGHT)),
            selection: COLORREF(GetSysColor(COLOR_HIGHLIGHT)),
        };
    }
    let mut light = 1u32;
    let mut size = 4;
    let _ = RegGetValueW(
        HKEY_CURRENT_USER,
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
        w!("AppsUseLightTheme"),
        RRF_RT_REG_DWORD,
        None,
        Some((&mut light as *mut u32).cast()),
        Some(&mut size),
    );
    colors(light == 0)
}
pub unsafe fn apply(app: &App, hwnd: HWND) {
    let p = detect();
    if CURRENT.get() == Some(p) {
        return;
    }
    CURRENT.set(Some(p));
    let dark = windows::core::BOOL::from(p.dark);
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        (&dark as *const windows::core::BOOL).cast(),
        size_of::<windows::core::BOOL>() as u32,
    );
    for &(h, ..) in &app.controls {
        // Documented opt-out of visual styles lets our CTLCOLOR/owner drawing
        // consistently cover legacy controls without private uxtheme ordinals.
        let _ = SetWindowTheme(h, w!(""), w!(""));
        if matches!(GetDlgCtrlID(h), 109 | 168 | 169 | 180 | 183) {
            let _ = SetWindowSubclass(h, Some(combo_subclass), 0x5343, 0);
        }
        let _ = SetWindowSubclass(h, Some(hover_subclass), 0x5348, 0);
        if GetDlgCtrlID(h) == 280 {
            let _ = SetWindowSubclass(h, Some(tabs_subclass), 0x5356, 0);
        }
        if GetDlgCtrlID(h) == 108 {
            let _ = SetWindowSubclass(h, Some(toggle_subclass), 0x5354, 0);
        }
        let _ = InvalidateRect(Some(h), None, true);
    }
    for table in [app.table, app.report, app.detail] {
        // Header notifications go to their ListView, not directly to our window.
        let _ = SetWindowSubclass(table, Some(table_subclass), 0x534f, hwnd.0 as usize);
        for (message, value) in [
            (LVM_SETBKCOLOR, p.surface),
            (LVM_SETTEXTBKCOLOR, p.surface),
            (LVM_SETTEXTCOLOR, p.text),
        ] {
            SendMessageW(table, message, None, Some(LPARAM(value.0 as isize)));
        }
        let header = HWND(SendMessageW(table, LVM_GETHEADER, None, None).0 as *mut _);
        if !header.is_invalid() {
            let _ = SetWindowTheme(header, w!(""), w!(""));
            let _ = SetWindowSubclass(header, Some(header_subclass), 0x534a, app.font.0 as usize);
            SetWindowLongW(
                header,
                GWL_STYLE,
                GetWindowLongW(header, GWL_STYLE) & !(HDS_BUTTONS as i32),
            );
        }
    }
    let _ = RedrawWindow(
        Some(hwnd),
        None,
        None,
        RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_FRAME,
    );
}

unsafe extern "system" fn table_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    parent: usize,
) -> LRESULT {
    if msg == WM_NOTIFY && lparam.0 != 0 {
        let hdr = &*(lparam.0 as *const NMHDR);
        let header = HWND(SendMessageW(hwnd, LVM_GETHEADER, None, None).0 as *mut _);
        if hdr.code == NM_CUSTOMDRAW && hdr.hwndFrom == header {
            return SendMessageW(
                HWND(parent as *mut _),
                WM_NOTIFY,
                Some(wparam),
                Some(lparam),
            );
        }
    }
    if msg == WM_PAINT {
        let result = DefSubclassProc(hwnd, msg, wparam, lparam);
        if SendMessageW(hwnd, LVM_GETITEMCOUNT, None, None).0 == 0 {
            let dc = GetDC(Some(hwnd));
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            rect.left += 20;
            rect.top += 52;
            rect.right -= 20;
            let font = HFONT(SendMessageW(hwnd, WM_GETFONT, None, None).0 as *mut _);
            let old = SelectObject(dc, font.into());
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, palette().muted);
            let mut text = wide("No records to show. Start a capture or load saved results.");
            DrawTextW(dc, &mut text, &mut rect, DT_WORDBREAK);
            SelectObject(dc, old);
            ReleaseDC(Some(hwnd), dc);
        }
        return result;
    }
    if msg == WM_NCPAINT {
        let result = DefSubclassProc(hwnd, msg, wparam, lparam);
        let mut info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            ..Default::default()
        };
        if GetScrollInfo(hwnd, SB_VERT, &mut info).is_ok() && info.nMax > info.nPage as i32 {
            let dc = GetWindowDC(Some(hwnd));
            let mut window = RECT::default();
            let _ = GetWindowRect(hwnd, &mut window);
            let width = (GetSystemMetrics(SM_CXVSCROLL) as i32).max(12);
            let h = (window.bottom - window.top).max(1);
            let track = RECT {
                left: window.right - window.left - width,
                top: 0,
                right: window.right - window.left,
                bottom: h,
            };
            fill(dc, &track, palette().bg);
            let travel = (h - 10).max(1);
            let thumb_h = ((h as i64 * info.nPage as i64) / (info.nMax as i64 + 1))
                .clamp(28, h as i64 - 4) as i32;
            let max_pos = (info.nMax - info.nPage as i32).max(1);
            let thumb_y =
                5 + ((travel - thumb_h) as i64 * info.nPos as i64 / max_pos as i64) as i32;
            graphs::rounded_rect(
                dc,
                1.,
                RECT {
                    left: track.left + 3,
                    top: thumb_y,
                    right: track.right - 3,
                    bottom: thumb_y + thumb_h,
                },
                palette().border,
                palette().border,
                4.,
            );
            ReleaseDC(Some(hwnd), dc);
        }
        return result;
    }
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(table_subclass), id);
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}
pub unsafe fn control_color(msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if !matches!(
        msg,
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN
    ) {
        return None;
    }
    let p = palette();
    let dc = HDC(wparam.0 as *mut _);
    let bg = if msg == WM_CTLCOLORBTN
        || (msg == WM_CTLCOLOREDIT && GetDlgCtrlID(HWND(lparam.0 as *mut _)) == 192)
    {
        p.bg
    } else {
        p.surface
    };
    SetTextColor(dc, p.text);
    SetBkColor(dc, bg);
    SetDCBrushColor(dc, bg);
    Some(LRESULT(GetStockObject(DC_BRUSH).0 as isize))
}
pub unsafe fn draw_button(item: &DRAWITEMSTRUCT) {
    let p = palette();
    let dc = item.hDC;
    if matches!(item.CtlID, 197..=199) {
        draw_caption(item);
        return;
    }
    let selected = match item.CtlID {
        181 | 182 => {
            let mut value = [0u16; 32];
            let n = GetWindowTextW(item.hwndItem, &mut value).max(0) as usize;
            matches!(
                String::from_utf16_lossy(&value[..n]).as_str(),
                "Pinned" | "Full view"
            )
        }
        189 => PAGE.get() == 6,
        308 => PAGE.get() == 3,
        194..=196 => GetParent(item.hwndItem)
            .ok()
            .and_then(|p| GetDlgItem(Some(p), 180).ok())
            .is_some_and(|c| {
                SendMessageW(c, CB_GETCURSEL, None, None).0 == (item.CtlID - 194) as isize
            }),
        _ => PAGE.get() + 301 == item.CtlID as usize,
    };
    let navigation = matches!(
        item.CtlID,
        107 | 115 | 120 | 189 | 301 | 302 | 303 | 305 | 306 | 308
    );
    let segmented = matches!(item.CtlID, 194..=196);
    let icon_only = matches!(item.CtlID, 181 | 182);
    let primary = matches!(item.CtlID, 101 | 106 | 160);
    let hover = !GetPropW(item.hwndItem, w!("SuperOpti.Hot")).0.is_null();
    let primary_bg = if p.dark && !p.high_contrast {
        metric_colors()[0]
    } else {
        p.accent
    };
    let disabled = item.itemState.0 & ODS_DISABLED.0 != 0
        || !windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(item.hwndItem).as_bool();
    let pressed = item.itemState.0 & ODS_SELECTED.0 != 0;
    let bg = if disabled {
        if selected { p.selection } else { p.surface }
    } else if pressed || selected {
        p.selection
    } else if primary {
        primary_bg
    } else if hover {
        if p.dark {
            color(43, 46, 60)
        } else {
            color(237, 238, 247)
        }
    } else if icon_only {
        p.bg
    } else if navigation {
        if p.dark {
            color(28, 30, 40)
        } else {
            color(248, 250, 254)
        }
    } else {
        p.surface
    };
    // Match the actual parent surface around rounded child-control corners.
    let parent_surface = if navigation {
        if p.dark {
            color(28, 30, 40)
        } else {
            color(248, 250, 254)
        }
    } else if matches!(item.CtlID,111..=114|121|122|130|176|194..=196)
        || (item.CtlID == 175 && PAGE.get() == 0)
    {
        p.surface
    } else {
        p.bg
    };
    fill(dc, &item.rcItem, parent_surface);
    let r = item.rcItem;
    let inset = if segmented { 2 } else { 0 };
    graphs::rounded_rect(
        dc,
        1.,
        RECT {
            left: r.left + inset,
            top: r.top + inset,
            right: r.right - 1 - inset,
            bottom: r.bottom - 1 - inset,
        },
        bg,
        if navigation || segmented || icon_only || (primary && !disabled) {
            bg
        } else {
            p.border
        },
        if segmented && !selected {
            0.
        } else {
            6. * GetDpiForWindow(item.hwndItem) as f32 / 96.
        },
    );
    let mut buffer = [0u16; 256];
    let n = GetWindowTextW(item.hwndItem, &mut buffer);
    let font = HFONT(SendMessageW(item.hwndItem, WM_GETFONT, None, None).0 as *mut _);
    let old = SelectObject(dc, font.into());
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(
        dc,
        if disabled {
            p.muted
        } else if p.high_contrast && (primary || pressed) {
            COLORREF(GetSysColor(COLOR_HIGHLIGHTTEXT))
        } else if primary && !pressed {
            if p.dark && !p.high_contrast {
                color(32, 19, 29)
            } else {
                color(255, 255, 255)
            }
        } else {
            p.text
        },
    );
    let mut rect = r;
    if navigation {
        let inset = (14 * GetDpiForWindow(item.hwndItem) / 96) as i32;
        rect.left += inset + 24;
        // Deliberate line icons retain a clear label and native keyboard target.
        let icon_rect = RECT {
            left: r.left + inset,
            top: r.top + (r.bottom - r.top) / 2 - 6,
            right: r.left + inset + 12,
            bottom: r.top + (r.bottom - r.top) / 2 + 6,
        };
        icons::draw(
            dc,
            match item.CtlID {
                107 | 115 => "minimize-2",
                301 => "layout-dashboard",
                189 => "cpu",
                308 => "chart-no-axes-column",
                306 => "arrow-left-right",
                305 => "network",
                302 => "shield-check",
                _ => "settings-2",
            },
            icon_rect,
            if selected {
                metric_colors()[0]
            } else {
                p.muted
            },
        );
    }
    if icon_only {
        let size = (14 * GetDpiForWindow(item.hwndItem) / 96) as i32;
        let cx = (r.left + r.right) / 2;
        let cy = (r.top + r.bottom) / 2;
        icons::draw(
            dc,
            if item.CtlID == 181 {
                "pin"
            } else {
                "minimize-2"
            },
            RECT {
                left: cx - size / 2,
                top: cy - size / 2,
                right: cx + size / 2,
                bottom: cy + size / 2,
            },
            if selected {
                metric_colors()[0]
            } else {
                p.muted
            },
        );
    } else {
        DrawTextW(
            dc,
            &mut buffer[..n as usize],
            &mut rect,
            (if navigation { DT_LEFT } else { DT_CENTER })
                | DT_VCENTER
                | DT_SINGLELINE
                | DT_END_ELLIPSIS,
        );
    }
    if item.itemState.0 & ODS_FOCUS.0 != 0
        && item.itemState.0 & ODS_NOFOCUSRECT.0 == 0
        && SendMessageW(item.hwndItem, WM_QUERYUISTATE, None, None).0 & UISF_HIDEFOCUS as isize == 0
    {
        rect.left += 4;
        rect.right -= 4;
        rect.top += 4;
        rect.bottom -= 4;
        let _ = DrawFocusRect(dc, &rect);
    }
    SelectObject(dc, old);
}
pub unsafe fn custom_draw(app: &App, lparam: LPARAM) -> Option<LRESULT> {
    let hdr = &*(lparam.0 as *const NMHDR);
    if hdr.code != NM_CUSTOMDRAW {
        return None;
    }
    let p = palette();
    if [app.table, app.report, app.detail].contains(&hdr.hwndFrom) {
        let draw = &mut *(lparam.0 as *mut NMLVCUSTOMDRAW);
        return Some(LRESULT(match draw.nmcd.dwDrawStage {
            CDDS_PREPAINT => CDRF_NOTIFYITEMDRAW as isize,
            CDDS_ITEMPREPAINT => {
                let selected = SendMessageW(
                    hdr.hwndFrom,
                    LVM_GETITEMSTATE,
                    Some(WPARAM(draw.nmcd.dwItemSpec)),
                    Some(LPARAM(LVIS_SELECTED.0 as isize)),
                )
                .0 != 0;
                draw.clrText = if selected && p.high_contrast {
                    COLORREF(GetSysColor(COLOR_HIGHLIGHTTEXT))
                } else {
                    p.text
                };
                draw.clrTextBk = if selected { p.selection } else { p.surface };
                (CDRF_NEWFONT | CDRF_NOTIFYPOSTPAINT | CDRF_NOTIFYSUBITEMDRAW) as isize
            }
            stage
                if stage.0 == (CDDS_ITEMPREPAINT.0 | CDDS_SUBITEM.0)
                    && app.page == 1
                    && hdr.hwndFrom == app.report
                    && draw.iSubItem == 0 =>
            {
                let mut rect = RECT {
                    left: LVIR_BOUNDS as i32,
                    ..Default::default()
                };
                SendMessageW(
                    app.report,
                    LVM_GETSUBITEMRECT,
                    Some(WPARAM(draw.nmcd.dwItemSpec)),
                    Some(LPARAM(&mut rect as *mut _ as isize)),
                );
                rect.right = rect.left
                    + SendMessageW(app.report, LVM_GETCOLUMNWIDTH, Some(WPARAM(0)), None).0 as i32;
                let selected = SendMessageW(
                    app.report,
                    LVM_GETITEMSTATE,
                    Some(WPARAM(draw.nmcd.dwItemSpec)),
                    Some(LPARAM(LVIS_SELECTED.0 as isize)),
                )
                .0 != 0;
                let bg = if selected { p.selection } else { p.surface };
                fill(draw.nmcd.hdc, &rect, bg);
                let mut text = [0u16; 128];
                let item = LVITEMW {
                    iSubItem: 0,
                    pszText: windows::core::PWSTR(text.as_mut_ptr()),
                    cchTextMax: 128,
                    ..Default::default()
                };
                let n = SendMessageW(
                    app.report,
                    LVM_GETITEMTEXTW,
                    Some(WPARAM(draw.nmcd.dwItemSpec)),
                    Some(LPARAM(&item as *const _ as isize)),
                )
                .0
                .max(0) as usize;
                let value = String::from_utf16_lossy(&text[..n]);
                let ink = if p.high_contrast {
                    if selected {
                        COLORREF(GetSysColor(COLOR_HIGHLIGHTTEXT))
                    } else {
                        p.text
                    }
                } else {
                    match value.as_str() {
                        "Critical" | "Blocked" => {
                            if p.dark {
                                color(246, 139, 150)
                            } else {
                                color(164, 40, 57)
                            }
                        }
                        "Warning" | "Review" => {
                            if p.dark {
                                color(234, 190, 116)
                            } else {
                                color(134, 86, 15)
                            }
                        }
                        "OK" => {
                            if p.dark {
                                color(121, 215, 181)
                            } else {
                                color(24, 111, 79)
                            }
                        }
                        _ => p.muted,
                    }
                };
                let channel =
                    |shift: u32| (((bg.0 >> shift) & 255) * 9 + ((ink.0 >> shift) & 255)) / 10;
                let tint = color(channel(0), channel(8), channel(16));
                let inset = (6. * app.scale) as i32;
                let badge = RECT {
                    left: rect.left + inset,
                    top: rect.top + (4. * app.scale) as i32,
                    right: rect.right - inset,
                    bottom: rect.bottom - (4. * app.scale) as i32,
                };
                graphs::rounded_rect(draw.nmcd.hdc, 1., badge, tint, tint, 4. * app.scale as f32);
                let old = SelectObject(draw.nmcd.hdc, app.font.into());
                SetBkMode(draw.nmcd.hdc, TRANSPARENT);
                SetTextColor(draw.nmcd.hdc, ink);
                let mut label = badge;
                DrawTextW(
                    draw.nmcd.hdc,
                    &mut text[..n],
                    &mut label,
                    DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_END_ELLIPSIS,
                );
                SelectObject(draw.nmcd.hdc, old);
                CDRF_SKIPDEFAULT as isize
            }
            stage if stage.0 == (CDDS_ITEMPREPAINT.0 | CDDS_SUBITEM.0) => {
                let table = hdr.hwndFrom;
                let column = draw.iSubItem;
                let row = draw.nmcd.dwItemSpec;
                let mut rect = RECT {
                    top: column,
                    left: LVIR_BOUNDS as i32,
                    ..Default::default()
                };
                SendMessageW(
                    table,
                    LVM_GETSUBITEMRECT,
                    Some(WPARAM(row)),
                    Some(LPARAM((&mut rect as *mut RECT) as isize)),
                );
                let width = SendMessageW(
                    table,
                    LVM_GETCOLUMNWIDTH,
                    Some(WPARAM(column as usize)),
                    None,
                )
                .0 as i32;
                if column == 0 {
                    rect.right = rect.left + width;
                }
                if width <= 0 {
                    return Some(LRESULT(CDRF_SKIPDEFAULT as isize));
                }
                let mut buffer = [0u16; 512];
                let item = LVITEMW {
                    iSubItem: column,
                    pszText: windows::core::PWSTR(buffer.as_mut_ptr()),
                    cchTextMax: 512,
                    ..Default::default()
                };
                let length = SendMessageW(
                    table,
                    LVM_GETITEMTEXTW,
                    Some(WPARAM(row)),
                    Some(LPARAM((&item as *const LVITEMW) as isize)),
                )
                .0
                .max(0) as usize;
                let selected = SendMessageW(
                    table,
                    LVM_GETITEMSTATE,
                    Some(WPARAM(row)),
                    Some(LPARAM(LVIS_SELECTED.0 as isize)),
                )
                .0 != 0;
                let bg = if selected { p.selection } else { p.surface };
                fill(draw.nmcd.hdc, &rect, bg);
                let inset = (8.0 * app.scale) as i32;
                let mut text_rect = rect;
                text_rect.left += inset;
                text_rect.right -= inset;
                if column == 0 && table == app.table {
                    let icon_size = (16.0 * app.scale).round() as i32;
                    if let Some(&(pid, created)) = app.displayed_rows.get(row) {
                        let _ = ui::draw_cached_icon(
                            app,
                            draw.nmcd.hdc,
                            pid,
                            created,
                            RECT {
                                left: text_rect.left,
                                top: rect.top + (rect.bottom - rect.top - icon_size) / 2,
                                right: text_rect.left + icon_size,
                                bottom: rect.top + (rect.bottom - rect.top + icon_size) / 2,
                            },
                        );
                    }
                    text_rect.left += (20.0 * app.scale).round() as i32;
                }
                if table == app.table
                    && matches!(column, 2 | 9)
                    && let Ok(score) = String::from_utf16_lossy(&buffer[..length]).parse::<f64>()
                {
                    let bar_width = ((text_rect.right - text_rect.left) as f64
                        * score.clamp(0., 100.)
                        / 100.) as i32;
                    fill(
                        draw.nmcd.hdc,
                        &RECT {
                            left: text_rect.left,
                            top: rect.top + (4.0 * app.scale) as i32,
                            right: text_rect.left + bar_width,
                            bottom: rect.bottom - (4.0 * app.scale) as i32,
                        },
                        if p.high_contrast {
                            p.selection
                        } else {
                            let accent = metric_colors()[0];
                            let blend = |shift: u32| {
                                (((bg.0 >> shift) & 255) * 3 + ((accent.0 >> shift) & 255)) / 4
                            };
                            color(blend(0), blend(8), blend(16))
                        },
                    );
                }
                let primary = if app.page == 1 && table == app.report {
                    column == 1
                } else {
                    column == 0
                };
                SetTextColor(
                    draw.nmcd.hdc,
                    if selected && p.high_contrast {
                        COLORREF(GetSysColor(COLOR_HIGHLIGHTTEXT))
                    } else if primary || selected {
                        p.text
                    } else {
                        p.muted
                    },
                );
                SetBkMode(draw.nmcd.hdc, TRANSPARENT);
                let old = SelectObject(
                    draw.nmcd.hdc,
                    if primary {
                        primary_font(app.scale)
                    } else {
                        app.font
                    }
                    .into(),
                );
                DrawTextW(
                    draw.nmcd.hdc,
                    &mut buffer[..length],
                    &mut text_rect,
                    (if if table == app.table {
                        (1..=9).contains(&column)
                    } else {
                        let mut info = LVCOLUMNW {
                            mask: LVCF_FMT,
                            ..Default::default()
                        };
                        SendMessageW(
                            table,
                            LVM_GETCOLUMNW,
                            Some(WPARAM(column as usize)),
                            Some(LPARAM(&mut info as *mut _ as isize)),
                        );
                        info.fmt.0 & LVCFMT_RIGHT.0 != 0
                    } {
                        DT_RIGHT
                    } else {
                        DT_LEFT
                    }) | DT_VCENTER
                        | DT_SINGLELINE
                        | DT_END_ELLIPSIS,
                );
                SelectObject(draw.nmcd.hdc, old);
                CDRF_SKIPDEFAULT as isize
            }
            CDDS_ITEMPOSTPAINT => {
                let mut rect = RECT {
                    left: LVIR_BOUNDS as i32,
                    ..Default::default()
                };
                SendMessageW(
                    hdr.hwndFrom,
                    LVM_GETITEMRECT,
                    Some(WPARAM(draw.nmcd.dwItemSpec)),
                    Some(LPARAM(&mut rect as *mut _ as isize)),
                );
                let mut client = RECT::default();
                let _ = GetClientRect(hdr.hwndFrom, &mut client);
                rect.left = 0;
                rect.right = client.right;
                fill(
                    draw.nmcd.hdc,
                    &RECT {
                        left: rect.left,
                        top: rect.bottom - 1,
                        right: rect.right,
                        bottom: rect.bottom,
                    },
                    p.border,
                );
                CDRF_DODEFAULT as isize
            }
            _ => CDRF_DODEFAULT as isize,
        }));
    }
    let is_header = [app.table, app.report, app.detail].iter().any(|table| {
        HWND(SendMessageW(*table, LVM_GETHEADER, None, None).0 as *mut _) == hdr.hwndFrom
    });
    if is_header {
        let header = hdr.hwndFrom;
        let draw = &*(lparam.0 as *const NMCUSTOMDRAW);
        if draw.dwDrawStage == CDDS_PREPAINT {
            fill(draw.hdc, &draw.rc, p.bg);
            return Some(LRESULT(CDRF_NOTIFYITEMDRAW as isize));
        }
        if draw.dwDrawStage == CDDS_ITEMPREPAINT {
            fill(draw.hdc, &draw.rc, p.bg);
            let mut text = [0u16; 256];
            let mut item = HDITEMW {
                mask: HDI_TEXT | HDI_FORMAT,
                pszText: windows::core::PWSTR(text.as_mut_ptr()),
                cchTextMax: 256,
                ..Default::default()
            };
            SendMessageW(
                header,
                HDM_GETITEMW,
                Some(WPARAM(draw.dwItemSpec)),
                Some(LPARAM((&mut item as *mut HDITEMW) as isize)),
            );
            let mut rect = draw.rc;
            let inset = (8 * GetDpiForWindow(header) / 96) as i32;
            rect.left += inset;
            rect.right -= inset;
            let old = SelectObject(draw.hdc, app.font.into());
            SetTextColor(draw.hdc, p.muted);
            SetBkMode(draw.hdc, TRANSPARENT);
            let length = text.iter().position(|&v| v == 0).unwrap_or(256);
            DrawTextW(
                draw.hdc,
                &mut text[..length],
                &mut rect,
                (if item.fmt.0 & HDF_RIGHT.0 != 0 {
                    DT_RIGHT
                } else {
                    DT_LEFT
                }) | DT_VCENTER
                    | DT_SINGLELINE
                    | DT_END_ELLIPSIS,
            );
            SelectObject(draw.hdc, old);
            fill(
                draw.hdc,
                &RECT {
                    left: draw.rc.left,
                    top: draw.rc.bottom - 1,
                    right: draw.rc.right,
                    bottom: draw.rc.bottom,
                },
                p.border,
            );
            return Some(LRESULT(CDRF_SKIPDEFAULT as isize));
        }
    }
    None
}

/// An empty native small-image slot gives tables a deliberate row rhythm.
pub unsafe fn size_rows(app: &App) {
    for table in [app.table, app.report, app.detail] {
        let height = ((if table == app.table { 28.0 } else { 30.0 }) * app.scale).round() as i32;
        let old = HIMAGELIST(
            SendMessageW(
                table,
                LVM_GETIMAGELIST,
                Some(WPARAM(LVSIL_SMALL as usize)),
                None,
            )
            .0,
        );
        let mut w = 0;
        let mut h = 0;
        if old.0 != 0 {
            let _ = ImageList_GetIconSize(old, Some(&mut w), Some(&mut h));
        }
        if h != height {
            let images = ImageList_Create(
                if table == app.table { height } else { 1 },
                height,
                ILC_COLOR32,
                1,
                1,
            );
            if images.0 != 0 {
                SendMessageW(
                    table,
                    LVM_SETIMAGELIST,
                    Some(WPARAM(LVSIL_SMALL as usize)),
                    Some(LPARAM(images.0 as isize)),
                );
                if old.0 != 0 {
                    let _ = ImageList_Destroy(Some(old));
                }
            }
        }
    }
}

unsafe extern "system" fn combo_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_PAINT {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let p = palette();
        let mut r = RECT::default();
        let _ = GetClientRect(hwnd, &mut r);
        fill(
            dc,
            &r,
            if matches!(GetDlgCtrlID(hwnd), 109 | 168 | 169) {
                p.surface
            } else {
                p.bg
            },
        );
        graphs::rounded_rect(
            dc,
            1.,
            r,
            p.surface,
            p.border,
            6. * GetDpiForWindow(hwnd) as f32 / 96.,
        );
        let index = SendMessageW(hwnd, CB_GETCURSEL, None, None).0;
        let mut value = [0u16; 256];
        let length = if index >= 0 {
            SendMessageW(hwnd, CB_GETLBTEXTLEN, Some(WPARAM(index as usize)), None).0
        } else {
            0
        };
        if (0..256).contains(&length) && index >= 0 {
            SendMessageW(
                hwnd,
                CB_GETLBTEXT,
                Some(WPARAM(index as usize)),
                Some(LPARAM(value.as_mut_ptr() as isize)),
            );
        }
        let font = HFONT(SendMessageW(hwnd, WM_GETFONT, None, None).0 as *mut _);
        let old = SelectObject(dc, font.into());
        SetTextColor(dc, p.text);
        SetBkMode(dc, TRANSPARENT);
        let scale = GetDpiForWindow(hwnd) as i32 / 96;
        let mut text_rect = RECT {
            left: 8 * scale.max(1),
            top: r.top,
            right: r.right - 22 * scale.max(1),
            bottom: r.bottom,
        };
        DrawTextW(
            dc,
            &mut value[..length.clamp(0, 255) as usize],
            &mut text_rect,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        SelectObject(dc, old);
        let pen = CreatePen(PS_SOLID, 1, p.muted);
        let old = SelectObject(dc, pen.into());
        let cx = r.right - 15 * scale.max(1);
        let cy = r.bottom / 2;
        let _ = MoveToEx(dc, cx - 4, cy - 2, None);
        let _ = LineTo(dc, cx, cy + 2);
        let _ = LineTo(dc, cx + 4, cy - 2);
        SelectObject(dc, old);
        let _ = DeleteObject(pen.into());
        if windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() == hwnd
            && SendMessageW(hwnd, WM_QUERYUISTATE, None, None).0 & UISF_HIDEFOCUS as isize == 0
        {
            let mut focus = r;
            focus.left += 3;
            focus.top += 3;
            focus.right -= 3;
            focus.bottom -= 3;
            let _ = DrawFocusRect(dc, &focus);
        }
        let _ = EndPaint(hwnd, &ps);
        return LRESULT(0);
    }
    if matches!(msg, WM_SETFOCUS | WM_KILLFOCUS | CB_SETCURSEL) {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(combo_subclass), id);
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn hover_subclass(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_MOUSEMOVE && GetPropW(hwnd, w!("SuperOpti.Hot")).0.is_null() {
        let _ = SetPropW(
            hwnd,
            w!("SuperOpti.Hot"),
            Some(HANDLE(std::ptr::dangling_mut())),
        );
        let mut track = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        let _ = TrackMouseEvent(&mut track);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    if msg == WM_MOUSELEAVE {
        let _ = RemovePropW(hwnd, w!("SuperOpti.Hot"));
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    if msg == WM_NCDESTROY {
        let _ = RemovePropW(hwnd, w!("SuperOpti.Hot"));
        let _ = RemoveWindowSubclass(hwnd, Some(hover_subclass), id);
    }
    DefSubclassProc(hwnd, msg, w, l)
}
unsafe extern "system" fn toggle_subclass(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_ERASEBKGND {
        return LRESULT(1);
    }
    if msg == WM_PAINT {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut r = RECT::default();
        let _ = GetClientRect(hwnd, &mut r);
        let p = palette();
        fill(dc, &r, p.bg);
        let scale = GetDpiForWindow(hwnd) as f64 / 96.;
        let px = |n: i32| (n as f64 * scale).round() as i32;
        let checked = SendMessageW(hwnd, BM_GETCHECK, None, None).0 == BST_CHECKED.0 as isize;
        let bg = if checked {
            metric_colors()[0]
        } else {
            p.border
        };
        let top = (r.bottom - px(18)) / 2;
        graphs::rounded_rect(
            dc,
            1.,
            RECT {
                left: 0,
                top,
                right: px(32),
                bottom: top + px(18),
            },
            bg,
            bg,
            9. * scale as f32,
        );
        let thumb = if checked && p.dark {
            color(32, 19, 29)
        } else {
            p.text
        };
        let tx = if checked { px(17) } else { px(3) };
        graphs::rounded_rect(
            dc,
            1.,
            RECT {
                left: tx,
                top: top + px(3),
                right: tx + px(12),
                bottom: top + px(15),
            },
            thumb,
            thumb,
            6. * scale as f32,
        );
        let font = HFONT(SendMessageW(hwnd, WM_GETFONT, None, None).0 as *mut _);
        let old = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, p.muted);
        let mut label = wide("Low-impact · 5s interval");
        let mut rect = RECT {
            left: px(42),
            top: 0,
            right: r.right,
            bottom: r.bottom,
        };
        DrawTextW(
            dc,
            &mut label,
            &mut rect,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );
        if windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() == hwnd {
            let _ = DrawFocusRect(dc, &r);
        }
        SelectObject(dc, old);
        let _ = EndPaint(hwnd, &ps);
        return LRESULT(0);
    }
    if matches!(msg, BM_SETCHECK | WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE) {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(toggle_subclass), id);
    }
    DefSubclassProc(hwnd, msg, w, l)
}

unsafe extern "system" fn header_subclass(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    font: usize,
) -> LRESULT {
    if msg == HDM_LAYOUT && l.0 != 0 {
        let result = DefSubclassProc(hwnd, msg, w, l);
        let layout = &mut *(l.0 as *mut HDLAYOUT);
        if !layout.pwpos.is_null() && !layout.prc.is_null() {
            let position = &mut *layout.pwpos;
            let next = (30 * GetDpiForWindow(hwnd) / 96) as i32;
            (*layout.prc).top += next - position.cy;
            position.cy = next;
        }
        return result;
    }
    if msg == WM_PAINT {
        // Column widths can change together when compact mode restores hidden columns.
        let _ = InvalidateRect(Some(hwnd), None, false);
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut bounds = RECT::default();
        let _ = GetClientRect(hwnd, &mut bounds);
        let p = palette();
        fill(dc, &bounds, p.surface);
        let current_font = GetParent(hwnd)
            .ok()
            .map(|p| SendMessageW(p, WM_GETFONT, None, None).0 as usize)
            .unwrap_or(font);
        let old = SelectObject(dc, HFONT(current_font as *mut _).into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, p.muted);
        let count = SendMessageW(hwnd, HDM_GETITEMCOUNT, None, None).0;
        for index in 0..count {
            let mut rect = RECT::default();
            SendMessageW(
                hwnd,
                HDM_GETITEMRECT,
                Some(WPARAM(index as usize)),
                Some(LPARAM(&mut rect as *mut _ as isize)),
            );
            if rect.right <= 0 || rect.left >= bounds.right {
                continue;
            }
            let mut text = [0u16; 256];
            let mut item = HDITEMW {
                mask: HDI_TEXT | HDI_FORMAT,
                pszText: windows::core::PWSTR(text.as_mut_ptr()),
                cchTextMax: 256,
                ..Default::default()
            };
            SendMessageW(
                hwnd,
                HDM_GETITEMW,
                Some(WPARAM(index as usize)),
                Some(LPARAM(&mut item as *mut _ as isize)),
            );
            let inset = (8 * GetDpiForWindow(hwnd) / 96) as i32;
            rect.left += inset;
            rect.right -= inset;
            let n = text.iter().position(|v| *v == 0).unwrap_or(text.len());
            DrawTextW(
                dc,
                &mut text[..n],
                &mut rect,
                DT_SINGLELINE
                    | DT_VCENTER
                    | DT_END_ELLIPSIS
                    | if item.fmt.0 & HDF_RIGHT.0 != 0 {
                        DT_RIGHT
                    } else {
                        DT_LEFT
                    },
            );
        }
        fill(
            dc,
            &RECT {
                left: 0,
                top: bounds.bottom - 1,
                right: bounds.right,
                bottom: bounds.bottom,
            },
            p.border,
        );
        SelectObject(dc, old);
        let _ = EndPaint(hwnd, &ps);
        return LRESULT(0);
    }
    if msg == WM_NCPAINT {
        return LRESULT(0);
    }
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(header_subclass), id);
    }
    DefSubclassProc(hwnd, msg, w, l)
}

unsafe fn draw_caption(item: &DRAWITEMSTRUCT) {
    let p = palette();
    let dc = item.hDC;
    let r = item.rcItem;
    let hover = !GetPropW(item.hwndItem, w!("SuperOpti.Hot")).0.is_null();
    let close = item.CtlID == 199;
    fill(
        dc,
        &r,
        if hover {
            if close { color(196, 43, 54) } else { p.surface }
        } else {
            p.bg
        },
    );
    let scale = GetDpiForWindow(item.hwndItem) as f64 / 96.;
    let size = (10. * scale).round() as i32;
    let cx = (r.left + r.right) / 2;
    let cy = (r.top + r.bottom) / 2;
    let a = cx - size / 2;
    let b = cy - size / 2;
    let pen = CreatePen(
        PS_SOLID,
        scale.round().max(1.) as i32,
        if hover && close {
            color(255, 255, 255)
        } else {
            p.text
        },
    );
    let old = SelectObject(dc, pen.into());
    let brush = SelectObject(dc, GetStockObject(HOLLOW_BRUSH));
    match item.CtlID {
        197 => {
            let _ = MoveToEx(dc, a, cy, None);
            let _ = LineTo(dc, a + size, cy);
        }
        198 => {
            let zoomed = GetParent(item.hwndItem)
                .ok()
                .is_some_and(|h| IsZoomed(h).as_bool());
            if zoomed {
                let d = (3. * scale).round() as i32;
                let _ = Rectangle(dc, a + d, b, a + size, b + size - d);
                let _ = Rectangle(dc, a, b + d, a + size - d, b + size);
            } else {
                let _ = Rectangle(dc, a, b, a + size, b + size);
            }
        }
        _ => {
            let _ = MoveToEx(dc, a, b, None);
            let _ = LineTo(dc, a + size, b + size);
            let _ = MoveToEx(dc, a + size, b, None);
            let _ = LineTo(dc, a, b + size);
        }
    }
    SelectObject(dc, brush);
    SelectObject(dc, old);
    let _ = DeleteObject(pen.into());
    if item.itemState.0 & ODS_FOCUS.0 != 0
        && SendMessageW(item.hwndItem, WM_QUERYUISTATE, None, None).0 & UISF_HIDEFOCUS as isize == 0
    {
        let mut focus = r;
        focus.left += 4;
        focus.top += 4;
        focus.right -= 4;
        focus.bottom -= 4;
        let _ = DrawFocusRect(dc, &focus);
    }
}

thread_local! {static PRIMARY_FONT:std::cell::Cell<(i32,HFONT)>=const{std::cell::Cell::new((0,HFONT(std::ptr::null_mut())))};}
unsafe fn primary_font(scale: f64) -> HFONT {
    let height = -(11. * scale).round() as i32;
    PRIMARY_FONT.with(|cache| {
        let (old_height, old) = cache.get();
        if old_height == height && !old.is_invalid() {
            return old;
        }
        let font = CreateFontW(
            height,
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
            font_face(),
        );
        if !old.is_invalid() {
            let _ = DeleteObject(old.into());
        }
        cache.set((height, font));
        font
    })
}
pub unsafe fn clear_fonts() {
    PRIMARY_FONT.with(|cache| {
        let (_, font) = cache.replace((0, HFONT::default()));
        if !font.is_invalid() {
            let _ = DeleteObject(font.into());
        }
    });
}

unsafe extern "system" fn tabs_subclass(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_ERASEBKGND {
        return LRESULT(1);
    }
    if msg == WM_PAINT {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut bounds = RECT::default();
        let _ = GetClientRect(hwnd, &mut bounds);
        let p = palette();
        fill(dc, &bounds, p.bg);
        graphs::rounded_rect(
            dc,
            1.,
            bounds,
            p.surface,
            p.border,
            6. * GetDpiForWindow(hwnd) as f32 / 96.,
        );
        let count = SendMessageW(hwnd, TCM_GETITEMCOUNT, None, None).0;
        let selected = SendMessageW(hwnd, TCM_GETCURSEL, None, None).0;
        let font = HFONT(SendMessageW(hwnd, WM_GETFONT, None, None).0 as *mut _);
        let old = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        for index in 0..count {
            let mut rect = RECT::default();
            SendMessageW(
                hwnd,
                TCM_GETITEMRECT,
                Some(WPARAM(index as usize)),
                Some(LPARAM(&mut rect as *mut _ as isize)),
            );
            let mut text = [0u16; 128];
            let mut tab = TCITEMW {
                mask: TCIF_TEXT,
                pszText: windows::core::PWSTR(text.as_mut_ptr()),
                cchTextMax: 128,
                ..Default::default()
            };
            SendMessageW(
                hwnd,
                TCM_GETITEMW,
                Some(WPARAM(index as usize)),
                Some(LPARAM(&mut tab as *mut _ as isize)),
            );
            let n = text.iter().position(|v| *v == 0).unwrap_or(text.len());
            SetTextColor(dc, if index == selected { p.text } else { p.muted });
            let mut label = rect;
            label.left += 8;
            label.right -= 8;
            DrawTextW(
                dc,
                &mut text[..n],
                &mut label,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            if index == selected {
                fill(
                    dc,
                    &RECT {
                        left: rect.left + 10,
                        top: bounds.bottom - 3,
                        right: rect.right - 10,
                        bottom: bounds.bottom - 1,
                    },
                    metric_colors()[0],
                );
                if windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() == hwnd
                    && SendMessageW(hwnd, WM_QUERYUISTATE, None, None).0 & UISF_HIDEFOCUS as isize
                        == 0
                {
                    let _ = DrawFocusRect(dc, &label);
                }
            }
        }
        SelectObject(dc, old);
        let _ = EndPaint(hwnd, &ps);
        return LRESULT(0);
    }
    if matches!(msg, TCM_SETCURSEL | WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE) {
        let result = DefSubclassProc(hwnd, msg, w, l);
        let _ = InvalidateRect(Some(hwnd), None, false);
        return result;
    }
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(tabs_subclass), id);
    }
    DefSubclassProc(hwnd, msg, w, l)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn luminance(color: COLORREF) -> f64 {
        let channel = |shift: u32| {
            let v = ((color.0 >> shift) & 255u32) as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        channel(0) * 0.2126 + channel(8) * 0.7152 + channel(16) * 0.0722
    }
    fn contrast(a: COLORREF, b: COLORREF) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    #[test]
    fn themes_preserve_normal_text_and_primary_action_contrast() {
        for dark in [false, true] {
            let p = colors(dark);
            for background in [p.bg, p.surface, p.selection] {
                assert!(
                    contrast(p.text, background) >= 4.5,
                    "main text contrast in dark={dark}"
                );
            }
            for background in [p.bg, p.surface] {
                assert!(
                    contrast(p.muted, background) >= 4.5,
                    "secondary text contrast in dark={dark}"
                );
            }
            assert!(
                contrast(color(255, 255, 255), p.accent) >= 4.5,
                "primary action contrast in dark={dark}"
            );
        }
    }
    #[test]
    fn native_gdi_resolves_sans_serif_for_regular_and_semibold() {
        unsafe {
            let dc = CreateCompatibleDC(None);
            for weight in [400, 600] {
                let font = CreateFontW(
                    -14,
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
                    font_face(),
                );
                assert!(!font.is_invalid());
                let old = SelectObject(dc, font.into());
                let mut face = [0u16; 128];
                let n = GetTextFaceW(dc, Some(&mut face));
                let actual = String::from_utf16_lossy(&face[..n.max(0) as usize]);
                SelectObject(dc, old);
                let _ = DeleteObject(font.into());
                assert!(
                    actual.contains("Segoe UI"),
                    "GDI resolved {weight} to {actual:?}"
                );
            }
            let _ = DeleteDC(dc);
        }
    }
}
