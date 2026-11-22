//! Procedural cards for the botanical generators: the 2x2 foliage atlas
//! (leaf card + inflorescence / rosette cells), blade ribbons, flower spikes,
//! nuts, flower clusters, berries, and the stroke-painted `bipinnate` and
//! `branchlet` leaf cards.
//!
//! Card space is `[0, 1]` with v down. Every card is painted with its
//! attachment at the bottom centre and its tip at the top, matching the leaf
//! card convention (v = 1 at the petiole). See `docs/botanical-generators.md`.

use crate::{
    constants::{PI, TAU},
    math::lerp,
    rng::Rng,
    species::{
        BladeCard, GeneratorFamily, InflorescenceKind, InflorescenceParams, LeafCardLayout,
        LeafShape, RosetteParams, Species, SpikeStyle,
    },
    textures::{RgbaTexture, fbm, leaf_card, value_noise, voronoi_f1_f2},
};
use glam::Vec2;

/// Atlas cell holding the ordinary leaf card / live rosette blade.
pub const CELL_LEAF: u8 = 0;
/// Atlas cell holding the fresh inflorescence / rosette flower spike.
pub const CELL_ORGAN: u8 = 1;
/// Atlas cell holding the aged inflorescence / dead skirt blade.
pub const CELL_AGED: u8 = 2;
/// Atlas cell holding the rosette raceme flower.
pub const CELL_FLOWER: u8 = 3;

/// Inset of the usable area inside an atlas cell, in atlas UV units.
const CELL_INSET: f32 = 0.012;

/// Map a cell-local UV (`[0, 1]`, v down) into the 2x2 foliage atlas.
pub fn atlas_uv(cell: u8, uv: Vec2) -> Vec2 {
    let ox = if cell & 1 == 0 { 0.0 } else { 0.5 };
    let oy = if cell & 2 == 0 { 0.0 } else { 0.5 };
    let span = 0.5 - 2.0 * CELL_INSET;
    Vec2::new(
        ox + CELL_INSET + uv.x.clamp(0.0, 1.0) * span,
        oy + CELL_INSET + uv.y.clamp(0.0, 1.0) * span,
    )
}

type Rgb = [f32; 3];

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}

fn scale(c: Rgb, k: f32) -> Rgb {
    [
        (c[0] * k).clamp(0.0, 1.0),
        (c[1] * k).clamp(0.0, 1.0),
        (c[2] * k).clamp(0.0, 1.0),
    ]
}

/// A tapered, colour-graded capsule. `a == b` paints a disc.
#[derive(Debug, Clone, Copy)]
struct Stroke {
    a: Vec2,
    b: Vec2,
    wa: f32,
    wb: f32,
    ca: Rgb,
    cb: Rgb,
}

/// Float RGBA canvas the painters composite strokes onto.
struct Canvas {
    res: u32,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    fn new(res: u32) -> Self {
        Self {
            res,
            px: vec![[0.0; 4]; (res * res) as usize],
        }
    }

    /// Fill with an opaque colour from a per-texel function of (u, v).
    fn fill(&mut self, mut f: impl FnMut(f32, f32) -> Rgb) {
        let inv = 1.0 / self.res as f32;
        for y in 0..self.res {
            for x in 0..self.res {
                let c = f((x as f32 + 0.5) * inv, (y as f32 + 0.5) * inv);
                self.px[(y * self.res + x) as usize] = [c[0], c[1], c[2], 1.0];
            }
        }
    }

    /// Composite strokes in order (later strokes on top), with a soft edge
    /// and a slight darkening toward each stroke's rim so they read as round.
    fn paint(&mut self, strokes: &[Stroke]) {
        let res = self.res as f32;
        let texel = 1.0 / res;
        for s in strokes {
            let w_max = s.wa.max(s.wb) + texel * 1.5;
            let lo = s.a.min(s.b) - Vec2::splat(w_max);
            let hi = s.a.max(s.b) + Vec2::splat(w_max);
            let x0 = (lo.x * res).floor().max(0.0) as u32;
            let x1 = ((hi.x * res).ceil().max(0.0) as u32).min(self.res);
            let y0 = (lo.y * res).floor().max(0.0) as u32;
            let y1 = ((hi.y * res).ceil().max(0.0) as u32).min(self.res);
            let ab = s.b - s.a;
            let len2 = ab.length_squared().max(1e-9);
            for y in y0..y1 {
                for x in x0..x1 {
                    let q = Vec2::new((x as f32 + 0.5) * texel, (y as f32 + 0.5) * texel);
                    let t = ((q - s.a).dot(ab) / len2).clamp(0.0, 1.0);
                    let w = lerp(s.wa, s.wb, t);
                    let d = (q - (s.a + ab * t)).length();
                    let alpha = ((w - d) / texel + 0.5).clamp(0.0, 1.0);
                    if alpha <= 0.0 {
                        continue;
                    }
                    let round = 0.80 + 0.20 * (1.0 - (d / w.max(1e-5)).clamp(0.0, 1.0));
                    let c = scale(mix(s.ca, s.cb, t), round);
                    let dst = &mut self.px[(y * self.res + x) as usize];
                    let da = dst[3];
                    let out_a = alpha + da * (1.0 - alpha);
                    for ch in 0..3 {
                        dst[ch] = (c[ch] * alpha + dst[ch] * da * (1.0 - alpha)) / out_a;
                    }
                    dst[3] = out_a;
                }
            }
        }
    }

