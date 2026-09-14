//! Small native stroke icons adapted from the pinned Lucide sources in assets/lucide.
//! No icon font, web renderer, shell extension, or background work is required.
use windows::Win32::{
    Foundation::{COLORREF, POINT, RECT},
    Graphics::Gdi::*,
};

pub unsafe fn draw(dc: HDC, name: &str, rect: RECT, color: COLORREF) {
    let scale = (rect.right - rect.left).min(rect.bottom - rect.top) as f64 / 24.;
    let point = |x: f64, y: f64| POINT {
        x: rect.left + (x * scale).round() as i32,
        y: rect.top + (y * scale).round() as i32,
    };
    let pen = CreatePen(PS_SOLID, (1.6 * scale).round().max(1.) as i32, color);
    let old_pen = SelectObject(dc, pen.into());
    let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
    let line = |xy: &[(f64, f64)]| {
        let points: Vec<_> = xy.iter().map(|&(x, y)| point(x, y)).collect();
        let _ = Polyline(dc, &points);
    };
    let box_at = |x: f64, y: f64, w: f64, h: f64| {
        let a = point(x, y);
        let b = point(x + w, y + h);
        let _ = RoundRect(
            dc,
            a.x,
            a.y,
            b.x,
            b.y,
            (2. * scale).round() as i32,
            (2. * scale).round() as i32,
        );
    };
    let circle = |x: f64, y: f64, r: f64| {
        let a = point(x - r, y - r);
        let b = point(x + r, y + r);
        let _ = Ellipse(dc, a.x, a.y, b.x, b.y);
    };
    match name {
        "audio-lines" => {
            for (x, a, b) in [
                (2., 10., 13.),
                (6., 6., 17.),
                (10., 3., 21.),
                (14., 8., 15.),
                (18., 5., 18.),
                (22., 10., 13.),
            ] {
                line(&[(x, a), (x, b)]);
            }
        }
        "layout-dashboard" => {
            box_at(3., 3., 7., 9.);
            box_at(14., 3., 7., 5.);
            box_at(14., 12., 7., 9.);
            box_at(3., 16., 7., 5.);
        }
        "cpu" => {
            box_at(4., 4., 16., 16.);
            box_at(8., 8., 8., 8.);
            for n in [7., 12., 17.] {
                line(&[(n, 2.), (n, 4.)]);
                line(&[(n, 20.), (n, 22.)]);
                line(&[(2., n), (4., n)]);
                line(&[(20., n), (22., n)]);
            }
        }
        "chart-no-axes-column" => {
            line(&[(5., 21.), (5., 15.)]);
            line(&[(12., 21.), (12., 3.)]);
            line(&[(19., 21.), (19., 9.)]);
        }
        "arrow-left-right" => {
            line(&[(8., 3.), (3., 8.), (8., 13.)]);
            line(&[(3., 8.), (21., 8.)]);
            line(&[(16., 11.), (21., 16.), (16., 21.)]);
            line(&[(21., 16.), (3., 16.)]);
        }
        "network" => {
            box_at(9., 2., 6., 6.);
            box_at(2., 16., 6., 6.);
            box_at(16., 16., 6., 6.);
            line(&[(12., 8.), (12., 12.)]);
            line(&[(5., 16.), (5., 12.), (19., 12.), (19., 16.)]);
        }
        "shield-check" => {
            // Flattened shield outline retains Lucide's proportions at tray/rail sizes.
            line(&[
                (20., 13.),
                (19.7, 15.5),
                (18.5, 18.),
                (16., 20.),
                (12., 22.),
                (8., 20.),
                (5.5, 18.),
                (4.3, 15.5),
                (4., 13.),
                (4., 6.),
                (5., 5.),
                (8., 4.4),
                (12., 2.),
                (16., 4.4),
                (19., 5.),
                (20., 6.),
                (20., 13.),
            ]);
            line(&[(9., 12.), (11., 14.), (15., 10.)]);
        }
        "settings-2" => {
            line(&[(14., 17.), (5., 17.)]);
            line(&[(19., 7.), (10., 7.)]);
            circle(17., 17., 3.);
            circle(7., 7., 3.);
        }
        "pin" => {
            line(&[(12., 17.), (12., 22.)]);
            line(&[
                (9., 11.),
                (9., 6.),
                (7., 6.),
                (6., 4.),
                (8., 2.),
                (16., 2.),
                (18., 4.),
                (17., 6.),
                (15., 6.),
                (15., 11.),
                (19., 14.),
                (19., 17.),
                (5., 17.),
                (5., 14.),
                (9., 11.),
            ]);
        }
        "minimize-2" => {
            line(&[(4., 14.), (10., 14.), (10., 20.)]);
            line(&[(10., 14.), (3., 21.)]);
            line(&[(20., 10.), (14., 10.), (14., 4.)]);
            line(&[(14., 10.), (21., 3.)]);
        }
        _ => {}
    }
    SelectObject(dc, old_brush);
    SelectObject(dc, old_pen);
    let _ = DeleteObject(pen.into());
}
