//! アプリ設定 (`%APPDATA%\FastFiler\settings.json`)。
//!
//! 旧版 (settings_store.rs) のスキーマ互換。無ければ旧ファイル (iced_/gpui_
//! 接頭辞) から自動移行 (N-05 — 読むだけで元は触らない)。
//! `get()` は static 経由でどこからでも参照できる (GPUI 版と同じ流儀)。

use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct AppSettings {
    /// テーマ名 (プリセット or ユーザーテーマ)。None なら既定 (ダーク)。
    #[serde(default)]
    pub theme: Option<String>,
    /// Everything HTTP サーバのポート (検索連携)。
    #[serde(default = "default_port")]
    pub everything_port: u16,
    /// タブバーの列数 (1〜4)。並びは行優先。
    #[serde(default = "default_tab_columns")]
    pub tab_columns: u8,
    /// タブバーに「ツリー」トグルボタンを表示するか。
    #[serde(default = "default_true")]
    pub show_tree_button: bool,
    /// UI フォントサイズ (px)。既定 16 (GPUI 版と同じ)。行高が追従する。
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    /// UI フォントファミリー。None ならシステム既定 (Yu Gothic UI)。
    #[serde(default)]
    pub font_family: Option<String>,
    /// UI スタイル名 (形状プリセット: モダン/シャープ/ソフト)。None なら既定。
    #[serde(default)]
    pub style: Option<String>,
    /// レンダラ: None/"lowmem" = 省メモリ (tiny-skia — **既定**。メモリ約 1/10・
    /// 起動高速) | "gpu" = wgpu/DX12 (大画面で描画が重い場合の選択肢)。
    #[serde(default)]
    pub renderer: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_port() -> u16 {
    80
}
fn default_tab_columns() -> u8 {
    1
}
fn default_font_size() -> f32 {
    16.0
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: None,
            everything_port: default_port(),
            tab_columns: default_tab_columns(),
            show_tree_button: true,
            font_size: default_font_size(),
            font_family: None,
            style: None,
            renderer: None,
        }
    }
}

impl AppSettings {
    /// フォントサイズに追従する一覧の行高 (px)。既定 16px → 24px (GPUI 版と同じ比)。
    pub fn row_h(&self) -> f32 {
        (self.font_size * 1.5).clamp(18.0, 48.0)
    }

    /// 一覧・各種ラベルの基準テキストサイズ。
    pub fn text_px(&self) -> f32 {
        self.font_size.clamp(10.0, 32.0) * 13.0 / 16.0
    }
}

fn config_dir() -> Option<PathBuf> {
    let base = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(base).join("FastFiler"))
}

fn config_path() -> Option<PathBuf> {
    Some(config_dir()?.join("settings.json"))
}

static STORE: OnceLock<RwLock<AppSettings>> = OnceLock::new();

fn store() -> &'static RwLock<AppSettings> {
    STORE.get_or_init(|| RwLock::new(load()))
}

fn load() -> AppSettings {
    let Some(dir) = config_dir() else {
        return AppSettings::default();
    };
    // 正式名 → 開発期の iced_ 名 → GPUI 版、の順 (自動移行 N-05)
    for name in ["settings.json", "iced_settings.json", "gpui_settings.json"] {
        if let Some(s) = fastfiler_core::persist::load_with_backup(&dir.join(name), |s| {
            serde_json::from_str(s).ok()
        }) {
            return s;
        }
    }
    AppSettings::default()
}

/// 現在の設定のコピーを返す。
pub fn get() -> AppSettings {
    store().read().map(|g| g.clone()).unwrap_or_default()
}

/// 設定を変更して即保存する (GPUI 版と同じ)。
pub fn update(f: impl FnOnce(&mut AppSettings)) -> AppSettings {
    let cur = update_in_memory(f);
    save(&cur);
    cur
}

/// メモリ上だけ変更する (高頻度変更用 — 保存は `flush()` に委ねる)。
pub fn update_in_memory(f: impl FnOnce(&mut AppSettings)) -> AppSettings {
    let mut cur = get();
    f(&mut cur);
    if let Ok(mut g) = store().write() {
        *g = cur.clone();
    }
    cur
}

/// 現在のメモリ上の設定をディスクへ書く (デバウンス保存の合流点)。
pub fn flush() {
    save(&get());
}

fn save(s: &AppSettings) {
    let Some(p) = config_path() else { return };
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = fastfiler_core::persist::write_atomic(&p, &json);
    }
}
