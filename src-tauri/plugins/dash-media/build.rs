const COMMANDS: &[&str] = &["update", "set_lang"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
