fn main() {
    println!("cargo:rustc-check-cfg=cfg(task1066_direct)");
    println!("cargo:rustc-cfg=task1066_direct");
}
