// SPDX-License-Identifier: MIT
//! Generates the texture sets of the built-in woods and panels
//! (mitcad#2186): for each look a tileable base colour, a tangent-space
//! normal map (OpenGL convention, green up) and a roughness map, 1024 x
//! 1024 pixels, written as PPM and PGM files that `generate.sh` turns into
//! JPEG files. The images are MitCad's own (made here from noise, no
//! photographs), under MitCad's licence.
//!
//! The grain runs along the image's width (u): the `grain` projection lays
//! the width along each face's longest side. The base colour's mean in
//! linear light is the look's colour (its `base_color` in
//! core/model-base/src/appearance.rs), so a body looks the same colour with
//! and without its texture; the roughness map's mean is its roughness.
//!
//! Plain Rust without dependencies: `rustc -O generate.rs` builds it, and
//! `generate <folder>` writes the files. The output depends only on this
//! file (fixed seeds), so the images can be made again.

use std::f64::consts::PI;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

const SIZE: usize = 1024;

// ---------------------------------------------------------------- noise

/// A hash of a lattice point and a seed: 32 well-mixed bits.
fn hash(x: i64, y: i64, seed: u64) -> u64 {
    let mut h = (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
        ^ seed.wrapping_mul(0x1656_67b1_9e37_79f9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^= h >> 33;
    h
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Gradient noise in [-1, 1] (about), periodic: (x, y) in lattice units,
/// the lattice repeating every (px, py) cells.
fn gradient(x: f64, y: f64, px: i64, py: i64, seed: u64) -> f64 {
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let ix = x0 as i64;
    let iy = y0 as i64;
    let corner = |cx: i64, cy: i64, dx: f64, dy: f64| {
        let h = hash(cx.rem_euclid(px), cy.rem_euclid(py), seed);
        let angle = (h & 0xffff) as f64 / 65536.0 * 2.0 * PI;
        angle.cos() * dx + angle.sin() * dy
    };
    let n00 = corner(ix, iy, fx, fy);
    let n10 = corner(ix + 1, iy, fx - 1.0, fy);
    let n01 = corner(ix, iy + 1, fx, fy - 1.0);
    let n11 = corner(ix + 1, iy + 1, fx - 1.0, fy - 1.0);
    let u = fade(fx);
    let v = fade(fy);
    let a = n00 + (n10 - n00) * u;
    let b = n01 + (n11 - n01) * u;
    (a + (b - a) * v) * 1.414
}

/// Fractal noise over the unit square, repeating at its edges: `px` and
/// `py` cells across for the first octave, each next one twice as many
/// at `gain` times the amplitude.
fn fbm(u: f64, v: f64, px: i64, py: i64, octaves: u32, gain: f64, seed: u64) -> f64 {
    let mut sum = 0.0;
    let mut amplitude = 1.0;
    let mut total = 0.0;
    for o in 0..octaves {
        let fx = px << o;
        let fy = py << o;
        sum += amplitude * gradient(u * fx as f64, v * fy as f64, fx, fy, seed + u64::from(o) * 7919);
        total += amplitude;
        amplitude *= gain;
    }
    sum / total
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A small random number generator for placing chips and strands.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        (hash(self.0 as i64, 17, 3) >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

// ---------------------------------------------------------------- images

/// The maps of one look before they are written: per pixel a pattern
/// value (dark where high), a height (mm) and a roughness offset, and the
/// colour weights that make dark parts warmer.
struct Fields {
    /// 0 the look's lightest wood, about 1 its darkest.
    pattern: Vec<f64>,
    /// Height in mm, for the normal map.
    height: Vec<f64>,
    /// Added to the look's roughness (the map's mean is set afterwards).
    rough: Vec<f64>,
}

impl Fields {
    fn new() -> Self {
        Self {
            pattern: vec![0.0; SIZE * SIZE],
            height: vec![0.0; SIZE * SIZE],
            rough: vec![0.0; SIZE * SIZE],
        }
    }
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// One look: its id (the file names), colour, roughness, how strongly the
/// pattern darkens (`depth`), how much warmer its dark parts are
/// (`warmth`), the normal map's strength and the repeat in mm (for the
/// normal map's slopes).
struct Look {
    id: &'static str,
    rgb: u32,
    roughness: f64,
    depth: f64,
    warmth: f64,
    relief: f64,
    repeat_mm: f64,
    make: fn(&mut Fields, u64),
    seed: u64,
}

fn write_ppm(path: &Path, width: usize, pixels: &[u8], channels: usize) -> std::io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    let magic = if channels == 3 { "P6" } else { "P5" };
    write!(out, "{magic}\n{width} {width}\n255\n")?;
    out.write_all(pixels)?;
    out.flush()
}

fn build(look: &Look, folder: &Path) -> std::io::Result<()> {
    let mut fields = Fields::new();
    (look.make)(&mut fields, look.seed);
    let n = SIZE * SIZE;
    // Base colour: each channel of the look's linear colour darkened by the
    // pattern, blue most and red least (darker wood is warmer), then
    // scaled so that the mean is the look's colour.
    let base = [16u32, 8, 0].map(|s| srgb_to_linear(f64::from((look.rgb >> s) & 0xff) / 255.0));
    let weights = [1.0 - 0.5 * look.warmth, 1.0, 1.0 + look.warmth];
    let mut linear = vec![[0.0f64; 3]; n];
    let mut sums = [0.0f64; 3];
    for (i, p) in fields.pattern.iter().enumerate() {
        for c in 0..3 {
            let value = base[c] * (-look.depth * p * weights[c]).exp();
            linear[i][c] = value;
            sums[c] += value;
        }
    }
    let scale = [0, 1, 2].map(|c| base[c] * n as f64 / sums[c]);
    let mut color = vec![0u8; n * 3];
    for (i, px) in linear.iter().enumerate() {
        for c in 0..3 {
            color[i * 3 + c] = (linear_to_srgb(px[c] * scale[c]) * 255.0).round() as u8;
        }
    }
    write_ppm(&folder.join(format!("{}-color.ppm", look.id)), SIZE, &color, 3)?;
    // Roughness: the look's value plus the offsets, the mean kept.
    let mean = fields.rough.iter().sum::<f64>() / n as f64;
    let rough: Vec<u8> = fields
        .rough
        .iter()
        .map(|r| ((look.roughness + r - mean).clamp(0.02, 1.0) * 255.0).round() as u8)
        .collect();
    write_ppm(&folder.join(format!("{}-roughness.pgm", look.id)), SIZE, &rough, 1)?;
    // Normal map from the height's slopes (central differences across the
    // repeating edges): red along u (right), green along v (up the image,
    // so against the rows), blue out of the surface.
    let mm_per_pixel = look.repeat_mm / SIZE as f64;
    let h = |x: usize, y: usize| fields.height[(y % SIZE) * SIZE + (x % SIZE)];
    let mut normal = vec![0u8; n * 3];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let du = (h(x + 1, y) - h(x + SIZE - 1, y)) / (2.0 * mm_per_pixel) * look.relief;
            // v grows up the image: the row above minus the row below.
            let dv = (h(x, y + SIZE - 1) - h(x, y + 1)) / (2.0 * mm_per_pixel) * look.relief;
            let length = (du * du + dv * dv + 1.0).sqrt();
            let v = [-du / length, -dv / length, 1.0 / length];
            for c in 0..3 {
                normal[(y * SIZE + x) * 3 + c] = ((v[c] * 0.5 + 0.5) * 255.0).round() as u8;
            }
        }
    }
    write_ppm(&folder.join(format!("{}-normal.ppm", look.id)), SIZE, &normal, 3)
}

// ---------------------------------------------------------------- woods

/// The figure of a wood face with the grain along u: growth rings cut at a
/// shallow angle, `rings` across the image, bent by low-frequency noise
/// (`wave`: how far, in rings), each ring light earlywood turning into a
/// darker latewood band (`late`: its share of the ring, `contrast`), fine
/// fibres and pores along the grain (`fibres`), slow colour changes
/// (`streaks`) and, for some woods, small dark flecks (`flecks`).
struct Grain {
    rings: f64,
    wave: f64,
    wave_cells: (i64, i64),
    late: f64,
    contrast: f64,
    sharp: f64,
    fibres: f64,
    streaks: f64,
    flecks: f64,
    /// The pore grooves' depth (mm) and the latewood's step (mm).
    groove: f64,
    step: f64,
}

fn grain(fields: &mut Fields, seed: u64, g: &Grain) {
    let rings = g.rings as i64;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let u = x as f64 / SIZE as f64;
            let v = y as f64 / SIZE as f64;
            // The rings' bends: slow ones (kept below a fold, so no closed
            // islands), and a little fine jitter.
            // The large sweeps of a flat-sawn face (rings cut at a slowly
            // changing angle) take up to a twelfth of the rings.
            let bend = g.wave * fbm(u, v, g.wave_cells.0, g.wave_cells.1, 2, 0.4, seed)
                + g.rings / 12.0 * fbm(u, v, 1, 1, 2, 0.35, seed + 7)
                + 0.06 * fbm(u, v, 16, 8, 2, 0.5, seed + 13);
            // Ring widths vary from ring to ring.
            let r = v * g.rings + bend + 0.3 * fbm(u, v, 1, (rings / 8).max(1), 2, 0.5, seed + 23);
            let ring = r.floor();
            let t = r - ring;
            // Each ring its own strength of latewood.
            let which = hash((ring as i64).rem_euclid(rings), 5, seed);
            let strength = 0.6 + 0.4 * (which & 0xffff) as f64 / 65536.0;
            let width = g.late * (0.6 + 0.8 * ((which >> 16) & 0xffff) as f64 / 65536.0);
            // Earlywood darkens a little toward the latewood band, which
            // ends sharply at the next ring.
            let early = 0.25 * t * t;
            let late = smoothstep(1.0 - width - g.sharp, 1.0 - width, t) * (1.0 - smoothstep(0.97, 1.0, t));
            // Fine fibres along the grain: thin dark streaks.
            let fibre = smoothstep(0.05, 0.45, fbm(u, v, 3, 320, 3, 0.5, seed + 31));
            // Pores: short dark dashes along the grain.
            let pore = smoothstep(0.3, 0.6, fbm(u, v, 48, 448, 2, 0.5, seed + 37));
            let streak = fbm(u, v, 2, 10, 3, 0.5, seed + 41);
            let fleck = if g.flecks > 0.0 {
                smoothstep(0.5, 0.7, fbm(u, v, 24, 160, 2, 0.5, seed + 43))
            } else {
                0.0
            };
            let i = y * SIZE + x;
            fields.pattern[i] = g.contrast * (strength * late + 0.5 * early)
                + g.fibres * (fibre + 0.6 * pore)
                + g.streaks * streak
                + g.flecks * fleck;
            fields.height[i] = g.step * strength * late - g.groove * (pore + 0.5 * fibre);
            // Pores and fibres rougher, the dense latewood a little smoother.
            fields.rough[i] = 0.1 * pore + 0.05 * fibre - 0.05 * late;
        }
    }
}

fn birch(fields: &mut Fields, seed: u64) {
    grain(fields, seed, &Grain {
        rings: 40.0, wave: 1.0, wave_cells: (2, 2), late: 0.2, contrast: 0.3, sharp: 0.3,
        fibres: 0.3, streaks: 0.25, flecks: 0.15, groove: 0.02, step: 0.01,
    });
}

fn cedar(fields: &mut Fields, seed: u64) {
    grain(fields, seed, &Grain {
        rings: 48.0, wave: 1.2, wave_cells: (2, 2), late: 0.3, contrast: 0.7, sharp: 0.15,
        fibres: 0.3, streaks: 0.6, flecks: 0.0, groove: 0.03, step: 0.02,
    });
}

fn cherry(fields: &mut Fields, seed: u64) {
    grain(fields, seed, &Grain {
        rings: 44.0, wave: 1.0, wave_cells: (2, 2), late: 0.25, contrast: 0.35, sharp: 0.3,
        fibres: 0.25, streaks: 0.35, flecks: 0.12, groove: 0.015, step: 0.01,
    });
}

fn douglas_fir(fields: &mut Fields, seed: u64) {
    grain(fields, seed, &Grain {
        rings: 56.0, wave: 1.3, wave_cells: (2, 2), late: 0.3, contrast: 1.0, sharp: 0.06,
        fibres: 0.2, streaks: 0.3, flecks: 0.0, groove: 0.02, step: 0.04,
    });
}

fn eucalyptus(fields: &mut Fields, seed: u64) {
    // Interlocked grain: ribbons of light and dark along the grain.
    grain(fields, seed, &Grain {
        rings: 64.0, wave: 0.8, wave_cells: (3, 2), late: 0.4, contrast: 0.4, sharp: 0.35,
        fibres: 0.4, streaks: 0.55, flecks: 0.0, groove: 0.025, step: 0.01,
    });
}

fn pine(fields: &mut Fields, seed: u64) {
    grain(fields, seed, &Grain {
        rings: 44.0, wave: 1.5, wave_cells: (2, 2), late: 0.25, contrast: 0.9, sharp: 0.1,
        fibres: 0.18, streaks: 0.3, flecks: 0.0, groove: 0.015, step: 0.035,
    });
}

fn teak(fields: &mut Fields, seed: u64) {
    grain(fields, seed, &Grain {
        rings: 52.0, wave: 0.9, wave_cells: (2, 2), late: 0.3, contrast: 0.5, sharp: 0.25,
        fibres: 0.45, streaks: 0.7, flecks: 0.0, groove: 0.035, step: 0.015,
    });
}

/// A rotary-cut veneer (plywood faces): the log peeled round, so few,
/// broad, strongly bent figure lines.
fn veneer(fields: &mut Fields, seed: u64, contrast: f64, fibres: f64) {
    grain(fields, seed, &Grain {
        rings: 26.0, wave: 2.0, wave_cells: (1, 1), late: 0.3, contrast, sharp: 0.2,
        fibres, streaks: 0.35, flecks: 0.0, groove: 0.015, step: 0.02,
    });
}

fn plywood_birch(fields: &mut Fields, seed: u64) {
    veneer(fields, seed, 0.3, 0.22);
}

fn plywood_eucalyptus(fields: &mut Fields, seed: u64) {
    veneer(fields, seed, 0.45, 0.35);
}

fn plywood_pine(fields: &mut Fields, seed: u64) {
    veneer(fields, seed, 0.85, 0.18);
}

fn plywood_spruce(fields: &mut Fields, seed: u64) {
    veneer(fields, seed, 0.55, 0.15);
}

// ---------------------------------------------------------------- panels

/// Draws a rotated rectangle onto the repeating image (wrapping at the
/// edges): `shade(along, across)` gives each covered pixel's pattern and
/// height (mm) from its place in the piece, in 0..1 each way.
#[allow(clippy::too_many_arguments)]
fn piece(
    fields: &mut Fields,
    cx: f64,
    cy: f64,
    length: f64,
    width: f64,
    angle: f64,
    level: f64,
    shade: &dyn Fn(f64, f64) -> (f64, f64, f64),
) {
    let (s, c) = angle.sin_cos();
    let half_x = 0.5 * (length * c.abs() + width * s.abs());
    let half_y = 0.5 * (length * s.abs() + width * c.abs());
    let x0 = (cx - half_x).floor() as i64;
    let x1 = (cx + half_x).ceil() as i64;
    let y0 = (cy - half_y).floor() as i64;
    let y1 = (cy + half_y).ceil() as i64;
    for py in y0..=y1 {
        for px in x0..=x1 {
            let dx = px as f64 + 0.5 - cx;
            let dy = py as f64 + 0.5 - cy;
            let along = dx * c + dy * s;
            let across = -dx * s + dy * c;
            if along.abs() > length / 2.0 || across.abs() > width / 2.0 {
                continue;
            }
            let (pattern, height, rough) = shade(along / length + 0.5, across / width + 0.5);
            let i = (py.rem_euclid(SIZE as i64) as usize) * SIZE + px.rem_euclid(SIZE as i64) as usize;
            fields.pattern[i] = pattern;
            fields.height[i] = level + height;
            fields.rough[i] = rough;
        }
    }
}

/// Oriented strand board: long flat strands, mostly along u, overlapping
/// in layers, each its own tone with its own fibres, darker gaps between.
fn osb(fields: &mut Fields, seed: u64) {
    // The gaps: dark and low.
    for i in 0..SIZE * SIZE {
        fields.pattern[i] = 1.1;
        fields.height[i] = -0.4;
        fields.rough[i] = 0.1;
    }
    let mut rng = Rng(seed);
    // 600 mm repeat: about 1.7 pixels a millimetre.
    let px_mm = SIZE as f64 / 600.0;
    let count = 2600;
    for k in 0..count {
        let length = rng.range(60.0, 130.0) * px_mm;
        let width = rng.range(12.0, 30.0) * px_mm;
        let angle = rng.range(-0.45, 0.45);
        let cx = rng.range(0.0, SIZE as f64);
        let cy = rng.range(0.0, SIZE as f64);
        let tone = rng.range(-0.25, 0.45);
        let strand_seed = seed + 1000 + k as u64;
        let level = k as f64 / count as f64 * 0.3;
        let phase = rng.next();
        piece(fields, cx, cy, length, width, angle, level, &|a, b| {
            // Fibres along the strand, a darker rim.
            let fibre = gradient(a * 3.0 + phase * 7.0, b * 22.0, 1 << 20, 1 << 20, strand_seed);
            let rim = 1.0 - smoothstep(0.0, 0.08, b.min(1.0 - b)) * smoothstep(0.0, 0.04, a.min(1.0 - a));
            (tone + 0.2 * fibre + 0.35 * rim, 0.03 * fibre - 0.05 * rim, 0.06 * fibre.abs())
        });
    }
}

/// Chipboard: a dense speckle of small chips in many tones, some dark,
/// over fine dust.
fn chipboard(fields: &mut Fields, seed: u64) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let u = x as f64 / SIZE as f64;
            let v = y as f64 / SIZE as f64;
            let i = y * SIZE + x;
            let dust = fbm(u, v, 128, 128, 2, 0.5, seed);
            fields.pattern[i] = 0.3 + 0.3 * dust;
            fields.height[i] = -0.05 + 0.01 * dust;
            fields.rough[i] = 0.04;
        }
    }
    let mut rng = Rng(seed);
    // 400 mm repeat: about 2.6 pixels a millimetre.
    let px_mm = SIZE as f64 / 400.0;
    let count = 90_000;
    for k in 0..count {
        let length = rng.range(1.0, 6.0) * px_mm;
        let width = rng.range(0.6, 2.0) * px_mm;
        let angle = rng.range(0.0, PI);
        let cx = rng.range(0.0, SIZE as f64);
        let cy = rng.range(0.0, SIZE as f64);
        let dark = rng.next() < 0.06;
        let tone = if dark { rng.range(0.9, 1.4) } else { rng.range(-0.3, 0.5) };
        let level = k as f64 / count as f64 * 0.05;
        piece(fields, cx, cy, length, width, angle, level, &move |_, _| (tone, 0.0, 0.0));
    }
}

