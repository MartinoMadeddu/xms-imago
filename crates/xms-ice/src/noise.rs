//! OSL's noises of a point, ported from OpenShadingLanguage (`oslnoise.h`,
//! `simplexnoise.cpp`) so a value made in ICE is the value an OSL shader gets
//! at the same point: the same hash (Bob Jenkins' `bjfinal`, as OpenImageIO
//! has it), lattice, gradients, fade and scale. Tested against OSL's own
//! expected values.
//!
//! Copyright (c) 2009-present Contributors to the Open Shading Language
//! project. All Rights Reserved. SPDX-License-Identifier: BSD-3-Clause.
//!
//! Redistribution and use in source and binary forms, with or without
//! modification, are permitted provided that the following conditions are
//! met: 1. Redistributions of source code must retain the above copyright
//! notice, this list of conditions and the following disclaimer. 2.
//! Redistributions in binary form must reproduce the above copyright notice,
//! this list of conditions and the following disclaimer in the documentation
//! and/or other materials provided with the distribution. 3. Neither the name
//! of the copyright holder nor the names of its contributors may be used to
//! endorse or promote products derived from this software without specific
//! prior written permission.
//!
//! THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
//! IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO,
//! THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
//! PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
//! CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
//! EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
//! PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS;
//! OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//! WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
//! OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF
//! ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

/// OpenImageIO's `bjhash::bjfinal`: three words mixed into one.
fn bjfinal(mut a: u32, mut b: u32, mut c: u32) -> u32 {
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(14));
    a ^= c;
    a = a.wrapping_sub(c.rotate_left(11));
    b ^= a;
    b = b.wrapping_sub(a.rotate_left(25));
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(16));
    a ^= c;
    a = a.wrapping_sub(c.rotate_left(4));
    b ^= a;
    b = b.wrapping_sub(a.rotate_left(14));
    c ^= b;
    c = c.wrapping_sub(b.rotate_left(24));
    c
}

/// OSL's `inthash` of three words.
fn inthash3(k0: u32, k1: u32, k2: u32) -> u32 {
    let start = 0xdead_beef_u32.wrapping_add((3 << 2) + 13);
    bjfinal(start.wrapping_add(k0), start.wrapping_add(k1), start.wrapping_add(k2))
}

/// A hash as a number from 0 to 1 (`bits_to_01`).
fn bits_to_01(bits: u32) -> f32 {
    const BY: f32 = (1.0f64 / u32::MAX as f64) as f32;
    bits as f32 * BY
}

fn ifloor(x: f32) -> i32 {
    x.floor() as i32
}

/// OSL's `cellnoise(p)`: one value from 0 to 1 per unit cube, the same all
/// through it.
pub fn cell(p: [f32; 3]) -> f32 {
    let [x, y, z] = p.map(|v| ifloor(v) as u32);
    bits_to_01(inthash3(x, y, z))
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// A dot product with one of the cube's twelve edge directions, by the hash.
fn grad(hash: i32, x: f32, y: f32, z: f32) -> f32 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    let u = if h & 1 != 0 { -u } else { u };
    let v = if h & 2 != 0 { -v } else { v };
    u + v
}

/// OpenImageIO's `trilerp`, in its order of operations.
fn trilerp(v: [f32; 8], s: f32, t: f32, r: f32) -> f32 {
    let (s1, t1, r1) = (1.0 - s, 1.0 - t, 1.0 - r);
    r1 * (t1 * (v[0] * s1 + v[1] * s) + t * (v[2] * s1 + v[3] * s))
        + r * (t1 * (v[4] * s1 + v[5] * s) + t * (v[6] * s1 + v[7] * s))
}

/// OSL's signed Perlin noise of a point (`snoise(p)`, `noise("perlin", p)`):
/// about -1 to 1, 0 at whole numbers.
pub fn perlin(p: [f32; 3]) -> f32 {
    let [x, y, z] = p;
    let (xi, yi, zi) = (ifloor(x), ifloor(y), ifloor(z));
    let (fx, fy, fz) = (x - xi as f32, y - yi as f32, z - zi as f32);
    let (u, v, w) = (fade(fx), fade(fy), fade(fz));
    let hash = |a: i32, b: i32, c: i32| inthash3(a as u32, b as u32, c as u32) as i32;
    let corner = |dx: i32, dy: i32, dz: i32| {
        grad(
            hash(xi.wrapping_add(dx), yi.wrapping_add(dy), zi.wrapping_add(dz)),
            fx - dx as f32,
            fy - dy as f32,
            fz - dz as f32,
        )
    };
    let v8 = [
        corner(0, 0, 0),
        corner(1, 0, 0),
        corner(0, 1, 0),
        corner(1, 1, 0),
        corner(0, 0, 1),
        corner(1, 0, 1),
        corner(0, 1, 1),
        corner(1, 1, 1),
    ];
    0.9820 * trilerp(v8, u, v, w)
}