    fn finish(self) -> RgbaTexture {
        let mut tex = RgbaTexture::new(self.res, self.res);
        for (i, p) in self.px.iter().enumerate() {
            let o = i * 4;
            tex.pixels[o] = (p[0].clamp(0.0, 1.0) * 255.0) as u8;
            tex.pixels[o + 1] = (p[1].clamp(0.0, 1.0) * 255.0) as u8;
            tex.pixels[o + 2] = (p[2].clamp(0.0, 1.0) * 255.0) as u8;
            tex.pixels[o + 3] = (p[3].clamp(0.0, 1.0) * 255.0) as u8;
        }
        tex.dilate_alpha_rgb(8);
        tex
    }
}

fn disc(c: Vec2, r: f32, color: Rgb) -> Stroke {
    Stroke {
        a: c,
        b: c,
        wa: r,
        wb: r,
        ca: color,
        cb: color,
    }
}

// ---------------------------------------------------------------------------
// Rosette cells
// ---------------------------------------------------------------------------

/// Live blade ribbon: base at the bottom of the cell, tip at the top.
fn blade_strokes(card: BladeCard, base: Rgb, tip: Rgb, rng: &mut Rng) -> Vec<Stroke> {
    let mut out = Vec::new();
    match card {
        BladeCard::Single => {
            // One strap leaf: parallel sides, tapering over the top third.
            let x = 0.5;
            let steps = 10;
            for i in 0..steps {
                let t0 = i as f32 / steps as f32;
                let t1 = (i + 1) as f32 / steps as f32;
                let w = |t: f32| 0.30 * (1.0 - (t.max(0.55) - 0.55) / 0.45).max(0.03).powf(0.8);
                out.push(Stroke {
                    a: Vec2::new(x, lerp(0.985, 0.02, t0)),
                    b: Vec2::new(x, lerp(0.985, 0.02, t1)),
                    wa: w(t0),
                    wb: w(t1),
                    ca: mix(base, tip, t0),
                    cb: mix(base, tip, t1),
                });
            }
            // Pale midrib and two side veins.
            for (dx, k) in [(0.0f32, 1.22f32), (-0.12, 1.10), (0.12, 1.10)] {
                out.push(Stroke {
                    a: Vec2::new(x + dx, 0.985),
                    b: Vec2::new(x + dx * 0.2, 0.10),
                    wa: 0.012,
                    wb: 0.003,
                    ca: scale(base, k),
                    cb: scale(tip, k),
                });
            }
        }
        BladeCard::Tuft => {
            // Fine needle leaves side by side, lengths staggered.
            let n = 7;
            for i in 0..n {
                let f = (i as f32 + 0.5) / n as f32;
                let x0 = lerp(0.20, 0.80, f) + rng.range(-0.03, 0.03);
                let x1 = lerp(0.06, 0.94, f) + rng.range(-0.05, 0.05);
                let top = rng.range(0.02, 0.26);
                let shade = rng.range(0.82, 1.14);
                let mid = Vec2::new(
                    lerp(x0, x1, 0.5) + rng.range(-0.02, 0.02),
                    lerp(0.985, top, 0.5),
                );
                let a = Vec2::new(x0, 0.985);
                let b = Vec2::new(x1, top);
                out.push(Stroke {
                    a,
                    b: mid,
                    wa: 0.034,
                    wb: 0.024,
                    ca: scale(base, shade),
                    cb: scale(mix(base, tip, 0.5), shade),
                });
                out.push(Stroke {
                    a: mid,
                    b,
                    wa: 0.024,
                    wb: 0.006,
                    ca: scale(mix(base, tip, 0.5), shade),
                    cb: scale(tip, shade),
                });
            }
        }
        BladeCard::Pinnate => {
            // Rachis with paired, forward-swept leaflets, longest mid-frond.
            let p0 = Vec2::new(0.5, 0.985);
            let p1 = Vec2::new(0.5, 0.03);
            out.push(Stroke {
                a: p0,
                b: p1,
                wa: 0.016,
                wb: 0.004,
                ca: scale(base, 1.25),
                cb: scale(tip, 1.15),
            });
            let pairs = 30;
            for i in 0..pairs {
                let t = 0.10 + 0.88 * (i as f32 / (pairs - 1) as f32);
                let origin = p0.lerp(p1, t);
                let env = ((t - 0.04) / 0.22).clamp(0.2, 1.0).sqrt()
                    * ((1.02 - t) / 0.45).clamp(0.12, 1.0).powf(0.7);
                let len = 0.46 * env * rng.range(0.92, 1.06);
                let shade = rng.range(0.84, 1.12);
                let c0 = scale(mix(base, tip, t * 0.6), shade);
                let c1 = scale(mix(base, tip, 0.35 + t * 0.6), shade);
                for side in [-1.0f32, 1.0] {
                    let ang = -PI * 0.5 + side * lerp(1.12, 0.42, t) + rng.range(-0.04, 0.04);
                    let tip_pt = (origin + Vec2::new(ang.cos(), ang.sin()) * len)
                        .clamp(Vec2::splat(0.01), Vec2::splat(0.99));
                    out.push(Stroke {
                        a: origin,
                        b: tip_pt,
                        wa: 0.0125,
                        wb: 0.003,
                        ca: c0,
                        cb: c1,
                    });
                }
            }
        }
    }
    out
}

