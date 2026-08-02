use crate::{
    Channels, DecodeError, DecodedImage, ImageDesc,
    header::{END_MARKER, HEADER_SIZE, parse_header},
    types::Pixel,
};

const OP_RGB: u8 = 0xfe;
const OP_RGBA: u8 = 0xff;
const MASK_2: u8 = 0xc0;
const OP_INDEX: u8 = 0x00;

pub fn decode(
    input: &[u8],
    requested_channels: Option<Channels>,
) -> Result<DecodedImage, DecodeError> {
    if input.len() < HEADER_SIZE + END_MARKER.len() {
        return Err(DecodeError::InputTooShort);
    }

    let desc = parse_header(input)?;
    let marker_start = input.len() - END_MARKER.len();

    if input[marker_start..] != END_MARKER {
        return Err(DecodeError::InvalidEndMarker);
    }

    let output_channels = requested_channels.unwrap_or(desc.channels);
    let expected_pixels = pixel_count(desc)?;
    let output_len = output_len(desc, output_channels)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(output_len)
        .map_err(|_| DecodeError::ImageTooLarge)?;

    let chunks = &input[HEADER_SIZE..marker_start];
    let mut decoder = Decoder::new(chunks);

    for _ in 0..expected_pixels {
        let pixel = decoder.next_pixel()?;
        write_pixel(&mut pixels, pixel, output_channels);
    }

    Ok(DecodedImage {
        pixels,
        desc,
        output_channels,
    })
}

struct Decoder<'a> {
    chunks: &'a [u8],
    cursor: usize,
    previous: Pixel,
    index: [Pixel; 64],
    run_remaining: u8,
}

impl<'a> Decoder<'a> {
    fn new(chunks: &'a [u8]) -> Self {
        Self {
            chunks,
            cursor: 0,
            previous: Pixel::INITIAL,
            index: [Pixel::default(); 64],
            run_remaining: 0,
        }
    }

    fn next_pixel(&mut self) -> Result<Pixel, DecodeError> {
        if self.run_remaining > 0 {
            self.run_remaining -= 1;
            return Ok(self.previous);
        }

        let byte = self.read_byte()?;

        let pixel = match byte {
            OP_RGB => {
                let [r, g, b] = self.read_operands()?;

                Pixel {
                    r,
                    g,
                    b,
                    a: self.previous.a,
                }
            }
            OP_RGBA => {
                let [r, g, b, a] = self.read_operands()?;

                Pixel { r, g, b, a }
            }
            byte if byte & MASK_2 == OP_INDEX => self.index[usize::from(byte & 0x3f)],
            _ => return Err(DecodeError::UnsupportedChunk(byte)),
        };

        self.previous = pixel;
        self.index[pixel.hash()] = pixel;

        Ok(pixel)
    }

    fn read_byte(&mut self) -> Result<u8, DecodeError> {
        let byte = self
            .chunks
            .get(self.cursor)
            .copied()
            .ok_or(DecodeError::TruncatedChunk)?;
        self.cursor += 1;
        Ok(byte)
    }

    fn read_operands<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let end = self
            .cursor
            .checked_add(N)
            .ok_or(DecodeError::TruncatedChunk)?;
        let bytes = self
            .chunks
            .get(self.cursor..end)
            .ok_or(DecodeError::TruncatedChunk)?;

        self.cursor = end;

        Ok(bytes.try_into().expect("fixed-size slice"))
    }
}

fn pixel_count(desc: ImageDesc) -> Result<usize, DecodeError> {
    let width = usize::try_from(desc.width).map_err(|_| DecodeError::SizeOverflow)?;
    let height = usize::try_from(desc.height).map_err(|_| DecodeError::SizeOverflow)?;

    width.checked_mul(height).ok_or(DecodeError::SizeOverflow)
}

fn output_len(desc: ImageDesc, output_channels: Channels) -> Result<usize, DecodeError> {
    let len = pixel_count(desc)?
        .checked_mul(output_channels.count())
        .ok_or(DecodeError::SizeOverflow)?;

    if len > isize::MAX as usize {
        return Err(DecodeError::ImageTooLarge);
    }

    Ok(len)
}