/// Simplex noise's gradients: the cube's twelve edges, four again.
const GRAD3: [[f32; 3]; 16] = [
    [1.0, 0.0, 1.0],
    [0.0, 1.0, 1.0],
    [-1.0, 0.0, 1.0],
    [0.0, -1.0, 1.0],
    [1.0, 0.0, -1.0],
    [0.0, 1.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, -1.0, -1.0],
    [1.0, -1.0, 0.0],
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [0.0, 1.0, -1.0],
    [0.0, -1.0, -1.0],
];

fn scramble(v0: u32, v1: u32, v2: u32) -> u32 {
    bjfinal(v0, v1, v2 ^ 0xdead_beef)
}

fn grad3(i: i32, j: i32, k: i32) -> [f32; 3] {
    let h = scramble(i as u32, j as u32, scramble(k as u32, 0, 0));
    GRAD3[(h & 15) as usize]
}

/// OSL's signed simplex noise of a point (`noise("simplex", p)`): about -1 to
/// 1, smoother and cheaper than Perlin's, without its grid.
pub fn simplex(p: [f32; 3]) -> f32 {
    let [x, y, z] = p;
    const F3: f32 = 0.333_333_33;
    const G3: f32 = 0.166_666_67;
    let s = (x + y + z) * F3;
    let (i, j, k) = (ifloor(x + s), ifloor(y + s), ifloor(z + s));
    let t = (i.wrapping_add(j).wrapping_add(k)) as f32 * G3;
    let (x0, y0, z0) = (x - (i as f32 - t), y - (j as f32 - t), z - (k as f32 - t));
    let ((i1, j1, k1), (i2, j2, k2)) = if x0 >= y0 {
        if y0 >= z0 {
            ((1, 0, 0), (1, 1, 0))
        } else if x0 >= z0 {
            ((1, 0, 0), (1, 0, 1))
        } else {
            ((0, 0, 1), (1, 0, 1))
        }
    } else if y0 < z0 {
        ((0, 0, 1), (0, 1, 1))
    } else if x0 < z0 {
        ((0, 1, 0), (0, 1, 1))
    } else {
        ((0, 1, 0), (1, 1, 0))
    };
    let corners = [
        (0, 0, 0, x0, y0, z0),
        (
            i1,
            j1,
            k1,
            x0 - i1 as f32 + G3,
            y0 - j1 as f32 + G3,
            z0 - k1 as f32 + G3,
        ),
        (
            i2,
            j2,
            k2,
            x0 - i2 as f32 + 2.0 * G3,
            y0 - j2 as f32 + 2.0 * G3,
            z0 - k2 as f32 + 2.0 * G3,
        ),
        (1, 1, 1, x0 - 1.0 + 3.0 * G3, y0 - 1.0 + 3.0 * G3, z0 - 1.0 + 3.0 * G3),
    ];
    let mut n = [0.0f32; 4];
    for (c, (di, dj, dk, cx, cy, cz)) in corners.into_iter().enumerate() {
        let t = 0.5 - cx * cx - cy * cy - cz * cz;
        if t >= 0.0 {
            let g = grad3(i.wrapping_add(di), j.wrapping_add(dj), k.wrapping_add(dk));
            let t2 = t * t;
            n[c] = t2 * t2 * (g[0] * cx + g[1] * cy + g[2] * cz);
        }
    }
    68.0 * (n[0] + n[1] + n[2] + n[3])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OSL's own expected values (`oslnoise_test.cpp`), at (x, x, x).
    #[test]
    fn perlin_and_cell_are_osl_s() {
        let perlin_at = [
            0.0, -0.298763, 0.12275, 0.0533643, 0.0, -0.106914, -0.61375, -0.504605, 0.0,
        ];
        for (i, want) in perlin_at.into_iter().enumerate() {
            let x = i as f32 / 4.0;
            let got = perlin([x; 3]);
            assert!((got - want).abs() < 1e-5, "perlin at {x}: {got}, OSL {want}");
        }
        for (x, want) in [(0.5, 0.611068), (1.5, 0.149855)] {
            let got = cell([x; 3]);
            assert!((got - want).abs() < 1e-5, "cell at {x}: {got}, OSL {want}");
        }
        // Whole through a cell, another at its edge.
        assert_eq!(cell([1.0001; 3]), cell([1.0; 3]));
        assert_eq!(cell([0.9999; 3]), cell([0.0; 3]));
        assert_ne!(cell([-0.0001; 3]), cell([0.0; 3]));
    }

    #[test]
    fn simplex_is_signed_smooth_and_the_same_each_time() {
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for i in 0..2000 {
            let p = [i as f32 * 0.137, i as f32 * 0.071 - 3.0, (i % 37) as f32 * 0.29];
            let n = simplex(p);
            assert_eq!(n, simplex(p));
            lo = lo.min(n);
            hi = hi.max(n);
            let near = simplex([p[0] + 1e-4, p[1], p[2]]);
            assert!((near - n).abs() < 0.01, "jumps at {p:?}");
        }
        assert!(lo < -0.5 && hi > 0.5 && lo >= -1.05 && hi <= 1.05, "{lo} {hi}");
    }
}
