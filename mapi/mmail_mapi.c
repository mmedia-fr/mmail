// SPDX-License-Identifier: GPL-3.0-or-later
//
// Fournisseur MAPI simple de MMail (Windows) : ce qui permet à un autre
// logiciel de créer un courriel dans MMail — « Envoyer vers › Destinataire »
// de l'Explorateur, l'export d'un état « par courriel » d'un logiciel de
// gestion, tout ce qui passe par MAPISendMail.
//
// Comment Windows arrive ici : le logiciel appelle MAPISendMail dans
// mapi32.dll, le « stub » de Windows, qui lit la messagerie par défaut
// (HKCU\Software\Clients\Mail, à défaut HKLM) puis la valeur DLLPath de
// HKLM\Software\Clients\Mail\MMail, et charge cette DLL dans le processus du
// logiciel. Cette clé est commune aux processus 32 et 64 bits (Windows 7 et
// suivants) : DLLPath vaut donc C:\Windows\System32\MMailMAPI.dll, où l'on
// dépose la DLL 64 bits, et Windows sert aux processus 32 bits celle de
// SysWOW64, du même nom (redirection du système de fichiers). D'où deux
// compilations du même source (cf. build/socle-mmail.iss et la CI).
//
// Ce que fait la DLL : elle ne rédige ni n'envoie rien elle-même. Elle copie
// les pièces jointes — le logiciel appelant efface souvent son fichier
// temporaire dès le retour —, écrit le message dans un fichier JSON et lance
// « MMail.exe --rediger <fichier> ». MMail ouvre une rédaction remplie (un
// MMail déjà ouvert la reçoit par son canal d'instance unique), puis efface
// le fichier : c'est le signe que la DLL attend avant de rendre la main.
// La personne relit, complète le destinataire et envoie : MAPI_DIALOG ou non,
// rien ne part sans elle.
//
// Pas de bibliothèque C d'exécution partagée : la DLL est chargée dans des
// processus qui ne l'ont pas forcément (compilée en /MT, cf. la CI).

#define WIN32_LEAN_AND_MEAN
#define UNICODE
#define _UNICODE
#include <windows.h>

#include <stdarg.h>
#include <stdio.h>
#include <wchar.h>

// ---- Types et codes de MAPI simple (mapi.h, recopiés pour ne dépendre
// d'aucun SDK particulier) ----------------------------------------------------

typedef ULONG_PTR LHANDLE, *LPLHANDLE;
typedef ULONG FLAGS;

typedef struct {
  ULONG ulReserved;
  ULONG ulRecipClass;
  LPSTR lpszName;
  LPSTR lpszAddress;
  ULONG ulEIDSize;
  LPVOID lpEntryID;
} MapiRecipDesc, *lpMapiRecipDesc;

typedef struct {
  ULONG ulReserved;
  ULONG flFlags;
  ULONG nPosition;
  LPSTR lpszPathName;
  LPSTR lpszFileName;
  LPVOID lpFileType;
} MapiFileDesc, *lpMapiFileDesc;

typedef struct {
  ULONG ulReserved;
  LPSTR lpszSubject;
  LPSTR lpszNoteText;
  LPSTR lpszMessageType;
  LPSTR lpszDateReceived;
  LPSTR lpszConversationID;
  FLAGS flFlags;
  lpMapiRecipDesc lpOriginator;
  ULONG nRecipCount;
  lpMapiRecipDesc lpRecips;
  ULONG nFileCount;
  lpMapiFileDesc lpFiles;
} MapiMessage, *lpMapiMessage;

typedef struct {
  ULONG ulReserved;
  ULONG ulRecipClass;
  PWSTR lpszName;
  PWSTR lpszAddress;
  ULONG ulEIDSize;
  PVOID lpEntryID;
} MapiRecipDescW, *lpMapiRecipDescW;

typedef struct {
  ULONG ulReserved;
  ULONG flFlags;
  ULONG nPosition;
  PWSTR lpszPathName;
  PWSTR lpszFileName;
  PVOID lpFileType;
} MapiFileDescW, *lpMapiFileDescW;

typedef struct {
  ULONG ulReserved;
  PWSTR lpszSubject;
  PWSTR lpszNoteText;
  PWSTR lpszMessageType;
  PWSTR lpszDateReceived;
  PWSTR lpszConversationID;
  FLAGS flFlags;
  lpMapiRecipDescW lpOriginator;
  ULONG nRecipCount;
  lpMapiRecipDescW lpRecips;
  ULONG nFileCount;
  lpMapiFileDescW lpFiles;
} MapiMessageW, *lpMapiMessageW;

#define SUCCESS_SUCCESS 0
#define MAPI_E_FAILURE 2
#define MAPI_E_INSUFFICIENT_MEMORY 5
#define MAPI_E_ATTACHMENT_NOT_FOUND 11
#define MAPI_E_ATTACHMENT_OPEN_FAILURE 12
#define MAPI_E_UNKNOWN_RECIPIENT 14
#define MAPI_E_NOT_SUPPORTED 26
#define MAPI_E_INVALID_MESSAGE 17

#define MAPI_TO 1
#define MAPI_CC 2
#define MAPI_BCC 3

