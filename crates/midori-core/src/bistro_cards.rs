//! Bespoke procedural leaf and floral card synthesizers for urban, topiary,
//! ornamental planter, climbing vine, and tropical bistro species.

use crate::{
    constants::{PI, TAU},
    math::lerp,
    rng::Rng,
    textures::RgbaTexture,
};
use glam::Vec2;

const TILES: usize = 16;

#[inline]
fn tile_of(c: f32) -> usize {
    ((c.clamp(0.0, 0.9999) * TILES as f32) as usize).min(TILES - 1)
}

#[inline]
fn quad_bezier(p0: Vec2, p1: Vec2, p2: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    p0 * (u * u) + p1 * (2.0 * u * t) + p2 * (t * t)
}

#[inline]
fn over(dst: &mut [f32; 4], src: [f32; 4]) {
    let sa = src[3];
    let da = dst[3];
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return;
    }
    for ch in 0..3 {
        dst[ch] = (src[ch] * sa + dst[ch] * da * (1.0 - sa)) / out_a;
    }
    dst[3] = out_a;
}

#[inline]
fn hash2(seed: u64, x: i32, y: i32) -> f32 {
    let mut h = seed
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((h ^ (h >> 31)) & 0xFF_FFFF) as f32 / 16_777_215.0
}

#[inline]
fn noise2(seed: u64, x: f32, y: f32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - ix as f32;
    let fy = y - iy as f32;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let a = hash2(seed, ix, iy);
    let b = hash2(seed, ix + 1, iy);
    let c = hash2(seed, ix, iy + 1);
    let d = hash2(seed, ix + 1, iy + 1);
    lerp(lerp(a, b, ux), lerp(c, d, ux), uy)
}

#[derive(Clone, Copy)]
struct Stroke {
    a: Vec2,
    b: Vec2,
    w_a: f32,
    w_b: f32,
    rgb: [f32; 3],
}

#[derive(Clone, Copy)]
struct BuxusLeaf {
    center: Vec2,
    sin: f32,
    cos: f32,
    scale: f32,
    shade: f32,
    new_growth: f32,
}

