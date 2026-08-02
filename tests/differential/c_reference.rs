use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use qoi_rs::{Channels, ColorSpace, ImageDesc, decode, encode};

#[derive(Debug)]
struct Case {
    name: &'static str,
    width: u32,
    height: u32,
    channels: Channels,
    colorspace: u8,
    pixels: Vec<u8>,
}

#[test]
fn c_and_rust_codecs_agree_on_deterministic_fixtures() {
    let exe = compile_qoi_ref();
    let work_dir = work_dir();

    fs::create_dir_all(&work_dir).expect("create differential work directory");

    for case in cases() {
        assert_eq!(
            case.pixels.len(),
            case.width as usize * case.height as usize * case.channels.count(),
            "{} has invalid fixture length",
            case.name
        );

        let raw_path = work_dir.join(format!("{}.raw", case.name));
        let c_qoi_path = work_dir.join(format!("{}.c.qoi", case.name));
        let rust_qoi_path = work_dir.join(format!("{}.rust.qoi", case.name));
        let c_decoded_path = work_dir.join(format!("{}.c-decoded.raw", case.name));

        fs::write(&raw_path, &case.pixels).expect("write raw fixture");

        run(
            &exe,
            &[
                "encode",
                &case.width.to_string(),
                &case.height.to_string(),
                &(case.channels.count()).to_string(),
                &case.colorspace.to_string(),
                raw_path.to_str().expect("utf-8 raw path"),
                c_qoi_path.to_str().expect("utf-8 qoi path"),
            ],
        );

        let c_qoi = fs::read(&c_qoi_path).expect("read C-encoded qoi file");
        let rust_qoi = encode(&case.pixels, case.desc()).expect("Rust encode should succeed");

        assert_eq!(rust_qoi, c_qoi, "{}", case.name);

        fs::write(&rust_qoi_path, &rust_qoi).expect("write Rust-encoded qoi file");

        let decoded = decode(&c_qoi, None).expect("Rust decode should succeed");

        assert_eq!(decoded.pixels, case.pixels, "{}", case.name);
        assert_eq!(decoded.desc.width, case.width, "{}", case.name);
        assert_eq!(decoded.desc.height, case.height, "{}", case.name);
        assert_eq!(decoded.desc.channels, case.channels, "{}", case.name);
        assert_eq!(
            decoded.desc.colorspace as u8, case.colorspace,
            "{}",
            case.name
        );
        assert_eq!(decoded.output_channels, case.channels, "{}", case.name);

        run(
            &exe,
            &[
                "decode",
                "0",
                rust_qoi_path.to_str().expect("utf-8 qoi path"),
                c_decoded_path.to_str().expect("utf-8 decoded path"),
            ],
        );

        assert_eq!(
            fs::read(&c_decoded_path).expect("read C-decoded raw file"),
            case.pixels,
            "{}",
            case.name
        );

        let rust_decoded_from_rust_qoi =
            decode(&rust_qoi, None).expect("Rust decode of Rust encode should succeed");

        assert_eq!(
            rust_decoded_from_rust_qoi.pixels, case.pixels,
            "{}",
            case.name
        );

        for requested in [Channels::Rgb, Channels::Rgba] {
            let c_decoded_path =
                work_dir.join(format!("{}.c-decoded-{}.raw", case.name, requested.count()));

            run(
                &exe,
                &[
                    "decode",
                    &requested.count().to_string(),
                    c_qoi_path.to_str().expect("utf-8 qoi path"),
                    c_decoded_path.to_str().expect("utf-8 decoded path"),
                ],
            );

            let c_pixels = fs::read(&c_decoded_path).expect("read C-decoded pixels");
            let rust_image = decode(&c_qoi, Some(requested))
                .expect("Rust requested-channel decode should succeed");

            assert_eq!(
                rust_image.pixels,
                c_pixels,
                "{} requested {} channels",
                case.name,
                requested.count()
            );
        }
    }
}

