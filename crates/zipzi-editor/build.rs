fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=ZIPZI_UPDATE_URL");
    println!("cargo:rerun-if-env-changed=ZIPZI_UPDATE_PUBLIC_KEY");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/windows/editor.rc");
        println!("cargo:rerun-if-changed=assets/app-icons/zipzi-editor.ico");
        embed_resource::compile("assets/windows/editor.rc", embed_resource::NONE)
            .manifest_required()
            .expect("embed Editor Windows resources");
    }
}