#define MAPI_DIALOG 0x00000008

// Session rendue par MAPILogon : une valeur quelconque, non nulle.
#define SESSION_MMAIL ((LHANDLE)0x4D4D4149)

// AppId de l'installeur (build/socle-mmail.iss) : la clé de désinstallation
// donne le dossier de MMail, par utilisateur ou pour la machine.
#define CLE_DESINSTALLATION L"Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{2F1D7A94-6C83-4E51-9B0A-7D3E5C8F1A62}_is1"

// Délai laissé à MMail pour prendre le message : un démarrage à froid sur un
// serveur de bureaux à distance chargé dépasse parfois la dizaine de secondes.
#define ATTENTE_MS 60000
// Dossiers d'envoi plus anciens retirés au passage suivant.
#define CONSERVATION_JOURS 14

static HMODULE s_module = NULL;

BOOL WINAPI DllMain(HINSTANCE module, DWORD raison, LPVOID reserve)
{
  (void)reserve;
  if (raison == DLL_PROCESS_ATTACH) {
    s_module = module;
    DisableThreadLibraryCalls(module);
  }
  return TRUE;
}

// ---- Petits outils -----------------------------------------------------------

static void* allouer(size_t taille)
{
  return HeapAlloc(GetProcessHeap(), HEAP_ZERO_MEMORY, taille);
}

static void liberer(void* p)
{
  if (p)
    HeapFree(GetProcessHeap(), 0, p);
}

// Chaîne de la page de code ANSI (MAPISendMail) en UTF-16, allouée.
static wchar_t* depuis_ansi(const char* s)
{
  if (!s)
    return NULL;
  int n = MultiByteToWideChar(CP_ACP, 0, s, -1, NULL, 0);
  if (n <= 0)
    return NULL;
  wchar_t* w = (wchar_t*)allouer((size_t)n * sizeof(wchar_t));
  if (w)
    MultiByteToWideChar(CP_ACP, 0, s, -1, w, n);
  return w;
}

static wchar_t* copie(const wchar_t* s)
{
  if (!s)
    return NULL;
  size_t n = wcslen(s) + 1;
  wchar_t* w = (wchar_t*)allouer(n * sizeof(wchar_t));
  if (w)
    memcpy(w, s, n * sizeof(wchar_t));
  return w;
}

// Tampon de texte UTF-16 extensible.
typedef struct {
  wchar_t* t;
  size_t n;
  size_t cap;
  int erreur;
} Tampon;

static void ajouter(Tampon* b, const wchar_t* s, size_t n)
{
  if (b->erreur)
    return;
  if (b->n + n + 1 > b->cap) {
    size_t cap = b->cap ? b->cap : 1024;
    while (b->n + n + 1 > cap)
      cap *= 2;
    wchar_t* t = (wchar_t*)allouer(cap * sizeof(wchar_t));
    if (!t) {
      b->erreur = 1;
      return;
    }
    if (b->t) {
      memcpy(t, b->t, b->n * sizeof(wchar_t));
      liberer(b->t);
    }
    b->t = t;
    b->cap = cap;
  }
  memcpy(b->t + b->n, s, n * sizeof(wchar_t));
  b->n += n;
  b->t[b->n] = 0;
}

static void ajouter_z(Tampon* b, const wchar_t* s)
{
  ajouter(b, s, wcslen(s));
}

// Chaîne JSON : guillemets, barres obliques inverses et caractères de contrôle
// échappés ; le reste tel quel (le fichier est écrit en UTF-8).
static void ajouter_json(Tampon* b, const wchar_t* s)
{
  ajouter(b, L"\"", 1);
  for (; s && *s; ++s) {
    wchar_t c = *s;
    if (c == L'"')
      ajouter(b, L"\\\"", 2);
    else if (c == L'\\')
      ajouter(b, L"\\\\", 2);
    else if (c == L'\n')
      ajouter(b, L"\\n", 2);
    else if (c == L'\r')
      ajouter(b, L"\\r", 2);
    else if (c == L'\t')
      ajouter(b, L"\\t", 2);
    else if (c < 0x20) {
      wchar_t e[8];
      swprintf(e, 8, L"\\u%04x", (unsigned)c);
      ajouter_z(b, e);
    } else
      ajouter(b, &c, 1);
  }
  ajouter(b, L"\"", 1);
}

static BOOL existe_fichier(const wchar_t* chemin)
{
  DWORD a = GetFileAttributesW(chemin);
  return a != INVALID_FILE_ATTRIBUTES && !(a & FILE_ATTRIBUTE_DIRECTORY);
}

// Dossier de travail : %LOCALAPPDATA%\MMail\mapi (à défaut, le dossier
// temporaire). Rend la longueur écrite, 0 en cas d'échec.
static size_t dossier_mapi(wchar_t* d, size_t cap)
{
  DWORD n = GetEnvironmentVariableW(L"LOCALAPPDATA", d, (DWORD)cap);
  if (n == 0 || n >= cap) {
    n = GetTempPathW((DWORD)cap, d);
    if (n == 0 || n >= cap)
      return 0;
  }
  size_t l = wcslen(d);
  if (l > 0 && d[l - 1] == L'\\')
    d[--l] = 0;
  if (swprintf(d + l, cap - l, L"\\MMail") < 0)
    return 0;
  CreateDirectoryW(d, NULL);
  l = wcslen(d);
  if (swprintf(d + l, cap - l, L"\\mapi") < 0)
    return 0;
  CreateDirectoryW(d, NULL);
  return wcslen(d);
}

