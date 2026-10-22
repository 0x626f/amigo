; Inno Setup 6.3+; definitions are supplied by scripts/distribute.py.
[Setup]
AppId=io.github.0x626f.amigo
AppName=Amigo
AppVersion={#AppVersion}
AppPublisher=0x626f
AppPublisherURL=https://github.com/0x626f/amigo
DefaultDirName={localappdata}\Programs\Amigo
DefaultGroupName=Amigo
PrivilegesRequired=lowest
ArchitecturesAllowed={#AppArchitecture}
ArchitecturesInstallIn64BitMode={#AppArchitecture}
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
SetupIconFile={#StageDir}\amigo.ico
UninstallDisplayIcon={app}\amigo.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
DisableProgramGroupPage=yes

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\Amigo"; Filename: "{app}\amigo.exe"; WorkingDir: "{app}"; IconFilename: "{app}\amigo.ico"

[Run]
Filename: "{app}\amigo.exe"; Description: "Launch Amigo"; Flags: nowait postinstall skipifsilent
