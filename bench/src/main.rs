use std::{
    cmp::Ordering,
    ffi::{c_int, c_uchar, c_uint, c_void},
    hint::black_box,
    ptr, slice,
    time::{Duration, Instant},
};

use qoi_rs::{Channels, ColorSpace, ImageDesc, decode, encode};

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 1024;
const WARMUP_ITERATIONS: usize = 10;
const MEASURED_ITERATIONS: usize = 100;

unsafe extern "C" {
    fn qoi_bench_encode(
        pixels: *const c_uchar,
        width: c_uint,
        height: c_uint,
        channels: c_uchar,
        colorspace: c_uchar,
        out: *mut *mut c_uchar,
        out_len: *mut c_int,
    ) -> c_int;

    fn qoi_bench_decode(
        qoi: *const c_uchar,
        qoi_len: c_int,
        requested_channels: c_int,
        out: *mut *mut c_uchar,
        out_len: *mut c_int,
    ) -> c_int;

    fn qoi_bench_free(ptr: *mut c_void);
}

fn main() {
    let mut args = std::env::args();
    let program = args.next().unwrap_or_else(|| "qoi-rs-bench".to_owned());

    match args.next().as_deref() {
        None => run_benchmark(),
        Some("profile") => run_profile(&program, args),
        Some(_) => profile_usage(&program),
    }
}

fn run_benchmark() {
    let fixtures = [
        Fixture::flat_rgba(),
        Fixture::gradient_rgb(),
        Fixture::noise_rgba(),
    ];

    println!("# QOI codec benchmark results");
    println!();
    print_environment();
    println!("warmup iterations: {WARMUP_ITERATIONS}");
    println!("measured iterations: {MEASURED_ITERATIONS}");
    println!();
    println!(
        "{:<13} {:<9} {:>14} {:>14} {:>9} {:>14} {:>14} {:>14}",
        "fixture",
        "operation",
        "C median",
        "Rust median",
        "Rust/C",
        "encoded bytes",
        "C MiB/s",
        "Rust MiB/s"
    );

    let mut checksum = 0u64;

    for fixture in &fixtures {
        let c_encoded = c_encode(&fixture.pixels, fixture.desc).expect("C encode should succeed");
        let rust_encoded =
            encode(&fixture.pixels, fixture.desc).expect("Rust encode should succeed");
        assert_eq!(
            rust_encoded, c_encoded,
            "{} encoded bytes differ",
            fixture.name
        );

        let c_decoded = c_decode(&c_encoded).expect("C decode should succeed");
        assert_eq!(
            c_decoded, fixture.pixels,
            "{} C decode differs",
            fixture.name
        );

        let rust_decoded = decode(&c_encoded, None).expect("Rust decode should succeed");
        assert_eq!(
            rust_decoded.pixels, fixture.pixels,
            "{} Rust decode differs",
            fixture.name
        );

        checksum ^= observe(&c_encoded);
        checksum ^= observe(&c_decoded);
        checksum ^= observe(&rust_decoded.pixels);

        let encode_result = measure_pair(
            || c_encode_observed(&fixture.pixels, fixture.desc),
            || {
                let encoded = encode(&fixture.pixels, fixture.desc).expect("Rust encode");
                observe(&encoded)
            },
        );
        checksum ^= encode_result.c.checksum;
        checksum ^= encode_result.rust.checksum;

        print_row(
            fixture,
            "encode",
            encode_result.c.median,
            encode_result.rust.median,
            c_encoded.len(),
        );

        let decode_result = measure_pair(
            || c_decode_observed(&c_encoded),
            || {
                let decoded = decode(&c_encoded, None).expect("Rust decode");
                observe(&decoded.pixels)
            },
        );
        checksum ^= decode_result.c.checksum;
        checksum ^= decode_result.rust.checksum;

        print_row(
            fixture,
            "decode",
            decode_result.c.median,
            decode_result.rust.median,
            c_encoded.len(),
        );
    }

    println!();
    println!("checksum: {checksum:016x}");
}

#[derive(Clone, Copy)]
enum Implementation {
    Rust,
    C,
}

impl Implementation {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "rust" => Some(Self::Rust),
            "c" => Some(Self::C),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum Operation {
    Encode,
    Decode,
}

impl Operation {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "encode" => Some(Self::Encode),
            "decode" => Some(Self::Decode),
            _ => None,
        }
    }
}

