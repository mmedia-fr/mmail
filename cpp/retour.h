// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
// Retour de la connexion Microsoft sous Android (0.6.0) : le navigateur rouvre
// MMail par l'adresse fr.mmedia.mmail://oauth?code=…&state=… (cf. le manifeste
// d'Android) ; QDesktopServices la remet ici, et l'interface la passe au noyau
// (Boite.terminerMicrosoft). Ailleurs, la connexion revient par l'écoute
// locale, et cet objet ne reçoit rien.
#include <QObject>
#include <QString>
#include <QUrl>

class RetourConnexion : public QObject
{
  Q_OBJECT

public:
  using QObject::QObject;

public slots:
  void recevoir(const QUrl& url) { emit recu(url.toString(QUrl::FullyEncoded)); }

signals:
  void recu(const QString& url);
};
