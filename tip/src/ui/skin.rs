// SPDX-License-Identifier: GPL-3.0-or-later

//! Textured backgrounds for the composition bar and the candidate list, as
//! 自然輸入法 has. Each is drawn here rather than shipped as an image: a
//! few kilobytes of code in both DLLs instead of a picture per skin, and
//! only the one chosen is ever made, once per process.

use std::{f32::consts::TAU, sync::OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skin {
    Wood,
    Washi,
    Marble,
    Bamboo,
    Night,
}

/// Colors that read on a skin, as RRGGBBAA like the settings' own.
pub(crate) struct Palette {
    pub(crate) fg: &'static str,
    pub(crate) selkey: &'static str,
    pub(crate) highlight_fg: &'static str,
    pub(crate) highlight_bg: &'static str,
    pub(crate) border: &'static str,
}

/// A tile repeated over the window, opaque BGRA.
pub(crate) struct Texture {
    pub(crate) size: u32,
    pub(crate) pixels: Vec<u8>,
}

/// Pixels a side: drawn at 192 DPI, the tile is 192 DIPs, sharp on the
/// screens that scale the most and wide enough not to show its repeats.
pub(crate) const TILE: u32 = 384;
pub(crate) const TILE_DPI: f32 = 192.0;

impl Skin {
    pub(crate) const ALL: [Skin; 5] = [
        Skin::Wood,
        Skin::Washi,
        Skin::Marble,
        Skin::Bamboo,
        Skin::Night,
    ];

    /// The skin the setting `name` is, or None for plain colors.
    pub(crate) fn from_name(name: &str) -> Option<Skin> {
        Self::ALL.into_iter().find(|skin| skin.name() == name)
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Skin::Wood => "wood",
            Skin::Washi => "washi",
            Skin::Marble => "marble",
            Skin::Bamboo => "bamboo",
            Skin::Night => "night",
        }
    }

    pub(crate) fn palette(self) -> Palette {
        match self {
            Skin::Wood => Palette {
                fg: "2B1A0EFF",
                selkey: "8A3B12FF",
                highlight_fg: "FFF4E2FF",
                highlight_bg: "5C3A21FF",
                border: "8C6A48FF",
            },
            Skin::Washi => Palette {
                fg: "2A2724FF",
                selkey: "B23A26FF",
                highlight_fg: "FFFFFFFF",
                highlight_bg: "B23A26FF",
                border: "D3C8B4FF",
            },
            Skin::Marble => Palette {
                fg: "23262BFF",
                selkey: "2F6BA8FF",
                highlight_fg: "FFFFFFFF",
                highlight_bg: "2F6BA8FF",
                border: "C3C7CDFF",
            },
            Skin::Bamboo => Palette {
                fg: "2A2F17FF",
                selkey: "4F7A1FFF",
                highlight_fg: "FFFFFFFF",
                highlight_bg: "4F7A1FFF",
                border: "A49E6EFF",
            },
            Skin::Night => Palette {
                fg: "E8ECF6FF",
                selkey: "93B6FFFF",
                highlight_fg: "10141FFF",
                highlight_bg: "B9CCFFFF",
                border: "3B4560FF",
            },
        }
    }

    pub(crate) fn texture(self) -> &'static Texture {
        static TEXTURES: [OnceLock<Texture>; 5] = [const { OnceLock::new() }; 5];
        let index = Self::ALL.iter().position(|&skin| skin == self).unwrap();
        TEXTURES[index].get_or_init(|| {
            let paint: fn(f32, f32) -> [f32; 3] = match self {
                Skin::Wood => wood,
                Skin::Washi => washi,
                Skin::Marble => marble,
                Skin::Bamboo => bamboo,
                Skin::Night => night,
            };
            let mut pixels = Vec::with_capacity((TILE * TILE * 4) as usize);
            for y in 0..TILE {
                for x in 0..TILE {
                    let (u, v) = (x as f32 / TILE as f32, y as f32 / TILE as f32);
                    let [r, g, b] = paint(u, v);
                    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
                    pixels.extend([byte(b), byte(g), byte(r), 255]);
                }
            }
            Texture { size: TILE, pixels }
        })
    }
}

// Each paints the tile at (u, v), both in [0, 1). Every noise repeats a whole
// number of times across the tile, so its edges meet.

fn wood(u: f32, v: f32) -> [f32; 3] {
    // Rings across, warped along the grain, and fine streaks within them.
    let warp = fbm(u * 2.0, v * 3.0, 2, 3, 4, 1);
    let rings = (v * 7.0 + warp * 1.6).fract();
    let ring = (rings * TAU).sin() * 0.5 + 0.5;
    let ring = ring * ring * ring;
    let streaks = fbm(u * 3.0, v * 96.0, 3, 96, 3, 2);
    let pores = noise(u * 192.0, v * 384.0, 192, 384, 3);
    let light = rgb(0xDDBF94);
    let dark = rgb(0xB88A5E);
    let t = 0.55 * ring + 0.35 * streaks + 0.08 * pores;
    mix(light, dark, t.clamp(0.0, 1.0))
}

fn washi(u: f32, v: f32) -> [f32; 3] {
    // Paper with a mottle and its fibers: long thin streaks every way.
    let mottle = fbm(u * 4.0, v * 4.0, 4, 4, 4, 11);
    let fibers_a = ridge(fbm(u * 6.0, v * 48.0, 6, 48, 3, 12));
    let fibers_b = ridge(fbm(u * 48.0, v * 6.0, 48, 6, 3, 13));
    let grain = noise(u * 384.0, v * 384.0, 384, 384, 14);
    let base = rgb(0xF5F0E4);
    let shade = 0.04 * (mottle - 0.5) + 0.03 * fibers_a.powi(12) - 0.02 * fibers_b.powi(12)
        + 0.012 * (grain - 0.5);
    base.map(|c| c + shade)
}