fn run_profile(program: &str, mut args: impl Iterator<Item = String>) {
    let implementation_name = args.next().unwrap_or_else(|| profile_usage(program));
    let operation_name = args.next().unwrap_or_else(|| profile_usage(program));
    let fixture_name = args.next().unwrap_or_else(|| profile_usage(program));
    let iterations_text = args.next().unwrap_or_else(|| profile_usage(program));

    if args.next().is_some() {
        profile_usage(program);
    }

    let implementation =
        Implementation::parse(&implementation_name).unwrap_or_else(|| profile_usage(program));
    let operation = Operation::parse(&operation_name).unwrap_or_else(|| profile_usage(program));
    let fixture = Fixture::named(&fixture_name).unwrap_or_else(|| profile_usage(program));
    let iterations = iterations_text
        .parse::<usize>()
        .ok()
        .filter(|iterations| *iterations > 0)
        .unwrap_or_else(|| profile_usage(program));

    let c_encoded = c_encode(&fixture.pixels, fixture.desc).expect("C encode should succeed");
    let rust_encoded = encode(&fixture.pixels, fixture.desc).expect("Rust encode should succeed");
    assert_eq!(rust_encoded, c_encoded, "encoded bytes differ");

    let c_decoded = c_decode(&c_encoded).expect("C decode should succeed");
    let rust_decoded = decode(&c_encoded, None).expect("Rust decode should succeed");
    assert_eq!(c_decoded, fixture.pixels, "C decode differs");
    assert_eq!(rust_decoded.pixels, fixture.pixels, "Rust decode differs");

    let mut checksum = 0u64;

    for _ in 0..iterations {
        let token = match (implementation, operation) {
            (Implementation::Rust, Operation::Encode) => {
                let encoded = encode(&fixture.pixels, fixture.desc).expect("Rust encode");
                observe(&encoded)
            }
            (Implementation::C, Operation::Encode) => {
                c_encode_observed(&fixture.pixels, fixture.desc)
            }
            (Implementation::Rust, Operation::Decode) => {
                let decoded = decode(&c_encoded, None).expect("Rust decode");
                observe(&decoded.pixels)
            }
            (Implementation::C, Operation::Decode) => c_decode_observed(&c_encoded),
        };

        checksum = checksum
            .wrapping_mul(0x9e37_79b1_85eb_ca87)
            .wrapping_add(black_box(token));
    }

    println!("profile: {implementation_name} {operation_name} {fixture_name} {iterations}");
    println!("checksum: {checksum:016x}");
}

fn profile_usage(program: &str) -> ! {
    eprintln!(
        "usage: {program} profile <rust|c> <encode|decode> \
         <flat-rgba|gradient-rgb|noise-rgba> <iterations>"
    );
    std::process::exit(2);
}

struct Fixture {
    name: &'static str,
    desc: ImageDesc,
    pixels: Vec<u8>,
}

impl Fixture {
    fn named(name: &str) -> Option<Self> {
        match name {
            "flat-rgba" => Some(Self::flat_rgba()),
            "gradient-rgb" => Some(Self::gradient_rgb()),
            "noise-rgba" => Some(Self::noise_rgba()),
            _ => None,
        }
    }

    fn flat_rgba() -> Self {
        let mut pixels = Vec::with_capacity(pixel_len(Channels::Rgba));

        for _ in 0..pixel_count() {
            pixels.extend_from_slice(&[24, 80, 160, 255]);
        }

        Self::new(
            "flat-rgba",
            Channels::Rgba,
            ColorSpace::SrgbWithLinearAlpha,
            pixels,
        )
    }

    fn gradient_rgb() -> Self {
        let mut pixels = Vec::with_capacity(pixel_len(Channels::Rgb));

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                pixels.push(x.wrapping_add(y) as u8);
                pixels.push(x.wrapping_add(y.wrapping_mul(2)) as u8);
                pixels.push(x.wrapping_mul(2).wrapping_add(y) as u8);
            }
        }

        Self::new("gradient-rgb", Channels::Rgb, ColorSpace::AllLinear, pixels)
    }

    fn noise_rgba() -> Self {
        let mut pixels = Vec::with_capacity(pixel_len(Channels::Rgba));
        let mut rng = XorShift64Star::new(0x515f4f495f525553);

        for _ in 0..pixel_len(Channels::Rgba) {
            pixels.push(rng.next_u8());
        }

        Self::new(
            "noise-rgba",
            Channels::Rgba,
            ColorSpace::SrgbWithLinearAlpha,
            pixels,
        )
    }

    fn new(
        name: &'static str,
        channels: Channels,
        colorspace: ColorSpace,
        pixels: Vec<u8>,
    ) -> Self {
        assert_eq!(pixels.len(), pixel_len(channels));

        Self {
            name,
            desc: ImageDesc {
                width: WIDTH,
                height: HEIGHT,
                channels,
                colorspace,
            },
            pixels,
        }
    }

    fn raw_mib(&self) -> f64 {
        self.pixels.len() as f64 / 1024.0 / 1024.0
    }
}