/// Dead skirt blade: the same layouts, bleached and ragged.
fn dead_blade_strokes(card: BladeCard, color: Rgb, rng: &mut Rng) -> Vec<Stroke> {
    let dark = scale(color, 0.62);
    let pale = [
        (color[0] * 1.25 + 0.05).min(1.0),
        (color[1] * 1.22 + 0.05).min(1.0),
        (color[2] * 1.15 + 0.04).min(1.0),
    ];
    let mut strokes = blade_strokes(card, dark, pale, rng);
    // Broken tips and a few stray fibres.
    for i in 0..6 {
        let x = 0.2 + 0.6 * ((i as f32 * 0.618_034).fract());
        let y0 = rng.range(0.35, 0.9);
        strokes.push(Stroke {
            a: Vec2::new(x, y0),
            b: Vec2::new(x + rng.range(-0.12, 0.12), y0 - rng.range(0.12, 0.30)),
            wa: 0.012,
            wb: 0.003,
            ca: scale(color, rng.range(0.7, 1.2)),
            cb: scale(pale, rng.range(0.8, 1.1)),
        });
    }
    strokes
}

/// Rosette spike cell: scape colour in the bottom quarter, the flowering
/// part above (`cylinder`), cone scales (`cone`), or a plain scape (`raceme`).
fn spike_cell(rosette: &RosetteParams, seed: u64, res: u32) -> RgbaTexture {
    let spike = rosette.spike.clone();
    let mut canvas = Canvas::new(res);
    let Some(spike) = spike else {
        return canvas.finish();
    };
    let (color, tip, scape) = (spike.color, spike.tip_color, spike.scape_color);
    canvas.fill(|u, v| {
        let grain = fbm(seed ^ 0xB1, u * 48.0, v * 12.0, 48, 12, 3);
        let scape_rgb = scale(scape, 0.82 + 0.36 * grain);
        match spike.style {
            SpikeStyle::Raceme => {
                // Woolly scape: fine pale flecks.
                let fleck = value_noise(seed ^ 0xB2, u * 128.0, v * 128.0, 128, 128);
                scale(scape, 0.78 + 0.5 * fleck)
            }
            SpikeStyle::Cylinder => {
                if v > 0.75 {
                    return scape_rgb;
                }
                // Florets in steep spirals; cream flowers toward the tip.
                let h = 1.0 - v / 0.75;
                let lattice =
                    (TAU * (u * 14.0 + v * 30.0)).cos() * (TAU * (u * 14.0 - v * 30.0)).cos();
                let bloom = value_noise(seed ^ 0xB3, u * 24.0, v * 48.0, 24, 48);
                let flowering = ((h - 0.25 + (bloom - 0.5) * 0.5) / 0.5).clamp(0.0, 1.0);
                scale(
                    mix(color, tip, flowering * (0.45 + 0.55 * bloom)),
                    0.80 + 0.26 * lattice,
                )
            }
            SpikeStyle::Cone => {
                // Overlapping cone scales with a pale spine tip each.
                let (f1, f2, cell) = voronoi_f1_f2(seed ^ 0xB4, u * 10.0, v * 12.0, 10, 12);
                let border = ((f2 - f1) / 0.22).clamp(0.0, 1.0);
                let boss = (1.0 - f1 * 1.4).clamp(0.0, 1.0);
                scale(
                    mix(color, tip, boss * boss * 0.9),
                    (0.55 + 0.45 * border) * (0.85 + 0.2 * cell),
                )
            }
        }
    });
    canvas.finish()
}

