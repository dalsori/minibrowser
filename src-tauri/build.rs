fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "open",
            "home",
            "settings",
            "reload",
            "back",
            "forward",
            "ws",
            "wsnew",
            "setadblock",
            "setengine",
            "getstate",
        ])),
    )
    .unwrap();
}