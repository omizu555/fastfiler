; FastFiler Windows インストーラ (Inno Setup)
;
; ビルド:
;   1. https://jrsoftware.org/isinfo.php から Inno Setup 6 をインストール
;   2. cargo build -p fastfiler-native --release   (target\release\fastfiler-native.exe を生成)
;   3. iscc installer\fastfiler.iss
;   4. installer\out\FastFilerSetup-0.1.0.exe が生成される
;
; 詳細は doc\RELEASE.md §5 を参照。

#define AppName        "FastFiler"
#define AppVersion     "0.1.0"
#define AppPublisher   "omizu555"
#define AppURL         "https://github.com/omizu555/fastfiler"
#define AppExeName     "fastfiler-native.exe"
#define AppId          "{{B7E1A6F0-7E81-4D77-8F4C-FASTFILER0001}"

[Setup]
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}
AppUpdatesURL={#AppURL}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
OutputDir=out
OutputBaseFilename=FastFilerSetup-{#AppVersion}
SetupIconFile=..\crates\fastfiler-native\assets\icon.ico
UninstallDisplayIcon={app}\{#AppExeName}
Compression=lzma2/ultra
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
MinVersion=10.0
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; ユーザー領域 (=%LOCALAPPDATA%\Programs\FastFiler) インストールにも対応

[Languages]
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "english";  MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md";                    DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "..\CONTEXT.md";                   DestDir: "{app}"; Flags: ignoreversion
Source: "..\doc\USAGE.md";                 DestDir: "{app}\doc"; Flags: ignoreversion
Source: "..\doc\STATUS.md";                DestDir: "{app}\doc"; Flags: ignoreversion
Source: "..\doc\BUILD.md";                 DestDir: "{app}\doc"; Flags: ignoreversion
Source: "..\doc\ARCHITECTURE.md";          DestDir: "{app}\doc"; Flags: ignoreversion
Source: "..\doc\RELEASE.md";               DestDir: "{app}\doc"; Flags: ignoreversion
Source: "..\doc\IDEAS.md";                 DestDir: "{app}\doc"; Flags: ignoreversion
Source: "..\doc\adr\*.md";                 DestDir: "{app}\doc\adr"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}";          Filename: "{app}\{#AppExeName}"
Name: "{group}\使い方";              Filename: "{app}\doc\USAGE.md"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}";    Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; 設定・ログは %APPDATA%\FastFiler\ に置かれるが、ユーザーデータなので
; アンインストール時には削除しない方針 (再インストール時の継承を優先)。
