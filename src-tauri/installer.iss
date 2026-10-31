; -- Discord-Toolbox.iss --
; Inno Setup 7 script for packaging the Tauri-built exe.
; Build the exe first, then run:
;   iscc installer.iss /DExePath="...\discord-keep-alive.exe"

#define ExePath = "target/release/discord-keep-alive.exe"

[Setup]
AppName=Discord Toolbox
AppVersion=1.0.0
AppPublisher=vc-keeper
AppCopyright=Copyright (c) 2026 vc-keeper contributors
DefaultDirName={autopf}\Discord Toolbox
DefaultGroupName=Discord Toolbox
OutputBaseFilename=Discord-Toolbox-setup
SetupIconFile=icons\icon.ico
UninstallDisplayIcon={app}\discord-keep-alive.exe
Compression=lzma
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64 arm64
PrivilegesRequired=lowest

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; Flags: ignoreversion
Source: "bin\vc-keeper-bot.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Discord Toolbox"; Filename: "{app}\discord-keep-alive.exe"
Name: "{autodesktop}\Discord Toolbox"; Filename: "{app}\discord-keep-alive.exe"

[Run]
Filename: "{app}\discord-keep-alive.exe"; Description: "Launch Discord Toolbox"; Flags: nowait postinstall skipifsilent

[Code]
function InitializeSetup(): Boolean;
begin
  Result := True;
end;