#[derive(Clone, Copy)]
struct Measurement {
    median: Duration,
    checksum: u64,
}

struct PairedMeasurement {
    c: Measurement,
    rust: Measurement,
}

fn measure_pair(
    mut c_operation: impl FnMut() -> u64,
    mut rust_operation: impl FnMut() -> u64,
) -> PairedMeasurement {
    let mut c_checksum = 0u64;
    let mut rust_checksum = 0u64;

    for iteration in 0..WARMUP_ITERATIONS {
        run_pair(
            iteration,
            &mut c_operation,
            &mut rust_operation,
            &mut c_checksum,
            &mut rust_checksum,
            None,
        );
    }

    let mut c_samples = Vec::with_capacity(MEASURED_ITERATIONS);
    let mut rust_samples = Vec::with_capacity(MEASURED_ITERATIONS);

    for iteration in 0..MEASURED_ITERATIONS {
        run_pair(
            iteration,
            &mut c_operation,
            &mut rust_operation,
            &mut c_checksum,
            &mut rust_checksum,
            Some((&mut c_samples, &mut rust_samples)),
        );
    }

    c_samples.sort_by(compare_duration);
    rust_samples.sort_by(compare_duration);

    PairedMeasurement {
        c: Measurement {
            median: c_samples[c_samples.len() / 2],
            checksum: c_checksum,
        },
        rust: Measurement {
            median: rust_samples[rust_samples.len() / 2],
            checksum: rust_checksum,
        },
    }
}

fn run_pair(
    iteration: usize,
    c_operation: &mut impl FnMut() -> u64,
    rust_operation: &mut impl FnMut() -> u64,
    c_checksum: &mut u64,
    rust_checksum: &mut u64,
    samples: Option<(&mut Vec<Duration>, &mut Vec<Duration>)>,
) {
    match samples {
        Some((c_samples, rust_samples)) => {
            if iteration.is_multiple_of(2) {
                run_measured(c_operation, c_checksum, c_samples);
                run_measured(rust_operation, rust_checksum, rust_samples);
            } else {
                run_measured(rust_operation, rust_checksum, rust_samples);
                run_measured(c_operation, c_checksum, c_samples);
            }
        }
        None => {
            if iteration.is_multiple_of(2) {
                *c_checksum ^= black_box(c_operation());
                *rust_checksum ^= black_box(rust_operation());
            } else {
                *rust_checksum ^= black_box(rust_operation());
                *c_checksum ^= black_box(c_operation());
            }
        }
    }
}

fn run_measured(
    operation: &mut impl FnMut() -> u64,
    checksum: &mut u64,
    samples: &mut Vec<Duration>,
) {
    let start = Instant::now();
    *checksum ^= black_box(operation());
    samples.push(start.elapsed());
}