// Journal de la DLL, sans destinataire, objet ni nom de fichier : qui a appelé,
// en quelle architecture, avec quoi, et ce qui en est sorti.
static void journal(const wchar_t* format, ...)
{
  wchar_t d[MAX_PATH];
  if (!dossier_mapi(d, MAX_PATH))
    return;
  wchar_t chemin[MAX_PATH];
  if (swprintf(chemin, MAX_PATH, L"%ls\\journal.txt", d) < 0)
    return;
  // Taille bornée : au-delà de 256 Ko, l'ancien journal part en .old.
  WIN32_FILE_ATTRIBUTE_DATA info;
  if (GetFileAttributesExW(chemin, GetFileExInfoStandard, &info) && info.nFileSizeLow > 256 * 1024) {
    wchar_t ancien[MAX_PATH];
    if (swprintf(ancien, MAX_PATH, L"%ls\\journal.old.txt", d) >= 0)
      MoveFileExW(chemin, ancien, MOVEFILE_REPLACE_EXISTING);
  }
  wchar_t ligne[1024];
  SYSTEMTIME t;
  GetLocalTime(&t);
  wchar_t exe[MAX_PATH] = L"?";
  GetModuleFileNameW(NULL, exe, MAX_PATH);
  const wchar_t* nom = wcsrchr(exe, L'\\');
  int n = swprintf(ligne, 1024, L"%04u-%02u-%02u %02u:%02u:%02u %ls %ls ", t.wYear, t.wMonth, t.wDay,
                   t.wHour, t.wMinute, t.wSecond, sizeof(void*) == 8 ? L"x64" : L"x86", nom ? nom + 1 : exe);
  if (n < 0)
    return;
  va_list args;
  va_start(args, format);
  int m = vswprintf(ligne + n, 1024 - (size_t)n - 2, format, args);
  va_end(args);
  if (m < 0)
    return;
  wcscat_s(ligne, 1024, L"\r\n");
  char utf8[3072];
  int octets = WideCharToMultiByte(CP_UTF8, 0, ligne, -1, utf8, sizeof utf8, NULL, NULL);
  if (octets <= 1)
    return;
  HANDLE f = CreateFileW(chemin, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_ALWAYS,
                         FILE_ATTRIBUTE_NORMAL, NULL);
  if (f == INVALID_HANDLE_VALUE)
    return;
  DWORD ecrits;
  WriteFile(f, utf8, (DWORD)(octets - 1), &ecrits, NULL);
  CloseHandle(f);
}

// Retire les dossiers d'envoi de plus de CONSERVATION_JOURS jours : leurs
// pièces jointes ont servi (ou la rédaction a été abandonnée).
static void menage(const wchar_t* racine)
{
  wchar_t motif[MAX_PATH];
  if (swprintf(motif, MAX_PATH, L"%ls\\envoi-*", racine) < 0)
    return;
  FILETIME maintenant;
  GetSystemTimeAsFileTime(&maintenant);
  ULARGE_INTEGER limite;
  limite.LowPart = maintenant.dwLowDateTime;
  limite.HighPart = maintenant.dwHighDateTime;
  limite.QuadPart -= (ULONGLONG)CONSERVATION_JOURS * 24 * 3600 * 10000000ULL;
  WIN32_FIND_DATAW e;
  HANDLE h = FindFirstFileW(motif, &e);
  if (h == INVALID_HANDLE_VALUE)
    return;
  do {
    if (!(e.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY))
      continue;
    ULARGE_INTEGER date;
    date.LowPart = e.ftCreationTime.dwLowDateTime;
    date.HighPart = e.ftCreationTime.dwHighDateTime;
    if (date.QuadPart >= limite.QuadPart)
      continue;
    wchar_t dossier[MAX_PATH], fichiers[MAX_PATH];
    if (swprintf(dossier, MAX_PATH, L"%ls\\%ls", racine, e.cFileName) < 0
        || swprintf(fichiers, MAX_PATH, L"%ls\\*", dossier) < 0)
      continue;
    WIN32_FIND_DATAW f;
    HANDLE hf = FindFirstFileW(fichiers, &f);
    if (hf != INVALID_HANDLE_VALUE) {
      do {
        if (f.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY)
          continue;
        wchar_t fichier[MAX_PATH];
        if (swprintf(fichier, MAX_PATH, L"%ls\\%ls", dossier, f.cFileName) >= 0) {
          SetFileAttributesW(fichier, FILE_ATTRIBUTE_NORMAL);
          DeleteFileW(fichier);
        }
      } while (FindNextFileW(hf, &f));
      FindClose(hf);
    }
    RemoveDirectoryW(dossier);
  } while (FindNextFileW(h, &e));
  FindClose(h);
}

