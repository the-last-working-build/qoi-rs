#![no_main]

use std::{
    ffi::{c_int, c_uchar, c_uint, c_void},
    ptr, slice,
};

use libfuzzer_sys::fuzz_target;
use qoi_rs::{Channels, ColorSpace, ImageDesc, decode, encode};

unsafe extern "C" {
    fn qoi_ref_encode(
        pixels: *const c_uchar,
        width: c_uint,
        height: c_uint,
        channels: c_uchar,
        colorspace: c_uchar,
        out: *mut *mut c_uchar,
        out_len: *mut c_int,
    ) -> c_int;

    fn qoi_ref_decode(
        qoi: *const c_uchar,
        qoi_len: c_int,
        requested_channels: c_int,
        out: *mut *mut c_uchar,
        out_len: *mut c_int,
    ) -> c_int;

    fn qoi_ref_free(ptr: *mut c_void);
}

fuzz_target!(|data: &[u8]| {
    let image = Image::from_input(data);

    let rust_encoded = encode(&image.pixels, image.desc).expect("Rust encode should succeed");
    let c_encoded = c_encode(&image.pixels, image.desc).expect("C encode should succeed");

    assert_eq!(rust_encoded, c_encoded);

    let rust_decoded_from_c = decode(&c_encoded, None).expect("Rust decode of C output succeeds");
    assert_eq!(rust_decoded_from_c.pixels, image.pixels);

    let c_decoded_from_rust = c_decode(&rust_encoded).expect("C decode of Rust output succeeds");
    assert_eq!(c_decoded_from_rust, image.pixels);

    let rust_decoded_from_rust =
        decode(&rust_encoded, None).expect("Rust decode of Rust output succeeds");
    assert_eq!(rust_decoded_from_rust.pixels, image.pixels);
});

struct Image {
    desc: ImageDesc,
    pixels: Vec<u8>,
}

impl Image {
    fn from_input(data: &[u8]) -> Self {
        let width = u32::from(data.first().copied().unwrap_or(0) % 64) + 1;
        let height = u32::from(data.get(1).copied().unwrap_or(0) % 64) + 1;
        let channels = if data.get(2).copied().unwrap_or(0) & 1 == 0 {
            Channels::Rgb
        } else {
            Channels::Rgba
        };
        let colorspace = if data.get(3).copied().unwrap_or(0) & 1 == 0 {
            ColorSpace::SrgbWithLinearAlpha
        } else {
            ColorSpace::AllLinear
        };

        let len = width as usize * height as usize * channels.count();
        let source = data.get(4..).unwrap_or(&[]);
        let mut pixels = Vec::with_capacity(len);

        if source.is_empty() {
            pixels.resize(len, 0);
        } else {
            for index in 0..len {
                pixels.push(source[index % source.len()]);
            }
        }

        Self {
            desc: ImageDesc {
                width,
                height,
                channels,
                colorspace,
            },
            pixels,
        }
    }
}

fn c_encode(pixels: &[u8], desc: ImageDesc) -> Option<Vec<u8>> {
    let mut out = ptr::null_mut();
    let mut out_len = 0;
    let ok = unsafe {
        qoi_ref_encode(
            pixels.as_ptr(),
            desc.width,
            desc.height,
            desc.channels as u8,
            desc.colorspace as u8,
            &mut out,
            &mut out_len,
        )
    };

    if ok == 0 || out.is_null() || out_len < 0 {
        return None;
    }

    let encoded = unsafe { slice::from_raw_parts(out, out_len as usize).to_vec() };
    unsafe {
        qoi_ref_free(out.cast());
    }

    Some(encoded)
}

fn c_decode(qoi: &[u8]) -> Option<Vec<u8>> {
    if qoi.len() > c_int::MAX as usize {
        return None;
    }

    let mut out = ptr::null_mut();
    let mut out_len = 0;
    let ok = unsafe { qoi_ref_decode(qoi.as_ptr(), qoi.len() as c_int, 0, &mut out, &mut out_len) };

    if ok == 0 || out.is_null() || out_len < 0 {
        return None;
    }

    let pixels = unsafe { slice::from_raw_parts(out, out_len as usize).to_vec() };
    unsafe {
        qoi_ref_free(out.cast());
    }

    Some(pixels)
}
