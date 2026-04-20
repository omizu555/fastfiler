// v4.0 (40b) OLE D&D 完全対応 — スキャフォールド
//
// HTML5 D&D は内部移動には十分ですが、以下のシナリオに対応するには
// Win32 OLE D&D (IDropTarget / IDataObject / DoDragDrop) が必要です:
//
//   - 外部アプリ (Outlook の添付, ブラウザのリンク, Photoshop のレイヤ) からの drop
//   - エクスプローラへ「コピー」or「ショートカット作成」のヒントを伴う drag-out
//   - CFSTR_FILEDESCRIPTORW + CFSTR_FILECONTENTS による仮想ファイル受け渡し
//
// 完全実装の方針 (windows-rs):
//   1. tauri::WindowEvent or AppHandle::get_webview_window から HWND を取得
//   2. RegisterDragDrop(hwnd, IDropTarget*) で受信側を有効化
//   3. IDropTarget の DragEnter/DragOver/Drop を実装し
//      CF_HDROP / CFSTR_FILEDESCRIPTORW を解釈 → ローカルパス or temp 展開後パスを
//      Tauri へ event ("ole-drop") で配信
//   4. 送信側: drag-start で IDataObject を構築し DoDragDrop を呼ぶ
//      - SetData で CF_HDROP (内部選択中の絶対パス群)
//      - 効果フラグ (DROPEFFECT_COPY/MOVE/LINK) を尊重し終了後に内部状態を更新
//
// COM apartment が STA であること、HWND がメインスレッドで作られていることに注意。
// 大物のため別セッションで段階的に実装します。

use crate::error::{AppError, AppResult};

#[tauri::command]
pub fn ole_dnd_register() -> AppResult<()> {
    // TODO(v4.0-40b): RegisterDragDrop で IDropTarget を window に紐付け
    Err(AppError::Other(
        "ole_dnd_register: 未実装 (v4.0 で対応予定)".into(),
    ))
}

#[tauri::command]
pub fn ole_dnd_start_drag(_paths: Vec<String>, _allowed_effects: u32) -> AppResult<u32> {
    // TODO(v4.0-40b): IDataObject を組み立てて DoDragDrop を呼ぶ
    Err(AppError::Other(
        "ole_dnd_start_drag: 未実装 (v4.0 で対応予定)".into(),
    ))
}
