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

`QOI_OP_RGB` and `QOI_OP_RGBA` are matched before applying the two-bit operation
mask.