/// Raceme flower: a woolly tube from the base, splitting into short lobes.
fn raceme_flower_cell(rosette: &RosetteParams, rng: &mut Rng, res: u32) -> RgbaTexture {
    let mut canvas = Canvas::new(res);
    let Some(spike) = rosette
        .spike
        .as_ref()
        .filter(|s| s.style == SpikeStyle::Raceme)
    else {
        return canvas.finish();
    };
    let base = Vec2::new(0.5, 0.985);
    let neck = Vec2::new(0.5, 0.36);
    let mut strokes = vec![
        Stroke {
            a: base,
            b: Vec2::new(0.5, 0.70),
            wa: 0.12,
            wb: 0.20,
            ca: spike.color,
            cb: mix(spike.color, spike.tip_color, 0.25),
        },
        Stroke {
            a: Vec2::new(0.5, 0.70),
            b: neck,
            wa: 0.20,
            wb: 0.17,
            ca: mix(spike.color, spike.tip_color, 0.25),
            cb: spike.tip_color,
        },
    ];
    for i in 0..6 {
        let f = i as f32 / 5.0 - 0.5;
        let ang = -PI * 0.5 + f * 1.9 + rng.range(-0.06, 0.06);
        strokes.push(Stroke {
            a: neck,
            b: neck + Vec2::new(ang.cos(), ang.sin()) * rng.range(0.24, 0.31),
            wa: 0.070,
            wb: 0.018,
            ca: spike.tip_color,
            cb: scale(spike.tip_color, 1.18),
        });
    }
    canvas.paint(&strokes);
    canvas.finish()
}

// ---------------------------------------------------------------------------
// Inflorescence cells
// ---------------------------------------------------------------------------

/// Opaque wrap for a banksia-style spike: spiralled floret pairs; `color`
/// where the flowers have opened (they open from the base), `color_tip`
/// where they are still in bud.
fn flower_spike_cell(inf: &InflorescenceParams, seed: u64, res: u32) -> RgbaTexture {
    let tip = inf.color_tip.unwrap_or(inf.color);
    let mut canvas = Canvas::new(res);
    canvas.fill(|u, v| {
        let h = 1.0 - v;
        let lattice = (TAU * (u * 13.0 + v * 9.0)).cos() * (TAU * (u * 13.0 - v * 9.0)).cos();
        let rows = (TAU * v * 26.0).cos() * 0.5 + 0.5;
        let ragged = fbm(seed ^ 0xC1, u * 12.0, v * 6.0, 12, 6, 3);
        let front = ((h - 0.55 + (ragged - 0.5) * 0.35) / 0.18).clamp(0.0, 1.0);
        let fuzz = value_noise(seed ^ 0xC2, u * 128.0, v * 128.0, 128, 128);
        let open = scale(inf.color, 0.78 + 0.22 * lattice + 0.14 * (fuzz - 0.5));
        let bud = scale(tip, 0.74 + 0.20 * lattice * rows);
        mix(open, bud, front)
    });
    canvas.finish()
}

/// Opaque wrap for an old cone: grey felt with dark, pale-lipped follicles.
fn aged_spike_cell(inf: &InflorescenceParams, seed: u64, res: u32) -> RgbaTexture {
    let mut canvas = Canvas::new(res);
    canvas.fill(|u, v| {
        let felt = fbm(seed ^ 0xC3, u * 40.0, v * 40.0, 40, 40, 3);
        let (f1, _, cell) = voronoi_f1_f2(seed ^ 0xC4, u * 7.0, v * 9.0, 7, 9);
        let mut c = scale(inf.aged_color, 0.72 + 0.50 * felt);
        if cell > 0.42 && f1 < 0.36 {
            let r = f1 / 0.36;
            let lip = (-(r - 0.72).powi(2) / 0.02).exp();
            let mouth = (1.0 - r / 0.55).clamp(0.0, 1.0);
            c = mix(c, scale(inf.aged_color, 1.55), lip * 0.8);
            c = mix(c, scale(inf.aged_color, 0.30), mouth);
        }
        c
    });
    canvas.finish()
}

/// Opaque wrap for a woody nut: fresh ones green-brown, aged ones the aged colour.
fn nut_cell(color: Rgb, rim: Rgb, seed: u64, res: u32) -> RgbaTexture {
    let mut canvas = Canvas::new(res);
    canvas.fill(|u, v| {
        let h = 1.0 - v;
        let grain = fbm(seed ^ 0xC5, u * 24.0, v * 6.0, 24, 6, 3);
        let speck = value_noise(seed ^ 0xC6, u * 96.0, v * 96.0, 96, 96);
        // Dark mouth at the open rim (top of the cell).
        let mouth = ((h - 0.90) / 0.10).clamp(0.0, 1.0);
        let body = scale(
            mix(color, rim, (h * 0.8).clamp(0.0, 1.0)),
            0.80 + 0.30 * grain + 0.10 * (speck - 0.5),
        );
        mix(body, scale(color, 0.25), mouth)
    });
    canvas.finish()
}

