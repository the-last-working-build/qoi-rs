# QOI Behavior

## Header

QOI streams start with a 14-byte header:

- bytes 0..4: magic `qoif`
- bytes 4..8: image width as big-endian `u32`
- bytes 8..12: image height as big-endian `u32`
- byte 12: channel count, either `3` for RGB or `4` for RGBA
- byte 13: colorspace, either `0` for sRGB with linear alpha or `1` for all linear

The decoder validates header fields in this order:

1. At least 14 bytes are present.
2. Magic equals `qoif`.
3. Width is nonzero.
4. Height is nonzero.
5. Channels are 3 or 4.
6. Colorspace is 0 or 1.
7. The C compatibility pixel-limit guard rejects
   `height >= 400_000_000 / width`.
8. `width * height` must succeed using checked arithmetic.

## Stream Trailer

Complete QOI streams end with the eight-byte marker
`00 00 00 00 00 00 00 01`.

Decoding a complete stream requires at least the 14-byte header and the
eight-byte trailer. The trailer bytes must match exactly.

## Pixel Index

Pixels are hashed into the 64-entry index table with:

```text
(r * 3 + g * 5 + b * 7 + a * 11) % 64
```

The decoder starts with an opaque black previous pixel `(0, 0, 0, 255)` and a
zero-initialized 64-entry pixel index.

## Decoding

When no output channel count is requested, the decoder uses the channel count
declared in the QOI header. When RGB output is requested from an RGBA stream,
alpha is discarded. When RGBA output is requested from an RGB stream, alpha is
preserved from the current decoder pixel state, which starts at `255`.

Implemented chunks:

- `QOI_OP_RGB` (`0xfe`) reads `r`, `g`, and `b`; alpha is unchanged.
- `QOI_OP_RGBA` (`0xff`) reads `r`, `g`, `b`, and `a`.
- `QOI_OP_INDEX` (`00xxxxxx`) loads the pixel from the 64-entry index.
- `QOI_OP_DIFF` (`01xxxxxx`) stores two-bit deltas for `r`, `g`, and `b` with
  a bias of `2`; alpha is unchanged.
- `QOI_OP_LUMA` (`10xxxxxx`) stores a six-bit green delta with a bias of `32`,
  followed by four-bit red-versus-green and blue-versus-green deltas with a
  bias of `8`; alpha is unchanged.
- `QOI_OP_RUN` (`11xxxxxx`) repeats the previous pixel for `(value & 0x3f) + 1`
  total emitted pixels, including the pixel emitted when the chunk is consumed.

`QOI_OP_RGB` and `QOI_OP_RGBA` are matched before applying the two-bit operation
mask.

DIFF computes:

```text
r = previous.r + (((byte >> 4) & 0x03) - 2)
g = previous.g + (((byte >> 2) & 0x03) - 2)
b = previous.b + ((byte & 0x03) - 2)
```

LUMA computes:

```text
dg = (first & 0x3f) - 32
dr_dg = ((second >> 4) & 0x0f) - 8
db_dg = (second & 0x0f) - 8

r = previous.r + dg + dr_dg
g = previous.g + dg
b = previous.b + dg + db_dg
```

DIFF and LUMA arithmetic wraps in `u8`.

## Decoder completion

A successful strict decode requires:

1. Exactly `width * height` pixels are emitted.
2. No run repetitions remain after the final pixel.
3. All chunk bytes before the end marker have been consumed.
4. The exact eight-byte end marker is present.

The final three conditions are stricter than the pinned C decoder for malformed
streams and are intentional safety validations.

## Encoding

Encoding validates dimensions with the same C compatibility pixel-limit guard
used by header parsing, and the raw pixel buffer length must be exactly
`width * height * channels`.

The encoder writes the 14-byte header, then processes pixels in source order
with the initial previous pixel `(0, 0, 0, 255)` and a zero-initialized index.
Its chunk decision order matches the pinned C reference:

1. Accumulate equal consecutive pixels into a RUN.
2. Flush a pending RUN before encoding a changed pixel.
3. Use INDEX when the current hash slot already contains the pixel.
4. Otherwise update the index slot with the current pixel.
5. If alpha is unchanged, try DIFF, then LUMA, then RGB.
6. If alpha changed, use RGBA.
7. Append the exact eight-byte end marker.

Signed channel differences use wrapping `u8` subtraction interpreted as `i8`,
matching the pinned C reference behavior.
