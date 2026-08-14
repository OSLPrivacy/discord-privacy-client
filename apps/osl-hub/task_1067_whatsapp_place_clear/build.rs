fn main() {
    println!("cargo:rustc-check-cfg=cfg(task1067_direct)");
    println!("cargo:rustc-cfg=task1067_direct");
}