/// Flower-cluster card: a rounded panicle of small flowers on fine stalks.
/// `density` in `0..=1` thins the panicle (aged clusters are sparse and
/// carry pods in `pod` colour).
fn cluster_cell(petal: Rgb, throat: Rgb, pod: Option<Rgb>, rng: &mut Rng, res: u32) -> RgbaTexture {
    let mut canvas = Canvas::new(res);
    let root = Vec2::new(0.5, 0.985);
    let hub = Vec2::new(0.5 + rng.range(-0.03, 0.03), 0.52);
    let stalk = [0.34, 0.32, 0.20];
    let mut strokes = vec![Stroke {
        a: root,
        b: hub,
        wa: 0.012,
        wb: 0.006,
        ca: stalk,
        cb: stalk,
    }];
    let flowers = if pod.is_some() { 26 } else { 78 };
    let mut heads = Vec::with_capacity(flowers);
    for i in 0..flowers {
        // Jittered sunflower packing gives an even, rounded head.
        let r = 0.43 * ((i as f32 + 0.5) / flowers as f32).sqrt() * rng.range(0.9, 1.05);
        let a = i as f32 * 2.399_963 + rng.range(-0.2, 0.2);
        let c = (hub + Vec2::new(a.cos() * r, a.sin() * r * 0.92 - 0.05))
            .clamp(Vec2::splat(0.05), Vec2::splat(0.95));
        heads.push(c);
        if i % 3 == 0 {
            strokes.push(Stroke {
                a: hub,
                b: c,
                wa: 0.004,
                wb: 0.0025,
                ca: stalk,
                cb: stalk,
            });
        }
    }
    for (i, &c) in heads.iter().enumerate() {
        let shade = rng.range(0.80, 1.16);
        if let Some(pod) = pod
            && i % 3 == 0
        {
            // Flat woody pod.
            strokes.push(disc(c, rng.range(0.045, 0.062), scale(pod, shade)));
            continue;
        }
        // Trumpet seen from the side/front: a short tube and a flared mouth.
        let out = (c - hub).try_normalize().unwrap_or(Vec2::NEG_Y);
        let size = rng.range(0.034, 0.050);
        strokes.push(Stroke {
            a: c - out * size * 1.3,
            b: c,
            wa: size * 0.45,
            wb: size * 0.95,
            ca: scale(mix(petal, throat, 0.5), shade * 0.9),
            cb: scale(petal, shade),
        });
        for k in 0..5 {
            let pa = k as f32 / 5.0 * TAU + rng.range(-0.3, 0.3);
            strokes.push(disc(
                c + Vec2::new(pa.cos(), pa.sin()) * size * 0.62,
                size * 0.56,
                scale(petal, shade * rng.range(0.94, 1.08)),
            ));
        }
        strokes.push(disc(c, size * 0.34, scale(throat, shade)));
    }
    canvas.paint(&strokes);
    canvas.finish()
}

/// Berry card: a branching stalk carrying round fruit.
fn berries_cell(color: Rgb, highlight: Rgb, rng: &mut Rng, res: u32) -> RgbaTexture {
    let mut canvas = Canvas::new(res);
    let root = Vec2::new(0.5, 0.985);
    let stalk = [0.36, 0.30, 0.20];
    let mut strokes = Vec::new();
    let mut fruit = Vec::new();
    let sprays = 5;
    for s in 0..sprays {
        let f = s as f32 / (sprays - 1) as f32 - 0.5;
        let ang = -PI * 0.5 + f * 1.5 + rng.range(-0.1, 0.1);
        let end = root + Vec2::new(ang.cos(), ang.sin()) * rng.range(0.48, 0.74);
        strokes.push(Stroke {
            a: root,
            b: end,
            wa: 0.008,
            wb: 0.004,
            ca: stalk,
            cb: stalk,
        });
        for k in 0..7 {
            let t = 0.35 + 0.65 * (k as f32 / 6.0);
            let node = root.lerp(end, t);
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let off = Vec2::new(-ang.sin(), ang.cos()) * side * rng.range(0.03, 0.09);
            let c = (node + off).clamp(Vec2::splat(0.06), Vec2::splat(0.94));
            strokes.push(Stroke {
                a: node,
                b: c,
                wa: 0.004,
                wb: 0.003,
                ca: stalk,
                cb: stalk,
            });
            fruit.push(c);
        }
    }
    for c in fruit {
        let r = rng.range(0.034, 0.046);
        let shade = rng.range(0.82, 1.10);
        strokes.push(disc(c, r, scale(color, shade)));
        strokes.push(disc(
            c + Vec2::new(-r * 0.3, -r * 0.3),
            r * 0.34,
            scale(highlight, shade),
        ));
    }
    canvas.paint(&strokes);
    canvas.finish()
}