// Nom de fichier sans les caractères que Windows refuse.
static void assainir(wchar_t* nom)
{
  for (wchar_t* c = nom; *c; ++c) {
    if (*c < 0x20 || wcschr(L"\\/:*?\"<>|", *c))
      *c = L'_';
  }
  // Ni point ni espace en fin de nom.
  size_t n = wcslen(nom);
  while (n > 0 && (nom[n - 1] == L'.' || nom[n - 1] == L' '))
    nom[--n] = 0;
}

// Chemin de MMail.exe : MMAIL_EXE (essais), le dossier de la DLL, puis la clé
// de désinstallation de l'utilisateur, puis celle de la machine.
static BOOL trouver_mmail(wchar_t* exe, size_t cap)
{
  DWORD n = GetEnvironmentVariableW(L"MMAIL_EXE", exe, (DWORD)cap);
  if (n > 0 && n < cap && existe_fichier(exe))
    return TRUE;

  if (GetModuleFileNameW(s_module, exe, (DWORD)cap) > 0) {
    wchar_t* fin = wcsrchr(exe, L'\\');
    if (fin && (size_t)(fin - exe) + 12 < cap) {
      wcscpy_s(fin + 1, cap - (size_t)(fin + 1 - exe), L"MMail.exe");
      if (existe_fichier(exe))
        return TRUE;
    }
  }

  const struct {
    HKEY racine;
    REGSAM vue;
  } cles[] = { { HKEY_CURRENT_USER, 0 }, { HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY } };
  for (size_t i = 0; i < sizeof cles / sizeof cles[0]; ++i) {
    HKEY cle;
    if (RegOpenKeyExW(cles[i].racine, CLE_DESINSTALLATION, 0, KEY_READ | cles[i].vue, &cle) != ERROR_SUCCESS)
      continue;
    wchar_t dossier[MAX_PATH];
    DWORD taille = sizeof dossier, type = 0;
    LONG r = RegQueryValueExW(cle, L"InstallLocation", NULL, &type, (LPBYTE)dossier, &taille);
    RegCloseKey(cle);
    if (r != ERROR_SUCCESS || (type != REG_SZ && type != REG_EXPAND_SZ))
      continue;
    dossier[MAX_PATH - 1] = 0;
    size_t l = wcslen(dossier);
    if (l > 0 && dossier[l - 1] == L'\\')
      dossier[--l] = 0;
    if (swprintf(exe, cap, L"%ls\\MMail.exe", dossier) > 0 && existe_fichier(exe))
      return TRUE;
  }
  return FALSE;
}

// ---- Le message, quel que soit le jeu de caractères de l'appel ---------------

typedef struct {
  ULONG classe;
  wchar_t* nom;
  wchar_t* adresse;
} Destinataire;

typedef struct {
  wchar_t* chemin;
  wchar_t* nom;
} Piece;

typedef struct {
  wchar_t* objet;
  wchar_t* texte;
  ULONG nb_destinataires;
  Destinataire* destinataires;
  ULONG nb_pieces;
  Piece* pieces;
} Message;

static void liberer_message(Message* m)
{
  for (ULONG i = 0; i < m->nb_destinataires; ++i) {
    liberer(m->destinataires[i].nom);
    liberer(m->destinataires[i].adresse);
  }
  for (ULONG i = 0; i < m->nb_pieces; ++i) {
    liberer(m->pieces[i].chemin);
    liberer(m->pieces[i].nom);
  }
  liberer(m->destinataires);
  liberer(m->pieces);
  liberer(m->objet);
  liberer(m->texte);
  memset(m, 0, sizeof *m);
}

// Adresse d'un destinataire : « SMTP:x@y » → « x@y » ; à défaut d'adresse, le
// nom (souvent l'adresse elle-même) ; « Nom <adresse> » quand les deux
// diffèrent — le nom entre guillemets s'il porte une virgule ou un
// point-virgule, que la rédaction prendrait pour des séparateurs.
static void ecrire_destinataire(Tampon* b, const Destinataire* d)
{
  const wchar_t* adresse = d->adresse && *d->adresse ? d->adresse : NULL;
  if (adresse && _wcsnicmp(adresse, L"SMTP:", 5) == 0)
    adresse += 5;
  const wchar_t* nom = d->nom && *d->nom ? d->nom : NULL;
  if (!adresse) {
    ajouter_z(b, nom ? nom : L"");
    return;
  }
  if (nom && _wcsicmp(nom, adresse) != 0 && !wcschr(nom, L'<') && !wcschr(nom, L'"')) {
    int guillemets = wcschr(nom, L',') || wcschr(nom, L';');
    if (guillemets)
      ajouter_z(b, L"\"");
    ajouter_z(b, nom);
    if (guillemets)
      ajouter_z(b, L"\"");
    ajouter_z(b, L" <");
    ajouter_z(b, adresse);
    ajouter_z(b, L">");
  } else
    ajouter_z(b, adresse);
}

