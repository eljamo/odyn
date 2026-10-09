//! odyn's overrides of nebula-tui: the wordmark and the kiln that replaces
//! nebula's galaxy on the splash and the empty GRID's welcome. Each seam in
//! upstream code carries an `// odyn:` comment.
//!
//! "Odyn" is Welsh for kiln.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

/// 5-row block bitmaps for O D Y N, drawn by nebula's wordmark code: `#` is
/// a block, `.` is empty.
pub const WORDMARK: &[&[&str; 5]] = &[
    &[".##.", "#..#", "#..#", "#..#", ".##."],
    &["###.", "#..#", "#..#", "#..#", "###."],
    &["#...#", ".#.#.", "..#..", "..#..", "..#.."],
    &["#...#", "##..#", "#.#.#", "#..##", "#...#"],
];

/// The wordmark's gradient, left to right (256-color indices): ember red to
/// flame yellow.
pub const MARK: &[u8] = &[160, 166, 202, 208, 214, 220];

/// The kiln, edit freely. Every row must be the same width.
///
/// - `%` is fire: it flickers.
/// - `:` is brick: drawn dim, and warmed by the fire when near the mouth.
/// - Any other character is outline.
/// - A space is empty. Between a row's first and last character it is still
///   the kiln's body, so no star shows through.
///
/// Smoke rises from the middle of the top row.
pub const KILN: &[&str] = &[
    r"           _====_           ",
    r"           |::::|           ",
    r"           |::::|           ",
    r"          /::::::\          ",
    r"        _/::::::::\_        ",
    r"      _/::::::::::::\_      ",
    r"     /::::::::::::::::\     ",
    r"    /::::::::::::::::::\    ",
    r"   |::::::::::::::::::::|   ",
    r"   |:::::::.----.:::::::|   ",
    r"   |:::::::|%%%%|:::::::|   ",
    r"   |:::::::|%%%%|:::::::|   ",
    r"  _|_______|%%%%|_______|_  ",
    r" /________________________\ ",
];

const OUTLINE: u8 = 173;
const BRICK: u8 = 95;
const BRICK_WARM: u8 = 131;
const FLAME: &[char] = &['.', '\'', '^', '*', '%', '#'];
const FLAME_COLORS: &[u8] = &[52, 88, 160, 202, 214, 226];
const SMOKE: &[char] = &['.', ':', 'o', 'O'];
const SMOKE_COLORS: &[u8] = &[237, 239, 242, 245];
/// How many rows of smoke rise above the chimney at most.
const SMOKE_ROWS: u16 = 12;

/// Draws the kiln, its smoke and a starfield across `area`, `t` seconds into
/// the scene, with `fade` (0 → 1) for the fade-in. The kiln sits on the row
/// above `text`, and nothing is drawn on a band around `text`. When the sky
/// is too small for the kiln, only the stars are drawn.
///
/// Returns true when it drew the sky, so nebula's galaxy is skipped.
pub fn draw_sky(
    buf: &mut Buffer,
    area: Rect,
    text: Rect,
    t: f32,
    fade: f32,
    accent: Color,
) -> bool {
    let carve = Rect {
        x: text.x.saturating_sub(3),
        y: text.y.saturating_sub(1),
        width: text.width + 6,
        height: text.height + 2,
    }
    .intersection(area);
    let in_carve = |x: u16, y: u16| {
        x >= carve.left() && x < carve.right() && y >= carve.top() && y < carve.bottom()
    };

    let kiln_w = KILN[0].chars().count() as u16;
    let kiln_h = KILN.len() as u16;
    let sky_bottom = carve.top().max(area.top());
    let fits = area.width >= kiln_w && sky_bottom >= area.top() + kiln_h;
    let kiln = fits.then(|| Rect {
        x: area.x + (area.width - kiln_w) / 2,
        y: sky_bottom - kiln_h,
        width: kiln_w,
        height: kiln_h,
    });

    // Painted cells, so the stars stay off the kiln and the smoke.
    let mut taken = vec![false; usize::from(area.width) * usize::from(area.height)];
    let mut take = |x: u16, y: u16| {
        taken[usize::from(y - area.y) * usize::from(area.width) + usize::from(x - area.x)] = true;
    };

    if let Some(k) = kiln {
        draw_kiln(buf, k, t, fade, &mut take);
        draw_smoke(buf, area, k, t, fade, &in_carve, &mut take);
    }
    draw_stars(buf, area, t, fade, accent, |x, y| {
        in_carve(x, y)
            || taken[usize::from(y - area.y) * usize::from(area.width) + usize::from(x - area.x)]
    });
    true
}

