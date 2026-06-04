# vendor/ — GPUI 完全移植 (zed フォルダ非依存)

FastFiler の GUI は Zed の **GPUI** を使う。ただし FastFiler と Zed は別々の Git
リポジトリで管理されるため、**zed フォルダを直接 path 参照しない**。代わりに GPUI と
その依存クレート群を本ディレクトリに**コピー (vendor)** し、独立したサブワークスペース
として自己完結ビルドできるようにしている。

## 取り込み元

| 項目 | 値 |
|---|---|
| 元リポジトリ | zed-industries/zed (ローカルチェックアウト) |
| 取り込みコミット | `6d72acdb9911a7bd6a4399c82562506f4758a23c` (2026-06-03) |
| 対象プラットフォーム | **Windows のみ** (x86_64-pc-windows-msvc) |

## 構成

- `Cargo.toml` … 独立サブワークスペース。zed ルートの `[workspace.package]` /
  `[workspace.dependencies]` / `[workspace.lints]` をミラーし、各クレートの
  `xxx.workspace = true` 継承を解決する。`members` は下記の vendor 済みクレートのみ。
- `crates/` … zed の `crates/<name>` と同じレイアウト (相対 path 依存がそのまま効く)。
- `tooling/perf` … zed の `tooling/perf` (一部クレートが参照)。

本サブワークスペースは Files 直下のメインワークスペースから `exclude` されており、
`crates/fastfiler-gpui` が `vendor/crates/gpui` を path 依存する。

## vendor 済みクレート (18)

Windows ビルドに必要な推移閉包 (`cargo metadata --filter-platform` で確定):

```
collections  gpui  gpui_macros  gpui_platform  gpui_shared_string
gpui_util  gpui_windows  http_client  refineable (+derive_refineable)
scheduler  sum_tree  util  util_macros  zlog  ztracing  ztracing_macro  perf
```

> Windows では `gpui_wgpu` / `wgpu` / `media` は不要 (gpui_windows は `windows`
> クレート経由で DirectX を直接使用)。

## FastFiler 向けの改変 (re-vendor 時に再適用すること)

最小限。いずれも「vendor していない非 Windows / dev 専用の内部クレート参照」を外すだけ:

1. **`crates/gpui_platform/Cargo.toml`** — Windows 専用に簡略化。
   macOS/Linux/wasm 依存 (`gpui_macos` / `gpui_linux` / `gpui_web`) を除去し、
   それらを参照していた feature (`font-kit` / `wayland` / `x11` / `runtime_shaders`)
   を **no-op (名前のみ)** に変更。
2. **`crates/gpui/Cargo.toml`** — 内部参照と zed git 依存を除去:
   - `media.workspace = true` (macOS 専用 deps)
   - `reqwest_client` (not-wasm dev-deps)
   - `gpui_web.workspace = true` (wasm dev-deps)
   - `font-kit` (macOS 専用・zed-industries git fork) と default からの除去
   - `scap` (screen-capture 用・zed git 依存) と `x11`/`screen-capture` feature の no-op 化
3. **`crates/gpui_windows/Cargo.toml`** — `scap` 依存と screen-capture feature の
   scap 参照を除去 (scap は zed git の `windows-capture` を芋づる式に引くため)。

各改変箇所は `# [FastFiler vendor]` コメントで明示している。

## git 依存の状況

`zed-industries/*` への git 依存 (font-kit / scap / windows-capture) は
**すべて除去済み** (`Cargo.lock` に該当なし)。

`Cargo.lock` に残る git ソースは **`smol-rs/async-task` (patch) のみ**。これは
zed とは無関係な標準的な非同期ランタイムクレートで、gpui が直接使う async-task を
移植元と同じ rev に固定するためのもの (Files 直下の `[patch.crates-io]`)。

## 更新 (re-vendor) 手順

1. 新しい zed チェックアウトを用意。
2. 上記 18 クレート + `tooling/perf` を `vendor/` へ上書きコピー。
3. 「FastFiler 向けの改変」を再適用 (`# [FastFiler vendor]` 箇所)。
4. `vendor/Cargo.toml` の `[workspace.dependencies]` / `[workspace.lints]` /
   `[workspace.package]` を新 zed のルート Cargo.toml から取り直す。
5. メインワークスペース直下で `cargo build -p fastfiler-gpui` が通ることを確認。
6. 本 README の「取り込みコミット」を更新。