static void ecrire_liste(Tampon* b, const wchar_t* cle, const Message* m, ULONG classe)
{
  ajouter_z(b, L",\n  ");
  ajouter_json(b, cle);
  ajouter_z(b, L": [");
  int premier = 1;
  for (ULONG i = 0; i < m->nb_destinataires; ++i) {
    if (m->destinataires[i].classe != classe)
      continue;
    Tampon d = { 0 };
    ecrire_destinataire(&d, &m->destinataires[i]);
    if (d.n > 0 && !d.erreur) {
      if (!premier)
        ajouter_z(b, L", ");
      ajouter_json(b, d.t);
      premier = 0;
    }
    liberer(d.t);
  }
  ajouter_z(b, L"]");
}

static BOOL ecrire_utf8(const wchar_t* chemin, const wchar_t* texte)
{
  int n = WideCharToMultiByte(CP_UTF8, 0, texte, -1, NULL, 0, NULL, NULL);
  if (n <= 0)
    return FALSE;
  char* octets = (char*)allouer((size_t)n);
  if (!octets)
    return FALSE;
  WideCharToMultiByte(CP_UTF8, 0, texte, -1, octets, n, NULL, NULL);
  BOOL ok = FALSE;
  HANDLE f = CreateFileW(chemin, GENERIC_WRITE, 0, NULL, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, NULL);
  if (f != INVALID_HANDLE_VALUE) {
    DWORD ecrits = 0;
    ok = WriteFile(f, octets, (DWORD)(n - 1), &ecrits, NULL) && ecrits == (DWORD)(n - 1);
    CloseHandle(f);
    if (!ok)
      DeleteFileW(chemin);
  }
  liberer(octets);
  return ok;
}

