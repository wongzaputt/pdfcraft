# Build & Release Guidelines for pdfcraft

Whenever asked to build the project or create a Windows release, always follow these exact steps:

1. **Sync `craft-fonts`:**
   - Ensure a local checkout of `craft-fonts` exists at `../craft-fonts` (relative to the project root).
   - If `../craft-fonts` does not exist, inspect `.github/workflows/` for the upstream `craft-fonts` repository URL and clone it to `../craft-fonts`. If it already exists, pull the latest changes (`git -C ../craft-fonts pull`).
   - *(Skip `craft-i18n` unless explicitly requested.)*

2. **Set Environment Variable & Compile Release Binary:**
   - Set the `CRAFT_FONTS_DIR` environment variable to the resolved absolute path of `../craft-fonts` before compiling (e.g., `$env:CRAFT_FONTS_DIR = (Resolve-Path "../craft-fonts").Path` in PowerShell).
   - Compile the Rust project in Release mode (`cargo build --release --target x86_64-pc-windows-msvc`), outputting to `target/release/`.

3. **Package Artifacts:**
   - Follow upstream packaging scripts/configs (e.g., `cargo-wix`, Tauri, or custom packaging scripts) to generate both the `.msi` installer and the portable `.zip` archive[cite: 3].

4. **Output Directory:**
   - Place all final release artifacts (`.msi` and `.zip`) into `dist/release/`[cite: 3].

5. **Verify:**
   - Confirm that both `.msi` and `.zip` exist in `dist/release/`, verify that `CRAFT_FONTS_DIR` was active during the build, and report their file sizes[cite: 3].