/// Boxwood (`Buxus sempervirens`) opposite-decussate glossy sprig card:
/// forked olive-tan woody twigs clothed in pairs of small, waxy, notched-tip
/// oval leaves with pale midribs and bright chartreuse-lime apical spring shoots.
pub(crate) fn buxus_sprig_card(
    color: Option<[f32; 3]>,
    rng: &mut Rng,
    seed: u64,
    res: u32,
) -> RgbaTexture {
    let base = color.unwrap_or([0.16, 0.34, 0.13]);
    let stem_rgb = [0.34, 0.33, 0.20];
    let px = 1.0 / res as f32;

    let mut strokes: Vec<Stroke> = Vec::new();
    let mut shoots: Vec<(Vec2, Vec2, Vec2, f32)> = Vec::new();

    // Central shoot + 4 opposite-angled lateral shoots
    let p0 = Vec2::new(0.50, 0.96);
    let p1 = Vec2::new(0.50 + rng.range(-0.025, 0.025), 0.54);
    let p2 = Vec2::new(0.50 + rng.range(-0.03, 0.03), 0.13);
    shoots.push((p0, p1, p2, 1.0));

    for &(t_attach, side, spread, len, scale_k) in &[
        (0.24_f32, -1.0_f32, 0.72_f32, 0.36_f32, 0.92_f32),
        (0.26, 1.0, 0.70, 0.35, 0.92),
        (0.54, -1.0, 0.62, 0.27, 0.82),
        (0.56, 1.0, 0.60, 0.26, 0.82),
    ] {
        let a = quad_bezier(p0, p1, p2, t_attach);
        let ang = -PI * 0.5 + side * (spread + rng.range(-0.06, 0.06));
        let dir = Vec2::new(ang.cos(), ang.sin());
        let tip = (a + dir * len).clamp(Vec2::splat(0.11), Vec2::splat(0.89));
        let mid = a.lerp(tip, 0.52) + Vec2::new(side * 0.02, -0.02);
        shoots.push((a, mid, tip, scale_k));
    }

    let mut leaves: Vec<BuxusLeaf> = Vec::new();

    for &(s0, s1, s2, scale_k) in &shoots {
        for seg in 0..5 {
            let t0 = seg as f32 / 5.0;
            let t1 = (seg + 1) as f32 / 5.0;
            strokes.push(Stroke {
                a: quad_bezier(s0, s1, s2, t0),
                b: quad_bezier(s0, s1, s2, t1),
                w_a: lerp(0.0085, 0.0034, t0) * scale_k,
                w_b: lerp(0.0085, 0.0034, t1) * scale_k,
                rgb: stem_rgb,
            });
        }

        let nodes = if scale_k > 0.95 { 8usize } else { 5usize };
        for n in 0..nodes {
            let t = (n as f32 + 0.35) / nodes as f32;
            let node = quad_bezier(s0, s1, s2, t);
            let tang = (quad_bezier(s0, s1, s2, (t + 0.04).min(1.0))
                - quad_bezier(s0, s1, s2, (t - 0.04).max(0.0)))
            .try_normalize()
            .unwrap_or(Vec2::new(0.0, -1.0));
            let perp = Vec2::new(-tang.y, tang.x);
            let new_growth = ((t - 0.58) / 0.42).clamp(0.0, 1.0);

            // Opposite pair + occasional decussate cross leaf for natural twig fullness
            for &side in &[-1.0_f32, 1.0] {
                let splay = lerp(0.98, 0.56, t) + rng.range(-0.12, 0.12);
                let dir = (tang * splay.cos() + perp * (side * splay.sin())).normalize();
                let l_scale = lerp(0.145, 0.105, t) * scale_k * rng.variance_mul(0.10);
                let petiole = 0.012 * scale_k;
                let center = (node + dir * (petiole + l_scale * 0.46))
                    .clamp(Vec2::splat(0.09), Vec2::splat(0.91));
                strokes.push(Stroke {
                    a: node,
                    b: node + dir * (petiole + l_scale * 0.12),
                    w_a: 0.0032 * scale_k,
                    w_b: 0.0018 * scale_k,
                    rgb: [0.38, 0.44, 0.20],
                });
                let rot = PI * 0.5 - dir.y.atan2(dir.x);
                let (sin, cos) = rot.sin_cos();
                leaves.push(BuxusLeaf {
                    center,
                    sin,
                    cos,
                    scale: l_scale,
                    shade: lerp(0.76, 1.12, t) * rng.range(0.92, 1.08),
                    new_growth,
                });
            }
            if n % 2 == 1 {
                let dir = (tang + perp * rng.range(-0.25, 0.25)).normalize();
                let l_scale = lerp(0.130, 0.098, t) * scale_k * rng.variance_mul(0.08);
                let center =
                    (node + dir * (l_scale * 0.42)).clamp(Vec2::splat(0.09), Vec2::splat(0.91));
                let rot = PI * 0.5 - dir.y.atan2(dir.x);
                let (sin, cos) = rot.sin_cos();
                leaves.push(BuxusLeaf {
                    center,
                    sin,
                    cos,
                    scale: l_scale,
                    shade: lerp(0.88, 1.18, t) * rng.range(0.95, 1.08),
                    new_growth: (new_growth + 0.18).clamp(0.0, 1.0),
                });
            }
        }

        // Apical 2-leaf bright spring-growth bud pair at tip
        let tip_tan = (s2 - s1).try_normalize().unwrap_or(Vec2::new(0.0, -1.0));
        let base_a = tip_tan.y.atan2(tip_tan.x);
        for fan in [-0.30_f32, 0.30] {
            let ang = base_a + fan;
            let dir = Vec2::new(ang.cos(), ang.sin());
            let l_scale = 0.096 * scale_k;
            let center = (s2 + dir * (l_scale * 0.46)).clamp(Vec2::splat(0.08), Vec2::splat(0.92));
            let rot = PI * 0.5 - ang;
            let (sin, cos) = rot.sin_cos();
            leaves.push(BuxusLeaf {
                center,
                sin,
                cos,
                scale: l_scale,
                shade: rng.range(1.12, 1.26),
                new_growth: 1.0,
            });
        }
    }

    let mut stroke_bins: Vec<Vec<usize>> = vec![Vec::new(); TILES * TILES];
    for (idx, st) in strokes.iter().enumerate() {
        let pad = st.w_a.max(st.w_b) + px * 2.0;
        let x0 = tile_of((st.a.x.min(st.b.x) - pad).max(0.0));
        let x1 = tile_of((st.a.x.max(st.b.x) + pad).min(1.0));
        let y0 = tile_of((st.a.y.min(st.b.y) - pad).max(0.0));
        let y1 = tile_of((st.a.y.max(st.b.y) + pad).min(1.0));
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                stroke_bins[ty * TILES + tx].push(idx);
            }
        }
    }

    let mut leaf_bins: Vec<Vec<usize>> = vec![Vec::new(); TILES * TILES];
    for (idx, lf) in leaves.iter().enumerate() {
        let rad = lf.scale * 0.62 + px * 2.0;
        let x0 = tile_of((lf.center.x - rad).max(0.0));
        let x1 = tile_of((lf.center.x + rad).min(1.0));
        let y0 = tile_of((lf.center.y - rad).max(0.0));
        let y1 = tile_of((lf.center.y + rad).min(1.0));
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                leaf_bins[ty * TILES + tx].push(idx);
            }
        }
    }

    let mut tex = RgbaTexture::new(res, res);
    for y in 0..res {
        let v = (y as f32 + 0.5) * px;
        let ty = tile_of(v);
        for x in 0..res {
            let u = (x as f32 + 0.5) * px;
            let tile_idx = ty * TILES + tile_of(u);
            let s_bin = &stroke_bins[tile_idx];
            let l_bin = &leaf_bins[tile_idx];
            if s_bin.is_empty() && l_bin.is_empty() {
                continue;
            }
            let q = Vec2::new(u, v);
            let mut acc = [0.0f32; 4];

            for &idx in s_bin {
                let st = &strokes[idx];
                let ab = st.b - st.a;
                let t = ((q - st.a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let w = lerp(st.w_a, st.w_b, t);
                let d = (q - (st.a + ab * t)).length();
                let a = ((w - d) / px + 0.5).clamp(0.0, 1.0);
                if a > 0.0 {
                    let cyl = 0.78 + 0.36 * (1.0 - (d / w.max(1e-4)).clamp(0.0, 1.0));
                    over(
                        &mut acc,
                        [
                            (st.rgb[0] * cyl).min(1.0),
                            (st.rgb[1] * cyl).min(1.0),
                            (st.rgb[2] * cyl).min(1.0),
                            a,
                        ],
                    );
                }
            }

            if !l_bin.is_empty() {
                let mottle = noise2(seed ^ 0xB070, u * 24.0, v * 24.0);
                for &idx in l_bin {
                    let lf = &leaves[idx];
                    let p = q - lf.center;
                    let r_max = lf.scale * 0.62 + px * 2.0;
                    if p.length_squared() > r_max * r_max {
                        continue;
                    }
                    let local =
                        Vec2::new(p.x * lf.cos - p.y * lf.sin, -(p.x * lf.sin + p.y * lf.cos))
                            / lf.scale;
                    // Buxus sempervirens rounded-elliptic thick blade with tiny emarginate tip notch
                    let y_norm = ((local.y + 0.50) / 1.00).clamp(0.0, 1.0);
                    let env = (y_norm * PI).sin().max(0.0).powf(0.58);
                    let half_w = 0.33 * env;
                    let tip_notch = if local.y > 0.38 && local.x.abs() < 0.065 {
                        0.038
                            * (1.0 - local.x.abs() / 0.065)
                            * ((local.y - 0.38) / 0.12).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let d_side = local.x.abs() - half_w;
                    let d_end = local.y.abs() - (0.50 - tip_notch);
                    let sdf = Vec2::new(d_side.max(0.0), d_end.max(0.0)).length()
                        + d_side.max(d_end).min(0.0);
                    let edge = (0.85 / (res as f32 * lf.scale.max(0.03))).max(0.0014);
                    let alpha = (0.5 - sdf / edge).clamp(0.0, 1.0);
                    if alpha <= 0.0 {
                        continue;
                    }

                    if acc[3] > 0.15 && alpha > 0.25 {
                        let occ = 1.0 - 0.24 * alpha;
                        acc[0] *= occ;
                        acc[1] *= occ;
                        acc[2] *= occ;
                    }

                    let midrib_w = lerp(0.020, 0.006, y_norm);
                    let midrib_k = if local.y < 0.44 && local.x.abs() < midrib_w {
                        1.0 - local.x.abs() / midrib_w
                    } else {
                        0.0
                    };
                    // Waxy boxwood cuticle sheen across convex upper dome
                    let dome_gloss = ((-((local.x - 0.07) * 3.8).powi(2)
                        - ((local.y - 0.08) * 2.6).powi(2))
                    .exp())
                        * 0.18;
                    let rim = (1.0 + sdf / 0.055).clamp(0.0, 1.0) * 0.10;
                    let shade =
                        lf.shade * (0.86 + 0.20 * y_norm + 0.10 * mottle + dome_gloss + rim);

                    let mature = [base[0], base[1], base[2]];
                    let spring = [
                        (base[0] * 1.55 + 0.08).min(0.56),
                        (base[1] * 1.48 + 0.09).min(0.68),
                        (base[2] * 1.05 + 0.02).min(0.24),
                    ];
                    let col_base = [
                        lerp(mature[0], spring[0], lf.new_growth),
                        lerp(mature[1], spring[1], lf.new_growth),
                        lerp(mature[2], spring[2], lf.new_growth),
                    ];
                    let c = [
                        (col_base[0] * shade + 0.14 * midrib_k).clamp(0.0, 1.0),
                        (col_base[1] * shade + 0.15 * midrib_k).clamp(0.0, 1.0),
                        (col_base[2] * shade + 0.04 * midrib_k).clamp(0.0, 1.0),
                    ];
                    over(&mut acc, [c[0], c[1], c[2], alpha]);
                }
            }

            tex.set(
                x,
                y,
                [
                    (acc[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[2].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[3].clamp(0.0, 1.0) * 255.0) as u8,
                ],
            );
        }
    }
    tex.dilate_alpha_rgb(12);
    tex
}

#[derive(Clone, Copy)]
struct ScaleSeg {
    a: Vec2,
    b: Vec2,
    w: f32,
    shade: f32,
    tip_warmth: f32,
}

#[allow(clippy::too_many_arguments)]
fn spawn_cypress_fan(
    origin: Vec2,
    angle: f32,
    length: f32,
    depth: u32,
    layer_shade: f32,
    rng: &mut Rng,
    twigs: &mut Vec<Stroke>,
    scales: &mut Vec<ScaleSeg>,
) {
    let bend = rng.range(-0.11, 0.11);
    let mid_dir = Vec2::new((angle + bend * 0.5).cos(), (angle + bend * 0.5).sin());
    let end_dir = Vec2::new((angle + bend).cos(), (angle + bend).sin());
    let mid = (origin + mid_dir * (length * 0.52)).clamp(Vec2::splat(0.08), Vec2::splat(0.92));
    let tip = (mid + end_dir * (length * 0.48)).clamp(Vec2::splat(0.09), Vec2::splat(0.93));

    if depth == 0 {
        twigs.push(Stroke {
            a: origin,
            b: origin.lerp(mid, 0.65),
            w_a: 0.014,
            w_b: 0.008,
            rgb: [0.36, 0.25, 0.17],
        });
    }

    let steps = ((length / 0.026).ceil() as usize).max(3);
    let start_s = if depth == 0 { 1 } else { 0 };
    for s in start_s..steps {
        let t0 = s as f32 / steps as f32;
        let t1 = (s + 1) as f32 / steps as f32;
        let pa = quad_bezier(origin, mid, tip, t0);
        let pb = quad_bezier(origin, mid, tip, t1);
        let apex_taper = if depth >= 2 {
            lerp(1.0, 0.52, t1.powf(1.4))
        } else {
            1.0 - 0.18 * t1
        };
        let w = lerp(0.030, 0.019, (depth as f32 / 3.0).clamp(0.0, 1.0)) * apex_taper;
        scales.push(ScaleSeg {
            a: pa,
            b: pb,
            w,
            shade: layer_shade
                * lerp(0.80, 1.14, (depth as f32) * 0.22 + t0 * 0.38)
                * rng.range(0.93, 1.07),
            tip_warmth: if depth >= 2 { t1 * 0.85 } else { t1 * 0.35 },
        });
    }

    if depth < 2 {
        let sub_forks = if depth == 0 { 3 } else { 2 };
        for k in 0..sub_forks {
            let t_attach = 0.24 + 0.28 * k as f32 + rng.range(-0.04, 0.04);
            let attach = quad_bezier(origin, mid, tip, t_attach.clamp(0.16, 0.84));
            for &side in &[-1.0_f32, 1.0] {
                let fork_ang = angle + bend * t_attach + side * rng.range(0.32, 0.52);
                let sub_len = length * rng.range(0.58, 0.74);
                spawn_cypress_fan(
                    attach,
                    fork_ang,
                    sub_len,
                    depth + 1,
                    layer_shade,
                    rng,
                    twigs,
                    scales,
                );
            }
        }
        spawn_cypress_fan(
            quad_bezier(origin, mid, tip, 0.72),
            angle + bend + rng.range(-0.07, 0.07),
            length * 0.56,
            depth + 1,
            layer_shade * 1.04,
            rng,
            twigs,
            scales,
        );
    }
}

/// Mediterranean Italian Cypress / Arborvitae (`Cupressus sempervirens`) scale-leaf fan spray:
/// lush multi-layered bouquet of flattened dichotomous scale-leaf plumes in warm olive-green.
pub(crate) fn scale_cypress_card(
    color: Option<[f32; 3]>,
    rng: &mut Rng,
    _seed: u64,
    res: u32,
) -> RgbaTexture {
    let base = color.unwrap_or([0.20, 0.35, 0.15]);
    let px = 1.0 / res as f32;

    let mut twigs: Vec<Stroke> = Vec::new();
    let mut scales: Vec<ScaleSeg> = Vec::new();

    // Back shadow plumes + sunlit front plumes fanning out from the woody collar
    for &(origin, ang, len, layer_shade) in &[
        (Vec2::new(0.48, 0.94), -PI * 0.5 - 0.46, 0.52, 0.74),
        (Vec2::new(0.52, 0.94), -PI * 0.5 + 0.46, 0.52, 0.74),
        (Vec2::new(0.50, 0.95), -PI * 0.5 - 0.20, 0.62, 0.90),
        (Vec2::new(0.50, 0.95), -PI * 0.5 + 0.20, 0.62, 0.92),
        (Vec2::new(0.50, 0.96), -PI * 0.5, 0.66, 1.06),
    ] {
        spawn_cypress_fan(
            origin,
            ang,
            len,
            0,
            layer_shade,
            rng,
            &mut twigs,
            &mut scales,
        );
    }

    let mut scale_bins: Vec<Vec<usize>> = vec![Vec::new(); TILES * TILES];
    for (idx, sc) in scales.iter().enumerate() {
        let len = (sc.b - sc.a).length();
        let pad = sc.w * 1.6 + len * 0.28 + px * 2.0;
        let x0 = tile_of((sc.a.x.min(sc.b.x) - pad).max(0.0));
        let x1 = tile_of((sc.a.x.max(sc.b.x) + pad).min(1.0));
        let y0 = tile_of((sc.a.y.min(sc.b.y) - pad).max(0.0));
        let y1 = tile_of((sc.a.y.max(sc.b.y) + pad).min(1.0));
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                scale_bins[ty * TILES + tx].push(idx);
            }
        }
    }

    let mut tex = RgbaTexture::new(res, res);
    for y in 0..res {
        let v = (y as f32 + 0.5) * px;
        let ty = tile_of(v);
        for x in 0..res {
            let u = (x as f32 + 0.5) * px;
            let q = Vec2::new(u, v);
            let mut acc = [0.0f32; 4];

            for st in &twigs {
                let ab = st.b - st.a;
                let t = ((q - st.a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let w = lerp(st.w_a, st.w_b, t);
                let d = (q - (st.a + ab * t)).length();
                let a = ((w - d) / px + 0.5).clamp(0.0, 1.0);
                if a > 0.0 {
                    over(&mut acc, [st.rgb[0], st.rgb[1], st.rgb[2], a]);
                }
            }

            let bin = &scale_bins[ty * TILES + tile_of(u)];
            for &idx in bin {
                let sc = &scales[idx];
                let ab = sc.b - sc.a;
                let len = ab.length().max(1e-5);
                let dir = ab / len;
                let perp = Vec2::new(-dir.y, dir.x);
                let rel = q - sc.a;
                let along = rel.dot(dir) / len;
                if !(-0.18..=1.22).contains(&along) {
                    continue;
                }
                let across = rel.dot(perp);
                // Imbricate diamond-lobed scale bead: swells in middle + paired keeled lateral points
                let bead = (along.clamp(0.0, 1.0) * PI).sin().powf(0.55);
                let w_eff = sc.w * (0.52 + 0.68 * bead);
                let d = across.abs() - w_eff;
                let alpha = (0.5 - d / (px * 1.1)).clamp(0.0, 1.0);
                if alpha <= 0.0 {
                    continue;
                }
                if acc[3] > 0.20 && alpha > 0.30 {
                    acc[0] *= 0.92;
                    acc[1] *= 0.92;
                    acc[2] *= 0.92;
                }
                // Scale-junction shadow crease at base of bead + bright keeled ridge along center
                let crease = 0.78 + 0.28 * along.clamp(0.0, 1.0);
                let ridge = 1.08 - 0.22 * (across.abs() / w_eff.max(1e-4)).clamp(0.0, 1.0);
                let s = sc.shade * crease * ridge;
                let warm = [
                    1.0 + 0.24 * sc.tip_warmth,
                    1.0 + 0.14 * sc.tip_warmth,
                    1.0 - 0.14 * sc.tip_warmth,
                ];
                over(
                    &mut acc,
                    [
                        (base[0] * s * warm[0]).clamp(0.0, 1.0),
                        (base[1] * s * warm[1]).clamp(0.0, 1.0),
                        (base[2] * s * warm[2]).clamp(0.0, 1.0),
                        alpha,
                    ],
                );
            }

            tex.set(
                x,
                y,
                [
                    (acc[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[2].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[3].clamp(0.0, 1.0) * 255.0) as u8,
                ],
            );
        }
    }
    tex.dilate_alpha_rgb(10);
    tex
}

#[derive(Clone, Copy)]
struct ViolaLeaf {
    center: Vec2,
    sin: f32,
    cos: f32,
    scale: f32,
    shade: f32,
}

#[derive(Clone, Copy)]
struct PansyBloom {
    center: Vec2,
    sin: f32,
    cos: f32,
    radius: f32,
    banner_rgb: [f32; 3],
    wing_rgb: [f32; 3],
    lip_rgb: [f32; 3],
    blotch_rgb: [f32; 3],
}

/// 2x2 sub-card floral bouquet & leaf-bed atlas (`LeafCardLayout::Floral`):
/// - Quadrant 0 (top-left): Royal Parisian violet-purple & white Pansy (*Viola*) blooms + scalloped foliage
/// - Quadrant 1 (top-right): Sunny Parisian cadmium-orange & marigold Pansy blooms + scalloped foliage
/// - Quadrant 2 (bottom-left): Primrose-cream & soft lavender Pansy blooms + scalloped foliage
/// - Quadrant 3 (bottom-right): Pure lush scalloped *Viola* & *Vinca* green foliage cluster (for cushion base & cascade vines)
pub(crate) fn floral_pansy_atlas_card(
    color: Option<[f32; 3]>,
    rng: &mut Rng,
    seed: u64,
    res: u32,
) -> RgbaTexture {
    let foliage_base = color.unwrap_or([0.14, 0.34, 0.12]);
    let px = 1.0 / res as f32;

    let mut strokes: Vec<Stroke> = Vec::new();
    let mut leaves: Vec<ViolaLeaf> = Vec::new();
    let mut blooms: Vec<PansyBloom> = Vec::new();

    for q_idx in 0..4usize {
        let q_origin = Vec2::new(
            if (q_idx & 1) == 0 { 0.0 } else { 0.50 },
            if (q_idx & 2) == 0 { 0.0 } else { 0.50 },
        );
        let q_min = q_origin + Vec2::splat(0.052);
        let q_max = q_origin + Vec2::splat(0.448);
        let hub = q_origin + Vec2::new(0.25, 0.34);

        // Radiating green stems + crenate Viola leaves framing the blooms
        let leaf_n = if q_idx == 3 { 22usize } else { 18usize };
        for i in 0..leaf_n {
            let layer = i as f32 / (leaf_n - 1) as f32;
            let ang = -PI * 0.5 + (i as f32 * 2.399_963).sin() * 1.45 + rng.range(-0.18, 0.18);
            let dir = Vec2::new(ang.cos(), ang.sin());
            let dist = lerp(0.135, 0.055, layer) * rng.range(0.82, 1.18);
            let l_scale = lerp(0.135, 0.095, layer) * rng.variance_mul(0.10);
            let center = (q_origin + Vec2::new(0.25, 0.27) + dir * dist).clamp(
                q_min + Vec2::splat(l_scale * 0.45),
                q_max - Vec2::splat(l_scale * 0.45),
            );
            strokes.push(Stroke {
                a: hub,
                b: center,
                w_a: 0.0036,
                w_b: 0.0020,
                rgb: [0.20, 0.36, 0.14],
            });
            let out_ang = (center - hub)
                .try_normalize()
                .map(|d| d.y.atan2(d.x))
                .unwrap_or(-PI * 0.5)
                + rng.range(-0.32, 0.32);
            let rot = PI * 0.5 - out_ang;
            let (sin, cos) = rot.sin_cos();
            leaves.push(ViolaLeaf {
                center,
                sin,
                cos,
                scale: l_scale,
                shade: lerp(0.72, 1.12, layer) * rng.range(0.92, 1.08),
            });
        }

        if q_idx < 3 {
            // 4 overlapping velvety 5-petal Pansy blossoms per floral quadrant
            let offsets = [
                Vec2::new(-0.068, 0.032),
                Vec2::new(0.072, 0.026),
                Vec2::new(-0.024, -0.064),
                Vec2::new(0.036, -0.020),
            ];
            for (b_i, &off) in offsets.iter().enumerate() {
                let c = (q_origin
                    + Vec2::new(0.25, 0.245)
                    + off
                    + Vec2::new(rng.range(-0.014, 0.014), rng.range(-0.014, 0.014)))
                .clamp(q_min + Vec2::splat(0.075), q_max - Vec2::splat(0.075));
                let rad = rng.range(0.068, 0.084);
                let rot = rng.range(-0.42, 0.42);
                let (sin, cos) = rot.sin_cos();

                let (banner_rgb, wing_rgb, lip_rgb, blotch_rgb) = match q_idx {
                    0 => {
                        // Royal Parisian Violet-Purple & Snow-White Pansies
                        if b_i % 2 == 0 {
                            (
                                [0.30, 0.10, 0.56],
                                [0.92, 0.90, 0.97],
                                [0.94, 0.92, 0.98],
                                [0.16, 0.04, 0.34],
                            )
                        } else {
                            (
                                [0.42, 0.15, 0.68],
                                [0.68, 0.46, 0.88],
                                [0.86, 0.78, 0.96],
                                [0.18, 0.04, 0.36],
                            )
                        }
                    }
                    1 => {
                        // Warm Parisian Cadmium-Orange & Marigold-Amber Pansies
                        if b_i % 2 == 0 {
                            (
                                [0.94, 0.40, 0.05],
                                [0.97, 0.52, 0.07],
                                [0.98, 0.60, 0.09],
                                [0.36, 0.08, 0.03],
                            )
                        } else {
                            (
                                [0.88, 0.30, 0.04],
                                [0.96, 0.66, 0.12],
                                [0.98, 0.74, 0.16],
                                [0.32, 0.07, 0.02],
                            )
                        }
                    }
                    _ => {
                        // Primrose-Cream, Butter-Gold & Sky-Lavender Pansies
                        if b_i % 2 == 0 {
                            (
                                [0.58, 0.52, 0.86],
                                [0.96, 0.94, 0.76],
                                [0.98, 0.92, 0.52],
                                [0.22, 0.10, 0.44],
                            )
                        } else {
                            (
                                [0.95, 0.93, 0.84],
                                [0.84, 0.80, 0.96],
                                [0.96, 0.95, 0.88],
                                [0.24, 0.12, 0.46],
                            )
                        }
                    }
                };

                blooms.push(PansyBloom {
                    center: c,
                    sin,
                    cos,
                    radius: rad,
                    banner_rgb,
                    wing_rgb,
                    lip_rgb,
                    blotch_rgb,
                });
            }
        }
    }

    let mut tex = RgbaTexture::new(res, res);
    for y in 0..res {
        let v = (y as f32 + 0.5) * px;
        // Strict clean cross-gutter at 0.48..0.52 so mipmaps never bleed across the 2x2 quadrants
        if (0.478..=0.522).contains(&v) {
            continue;
        }
        for x in 0..res {
            let u = (x as f32 + 0.5) * px;
            if (0.478..=0.522).contains(&u) {
                continue;
            }
            let q = Vec2::new(u, v);
            let mut acc = [0.0f32; 4];

            for st in &strokes {
                let ab = st.b - st.a;
                let t = ((q - st.a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let w = lerp(st.w_a, st.w_b, t);
                let d = (q - (st.a + ab * t)).length();
                let a = ((w - d) / px + 0.5).clamp(0.0, 1.0);
                if a > 0.0 {
                    over(&mut acc, [st.rgb[0], st.rgb[1], st.rgb[2], a]);
                }
            }

            let mottle = noise2(seed ^ 0xF10A, u * 28.0, v * 28.0);
            for lf in &leaves {
                let p = q - lf.center;
                let r_max = lf.scale * 0.65;
                if p.length_squared() > r_max * r_max {
                    continue;
                }
                let local = Vec2::new(p.x * lf.cos - p.y * lf.sin, -(p.x * lf.sin + p.y * lf.cos))
                    / lf.scale;
                let y_norm = ((local.y + 0.48) / 0.98).clamp(0.0, 1.0);
                let crenate = 0.028 * (y_norm * 7.0 * TAU).cos().abs();
                let half_w = 0.34 * (y_norm * PI).sin().max(0.0).powf(0.68) + crenate;
                let d_side = local.x.abs() - half_w;
                let d_end = local.y.abs() - 0.49;
                let sdf = Vec2::new(d_side.max(0.0), d_end.max(0.0)).length()
                    + d_side.max(d_end).min(0.0);
                let edge = (0.9 / (res as f32 * lf.scale)).max(0.0018);
                let alpha = (0.5 - sdf / edge).clamp(0.0, 1.0);
                if alpha <= 0.0 {
                    continue;
                }
                if acc[3] > 0.15 && alpha > 0.25 {
                    acc[0] *= 0.80;
                    acc[1] *= 0.80;
                    acc[2] *= 0.80;
                }
                let vein = if local.x.abs() < 0.016 && local.y < 0.42 {
                    1.18
                } else {
                    1.0 + 0.08 * ((local.y - local.x.abs() * 0.55) * 12.0).sin().abs()
                };
                let s = lf.shade * vein * (0.88 + 0.20 * y_norm + 0.10 * mottle);
                over(
                    &mut acc,
                    [
                        (foliage_base[0] * s).clamp(0.0, 1.0),
                        (foliage_base[1] * s).clamp(0.0, 1.0),
                        (foliage_base[2] * s).clamp(0.0, 1.0),
                        alpha,
                    ],
                );
            }

            // Composite 5-petal velvety Pansy blossoms over foliage with soft drop-shadow AO
            for bl in &blooms {
                let p = q - bl.center;
                if p.length_squared() > (bl.radius * 1.35).powi(2) {
                    continue;
                }
                let lp =
                    Vec2::new(p.x * bl.cos - p.y * bl.sin, p.x * bl.sin + p.y * bl.cos) / bl.radius;

                // Cast soft petal shadow onto underlying green Viola leaves
                let r_center = lp.length();
                if r_center < 1.06 && acc[3] > 0.2 {
                    let shadow = 1.0 - 0.28 * (1.0 - (r_center / 1.06)).clamp(0.0, 1.0);
                    acc[0] *= shadow;
                    acc[1] *= shadow;
                    acc[2] *= shadow;
                }

                // 5 overlapping petals: 2 upper banners, 2 lateral wings, 1 broad lower lip
                let petals: [(Vec2, Vec2, [f32; 3], bool); 5] = [
                    (
                        Vec2::new(-0.28, -0.34),
                        Vec2::new(0.56, 0.52),
                        bl.banner_rgb,
                        false,
                    ),
                    (
                        Vec2::new(0.28, -0.34),
                        Vec2::new(0.56, 0.52),
                        bl.banner_rgb,
                        false,
                    ),
                    (
                        Vec2::new(-0.42, 0.04),
                        Vec2::new(0.52, 0.46),
                        bl.wing_rgb,
                        true,
                    ),
                    (
                        Vec2::new(0.42, 0.04),
                        Vec2::new(0.52, 0.46),
                        bl.wing_rgb,
                        true,
                    ),
                    (
                        Vec2::new(0.0, 0.38),
                        Vec2::new(0.64, 0.54),
                        bl.lip_rgb,
                        true,
                    ),
                ];

                for &(pc, pr, p_rgb, has_blotch) in &petals {
                    let dp = (lp - pc) / pr;
                    let ang = dp.y.atan2(dp.x);
                    let ruffle = 1.0 + 0.045 * (ang * 7.0).cos();
                    let dist = dp.length() - ruffle;
                    let p_edge = (1.1 / (res as f32 * bl.radius * pr.x)).max(0.015);
                    let pa = (0.5 - dist / p_edge).clamp(0.0, 1.0);
                    if pa <= 0.0 {
                        continue;
                    }
                    if acc[3] > 0.2 && pa > 0.3 {
                        acc[0] *= 0.90;
                        acc[1] *= 0.90;
                        acc[2] *= 0.90;
                    }
                    // Velvety radial silk striation + cupped rim highlight
                    let ray_ang = lp.y.atan2(lp.x);
                    let silk = 0.94 + 0.08 * (ray_ang * 26.0).sin().abs();
                    let cup_shade = lerp(0.84, 1.10, (r_center / 0.95).clamp(0.0, 1.0)) * silk;
                    let mut col = [
                        (p_rgb[0] * cup_shade).clamp(0.0, 1.0),
                        (p_rgb[1] * cup_shade).clamp(0.0, 1.0),
                        (p_rgb[2] * cup_shade).clamp(0.0, 1.0),
                    ];

                    // Dark velvety pansy face blotch + radiating whisker lines on wings & lower lip
                    if has_blotch {
                        let whisker = (1.0 - (ray_ang * 18.0).sin().abs()).powi(3);
                        let blotch_r = 0.40 + 0.14 * whisker;
                        let bk = ((blotch_r - r_center) / 0.14).clamp(0.0, 1.0);
                        col[0] = lerp(col[0], bl.blotch_rgb[0], bk * 0.92);
                        col[1] = lerp(col[1], bl.blotch_rgb[1], bk * 0.92);
                        col[2] = lerp(col[2], bl.blotch_rgb[2], bk * 0.92);
                    }

                    over(&mut acc, [col[0], col[1], col[2], pa]);
                }

                // Glowing golden-yellow central Pansy eye spot
                let eye_d = (lp - Vec2::new(0.0, 0.05)).length() - 0.135;
                let eye_a = (0.5 - eye_d / 0.035).clamp(0.0, 1.0);
                if eye_a > 0.0 {
                    over(&mut acc, [0.99, 0.80, 0.08, eye_a]);
                }
            }

            tex.set(
                x,
                y,
                [
                    (acc[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[2].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[3].clamp(0.0, 1.0) * 255.0) as u8,
                ],
            );
        }
    }
    tex.dilate_alpha_rgb(8);
    tex
}

/// Glossy tropical Bird-of-Paradise / banana paddle blade (`LeafShape::Paddle`):
/// pale celadon-lime raised central midrib, ~96 crisp parallel-pinnate lateral veins,
/// waxy tropical sheen, and 3 natural wind-split notches along the margin.
pub(crate) fn paddle_leaf_card(
    color: Option<[f32; 3]>,
    _rng: &mut Rng,
    seed: u64,
    res: u32,
) -> RgbaTexture {
    let base = color.unwrap_or([0.16, 0.38, 0.14]);
    let px = 1.0 / res as f32;
    let mut tex = RgbaTexture::new(res, res);

    // Asymmetric natural V-shaped wind tears (along_y, side, tear_depth_frac, half_gap)
    let splits: [(f32, f32, f32, f32); 3] = [
        (0.34, -1.0, 0.74, 0.016),
        (0.68, -1.0, 0.64, 0.014),
        (0.52, 1.0, 0.70, 0.015),
    ];

    for y in 0..res {
        let v = (y as f32 + 0.5) * px;
        // along = 0 at petiole collar (v = 0.96), 1 at rounded paddle apex (v = 0.04)
        let along = ((0.96 - v) / 0.92).clamp(-0.05, 1.05);
        if !(0.0..=1.0).contains(&along) {
            continue;
        }
        let collar = (along / 0.13).clamp(0.0, 1.0).powf(0.62);
        let apex = ((1.0 - along) / 0.18).clamp(0.0, 1.0).powf(0.50);
        let belly = 0.90 + 0.14 * (along * PI).sin();
        let half_w = 0.41 * (0.06 + 0.94 * collar * apex * belly);
        let midrib_w = lerp(0.020, 0.0045, along);

        for x in 0..res {
            let u = (x as f32 + 0.5) * px;
            let dx = u - 0.5;
            let ax = dx.abs();
            // Smoothly rounded botanical paddle apex so ribbon tips never look polygonal
            let d_tip_cap = if along > 0.80 {
                let ey = (along - 0.80) / 0.195;
                let ex = ax / 0.34;
                (Vec2::new(ex, ey).length() - 1.0) * 0.22
            } else {
                -1.0
            };
            let d_outer = (ax - half_w).max(d_tip_cap);
            if d_outer > px * 2.0 {
                continue;
            }

            // Evaluate V-shaped wind splits slanted along the 74-deg parallel lateral veins
            let vein_coord = along - ax * 0.28;
            let mut split_sdf = -1.0_f32;
            for &(s_y, s_side, depth_frac, gap) in &splits {
                if dx * s_side > 0.0 && ax > half_w * (1.0 - depth_frac) {
                    let reach = ((ax - half_w * (1.0 - depth_frac)) / (half_w * depth_frac))
                        .clamp(0.0, 1.0);
                    let open_w = gap * reach.powf(0.85);
                    let d_tear = open_w - (vein_coord - s_y).abs();
                    split_sdf = split_sdf.max(d_tear);
                }
            }

            let sdf = d_outer.max(split_sdf);
            let alpha = (0.5 - sdf / (px * 1.15)).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }

            let mottle = noise2(seed ^ 0x572E, u * 14.0, v * 14.0);
            let mut shade = (0.84 + 0.22 * along + 0.08 * mottle)
                * (1.02 - 0.14 * (ax / half_w.max(1e-3)).powi(2));
            let rgb = if ax < midrib_w {
                // Raised cylindrical pale celadon-lime midrib
                let k = 1.0 - (ax / midrib_w).powi(2);
                let cyl = 0.92 + 0.26 * k;
                [
                    lerp(base[0], 0.42, 0.72 * k) * cyl,
                    lerp(base[1], 0.62, 0.72 * k) * cyl,
                    lerp(base[2], 0.24, 0.65 * k) * cyl,
                ]
            } else {
                // ~96 crisp parallel-pinnate Strelitzia / banana lateral striations + waxy specular band
                let p_primary = (vein_coord * 48.0 * TAU).cos() * 0.5 + 0.5;
                let p_fine = (vein_coord * 144.0 * TAU).cos() * 0.5 + 0.5;
                let vein_ridge = 0.91 + 0.13 * p_primary.powi(3) + 0.05 * p_fine;
                let gloss_band = (-(((ax / half_w.max(1e-3)) - 0.42) * 3.4).powi(2)).exp() * 0.14;
                let margin_rim = (1.0 + d_outer / 0.018).clamp(0.0, 1.0) * 0.10;
                shade *= vein_ridge + gloss_band + margin_rim;
                [
                    (base[0] * shade + 0.03 * p_primary.powi(4)).clamp(0.0, 1.0),
                    (base[1] * shade + 0.04 * p_primary.powi(4)).clamp(0.0, 1.0),
                    (base[2] * shade).clamp(0.0, 1.0),
                ]
            };

            tex.set(
                x,
                y,
                [
                    (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
                    (alpha * 255.0) as u8,
                ],
            );
        }
    }
    tex.dilate_alpha_rgb(12);
    tex
}
