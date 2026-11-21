// SPDX-License-Identifier: GPL-3.0-or-later
//
// Ouvre un fichier avec un logiciel désigné : les pièces jointes s'ouvrent
// avec le logiciel choisi par type de fichier, sans dépendre des associations
// du système (décision 9). QML ne sait pas lancer un programme.
//
// Le choix — quel logiciel pour quelle extension — est gardé par l'interface,
// sur ce poste seulement : un chemin de programme ne vaut que sur la machine
// où il a été désigné (décision 18).
#pragma once

#include <QtCore/QDateTime>
#include <QtCore/QDir>
#include <QtCore/QFileInfo>
#include <QtCore/QHash>
#include <QtCore/QMimeDatabase>
#include <QtCore/QObject>
#include <QtCore/QProcess>
#include <QtCore/QStandardPaths>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QUrl>
#include <QtCore/QVariantMap>

#include <QtGui/QDesktopServices>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniObject>
#include <QtCore/qcoreapplication_platform.h>
#endif

#ifdef Q_OS_WIN
#include <windows.h>
#include <shlwapi.h>

#include <string>
#endif

class Lanceur : public QObject
{
  Q_OBJECT
public:
  explicit Lanceur(QObject* parent = nullptr)
    : QObject(parent)
  {
  }

  /// Lance `programme` sur `fichier`, détaché : MMail ne l'attend pas. L'un et
  /// l'autre en chemin ou en adresse `file:`. Faux si le programme n'existe
  /// pas, n'est pas exécutable, ou n'a pas pu démarrer.
  Q_INVOKABLE bool lancer(const QString& programme, const QString& fichier) const
  {
    const QString exe = chemin_(programme);
    const QFileInfo info(exe);
    if (exe.isEmpty() || !info.isFile() || !info.isExecutable())
      return false;
    return QProcess::startDetached(exe, QStringList{ chemin_(fichier) });
  }

  /// Ouvre un fichier local (chemin ou adresse `file:`) avec le logiciel du
  /// système. Sous Android, par une adresse `content:` du FileProvider de
  /// MMail (packaging/android/src/fr/mmedia/mmail/Ouvrir.java) : Android
  /// refuse une adresse `file:` passée à une autre application, et
  /// Qt.openUrlExternally n'ouvrait rien. Faux si rien ne l'a ouvert.
  Q_INVOKABLE bool ouvrir(const QString& fichier) const
  {
    const QString local = chemin_(fichier);
#ifdef Q_OS_ANDROID
    // Déjà une adresse d'un fournisseur (fichier choisi dans le sélecteur
    // d'Android) : Qt sait la transmettre.
    if (fichier.startsWith(QLatin1String("content:")))
      return QDesktopServices::openUrl(QUrl(fichier));
    const QJniObject contexte = QNativeInterface::QAndroidApplication::context();
    if (!contexte.isValid())
      return false;
    const QString type = QMimeDatabase().mimeTypeForFile(local).name();
    return QJniObject::callStaticMethod<jboolean>("fr/mmedia/mmail/Ouvrir", "fichier",
                                                  "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Z",
                                                  contexte.object(), QJniObject::fromString(local).object<jstring>(),
                                                  QJniObject::fromString(type).object<jstring>());
#else
    return QDesktopServices::openUrl(QUrl::fromLocalFile(local));
#endif
  }

  /// Destination à donner au noyau pour enregistrer la pièce `nom` à
  /// l'emplacement `cible`, choisi dans la fenêtre d'enregistrement. Sous
  /// Android, cette fenêtre rend l'adresse d'un document (`content:`), où le
  /// noyau n'écrit pas : la pièce va alors dans un fichier de travail, dont
  /// l'adresse `file:` est rendue, et le document est retenu pour
  /// `terminerEnregistrement`. Toute autre adresse est rendue telle quelle.
  Q_INVOKABLE QString relaisEnregistrement(const QString& cible, const QString& nom)
  {
#ifdef Q_OS_ANDROID
    if (!cible.startsWith(QLatin1String("content:")))
      return cible;
    // Un dossier par enregistrement : deux pièces de même nom enregistrées
    // coup sur coup ne se mêlent pas.
    const QString dossier = QStandardPaths::writableLocation(QStandardPaths::CacheLocation)
      + QStringLiteral("/enregistrement/") + QString::number(QDateTime::currentMSecsSinceEpoch())
      + QLatin1Char('-') + QString::number(++relaisFaits_);
    if (!QDir().mkpath(dossier))
      return cible;
    QString sur = nom;
    sur.replace(QLatin1Char('/'), QLatin1Char('_')).replace(QLatin1Char('\\'), QLatin1Char('_'));
    const QString fichier = dossier + QLatin1Char('/') + (sur.isEmpty() ? QStringLiteral("piece") : sur);
    relais_.insert(fichier, cible);
    return QUrl::fromLocalFile(fichier).toString();
#else
    Q_UNUSED(nom);
    return cible;
#endif
  }

