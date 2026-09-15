//! Unread-count badges for the tray and taskbar icons.
//!
//! egui's fonts cannot be reached from the tray thread, and a font crate for at most three
//! characters is not worth the dependency, so the glyphs are a scaled 3x5 bitmap font.

/// Discord's rule, and the one the sidebar badges use: a real number up to 99, then `99+`.
pub fn label(count: u32) -> String {
    if count > 99 {
        "99+".to_owned()
    } else {
        count.to_string()
    }
}

/// 3x5, one bit per pixel, most significant bit leftmost in each row.
fn glyph(c: char) -> Option<[u8; 5]> {
    Some(match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b001, 0b001],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        _ => return None,
    })
}

const GLYPH_W: i32 = 3;
const GLYPH_H: i32 = 5;
const GAP: i32 = 1;

const BADGE_BG: [u8; 4] = [0xE0, 0x4C, 0x4C, 0xFF];
const BADGE_FG: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

/// Paint a count onto an RGBA image, bottom-right, in place.
///
/// A count of zero paints nothing, so the caller can pass the number through without deciding.
pub fn draw(rgba: &mut [u8], w: u32, h: u32, count: u32) {
    if count == 0 || w < 8 || h < 8 {
        return;
    }
    let text = label(count);
    let chars: Vec<[u8; 5]> = text.chars().filter_map(glyph).collect();
    if chars.is_empty() {
        return;
    }

    // The badge takes a fixed share of the icon, and the glyph scale follows from what fits.
    let (w, h) = (w as i32, h as i32);
    let pad = (w.min(h) / 16).max(1);
    let badge_h = (h * 7 / 16).max(GLYPH_H + 2);
    let scale = ((badge_h - 2 * pad) / GLYPH_H).max(1);
    let text_w = chars.len() as i32 * GLYPH_W * scale + (chars.len() as i32 - 1) * GAP * scale;
    let badge_w = (text_w + 4 * pad).min(w);
    let x0 = w - badge_w;
    let y0 = h - badge_h;

    let radius = (badge_h / 2) as f32;
    let put = |rgba: &mut [u8], x: i32, y: i32, c: [u8; 4]| {
        if x < 0 || y < 0 || x >= w || y >= h {
            return;
        }
        let i = ((y * w + x) * 4) as usize;
        rgba[i..i + 4].copy_from_slice(&c);
    };

    // A pill rather than a circle: "99+" does not fit in a circle that leaves the logo visible.
    for y in y0..h {
        for x in x0..w {
            let fy = y as f32 + 0.5;
            let fx = x as f32 + 0.5;
            let cy = y0 as f32 + radius;
            let left = x0 as f32 + radius;
            let right = w as f32 - radius;
            let inside = if fx < left {
                let (dx, dy) = (fx - left, fy - cy);
                dx * dx + dy * dy <= radius * radius
            } else if fx > right {
                let (dx, dy) = (fx - right, fy - cy);
                dx * dx + dy * dy <= radius * radius
            } else {
                (fy - cy).abs() <= radius
            };
            if inside {
                put(rgba, x, y, BADGE_BG);
            }
        }
    }

    let tx = x0 + (badge_w - text_w) / 2;
    let ty = y0 + (badge_h - GLYPH_H * scale) / 2;
    for (i, g) in chars.iter().enumerate() {
        let gx = tx + i as i32 * (GLYPH_W + GAP) * scale;
        for (row, bits) in g.iter().enumerate() {
            for col in 0..GLYPH_W {
                if bits & (1 << (GLYPH_W - 1 - col)) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        put(
                            rgba,
                            gx + col * scale + dx,
                            ty + row as i32 * scale + dy,
                            BADGE_FG,
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank(w: u32, h: u32) -> Vec<u8> {
        vec![0u8; (w * h * 4) as usize]
    }

    fn painted(rgba: &[u8]) -> usize {
        rgba.chunks_exact(4).filter(|p| p[3] != 0).count()
    }

    #[test]
    fn counts_read_the_way_the_sidebar_reads_them() {
        assert_eq!(label(1), "1");
        assert_eq!(label(99), "99");
        assert_eq!(label(100), "99+");
        assert_eq!(label(4000), "99+");
    }

    #[test]
    fn zero_paints_nothing() {
        let mut img = blank(64, 64);
        draw(&mut img, 64, 64, 0);
        assert_eq!(painted(&img), 0);
    }

    #[test]
    fn a_badge_paints_and_stays_inside_the_icon() {
        for count in [1u32, 9, 42, 99, 100, 9999] {
            let (w, h) = (64u32, 64u32);
            let mut img = blank(w, h);
            draw(&mut img, w, h, count);
            assert!(painted(&img) > 20, "{count} painted almost nothing");
            assert_eq!(img.len(), (w * h * 4) as usize, "{count} resized the image");
            // Bottom-right: nothing may land in the top-left quadrant.
            for y in 0..h / 2 {
                for x in 0..w / 2 {
                    let i = ((y * w + x) * 4) as usize;
                    assert_eq!(img[i + 3], 0, "{count} painted at {x},{y}");
                }
            }
        }
    }

    /// Wider text must make a wider badge, or "99+" would be clipped into "99".
    #[test]
    fn a_longer_count_takes_more_room() {
        let (w, h) = (64u32, 64u32);
        let mut one = blank(w, h);
        draw(&mut one, w, h, 1);
        let mut many = blank(w, h);
        draw(&mut many, w, h, 100);
        assert!(painted(&many) > painted(&one), "99+ is no wider than 1");
    }

    /// An icon too small for a legible badge is left alone rather than scribbled on.
    #[test]
    fn a_tiny_icon_is_left_alone() {
        let mut img = blank(4, 4);
        draw(&mut img, 4, 4, 5);
        assert_eq!(painted(&img), 0);
    }

    #[test]
    fn every_glyph_the_label_can_produce_exists() {
        for n in [0u32, 1, 7, 10, 99, 100] {
            for c in label(n).chars() {
                assert!(glyph(c).is_some(), "no glyph for {c:?}");
            }
        }
    }
}
