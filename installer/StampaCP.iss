; Script di installazione per StampaCP (Inno Setup 6)
; Compilazione: ISCC.exe installer\StampaCP.iss  ->  dist\StampaCP-2.0.0-setup.exe

#define MyAppName "StampaCP"
#define MyAppVersion "2.0.0"
#define MyAppPublisher "Peppe CP"
#define MyAppURL "https://github.com/gsepp86/StampaCP"
#define MyAppExe "StampaCP.exe"

[Setup]
AppId={{6E1F3B2A-4C7D-4E8F-9A10-5B2C7D8E9F01}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=StampaCP-{#MyAppVersion}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
UninstallDisplayIcon={app}\{#MyAppExe}
UninstallDisplayName={#MyAppName}

[Languages]
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"

[Tasks]
Name: "desktopicon"; Description: "Crea un'icona sul desktop"; GroupDescription: "Icone aggiuntive:"; Flags: unchecked
Name: "autostart"; Description: "Avvia {#MyAppName} all'accensione del PC (serve alla stampante virtuale)"; GroupDescription: "Avvio automatico:"

[Files]
Source: "..\target\release\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "Installa-Stampante.ps1"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; IconFilename: "{app}\{#MyAppExe}"
Name: "{group}\Disinstalla {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon; IconFilename: "{app}\{#MyAppExe}"
Name: "{userstartup}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; Tasks: autostart; IconFilename: "{app}\{#MyAppExe}"

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\Installa-Stampante.ps1"""; StatusMsg: "Installazione della stampante virtuale {#MyAppName}..."; Flags: runhidden waituntilterminated
Filename: "{app}\{#MyAppExe}"; Description: "Avvia {#MyAppName}"; Flags: nowait postinstall skipifsilent unchecked

