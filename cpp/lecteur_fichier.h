// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lecture d'un fichier choisi dans le sélecteur du système, pour le noyau :
// sous Android, le sélecteur rend une adresse « content:// » que seul Qt sait
// ouvrir (le noyau Rust ne lit que des chemins). Sert à l'import des
// certificats S/MIME (0.8.0), petits fichiers : le contenu passe en base64.
#pragma once

#include <QtCore/QByteArray>
#include <QtCore/QFile>
#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtCore/QUrl>

class LecteurFichier : public QObject
{
  Q_OBJECT
public:
  explicit LecteurFichier(QObject* parent = nullptr)
    : QObject(parent)
  {
  }

  // Contenu du fichier en base64 ; vide s'il ne s'ouvre pas ou dépasse
  // `plafond` octets.
  Q_INVOKABLE QString base64(const QString& adresse, int plafond = 1 << 20) const
  {
    const QUrl url(adresse);
    const QString chemin = url.isLocalFile() ? url.toLocalFile() : adresse;
    QFile fichier(chemin);
    if (!fichier.open(QIODevice::ReadOnly))
      return QString();
    const QByteArray octets = fichier.read(qint64(plafond) + 1);
    if (octets.isEmpty() || octets.size() > plafond)
      return QString();
    return QString::fromLatin1(octets.toBase64());
  }
};