// Le cœur : copie des pièces, fichier du message, lancement de MMail, attente.
static ULONG transmettre(const Message* m, FLAGS options, const wchar_t* fonction)
{
  wchar_t racine[MAX_PATH];
  if (!dossier_mapi(racine, MAX_PATH)) {
    journal(L"%ls : dossier de travail introuvable", fonction);
    return MAPI_E_FAILURE;
  }
  menage(racine);

  // Un dossier par envoi, à son nom unique.
  SYSTEMTIME t;
  GetLocalTime(&t);
  wchar_t dossier[MAX_PATH];
  if (swprintf(dossier, MAX_PATH, L"%ls\\envoi-%04u%02u%02u-%02u%02u%02u-%lu-%lu", racine, t.wYear, t.wMonth,
               t.wDay, t.wHour, t.wMinute, t.wSecond, GetCurrentProcessId(), GetTickCount())
        < 0
      || !CreateDirectoryW(dossier, NULL)) {
    journal(L"%ls : création du dossier d'envoi impossible (%lu)", fonction, GetLastError());
    return MAPI_E_FAILURE;
  }

  Tampon json = { 0 };
  ajouter_z(&json, L"{\n  \"version\": 1,\n  \"objet\": ");
  ajouter_json(&json, m->objet ? m->objet : L"");
  ajouter_z(&json, L",\n  \"texte\": ");
  ajouter_json(&json, m->texte ? m->texte : L"");
  ecrire_liste(&json, L"a", m, MAPI_TO);
  ecrire_liste(&json, L"cc", m, MAPI_CC);
  ecrire_liste(&json, L"cci", m, MAPI_BCC);
  ajouter_z(&json, L",\n  \"pieces\": [");

  for (ULONG i = 0; i < m->nb_pieces; ++i) {
    const Piece* p = &m->pieces[i];
    if (!p->chemin || !existe_fichier(p->chemin)) {
      journal(L"%ls : pièce jointe %lu introuvable", fonction, i + 1);
      liberer(json.t);
      return MAPI_E_ATTACHMENT_NOT_FOUND;
    }
    // Le nom affiché par l'appelant s'il en donne un (un fichier temporaire
    // « ~tmp123.pdf » peut porter le nom de l'état), sinon celui du fichier.
    wchar_t nom[MAX_PATH];
    const wchar_t* source = p->nom && *p->nom ? p->nom : NULL;
    if (source) {
      const wchar_t* dernier = wcsrchr(source, L'\\');
      if (dernier)
        source = dernier + 1;
    } else {
      const wchar_t* dernier = wcsrchr(p->chemin, L'\\');
      source = dernier ? dernier + 1 : p->chemin;
    }
    wcsncpy_s(nom, MAX_PATH, source, _TRUNCATE);
    assainir(nom);
    if (!*nom)
      swprintf(nom, MAX_PATH, L"piece-%lu", i + 1);
    // Deux pièces du même nom : « (2) », « (3) »… devant l'extension.
    wchar_t cible[MAX_PATH];
    if (swprintf(cible, MAX_PATH, L"%ls\\%ls", dossier, nom) < 0) {
      liberer(json.t);
      return MAPI_E_FAILURE;
    }
    for (int k = 2; existe_fichier(cible) && k < 100; ++k) {
      wchar_t base[MAX_PATH];
      wcsncpy_s(base, MAX_PATH, nom, _TRUNCATE);
      wchar_t* point = wcsrchr(base, L'.');
      const wchar_t* extension = L"";
      if (point) {
        *point = 0;
        extension = nom + (point - base);
      }
      swprintf(cible, MAX_PATH, L"%ls\\%ls (%d)%ls", dossier, base, k, extension);
    }
    if (!CopyFileW(p->chemin, cible, TRUE)) {
      journal(L"%ls : copie de la pièce jointe %lu impossible (%lu)", fonction, i + 1, GetLastError());
      liberer(json.t);
      return MAPI_E_ATTACHMENT_OPEN_FAILURE;
    }
    SetFileAttributesW(cible, FILE_ATTRIBUTE_NORMAL);
    if (i > 0)
      ajouter_z(&json, L", ");
    ajouter_json(&json, cible);
  }
  ajouter_z(&json, L"]\n}\n");
  if (json.erreur) {
    liberer(json.t);
    return MAPI_E_INSUFFICIENT_MEMORY;
  }

  wchar_t fichier[MAX_PATH];
  if (swprintf(fichier, MAX_PATH, L"%ls\\message.json", dossier) < 0 || !ecrire_utf8(fichier, json.t)) {
    journal(L"%ls : écriture du message impossible (%lu)", fonction, GetLastError());
    liberer(json.t);
    return MAPI_E_FAILURE;
  }
  liberer(json.t);

  // Essais de fabrication : le message est préparé, MMail n'est pas lancé.
  wchar_t essai[8];
  if (GetEnvironmentVariableW(L"MMAIL_MAPI_ESSAI", essai, 8) > 0) {
    journal(L"%ls : essai, message préparé dans %ls", fonction, dossier);
    return SUCCESS_SUCCESS;
  }

  wchar_t exe[MAX_PATH];
  if (!trouver_mmail(exe, MAX_PATH)) {
    journal(L"%ls : MMail.exe introuvable", fonction);
    return MAPI_E_FAILURE;
  }
  wchar_t ligne[3 * MAX_PATH];
  if (swprintf(ligne, 3 * MAX_PATH, L"\"%ls\" --rediger \"%ls\"", exe, fichier) < 0)
    return MAPI_E_FAILURE;
  wchar_t repertoire[MAX_PATH];
  wcsncpy_s(repertoire, MAX_PATH, exe, _TRUNCATE);
  wchar_t* fin = wcsrchr(repertoire, L'\\');
  if (fin)
    *fin = 0;

  // Le premier plan revient à la rédaction que MMail va ouvrir.
  AllowSetForegroundWindow(ASFW_ANY);
  STARTUPINFOW si;
  PROCESS_INFORMATION pi;
  memset(&si, 0, sizeof si);
  si.cb = sizeof si;
  memset(&pi, 0, sizeof pi);
  if (!CreateProcessW(exe, ligne, NULL, NULL, FALSE, 0, NULL, repertoire, &si, &pi)) {
    journal(L"%ls : lancement de MMail impossible (%lu)", fonction, GetLastError());
    return MAPI_E_FAILURE;
  }
  CloseHandle(pi.hThread);

  // MMail efface le fichier une fois la rédaction ouverte. Un lancement qui
  // échoue (code de sortie non nul avant d'avoir pris le message) est rendu
  // comme tel à l'appelant ; un MMail lent, pas : la rédaction viendra.
  ULONG resultat = SUCCESS_SUCCESS;
  DWORD debut = GetTickCount();
  BOOL termine = FALSE;
  while (existe_fichier(fichier)) {
    if (GetTickCount() - debut > ATTENTE_MS) {
      journal(L"%ls : MMail n'a pas pris le message en %u s", fonction, ATTENTE_MS / 1000);
      break;
    }
    if (!termine && WaitForSingleObject(pi.hProcess, 100) == WAIT_OBJECT_0) {
      termine = TRUE;
      DWORD code = 0;
      GetExitCodeProcess(pi.hProcess, &code);
      if (code != 0 && existe_fichier(fichier)) {
        journal(L"%ls : MMail sorti en erreur (%lu) sans prendre le message", fonction, code);
        resultat = MAPI_E_FAILURE;
        break;
      }
    } else if (termine)
      Sleep(100);
  }
  CloseHandle(pi.hProcess);
  if (resultat == SUCCESS_SUCCESS)
    journal(L"%ls : %lu destinataire(s), %lu pièce(s), options 0x%lx : transmis à MMail", fonction,
            m->nb_destinataires, m->nb_pieces, (unsigned long)options);
  return resultat;
}

// ---- Fonctions exportées -------------------------------------------------------

