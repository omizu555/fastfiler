//! パフォーマンス計測 (デバッグ用)。
//!
//! 仕様:
//! - 常時 ON、リングバッファ 500 件 + カテゴリ別集計
//! - `perf::scope(kind, detail)` で RAII guard を取り、Drop 時に記録
//! - `parking_lot::Mutex` で thread-safe。worker thread からも呼べる
//! - ScopeGuard は `!Send` (PhantomData<Rc<()>>) で thread 跨ぎ誤用を防止
//! - 設定 → デバッグタブが UI 側 polling で snapshot を表示
//!
//! 重要な設計判断:
//! - `MetricSample` は `Instant`/`SystemTime` を保持し、表示用文字列は export 時に生成
//!   (record の critical section を最短にする)
//! - `detail` は record 時点で 256 chars truncate
//! - `Drop` は絶対 panic させない (lock 失敗時は record skip)

use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

/// 計測カテゴリ。固定 enum で集計テーブルの行が確実に揃う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricKind {
    /// フォルダ列挙 (read_dir → 表示完了)
    ListDir,
    /// ツリー load_children (展開時の子ノード読込)
    TreeLoad,
    /// ファイルコピー (worker job 全体)
    Copy,
    /// ファイル移動 (worker job 全体)
    Move,
    /// ゴミ箱送り
    Delete,
    /// ペインナビゲーション (path 変更 + history + signal 更新の handler 時間)
    Navigate,
    /// フィルタ更新 (search_query 変化時の絞り込み)
    Filter,
    /// 列ソート切替
    Sort,
    /// タブ切替 (signal set のハンドラ時間)
    TabSwitch,
    /// ペイン分割
    PaneSplit,
}

impl MetricKind {
    pub const ALL: &'static [MetricKind] = &[
        MetricKind::ListDir,
        MetricKind::TreeLoad,
        MetricKind::Copy,
        MetricKind::Move,
        MetricKind::Delete,
        MetricKind::Navigate,
        MetricKind::Filter,
        MetricKind::Sort,
        MetricKind::TabSwitch,
        MetricKind::PaneSplit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MetricKind::ListDir => "ListDir",
            MetricKind::TreeLoad => "TreeLoad",
            MetricKind::Copy => "Copy",
            MetricKind::Move => "Move",
            MetricKind::Delete => "Delete",
            MetricKind::Navigate => "Navigate",
            MetricKind::Filter => "Filter",
            MetricKind::Sort => "Sort",
            MetricKind::TabSwitch => "TabSwitch",
            MetricKind::PaneSplit => "PaneSplit",
        }
    }
}

const DETAIL_MAX: usize = 256;
const RING_CAP: usize = 500;

/// 個別の計測サンプル。
#[derive(Debug, Clone)]
pub struct MetricSample {
    pub kind: MetricKind,
    pub detail: String,
    pub dur_ms: f64,
    pub at: SystemTime,
}

/// カテゴリ別集計。
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricAgg {
    pub count: u64,
    pub sum_ms: f64,
    pub max_ms: f64,
    pub min_ms: f64,
    pub last_ms: f64,
}

impl MetricAgg {
    pub fn avg_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum_ms / self.count as f64
        }
    }
    fn record(&mut self, dur_ms: f64) {
        if self.count == 0 {
            self.min_ms = dur_ms;
            self.max_ms = dur_ms;
        } else {
            if dur_ms < self.min_ms {
                self.min_ms = dur_ms;
            }
            if dur_ms > self.max_ms {
                self.max_ms = dur_ms;
            }
        }
        self.count += 1;
        self.sum_ms += dur_ms;
        self.last_ms = dur_ms;
    }
}

#[derive(Debug, Default)]
struct MetricsStore {
    ring: VecDeque<MetricSample>,
    aggs: HashMap<MetricKind, MetricAgg>,
}

fn store() -> &'static Mutex<MetricsStore> {
    static STORE: OnceLock<Mutex<MetricsStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(MetricsStore::default()))
}

/// snapshot 結果 (UI 側に渡す値)。
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// カテゴリ別集計 (MetricKind::ALL の順)
    pub aggs: Vec<(MetricKind, MetricAgg)>,
    /// 直近サンプル (新しい順)
    pub samples: Vec<MetricSample>,
}

/// スナップショットを取る。dialog poll で 500ms ごとに呼ばれる。
pub fn snapshot() -> Snapshot {
    let s = store().lock();
    let aggs: Vec<(MetricKind, MetricAgg)> = MetricKind::ALL
        .iter()
        .map(|k| (*k, s.aggs.get(k).cloned().unwrap_or_default()))
        .collect();
    // 新しい順 (push_back しているので逆順)
    let samples: Vec<MetricSample> = s.ring.iter().rev().cloned().collect();
    Snapshot { aggs, samples }
}

/// 全クリア (集計 + リング)。
pub fn clear() {
    let mut s = store().lock();
    s.ring.clear();
    s.aggs.clear();
}

/// 集計表 + 直近ログをテキスト形式で吐き出す (クリップボードコピー用)。
pub fn export_text() -> String {
    let snap = snapshot();
    let mut out = String::new();
    out.push_str("=== FastFiler Performance Metrics ===\n\n");
    out.push_str("[Aggregate]\n");
    out.push_str(&format!(
        "{:<11} {:>7} {:>10} {:>10} {:>10} {:>10}\n",
        "kind", "count", "avg(ms)", "max(ms)", "min(ms)", "last(ms)"
    ));
    for (k, a) in &snap.aggs {
        out.push_str(&format!(
            "{:<11} {:>7} {:>10.2} {:>10.2} {:>10.2} {:>10.2}\n",
            k.label(),
            a.count,
            a.avg_ms(),
            a.max_ms,
            a.min_ms,
            a.last_ms,
        ));
    }
    out.push_str("\n[Recent samples (newest first, up to 100)]\n");
    for s in snap.samples.iter().take(100) {
        out.push_str(&format!(
            "{} [{}] {:>8.2}ms {}\n",
            format_systemtime_jst(s.at),
            s.kind.label(),
            s.dur_ms,
            s.detail
        ));
    }
    out
}

