//! 仮想ファイル貼り付け (virtual_files.rs + file_jobs::run_virtual_paste) の
//! 端から端までの検証。rdpclip / Outlook が載せるのと同じ
//! FileGroupDescriptorW + FileContents を自前の IDataObject で
//! OleSetClipboard し、読取 → 抽出ジョブ → ファイル検証まで通す。
//!
//! **システムのクリップボードを書き換える**ため #[ignore] — 手動実行:
//! `cargo test -p fastfiler-domain --test virtual_paste -- --ignored`

#![cfg(windows)]

use fastfiler_domain::file_jobs::{JobRegistry, VirtualPasteItem};
use fastfiler_domain::virtual_files::{clipboard_read_virtual_entries, virtual_files_available};

use windows::core::implement;
use windows::Win32::Foundation::{
    BOOL, DATA_S_SAMEFORMATETC, DV_E_FORMATETC, DV_E_TYMED, E_NOTIMPL, E_OUTOFMEMORY, HGLOBAL,
    OLE_E_ADVISENOTSUPPORTED, S_OK,
};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::System::Com::{
    IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, DATADIR_GET,
    DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL, TYMED_ISTREAM,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatA;
use windows::Win32::System::Ole::{OleInitialize, OleSetClipboard, OleUninitialize};
use windows::Win32::UI::Shell::{
    SHCreateMemStream, SHCreateStdEnumFmtEtc, FD_ATTRIBUTES, FD_FILESIZE, FILEDESCRIPTORW,
};

fn cf_descriptor() -> u16 {
    unsafe { RegisterClipboardFormatA(windows::core::s!("FileGroupDescriptorW")) as u16 }
}

fn cf_contents() -> u16 {
    unsafe { RegisterClipboardFormatA(windows::core::s!("FileContents")) as u16 }
}

/// FILEGROUPDESCRIPTORW バイト列 (DWORD cItems + packed FILEDESCRIPTORW[])。
fn build_descriptor_bytes(items: &[(&str, bool, Option<u64>)]) -> Vec<u8> {
    let fd_size = std::mem::size_of::<FILEDESCRIPTORW>();
    let mut buf = vec![0u8; 4 + items.len() * fd_size];
    unsafe {
        (buf.as_mut_ptr() as *mut u32).write_unaligned(items.len() as u32);
        for (i, (name, is_dir, size)) in items.iter().enumerate() {
            let mut fd = FILEDESCRIPTORW::default();
            let mut flags = FD_ATTRIBUTES.0 as u32;
            fd.dwFileAttributes = if *is_dir {
                FILE_ATTRIBUTE_DIRECTORY.0
            } else {
                FILE_ATTRIBUTE_NORMAL.0
            };
            if let Some(s) = size {
                flags |= FD_FILESIZE.0 as u32;
                fd.nFileSizeHigh = (s >> 32) as u32;
                fd.nFileSizeLow = *s as u32;
            }
            fd.dwFlags = flags;
            let mut name_buf = [0u16; 260];
            for (j, u) in name.encode_utf16().enumerate() {
                name_buf[j] = u;
            }
            fd.cFileName = name_buf;
            (buf.as_mut_ptr().add(4 + i * fd_size) as *mut FILEDESCRIPTORW).write_unaligned(fd);
        }
    }
    buf
}

/// rdpclip / Outlook 相当の仮想ファイル送信側 (テスト用最小実装)。
#[implement(IDataObject)]
struct VirtualClipSource {
    descriptor: Vec<u8>,
    /// lindex ごとの FileContents (フォルダは None)。
    contents: Vec<Option<Vec<u8>>>,
    cf_desc: u16,
    cf_cont: u16,
}

impl IDataObject_Impl for VirtualClipSource_Impl {
    fn GetData(&self, pformatetc: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        let fe = unsafe { &*pformatetc };
        if fe.dwAspect != DVASPECT_CONTENT.0 {
            return Err(DV_E_FORMATETC.into());
        }
        let bytes: &[u8] = if fe.cfFormat == self.cf_desc {
            &self.descriptor
        } else if fe.cfFormat == self.cf_cont {
            self.contents
                .get(fe.lindex as usize)
                .and_then(|o| o.as_deref())
                .ok_or_else(|| windows::core::Error::from(DV_E_FORMATETC))?
        } else {
            return Err(DV_E_FORMATETC.into());
        };
        // rdpclip は FileContents を IStream で返す — 奇数 lindex を IStream に
        // して受信側の両 tymed 分岐を検証する
        if fe.cfFormat == self.cf_cont && fe.lindex % 2 == 1 {
            if (fe.tymed & TYMED_ISTREAM.0 as u32) == 0 {
                return Err(DV_E_TYMED.into());
            }
            let stream = unsafe { SHCreateMemStream(Some(bytes)) }
                .ok_or_else(|| windows::core::Error::from(E_OUTOFMEMORY))?;
            let mut medium: STGMEDIUM = unsafe { std::mem::zeroed() };
            medium.tymed = TYMED_ISTREAM.0 as u32;
            medium.u = STGMEDIUM_0 {
                pstm: std::mem::ManuallyDrop::new(Some(stream)),
            };
            medium.pUnkForRelease = std::mem::ManuallyDrop::new(None);
            return Ok(medium);
        }
        if (fe.tymed & TYMED_HGLOBAL.0 as u32) == 0 {
            return Err(DV_E_TYMED.into());
        }
        unsafe {
            let h = fastfiler_domain::hdrop::alloc_hglobal_from_bytes(bytes)
                .map_err(|e| windows::core::Error::new(E_OUTOFMEMORY, e.to_string()))?;
            let mut medium: STGMEDIUM = std::mem::zeroed();
            medium.tymed = TYMED_HGLOBAL.0 as u32;
            medium.u = STGMEDIUM_0 {
                hGlobal: HGLOBAL(h.0),
            };
            medium.pUnkForRelease = std::mem::ManuallyDrop::new(None);
            Ok(medium)
        }
    }

    fn GetDataHere(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *mut STGMEDIUM,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> windows::core::HRESULT {
        let fe = unsafe { &*pformatetc };
        if fe.dwAspect != DVASPECT_CONTENT.0 || (fe.tymed & TYMED_HGLOBAL.0 as u32) == 0 {
            return DV_E_TYMED;
        }
        if fe.cfFormat == self.cf_desc || fe.cfFormat == self.cf_cont {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        _pformatectin: *const FORMATETC,
        pformatetcout: *mut FORMATETC,
    ) -> windows::core::HRESULT {
        unsafe {
            if !pformatetcout.is_null() {
                (*pformatetcout).ptd = std::ptr::null_mut();
            }
        }
        DATA_S_SAMEFORMATETC
    }

    fn SetData(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *const STGMEDIUM,
        _frelease: BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, dwdirection: u32) -> windows::core::Result<IEnumFORMATETC> {
        if dwdirection != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        let fe = |cf: u16| FORMATETC {
            cfFormat: cf,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        let formats = [fe(self.cf_desc), fe(self.cf_cont)];
        unsafe { SHCreateStdEnumFmtEtc(&formats) }
    }

    fn DAdvise(
        &self,
        _pformatetc: *const FORMATETC,
        _advf: u32,
        _padvsink: Option<&IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _dwconnection: u32) -> windows::core::Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

#[test]
#[ignore = "システムのクリップボードを書き換えるため手動実行 (-- --ignored)"]
fn virtual_clipboard_paste_end_to_end() {
    unsafe {
        OleInitialize(None).ok().expect("OleInitialize");
    }

    // 1. 仮想ファイル 3 件 (フォルダ + 中身 + トップレベル) をクリップボードへ
    let descriptor = build_descriptor_bytes(&[
        ("dir", true, None),
        ("dir\\inner.txt", false, Some(11)),
        ("top.txt", false, None), // FD_FILESIZE なし (サイズ不明も受ける)
    ]);
    let source = VirtualClipSource {
        descriptor,
        contents: vec![
            None,
            Some(b"hello inner".to_vec()),
            Some(b"hello top".to_vec()),
        ],
        cf_desc: cf_descriptor(),
        cf_cont: cf_contents(),
    };
    let data: IDataObject = source.into();
    unsafe { OleSetClipboard(&data).expect("OleSetClipboard") };

    let result = std::panic::catch_unwind(|| {
        // 2. 記述子の読取 (can_paste 判定と計画に使う経路)
        assert!(virtual_files_available());
        let entries = clipboard_read_virtual_entries()
            .expect("read descriptors")
            .expect("virtual entries present");
        assert_eq!(entries.len(), 3);
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].rel_path, "dir");
        assert_eq!(entries[1].rel_path, "dir\\inner.txt");
        assert_eq!(entries[1].size, Some(11));
        assert_eq!(entries[2].rel_path, "top.txt");
        assert_eq!(entries[2].size, None);

        // 3. 抽出ジョブ (UI の貼り付けと同じ経路)
        let tmp = tempfile::tempdir().expect("tempdir");
        let items: Vec<VirtualPasteItem> = entries
            .iter()
            .map(|e| VirtualPasteItem {
                index: e.index,
                rel_path: e.rel_path.clone(),
                is_dir: e.is_dir,
                size: e.size,
            })
            .collect();
        let reg = JobRegistry::default();
        let sink = |_: &str, _: serde_json::Value| {};
        reg.run_virtual_paste(&sink, 1, tmp.path().to_path_buf(), items)
            .expect("run_virtual_paste");

        assert!(tmp.path().join("dir").is_dir());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("dir").join("inner.txt")).unwrap(),
            "hello inner"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("top.txt")).unwrap(),
            "hello top"
        );
    });

    // 4. 掃除 (自前のデータオブジェクトをクリップボードから外す)
    unsafe {
        let _ = OleSetClipboard(None);
        OleUninitialize();
    }
    if let Err(p) = result {
        std::panic::resume_unwind(p);
    }
}
