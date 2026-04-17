// build.rs
fn main() {
    // 告诉 Rust 静态库在哪里，并链接它
    println!("cargo:rustc-link-search=native=native");  // native 是存放 libtcp_conn.a 的文件夹
    println!("cargo:rustc-link-lib=static=tcp_conn");    // 链接 libtcp_conn.a

    // 链接系统动态库 libev
    // println!("cargo:rustc-link-lib=ev");
    println!("cargo:rustc-link-lib=pthread");
}