fn write_pixel(pixels: &mut Vec<u8>, pixel: Pixel, output_channels: Channels) {
    pixels.push(pixel.r);
    pixels.push(pixel.g);
    pixels.push(pixel.b);

    if output_channels == Channels::Rgba {
        pixels.push(pixel.a);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColorSpace;

    fn header(width: u32, height: u32, channels: Channels) -> Vec<u8> {
        let mut input = Vec::with_capacity(HEADER_SIZE);
        input.extend_from_slice(b"qoif");
        input.extend_from_slice(&width.to_be_bytes());
        input.extend_from_slice(&height.to_be_bytes());
        input.push(channels as u8);
        input.push(ColorSpace::SrgbWithLinearAlpha as u8);
        input
    }

    fn file(width: u32, height: u32, channels: Channels, chunks: &[u8]) -> Vec<u8> {
        let mut input = header(width, height, channels);
        input.extend_from_slice(chunks);
        input.extend_from_slice(&END_MARKER);
        input
    }

    #[test]
    fn decodes_one_pixel_rgb_file_to_rgb() {
        let input = file(1, 1, Channels::Rgb, &[OP_RGB, 1, 2, 3]);

        assert_eq!(
            decode(&input, None),
            Ok(DecodedImage {
                pixels: vec![1, 2, 3],
                desc: ImageDesc {
                    width: 1,
                    height: 1,
                    channels: Channels::Rgb,
                    colorspace: ColorSpace::SrgbWithLinearAlpha,
                },
                output_channels: Channels::Rgb,
            })
        );
    }

    #[test]
    fn decodes_one_pixel_rgb_file_to_rgba_with_opaque_alpha() {
        let input = file(1, 1, Channels::Rgb, &[OP_RGB, 1, 2, 3]);
        let image = decode(&input, Some(Channels::Rgba)).expect("decode should succeed");

        assert_eq!(image.pixels, vec![1, 2, 3, 255]);
        assert_eq!(image.desc.channels, Channels::Rgb);
        assert_eq!(image.output_channels, Channels::Rgba);
    }

    #[test]
    fn decodes_one_pixel_rgba_file_to_rgba() {
        let input = file(1, 1, Channels::Rgba, &[OP_RGBA, 1, 2, 3, 4]);
        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(image.pixels, vec![1, 2, 3, 4]);
        assert_eq!(image.desc.channels, Channels::Rgba);
        assert_eq!(image.output_channels, Channels::Rgba);
    }

    #[test]
    fn decodes_one_pixel_rgba_file_to_rgb_with_alpha_discarded() {
        let input = file(1, 1, Channels::Rgba, &[OP_RGBA, 1, 2, 3, 4]);
        let image = decode(&input, Some(Channels::Rgb)).expect("decode should succeed");

        assert_eq!(image.pixels, vec![1, 2, 3]);
        assert_eq!(image.desc.channels, Channels::Rgba);
        assert_eq!(image.output_channels, Channels::Rgb);
    }

    #[test]
    fn rejects_truncated_rgb_chunk() {
        let input = file(1, 1, Channels::Rgb, &[OP_RGB, 1, 2]);

        assert_eq!(decode(&input, None), Err(DecodeError::TruncatedChunk));
    }

    #[test]
    fn rejects_truncated_rgba_chunk() {
        let input = file(1, 1, Channels::Rgba, &[OP_RGBA, 1, 2, 3]);

        assert_eq!(decode(&input, None), Err(DecodeError::TruncatedChunk));
    }

    #[test]
    fn rejects_incorrect_end_marker() {
        let mut input = file(1, 1, Channels::Rgb, &[OP_RGB, 1, 2, 3]);
        *input.last_mut().expect("end marker byte") = 0;

        assert_eq!(decode(&input, None), Err(DecodeError::InvalidEndMarker));
    }

    #[test]
    fn decodes_index_chunk_from_stored_pixel() {
        let pixel = Pixel {
            r: 1,
            g: 2,
            b: 3,
            a: 255,
        };
        let input = file(
            2,
            1,
            Channels::Rgb,
            &[
                OP_RGB,
                pixel.r,
                pixel.g,
                pixel.b,
                OP_INDEX | pixel.hash() as u8,
            ],
        );

        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(image.pixels, vec![1, 2, 3, 1, 2, 3]);
    }

    #[test]
    fn rejects_output_size_overflow_or_unsupported_size() {
        let desc = ImageDesc {
            width: u32::MAX,
            height: u32::MAX,
            channels: Channels::Rgba,
            colorspace: ColorSpace::SrgbWithLinearAlpha,
        };

        assert!(matches!(
            output_len(desc, Channels::Rgba),
            Err(DecodeError::SizeOverflow | DecodeError::ImageTooLarge)
        ));
    }

    #[test]
    fn rejects_stream_shorter_than_header_and_trailer() {
        let input = header(1, 1, Channels::Rgb);

        assert_eq!(decode(&input, None), Err(DecodeError::InputTooShort));
    }

    #[test]
    fn returns_error_for_header_and_trailer_without_chunks() {
        let input = file(1, 1, Channels::Rgb, &[]);

        assert_eq!(decode(&input, None), Err(DecodeError::TruncatedChunk));
    }
}
