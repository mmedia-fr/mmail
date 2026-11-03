// SPDX-License-Identifier: GPL-3.0-or-later
//
// Messagerie par défaut du poste (Windows) : celle que Windows sert aux autres
// logiciels par MAPI — « Envoyer vers › Destinataire » de l'Explorateur,
// l'export d'un document ou d'un état « par courriel » d'un logiciel de
// gestion.
//
// Le choix vaut pour la session : valeur par défaut de
// HKCU\Software\Clients\Mail, que Windows lit avant celle de la machine
// (HKLM), et qu'écrivent aussi Outlook ou Thunderbird quand on les choisit
// par défaut. Les messageries offertes sont celles que leur installation a
// déclarées sous HKLM\Software\Clients\Mail ; MMail n'y figure que si son
// fournisseur MAPI a été installé pour la machine (installation « pour tous
// les utilisateurs », cf. build/socle-mmail.iss et mapi/mmail_mapi.c).
//
// Les liens « mailto: » sont un autre réglage, que Windows réserve à ses
// propres paramètres (« Applications par défaut ») : le dialogue les ouvre.
#pragma once

#include <QtCore/QFileInfo>
#include <QtCore/QObject>
#include <QtCore/QSettings>
#include <QtCore/QString>
#include <QtCore/QVariantList>
#include <QtCore/QVariantMap>

class Messagerie : public QObject
{
  Q_OBJECT
  /// Vrai sous Windows seulement : ailleurs, pas de MAPI.
  Q_PROPERTY(bool disponible READ disponible CONSTANT)
public:
  explicit Messagerie(QObject* parent = nullptr)
    : QObject(parent)
  {
  }

  bool disponible() const
  {
#ifdef Q_OS_WIN
    return true;
#else
    return false;
#endif
  }

  /// Messageries déclarées pour la machine : [{nom, libelle}], nom = celui de
  /// la clé (ce que l'on écrit pour la choisir), libelle = son nom affiché.
  Q_INVOKABLE QVariantList clients() const
  {
    QVariantList liste;
#ifdef Q_OS_WIN
    QSettings machine(QStringLiteral("HKEY_LOCAL_MACHINE\\SOFTWARE\\Clients\\Mail"), QSettings::NativeFormat);
    const QStringList cles = machine.childGroups();
    for (const QString& cle : cles) {
      machine.beginGroup(cle);
      // Seules les messageries qui servent MAPI : un gestionnaire de liens
      // déclaré là sans DLL ne recevrait rien.
      const bool mapi = machine.contains(QStringLiteral("DLLPath")) || machine.contains(QStringLiteral("DLLPathEx"));
      QString libelle = machine.value(QStringLiteral("Default")).toString();
      machine.endGroup();
      if (!mapi)
        continue;
      if (libelle.isEmpty())
        libelle = cle;
      liste.append(QVariantMap{ { QStringLiteral("nom"), cle }, { QStringLiteral("libelle"), libelle } });
    }
#endif
    return liste;
  }

  /// Messagerie choisie pour la session, à défaut celle de la machine ; vide
  /// si aucune.
  Q_INVOKABLE QString actuelle() const
  {
#ifdef Q_OS_WIN
    const QString session = valeurParDefaut_(QStringLiteral("HKEY_CURRENT_USER\\Software\\Clients\\Mail"));
    if (!session.isEmpty())
      return session;
    return machine();
#else
    return QString();
#endif
  }

  /// Messagerie par défaut de la machine, qui vaut pour les sessions qui n'en
  /// ont pas choisi.
  Q_INVOKABLE QString machine() const
  {
#ifdef Q_OS_WIN
    return valeurParDefaut_(QStringLiteral("HKEY_LOCAL_MACHINE\\SOFTWARE\\Clients\\Mail"));
#else
    return QString();
#endif
  }

  /// Choisit la messagerie de la session ; vide : revenir à celle de la
  /// machine. Faux si l'écriture a échoué.
  Q_INVOKABLE bool choisir(const QString& nom)
  {
#ifdef Q_OS_WIN
    QSettings session(QStringLiteral("HKEY_CURRENT_USER\\Software\\Clients\\Mail"), QSettings::NativeFormat);
    if (nom.isEmpty())
      session.remove(QStringLiteral("Default"));
    else
      session.setValue(QStringLiteral("Default"), nom);
    session.sync();
    return session.status() == QSettings::NoError && actuelle() == (nom.isEmpty() ? machine() : nom);
#else
    Q_UNUSED(nom);
    return false;
#endif
  }

  /// Vrai si MMail peut servir MAPI sur ce poste : déclaré pour la machine,
  /// et sa DLL présente pour les deux architectures.
  Q_INVOKABLE bool mmailInscrit() const
  {
#ifdef Q_OS_WIN
    QSettings mmail(QStringLiteral("HKEY_LOCAL_MACHINE\\SOFTWARE\\Clients\\Mail\\MMail"), QSettings::NativeFormat);
    const QString dll = mmail.value(QStringLiteral("DLLPath")).toString();
    if (dll.isEmpty() || !QFileInfo::exists(dll))
      return false;
    // La copie 32 bits, que Windows sert aux logiciels 32 bits à la place de
    // celle de System32.
    const QString windows = qEnvironmentVariable("SystemRoot", QStringLiteral("C:\\Windows"));
    return QFileInfo::exists(windows + QStringLiteral("\\SysWOW64\\") + QFileInfo(dll).fileName());
#else
    return false;
#endif
  }

private:
  static QString valeurParDefaut_(const QString& cle)
  {
    QSettings s(cle, QSettings::NativeFormat);
    return s.value(QStringLiteral("Default")).toString();
  }
};
