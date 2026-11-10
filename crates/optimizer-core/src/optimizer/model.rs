//! Internal integer model used by the packing engine.
//!
//! All lengths are stored in hundredths of a millimetre (`i64`) so that comparisons are exact.
//! Every piece is inflated by the kerf on its right and top side and every usable panel area is
//! extended by one kerf. Two inflated pieces that do not overlap are therefore always separated by
//! at least one full kerf, regardless of which side they touch.

/// Scale factor between external `f64` millimetres and internal units.
pub const SCALE: f64 = 100.0;

pub fn to_units(value: f64) -> i64 {
    (value * SCALE).round() as i64
}

pub fn to_mm(value: i64) -> f64 {
    value as f64 / SCALE
}

/// One orientation a piece may be placed in (inflated dimensions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Orientation {
    pub w: i64,
    pub h: i64,
    pub rotated: bool,
}

/// A single required piece after expanding quantities.
#[derive(Debug, Clone)]
pub struct Piece {
    pub id: String,
    /// Inflated dimensions in the requested orientation.
    pub w: i64,
    pub h: i64,
    pub can_rotate: bool,
    /// Real (not inflated) area, used for utilisation statistics.
    pub real_area: i64,
    /// Group of identical pieces (same dimensions and rotation rule).
    pub kind: usize,
}

impl Piece {
    /// Orientations in the given frame. `transposed` swaps the axes of the frame.
    pub fn orientations(&self, transposed: bool) -> impl Iterator<Item = Orientation> {
        let (w, h) = if transposed {
            (self.h, self.w)
        } else {
            (self.w, self.h)
        };
        let first = Orientation {
            w,
            h,
            rotated: false,
        };
        let second = if self.can_rotate && self.w != self.h {
            Some(Orientation {
                w: h,
                h: w,
                rotated: true,
            })
        } else {
            None
        };
        std::iter::once(first).chain(second)
    }

    pub fn inflated_area(&self) -> i64 {
        self.w * self.h
    }
}

/// Usable panel description in internal units.
#[derive(Debug, Clone)]
pub struct BinSpec {
    pub type_index: usize,
    /// Inflated usable width/height (usable + kerf).
    pub w: i64,
    pub h: i64,
    /// Full panel area in mm² (cost of using the panel).
    pub panel_area: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl Rect {
    pub fn area(&self) -> i64 {
        self.w * self.h
    }

    pub fn transposed(&self) -> Rect {
        Rect {
            x: self.y,
            y: self.x,
            w: self.h,
            h: self.w,
        }
    }
}

/// A piece placed in a bin, inflated coordinates relative to the usable origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub piece: usize,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    pub rotated: bool,
}

impl Placed {
    pub fn rect(&self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
        }
    }

    pub fn transposed(&self) -> Placed {
        Placed {
            piece: self.piece,
            x: self.y,
            y: self.x,
            w: self.h,
            h: self.w,
            rotated: self.rotated,
        }
    }
}

/// A packed panel.
#[derive(Debug, Clone)]
pub struct PackedBin {
    pub spec: BinSpec,
    pub placed: Vec<Placed>,
    pub used_real_area: i64,
}

impl PackedBin {
    pub fn new(spec: BinSpec, placed: Vec<Placed>, pieces: &[Piece]) -> Self {
        let used_real_area = placed.iter().map(|p| pieces[p.piece].real_area).sum();
        Self {
            spec,
            placed,
            used_real_area,
        }
    }

    /// Fraction of the full panel area covered by real pieces.
    pub fn efficiency(&self) -> f64 {
        to_mm2(self.used_real_area) / self.spec.panel_area
    }
}

pub fn to_mm2(area_units: i64) -> f64 {
    area_units as f64 / (SCALE * SCALE)
}

/// Small deterministic random generator (xorshift64*), so results are reproducible.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform value in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}