/// MDF: fine fibres pressed smooth: an almost plain face with faint
/// mottling.
fn mdf(fields: &mut Fields, seed: u64) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let u = x as f64 / SIZE as f64;
            let v = y as f64 / SIZE as f64;
            let i = y * SIZE + x;
            let mottle = fbm(u, v, 4, 4, 4, 0.55, seed);
            let fine = fbm(u, v, 256, 256, 2, 0.5, seed + 5);
            fields.pattern[i] = 0.18 * mottle + 0.08 * fine;
            fields.height[i] = 0.004 * fine;
            fields.rough[i] = 0.03 * fine;
        }
    }
}

// ---------------------------------------------------------------- looks

/// The looks, with the colours and roughness of their appearances
/// (core/model-base/src/appearance.rs) and the repeat their textures have
/// there.
const LOOKS: &[Look] = &[
    Look { id: "wood_birch", rgb: 0xe0c49a, roughness: 0.55, depth: 0.55, warmth: 0.35, relief: 8.0, repeat_mm: 800.0, make: birch, seed: 101 },
    Look { id: "wood_cedar", rgb: 0xa8643c, roughness: 0.55, depth: 0.6, warmth: 0.3, relief: 8.0, repeat_mm: 800.0, make: cedar, seed: 102 },
    Look { id: "wood_cherry", rgb: 0x9a4f2c, roughness: 0.5, depth: 0.5, warmth: 0.25, relief: 8.0, repeat_mm: 800.0, make: cherry, seed: 103 },
    Look { id: "wood_douglas_fir", rgb: 0xc89060, roughness: 0.55, depth: 0.6, warmth: 0.4, relief: 8.0, repeat_mm: 800.0, make: douglas_fir, seed: 104 },
    Look { id: "wood_eucalyptus", rgb: 0xbf9a6a, roughness: 0.55, depth: 0.55, warmth: 0.35, relief: 8.0, repeat_mm: 800.0, make: eucalyptus, seed: 105 },
    Look { id: "wood_pine", rgb: 0xe2c58e, roughness: 0.55, depth: 0.55, warmth: 0.45, relief: 8.0, repeat_mm: 800.0, make: pine, seed: 106 },
    Look { id: "wood_teak", rgb: 0xa8743c, roughness: 0.5, depth: 0.55, warmth: 0.3, relief: 8.0, repeat_mm: 800.0, make: teak, seed: 107 },
    Look { id: "panel_chipboard", rgb: 0xc8ab7c, roughness: 0.8, depth: 0.5, warmth: 0.3, relief: 3.0, repeat_mm: 400.0, make: chipboard, seed: 108 },
    Look { id: "panel_mdf", rgb: 0xb38a5e, roughness: 0.75, depth: 0.5, warmth: 0.2, relief: 4.0, repeat_mm: 600.0, make: mdf, seed: 109 },
    Look { id: "panel_osb", rgb: 0xc89a5a, roughness: 0.75, depth: 0.45, warmth: 0.35, relief: 5.0, repeat_mm: 600.0, make: osb, seed: 110 },
    Look { id: "plywood_birch", rgb: 0xe6cfa4, roughness: 0.6, depth: 0.5, warmth: 0.35, relief: 8.0, repeat_mm: 1000.0, make: plywood_birch, seed: 111 },
    Look { id: "plywood_eucalyptus", rgb: 0xc49a70, roughness: 0.6, depth: 0.5, warmth: 0.35, relief: 8.0, repeat_mm: 1000.0, make: plywood_eucalyptus, seed: 112 },
    Look { id: "plywood_pine", rgb: 0xdcc08c, roughness: 0.6, depth: 0.55, warmth: 0.45, relief: 8.0, repeat_mm: 1000.0, make: plywood_pine, seed: 113 },
    Look { id: "plywood_spruce", rgb: 0xe8d2a6, roughness: 0.6, depth: 0.5, warmth: 0.4, relief: 8.0, repeat_mm: 1000.0, make: plywood_spruce, seed: 114 },
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(folder) = args.get(1) else {
        eprintln!("usage: generate <folder> [look id ...]");
        std::process::exit(2);
    };
    let only: Vec<&str> = args[2..].iter().map(String::as_str).collect();
    let folder = Path::new(folder);
    for look in LOOKS {
        if !only.is_empty() && !only.contains(&look.id) {
            continue;
        }
        build(look, folder).unwrap_or_else(|e| panic!("{}: {e}", look.id));
        println!("{}", look.id);
    }
}
