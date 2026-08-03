#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Channels {
    Rgb = 3,
    Rgba = 4,
}

impl Channels {
    pub const fn count(self) -> usize {
        self as usize
    }
}

impl TryFrom<u8> for Channels {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            3 => Ok(Self::Rgb),
            4 => Ok(Self::Rgba),
            other => Err(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ColorSpace {
    SrgbWithLinearAlpha = 0,
    AllLinear = 1,
}

impl TryFrom<u8> for ColorSpace {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::SrgbWithLinearAlpha),
            1 => Ok(Self::AllLinear),
            other => Err(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageDesc {
    pub width: u32,
    pub height: u32,
    pub channels: Channels,
    pub colorspace: ColorSpace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    /// Raw RGB or RGBA output bytes.
    pub pixels: Vec<u8>,

    /// Metadata stored in the QOI header.
    pub desc: ImageDesc,

    /// Actual number of channels in `pixels`.
    pub output_channels: Channels,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Pixel {
    pub(crate) r: u8,
    pub(crate) g: u8,
    pub(crate) b: u8,
    pub(crate) a: u8,
}

impl Pixel {
    pub(crate) const INITIAL: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    pub(crate) fn hash(self) -> usize {
        (usize::from(self.r) * 3
            + usize::from(self.g) * 5
            + usize::from(self.b) * 7
            + usize::from(self.a) * 11)
            % 64
    }
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, size_of};

    use super::Pixel;

    const HASH_EDGE_VALUES: [u8; 10] = [0, 1, 2, 15, 31, 63, 127, 128, 254, 255];

    fn qoi_hash_formula(pixel: Pixel) -> usize {
        (usize::from(pixel.r) * 3
            + usize::from(pixel.g) * 5
            + usize::from(pixel.b) * 7
            + usize::from(pixel.a) * 11)
            % 64
    }

    fn narrow_hash_candidate(pixel: Pixel) -> usize {
        let hash = u32::from(pixel.r) * 3
            + u32::from(pixel.g) * 5
            + u32::from(pixel.b) * 7
            + u32::from(pixel.a) * 11;

        (hash & 63) as usize
    }

    #[test]
    fn initial_pixel_is_opaque_black() {
        assert_eq!(
            Pixel::INITIAL,
            Pixel {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }
        );
    }

    #[test]
    fn pixel_layout_is_four_one_byte_aligned_channels() {
        assert_eq!(size_of::<Pixel>(), 4);
        assert_eq!(align_of::<Pixel>(), 1);
    }

    #[test]
    fn pixel_hash_has_known_qoi_slots_and_collision() {
        let transparent_black = Pixel::default();
        let colliding = Pixel {
            r: 5,
            g: 1,
            b: 0,
            a: 4,
        };

        assert_eq!(transparent_black.hash(), 0);
        assert_eq!(colliding.hash(), 0);
        assert_eq!(Pixel::INITIAL.hash(), 53);
        assert_eq!(
            Pixel {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            }
            .hash(),
            14
        );
        assert_eq!(
            Pixel {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            }
            .hash(),
            38
        );
    }

    #[test]
    fn narrow_hash_is_equivalent_at_edges_and_for_two_million_pixels() {
        for &r in &HASH_EDGE_VALUES {
            for &g in &HASH_EDGE_VALUES {
                for &b in &HASH_EDGE_VALUES {
                    for &a in &HASH_EDGE_VALUES {
                        let pixel = Pixel { r, g, b, a };
                        let expected = qoi_hash_formula(pixel);

                        assert_eq!(pixel.hash(), expected);
                        assert_eq!(narrow_hash_candidate(pixel), expected);
                    }
                }
            }
        }

        let mut state = 0x515f_4f49_5f48_4153u64;

        for _ in 0..2_000_000 {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let value = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
            let channels = value.to_le_bytes();
            let pixel = Pixel {
                r: channels[0],
                g: channels[1],
                b: channels[2],
                a: channels[3],
            };
            let expected = qoi_hash_formula(pixel);

            assert_eq!(pixel.hash(), expected);
            assert_eq!(narrow_hash_candidate(pixel), expected);
        }
    }
}