/// 既に計測済みの ms 値を手動で記録する (navigate 内で navigate 全体と ListDir を
/// 分けて記録するような用途)。detail は record 内で truncate される。
pub fn record_manual(kind: MetricKind, detail: impl Into<String>, dur_ms: f64) {
    let mut d: String = detail.into();
    if d.len() > DETAIL_MAX {
        let mut end = DETAIL_MAX;
        while end > 0 && !d.is_char_boundary(end) {
            end -= 1;
        }
        d.truncate(end);
        d.push('…');
    }
    record(kind, d, dur_ms, SystemTime::now());
}

/// 内部 record 関数。Drop から呼ばれる。絶対 panic させない。
fn record(kind: MetricKind, detail: String, dur_ms: f64, at: SystemTime) {
    // try_lock で contention 時は捨てる選択肢もあるが、UI 側 poll は 500ms 間隔で
    // 競合しても十分すぐ取れるので普通の lock を使う。
    let mut s = store().lock();
    s.aggs.entry(kind).or_default().record(dur_ms);
    if s.ring.len() >= RING_CAP {
        s.ring.pop_front();
    }
    s.ring.push_back(MetricSample {
        kind,
        detail,
        dur_ms,
        at,
    });
}

/// RAII 計測 guard。Drop 時に記録される。
///
/// `!Send`: thread 跨ぎ誤用 (UI で作って worker で Drop 等) を防ぐ。
/// worker で測りたい場合は worker closure 内で `perf::scope` を呼ぶこと。
pub struct ScopeGuard {
    kind: MetricKind,
    detail: String,
    start: Instant,
    started_at: SystemTime,
    _not_send: PhantomData<Rc<()>>,
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        let dur_ms = self.start.elapsed().as_secs_f64() * 1000.0;
        // panic unwind 中でも安全に記録 (poison は parking_lot が出さない、Drop 内 panic 禁止)
        let kind = self.kind;
        let detail = std::mem::take(&mut self.detail);
        let at = self.started_at;
        // catch_unwind は要らない (record は panic しない設計)
        record(kind, detail, dur_ms, at);
    }
}

/// 計測スコープを開始。返ってきた guard を `let _g = perf::scope(...);` で保持。
pub fn scope(kind: MetricKind, detail: impl Into<String>) -> ScopeGuard {
    let mut d: String = detail.into();
    if d.len() > DETAIL_MAX {
        // char boundary を尊重
        let mut end = DETAIL_MAX;
        while end > 0 && !d.is_char_boundary(end) {
            end -= 1;
        }
        d.truncate(end);
        d.push('…');
    }
    ScopeGuard {
        kind,
        detail: d,
        start: Instant::now(),
        started_at: SystemTime::now(),
        _not_send: PhantomData,
    }
}

/// SystemTime → JST "HH:MM:SS.mmm" (logger.rs と同じ計算ロジックを軽量に)。
pub fn format_systemtime_jst(t: SystemTime) -> String {
    let dur = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let ms = dur.subsec_millis();
    let jst = secs + 9 * 3600;
    let sod = jst % 86400;
    let hh = sod / 3600;
    let mm = (sod % 3600) / 60;
    let ss = sod % 60;
    format!("{:02}:{:02}:{:02}.{:03}", hh, mm, ss, ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    /// `store()` は process global なので、テスト間の干渉を避けるため
    /// 1 つの #[test] にまとめてシリアル実行する。
    #[test]
    fn perf_scope_behavior() {
        // case 1: scope_records_duration_and_detail
        clear();
        {
            let _g = scope(MetricKind::ListDir, "C:\\test");
            sleep(Duration::from_millis(2));
        }
        let snap = snapshot();
        let (_, agg) = snap
            .aggs
            .iter()
            .find(|(k, _)| *k == MetricKind::ListDir)
            .unwrap();
        assert_eq!(agg.count, 1);
        assert!(agg.last_ms >= 1.0);
        assert_eq!(snap.samples.len(), 1);
        assert_eq!(snap.samples[0].detail, "C:\\test");

        // case 2: detail truncation
        clear();
        let long = "x".repeat(1000);
        drop(scope(MetricKind::Filter, long));
        let snap = snapshot();
        assert!(snap.samples[0].detail.chars().count() <= DETAIL_MAX + 1);

        // case 3: ring buffer cap
        clear();
        for i in 0..(RING_CAP + 50) {
            drop(scope(MetricKind::Sort, format!("col-{}", i)));
        }
        let snap = snapshot();
        assert_eq!(snap.samples.len(), RING_CAP);
        assert!(snap.samples[0].detail.starts_with("col-"));

        // case 4: agg min/max/avg/last
        clear();
        let now = SystemTime::now();
        record(MetricKind::Copy, "a".into(), 10.0, now);
        record(MetricKind::Copy, "b".into(), 30.0, now);
        record(MetricKind::Copy, "c".into(), 20.0, now);
        let snap = snapshot();
        let (_, a) = snap
            .aggs
            .iter()
            .find(|(k, _)| *k == MetricKind::Copy)
            .unwrap();
        assert_eq!(a.count, 3);
        assert!((a.avg_ms() - 20.0).abs() < 1e-6);
        assert!((a.max_ms - 30.0).abs() < 1e-6);
        assert!((a.min_ms - 10.0).abs() < 1e-6);
        assert!((a.last_ms - 20.0).abs() < 1e-6);
    }
}
