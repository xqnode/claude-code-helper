; Claude Code Helper Windows 安装包
; 编译前请先: cargo build --release
; 或运行仓库根目录 build-installer.bat

#ifndef MyAppVersion
  #define MyAppVersion "0.2.1"
#endif

#define MyAppName "Claude Code Helper"
#define MyAppPublisher "Claude Code Helper"
#define MyAppExeName "claude-code-helper.exe"
#define MyAppURL "https://github.com/xqnode/claude-code-helper"

[Setup]
AppId={{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
DefaultDirName={autopf}\ClaudeCodeHelper
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=ClaudeCodeHelper-{#MyAppVersion}-Setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\assets\claude-code-helper.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=force
CloseApplicationsFilter=claude-code-helper.exe

[Languages]
; 使用仓库内语言包，避免 CI 静默安装 Inno Setup 时缺少 compiler:Languages\ChineseSimplified.isl
Name: "chinesesimplified"; MessagesFile: "languages\ChineseSimplified.isl"

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
Filename: "{app}\{#MyAppExeName}"; Parameters: "init"; StatusMsg: "正在初始化 Claude Code 配置…"; Flags: runhidden waituntilterminated postinstall
Filename: "{app}\{#MyAppExeName}"; Parameters: "start"; Description: "启动 {#MyAppName}"; Flags: runhidden nowait postinstall skipifsilent

[UninstallRun]
Filename: "{cmd}"; Parameters: "/C taskkill /IM {#MyAppExeName} /F >NUL 2>&1"; Flags: runhidden; RunOnceId: "StopClaudeCodeHelper"