ULONG WINAPI MAPISendMailW(LHANDLE session, ULONG_PTR fenetre, lpMapiMessageW message, FLAGS options,
                           ULONG reserve)
{
  (void)session;
  (void)fenetre;
  (void)reserve;
  if (!message)
    return MAPI_E_INVALID_MESSAGE;
  Message m;
  memset(&m, 0, sizeof m);
  m.objet = copie(message->lpszSubject);
  m.texte = copie(message->lpszNoteText);
  if (message->nRecipCount > 0 && message->lpRecips) {
    m.destinataires = (Destinataire*)allouer(message->nRecipCount * sizeof(Destinataire));
    if (!m.destinataires)
      return MAPI_E_INSUFFICIENT_MEMORY;
    m.nb_destinataires = message->nRecipCount;
    for (ULONG i = 0; i < message->nRecipCount; ++i) {
      m.destinataires[i].classe = message->lpRecips[i].ulRecipClass;
      m.destinataires[i].nom = copie(message->lpRecips[i].lpszName);
      m.destinataires[i].adresse = copie(message->lpRecips[i].lpszAddress);
    }
  }
  if (message->nFileCount > 0 && message->lpFiles) {
    m.pieces = (Piece*)allouer(message->nFileCount * sizeof(Piece));
    if (!m.pieces) {
      liberer_message(&m);
      return MAPI_E_INSUFFICIENT_MEMORY;
    }
    m.nb_pieces = message->nFileCount;
    for (ULONG i = 0; i < message->nFileCount; ++i) {
      m.pieces[i].chemin = copie(message->lpFiles[i].lpszPathName);
      m.pieces[i].nom = copie(message->lpFiles[i].lpszFileName);
    }
  }
  ULONG r = transmettre(&m, options, L"MAPISendMailW");
  liberer_message(&m);
  return r;
}

ULONG WINAPI MAPISendMail(LHANDLE session, ULONG_PTR fenetre, lpMapiMessage message, FLAGS options,
                          ULONG reserve)
{
  (void)session;
  (void)fenetre;
  (void)reserve;
  if (!message)
    return MAPI_E_INVALID_MESSAGE;
  Message m;
  memset(&m, 0, sizeof m);
  m.objet = depuis_ansi(message->lpszSubject);
  m.texte = depuis_ansi(message->lpszNoteText);
  if (message->nRecipCount > 0 && message->lpRecips) {
    m.destinataires = (Destinataire*)allouer(message->nRecipCount * sizeof(Destinataire));
    if (!m.destinataires)
      return MAPI_E_INSUFFICIENT_MEMORY;
    m.nb_destinataires = message->nRecipCount;
    for (ULONG i = 0; i < message->nRecipCount; ++i) {
      m.destinataires[i].classe = message->lpRecips[i].ulRecipClass;
      m.destinataires[i].nom = depuis_ansi(message->lpRecips[i].lpszName);
      m.destinataires[i].adresse = depuis_ansi(message->lpRecips[i].lpszAddress);
    }
  }
  if (message->nFileCount > 0 && message->lpFiles) {
    m.pieces = (Piece*)allouer(message->nFileCount * sizeof(Piece));
    if (!m.pieces) {
      liberer_message(&m);
      return MAPI_E_INSUFFICIENT_MEMORY;
    }
    m.nb_pieces = message->nFileCount;
    for (ULONG i = 0; i < message->nFileCount; ++i) {
      m.pieces[i].chemin = depuis_ansi(message->lpFiles[i].lpszPathName);
      m.pieces[i].nom = depuis_ansi(message->lpFiles[i].lpszFileName);
    }
  }
  ULONG r = transmettre(&m, options, L"MAPISendMail");
  liberer_message(&m);
  return r;
}

// Fichiers séparés par un délimiteur (le plus souvent « ; »), avec leurs noms
// d'affichage facultatifs dans le même ordre.
ULONG WINAPI MAPISendDocuments(ULONG_PTR fenetre, LPSTR delimiteur, LPSTR chemins, LPSTR noms, ULONG reserve)
{
  (void)fenetre;
  (void)reserve;
  if (!chemins || !*chemins)
    return MAPI_E_ATTACHMENT_NOT_FOUND;
  char sep = delimiteur && *delimiteur ? *delimiteur : ';';
  wchar_t* tous = depuis_ansi(chemins);
  wchar_t* tous_noms = depuis_ansi(noms);
  if (!tous)
    return MAPI_E_INSUFFICIENT_MEMORY;
  wchar_t wsep = (wchar_t)(unsigned char)sep;
  ULONG nombre = 1;
  for (wchar_t* c = tous; *c; ++c)
    if (*c == wsep)
      ++nombre;
  Message m;
  memset(&m, 0, sizeof m);
  m.pieces = (Piece*)allouer(nombre * sizeof(Piece));
  if (!m.pieces) {
    liberer(tous);
    liberer(tous_noms);
    return MAPI_E_INSUFFICIENT_MEMORY;
  }
  wchar_t* suite = tous;
  wchar_t* suite_noms = tous_noms;
  for (ULONG i = 0; i < nombre && suite; ++i) {
    wchar_t* fin = wcschr(suite, wsep);
    if (fin)
      *fin = 0;
    if (*suite) {
      m.pieces[m.nb_pieces].chemin = copie(suite);
      if (suite_noms) {
        wchar_t* fin_nom = wcschr(suite_noms, wsep);
        if (fin_nom)
          *fin_nom = 0;
        m.pieces[m.nb_pieces].nom = copie(suite_noms);
        suite_noms = fin_nom ? fin_nom + 1 : NULL;
      }
      ++m.nb_pieces;
    }
    suite = fin ? fin + 1 : NULL;
  }
  liberer(tous);
  liberer(tous_noms);
  ULONG r = transmettre(&m, MAPI_DIALOG, L"MAPISendDocuments");
  liberer_message(&m);
  return r;
}

