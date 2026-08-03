use crate::{
    Channels, DecodeError, DecodedImage, ImageDesc,
    header::{END_MARKER, HEADER_SIZE, parse_header},
    types::Pixel,
};

const OP_RGB: u8 = 0xfe;
const OP_RGBA: u8 = 0xff;
const MASK_2: u8 = 0xc0;
const MASK_DATA: u8 = !MASK_2;
const OP_INDEX: u8 = 0x00;
const OP_DIFF: u8 = 0x40;
const OP_LUMA: u8 = 0x80;
const OP_RUN: u8 = 0xc0;
const OP_INDEX_MAX: u8 = OP_INDEX | MASK_DATA;
const OP_DIFF_MAX: u8 = OP_DIFF | MASK_DATA;
const OP_LUMA_MAX: u8 = OP_LUMA | MASK_DATA;
const OP_RUN_MAX: u8 = OP_RGB - 1;

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

    match output_channels {
        Channels::Rgb => {
            for _ in 0..expected_pixels {
                let pixel = decoder.next_pixel()?;
                pixels.extend_from_slice(&[pixel.r, pixel.g, pixel.b]);
            }
        }
        Channels::Rgba => {
            for _ in 0..expected_pixels {
                let pixel = decoder.next_pixel()?;
                pixels.extend_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
            }
        }
    }

    if decoder.run_remaining != 0 {
        return Err(DecodeError::TooManyPixels);
    }

    if decoder.cursor != chunks.len() {
        return Err(DecodeError::TrailingData);
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
                let [r, g, b, a] = self.read_rgba_operands()?;

                Pixel { r, g, b, a }
            }
            OP_INDEX..=OP_INDEX_MAX => self.index[usize::from(byte & MASK_DATA)],
            OP_DIFF..=OP_DIFF_MAX => {
                let dr = ((byte >> 4) & 0x03) as i8 - 2;
                let dg = ((byte >> 2) & 0x03) as i8 - 2;
                let db = (byte & 0x03) as i8 - 2;

                Pixel {
                    r: self.previous.r.wrapping_add_signed(dr),
                    g: self.previous.g.wrapping_add_signed(dg),
                    b: self.previous.b.wrapping_add_signed(db),
                    a: self.previous.a,
                }
            }
            OP_LUMA..=OP_LUMA_MAX => {
                let second = self.read_byte()?;

                let dg = (byte & 0x3f) as i8 - 32;
                let dr_dg = ((second >> 4) & 0x0f) as i8 - 8;
                let db_dg = (second & 0x0f) as i8 - 8;

                Pixel {
                    r: self.previous.r.wrapping_add_signed(dg + dr_dg),
                    g: self.previous.g.wrapping_add_signed(dg),
                    b: self.previous.b.wrapping_add_signed(dg + db_dg),
                    a: self.previous.a,
                }
            }
            OP_RUN..=OP_RUN_MAX => {
                self.run_remaining = byte & MASK_DATA;
                self.previous
            }
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

    fn read_rgba_operands(&mut self) -> Result<[u8; 4], DecodeError> {
        let operands = self
            .chunks
            .get(self.cursor..)
            .and_then(|tail| tail.first_chunk::<4>())
            .ok_or(DecodeError::TruncatedChunk)?;

        self.cursor += operands.len();

        Ok(*operands)
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
    fn rejects_rgba_chunk_missing_all_operands() {
        let input = file(1, 1, Channels::Rgba, &[OP_RGBA]);

        assert_eq!(decode(&input, None), Err(DecodeError::TruncatedChunk));
    }

    #[test]
    fn rejects_rgba_chunk_with_one_operand() {
        let input = file(1, 1, Channels::Rgba, &[OP_RGBA, 1]);

        assert_eq!(decode(&input, None), Err(DecodeError::TruncatedChunk));
    }

    #[test]
    fn rejects_rgba_chunk_with_two_operands() {
        let input = file(1, 1, Channels::Rgba, &[OP_RGBA, 1, 2]);

        assert_eq!(decode(&input, None), Err(DecodeError::TruncatedChunk));
    }

    #[test]
    fn rgba_chunk_leaves_cursor_at_following_opcode() {
        let input = file(
            2,
            1,
            Channels::Rgba,
            &[OP_RGBA, 1, 2, 3, 4, OP_RGBA, 5, 6, 7, 8],
        );

        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(image.pixels, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn rgba_operands_ending_at_chunk_boundary_succeed() {
        let input = file(2, 1, Channels::Rgba, &[OP_RUN, OP_RGBA, 1, 2, 3, 4]);

        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(image.pixels, vec![0, 0, 0, 255, 1, 2, 3, 4]);
    }

    #[test]
    fn rgba_completing_image_rejects_unused_chunk_data() {
        let input = file(1, 1, Channels::Rgba, &[OP_RGBA, 1, 2, 3, 4, OP_INDEX]);

        assert_eq!(decode(&input, None), Err(DecodeError::TrailingData));
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
    fn decodes_diff_with_wrapping_arithmetic() {
        let diff = OP_DIFF | (1 << 4) | (2 << 2) | 3;
        let input = file(1, 1, Channels::Rgba, &[diff]);

        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(image.pixels, vec![255, 0, 1, 255]);
    }

    #[test]
    fn decodes_luma_relative_differences() {
        let input = file(
            2,
            1,
            Channels::Rgba,
            &[
                OP_RGB,
                100,
                100,
                100,
                OP_LUMA | (32 + 5),
                ((8 + 2) << 4) | (8 - 3),
            ],
        );

        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(image.pixels, vec![100, 100, 100, 255, 107, 105, 102, 255]);
    }

    #[test]
    fn decodes_luma_with_wrapping_arithmetic() {
        let input = file(1, 1, Channels::Rgba, &[OP_LUMA, 0]);

        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(image.pixels, vec![216, 224, 216, 255]);
    }

    #[test]
    fn rejects_luma_chunk_missing_second_byte() {
        assert_eq!(
            decode(&file(1, 1, Channels::Rgb, &[OP_LUMA]), None),
            Err(DecodeError::TruncatedChunk)
        );
    }

    #[test]
    fn decodes_run_length_one_of_previous_pixel() {
        let input = file(2, 1, Channels::Rgb, &[OP_RGB, 1, 2, 3, OP_RUN]);

        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(image.pixels, vec![1, 2, 3, 1, 2, 3]);
    }

    #[test]
    fn decodes_maximum_run_of_62_pixels() {
        let input = file(62, 1, Channels::Rgb, &[OP_RUN | 61]);

        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(image.pixels, vec![0; 62 * 3]);
    }

    #[test]
    fn run_continuation_does_not_consume_input() {
        let input = file(
            4,
            1,
            Channels::Rgb,
            &[OP_RGB, 10, 20, 30, OP_RUN | 1, OP_RGB, 1, 2, 3],
        );

        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(
            image.pixels,
            vec![10, 20, 30, 10, 20, 30, 10, 20, 30, 1, 2, 3]
        );
    }

    #[test]
    fn decodes_run_of_initial_opaque_black_pixel() {
        let input = file(1, 1, Channels::Rgba, &[OP_RUN]);

        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(image.pixels, vec![0, 0, 0, 255]);
    }

    #[test]
    fn rejects_run_exceeding_declared_pixel_count() {
        let input = file(1, 1, Channels::Rgb, &[OP_RUN | 1]);

        assert_eq!(decode(&input, None), Err(DecodeError::TooManyPixels));
    }

    #[test]
    fn run_chunk_indexes_the_current_pixel() {
        let initial_hash = Pixel::INITIAL.hash() as u8;

        let input = file(2, 1, Channels::Rgba, &[OP_RUN, OP_INDEX | initial_hash]);

        let image = decode(&input, None).expect("decode should succeed");

        assert_eq!(image.pixels, vec![0, 0, 0, 255, 0, 0, 0, 255]);
    }

    #[test]
    fn rejects_unused_chunk_data() {
        let input = file(1, 1, Channels::Rgb, &[OP_RGB, 1, 2, 3, OP_INDEX]);

        assert_eq!(decode(&input, None), Err(DecodeError::TrailingData));
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
