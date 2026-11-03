// SPDX-License-Identifier: GPL-3.0-or-later
//
// Relève en arrière-plan sous Android (0.7.0). MMail quitté ou passé en
// arrière-plan ne relève plus rien : une tâche du système (JobScheduler,
// fr.mmedia.mmail.Releve, packaging/android/src/) regarde la boîte de
// réception de chaque compte toutes les quinze minutes au mieux, réseau
// présent, et montre les messages arrivés en notification (core/src/releve.rs).
//
// Ici, ce que l'interface en tient : programmer la tâche (au lancement, et
// quand la personne change le réglage) et demander la permission de notifier
// (Android 13 et au-delà). MMail au premier plan, la tâche ne fait rien et ses
// notifications s'effacent : Releve.java le voit lui-même. Ailleurs
// qu'Android, rien.
#pragma once

#include <QtCore/QObject>
#include <QtCore/QSettings>
#include <QtCore/QString>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniObject>
#include <QtCore/qcoreapplication_platform.h>
#endif

class Releve : public QObject
{
  Q_OBJECT
  /// Vrai si la relève en arrière-plan est possible (Android).
  Q_PROPERTY(bool disponible READ disponible CONSTANT)
  /// Réglage de la personne, gardé par MMail : relève et notifications en
  /// arrière-plan (oui par défaut).
  Q_PROPERTY(bool actif READ actif WRITE poserActif NOTIFY actifChange)

public:
  explicit Releve(QObject* parent = nullptr) : QObject(parent) {}

  bool disponible() const
  {
#ifdef Q_OS_ANDROID
    return true;
#else
    return false;
#endif
  }

  bool actif() const { return QSettings().value(QStringLiteral("releve/actif"), true).toBool(); }

  void poserActif(bool oui)
  {
    if (oui == actif())
      return;
    QSettings().setValue(QStringLiteral("releve/actif"), oui);
    programmer(m_index);
    emit actifChange();
  }

  /// Programme la tâche pour le profil `index` (chemin de index.sqlite), ou
  /// la retire si le réglage est coupé. Demande au passage la permission de
  /// notifier, si Android la veut et ne l'a pas.
  Q_INVOKABLE void programmer(const QString& index)
  {
    m_index = index;
#ifdef Q_OS_ANDROID
    const QJniObject contexte = QNativeInterface::QAndroidApplication::context();
    if (!contexte.isValid())
      return;
    QJniObject::callStaticMethod<void>("fr/mmedia/mmail/Releve", "programmer",
                                       "(Landroid/content/Context;Ljava/lang/String;Z)V",
                                       contexte.object(), QJniObject::fromString(index).object<jstring>(),
                                       jboolean(actif()));
    if (actif())
      QJniObject::callStaticMethod<void>("fr/mmedia/mmail/Releve", "demanderPermission",
                                         "(Landroid/content/Context;)V", contexte.object());
#endif
  }

signals:
  void actifChange();

private:
  QString m_index;
};
