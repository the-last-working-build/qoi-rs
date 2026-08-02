fn main() {
    cc::Build::new()
        .file("reference.c")
        .flag_if_supported("-std=c99")
        .compile("qoi_ref");

    println!("cargo:rerun-if-changed=reference.c");
    println!("cargo:rerun-if-changed=../reference/qoi/qoi.h");
}
