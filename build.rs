fn main() {
    // Windows: embed the app icon as resource 1 (GPUI loads that ID for the window class).
    #[cfg(windows)]
    {
        // GPUI renders the whole window from one large function; the default 1 MB main-thread
        // stack overflows in debug builds.
        println!("cargo:rustc-link-arg-bins=/STACK:16777216");
        println!("cargo:rerun-if-changed=assets/opendvp.rc");
        println!("cargo:rerun-if-changed=assets/opendvp.ico");
        embed_resource::compile("assets/opendvp.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
