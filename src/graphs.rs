//! Antialiased, shape-preserving measured-data curves. No averaging or idle animation.
use std::{ptr::null_mut, sync::OnceLock};
use windows::Win32::{
    Foundation::{COLORREF, POINT, RECT},
    Graphics::{Gdi::*, GdiPlus::*},
};

// Cubic Hermite tangents constrained to each interval's range. Local extrema
// have zero tangent; flat intervals stay flat, so interpolation invents no peaks.
fn curve(samples: &[(f32, f32)]) -> Vec<PointF> {
    if samples.len() < 2 {
        return Vec::new();
    }
    let mut slopes = Vec::with_capacity(samples.len() - 1);
    for pair in samples.windows(2) {
        let dx = pair[1].0 - pair[0].0;
        if dx <= 0. || !dx.is_finite() || !pair[0].1.is_finite() || !pair[1].1.is_finite() {
            return Vec::new();
        }
        slopes.push((pair[1].1 - pair[0].1) / dx);
    }
    let mut tangent = vec![0.; samples.len()];
    tangent[0] = slopes[0];
    tangent[samples.len() - 1] = slopes[slopes.len() - 1];
    for i in 1..samples.len() - 1 {
        let (a, b) = (slopes[i - 1], slopes[i]);
        if a * b > 0. {
            tangent[i] = 2. * a * b / (a + b);
        }
    }
    for (i, &s) in slopes.iter().enumerate() {
        if s == 0. {
            tangent[i] = 0.;
            tangent[i + 1] = 0.;
        } else {
            let a = tangent[i] / s;
            let b = tangent[i + 1] / s;
            let norm = a * a + b * b;
            if norm > 9. {
                let factor = 3. / norm.sqrt();
                tangent[i] = factor * a * s;
                tangent[i + 1] = factor * b * s;
            }
        }
    }
    let mut points = Vec::with_capacity(3 * samples.len() - 2);
    points.push(PointF {
        X: samples[0].0,
        Y: samples[0].1,
    });
    for i in 0..samples.len() - 1 {
        let ((x, y), (nx, ny)) = (samples[i], samples[i + 1]);
        let third = (nx - x) / 3.;
        points.extend([
            PointF {
                X: x + third,
                Y: y + third * tangent[i],
            },
            PointF {
                X: nx - third,
                Y: ny - third * tangent[i + 1],
            },
            PointF { X: nx, Y: ny },
        ]);
    }
    points
}

unsafe fn startup() -> usize {
    static TOKEN: OnceLock<usize> = OnceLock::new();
    let token = TOKEN.get_or_init(|| {
        let mut token = 0;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            ..Default::default()
        };
        if GdiplusStartup(&mut token, &input, null_mut()) != Ok {
            return 0;
        }
        token
    });
    *token
}

unsafe fn render(
    dc: HDC,
    scale: f32,
    samples: &[(f32, f32)],
    color: COLORREF,
    width: f32,
    fill: Option<(f32, u8)>,
) {
    let token = startup();
    let mut points = curve(samples);
    if points.is_empty() {
        return;
    }
    if token == 0 {
        if fill.is_none() {
            let pen = CreatePen(PS_SOLID, width.round().max(1.) as i32, color);
            let old = SelectObject(dc, pen.into());
            let raw: Vec<_> = samples
                .iter()
                .map(|&(x, y)| POINT {
                    x: x.round() as i32,
                    y: y.round() as i32,
                })
                .collect();
            let _ = Polyline(dc, &raw);
            SelectObject(dc, old);
            let _ = DeleteObject(pen.into());
        }
        return;
    }
    let mut origin = POINT::default();
    let _ = GetViewportOrgEx(dc, &mut origin);
    for p in &mut points {
        p.X = p.X * scale + origin.x as f32;
        p.Y = p.Y * scale + origin.y as f32;
    }
    let saved = SaveDC(dc);
    SetMapMode(dc, MM_TEXT);
    let _ = SetViewportOrgEx(dc, 0, 0, None);
    let mut graphics = null_mut();
    if GdipCreateFromHDC(dc, &mut graphics) == Ok {
        GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
        let rgb = ((color.0 & 255) << 16) | (color.0 & 0xff00) | ((color.0 >> 16) & 255);
        if let Some((baseline, alpha)) = fill {
            let mut path = null_mut();
            let mut brush = null_mut();
            if GdipCreatePath(FillModeAlternate, &mut path) == Ok {
                GdipAddPathBeziers(path, points.as_ptr(), points.len() as i32);
                let first = points[0];
                let last = points[points.len() - 1];
                let y = baseline * scale + origin.y as f32;
                GdipAddPathLine(path, last.X, last.Y, last.X, y);
                GdipAddPathLine(path, last.X, y, first.X, y);
                GdipClosePathFigure(path);
                if GdipCreateSolidFill(((alpha as u32) << 24) | rgb, &mut brush) == Ok {
                    GdipFillPath(graphics, brush.cast(), path);
                    GdipDeleteBrush(brush.cast());
                }
                GdipDeletePath(path);
            }
        } else {
            let mut pen = null_mut();
            if GdipCreatePen1(0xff000000 | rgb, width * scale, UnitPixel, &mut pen) == Ok {
                GdipDrawBeziers(graphics, pen, points.as_ptr(), points.len() as i32);
                GdipDeletePen(pen);
            }
        }
        GdipDeleteGraphics(graphics);
    }
    let _ = RestoreDC(dc, saved);
}

