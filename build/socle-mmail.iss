; Installeur Windows de MMail — socle Rust / Qt 6 (Inno Setup 6).
;
; Compilation :
;   ISCC.exe /DAppVersion=0.1.0 build\socle-mmail.iss
;
; Prérequis : dist\MMail\ doit contenir MMail.exe et les bibliothèques Qt
; déposées par windeployqt (cf. le workflow « socle », étape « Paquet Windows »).
; Sortie    : dist\MMail-<version>-setup.exe
;
; L'AppId est propre à MMail — distinct de celui de MMdedit, dont ce script est
; issu : installer l'un ne remplace jamais l'autre. Aucune association de
; fichiers : un client de messagerie n'ouvre pas de documents, et le lien
; « mailto: » attendra la rédaction.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#define AppName        "MMail"
#define AppPublisher   "M-Media"
#define AppExe         "MMail.exe"

[Setup]
AppId={{2F1D7A94-6C83-4E51-9B0A-7D3E5C8F1A62}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName} {#AppVersion}
OutputDir=..\dist
OutputBaseFilename={#AppName}-{#AppVersion}-setup
SetupIconFile=..\assets\mmail.ico
; Licence presentee a l'installation (exigence morale du copyleft : l'utilisateur
; doit savoir sous quels termes il recoit le programme).
LicenseFile=..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; Installation dans le profil par défaut (aucune élévation exigée) ; l'assistant
; propose l'installation pour tous les utilisateurs si on dispose de l'admin.
PrivilegesRequired=lowest
; « commandline » déclare /ALLUSERS et /CURRENTUSER recevables en ligne de
; commande : tout déploiement automatisé en dépend (il s'exécute sous SYSTEM et
; exige une installation pour la machine entière).
PrivilegesRequiredOverridesAllowed=commandline dialog
ArchitecturesInstallIn64BitMode=x64compatible
ArchitecturesAllowed=x64compatible
DisableProgramGroupPage=yes
ShowLanguageDialog=no

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "Créer un raccourci sur le &Bureau"; \
    GroupDescription: "Raccourcis :"; Flags: unchecked

[Files]
; Le dossier entier : le programme, les bibliothèques Qt et les greffons QML.
Source: "..\dist\MMail\*"; DestDir: "{app}"; \
    Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\README.md";      DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "..\LICENSE";        DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}";  Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "Lancer {#AppName}"; \
    Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: dirifempty; Name: "{app}"
