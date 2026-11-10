fn main() {
    // English is the source language; translations live in
    // lang/<code>/LC_MESSAGES/kyxen-gui.po and are compiled into the binary.
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("compile ui");
    println!("cargo:rerun-if-changed=lang");
}
