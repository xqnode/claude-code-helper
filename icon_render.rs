// 应用图标像素渲染（托盘、窗口与 Windows .ico 共用）。
// 几何与 assets/brand-icon.svg 保持一致。

const BOLT_VERTS_32: &[(f32, f32)] = &[
    (16.0, 5.5),
    (10.5, 16.5),
    (14.0, 16.5),
    (9.5, 26.5),
    (22.5, 13.0),
    (17.5, 13.0),
    (21.5, 5.5),
];

const BOLT_SCALE: f32 = 1.2;
const CORNER_RADIUS_32: f32 = 10.0;

pub fn render_icon_rgba(size: u32) -> Vec<u8> {
    if size <= 48 {
        let scale = if size <= 16 { 4 } else { 2 };
        let big = render_icon_rgba_inner(size * scale);
        downscale_box(&big, size * scale, size)
    } else {
        render_icon_rgba_inner(size)
    }
}

fn render_icon_rgba_inner(size: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let amber_gold = (232u8, 163, 23);
    let amber_gold_hi = (245u8, 200, 74);

    let unit = size as f32 / 32.0;
    let canvas = size as f32;
    let radius = CORNER_RADIUS_32 * unit;
    let center = 16.0 * unit;

    let scaled: Vec<(f32, f32)> = BOLT_VERTS_32
        .iter()
        .map(|&(x, y)| (x * unit, y * unit))
        .collect();
    let bolt = scale_polygon(&scaled, center, center, BOLT_SCALE);

    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            if !in_rounded_rect(px, py, canvas, canvas, radius) {
                continue;
            }

            if point_in_polygon(px, py, &bolt) {
                let color = if py < 13.5 * unit {
                    amber_gold_hi
                } else {
                    amber_gold
                };
                put_pixel(&mut rgba, x, y, size, color, 255);
            } else {
                put_pixel(&mut rgba, x, y, size, background_black(), 255);
            }
        }
    }

    rgba
}

fn downscale_box(rgba: &[u8], from: u32, to: u32) -> Vec<u8> {
    let ratio = from / to;
    let mut out = vec![0u8; (to * to * 4) as usize];
    for y in 0..to {
        for x in 0..to {
            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            let mut a = 0u32;
            let mut n = 0u32;
            for dy in 0..ratio {
                for sx_base in 0..ratio {
                    let sx = x * ratio + sx_base;
                    let sy = y * ratio + dy;
                    let i = ((sy * from + sx) * 4) as usize;
                    r += rgba[i] as u32;
                    g += rgba[i + 1] as u32;
                    b += rgba[i + 2] as u32;
                    a += rgba[i + 3] as u32;
                    n += 1;
                }
            }
            let o = ((y * to + x) * 4) as usize;
            out[o] = (r / n) as u8;
            out[o + 1] = (g / n) as u8;
            out[o + 2] = (b / n) as u8;
            out[o + 3] = (a / n) as u8;
        }
    }
    out
}

fn background_black() -> (u8, u8, u8) {
    (0, 0, 0)
}

fn scale_polygon(verts: &[(f32, f32)], cx: f32, cy: f32, scale: f32) -> Vec<(f32, f32)> {
    verts
        .iter()
        .map(|&(x, y)| (cx + (x - cx) * scale, cy + (y - cy) * scale))
        .collect()
}

fn in_rounded_rect(px: f32, py: f32, w: f32, h: f32, r: f32) -> bool {
    let r = r.min(w / 2.0).min(h / 2.0);
    let dx = (px - w / 2.0).abs();
    let dy = (py - h / 2.0).abs();
    if dx <= w / 2.0 - r {
        return dy <= h / 2.0;
    }
    if dy <= h / 2.0 - r {
        return dx <= w / 2.0;
    }
    let qx = dx - (w / 2.0 - r);
    let qy = dy - (h / 2.0 - r);
    qx * qx + qy * qy <= r * r
}

fn put_pixel(rgba: &mut [u8], x: u32, y: u32, size: u32, rgb: (u8, u8, u8), a: u8) {
    if x >= size || y >= size {
        return;
    }
    let i = ((y * size + x) * 4) as usize;
    rgba[i] = rgb.0;
    rgba[i + 1] = rgb.1;
    rgba[i + 2] = rgb.2;
    rgba[i + 3] = a;
}

fn point_in_polygon(x: f32, y: f32, verts: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let n = verts.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = verts[i];
        let (xj, yj) = verts[j];
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bolt_is_centered_at_32px() {
        let rgba = render_icon_rgba(32);
        let center = sample(&rgba, 32, 16, 16);
        let top_left = sample(&rgba, 32, 6, 6);
        assert!(center.0 > 200, "center should be gold bolt");
        assert_eq!(top_left, (0, 0, 0), "corner should be black background");
    }

    fn sample(rgba: &[u8], size: u32, x: u32, y: u32) -> (u8, u8, u8) {
        let i = ((y * size + x) * 4) as usize;
        (rgba[i], rgba[i + 1], rgba[i + 2])
    }
}
