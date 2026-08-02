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

## Pixel Index

Pixels are hashed into the 64-entry index table with:

```text
(r * 3 + g * 5 + b * 7 + a * 11) % 64
```
