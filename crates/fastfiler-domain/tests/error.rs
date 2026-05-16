// Phase 2C-2: AppError の構造化 (kind()) と Display のスナップショット。
use fastfiler_domain::error::AppError;

#[test]
fn kind_returns_machine_readable_tag() {
    assert_eq!(AppError::Canceled.kind(), "canceled");
    assert_eq!(AppError::Plugin("x".into()).kind(), "plugin");
    assert_eq!(AppError::Win32("x".into()).kind(), "win32");
    assert_eq!(AppError::Parse("x".into()).kind(), "parse");
    assert_eq!(AppError::EnvMissing("APPDATA").kind(), "env_missing");
    assert_eq!(AppError::NotFound("x".into()).kind(), "not_found");
    assert_eq!(AppError::InvalidPath("x".into()).kind(), "invalid_path");
    assert_eq!(AppError::NotSupported("x".into()).kind(), "not_supported");
    assert_eq!(AppError::Watch("x".into()).kind(), "watch");
    assert_eq!(AppError::Other("x".into()).kind(), "other");
    let io = AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, "e"));
    assert_eq!(io.kind(), "io");
}

#[test]
fn display_messages_include_context() {
    assert_eq!(format!("{}", AppError::Canceled), "canceled");
    assert_eq!(
        format!("{}", AppError::Plugin("zip read".into())),
        "plugin error: zip read"
    );
    assert_eq!(
        format!("{}", AppError::EnvMissing("APPDATA")),
        "env var missing: APPDATA"
    );
}

#[test]
fn serialize_keeps_string_compatibility() {
    // フロント (TS) は invoke().catch(e => string) を仮定しているため、
    // シリアライズは文字列のままであることを保証する。
    let json = serde_json::to_string(&AppError::Canceled).unwrap();
    assert_eq!(json, "\"canceled\"");
    let json2 = serde_json::to_string(&AppError::Plugin("x".into())).unwrap();
    assert_eq!(json2, "\"plugin error: x\"");
}
