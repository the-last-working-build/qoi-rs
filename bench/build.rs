fn main() {
    println!("cargo:rerun-if-changed=reference.c");
    println!("cargo:rerun-if-changed=../reference/qoi/qoi.h");
    println!(
        "cargo:rustc-env=BENCH_COMMIT={}",
        output("git", &["rev-parse", "--short", "HEAD"])
    );
    println!(
        "cargo:rustc-env=BENCH_BRANCH={}",
        output("git", &["branch", "--show-current"])
    );
    println!("cargo:rustc-env=BENCH_TREE_STATE={}", tree_state());
    println!(
        "cargo:rustc-env=BENCH_RUSTC={}",
        output("rustc", &["--version"])
    );
    println!("cargo:rustc-env=BENCH_CC={}", output("cc", &["--version"]));

    cc::Build::new()
        .file("reference.c")
        .flag_if_supported("-std=c99")
        .opt_level(3)
        .define("NDEBUG", None)
        .compile("qoi_bench_ref");
}

fn output(program: &str, args: &[&str]) -> String {
    std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("unknown")
                .to_owned()
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

fn tree_state() -> &'static str {
    if status("git", &["diff", "--quiet"]) && status("git", &["diff", "--cached", "--quiet"]) {
        "clean"
    } else {
        "dirty working tree at benchmark build time"
    }
}

fn status(program: &str, args: &[&str]) -> bool {
    std::process::Command::new(program)
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