// Une session n'a pas de sens ici : MMail tient ses comptes lui-même.
ULONG WINAPI MAPILogon(ULONG_PTR fenetre, LPSTR profil, LPSTR mot_de_passe, FLAGS options, ULONG reserve,
                       LPLHANDLE session)
{
  (void)fenetre;
  (void)profil;
  (void)mot_de_passe;
  (void)options;
  (void)reserve;
  if (session)
    *session = SESSION_MMAIL;
  return SUCCESS_SUCCESS;
}

ULONG WINAPI MAPILogoff(LHANDLE session, ULONG_PTR fenetre, FLAGS options, ULONG reserve)
{
  (void)session;
  (void)fenetre;
  (void)options;
  (void)reserve;
  return SUCCESS_SUCCESS;
}

// Blocs rendus à l'appelant (MAPIResolveName) : précédés d'une marque, pour
// ne libérer que ce qui vient d'ici.
#define MARQUE_BLOC 0x4D4D4D42UL
typedef struct {
  ULONG marque;
  ULONG remplissage;
} EnTeteBloc;

ULONG WINAPI MAPIFreeBuffer(LPVOID bloc)
{
  if (!bloc)
    return SUCCESS_SUCCESS;
  EnTeteBloc* e = (EnTeteBloc*)bloc - 1;
  if (e->marque != MARQUE_BLOC)
    return MAPI_E_FAILURE;
  e->marque = 0;
  liberer(e);
  return SUCCESS_SUCCESS;
}

// Résolution d'un nom : rendu tel quel, comme adresse quand il en a la forme.
// Les logiciels qui résolvent avant d'envoyer obtiennent ainsi un
// destinataire, que la personne corrige au besoin dans la rédaction.
ULONG WINAPI MAPIResolveName(LHANDLE session, ULONG_PTR fenetre, LPSTR nom, FLAGS options, ULONG reserve,
                             lpMapiRecipDesc* resultat)
{
  (void)session;
  (void)fenetre;
  (void)options;
  (void)reserve;
  if (!resultat)
    return MAPI_E_FAILURE;
  *resultat = NULL;
  if (!nom || !*nom)
    return MAPI_E_UNKNOWN_RECIPIENT;
  size_t n = strlen(nom) + 1;
  int smtp = strchr(nom, '@') != NULL;
  EnTeteBloc* e = (EnTeteBloc*)allouer(sizeof(EnTeteBloc) + sizeof(MapiRecipDesc) + 2 * n + 5);
  if (!e)
    return MAPI_E_INSUFFICIENT_MEMORY;
  e->marque = MARQUE_BLOC;
  MapiRecipDesc* d = (MapiRecipDesc*)(e + 1);
  char* texte = (char*)(d + 1);
  memcpy(texte, nom, n);
  d->ulRecipClass = MAPI_TO;
  d->lpszName = texte;
  if (smtp) {
    char* adresse = texte + n;
    memcpy(adresse, "SMTP:", 5);
    memcpy(adresse + 5, nom, n);
    d->lpszAddress = adresse;
  }
  *resultat = d;
  return SUCCESS_SUCCESS;
}

// Le reste de MAPI simple — lecture de la boîte, carnet d'adresses — n'est pas
// offert : MMail a sa propre interface pour cela.
ULONG WINAPI MAPIAddress(LHANDLE a, ULONG_PTR b, LPSTR c, ULONG d, LPSTR e, ULONG f, lpMapiRecipDesc g, FLAGS h,
                         ULONG i, LPULONG j, lpMapiRecipDesc* k)
{
  (void)a, (void)b, (void)c, (void)d, (void)e, (void)f, (void)g, (void)h, (void)i, (void)j, (void)k;
  return MAPI_E_NOT_SUPPORTED;
}

ULONG WINAPI MAPIDetails(LHANDLE a, ULONG_PTR b, lpMapiRecipDesc c, FLAGS d, ULONG e)
{
  (void)a, (void)b, (void)c, (void)d, (void)e;
  return MAPI_E_NOT_SUPPORTED;
}

ULONG WINAPI MAPIFindNext(LHANDLE a, ULONG_PTR b, LPSTR c, LPSTR d, FLAGS e, ULONG f, LPSTR g)
{
  (void)a, (void)b, (void)c, (void)d, (void)e, (void)f, (void)g;
  return MAPI_E_NOT_SUPPORTED;
}

ULONG WINAPI MAPIReadMail(LHANDLE a, ULONG_PTR b, LPSTR c, FLAGS d, ULONG e, lpMapiMessage* f)
{
  (void)a, (void)b, (void)c, (void)d, (void)e, (void)f;
  return MAPI_E_NOT_SUPPORTED;
}

ULONG WINAPI MAPISaveMail(LHANDLE a, ULONG_PTR b, lpMapiMessage c, FLAGS d, ULONG e, LPSTR f)
{
  (void)a, (void)b, (void)c, (void)d, (void)e, (void)f;
  return MAPI_E_NOT_SUPPORTED;
}

ULONG WINAPI MAPIDeleteMail(LHANDLE a, ULONG_PTR b, LPSTR c, FLAGS d, ULONG e)
{
  (void)a, (void)b, (void)c, (void)d, (void)e;
  return MAPI_E_NOT_SUPPORTED;
}
