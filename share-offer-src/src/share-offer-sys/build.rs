// build.rs
use std::env;
use std::path::PathBuf;
fn main() {
    // 头文件路径，根据你的实际路径调整
    let header_path = "wrapper.h";

    let bindings = bindgen::Builder::default()
        .header(header_path)
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    // 告诉 Rust 静态库在哪里，并链接它
    println!("cargo:rustc-link-search=native=native"); // native 是存放 libtcp_conn.a 的文件夹
    println!("cargo:rustc-link-lib=static=tcp_conn"); // 链接 libtcp_conn.a
    println!("cargo:rerun-if-changed=wrapper.h");

    // 链接系统动态库 libev
    // println!("cargo:rustc-link-lib=ev");
    println!("cargo:rustc-link-lib=pthread");
    println!("cargo:rustc-link-lib=stdc++");
}