fn c_encode(pixels: &[u8], desc: ImageDesc) -> Option<Vec<u8>> {
    let mut out = ptr::null_mut();
    let mut out_len = 0;
    let ok = unsafe {
        qoi_bench_encode(
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
        qoi_bench_free(out.cast());
    }

    Some(encoded)
}

fn c_decode(qoi: &[u8]) -> Option<Vec<u8>> {
    if qoi.len() > c_int::MAX as usize {
        return None;
    }

    let mut out = ptr::null_mut();
    let mut out_len = 0;
    let ok =
        unsafe { qoi_bench_decode(qoi.as_ptr(), qoi.len() as c_int, 0, &mut out, &mut out_len) };

    if ok == 0 || out.is_null() || out_len < 0 {
        return None;
    }

    let decoded = unsafe { slice::from_raw_parts(out, out_len as usize).to_vec() };
    unsafe {
        qoi_bench_free(out.cast());
    }

    Some(decoded)
}

fn c_encode_observed(pixels: &[u8], desc: ImageDesc) -> u64 {
    let mut out = ptr::null_mut();
    let mut out_len = 0;

    let ok = unsafe {
        qoi_bench_encode(
            pixels.as_ptr(),
            desc.width,
            desc.height,
            desc.channels as u8,
            desc.colorspace as u8,
            &mut out,
            &mut out_len,
        )
    };

    assert!(ok != 0 && !out.is_null() && out_len >= 0);

    let bytes = unsafe { slice::from_raw_parts(out, out_len as usize) };
    let token = observe(bytes);

    unsafe {
        qoi_bench_free(out.cast());
    }

    token
}

fn c_decode_observed(qoi: &[u8]) -> u64 {
    assert!(qoi.len() <= c_int::MAX as usize);

    let mut out = ptr::null_mut();
    let mut out_len = 0;

    let ok =
        unsafe { qoi_bench_decode(qoi.as_ptr(), qoi.len() as c_int, 0, &mut out, &mut out_len) };

    assert!(ok != 0 && !out.is_null() && out_len >= 0);

    let bytes = unsafe { slice::from_raw_parts(out, out_len as usize) };
    let token = observe(bytes);

    unsafe {
        qoi_bench_free(out.cast());
    }

    token
}

fn observe(bytes: &[u8]) -> u64 {
    let len = bytes.len();
    let first = bytes.first().copied().unwrap_or(0);
    let middle = bytes.get(len / 2).copied().unwrap_or(0);
    let last = bytes.last().copied().unwrap_or(0);

    black_box((bytes.as_ptr(), len, first, middle, last));

    (len as u64) ^ (u64::from(first) << 8) ^ (u64::from(middle) << 16) ^ (u64::from(last) << 24)
}

fn print_row(
    fixture: &Fixture,
    operation: &str,
    c_median: Duration,
    rust_median: Duration,
    encoded_len: usize,
) {
    let c_ns = c_median.as_nanos();
    let rust_ns = rust_median.as_nanos();
    let ratio = rust_median.as_secs_f64() / c_median.as_secs_f64();
    let c_throughput = throughput(fixture.raw_mib(), c_median);
    let rust_throughput = throughput(fixture.raw_mib(), rust_median);

    println!(
        "{:<13} {:<9} {:>14} {:>14} {:>9.2} {:>14} {:>14.2} {:>14.2}",
        fixture.name,
        operation,
        format!("{c_ns} ns"),
        format!("{rust_ns} ns"),
        ratio,
        encoded_len,
        c_throughput,
        rust_throughput
    );
}

fn print_environment() {
    println!(
        "commit: {}",
        option_env!("BENCH_COMMIT").unwrap_or("unknown")
    );
    println!(
        "branch: {}",
        option_env!("BENCH_BRANCH").unwrap_or("unknown")
    );
    println!(
        "tree state: {}",
        option_env!("BENCH_TREE_STATE").unwrap_or("unknown")
    );
    println!("operating system: {}", std::env::consts::OS);
    println!("architecture: {}", std::env::consts::ARCH);
    println!("cpu: {}", cpu_name());
    println!("rust compiler: {}", rustc_version());
    println!("c compiler: {}", cc_version());
    println!("rust optimization: release");
    println!("c optimization: -O3 -DNDEBUG via cc build script");
    println!();
}

fn rustc_version() -> &'static str {
    option_env!("BENCH_RUSTC").unwrap_or("unknown")
}

fn cc_version() -> &'static str {
    option_env!("BENCH_CC").unwrap_or("cc")
}

fn cpu_name() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|cpuinfo| {
            cpuinfo.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|rest| rest.split_once(':'))
                    .map(|(_, name)| name.trim().to_owned())
            })
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

fn throughput(raw_mib: f64, duration: Duration) -> f64 {
    raw_mib / duration.as_secs_f64()
}

fn compare_duration(left: &Duration, right: &Duration) -> Ordering {
    left.as_nanos()
        .cmp(&right.as_nanos())
        .then_with(|| left.subsec_nanos().cmp(&right.subsec_nanos()))
}

fn pixel_count() -> usize {
    WIDTH as usize * HEIGHT as usize
}

fn pixel_len(channels: Channels) -> usize {
    pixel_count() * channels.count()
}

struct XorShift64Star {
    state: u64,
}

impl XorShift64Star {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u8(&mut self) -> u8 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;

        (x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 56) as u8
    }
}
