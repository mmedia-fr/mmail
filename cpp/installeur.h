// SPDX-License-Identifier: GPL-3.0-or-later
//
// Installation d'une mise à jour téléchargée par le noyau (cf.
// core/src/mise_a_jour.rs) : elle suit la fin du processus, puisqu'un programme
// en marche ne remplace pas ses propres fichiers (demande de Manu du
// 10/10/2026 : « maintenant », « à l'arrêt » ou « jamais »).
//
// Windows : l'installeur Inno Setup, lancé sans question à la fermeture
// (/VERYSILENT ; /SILENT, avec sa barre d'avancement, quand MMail doit se
// rouvrir). Il attend la fin de MMail avant de remplacer ses fichiers
// (/ATTENDRE=<pid>) et le relance s'il le faut (/RELANCER=1), cf.
// build/socle-mmail.iss. Une installation pour tous les utilisateurs (dans
// Program Files) demande l'autorisation d'un administrateur : elle n'est
// proposée qu'à un compte qui peut la donner.
//
// Linux, en AppImage : le noyau remplace le fichier lui-même, d'un seul
// renommage ; ne reste ici que la relance.
//
// Ailleurs — Android, macOS, programme compilé sur place — rien ne s'installe
// seul : la page de la version s'ouvre.
#pragma once

#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtCore/QFileInfo>
#include <QtCore/QObject>
#include <QtCore/QProcess>
#include <QtCore/QStandardPaths>
#include <QtCore/QString>
#include <QtCore/QStringList>

#ifdef Q_OS_WIN
#include <windows.h>
#endif

class Installeur : public QObject
{
  Q_OBJECT
public:
  explicit Installeur(QObject* parent = nullptr)
    : QObject(parent)
  {
  }

  /// Paquet que cette copie de MMail sait installer : « setup », « appimage »,
  /// ou vide.
  Q_INVOKABLE QString genre() const
  {
#ifdef Q_OS_WIN
    // Installé par l'installeur, et non copié à la main ou lancé depuis le
    // dossier de compilation.
    const QString dossier = QCoreApplication::applicationDirPath();
    if (!QFileInfo::exists(dossier + QStringLiteral("/unins000.exe")))
      return {};
    if (pourLaMachine() && !administrateur())
      return {};
    return QStringLiteral("setup");
#elif defined(Q_OS_LINUX) && !defined(Q_OS_ANDROID)
    const QFileInfo image(appImage());
    // Le renommage final se fait dans le dossier de l'AppImage.
    if (image.fileName().isEmpty() || !image.isFile() || !QFileInfo(image.absolutePath()).isWritable())
      return {};
    return QStringLiteral("appimage");
#else
    return {};
#endif
  }

  /// Vrai si MMail est installé pour tous les utilisateurs de l'ordinateur :
  /// sa mise à jour demande alors l'autorisation d'un administrateur.
  Q_INVOKABLE bool pourTousLesUtilisateurs() const { return pourLaMachine(); }