  /// Suite d'un enregistrement relayé par `relaisEnregistrement`, une fois la
  /// pièce écrite à `chemin` : recopie dans le document retenu, puis retrait
  /// du fichier de travail. `relaye` faux : `chemin` n'était pas relayé, rien
  /// n'est fait. Sinon `erreur`, vide si tout est écrit, et `nom`, celui du
  /// document chez son fournisseur.
  Q_INVOKABLE QVariantMap terminerEnregistrement(const QString& chemin)
  {
    QVariantMap issue{ { QStringLiteral("relaye"), false } };
#ifdef Q_OS_ANDROID
    const QString local = chemin_(chemin);
    const auto trouve = relais_.constFind(local);
    if (trouve == relais_.constEnd())
      return issue;
    const QString document = trouve.value();
    relais_.erase(trouve);
    QString erreur = QStringLiteral("contexte Android introuvable");
    QString nom;
    const QJniObject contexte = QNativeInterface::QAndroidApplication::context();
    if (contexte.isValid()) {
      const QJniObject reponse = QJniObject::callStaticObjectMethod(
        "fr/mmedia/mmail/Enregistrer", "copier",
        "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;", contexte.object(),
        QJniObject::fromString(local).object<jstring>(), QJniObject::fromString(document).object<jstring>());
      // Objet nul : exception Java, que QJniObject a écartée.
      erreur = reponse.isValid() ? reponse.toString() : QStringLiteral("écriture du document interrompue");
      nom = QJniObject::callStaticObjectMethod("fr/mmedia/mmail/Enregistrer", "nom",
                                               "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
                                               contexte.object(), QJniObject::fromString(document).object<jstring>())
              .toString();
    }
    QDir(QFileInfo(local).absolutePath()).removeRecursively();
    issue.insert(QStringLiteral("relaye"), true);
    issue.insert(QStringLiteral("erreur"), erreur);
    issue.insert(QStringLiteral("nom"), nom.isEmpty() ? QFileInfo(local).fileName() : nom);
#else
    Q_UNUSED(chemin);
#endif
    return issue;
  }

  /// Nom lisible d'un programme : « C:/…/Acrobat.exe » → « Acrobat ».
  Q_INVOKABLE QString nom(const QString& programme) const
  {
    return QFileInfo(chemin_(programme)).completeBaseName();
  }

  /// Chemin local d'une adresse `file:` (un chemin est rendu tel quel).
  Q_INVOKABLE QString chemin(const QString& adresse) const { return chemin_(adresse); }

  /// Adresse `file:` d'un chemin local, pour Qt.openUrlExternally : « # » ou
  /// « % » dans un nom de fichier y sont encodés.
  Q_INVOKABLE QUrl adresse(const QString& chemin) const { return QUrl::fromLocalFile(chemin_(chemin)); }

  /// Vrai si le système sait ouvrir seul les fichiers de cette extension.
  /// Sous Windows, faux quand il demanderait « Comment voulez-vous ouvrir ce
  /// fichier ? » : le logiciel choisi dans cette fenêtre échappe à MMail, qui
  /// ne peut donc pas le retenir. Ailleurs, toujours vrai : le système décide.
  Q_INVOKABLE bool associe(const QString& extension) const
  {
#ifdef Q_OS_WIN
    if (extension.isEmpty())
      return false;
    const std::wstring ext = L"." + extension.toStdWString();
    // Une commande, ou un verbe délégué : c'est ainsi que s'ouvrent les
    // applications du Store (Photos, par exemple). Sans pszOut, l'appel rend
    // S_FALSE et la taille du texte quand l'association existe.
    for (const ASSOCSTR quoi : { ASSOCSTR_COMMAND, ASSOCSTR_DELEGATEEXECUTE }) {
      DWORD taille = 0;
      if (AssocQueryStringW(ASSOCF_INIT_IGNOREUNKNOWN, quoi, ext.c_str(), nullptr, nullptr,
                            &taille) == S_FALSE
          && taille > 1)
        return true;
    }
    return false;
#else
    Q_UNUSED(extension);
    return true;
#endif
  }

private:
  static QString chemin_(const QString& adresse)
  {
    return adresse.startsWith(QLatin1String("file:")) ? QUrl(adresse).toLocalFile() : adresse;
  }

#ifdef Q_OS_ANDROID
  /// Enregistrements relayés en cours : fichier de travail → document.
  QHash<QString, QString> relais_;
  int relaisFaits_ = 0;
#endif
};
