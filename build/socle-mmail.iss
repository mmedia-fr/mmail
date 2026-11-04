; Installeur Windows de MMail — socle Rust / Qt 6 (Inno Setup 6).
;
; Compilation :
;   ISCC.exe /DAppVersion=0.1.1 build\socle-mmail.iss
;
; Prérequis : dist\MMail\ doit contenir MMail.exe et les bibliothèques Qt
; déposées par windeployqt (cf. le workflow « socle », étape « Paquet Windows »).
; Sortie    : dist\MMail-<version>-setup.exe
;
; L'AppId est propre à MMail — distinct de celui de MMdedit, dont ce script est
; issu : installer l'un ne remplace jamais l'autre. Aucune association de
; fichiers : un client de messagerie n'ouvre pas de documents.
;
; Messagerie de Windows (section [Registry]) : MMail se déclare gestionnaire
; possible des liens « mailto: » (Applications par défaut de Windows) et, pour
; la machine seulement, messagerie MAPI — ce que les autres logiciels
; appellent pour créer un courriel (« Envoyer vers › Destinataire »…, cf.
; mapi/mmail_mapi.c). Le choix reste à la personne : Réglages › Messagerie par
; défaut (cf. cpp/messagerie.h) ou les réglages de Windows.

#ifndef AppVersion
  #define AppVersion "0.1.1"
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
; En silence, Inno Setup ferme d'office les applications qui tiennent un
; fichier à remplacer (gestionnaire de redémarrage). Le fournisseur MAPI est
; chargé par l'Explorateur ou le logiciel qui a créé un courriel : les fermer
; en pleine journée n'est pas admissible. Les DLL sont donc hors du filtre ;
; celle de MAPI, en service, est écartée sous un autre nom (cf.
; EcarterFournisseurMapi), et MMail.exe, lui, reste surveillé.
CloseApplicationsFilter=*.exe,*.chm
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
; Fournisseur MAPI pour la machine (cf. mapi/mmail_mapi.c) : la clé
; Clients\Mail est commune aux processus 32 et 64 bits, DLLPath désigne donc
; System32, et Windows sert aux processus 32 bits la copie de SysWOW64, du
; même nom. Installation pour tous les utilisateurs seulement : une
; installation par utilisateur ne peut pas écrire dans ces dossiers.
Source: "..\dist\MMail\MMailMAPI64.dll"; DestDir: "{sys}"; DestName: "MMailMAPI.dll"; \
    Flags: ignoreversion uninsrestartdelete; Check: IsAdminInstallMode; \
    BeforeInstall: EcarterFournisseurMapi
Source: "..\dist\MMail\MMailMAPI32.dll"; DestDir: "{syswow64}"; DestName: "MMailMAPI.dll"; \
    Flags: ignoreversion uninsrestartdelete; Check: IsAdminInstallMode; \
    BeforeInstall: EcarterFournisseurMapi
Source: "..\LICENSE";        DestDir: "{app}"; Flags: ignoreversion
; Personnalisation à l'installation (0.8.6) : un dossier « personnalisation »
; posé à côté de l'installeur — icône, logo, couleurs d'une organisation —
; est copié avec MMail. Absent, rien ne change ; une mise à jour, lancée sans
; lui, garde celui déjà installé.
Source: "{src}\personnalisation\*"; DestDir: "{app}\personnalisation"; \
    Flags: external skipifsourcedoesntexist recursesubdirs createallsubdirs ignoreversion

[Icons]
; Avec l'icône de l'organisation si elle en fournit une (0.8.6).
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; Check: not IconePersonnalisee
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; \
    IconFilename: "{app}\personnalisation\icone.ico"; Check: IconePersonnalisee
Name: "{autodesktop}\{#AppName}";  Filename: "{app}\{#AppExe}"; Tasks: desktopicon; Check: not IconePersonnalisee
Name: "{autodesktop}\{#AppName}";  Filename: "{app}\{#AppExe}"; Tasks: desktopicon; \
    IconFilename: "{app}\personnalisation\icone.ico"; Check: IconePersonnalisee

[Run]
Filename: "{app}\{#AppExe}"; Description: "Lancer {#AppName}"; \
    Flags: nowait postinstall skipifsilent
; Mise à jour lancée par MMail pour se rouvrir ensuite (« Redémarrer MMail ») :
; relancé sous le compte de la personne, même si l'installation a été élevée.
Filename: "{app}\{#AppExe}"; Flags: nowait runasoriginaluser; Check: RelancerDemande