  /// Dossier où le paquet est téléchargé.
  Q_INVOKABLE QString dossier() const
  {
    if (genre() == QStringLiteral("appimage"))
      return QFileInfo(appImage()).absolutePath();
    return QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation) +
           QStringLiteral("/mises-a-jour");
  }

  /// Chemin du paquet de `version` une fois téléchargé : l'AppImage elle-même,
  /// remplacée sur place pour que ses raccourcis restent bons ; l'installeur
  /// dans `dossier()`.
  Q_INVOKABLE QString destination(const QString& version) const
  {
    if (genre() == QStringLiteral("appimage"))
      return QFileInfo(appImage()).absoluteFilePath();
    return dossier() + QStringLiteral("/MMail-") + version + QStringLiteral("-setup.exe");
  }

  /// Installe `fichier` à la fermeture de MMail ; `relancer` rouvre MMail
  /// ensuite.
  Q_INVOKABLE void programmer(const QString& fichier, bool relancer)
  {
    s_fichier = fichier;
    s_relancer = relancer;
  }

  Q_INVOKABLE void annuler() { s_fichier.clear(); }

  Q_INVOKABLE bool programme() const { return !s_fichier.isEmpty(); }

  /// La session se ferme (arrêt ou déconnexion) : Windows n'attendrait pas la
  /// fin d'un installeur lancé maintenant, et un remplacement à moitié fait
  /// laisserait MMail hors d'usage. L'installation attend la fermeture
  /// suivante.
  static void sessionFinie() { s_sessionFinie = true; }

  /// À appeler une fois la boucle d'événements finie.
  static void executer()
  {
    if (s_fichier.isEmpty() || s_sessionFinie || !QFileInfo::exists(s_fichier))
      return;
    const QString pid = QString::number(QCoreApplication::applicationPid());
#ifdef Q_OS_WIN
    QStringList arguments{ s_relancer ? QStringLiteral("/SILENT") : QStringLiteral("/VERYSILENT"),
                           QStringLiteral("/SUPPRESSMSGBOXES"), QStringLiteral("/NORESTART"),
                           QStringLiteral("/SP-"),
                           pourLaMachine() ? QStringLiteral("/ALLUSERS") : QStringLiteral("/CURRENTUSER"),
                           QStringLiteral("/ATTENDRE=") + pid };
    if (s_relancer)
      arguments << QStringLiteral("/RELANCER=1");
    QProcess::startDetached(QDir::toNativeSeparators(s_fichier), arguments);
#elif defined(Q_OS_LINUX) && !defined(Q_OS_ANDROID)
    // L'AppImage est déjà remplacée. La relance attend la fin de ce processus :
    // le verrou du profil ne laisse ouvrir qu'un MMail à la fois.
    if (!s_relancer)
      return;
    QProcess relance;
    relance.setProgram(QStringLiteral("/bin/sh"));
    relance.setArguments({ QStringLiteral("-c"),
                           QStringLiteral("while kill -0 \"$0\" 2>/dev/null; do sleep 0.2; done; exec \"$1\""),
                           pid, s_fichier });
    // Ce que l'AppImage en marche a posé pour elle-même : la nouvelle pose le
    // sien.
    QProcessEnvironment environnement = QProcessEnvironment::systemEnvironment();
    for (const char* nom : { "APPDIR", "APPIMAGE", "ARGV0", "OWD" })
      environnement.remove(QString::fromLatin1(nom));
    relance.setProcessEnvironment(environnement);
    relance.startDetached();
#endif
  }

private:
  static inline QString s_fichier;
  static inline bool s_relancer = false;
  static inline bool s_sessionFinie = false;

  static bool pourLaMachine()
  {
#ifdef Q_OS_WIN
    const QString dossier = QDir::cleanPath(QCoreApplication::applicationDirPath()) + QLatin1Char('/');
    for (const char* variable : { "ProgramW6432", "ProgramFiles", "ProgramFiles(x86)" }) {
      const QString racine = QDir::fromNativeSeparators(qEnvironmentVariable(variable));
      if (!racine.isEmpty() && dossier.startsWith(QDir::cleanPath(racine) + QLatin1Char('/'), Qt::CaseInsensitive))
        return true;
    }
#endif
    return false;
  }

#if defined(Q_OS_LINUX) && !defined(Q_OS_ANDROID)
  /// Chemin de l'AppImage en marche, que son lanceur pose dans l'environnement.
  static QString appImage() { return qEnvironmentVariable("APPIMAGE"); }
#else
  static QString appImage() { return {}; }
#endif

#ifdef Q_OS_WIN
  /// Vrai si ce compte peut autoriser une installation : administrateur, que
  /// son jeton soit restreint par le contrôle de compte (UAC) ou déjà élevé.
  static bool administrateur()
  {
    HANDLE jeton = nullptr;
    if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &jeton))
      return false;
    TOKEN_ELEVATION_TYPE type = TokenElevationTypeDefault;
    DWORD taille = 0;
    const bool lu = GetTokenInformation(jeton, TokenElevationType, &type, sizeof(type), &taille);
    CloseHandle(jeton);
    if (lu && type != TokenElevationTypeDefault)
      return true;
    // Contrôle de compte désactivé, ou compte standard : l'appartenance au
    // groupe des administrateurs tranche.
    SID_IDENTIFIER_AUTHORITY autorite = SECURITY_NT_AUTHORITY;
    PSID administrateurs = nullptr;
    if (!AllocateAndInitializeSid(&autorite, 2, SECURITY_BUILTIN_DOMAIN_RID, DOMAIN_ALIAS_RID_ADMINS, 0, 0, 0,
                                  0, 0, 0, &administrateurs))
      return false;
    BOOL membre = FALSE;
    CheckTokenMembership(nullptr, administrateurs, &membre);
    FreeSid(administrateurs);
    return membre;
  }
#endif
};
