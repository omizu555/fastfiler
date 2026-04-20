// v4.0 (40a) ネイティブ IContextMenu 統合 — スキャフォールド
//
// このモジュールは「Windows のシェル右クリックメニュー (送る / 開く / 7-Zip / Git Bash …) を
// 当アプリのコンテキストメニューに統合する」ための土台です。
//
// 完全実装には以下が必要です（windows-rs ベース）:
//   1. SHParseDisplayName で各選択パスから ITEMIDLIST を取得
//   2. SHBindToParent で親フォルダの IShellFolder と PIDL を取得
//   3. IShellFolder::GetUIObjectOf(IID_IContextMenu) で IContextMenu を取得
//   4. IContextMenu::QueryContextMenu で動的メニュー ID を割り付け
//   5. IContextMenu::GetCommandString でローカライズ済みラベル/ヘルプを抽出して
//      フロントへ JSON で返却
//   6. ユーザが選んだら IContextMenu::InvokeCommand を呼ぶ
//
// COM の初期化 (CoInitializeEx) と STA スレッドモデルが必須なため、
// 専用の "shell menu worker" スレッドで処理する設計を想定しています。
// 大物のため別セッションで段階的に実装します。

use crate::error::{AppError, AppResult};
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct ShellMenuItem {
    pub id: u32,
    pub label: String,
    pub icon: Option<String>,
    pub help: Option<String>,
    pub separator: bool,
    pub submenu: Vec<ShellMenuItem>,
}

#[tauri::command]
pub fn shell_menu_query(_paths: Vec<String>) -> AppResult<Vec<ShellMenuItem>> {
    // TODO(v4.0-40a): IContextMenu::QueryContextMenu を実装
    Err(AppError::Other(
        "shell_menu_query: 未実装 (v4.0 で対応予定)".into(),
    ))
}

#[tauri::command]
pub fn shell_menu_invoke(_paths: Vec<String>, _id: u32) -> AppResult<()> {
    // TODO(v4.0-40a): IContextMenu::InvokeCommand を実装
    Err(AppError::Other(
        "shell_menu_invoke: 未実装 (v4.0 で対応予定)".into(),
    ))
}
