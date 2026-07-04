# Recon Report — FastFiler

- **total_files (first-party .rs in scope): 39** (crates/fastfiler-domain + crates/fastfiler-gpui; vendor/ and target/ excluded)
- Scope: `crates/fastfiler-domain`, `crates/fastfiler-gpui` (per goal.json)
- Generated: Phase 1 (recon, shallow & wide)

## 1. What it is

FastFiler — a Windows-targeted fast file manager. Core identity (CONTEXT.md / README.md):
1. Vertical tabs + arbitrary split panes (BSP) — the reason the app exists
2. Speed (instant open of huge folders like `C:\Windows\System32`)
3. Deep Windows integration (shell / OLE D&D / default handler)
4. Limited extensibility (user commands + hotkeys + themes only)

History: old Tauri 2 + Solid.js impl removed 2026-05; old floem impl fully ported to GPUI 2026-06 to structurally fix tab/pane memory growth (ADR 0012). GPL-3.0-or-later (links vendored GPL crates zlog/ztracing).

## 2. Crate / layer structure (Cargo workspace)

| Crate | Role | edition | src files | notes |
|---|---|---|---|---|
| `fastfiler-domain` | OS / file-ops logic, GUI-independent (unchanged since floem era) | 2021 | 19 | lib + 18 modules |
| `fastfiler-gpui` | GPUI GUI binary (`fastfiler.exe`) | 2024 | 12 | depends on domain + vendored gpui |
| `vendor/` (EXCLUDED) | GPUI + 18 deps ported from Zed | — | 225 | independent sub-workspace |

Toolchain: Rust 1.95.0 (GPUI requirement). `[patch.crates-io] async-task` git-pinned.

### Domain modules (lib.rs public exports)
ascii_tree, error, events, everything, file_jobs, file_ops, fs, icons, ole_dnd, path_util, search, shell, shell_assoc, templates, undo, user_commands, watcher, win_clipboard.
Dropped (deleted): plugin system (ADR 0003), built-in terminal (ADR 0004), thumbnails/preview (ADR 0005).

### GUI modules
- `main.rs` (108) — entry: keybind registration / session restore / window creation
- `app.rs` (1896) — FastFilerApp root Entity: tabs / BSP tree / resize / session save / tree integration / settings screen
- `pane.rs` (3883) — PaneView (1 pane): listing / selection / operations / modal / context menu / D&D / search / undo / watcher
- `tree.rs` (417) — workspace tree (drive roots / lazy expand / UNC)
- `text_input.rs` (680) — IME-aware single-line text input (ported from gpui example)
- `theme.rs` (645) — themes (presets + themes/*.json) / styles / UI font-size static accessors
- `settings_store.rs` (111) — settings read/write (gpui_settings.json, immediate save)
- `hotkeys.rs` (182) — hotkey definitions & loading (gpui_hotkeys.json)
- `sink.rs` (33) — EventSink → async-channel bridge
- `persist.rs` (129) — crash-safe save (tmp+fsync+rename / .bak)
- `session.rs` (101) — session persistence (JSON via persist)
- `win32_single_instance.rs` (60) — single-instance guard (focus existing window)

## 3. Key dependencies

- domain: serde, serde_json, thiserror, once_cell, parking_lot, lru, image(png), notify, ignore, regex, ureq(json), urlencoding; windows 0.58 (Shell/Ole/Com/Gdi/Clipboard/...), winreg.
- gpui: gpui + gpui_platform (vendored), fastfiler-domain, async-channel, serde, unicode-segmentation, raw-window-handle, windows 0.61, embed-resource (build).

## 4. State model (from ARCHITECTURE.md)

```
Entity<FastFilerApp>            root
├── tabs: Vec<TabState>
│    └── TabState { root: PaneNode (BSP), focused, subs }
│         PaneNode = Leaf(Entity<PaneView>) | Split{ id, dir, ratios, children }
├── tree: Entity<TreeView>
└── PaneView (1 pane = 1 Entity) { cur_path, entries, cursor, selected(BTreeSet), anchor, modal, context_menu, watcher(Arc), sink, jobs(Arc) }
```

Memory lifecycle is the migration's core: closing a tab/pane = drop `Entity<PaneView>` + `Subscription`; `PaneView::drop` cascades release of watcher thread / sink channel / spawn loop. Instrumented by `PANES_ALIVE` (AtomicI64).

Reactivity: single update route `entity.update(cx, |s,cx|{...; cx.notify()})`; `cx.observe` / `cx.subscribe`; `uniform_list` for large rows; background→UI via `EventSink → async-channel → cx.spawn`; debounce via `background_executor().timer`.

Domain boundary: only 2 connection points — (1) synchronous API calls (fs/file_ops/icons/win_clipboard/shell), (2) EventSink (watcher/file_jobs/search emit fs-change / job progress / done).

## 5. Existing documentation (coexist per goal)

doc/: README, USAGE, ARCHITECTURE, BUILD, COMMANDS, COMMANDS-RECIPES, HOTKEYS, THEMES, IDEAS. 12 ADRs (adr/0001–0012), 5 plans (doc/plan/). `.github/` has copilot-instructions + per-crate instructions + lsp.json.

## 6. Build / deploy

`cargo build -p fastfiler-gpui --release` → `target/release/fastfiler.exe`. `build.rs` embeds icon (embed-resource, assets/icon.rc). `scripts/make_icon.ps1`. No CI workflow found (only .github/instructions). release profile: panic=abort, lto, opt-level=s, strip.

## 7. Language mix

100% Rust (first-party). ~13,425 LOC across 39 .rs files. Largest: pane.rs (3883), app.rs (1896), ole_dnd.rs (848), text_input.rs (680), theme.rs (645).

## 8. Template implication

Desktop GUI application = composite (domain library + GPUI desktop app). None of the 4 shipped templates fit directly; library-sdk partially fits the domain crate. → Recommend a Claude custom outline for a desktop GUI app (see Phase 1 template decision).
