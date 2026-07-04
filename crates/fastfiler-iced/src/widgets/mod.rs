//! 自前ウィジェット群 (計画書 §5.1)。

pub mod context_menu;
pub mod drag_handle;
pub mod file_list;
pub mod tab_bar;
pub mod tree_list;

/// ラベルを概算幅で「収まる分 + …」に切り詰める (エクスプローラ風)。
/// 直描きテキストは幅制限すると折り返しが発生し行が重なるため、
/// レイアウト前に文字列自体を短くする。グリフ幅は ASCII ≈ 0.55em /
/// それ以外 ≈ 1.0em の概算 (誤差数 px)。
pub(crate) fn ellipsize(label: &str, max_w: f32, font_px: f32) -> String {
    let char_w = |c: char| {
        if c.is_ascii() {
            font_px * 0.55
        } else {
            font_px
        }
    };
    let total: f32 = label.chars().map(char_w).sum();
    if total <= max_w {
        return label.to_string();
    }
    let ell_w = font_px; // "…"
    let mut used = 0.0;
    let mut out = String::new();
    for c in label.chars() {
        let w = char_w(c);
        if used + w + ell_w > max_w {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push('…');
    out
}
