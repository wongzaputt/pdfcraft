# Build & Release Guidelines for pdfcraft

Whenever asked to build the project or create a Windows release, always follow these exact steps:

1. **Sync `craft-i18n`:** Inspect `.github/workflows/` or upstream build scripts to locate the target path for `craft-i18n`. Ensure the latest `craft-i18n` repository/submodules are pulled and placed into the exact directory expected by the build system before compiling.
2. **Compile Release Binary:** Build the Rust project in Release mode (`cargo build --release --target x86_64-pc-windows-msvc`), outputting to `target/release/`.
3. **Package Artifacts:** Follow upstream packaging scripts/configs (e.g., `cargo-wix`, Tauri, or custom scripts) to generate both the `.msi` installer and the portable `.zip` archive (including required assets and `craft-i18n` files).
4. **Output Directory:** Place all final release artifacts (`.msi` and `.zip`) into `dist/release/`.
5. **Verify:** Confirm that both `.msi` and `.zip` exist in `dist/release/` and report their file sizes.