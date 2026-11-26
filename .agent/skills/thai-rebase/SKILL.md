---
name: thai-rebase
description: >-
  Rebase feat/thai-font-support onto upstream/main, preserve all Thai font,
  display, keyboard/typing, Arabic numeral scaling, and session print-memory
  changes during conflict resolution, verify with cargo fmt/check/test, and
  push with --force-with-lease (without building release installers unless
  explicitly requested). Activate when the user says "ทำเรื่อง Rebase เหมือนเดิม"
  or asks to rebase feat/thai-font-support onto upstream/main.
---

# Thai Branch Rebase Workflow (`feat/thai-font-support`)

Follow these steps whenever the user says **"ทำเรื่อง Rebase เหมือนเดิม"** (or asks to rebase `feat/thai-font-support` onto `upstream/main`):

## 1. Check Upstream & Sync Prerequisites
- Ensure working tree is clean (if there are local edits meant to stay, commit them first).
- Fetch `upstream`:
  `git fetch upstream`
- Report how many new commits `upstream/main` has (`git rev-list --count HEAD..upstream/main`) and its latest commit (`git log -1 --oneline upstream/main`).

## 2. Rebase & Resolve Conflicts
- Run `git rebase upstream/main`.
- If conflicts occur, resolve them by **prioritizing `feat/thai-font-support`** so all custom features remain intact:
  1. **Thai font support:** auto-discovery, active Thai font selection, `.ttc` support, shaping, font comboboxes (`font_combo.rs`, `hotbar.rs`, `properties.rs`, `marks_ui.rs`, `comment_props.rs`, `crates/fonts`, `crates/edit`, `crates/forms`).
  2. **Display & keyboard input:** Thai rendering, UI preview, and Thai keyboard typing in UI/dialogs/forms.
  3. **Arabic numeral scaling:** scaling Arabic numerals balanced with Thai text (`crates/fonts`).
  4. **Session print-settings memory:** remembering all print settings within the session only after printing (and discarding on Cancel) in `lib.rs`, `app.rs`, `workflows/print_ui.rs`, `workflows/print_ui/dialog.rs`.
- If `upstream/main` refactored UI frameworks (e.g. `craft_ui` APIs), adapt the wiring to match the new upstream UI APIs without losing any of the above behaviors.
- Continue and complete the rebase (`git rebase --continue`).

## 3. Verify Formatting, Compilation & Tests
Set `CRAFT_FONTS_DIR` to `../craft-fonts` (pull `../craft-fonts` first if needed) and run:
1. `cargo fmt --check` (run `cargo fmt` and commit any style fix if needed)
2. `cargo check -p pdfcraft -p pdfcraft-cli`
3. `cargo test -p pdfcraft-fonts shaped_text`
4. `cargo test -p pdfcraft-ui-egui --lib print_ui`

## 4. Commit & Push (No Release Build by Default)
- Commit any conflict-resolution or formatting fixups if present.
- Push to `origin`:
  `git push origin feat/thai-font-support --force-with-lease`
- **Do NOT build** the Windows `.msi` / `.zip` release unless the user explicitly asks to build.
