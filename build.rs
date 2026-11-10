fn main() {
    println!("cargo:rerun-if-changed=assets/xerahs.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/xerahs.ico");
        res.compile().expect("embed icon resource");
    }
}