[Registry]
; Liens « mailto: » : MMail proposé dans les Applications par défaut de
; Windows (pour la session ou la machine, selon l'installation). Windows
; réserve le choix à la personne ; « --mailto » ouvre une rédaction (cf.
; cpp/main.cpp). Les capacités ne vont pas sous Clients\Mail\MMail : par
; utilisateur, une clé de ce nom sans DLL pourrait masquer celle de la machine.
Root: HKA; Subkey: "Software\Classes\MMail.mailto"; ValueType: string; ValueName: ""; \
    ValueData: "Adresse de courriel (MMail)"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\MMail.mailto"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\MMail.mailto\DefaultIcon"; ValueType: string; ValueName: ""; \
    ValueData: "{app}\{#AppExe},0"
Root: HKA; Subkey: "Software\Classes\MMail.mailto\shell\open\command"; ValueType: string; ValueName: ""; \
    ValueData: """{app}\{#AppExe}"" --mailto ""%1"""
Root: HKA; Subkey: "Software\M-Media\Integration"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\M-Media\Integration\Capabilities"; ValueType: string; ValueName: "ApplicationName"; \
    ValueData: "MMail"
Root: HKA; Subkey: "Software\M-Media\Integration\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; \
    ValueData: "Messagerie MMail"
Root: HKA; Subkey: "Software\M-Media\Integration\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; \
    ValueData: "{app}\{#AppExe},0"
Root: HKA; Subkey: "Software\M-Media\Integration\Capabilities\URLAssociations"; ValueType: string; \
    ValueName: "mailto"; ValueData: "MMail.mailto"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "MMail"; \
    ValueData: "Software\M-Media\Integration\Capabilities"; Flags: uninsdeletevalue
; Messagerie MAPI de la machine : MMail figure parmi les messageries que l'on
; peut choisir par défaut, sans le devenir d'office.
Root: HKLM; Subkey: "SOFTWARE\Clients\Mail\MMail"; ValueType: string; ValueName: ""; ValueData: "MMail"; \
    Flags: uninsdeletekey; Check: IsAdminInstallMode
Root: HKLM; Subkey: "SOFTWARE\Clients\Mail\MMail"; ValueType: string; ValueName: "DLLPath"; \
    ValueData: "{sys}\MMailMAPI.dll"; Check: IsAdminInstallMode
Root: HKLM; Subkey: "SOFTWARE\Clients\Mail\MMail\DefaultIcon"; ValueType: string; ValueName: ""; \
    ValueData: "{app}\{#AppExe},0"; Check: IsAdminInstallMode
Root: HKLM; Subkey: "SOFTWARE\Clients\Mail\MMail\shell\open\command"; ValueType: string; ValueName: ""; \
    ValueData: """{app}\{#AppExe}"""; Check: IsAdminInstallMode

[UninstallDelete]
Type: dirifempty; Name: "{app}"
Type: files; Name: "{sys}\MMailMAPI.dll.ancien*"
Type: files; Name: "{syswow64}\MMailMAPI.dll.ancien*"

[Code]
// Mise à jour assistée (cf. cpp/installeur.h) : MMail lance cet installeur en
// se fermant, avec /ATTENDRE=<son numéro de processus>. Ses fichiers ne se
// remplacent qu'une fois le programme arrêté : on attend sa fin, deux minutes
// au plus — au-delà, l'installation se poursuit et Windows dira ce qui reste
// ouvert.
const
  SYNCHRONIZE = $00100000;

function OpenProcess(Acces: Cardinal; Heritage: Longint; Processus: Longint): THandle;
  external 'OpenProcess@kernel32.dll stdcall';
function WaitForSingleObject(Objet: THandle; Delai: Cardinal): Cardinal;
  external 'WaitForSingleObject@kernel32.dll stdcall';
function CloseHandle(Objet: THandle): Longint;
  external 'CloseHandle@kernel32.dll stdcall';

function InitializeSetup(): Boolean;
var
  Pid: Longint;
  Processus: THandle;
begin
  Pid := StrToIntDef(ExpandConstant('{param:ATTENDRE|0}'), 0);
  if Pid > 0 then
  begin
    Processus := OpenProcess(SYNCHRONIZE, 0, Pid);
    if Processus <> 0 then
    begin
      Log('Attente de la fin de MMail (processus ' + IntToStr(Pid) + ')');
      if WaitForSingleObject(Processus, 120000) = 0 then
        Log('MMail est fermé')
      else
        Log('MMail toujours ouvert après deux minutes');
      CloseHandle(Processus);
    end
    else
      Log('Processus ' + IntToStr(Pid) + ' déjà terminé');
  end;
  Result := True;
end;

// /RELANCER=1 : MMail se rouvre une fois installé.
function RelancerDemande(): Boolean;
begin
  Result := ExpandConstant('{param:RELANCER|0}') = '1';
end;

// Fournisseur MAPI chargé dans un processus — l'Explorateur, après un
// « Envoyer vers › Destinataire » : Windows refuse de l'écraser, mais permet
// de le renommer. Il est écarté sous un autre nom, que l'installation
// suivante (ou la désinstallation) efface ; jamais de redémarrage demandé.
procedure EcarterFournisseurMapi();
var
  Chemin, Ancien: String;
  I: Integer;
  Redirection: Boolean;
begin
  Redirection := EnableFsRedirection(False);
  try
    Chemin := ExpandConstant(CurrentFileName);
    for I := 1 to 20 do
    begin
      Ancien := Chemin + '.ancien' + IntToStr(I);
      if FileExists(Ancien) then
        DeleteFile(Ancien);
    end;
    if FileExists(Chemin) and DeleteFile(Chemin) then
      Log('Fournisseur MAPI remplacé : ' + Chemin)
    else if FileExists(Chemin) then
      for I := 1 to 20 do
      begin
        Ancien := Chemin + '.ancien' + IntToStr(I);
        if not FileExists(Ancien) then
        begin
          if RenameFile(Chemin, Ancien) then
            Log('Fournisseur MAPI en service écarté : ' + Ancien)
          else
            Log('Fournisseur MAPI en service, impossible à écarter : ' + Chemin);
          Break;
        end;
      end;
  finally
    EnableFsRedirection(Redirection);
  end;
end;

// Désinstallation : si MMail était la messagerie choisie pour cette session,
// le choix revient à celui de la machine plutôt que de désigner un absent.
procedure CurUninstallStepChanged(Etape: TUninstallStep);
var
  Choix: String;
begin
  if Etape = usPostUninstall then
    if RegQueryStringValue(HKCU, 'Software\Clients\Mail', '', Choix) and (CompareText(Choix, 'MMail') = 0) then
      RegDeleteValue(HKCU, 'Software\Clients\Mail', '');
end;

// Personnalisation à l'installation (0.8.6) : l'icône de l'organisation, une
// fois les fichiers copiés, sert aux raccourcis.
function IconePersonnalisee(): Boolean;
begin
  Result := FileExists(ExpandConstant('{app}\personnalisation\icone.ico'));
end;
