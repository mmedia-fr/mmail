// SPDX-License-Identifier: GPL-3.0-or-later
//
// Connexion d'un compte Microsoft ou Google par Android (0.7.0) : MSAL, et
// les services Google (packaging/android/src/fr/mmedia/mmail/Jetons.java).
// L'interface demande la connexion (connecter) ; l'issue revient en JSON
// (termine), que Main.qml remet à Boite.terminerAndroid. Ailleurs
// qu'Android, rien : disponible est faux.
#pragma once

#include <QtCore/QMetaObject>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QString>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniObject>
#include <QtCore/qcoreapplication_platform.h>
#endif

class ConnexionAndroid : public QObject
{
  Q_OBJECT
  /// Vrai si la connexion passe par Android (MSAL, services Google).
  Q_PROPERTY(bool disponible READ disponible CONSTANT)

public:
  explicit ConnexionAndroid(QObject* parent = nullptr) : QObject(parent)
  {
    instance() = this;
#ifdef Q_OS_ANDROID
    // Le noyau demandera les jetons des comptes connectés par Android dès
    // leur première connexion : le pont est prêt avant.
    const QJniObject contexte = QNativeInterface::QAndroidApplication::context();
    if (contexte.isValid())
      QJniObject::callStaticMethod<void>("fr/mmedia/mmail/Jetons", "preparer", "(Landroid/content/Context;)V",
                                         contexte.object());
#endif
  }

  bool disponible() const
  {
#ifdef Q_OS_ANDROID
    return true;
#else
    return false;
#endif
  }

  /// Connexion d'un compte : `service` « microsoft » ou « google »,
  /// `indice` l'adresse d'un compte à reconnecter (vide pour un nouveau).
  Q_INVOKABLE void connecter(const QString& service, const QString& indice)
  {
#ifdef Q_OS_ANDROID
    const QJniObject contexte = QNativeInterface::QAndroidApplication::context();
    if (!contexte.isValid())
      return;
    QJniObject::callStaticMethod<void>(
        "fr/mmedia/mmail/Jetons", "connecter", "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)V",
        contexte.object(), QJniObject::fromString(service).object<jstring>(),
        QJniObject::fromString(indice).object<jstring>());
#else
    Q_UNUSED(service);
    Q_UNUSED(indice);
#endif
  }

  /// Issue d'une connexion, depuis Java (cpp/jetons_android.cpp), sur
  /// n'importe quel fil : remise à l'interface sur le sien.
  static void remettre(const QString& issue)
  {
    ConnexionAndroid* c = instance();
    if (c)
      QMetaObject::invokeMethod(c, [c, issue] { emit c->termine(issue); }, Qt::QueuedConnection);
  }

signals:
  void termine(const QString& issue);

private:
  static QPointer<ConnexionAndroid>& instance()
  {
    static QPointer<ConnexionAndroid> c;
    return c;
  }
};