impl Case {
    fn desc(&self) -> ImageDesc {
        ImageDesc {
            width: self.width,
            height: self.height,
            channels: self.channels,
            colorspace: ColorSpace::try_from(self.colorspace).expect("fixture colorspace"),
        }
    }
}

fn compile_qoi_ref() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = manifest_dir.join("tools/c-reference/qoi_ref.c");
    let exe = work_dir().join(if cfg!(windows) {
        "qoi-ref.exe"
    } else {
        "qoi-ref"
    });

    fs::create_dir_all(exe.parent().expect("executable parent")).expect("create build directory");

    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());

    let output = Command::new(compiler)
        .arg("-std=c99")
        .arg("-O2")
        .arg(&source)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run C compiler");

    assert!(
        output.status.success(),
        "failed to compile qoi_ref.c\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    exe
}

fn run(exe: &Path, args: &[&str]) {
    let output = Command::new(exe).args(args).output().expect("run qoi-ref");

    assert!(
        output.status.success(),
        "qoi-ref {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn work_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/differential")
        .join(format!("c-reference-{}", std::process::id()))
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "rgb_one_by_one_edges",
            width: 1,
            height: 1,
            channels: Channels::Rgb,
            colorspace: 0,
            pixels: vec![0, 255, 1],
        },
        Case {
            name: "rgba_one_by_one_alpha",
            width: 1,
            height: 1,
            channels: Channels::Rgba,
            colorspace: 1,
            pixels: vec![255, 0, 255, 0],
        },
        Case {
            name: "rgb_repeated_pixels",
            width: 8,
            height: 1,
            channels: Channels::Rgb,
            colorspace: 0,
            pixels: repeat_rgb(&[[7, 8, 9]], 8),
        },
        Case {
            name: "rgb_nonconsecutive_index_reuse",
            width: 3,
            height: 1,
            channels: Channels::Rgb,
            colorspace: 0,
            pixels: rgb(&[[1, 2, 3], [10, 20, 30], [1, 2, 3]]),
        },
        Case {
            name: "rgb_small_channel_deltas",
            width: 6,
            height: 1,
            channels: Channels::Rgb,
            colorspace: 0,
            pixels: rgb(&[
                [10, 10, 10],
                [11, 10, 9],
                [12, 11, 9],
                [12, 13, 10],
                [13, 14, 12],
                [14, 14, 13],
            ]),
        },
        Case {
            name: "rgb_large_channel_deltas_and_edges",
            width: 6,
            height: 1,
            channels: Channels::Rgb,
            colorspace: 0,
            pixels: rgb(&[
                [0, 0, 0],
                [250, 3, 252],
                [4, 255, 5],
                [255, 255, 255],
                [1, 254, 0],
                [128, 64, 192],
            ]),
        },
        Case {
            name: "rgba_alpha_changes",
            width: 5,
            height: 1,
            channels: Channels::Rgba,
            colorspace: 0,
            pixels: rgba(&[
                [1, 2, 3, 255],
                [1, 2, 3, 254],
                [10, 20, 30, 0],
                [10, 20, 30, 255],
                [0, 255, 0, 128],
            ]),
        },
        Case {
            name: "rgb_multi_pixel_mixed",
            width: 4,
            height: 2,
            channels: Channels::Rgb,
            colorspace: 1,
            pixels: rgb(&[
                [0, 0, 0],
                [0, 0, 0],
                [1, 1, 1],
                [2, 3, 2],
                [200, 10, 240],
                [200, 10, 240],
                [199, 9, 241],
                [255, 0, 255],
            ]),
        },
    ]
}

fn rgb(pixels: &[[u8; 3]]) -> Vec<u8> {
    pixels.iter().flatten().copied().collect()
}

fn rgba(pixels: &[[u8; 4]]) -> Vec<u8> {
    pixels.iter().flatten().copied().collect()
}

fn repeat_rgb(pixels: &[[u8; 3]], times: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() * times * 3);

    for _ in 0..times {
        out.extend(rgb(pixels));
    }

    out
}
