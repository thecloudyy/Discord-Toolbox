; -- Discord-Toolbox.iss --
; Inno Setup 7 script for packaging the Tauri-built exe.
; Build the exe first, then run:
;   iscc installer.iss /DExePath="...\discord-toolbox.exe"

#define ExePath = "target/release/discord-toolbox.exe"

[Setup]
AppName=Discord Toolbox
AppVersion=1.0.4
AppPublisher=thecloudyy
AppCopyright=Copyright (c) 2026 thecloudyy
DefaultDirName={autopf}\Discord Toolbox
DefaultGroupName=Discord Toolbox
OutputBaseFilename=Discord-Toolbox-setup
SetupIconFile=icons\icon.ico
UninstallDisplayIcon={app}\discord-toolbox.exe
Compression=lzma
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64 arm64
PrivilegesRequired=lowest

[InstallDelete]
Type: files; Name: "{app}\discord-keep-alive.exe"
Type: files; Name: "{app}\vc-keeper-bot.exe"

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; Flags: ignoreversion
Source: "bin\discord-toolbox-bot.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Discord Toolbox"; Filename: "{app}\discord-toolbox.exe"
Name: "{autodesktop}\Discord Toolbox"; Filename: "{app}\discord-toolbox.exe"

[Run]
Filename: "{app}\discord-toolbox.exe"; Description: "Launch Discord Toolbox"; Flags: nowait postinstall skipifsilent

[Code]
function InitializeSetup(): Boolean;
begin
  Result := True;
end;