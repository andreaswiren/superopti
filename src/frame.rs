//! Native resizing, dragging and Snap hit testing for the integrated title bar.
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        UI::{
            Controls::*,
            HiDpi::*,
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

pub unsafe fn install(hwnd: HWND) {
    let margins = MARGINS {
        cxLeftWidth: 1,
        cxRightWidth: 1,
        cyTopHeight: 1,
        cyBottomHeight: 1,
    };
    let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
    if let Ok(button) = GetDlgItem(Some(hwnd), 198) {
        let _ = SetWindowSubclass(button, Some(maximize_hit), 198, 0);
    }
    let _ = SetWindowPos(
        hwnd,
        None,
        0,
        0,
        0,
        0,
        SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
}
unsafe extern "system" fn maximize_hit(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    if msg == WM_NCHITTEST {
        return LRESULT(HTMAXBUTTON as isize);
    }
    if matches!(
        msg,
        WM_NCLBUTTONDOWN | WM_NCLBUTTONUP | WM_NCMOUSEMOVE | WM_NCMOUSELEAVE
    ) && let Ok(parent) = GetParent(hwnd)
    {
        return SendMessageW(parent, msg, Some(wparam), Some(lparam));
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}
pub unsafe fn handle(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if wparam.0 == HTMAXBUTTON as usize {
        if msg == WM_NCLBUTTONDOWN {
            return Some(LRESULT(0));
        }
        if msg == WM_NCLBUTTONUP {
            let command = if IsZoomed(hwnd).as_bool() {
                SC_RESTORE
            } else {
                SC_MAXIMIZE
            };
            let _ = PostMessageW(
                Some(hwnd),
                WM_SYSCOMMAND,
                WPARAM(command as usize),
                LPARAM(0),
            );
            return Some(LRESULT(0));
        }
    }
    if msg == WM_NCCALCSIZE && wparam.0 != 0 {
        let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
        if IsZoomed(hwnd).as_bool() {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                params.rgrc[0] = info.rcWork;
            }
        }
        return Some(LRESULT(0));
    }
    if msg == WM_NCHITTEST {
        let mut dwm = LRESULT(0);
        if DwmDefWindowProc(hwnd, msg, wparam, lparam, &mut dwm).as_bool()
            && dwm.0 != HTCLIENT as isize
        {
            return Some(dwm);
        }
        let mut point = POINT {
            x: lparam.0 as u16 as i16 as i32,
            y: (lparam.0 >> 16) as u16 as i16 as i32,
        };
        let _ = ScreenToClient(hwnd, &mut point);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let dpi = GetDpiForWindow(hwnd);
        let border = GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi)
            + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
        if !IsZoomed(hwnd).as_bool() {
            let left = point.x < border;
            let right = point.x >= rect.right - border;
            let top = point.y < border;
            let bottom = point.y >= rect.bottom - border;
            let hit = match (left, right, top, bottom) {
                (true, _, true, _) => HTTOPLEFT,
                (_, true, true, _) => HTTOPRIGHT,
                (true, _, _, true) => HTBOTTOMLEFT,
                (_, true, _, true) => HTBOTTOMRIGHT,
                (true, _, _, _) => HTLEFT,
                (_, true, _, _) => HTRIGHT,
                (_, _, true, _) => HTTOP,
                (_, _, _, true) => HTBOTTOM,
                _ => HTCLIENT,
            };
            if hit != HTCLIENT {
                return Some(LRESULT(hit as isize));
            }
        }
        if let Ok(button) = GetDlgItem(Some(hwnd), 198) {
            let mut max = RECT::default();
            let _ = GetWindowRect(button, &mut max);
            let mut cursor = POINT {
                x: point.x,
                y: point.y,
            };
            let _ = ClientToScreen(hwnd, &mut cursor);
            if cursor.x >= max.left
                && cursor.x < max.right
                && cursor.y >= max.top
                && cursor.y < max.bottom
            {
                return Some(LRESULT(HTMAXBUTTON as isize));
            }
        }
        if point.y < (28 * dpi / 96) as i32 {
            return Some(LRESULT(HTCAPTION as isize));
        }
        return Some(LRESULT(HTCLIENT as isize));
    }
    if matches!(msg, WM_NCMOUSEMOVE | WM_NCMOUSELEAVE) {
        if let Ok(button) = GetDlgItem(Some(hwnd), 198) {
            if msg == WM_NCMOUSEMOVE && wparam.0 == HTMAXBUTTON as usize {
                let _ = SetPropW(
                    button,
                    w!("SuperOpti.Hot"),
                    Some(HANDLE(std::ptr::dangling_mut())),
                );
            } else {
                let _ = RemovePropW(button, w!("SuperOpti.Hot"));
            }
            let _ = InvalidateRect(Some(button), None, false);
        }
        let mut result = LRESULT(0);
        let _ = DwmDefWindowProc(hwnd, msg, wparam, lparam, &mut result);
    }
    None
}