// ---------------------------------------------------------------------------
// Stroke-painted leaf cards
// ---------------------------------------------------------------------------

/// One bipinnate leaf from `base` to `tip`: rachis, paired pinnae, each
/// pinna lined with tiny leaflets.
fn bipinnate_leaf(base: Vec2, tip: Vec2, color: Rgb, rng: &mut Rng, out: &mut Vec<Stroke>) {
    let axis = tip - base;
    let len = axis.length().max(1e-4);
    let dir = axis / len;
    let nrm = Vec2::new(-dir.y, dir.x);
    let rachis = [0.42, 0.44, 0.22];
    out.push(Stroke {
        a: base,
        b: tip,
        wa: len * 0.012,
        wb: len * 0.004,
        ca: rachis,
        cb: rachis,
    });
    let pairs = 9;
    for i in 0..pairs {
        let t = 0.14 + 0.82 * (i as f32 / (pairs - 1) as f32);
        let origin = base + axis * t;
        let env = ((t - 0.02) / 0.3).clamp(0.3, 1.0).sqrt() * ((1.04 - t) / 0.5).clamp(0.2, 1.0);
        let pinna_len = len * 0.36 * env;
        for side in [-1.0f32, 1.0] {
            let sweep = lerp(1.15, 0.55, t) + rng.range(-0.06, 0.06);
            let pdir = (dir * sweep.cos() + nrm * side * sweep.sin()).normalize();
            let pnrm = Vec2::new(-pdir.y, pdir.x);
            let pend = origin + pdir * pinna_len;
            out.push(Stroke {
                a: origin,
                b: pend,
                wa: len * 0.005,
                wb: len * 0.002,
                ca: rachis,
                cb: rachis,
            });
            let leaflets = 9;
            let shade = rng.range(0.80, 1.16);
            let c0 = scale(color, shade * 0.92);
            let c1 = scale(color, shade * 1.10);
            for k in 0..leaflets {
                let lt = (k as f32 + 0.6) / leaflets as f32;
                let node = origin + pdir * (pinna_len * lt);
                let ll = len * 0.040 * (1.0 - 0.45 * lt);
                for ls in [-1.0f32, 1.0] {
                    let ldir = (pdir * 0.45 + pnrm * ls).normalize();
                    out.push(Stroke {
                        a: node,
                        b: node + ldir * ll,
                        wa: len * 0.0085,
                        wb: len * 0.0035,
                        ca: c0,
                        cb: c1,
                    });
                }
            }
        }
    }
}

/// One sheoak shoot from `base` toward `tip`: a short twig carrying long
/// jointed branchlets that spread and then fall forward.
fn branchlet_tuft(base: Vec2, tip: Vec2, color: Rgb, rng: &mut Rng, out: &mut Vec<Stroke>) {
    let axis = tip - base;
    let len = axis.length().max(1e-4);
    let dir = axis / len;
    let nrm = Vec2::new(-dir.y, dir.x);
    let twig = [0.33, 0.27, 0.20];
    let twig_end = base + axis * 0.42;
    out.push(Stroke {
        a: base,
        b: twig_end,
        wa: len * 0.014,
        wb: len * 0.006,
        ca: twig,
        cb: twig,
    });
    let wires = 22;
    for i in 0..wires {
        let t = rng.range(0.05, 1.0);
        let node = base.lerp(twig_end, t);
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let spread = rng.range(0.10, 0.62) * side;
        let d0 = (dir * spread.cos() + nrm * spread.sin()).normalize();
        // The far half relaxes back toward the shoot axis (the wires hang).
        let d1 = (d0 * 0.55 + dir * 0.45).normalize();
        let wire_len = len * rng.range(0.50, 0.86);
        let mid = node + d0 * wire_len * 0.5;
        let end = (mid + d1 * wire_len * 0.5).clamp(Vec2::splat(0.01), Vec2::splat(0.99));
        let shade = rng.range(0.74, 1.18);
        let c0 = scale(color, shade * 0.9);
        let c1 = scale(color, shade * 1.08);
        out.push(Stroke {
            a: node,
            b: mid,
            wa: len * 0.0052,
            wb: len * 0.0042,
            ca: c0,
            cb: mix(c0, c1, 0.5),
        });
        out.push(Stroke {
            a: mid,
            b: end,
            wa: len * 0.0042,
            wb: len * 0.0022,
            ca: mix(c0, c1, 0.5),
            cb: c1,
        });
    }
}

