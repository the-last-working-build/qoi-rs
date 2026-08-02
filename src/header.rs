use crate::{ColorSpace, DecodeError, ImageDesc, types::Channels};

const MAGIC: &[u8; 4] = b"qoif";
pub(crate) const HEADER_SIZE: usize = 14;
// Used when full stream validation lands with the decoder state machine.
#[allow(dead_code)]
pub(crate) const END_MARKER: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];
const PIXELS_MAX: u32 = 400_000_000;

pub(crate) fn parse_header(input: &[u8]) -> Result<ImageDesc, DecodeError> {
    if input.len() < HEADER_SIZE {
        return Err(DecodeError::InputTooShort);
    }

    if &input[..4] != MAGIC {
        return Err(DecodeError::InvalidMagic);
    }

    let width = u32::from_be_bytes(input[4..8].try_into().expect("fixed-size slice"));
    let height = u32::from_be_bytes(input[8..12].try_into().expect("fixed-size slice"));

    if width == 0 {
        return Err(DecodeError::ZeroWidth);
    }

    if height == 0 {
        return Err(DecodeError::ZeroHeight);
    }

    let channels = Channels::try_from(input[12]).map_err(DecodeError::InvalidChannels)?;
    let colorspace = ColorSpace::try_from(input[13]).map_err(DecodeError::InvalidColorSpace)?;

    if height >= PIXELS_MAX / width {
        return Err(DecodeError::ImageTooLarge);
    }

    width.checked_mul(height).ok_or(DecodeError::SizeOverflow)?;

    Ok(ImageDesc {
        width,
        height,
        channels,
        colorspace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: u32, height: u32, channels: u8, colorspace: u8) -> [u8; HEADER_SIZE] {
        let mut input = [0; HEADER_SIZE];
        input[..4].copy_from_slice(MAGIC);
        input[4..8].copy_from_slice(&width.to_be_bytes());
        input[8..12].copy_from_slice(&height.to_be_bytes());
        input[12] = channels;
        input[13] = colorspace;
        input
    }

    #[test]
    fn parses_valid_one_by_one_rgb_header() {
        let input = header(1, 1, 3, 0);

        assert_eq!(
            parse_header(&input),
            Ok(ImageDesc {
                width: 1,
                height: 1,
                channels: Channels::Rgb,
                colorspace: ColorSpace::SrgbWithLinearAlpha,
            })
        );
    }

    #[test]
    fn parses_valid_one_by_one_rgba_header() {
        let input = header(1, 1, 4, 1);

        assert_eq!(
            parse_header(&input),
            Ok(ImageDesc {
                width: 1,
                height: 1,
                channels: Channels::Rgba,
                colorspace: ColorSpace::AllLinear,
            })
        );
    }

    #[test]
    fn rejects_input_shorter_than_header() {
        let input = [0; HEADER_SIZE - 1];

        assert_eq!(parse_header(&input), Err(DecodeError::InputTooShort));
    }

    #[test]
    fn rejects_invalid_magic() {
        let mut input = header(1, 1, 3, 0);
        input[..4].copy_from_slice(b"noif");

        assert_eq!(parse_header(&input), Err(DecodeError::InvalidMagic));
    }

    #[test]
    fn checks_magic_before_dimensions() {
        let input = [0; HEADER_SIZE];

        assert_eq!(parse_header(&input), Err(DecodeError::InvalidMagic));
    }

    #[test]
    fn rejects_zero_width() {
        let input = header(0, 1, 3, 0);

        assert_eq!(parse_header(&input), Err(DecodeError::ZeroWidth));
    }

    #[test]
    fn rejects_zero_height() {
        let input = header(1, 0, 3, 0);

        assert_eq!(parse_header(&input), Err(DecodeError::ZeroHeight));
    }

    #[test]
    fn rejects_channels_outside_rgb_rgba() {
        for channels in [2, 5] {
            let input = header(1, 1, channels, 0);

            assert_eq!(
                parse_header(&input),
                Err(DecodeError::InvalidChannels(channels))
            );
        }
    }

    #[test]
    fn rejects_colorspace_outside_qoi_range() {
        let input = header(1, 1, 3, 2);

        assert_eq!(parse_header(&input), Err(DecodeError::InvalidColorSpace(2)));
    }

    #[test]
    fn rejects_dimensions_at_c_pixel_limit() {
        let input = header(3, PIXELS_MAX / 3, 3, 0);

        assert_eq!(parse_header(&input), Err(DecodeError::ImageTooLarge));
    }

    #[test]
    fn accepts_dimensions_below_c_pixel_limit() {
        let width = 3;
        let height = PIXELS_MAX / width - 1;
        let input = header(width, height, 3, 0);

        assert!(parse_header(&input).is_ok());
    }

    #[test]
    fn parses_width_and_height_as_big_endian() {
        let input = header(0x0000_0102, 0x0000_0304, 3, 0);

        assert_eq!(
            parse_header(&input),
            Ok(ImageDesc {
                width: 0x0000_0102,
                height: 0x0000_0304,
                channels: Channels::Rgb,
                colorspace: ColorSpace::SrgbWithLinearAlpha,
            })
        );
    }

    #[test]
    fn end_marker_matches_qoi_sentinel() {
        assert_eq!(END_MARKER, [0, 0, 0, 0, 0, 0, 0, 1]);
    }
}
