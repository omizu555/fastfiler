//! メモリ調査用の計装 (cargo feature `mem-debug` でのみ有効)。
//!
//! 目的:
//!   1. PaneState / Tab の **論理 live 数** を計数し、タブ/ペインを閉じた後に
//!      ベースラインへ戻るか (= 真のリークか) を判別する。
//!   2. counting global allocator でヒープ実使用量を測り、WorkingSet との差から
//!      「解放済みだが OS に返らないだけ (wgpu では普通)」と「ヒープリーク」を区別する。
//!
//! feature OFF 時は [`AliveHandle`] が `()` (ZST) になり、計測コードは一切コンパイル
//! されないため完全ゼロコスト。

// ────────────────────────────────────────────────────────────────
// feature ON: 実体
// ────────────────────────────────────────────────────────────────
#[cfg(feature = "mem-debug")]
mod imp {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
    use std::sync::Arc;

    pub static PANES_ALIVE: AtomicI64 = AtomicI64::new(0);
    pub static TABS_ALIVE: AtomicI64 = AtomicI64::new(0);

    static HEAP_CUR: AtomicUsize = AtomicUsize::new(0);
    static HEAP_PEAK: AtomicUsize = AtomicUsize::new(0);

    /// 1 つの論理インスタンスに対応するトークン。`Arc` で clone 間共有し、
    /// 最後の clone が drop された時にカウンタを減らす。
    pub struct LiveToken {
        counter: &'static AtomicI64,
    }

    impl LiveToken {
        fn new(counter: &'static AtomicI64) -> Self {
            counter.fetch_add(1, Ordering::Relaxed);
            Self { counter }
        }
    }

    impl Drop for LiveToken {
        fn drop(&mut self) {
            self.counter.fetch_sub(1, Ordering::Relaxed);
        }
    }

    pub type AliveHandle = Arc<LiveToken>;

    pub fn pane_token() -> AliveHandle {
        Arc::new(LiveToken::new(&PANES_ALIVE))
    }

    pub fn tab_token() -> AliveHandle {
        Arc::new(LiveToken::new(&TABS_ALIVE))
    }

    /// `System` をラップしてヒープ確保量を計数する global allocator。
    pub struct TrackingAlloc;

    unsafe impl GlobalAlloc for TrackingAlloc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let p = System.alloc(layout);
            if !p.is_null() {
                let cur = HEAP_CUR.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
                HEAP_PEAK.fetch_max(cur, Ordering::Relaxed);
            }
            p
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout);
            HEAP_CUR.fetch_sub(layout.size(), Ordering::Relaxed);
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let p = System.realloc(ptr, layout, new_size);
            if !p.is_null() {
                if new_size >= layout.size() {
                    let d = new_size - layout.size();
                    let cur = HEAP_CUR.fetch_add(d, Ordering::Relaxed) + d;
                    HEAP_PEAK.fetch_max(cur, Ordering::Relaxed);
                } else {
                    HEAP_CUR.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
                }
            }
            p
        }
    }

    /// 1 時点のメモリスナップショット (バイト単位、live 数は個数)。
    pub struct MemSnapshot {
        pub panes: i64,
        pub tabs: i64,
        pub heap_cur: usize,
        pub heap_peak: usize,
        pub working_set: u64,
        pub private_bytes: u64,
        pub threads: u32,
        pub handles: u32,
    }

    #[cfg(windows)]
    fn process_mem() -> (u64, u64) {
        use windows::Win32::System::ProcessStatus::{
            K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;
        unsafe {
            let mut c = PROCESS_MEMORY_COUNTERS::default();
            let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, size).as_bool() {
                (c.WorkingSetSize as u64, c.PagefileUsage as u64)
            } else {
                (0, 0)
            }
        }
    }

    /// (スレッド数, ハンドル数) を取得。OS リソースのリーク検出用。
    /// notify watcher 等が解放されないとここが増え続ける。
    #[cfg(windows)]
    fn process_handles_threads() -> (u32, u32) {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
        };
        use windows::Win32::System::Threading::GetProcessHandleCount;
        use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId};
        let mut handles: u32 = 0;
        unsafe {
            let _ = GetProcessHandleCount(GetCurrentProcess(), &mut handles);
        }
        let mut threads: u32 = 0;
        unsafe {
            let pid = GetCurrentProcessId();
            if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) {
                let mut entry = THREADENTRY32 {
                    dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
                    ..Default::default()
                };
                if Thread32First(snap, &mut entry).is_ok() {
                    loop {
                        if entry.th32OwnerProcessID == pid {
                            threads += 1;
                        }
                        if Thread32Next(snap, &mut entry).is_err() {
                            break;
                        }
                    }
                }
                let _ = windows::Win32::Foundation::CloseHandle(HANDLE(snap.0));
            }
        }
        (threads, handles)
    }

    #[cfg(not(windows))]
    fn process_handles_threads() -> (u32, u32) {
        (0, 0)
    }

    #[cfg(not(windows))]
    fn process_mem() -> (u64, u64) {
        (0, 0)
    }

    pub fn snapshot() -> MemSnapshot {
        let (working_set, private_bytes) = process_mem();
        let (threads, handles) = process_handles_threads();
        MemSnapshot {
            panes: PANES_ALIVE.load(Ordering::Relaxed),
            tabs: TABS_ALIVE.load(Ordering::Relaxed),
            heap_cur: HEAP_CUR.load(Ordering::Relaxed),
            heap_peak: HEAP_PEAK.load(Ordering::Relaxed),
            working_set,
            private_bytes,
            threads,
            handles,
        }
    }

    pub fn log_snapshot(label: &str) {
        let s = snapshot();
        let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
        let mbz = |b: usize| b as f64 / (1024.0 * 1024.0);
        crate::flog!(
            "[mem] {} panes={} tabs={} threads={} handles={} heap_cur={:.1}MB heap_peak={:.1}MB ws={:.1}MB priv={:.1}MB",
            label,
            s.panes,
            s.tabs,
            s.threads,
            s.handles,
            mbz(s.heap_cur),
            mbz(s.heap_peak),
            mb(s.working_set),
            mb(s.private_bytes),
        );
    }
}

#[cfg(feature = "mem-debug")]
pub use imp::{
    log_snapshot, pane_token, snapshot, tab_token, AliveHandle, MemSnapshot, TrackingAlloc,
};

// ────────────────────────────────────────────────────────────────
// feature OFF: ゼロコストなダミー
// ────────────────────────────────────────────────────────────────
#[cfg(not(feature = "mem-debug"))]
mod imp_off {
    /// feature OFF 時は ZST。構造体フィールドとして常設してもコスト無し。
    pub type AliveHandle = ();

    #[inline(always)]
    pub fn pane_token() -> AliveHandle {}

    #[inline(always)]
    pub fn tab_token() -> AliveHandle {}
}

#[cfg(not(feature = "mem-debug"))]
pub use imp_off::{pane_token, tab_token, AliveHandle};
