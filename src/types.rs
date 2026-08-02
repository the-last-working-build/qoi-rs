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
    use super::Pixel;

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
    fn pixel_hash_matches_qoi_formula() {
        let pixel = Pixel {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        };

        assert_eq!(pixel.hash(), 14);
    }
}
