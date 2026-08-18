fn main() {
    let attributes = tauri_build::Attributes::new();

    // Ship a custom Windows manifest so the launcher runs elevated. G-Earth
    // needs administrator rights (it edits the hosts file to redirect the game
    // host at its proxy) and a spawned child inherits our token, so the
    // elevation has to happen here rather than per-child.
    #[cfg(windows)]
    let attributes = attributes.windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
    );

    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
