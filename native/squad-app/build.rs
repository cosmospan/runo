//! Embeds the app icon into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/squadcode.ico");
    println!("cargo:rerun-if-changed=squadcode.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("squadcode.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("could not embed the Windows icon");
    }
}