fn draw_kiln(buf: &mut Buffer, k: Rect, t: f32, fade: f32, take: &mut impl FnMut(u16, u16)) {
    // The mouth's centre, for warming the brick around it.
    let mouth = KILN.iter().enumerate().find_map(|(row, line)| {
        let col = line.find('%')?;
        let width = line.matches('%').count();
        Some((col as f32 + width as f32 / 2.0, row as f32))
    });
    let fire_top = KILN.iter().position(|l| l.contains('%')).unwrap_or(0);
    let fire_rows = KILN.iter().filter(|l| l.contains('%')).count().max(1);
    let flicker = 0.5 + 0.5 * (t * 7.0).sin() * (t * 3.1).cos();

    for (row, line) in KILN.iter().enumerate() {
        let first = line.find(|c: char| c != ' ');
        let last = line.rfind(|c: char| c != ' ');
        let (Some(first), Some(last)) = (first, last) else {
            continue;
        };
        for (col, ch) in line.chars().enumerate() {
            if col < first || col > last {
                continue;
            }
            let (x, y) = (k.x + col as u16, k.y + row as u16);
            take(x, y);
            // Materialise cell by cell as the scene fades in.
            if hash01(i32::from(x), i32::from(y), 77) > fade * 1.15 {
                continue;
            }
            let cell = &mut buf[(x, y)];
            match ch {
                ' ' => {}
                '%' => {
                    let depth = (row - fire_top) as f32 / fire_rows as f32;
                    let heat = (vnoise(col as f32 * 0.9, row as f32 * 1.3 + t * 5.0, 7) * 0.75
                        + depth * 0.35
                        + flicker * 0.15)
                        .clamp(0.0, 1.0);
                    let i = (heat * (FLAME.len() - 1) as f32).round() as usize;
                    cell.set_char(FLAME[i])
                        .set_fg(Color::Indexed(FLAME_COLORS[i]));
                }
                ':' => {
                    let warm = mouth.is_some_and(|(mx, my)| {
                        let (dx, dy) = (col as f32 - mx, (row as f32 - my) * 2.0);
                        (dx * dx + dy * dy).sqrt() < 4.0 + flicker * 2.0
                    });
                    cell.set_char(':').set_fg(Color::Indexed(if warm {
                        BRICK_WARM
                    } else {
                        BRICK
                    }));
                }
                _ => {
                    cell.set_char(ch).set_fg(Color::Indexed(OUTLINE));
                }
            }
        }
    }
}

fn draw_smoke(
    buf: &mut Buffer,
    area: Rect,
    k: Rect,
    t: f32,
    fade: f32,
    in_carve: &impl Fn(u16, u16) -> bool,
    take: &mut impl FnMut(u16, u16),
) {
    let chimney = f32::from(k.x) + f32::from(k.width) / 2.0;
    let rows = SMOKE_ROWS.min(k.y.saturating_sub(area.y));
    for step in 1..=rows {
        let y = k.y - step;
        let s = f32::from(step);
        // The column drifts more and spreads wider as it rises.
        let centre = chimney + (s * 0.35 - t * 0.8).sin() * s * 0.25;
        let half = 1.0 + s * 0.35;
        let thin = 1.0 - s / f32::from(SMOKE_ROWS + 1);
        let left = (centre - half).floor().max(f32::from(area.left())) as u16;
        let right = (centre + half).ceil().min(f32::from(area.right() - 1)) as u16;
        for x in left..=right {
            if in_carve(x, y) {
                continue;
            }
            let edge = 1.0 - ((f32::from(x) - centre).abs() / half).min(1.0);
            let puff = vnoise(f32::from(x) * 0.6, (f32::from(y) + t * 2.0) * 0.5, 31);
            let d = edge * puff * thin * fade;
            if d < 0.12 {
                continue;
            }
            let i = (((d - 0.12) / 0.6) * (SMOKE.len() - 1) as f32)
                .round()
                .min((SMOKE.len() - 1) as f32) as usize;
            buf[(x, y)]
                .set_char(SMOKE[i])
                .set_fg(Color::Indexed(SMOKE_COLORS[i]));
            take(x, y);
        }
    }
}

/// Sparse stars on their own twinkle phases, plus the rare accent-coloured
/// sparkle. The same sky as nebula's, so the scene feels like home.
fn draw_stars(
    buf: &mut Buffer,
    area: Rect,
    t: f32,
    fade: f32,
    accent: Color,
    skip: impl Fn(u16, u16) -> bool,
) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if skip(x, y) {
                continue;
            }
            let h = hash(i32::from(x), i32::from(y), 12_345);
            if !h.is_multiple_of(53) {
                continue;
            }
            let phase = ((h >> 8) % 8) as f32 * 0.8;
            let tw = ((t * 2.5 + phase).sin() * 0.5 + 0.5) * fade;
            if tw <= 0.45 {
                continue;
            }
            if (h >> 4).is_multiple_of(111) {
                buf[(x, y)].set_char('+').set_fg(accent);
            } else if tw > 0.8 {
                buf[(x, y)].set_char('·').set_fg(Color::Indexed(189));
            } else {
                buf[(x, y)].set_char('.').set_fg(Color::Indexed(60));
            }
        }
    }
}

fn hash(x: i32, y: i32, salt: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374_761_393)
        ^ (y as u32).wrapping_mul(668_265_263)
        ^ salt.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

fn hash01(x: i32, y: i32, salt: u32) -> f32 {
    (hash(x, y, salt) & 0xffff) as f32 / 65535.0
}

/// One octave of smooth 2D value noise.
fn vnoise(x: f32, y: f32, salt: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let a = hash01(xi, yi, salt);
    let b = hash01(xi + 1, yi, salt);
    let c = hash01(xi, yi + 1, salt);
    let d = hash01(xi + 1, yi + 1, salt);
    a + (b - a) * sx + (c - a) * sy + (a - b - c + d) * sx * sy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiln_rows_are_one_width() {
        let width = KILN[0].chars().count();
        for (row, line) in KILN.iter().enumerate() {
            assert_eq!(line.chars().count(), width, "row {row}: {line:?}");
        }
    }

    #[test]
    fn wordmark_letters_are_five_rows_of_one_width() {
        for letter in WORDMARK {
            assert!(
                letter.iter().all(|row| row.len() == letter[0].len()),
                "{letter:?}"
            );
        }
    }
}
