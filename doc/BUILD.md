# FastFiler ビルド & インストール手順

## 必要環境

- Windows 10/11 + WebView2 Runtime
- Node.js 22+
- Rust 1.77+
- Visual Studio 2022 Build Tools (MSVC)

---

## 開発（ホットリロード起動）

```powershell
npm install
npm run tauri:dev
```

初回ビルドは Rust の依存解決で 5〜15 分ほどかかります。2 回目以降はインクリメンタル。

---

## 本番ビルド & インストーラ作成

```powershell
npm install
npm run tauri:build
```

成果物は `src-tauri/target/release/` 以下に作成されます。

| ファイル | 場所 | 用途 |
|---|---|---|
| `FastFiler.exe` | `src-tauri/target/release/` | スタンドアロン実行ファイル（ZIP 配布用） |
| `FastFiler_<ver>_x64-setup.exe` | `src-tauri/target/release/bundle/nsis/` | NSIS インストーラ（推奨配布形式） |

> 配布ターゲットは `src-tauri/tauri.conf.json` の `bundle.targets` で切替可能です。`["nsis", "msi"]` にすれば MSI も同時生成されます（WiX が無ければ初回はダウンロードされます）。

---

## 初回リリース手順

1. **バージョン更新**
   - `package.json` の `version`
   - `src-tauri/tauri.conf.json` の `version`
   - `src-tauri/Cargo.toml` の `version`
   （3 箇所が一致している必要があります）
2. **クリーンビルド**
   ```powershell
   Remove-Item -Recurse -Force dist, src-tauri\target\release\bundle -ErrorAction SilentlyContinue
   npm install
   npm run tauri:build
   ```
3. **動作確認**
   - 生成された `FastFiler_0.1.0_x64-setup.exe` をクリーン環境（または別ユーザー）でインストール
   - 初回起動 → ドライブ列挙 / タブ作成 / 設定保存 / アンインストールまで一通り確認
4. **GitHub Release 作成**
   - タグ: `v0.1.0`
   - タイトル: `FastFiler 0.1.0 (Initial release)`
   - 添付: `FastFiler_0.1.0_x64-setup.exe` と（任意で）`FastFiler.exe` を ZIP 化したもの
   - 説明欄に主要機能（縦タブ / 任意分割 / 連動 / Everything 連携 / プラグイン基盤）と動作要件（Windows 10/11 + WebView2）を記載
5. **README に最新リリースリンク**
   - 必要に応じてバッジ・ダウンロードリンクを追記

---

## コード署名（任意・将来対応）

未署名 EXE は SmartScreen 警告が出ます。コード署名証明書をお持ちの場合、`tauri.conf.json` の `bundle.windows.signCommand` に `signtool` 呼び出しを設定すると自動署名されます。初回リリースは未署名でも問題ありません。

---

## WebView2 ランタイム

Windows 11 / 最近の Windows 10 には標準同梱されていますが、古い環境向けに `bundle.windows.webviewInstallMode` を `downloadBootstrapper`（既定）にしておくと、インストーラ実行時に必要なら自動ダウンロードされます。
