; Codex Helper Windows 安装包
; 编译前请先: cargo build --release
; 或运行仓库根目录 build-installer.bat

#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif

#define MyAppName "Codex Helper"
#define MyAppPublisher "Codex Helper"
#define MyAppExeName "codex-helper.exe"
#define MyAppURL "https://github.com/yourname/codex-helper"

[Setup]
AppId={{8F3C2A91-6D4E-4B2A-9C1F-7E5A0D3B8F62}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
DefaultDirName={autopf}\CodexHelper
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=CodexHelper-{#MyAppVersion}-Setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\assets\codex-helper.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=force
CloseApplicationsFilter=codex-helper.exe

[Languages]
Name: "chinesesimplified"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加选项:"; Flags: unchecked
Name: "startup"; Description: "登录 Windows 时自动启动"; GroupDescription: "附加选项:"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Parameters: "start"; Comment: "启动托盘与本地代理"
Name: "{group}\设置 API Key…"; Filename: "{app}\{#MyAppExeName}"; Parameters: "settings"; Comment: "打开设置窗口（需已启动）"
Name: "{group}\诊断环境"; Filename: "{app}\{#MyAppExeName}"; Parameters: "doctor"; Comment: "检查配置与代理"
Name: "{group}\卸载 {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Parameters: "start"; Tasks: desktopicon
Name: "{userstartup}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Parameters: "start"; Tasks: startup

[Run]
Filename: "{cmd}"; Parameters: "/C ie4uinit.exe -show"; Flags: runhidden nowait
Filename: "{app}\{#MyAppExeName}"; Parameters: "init"; StatusMsg: "正在初始化 Codex 配置…"; Flags: runhidden waituntilterminated postinstall
Filename: "{app}\{#MyAppExeName}"; Parameters: "start"; Description: "启动 {#MyAppName}"; Flags: runhidden nowait postinstall skipifsilent

[UninstallRun]
Filename: "{cmd}"; Parameters: "/C taskkill /IM {#MyAppExeName} /F >NUL 2>&1"; Flags: runhidden; RunOnceId: "StopCodexHelper"
