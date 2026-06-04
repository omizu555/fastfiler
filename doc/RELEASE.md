# FastFiler リリース手順 (GPUI 版)

最終更新: 2026-06-04 (GPUI 移植後)

配布用バイナリ (ZIP) を作る手順。実装範囲は [`STATUS.md`](./STATUS.md) を参照。

---

## 1. リリース対象

| 項目 | 値 |
|---|---|
| バージョン | `crates/fastfiler-gpui/Cargo.toml` の `version` |
| プラットフォーム | Windows 10 / 11 (x64) のみ |
| 配布形式 | 単一実行ファイル `fastfiler-gpui.exe` + ドキュメント (ZIP) |
| ランタイム依存 | なし (WebView2 不要) |

---

## 2. リリース前チェックリスト

### コード品質
- [ ] `cargo fmt --all` (差分なし)
- [ ] `cargo clippy -p fastfiler-gpui -p fastfiler-domain -- -D warnings`
- [ ] `cargo build -p fastfiler-gpui --release` (warnings 0)

### 動作確認 (手動)
- [ ] 起動 → 前回セッション (タブ / 分割 / ウィンドウ位置) が復元される
- [ ] フォルダ展開速度 (`C:\Windows\System32` を開いて即時表示)
- [ ] タブ追加/閉じ・ペイン分割/閉じ → **`live panes` がベースラインへ戻る**
- [ ] D&D (ペイン間 / エクスプローラ → FastFiler)
- [ ] コピー / 切り取り / 貼り付け (エクスプローラ相互) + 進捗表示
- [ ] リネーム / 新規フォルダ / 新規ファイル (日本語 IME 入力)
- [ ] ごみ箱削除 (複数選択)
- [ ] ワークスペースツリー (展開 / フォーカスペインに開く / 幅変更)
- [ ] 右クリックメニュー (行 / 背景)

---

## 3. ビルドと梱包

```powershell
cargo build -p fastfiler-gpui --release

# ZIP 構成
fastfiler-<version>-win-x64/
├ fastfiler-gpui.exe
├ README.md
└ doc/USAGE.md
```

```powershell
$v = "x.y.z"
$dir = "fastfiler-$v-win-x64"
mkdir $dir; mkdir $dir\doc
copy target\release\fastfiler-gpui.exe $dir\
copy README.md $dir\
copy doc\USAGE.md $dir\doc\
Compress-Archive -Path $dir -DestinationPath "$dir.zip"
```

---

## 4. タグ付け

```powershell
git tag -a gpui-vX.Y.Z -m "FastFiler GPUI vX.Y.Z"
git push origin gpui-vX.Y.Z   # 任意
```

---

## 5. 既知の制限 (リリースノートに記載)

[`STATUS.md`](./STATUS.md) の「未実装 (採用予定)」を転記する
(D&D 外部送信 / 検索 UI / Undo UI / テーマ設定 など)。
