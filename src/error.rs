use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    InputTooShort,
    InvalidMagic,
    ZeroWidth,
    ZeroHeight,
    InvalidChannels(u8),
    InvalidColorSpace(u8),
    ImageTooLarge,
    SizeOverflow,
    InvalidEndMarker,
    TruncatedChunk,
    TooManyPixels,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooShort => formatter.write_str("input is too short"),
            Self::InvalidMagic => formatter.write_str("invalid QOI magic"),
            Self::ZeroWidth => formatter.write_str("image width is zero"),
            Self::ZeroHeight => formatter.write_str("image height is zero"),
            Self::InvalidChannels(value) => {
                write!(formatter, "invalid channel count: {value}")
            }
            Self::InvalidColorSpace(value) => {
                write!(formatter, "invalid colorspace: {value}")
            }
            Self::ImageTooLarge => formatter.write_str("image exceeds the supported size"),
            Self::SizeOverflow => formatter.write_str("image size calculation overflowed"),
            Self::InvalidEndMarker => formatter.write_str("invalid QOI end marker"),
            Self::TruncatedChunk => formatter.write_str("truncated QOI chunk"),
            Self::TooManyPixels => formatter.write_str("chunk exceeds expected pixel count"),
        }
    }
}

impl std::error::Error for DecodeError {}
