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
struct PansyPalette {
    banner_inner: [f32; 3],
    banner_outer: [f32; 3],
    wing_base: [f32; 3],
    wing_rim: [f32; 3],
    lip_base: [f32; 3],
    lip_rim: [f32; 3],
    blotch_rgb: [f32; 3],
    halo_rgb: [f32; 3],
    blotch_size: f32,
}

/// Evaluate a single zygomorphic obovate/fan Pansy petal in normalized corolla space `lp`:
/// returns `(signed_distance, along_0_1, across_signed_norm)`.
#[inline]
fn eval_pansy_petal(
    lp: Vec2,
    base_pt: Vec2,
    tip_dir: Vec2,
    len: f32,
    max_half_w: f32,
    notch_depth: f32,
    phase: f32,
) -> (f32, f32, f32) {
    let perp = Vec2::new(-tip_dir.y, tip_dir.x);
    let c = base_pt + tip_dir * (len * 0.53);
    let rel_c = lp - c;
    let u_ax = rel_c.dot(perp);
    let v_ax = rel_c.dot(tip_dir);

    // Narrower tapered claw near throat (v_ax < 0) swelling into a broad rounded outer lobe
    let claw_t = ((v_ax / (len * 0.52)) + 1.0).clamp(0.0, 1.35);
    let taper = lerp(0.42, 1.08, claw_t.powf(0.62));
    let dx = u_ax / (max_half_w * taper).max(1e-4);
    let dy = v_ax / (len * 0.50).max(1e-4);

    let ang = dy.atan2(dx);
    let outer_w = ((dy + 0.25) / 1.15).clamp(0.0, 1.0);
    let ruffle = outer_w
        * (0.030 * (ang * 3.0 + phase).sin()
            + 0.019 * (ang * 5.0 - phase * 1.7).cos()
            + 0.010 * (ang * 9.0 + phase * 0.8).sin());
    let notch = if notch_depth > 0.0 && dy > 0.30 {
        notch_depth * (-(dx * 3.9).powi(2)).exp() * ((dy - 0.30) / 0.70).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let rad_allow = 1.0 + ruffle - notch;
    let sdf = (Vec2::new(dx, dy).length() - rad_allow) * (len * 0.46);
    let along = ((lp - base_pt).dot(tip_dir) / len.max(1e-4)).clamp(0.0, 1.0);
    let across = (u_ax / max_half_w.max(1e-4)).clamp(-1.2, 1.2);
    (sdf, along, across)
}

/// 2x2 single-organ high-resolution botanical floral & foliage atlas (`LeafCardLayout::Floral`):
/// - Quadrant 0 (top-left): Single crisp 5-petal Royal Parisian Violet & Pearl-Ivory Picotee Pansy
/// - Quadrant 1 (top-right): Single crisp 5-petal Parisian Marigold-Amber & Mahogany-Sienna Pansy
/// - Quadrant 2 (bottom-left): Single crisp 5-petal Primrose-Butter & Sky-Periwinkle Whiskered Viola
/// - Quadrant 3 (bottom-right): Single glossy crenate-ovate *Viola* / *Vinca* emerald green leaf blade
pub(crate) fn floral_pansy_atlas_card(
    color: Option<[f32; 3]>,
    _rng: &mut Rng,
    seed: u64,
    res: u32,
) -> RgbaTexture {
    let foliage_base = color.unwrap_or([0.14, 0.35, 0.11]);
    let px = 1.0 / res as f32;

    let palettes: [PansyPalette; 3] = [
        // Quadrant 0: Royal Parisian Velvet Violet & Luminous Pearl-Ivory Picotee
        PansyPalette {
            banner_inner: [0.20, 0.04, 0.44],
            banner_outer: [0.46, 0.17, 0.76],
            wing_base: [0.97, 0.96, 0.94],
            wing_rim: [0.56, 0.36, 0.86],
            lip_base: [0.98, 0.97, 0.95],
            lip_rim: [0.52, 0.32, 0.84],
            blotch_rgb: [0.11, 0.02, 0.25],
            halo_rgb: [0.34, 0.10, 0.64],
            blotch_size: 0.38,
        },
        // Quadrant 1: Sunny Parisian Marigold-Gold & Velvety Burnt Mahogany-Sienna
        PansyPalette {
            banner_inner: [0.68, 0.14, 0.02],
            banner_outer: [0.97, 0.44, 0.04],
            wing_base: [0.98, 0.58, 0.06],
            wing_rim: [1.00, 0.72, 0.11],
            lip_base: [0.99, 0.54, 0.05],
            lip_rim: [1.00, 0.70, 0.09],
            blotch_rgb: [0.22, 0.03, 0.01],
            halo_rgb: [0.74, 0.16, 0.02],
            blotch_size: 0.40,
        },
        // Quadrant 2: Primrose-Cream, Warm Butter-Gold & Sky-Periwinkle Whiskered Viola
        PansyPalette {
            banner_inner: [0.40, 0.36, 0.78],
            banner_outer: [0.68, 0.66, 0.94],
            wing_base: [0.98, 0.95, 0.76],
            wing_rim: [0.78, 0.76, 0.94],
            lip_base: [0.99, 0.90, 0.46],
            lip_rim: [0.99, 0.96, 0.80],
            blotch_rgb: [0.16, 0.06, 0.38],
            halo_rgb: [0.38, 0.30, 0.72],
            blotch_size: 0.17,
        },
    ];

    let dir_ul = Vec2::new(-0.44, -0.90).normalize();
    let dir_ur = Vec2::new(0.44, -0.90).normalize();
    let dir_wl = Vec2::new(-0.985, -0.17).normalize();
    let dir_wr = Vec2::new(0.985, -0.17).normalize();
    let dir_lip = Vec2::new(0.0, 1.0);

    let mut tex = RgbaTexture::new(res, res);
    for y in 0..res {
        let v = (y as f32 + 0.5) * px;
        // Strict clean cross-gutter at 0.48..0.52 and outer border so mipmaps never bleed
        if !(0.016..=0.984).contains(&v) || (0.478..=0.522).contains(&v) {
            continue;
        }
        let qy = usize::from(v >= 0.5);
        let v0 = if qy == 0 { 0.02 } else { 0.52 };
        let sv = (v - v0) / 0.46;
        if !(-0.02..=1.02).contains(&sv) {
            continue;
        }

        for x in 0..res {
            let u = (x as f32 + 0.5) * px;
            if !(0.016..=0.984).contains(&u) || (0.478..=0.522).contains(&u) {
                continue;
            }
            let qx = usize::from(u >= 0.5);
            let q_idx = qx | (qy << 1);
            let u0 = if qx == 0 { 0.02 } else { 0.52 };
            let su = (u - u0) / 0.46;
            if !(-0.02..=1.02).contains(&su) {
                continue;
            }

            let mut acc = [0.0f32; 4];

            if q_idx < 3 {
                // Single high-resolution 5-petal zygomorphic Pansy corolla
                let pal = &palettes[q_idx];
                let lp = Vec2::new(su - 0.50, sv - 0.49) / 0.44;
                if lp.length_squared() > 1.32 {
                    continue;
                }

                let q_seed = seed ^ ((q_idx as u64 + 1) * 0x517C_C1B7);
                let tex_noise = noise2(q_seed, su * 18.0, sv * 18.0);
                let fine_silk = noise2(q_seed ^ 0xA391, su * 54.0, sv * 54.0);
                let edge_aa = (px / (0.46 * 0.44) * 1.15).max(0.004);

                // Back-to-front botanical imbrication (claws overlap past center so zero pinhole gap forms):
                // 0: Left Banner, 1: Right Banner, 2: Left Wing, 3: Right Wing, 4: Anterior Lower Lip
                let specs: [(Vec2, Vec2, f32, f32, f32, u8); 5] = [
                    (Vec2::new(-0.01, 0.09), dir_ul, 1.01, 0.56, 0.0, 0),
                    (Vec2::new(0.01, 0.09), dir_ur, 1.01, 0.56, 0.0, 0),
                    (Vec2::new(0.03, 0.06), dir_wl, 0.96, 0.49, 0.0, 1),
                    (Vec2::new(-0.03, 0.06), dir_wr, 0.96, 0.49, 0.0, 1),
                    (Vec2::new(0.0, -0.01), dir_lip, 0.96, 0.63, 0.085, 2),
                ];

                // Deep-velvet throat backstop ensures 100% solid alpha inside central calyx spur
                let throat_back_d = (lp - Vec2::new(0.0, 0.04)).length() - 0.11;
                let throat_back_a = (0.5 - throat_back_d / edge_aa).clamp(0.0, 1.0);
                if throat_back_a > 0.0 {
                    over(
                        &mut acc,
                        [
                            pal.blotch_rgb[0],
                            pal.blotch_rgb[1],
                            pal.blotch_rgb[2],
                            throat_back_a,
                        ],
                    );
                }

                for (p_i, &(base_pt, t_dir, p_len, half_w, notch, kind)) in specs.iter().enumerate()
                {
                    let phase = (q_idx as f32) * 1.9 + (p_i as f32) * 1.15;
                    let (sdf, along, across) =
                        eval_pansy_petal(lp, base_pt, t_dir, p_len, half_w, notch, phase);

                    // Soft inter-petal contact drop shadow cast onto underlying petals
                    if sdf < 0.055 && acc[3] > 0.25 {
                        let sh_k = ((0.055 - sdf) / 0.055).clamp(0.0, 1.0);
                        let darken = 1.0 - 0.17 * sh_k * (1.0 - 0.45 * along);
                        acc[0] *= darken;
                        acc[1] *= darken;
                        acc[2] *= darken;
                    }

                    let pa = (0.5 - sdf / edge_aa).clamp(0.0, 1.0);
                    if pa <= 0.0 {
                        continue;
                    }

                    // Organic radial silk fibers from petal claw
                    let p_rel = lp - base_pt;
                    let p_ang = p_rel.y.atan2(p_rel.x);
                    let fiber_a = noise2(q_seed ^ (p_i as u64 * 7919), p_ang * 12.0, along * 3.5);
                    let fiber_b = noise2(q_seed ^ (p_i as u64 * 3571), p_ang * 29.0, along * 5.0);
                    let velvet = 0.94 + 0.07 * fiber_a + 0.04 * fiber_b + 0.02 * fine_silk;

                    let mut col = if kind == 0 {
                        // Upper banner petals: rich inner velvet grading to luminous outer rim
                        let g = (along * 0.85 + 0.15 * tex_noise).clamp(0.0, 1.0);
                        [
                            lerp(pal.banner_inner[0], pal.banner_outer[0], g) * velvet,
                            lerp(pal.banner_inner[1], pal.banner_outer[1], g) * velvet,
                            lerp(pal.banner_inner[2], pal.banner_outer[2], g) * velvet,
                        ]
                    } else {
                        let (c_base, c_rim) = if kind == 1 {
                            (pal.wing_base, pal.wing_rim)
                        } else {
                            (pal.lip_base, pal.lip_rim)
                        };
                        // Watercolor picotee / rim blush along outer 28% of petal margin
                        let rim_dist = (-sdf / 0.18).clamp(0.0, 1.0);
                        let picotee = ((1.0 - rim_dist)
                            * (0.68 + 0.52 * fiber_a)
                            * ((along - 0.38) / 0.62).clamp(0.0, 1.0))
                        .clamp(0.0, 1.0);
                        let lobe_sun = 0.93 + 0.09 * along;
                        let mut c = [
                            lerp(c_base[0], c_rim[0], picotee * 0.86) * velvet * lobe_sun,
                            lerp(c_base[1], c_rim[1], picotee * 0.86) * velvet * lobe_sun,
                            lerp(c_base[2], c_rim[2], picotee * 0.86) * velvet * lobe_sun,
                        ];

                        // Authentic velvety rounded-spade Pansy face blotch (on q=0,1) vs fine
                        // botanical pencil-whiskers (on q=2 Primrose Whiskered Viola)
                        let center_bias = if kind == 1 {
                            let target = if p_i == 2 { -0.18 } else { 0.18 };
                            1.0 - ((across - target) * 1.22).powi(2).min(1.0)
                        } else {
                            1.0 - (across * 1.32).powi(2).min(1.0)
                        };

                        if center_bias > 0.0 {
                            let brush = 0.86 + 0.28 * fiber_a + 0.10 * fiber_b;
                            let feather = pal.blotch_size * center_bias.powf(0.58) * brush;
                            let halo_k =
                                ((feather + 0.12 - along) / 0.15).clamp(0.0, 1.0) * center_bias;
                            let core_k = ((feather - along) / 0.065).clamp(0.0, 1.0)
                                * center_bias.powf(0.35);

                            c[0] = lerp(c[0], pal.halo_rgb[0], halo_k * 0.62);
                            c[1] = lerp(c[1], pal.halo_rgb[1], halo_k * 0.62);
                            c[2] = lerp(c[2], pal.halo_rgb[2], halo_k * 0.62);

                            c[0] = lerp(c[0], pal.blotch_rgb[0], core_k * 0.96);
                            c[1] = lerp(c[1], pal.blotch_rgb[1], core_k * 0.96);
                            c[2] = lerp(c[2], pal.blotch_rgb[2], core_k * 0.96);

                            // Delicate sparse pencil-whiskers (prominent on q=2 Whiskered Viola,
                            // short subtle margin-tips on q=0,1 blotched Pansies)
                            let w_freq = if kind == 1 { 7.5 } else { 9.5 };
                            let w_wave =
                                (across * w_freq * PI + fiber_a * 0.9).cos().abs().powi(14);
                            let extra = if q_idx == 2 { 0.28 } else { 0.09 };
                            let w_reach = (pal.blotch_size + extra) * center_bias.powf(0.50);
                            let w_env = ((w_reach - along) / 0.14).clamp(0.0, 1.0)
                                * ((along - 0.04) / 0.06).clamp(0.0, 1.0);
                            let w_k = (w_wave * w_env * 0.92).clamp(0.0, 1.0);
                            c[0] = lerp(c[0], pal.blotch_rgb[0], w_k * 0.88);
                            c[1] = lerp(c[1], pal.blotch_rgb[1], w_k * 0.88);
                            c[2] = lerp(c[2], pal.blotch_rgb[2], w_k * 0.88);
                        }
                        c
                    };

                    // Recessed claw shadow right around throat attachment
                    let throat_ao = 0.84 + 0.16 * (along / 0.28).clamp(0.0, 1.0);
                    col[0] = (col[0] * throat_ao).clamp(0.0, 1.0);
                    col[1] = (col[1] * throat_ao).clamp(0.0, 1.0);
                    col[2] = (col[2] * throat_ao).clamp(0.0, 1.0);

                    over(&mut acc, [col[0], col[1], col[2], pa]);
                }

                // Warm golden-saffron keyhole spur throat + subtle olive-chartreuse stigma brow
                let ep = lp - Vec2::new(0.0, 0.048);
                let v_w = lerp(0.028, 0.064, ((ep.y + 0.052) / 0.105).clamp(0.0, 1.0));
                let keyhole_d = Vec2::new(ep.x / v_w, ep.y / 0.064).length() - 1.0;
                let eye_a = (0.5 - keyhole_d / 0.14).clamp(0.0, 1.0) * acc[3].min(1.0);
                if eye_a > 0.0 {
                    let g_t = ((ep.y + 0.048) / 0.096).clamp(0.0, 1.0);
                    let eye_rgb = [
                        lerp(0.56, 1.00, (g_t * 2.2).clamp(0.0, 1.0)),
                        lerp(0.68, 0.78, g_t),
                        lerp(0.16, 0.02, g_t),
                    ];
                    over(&mut acc, [eye_rgb[0], eye_rgb[1], eye_rgb[2], eye_a]);
                }
            } else {
                // Quadrant 3: Pointed ovate-lanceolate crenate Viola / Vinca garden leaf blade
                let dx = su - 0.50;
                let ax = dx.abs();
                let edge_aa = (px / 0.46 * 1.15).max(0.0025);

                // Slender tapered celadon-green petiole at base (sv in 0.87..0.985)
                if (0.87..=0.985).contains(&sv) {
                    let pt = ((sv - 0.87) / 0.115).clamp(0.0, 1.0);
                    let pet_w = lerp(0.020, 0.012, pt);
                    let p_alpha = (0.5 - (ax - pet_w) / edge_aa).clamp(0.0, 1.0);
                    if p_alpha > 0.0 {
                        let cyl = 0.88 + 0.24 * (1.0 - (ax / pet_w.max(1e-4)).powi(2));
                        over(
                            &mut acc,
                            [
                                (0.22 * cyl).clamp(0.0, 1.0),
                                (0.42 * cyl).clamp(0.0, 1.0),
                                (0.13 * cyl).clamp(0.0, 1.0),
                                p_alpha,
                            ],
                        );
                    }
                }

                // Pointed ovate-serrate lamina from base (sv = 0.90 -> along = 0) to acute tip (sv = 0.03 -> along = 1)
                let along = (0.90 - sv) / 0.87;
                if (-0.02..=1.02).contains(&along) {
                    let a_clamped = along.clamp(0.0, 1.0);
                    // Broadest at along ~ 0.34, tapering cleanly to an acute botanical leaf apex
                    let base_curve = (a_clamped / 0.34).clamp(0.0, 1.0).powf(0.58);
                    let tip_taper = ((1.0 - a_clamped) / 0.68).clamp(0.0, 1.0).powf(0.78);
                    let env = if a_clamped < 0.34 {
                        base_curve
                    } else {
                        tip_taper * (1.0 + 0.12 * ((a_clamped - 0.34) * PI).sin())
                    };
                    // 7 crisp forward-angled crenate-serrate botanical teeth per side
                    let tooth_f = (a_clamped * 7.0).fract();
                    let tooth = (tooth_f.powf(0.55) * (1.0 - tooth_f)).max(0.0) * 3.1;
                    let crenate =
                        0.020 * tooth * (a_clamped * PI).sin().max(0.0) * (1.0 - 0.35 * a_clamped);
                    let half_w = 0.265 * env + crenate;

                    let d_side = ax - half_w;
                    let d_end = (along - 0.50).abs() - 0.50;
                    let sdf = Vec2::new(d_side.max(0.0), d_end.max(0.0)).length()
                        + d_side.max(d_end).min(0.0);
                    let l_alpha = (0.5 - sdf / edge_aa).clamp(0.0, 1.0);

                    if l_alpha > 0.0 {
                        let cell = noise2(seed ^ 0x710A, su * 32.0, sv * 32.0);
                        let micro = noise2(seed ^ 0x3E19, su * 84.0, sv * 84.0);
                        let w_frac = (ax / half_w.max(1e-3)).clamp(0.0, 1.0);

                        let convex = 1.05 - 0.16 * (w_frac - 0.34).powi(2);
                        let margin_rim = ((1.0 + sdf / 0.034).clamp(0.0, 1.0)).powi(2);
                        let midrib_w = lerp(0.014, 0.0028, a_clamped);

                        let rgb = if ax < midrib_w && a_clamped < 0.96 {
                            let k = 1.0 - (ax / midrib_w.max(1e-4)).powi(2);
                            [
                                lerp(foliage_base[0], 0.34, 0.68 * k),
                                lerp(foliage_base[1], 0.56, 0.68 * k),
                                lerp(foliage_base[2], 0.19, 0.62 * k),
                            ]
                        } else {
                            let v_coord = a_clamped - 0.36 * w_frac.powf(1.30);
                            let sec_wave = (v_coord * 6.5 * TAU).cos() * 0.5 + 0.5;
                            let sec_ridge = sec_wave.powi(11) * (1.0 - w_frac * 0.50);
                            let shade =
                                convex * (0.84 + 0.24 * a_clamped + 0.10 * cell + 0.04 * micro);
                            let mut r = foliage_base[0] * shade;
                            let mut g = foliage_base[1] * shade;
                            let mut b = foliage_base[2] * shade;
                            r = lerp(r, 0.29, sec_ridge * 0.46);
                            g = lerp(g, 0.51, sec_ridge * 0.46);
                            b = lerp(b, 0.17, sec_ridge * 0.42);
                            r = lerp(r, 0.29, margin_rim * 0.45);
                            g = lerp(g, 0.52, margin_rim * 0.45);
                            b = lerp(b, 0.15, margin_rim * 0.40);
                            [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)]
                        };

                        over(&mut acc, [rgb[0], rgb[1], rgb[2], l_alpha]);
                    }
                }
            }

            if acc[3] > 0.0 {
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
    }
    tex.dilate_alpha_rgb(10);
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
