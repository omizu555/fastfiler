//! 設定・セッションファイルのクラッシュ安全な保存／読み込み。
//!
//! 突然の電源断・強制終了でファイルが空 / 破損になり、タブ等の情報が消えるのを防ぐ。
//! 素の `std::fs::write` は「長さ 0 へ切り詰めてから書き直す」ため、書き込み中の
//! 電源断や Windows のライトキャッシュ未フラッシュにより 0 バイト / 途中切れの
//! ファイルが残ることがある (→ 次回起動で復元失敗 → 既定状態で立ち上がる)。
//!
//! ここでは以下で守る:
//! 1. **アトミック書き込み**: `*.tmp` へ書き → `sync_all` で物理ディスクまで確実に
//!    落とし → `rename` で本体へアトミック置換。本体は常に「古い完全版」か
//!    「新しい完全版」のどちらかになり、中途半端な状態が存在しない。
//! 2. **バックアップ + フォールバック**: 置換前に直前の正常版を `*.bak` へ複製。
//!    読み込み時は本体が壊れていれば `*.bak` を試す。

use std::path::{Path, PathBuf};

/// `path` の末尾に拡張子を **追加** したパスを作る (置換ではない)。
/// 例: `gpui_session.json` → `gpui_session.json.tmp` / `.bak`。
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

/// `path` へ `contents` をアトミックかつ耐久的に書き込む。
///
/// 失敗しても呼び出し側は致命的に扱わない想定 (保存はベストエフォート)。
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::fs;
    use std::io::Write;

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    // 1. 一時ファイルへ書き出し、物理ディスクまで確実にフラッシュする。
    let tmp = with_suffix(path, ".tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.flush()?;
        // OS のライトキャッシュをフラッシュ。これにより電源断後も tmp は
        // 完全な内容で残る (rename 後の本体が空になる事故を防ぐ)。
        f.sync_all()?;
    }

    // 2. 直前の正常版を .bak に退避 (本体置換が万一壊れても復旧できる保険)。
    //    本体がまだ無い初回保存では何もしない。複製失敗は無視 (致命的でない)。
    if path.exists() {
        let _ = fs::copy(path, with_suffix(path, ".bak"));
    }

    // 3. tmp → 本体。同一ディレクトリ内の rename なので OS レベルでアトミックに
    //    置換され、本体は瞬時に新版へ切り替わる (中途半端な状態を経由しない)。
    fs::rename(&tmp, path)
}

/// 本体 → `.bak` の順にパースを試し、最初に成功した値を返す。
///
/// 本体が存在しない / 読めない / パースに失敗した場合は `.bak` を試し、
/// それも駄目なら `None`。`parse` はパース成否を `Option` で表す。
pub fn load_with_backup<T>(path: &Path, parse: impl Fn(&str) -> Option<T>) -> Option<T> {
    for p in [path.to_path_buf(), with_suffix(path, ".bak")] {
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Some(v) = parse(&s) {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用の一意な作業ディレクトリ (tempfile 依存を避け std だけで完結)。
    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("fastfiler-persist-test")
            .join(format!("{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_then_load_roundtrip() {
        let p = scratch_dir("roundtrip").join("session.json");
        write_atomic(&p, "hello").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "hello");
        let got = load_with_backup(&p, |s| Some(s.to_string()));
        assert_eq!(got.as_deref(), Some("hello"));
    }

    #[test]
    fn creates_parent_dirs() {
        let p = scratch_dir("mkdir").join("nested").join("a").join("session.json");
        write_atomic(&p, "x").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "x");
    }

    #[test]
    fn keeps_backup_of_previous_version() {
        let p = scratch_dir("backup").join("session.json");
        write_atomic(&p, "v1").unwrap();
        write_atomic(&p, "v2").unwrap();
        // 本体は最新、.bak は直前。
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "v2");
        assert_eq!(
            std::fs::read_to_string(with_suffix(&p, ".bak")).unwrap(),
            "v1"
        );
    }

    #[test]
    fn falls_back_to_backup_when_main_is_corrupt() {
        let p = scratch_dir("fallback").join("session.json");
        write_atomic(&p, "good-v1").unwrap();
        write_atomic(&p, "good-v2").unwrap(); // .bak = good-v1
        // 電源断で本体が空 / 壊れた状況を模倣 (パース失敗扱いにする)。
        std::fs::write(&p, "").unwrap();
        let got = load_with_backup(&p, |s| {
            let s = s.trim();
            if s.is_empty() { None } else { Some(s.to_string()) }
        });
        assert_eq!(got.as_deref(), Some("good-v1"));
    }
}
