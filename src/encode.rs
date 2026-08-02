use crate::{
    Channels, EncodeError, ImageDesc,
    header::{END_MARKER, HEADER_SIZE},
    types::Pixel,
};

const MAGIC: &[u8; 4] = b"qoif";
const PIXELS_MAX: u32 = 400_000_000;

const OP_RGB: u8 = 0xfe;
const OP_RGBA: u8 = 0xff;
const OP_INDEX: u8 = 0x00;
const OP_DIFF: u8 = 0x40;
const OP_LUMA: u8 = 0x80;
const OP_RUN: u8 = 0xc0;

pub fn encode(pixels: &[u8], desc: ImageDesc) -> Result<Vec<u8>, EncodeError> {
    let max_size = validate_input(pixels, desc)?;
    let channels = desc.channels.count();
    let mut output = Vec::new();
    output
        .try_reserve_exact(max_size)
        .map_err(|_| EncodeError::ImageTooLarge)?;

    write_header(&mut output, desc);

    let mut index = [Pixel::default(); 64];
    let mut previous = Pixel::INITIAL;
    let mut pixel = previous;
    let mut run = 0u8;
    let pixel_end = pixels.len().saturating_sub(channels);
    let mut position = 0;

    while position < pixels.len() {
        pixel.r = pixels[position];
        pixel.g = pixels[position + 1];
        pixel.b = pixels[position + 2];

        if desc.channels == Channels::Rgba {
            pixel.a = pixels[position + 3];
        }

        if pixel == previous {
            run += 1;

            if run == 62 || position == pixel_end {
                output.push(OP_RUN | (run - 1));
                run = 0;
            }
        } else {
            if run > 0 {
                output.push(OP_RUN | (run - 1));
                run = 0;
            }

            let index_pos = pixel.hash();

            if index[index_pos] == pixel {
                output.push(OP_INDEX | index_pos as u8);
            } else {
                index[index_pos] = pixel;
                encode_changed_pixel(&mut output, pixel, previous);
            }
        }

        previous = pixel;
        position += channels;
    }

    output.extend_from_slice(&END_MARKER);

    Ok(output)
}

fn validate_input(pixels: &[u8], desc: ImageDesc) -> Result<usize, EncodeError> {
    if desc.width == 0 {
        return Err(EncodeError::ZeroWidth);
    }

    if desc.height == 0 {
        return Err(EncodeError::ZeroHeight);
    }

    if desc.height >= PIXELS_MAX / desc.width {
        return Err(EncodeError::ImageTooLarge);
    }

    let pixel_count = pixel_count(desc)?;
    let expected_len = pixel_count
        .checked_mul(desc.channels.count())
        .ok_or(EncodeError::SizeOverflow)?;

    if pixels.len() != expected_len {
        return Err(EncodeError::InvalidPixelBufferLength {
            expected: expected_len,
            actual: pixels.len(),
        });
    }

    let max_size = pixel_count
        .checked_mul(desc.channels.count() + 1)
        .and_then(|len| len.checked_add(HEADER_SIZE))
        .and_then(|len| len.checked_add(END_MARKER.len()))
        .ok_or(EncodeError::SizeOverflow)?;

    if max_size > isize::MAX as usize {
        return Err(EncodeError::ImageTooLarge);
    }

    Ok(max_size)
}

fn pixel_count(desc: ImageDesc) -> Result<usize, EncodeError> {
    let width = usize::try_from(desc.width).map_err(|_| EncodeError::SizeOverflow)?;
    let height = usize::try_from(desc.height).map_err(|_| EncodeError::SizeOverflow)?;

    width.checked_mul(height).ok_or(EncodeError::SizeOverflow)
}

fn write_header(output: &mut Vec<u8>, desc: ImageDesc) {
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&desc.width.to_be_bytes());
    output.extend_from_slice(&desc.height.to_be_bytes());
    output.push(desc.channels as u8);
    output.push(desc.colorspace as u8);
}

fn encode_changed_pixel(output: &mut Vec<u8>, pixel: Pixel, previous: Pixel) {
    if pixel.a == previous.a {
        let dr = pixel.r.wrapping_sub(previous.r) as i8;
        let dg = pixel.g.wrapping_sub(previous.g) as i8;
        let db = pixel.b.wrapping_sub(previous.b) as i8;

        let dr_dg = dr.wrapping_sub(dg);
        let db_dg = db.wrapping_sub(dg);

        if (-2..=1).contains(&dr) && (-2..=1).contains(&dg) && (-2..=1).contains(&db) {
            output.push(OP_DIFF | ((dr + 2) as u8) << 4 | ((dg + 2) as u8) << 2 | (db + 2) as u8);
        } else if (-8..=7).contains(&dr_dg) && (-32..=31).contains(&dg) && (-8..=7).contains(&db_dg)
        {
            output.push(OP_LUMA | (dg + 32) as u8);
            output.push(((dr_dg + 8) as u8) << 4 | (db_dg + 8) as u8);
        } else {
            output.push(OP_RGB);
            output.push(pixel.r);
            output.push(pixel.g);
            output.push(pixel.b);
        }
    } else {
        output.push(OP_RGBA);
        output.push(pixel.r);
        output.push(pixel.g);
        output.push(pixel.b);
        output.push(pixel.a);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColorSpace, decode};

    fn desc(width: u32, height: u32, channels: Channels) -> ImageDesc {
        ImageDesc {
            width,
            height,
            channels,
            colorspace: ColorSpace::SrgbWithLinearAlpha,
        }
    }

    #[test]
    fn encodes_one_pixel_rgb_file() {
        let encoded = encode(&[1, 2, 3], desc(1, 1, Channels::Rgb)).expect("encode succeeds");
        let decoded = decode(&encoded, None).expect("decode succeeds");

        assert_eq!(decoded.pixels, vec![1, 2, 3]);
        assert_eq!(decoded.desc, desc(1, 1, Channels::Rgb));
    }

    #[test]
    fn encodes_initial_opaque_black_as_run() {
        let encoded = encode(&[0, 0, 0], desc(1, 1, Channels::Rgb)).expect("encode succeeds");

        assert_eq!(encoded[HEADER_SIZE], OP_RUN);
    }

    #[test]
    fn rejects_zero_width() {
        assert_eq!(
            encode(&[], desc(0, 1, Channels::Rgb)),
            Err(EncodeError::ZeroWidth)
        );
    }

    #[test]
    fn rejects_zero_height() {
        assert_eq!(
            encode(&[], desc(1, 0, Channels::Rgb)),
            Err(EncodeError::ZeroHeight)
        );
    }

    #[test]
    fn rejects_invalid_pixel_buffer_length() {
        assert_eq!(
            encode(&[1, 2], desc(1, 1, Channels::Rgb)),
            Err(EncodeError::InvalidPixelBufferLength {
                expected: 3,
                actual: 2,
            })
        );
    }
}
