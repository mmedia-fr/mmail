// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
//
// Coffre des mots de passe : le magasin de secrets du système, jamais un
// fichier du programme (décision 18 du dossier de projet).
//
// L'implantation est QtKeychain, compilée avec le programme :
//   - Windows : Gestionnaire d'identifiants (Credential Manager), chiffré par
//     DPAPI pour la session de l'utilisateur ;
//   - Linux   : Secret Service (GNOME Keyring, KeePassXC…) par libsecret, ou
//     KWallet par D-Bus ;
//   - Android : Android Keystore ;
//   - macOS   : Trousseau (construit par portabilité, non distribué).
//
// Aucun repli en clair : si le système n'offre pas de coffre, l'opération
// échoue et l'interface le dit — le mot de passe sera redemandé.
//
// Toutes les opérations sont asynchrones : le Trousseau, KWallet ou le
// déverrouillage d'un coffre GNOME peuvent solliciter l'utilisateur.

#include <QtCore/QObject>
#include <QtCore/QString>

#include <qtkeychain/keychain.h>

class Coffre : public QObject
{
  Q_OBJECT
  Q_PROPERTY(QString service READ service CONSTANT)

public:
  explicit Coffre(QObject* parent = nullptr) : QObject(parent) {}

  /// Nom sous lequel les secrets apparaissent dans le coffre du système.
  QString service() const { return QStringLiteral("MMail"); }

  /// Lit le secret d'un compte. Issue : signal `lu`.
  Q_INVOKABLE void lire(const QString& compte)
  {
    auto* travail = new QKeychain::ReadPasswordJob(service(), this);
    travail->setAutoDelete(true);
    travail->setInsecureFallback(false);
    travail->setKey(compte);
    connect(travail, &QKeychain::Job::finished, this, [this, travail, compte](QKeychain::Job*) {
      switch (travail->error()) {
      case QKeychain::NoError:
        emit lu(compte, travail->textData(), true, QString());
        break;
      case QKeychain::EntryNotFound:
        emit lu(compte, QString(), false, QString());
        break;
      default:
        emit lu(compte, QString(), false, travail->errorString());
      }
    });
    travail->start();
  }

  /// Écrit — ou remplace — le secret d'un compte. Issue : signal `ecrit`.
  Q_INVOKABLE void ecrire(const QString& compte, const QString& secret)
  {
    auto* travail = new QKeychain::WritePasswordJob(service(), this);
    travail->setAutoDelete(true);
    travail->setInsecureFallback(false);
    travail->setKey(compte);
    travail->setTextData(secret);
    connect(travail, &QKeychain::Job::finished, this, [this, travail, compte](QKeychain::Job*) {
      const bool ok = travail->error() == QKeychain::NoError;
      emit ecrit(compte, ok, ok ? QString() : travail->errorString());
    });
    travail->start();
  }

  /// Retire le secret d'un compte. Issue : signal `efface`. Un secret déjà
  /// absent n'est pas une erreur.
  Q_INVOKABLE void effacer(const QString& compte)
  {
    auto* travail = new QKeychain::DeletePasswordJob(service(), this);
    travail->setAutoDelete(true);
    travail->setInsecureFallback(false);
    travail->setKey(compte);
    connect(travail, &QKeychain::Job::finished, this, [this, travail, compte](QKeychain::Job*) {
      const auto code = travail->error();
      const bool ok = code == QKeychain::NoError || code == QKeychain::EntryNotFound;
      emit efface(compte, ok, ok ? QString() : travail->errorString());
    });
    travail->start();
  }

signals:
  void lu(const QString& compte, const QString& secret, bool trouve, const QString& erreur);
  void ecrit(const QString& compte, bool ok, const QString& erreur);
  void efface(const QString& compte, bool ok, const QString& erreur);
};