/// Rect/radius use logical units; use scale=1 for child-control pixel DCs.
pub unsafe fn rounded_rect(
    dc: HDC,
    scale: f32,
    rect: RECT,
    fill: COLORREF,
    border: COLORREF,
    radius: f32,
) {
    // Small controls get a 3x raster pass before downsampling. This removes
    // single-pixel stair steps around tight corner radii at 100% display scale.
    let width = ((rect.right - rect.left) as f32 * scale).round() as i32;
    let height = ((rect.bottom - rect.top) as f32 * scale).round() as i32;
    if width > 0 && width <= 600 && height > 0 && height <= 80 {
        let mut origin = POINT::default();
        let _ = GetViewportOrgEx(dc, &mut origin);
        let x = (rect.left as f32 * scale).round() as i32 + origin.x;
        let y = (rect.top as f32 * scale).round() as i32 + origin.y;
        let raster = CreateCompatibleDC(Some(dc));
        let bitmap = CreateCompatibleBitmap(dc, width * 3, height * 3);
        if !raster.0.is_null() && !bitmap.0.is_null() {
            let old = SelectObject(raster, bitmap.into());
            let saved = SaveDC(dc);
            SetMapMode(dc, MM_TEXT);
            let _ = SetViewportOrgEx(dc, 0, 0, None);
            let _ = StretchBlt(
                raster,
                0,
                0,
                width * 3,
                height * 3,
                Some(dc),
                x,
                y,
                width,
                height,
                SRCCOPY,
            );
            rounded_native(
                raster,
                3.,
                RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                },
                fill,
                border,
                radius * scale,
            );
            SetStretchBltMode(dc, HALFTONE);
            let _ = SetBrushOrgEx(dc, 0, 0, None);
            let _ = StretchBlt(
                dc,
                x,
                y,
                width,
                height,
                Some(raster),
                0,
                0,
                width * 3,
                height * 3,
                SRCCOPY,
            );
            let _ = RestoreDC(dc, saved);
            SelectObject(raster, old);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(raster);
            return;
        }
        if !bitmap.0.is_null() {
            let _ = DeleteObject(bitmap.into());
        }
        if !raster.0.is_null() {
            let _ = DeleteDC(raster);
        }
    }
    rounded_native(dc, scale, rect, fill, border, radius);
}

unsafe fn rounded_native(
    dc: HDC,
    scale: f32,
    rect: RECT,
    fill: COLORREF,
    border: COLORREF,
    radius: f32,
) {
    if startup() == 0 {
        return;
    }
    let mut origin = POINT::default();
    let _ = GetViewportOrgEx(dc, &mut origin);
    let saved = SaveDC(dc);
    SetMapMode(dc, MM_TEXT);
    let _ = SetViewportOrgEx(dc, 0, 0, None);
    let mut graphics = null_mut();
    if GdipCreateFromHDC(dc, &mut graphics) == Ok {
        GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
        let x = rect.left as f32 * scale + origin.x as f32 + 0.5;
        let y = rect.top as f32 * scale + origin.y as f32 + 0.5;
        let w = (rect.right - rect.left) as f32 * scale - 1.;
        let h = (rect.bottom - rect.top) as f32 * scale - 1.;
        let d = (radius * 2. * scale).min(w).min(h).max(0.1);
        let mut path = null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path) == Ok {
            GdipAddPathArc(path, x, y, d, d, 180., 90.);
            GdipAddPathArc(path, x + w - d, y, d, d, 270., 90.);
            GdipAddPathArc(path, x + w - d, y + h - d, d, d, 0., 90.);
            GdipAddPathArc(path, x, y + h - d, d, d, 90., 90.);
            GdipClosePathFigure(path);
            let argb = |c: COLORREF| {
                0xff000000 | ((c.0 & 255) << 16) | (c.0 & 0xff00) | ((c.0 >> 16) & 255)
            };
            let mut brush = null_mut();
            if GdipCreateSolidFill(argb(fill), &mut brush) == Ok {
                GdipFillPath(graphics, brush.cast(), path);
                GdipDeleteBrush(brush.cast());
            }
            let mut pen = null_mut();
            if GdipCreatePen1(argb(border), scale.max(1.), UnitPixel, &mut pen) == Ok {
                GdipDrawPath(graphics, pen, path);
                GdipDeletePen(pen);
            }
            GdipDeletePath(path);
        }
        GdipDeleteGraphics(graphics);
    }
    let _ = RestoreDC(dc, saved);
}

pub unsafe fn line(dc: HDC, scale: f32, points: &[(f32, f32)], color: COLORREF, width: f32) {
    render(dc, scale, points, color, width, None);
}
pub unsafe fn area(
    dc: HDC,
    scale: f32,
    points: &[(f32, f32)],
    baseline: f32,
    color: COLORREF,
    alpha: u8,
) {
    render(dc, scale, points, color, 0., Some((baseline, alpha)));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interpolation_preserves_samples_and_bounds_even_at_sharp_peaks() {
        let samples = [
            (0., 0.),
            (1., 99.),
            (2., 1.),
            (4., 1.),
            (5., 80.),
            (10., 100.),
        ];
        let points = curve(&samples);
        for (i, pair) in samples.windows(2).enumerate() {
            let p = &points[i * 3..i * 3 + 4];
            assert_eq!((p[0].X, p[0].Y), pair[0]);
            assert_eq!((p[3].X, p[3].Y), pair[1]);
            for step in 0..=100 {
                let t = step as f32 / 100.;
                let u = 1. - t;
                let y = u * u * u * p[0].Y
                    + 3. * u * u * t * p[1].Y
                    + 3. * u * t * t * p[2].Y
                    + t * t * t * p[3].Y;
                assert!(
                    y >= pair[0].1.min(pair[1].1) - 0.001 && y <= pair[0].1.max(pair[1].1) + 0.001
                );
            }
        }
        assert!(curve(&[(0., 1.), (0., 2.)]).is_empty());
        assert!(curve(&[(0., 1.), (1., f32::NAN)]).is_empty());
    }
}
