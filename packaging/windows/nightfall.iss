; NIGHTFALL Browser - Windows installer (Inno Setup 6, free).
; Built in CI: iscc /DAppVersion=x.y.z /DSourceDir=..\..\dist\NIGHTFALL packaging\windows\nightfall.iss
; Per-user install: no administrator rights needed. Uninstall removes the
; program files only; the encrypted profile in %LOCALAPPDATA%\NIGHTFALL is kept
; unless the user chooses to delete it.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\dist\NIGHTFALL"
#endif

[Setup]
AppId={{6E1F4C1A-9B57-4C59-9A3B-4E1A2F0B7D11}
AppName=NIGHTFALL
AppVersion={#AppVersion}
AppPublisher=NIGHTFALL contributors
AppPublisherURL=https://github.com/BatStream-off/NIGHTFALL
DefaultDirName={localappdata}\Programs\NIGHTFALL
DefaultGroupName=NIGHTFALL
PrivilegesRequired=lowest
OutputDir=..\..\dist
OutputBaseFilename=NIGHTFALL-{#AppVersion}-windows-x64-setup
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE
UninstallDisplayName=NIGHTFALL Browser
MinVersion=10.0
SetupIconFile=..\..\app\resources\win\nightfall.ico
UninstallDisplayIcon={app}\nightfall.exe
; Automatic updates run this installer silently: close NIGHTFALL first.
CloseApplications=force
RestartApplications=no

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "Créer une icône sur le Bureau"; GroupDescription: "Raccourcis :"

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Registry]
; NIGHTFALL listed as a web browser (Settings > Apps > Default apps). Nothing
; is made the default here: the user chooses it in Windows' own settings.
Root: HKCU; Subkey: "Software\Classes\NIGHTFALLURL"; ValueType: string; ValueName: ""; ValueData: "Adresse web NIGHTFALL"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\NIGHTFALLURL"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\NIGHTFALLURL\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\nightfall.exe,0"
Root: HKCU; Subkey: "Software\Classes\NIGHTFALLURL\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\nightfall.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\NIGHTFALL"; ValueType: string; ValueName: ""; ValueData: "NIGHTFALL"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\NIGHTFALL\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\nightfall.exe,0"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\NIGHTFALL\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\nightfall.exe"""
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\NIGHTFALL\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "NIGHTFALL"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\NIGHTFALL\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Navigateur sécurisé NIGHTFALL"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\NIGHTFALL\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\nightfall.exe,0"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\NIGHTFALL\Capabilities\URLAssociations"; ValueType: string; ValueName: "http"; ValueData: "NIGHTFALLURL"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\NIGHTFALL\Capabilities\URLAssociations"; ValueType: string; ValueName: "https"; ValueData: "NIGHTFALLURL"
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "NIGHTFALL"; ValueData: "Software\Clients\StartMenuInternet\NIGHTFALL\Capabilities"; Flags: uninsdeletevalue

[Icons]
Name: "{group}\NIGHTFALL"; Filename: "{app}\nightfall.exe"
Name: "{userdesktop}\NIGHTFALL"; Filename: "{app}\nightfall.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\nightfall.exe"; Description: "Lancer NIGHTFALL"; Flags: nowait postinstall skipifsilent
; Relaunch after an automatic update started with "Redémarrer pour mettre à jour".
Filename: "{app}\nightfall.exe"; Flags: nowait; Check: ShouldRelaunch

[Code]
function ShouldRelaunch: Boolean;
begin
  Result := Pos('/NFRELAUNCH', UpperCase(GetCmdTail)) > 0;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    if MsgBox('Supprimer aussi vos données NIGHTFALL (profil chiffré : favoris, historique, mots de passe, sessions) ?' + #13#10 +
              'Cette action est irréversible. Vos téléchargements ne sont pas concernés.',
              mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      DelTree(ExpandConstant('{localappdata}\NIGHTFALL'), True, True, True);
  end;
end;