/// Leaf card for the stroke-painted shapes (`bipinnate`, `branchlet`).
/// `single` draws one leaf filling the card; every other layout draws a
/// spray of several on a forked twig.
pub(crate) fn stroke_leaf_card(
    shape: LeafShape,
    layout: LeafCardLayout,
    color: Option<[f32; 3]>,
    rng: &mut Rng,
    res: u32,
) -> RgbaTexture {
    let base = color.unwrap_or([0.27, 0.44, 0.17]);
    let mut strokes = Vec::new();
    let leaf = |a: Vec2, b: Vec2, rng: &mut Rng, out: &mut Vec<Stroke>| match shape {
        LeafShape::Branchlet => branchlet_tuft(a, b, base, rng, out),
        _ => bipinnate_leaf(a, b, base, rng, out),
    };
    let root = Vec2::new(0.5, 0.985);
    if layout == LeafCardLayout::Single {
        leaf(root, Vec2::new(0.5, 0.03), rng, &mut strokes);
    } else {
        let twig = [0.31, 0.24, 0.16];
        let top = Vec2::new(0.5 + rng.range(-0.03, 0.03), 0.42);
        strokes.push(Stroke {
            a: root,
            b: top,
            wa: 0.010,
            wb: 0.005,
            ca: twig,
            cb: twig,
        });
        // Five leaves fanned from nodes up the twig; the outer pair lowest.
        for (t, ang, reach) in [
            (0.30f32, -1.05f32, 0.50f32),
            (0.38, 1.00, 0.52),
            (0.62, -0.52, 0.56),
            (0.72, 0.50, 0.56),
            (1.00, 0.0, 0.40),
        ] {
            let node = root.lerp(top, t);
            let a = -PI * 0.5 + ang + rng.range(-0.08, 0.08);
            let end = (node + Vec2::new(a.cos(), a.sin()) * reach * rng.range(0.92, 1.05))
                .clamp(Vec2::splat(0.03), Vec2::splat(0.97));
            leaf(node, end, rng, &mut strokes);
        }
    }
    let mut canvas = Canvas::new(res);
    canvas.paint(&strokes);
    canvas.finish()
}

// ---------------------------------------------------------------------------
// Atlas assembly
// ---------------------------------------------------------------------------

fn blit(atlas: &mut RgbaTexture, cell: u8, src: &RgbaTexture) {
    let half = atlas.width / 2;
    let ox = if cell & 1 == 0 { 0 } else { half };
    let oy = if cell & 2 == 0 { 0 } else { half };
    for y in 0..half.min(src.height) {
        for x in 0..half.min(src.width) {
            atlas.set(ox + x, oy + y, src.get(x, y));
        }
    }
}