fn marble(u: f32, v: f32) -> [f32; 3] {
    // Veins along a slant, turbulent, over a faint cloud.
    let turbulence = fbm(u * 3.0, v * 3.0, 3, 3, 5, 21);
    let vein = (((u * 2.0 + v) + turbulence * 2.2) * TAU / 2.0).sin().abs();
    let vein = (1.0 - vein).powi(5);
    let fine = (((u * 5.0 - v * 3.0) + fbm(u * 5.0, v * 5.0, 5, 5, 4, 22) * 1.8) * TAU / 2.0)
        .sin()
        .abs();
    let fine = (1.0 - fine).powi(12);
    let cloud = fbm(u * 2.0, v * 2.0, 2, 2, 4, 23);
    let base = mix(rgb(0xF4F3F0), rgb(0xE6E5E2), cloud);
    let veined = mix(base, rgb(0xB4B8BE), 0.4 * vein);
    mix(veined, rgb(0xD2D5D9), 0.25 * fine)
}

fn bamboo(u: f32, v: f32) -> [f32; 3] {
    // A blind: slats down, each its own shade. No threads across: they
    // strike through a row of candidates.
    const SLATS: f32 = 16.0;
    let slat = ((u * SLATS).floor() as i32).rem_euclid(SLATS as i32);
    let across = (u * SLATS).fract();
    let tint = hash(slat, 0, 31) - 0.5;
    let edge = (1.0 - (across * 2.0 - 1.0).abs()).powf(0.35);
    let fibers = fbm(u * 64.0, v * 4.0, 64, 4, 3, 32);
    let base = mix(rgb(0xDCD7AC), rgb(0xCFC895), fibers);
    base.map(|c| (c + 0.025 * tint) * (0.9 + 0.1 * edge))
}

fn night(u: f32, v: f32) -> [f32; 3] {
    // A clouded dark sky, with stars too faint and small to take a letter.
    let cloud = fbm(u * 3.0, v * 3.0, 3, 3, 5, 41);
    let base = mix(rgb(0x1B2132), rgb(0x2A3350), cloud * cloud);
    let pixel = |t: f32| ((t * TILE as f32) as i32).rem_euclid(TILE as i32);
    let (x, y) = (pixel(u), pixel(v));
    let star = hash(x, y, 42);
    if star > 0.9985 {
        let glow = 0.25 + 0.35 * hash(x, y, 43);
        mix(base, [0.9, 0.93, 1.0], glow)
    } else {
        base
    }
}

fn rgb(hex: u32) -> [f32; 3] {
    [hex >> 16, hex >> 8, hex].map(|c| (c & 0xFF) as f32 / 255.0)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// High where `n` crosses the middle: thin lines out of soft noise.
fn ridge(n: f32) -> f32 {
    1.0 - (n * 2.0 - 1.0).abs()
}

/// In [0, 1), the same for the same cell and seed.
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(0x8DA6_B343)
        .wrapping_add((y as u32).wrapping_mul(0xD816_3841))
        .wrapping_add(seed.wrapping_mul(0xCB1A_B31F));
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Value noise in [0, 1) over cells repeating every `px` by `py`.
fn noise(x: f32, y: f32, px: i32, py: i32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (x0, y0) = (x0 as i32, y0 as i32);
    let at = |dx: i32, dy: i32| hash((x0 + dx).rem_euclid(px), (y0 + dy).rem_euclid(py), seed);
    let top = at(0, 0) + (at(1, 0) - at(0, 0)) * sx;
    let bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * sx;
    top + (bottom - top) * sy
}

/// Octaves of noise, each twice as fine and half as strong, in [0, 1).
fn fbm(x: f32, y: f32, px: i32, py: i32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut weight, mut total) = (0.0, 0.5, 0.0);
    for octave in 0..octaves {
        let scale = (1 << octave) as f32;
        let period = 1 << octave;
        sum += weight * noise(x * scale, y * scale, px * period, py * period, seed + octave);
        total += weight;
        weight *= 0.5;
    }
    sum / total
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tile repeats over the window: its opposite edges must meet, or
    /// every repeat shows a seam.
    #[test]
    fn every_tile_meets_itself_at_the_edges() {
        for skin in Skin::ALL {
            let step = 1.0 / TILE as f32;
            for i in 0..TILE {
                let t = i as f32 / TILE as f32;
                let paint: fn(f32, f32) -> [f32; 3] = match skin {
                    Skin::Wood => wood,
                    Skin::Washi => washi,
                    Skin::Marble => marble,
                    Skin::Bamboo => bamboo,
                    Skin::Night => night,
                };
                // Going past an edge is the same as coming in at the other.
                for (a, b) in [((1.0, t), (0.0, t)), ((t, 1.0), (t, 0.0))] {
                    let (a, b) = (paint(a.0, a.1), paint(b.0, b.1));
                    let near = (0..3).all(|c| (a[c] - b[c]).abs() < 0.03);
                    assert!(near, "{skin:?} at {t}: {a:?} {b:?} (step {step})");
                }
            }
        }
    }

    #[test]
    fn a_setting_names_a_skin_or_plain_colors() {
        for skin in Skin::ALL {
            assert_eq!(Skin::from_name(skin.name()), Some(skin));
        }
        assert_eq!(Skin::from_name(""), None);
    }
}