/// Build the 2x2 foliage atlas for a species that uses one (see
/// [`Species::uses_foliage_atlas`]). Each cell is painted at half of `res`.
pub(crate) fn foliage_atlas(species: &Species, seed: u64, res: u32) -> RgbaTexture {
    let res = (res / 2).max(8) * 2;
    let half = res / 2;
    let mut atlas = RgbaTexture::new(res, res);
    let params = &species.textures;
    let mut rng = Rng::from_seed(seed ^ 0xA71A_5CE1);

    if species.generator.family == GeneratorFamily::Rosette {
        let rosette = species.rosette.clone().unwrap_or_default();
        let leaf = params.leaf_color.unwrap_or([0.27, 0.44, 0.17]);
        let base = rosette.color_base.unwrap_or(scale(leaf, 0.78));
        let tip = rosette.color_tip.unwrap_or(scale(leaf, 1.12));
        let mut live = Canvas::new(half);
        live.paint(&blade_strokes(rosette.blade_card, base, tip, &mut rng));
        blit(&mut atlas, CELL_LEAF, &live.finish());
        let mut dead = Canvas::new(half);
        dead.paint(&dead_blade_strokes(
            rosette.blade_card,
            rosette.skirt_color,
            &mut rng,
        ));
        blit(&mut atlas, CELL_AGED, &dead.finish());
        blit(&mut atlas, CELL_ORGAN, &spike_cell(&rosette, seed, half));
        blit(
            &mut atlas,
            CELL_FLOWER,
            &raceme_flower_cell(&rosette, &mut rng, half),
        );
        return atlas;
    }

    let card = leaf_card(
        params.leaf_shape,
        params.leaf_card,
        params.leaf_color,
        seed,
        half,
    );
    blit(&mut atlas, CELL_LEAF, &card);
    if let Some(inf) = &species.inflorescence {
        let tip = inf.color_tip.unwrap_or(inf.color);
        let (fresh, aged) = match inf.kind {
            InflorescenceKind::Spike => (
                flower_spike_cell(inf, seed, half),
                aged_spike_cell(inf, seed, half),
            ),
            InflorescenceKind::Nut => (
                nut_cell(inf.color, tip, seed, half),
                nut_cell(
                    inf.aged_color,
                    scale(inf.aged_color, 1.25),
                    seed ^ 0x55,
                    half,
                ),
            ),
            InflorescenceKind::Cluster => (
                cluster_cell(inf.color, tip, None, &mut rng, half),
                cluster_cell(
                    mix(inf.color, inf.aged_color, 0.6),
                    inf.aged_color,
                    Some(inf.aged_color),
                    &mut rng,
                    half,
                ),
            ),
            InflorescenceKind::Berries => (
                berries_cell(inf.color, tip, &mut rng, half),
                berries_cell(inf.aged_color, scale(inf.aged_color, 1.2), &mut rng, half),
            ),
        };
        blit(&mut atlas, CELL_ORGAN, &fresh);
        blit(&mut atlas, CELL_AGED, &aged);
    }
    atlas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(tex: &RgbaTexture, cell: u8) -> f32 {
        let half = tex.width / 2;
        let ox = if cell & 1 == 0 { 0 } else { half };
        let oy = if cell & 2 == 0 { 0 } else { half };
        let mut n = 0u32;
        for y in 0..half {
            for x in 0..half {
                if tex.get(ox + x, oy + y)[3] > 127 {
                    n += 1;
                }
            }
        }
        n as f32 / (half * half) as f32
    }

    #[test]
    fn atlas_uv_stays_inside_its_cell() {
        for cell in 0..4u8 {
            for (u, v) in [(0.0, 0.0), (1.0, 1.0), (0.5, 0.25)] {
                let p = atlas_uv(cell, Vec2::new(u, v));
                let ox = if cell & 1 == 0 { 0.0 } else { 0.5 };
                let oy = if cell & 2 == 0 { 0.0 } else { 0.5 };
                assert!(p.x > ox && p.x < ox + 0.5, "cell {cell} u {}", p.x);
                assert!(p.y > oy && p.y < oy + 0.5, "cell {cell} v {}", p.y);
            }
        }
    }

    #[test]
    fn stroke_leaf_cards_have_alpha_and_are_deterministic() {
        for shape in [LeafShape::Bipinnate, LeafShape::Branchlet] {
            for layout in [LeafCardLayout::Single, LeafCardLayout::Cluster] {
                let a = stroke_leaf_card(shape, layout, None, &mut Rng::from_seed(5), 128);
                let b = stroke_leaf_card(shape, layout, None, &mut Rng::from_seed(5), 128);
                assert_eq!(a.pixels, b.pixels);
                let cov = a.alpha_coverage();
                assert!(
                    cov > 0.03 && cov < 0.9,
                    "{shape:?}/{layout:?} coverage {cov}"
                );
            }
        }
    }

    #[test]
    fn rosette_atlas_fills_blade_skirt_and_spike_cells() {
        let species = Species::from_toml(
            r#"
[species]
name = "Atlas Rosette"
[generator]
family = "rosette"
[trunk]
height = 1.0
[rosette]
skirt = 0.5
skirt_blades = 40
[rosette.spike]
style = "cylinder"
"#,
        )
        .unwrap();
        let atlas = foliage_atlas(&species, 1, 256);
        assert_eq!((atlas.width, atlas.height), (256, 256));
        assert!(coverage(&atlas, CELL_LEAF) > 0.05);
        assert!(coverage(&atlas, CELL_AGED) > 0.05);
        assert!(coverage(&atlas, CELL_ORGAN) > 0.99, "spike wrap is opaque");
        assert!(
            coverage(&atlas, CELL_FLOWER) < 0.01,
            "no raceme flower painted"
        );
    }

    #[test]
    fn inflorescence_atlas_keeps_the_leaf_card_and_adds_organs() {
        for kind in ["spike", "nut", "cluster", "berries"] {
            let species = Species::from_toml(&format!(
                r#"
[species]
name = "Atlas Tree"
[trunk]
height = 4.0
[inflorescence]
kind = "{kind}"
"#
            ))
            .unwrap();
            let atlas = foliage_atlas(&species, 9, 256);
            assert!(
                coverage(&atlas, CELL_LEAF) > 0.03,
                "{kind}: leaf card missing"
            );
            assert!(
                coverage(&atlas, CELL_ORGAN) > 0.03,
                "{kind}: organ cell empty"
            );
            assert!(
                coverage(&atlas, CELL_AGED) > 0.03,
                "{kind}: aged cell empty"
            );
            assert!(
                coverage(&atlas, CELL_FLOWER) < 0.01,
                "{kind}: cell 3 must stay empty"
            );
        }
    }
}
