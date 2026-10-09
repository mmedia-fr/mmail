// SPDX-License-Identifier: GPL-3.0-or-later
//
// Accès au presse-papier du système, que QML n'expose pas : il ne sait que
// copier depuis un champ de texte, pas lire ce qui s'y trouve ni savoir qu'un
// tiers vient d'y déposer quelque chose.
//
// Cet objet ne décide de rien : la règle — protection, écho, origine — est dans
// le noyau Rust (core/src/presse_papier.rs), où elle est testée.
#pragma once

#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QFileInfo>
#include <QtCore/QMimeData>
#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QUrl>
#include <QtCore/QUuid>
#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>
#include <QtGui/QImage>

class PressePapier : public QObject
{
  Q_OBJECT
  Q_PROPERTY(QString texte READ texte NOTIFY change)
public:
  explicit PressePapier(QObject* parent = nullptr)
    : QObject(parent)
  {
    if (QClipboard* presse = QGuiApplication::clipboard())
      connect(presse, &QClipboard::dataChanged, this, &PressePapier::change);
  }

  QString texte() const
  {
    QClipboard* presse = QGuiApplication::clipboard();
    return presse ? presse->text() : QString();
  }

  Q_INVOKABLE void deposer(const QString& texte) const
  {
    if (QClipboard* presse = QGuiApplication::clipboard())
      presse->setText(texte);
  }

  /// Image seule du presse-papier — une capture d'écran, une image copiée
  /// dans un navigateur —, enregistrée dans `dossier` : rend son adresse
  /// `file:`, vide sinon. Avec du texte (des cellules de tableur portent aussi
  /// une image), le collage ordinaire l'emporte.
  Q_INVOKABLE QString enregistrerImage(const QString& dossier) const
  {
    QClipboard* presse = QGuiApplication::clipboard();
    const QMimeData* donnees = presse ? presse->mimeData() : nullptr;
    if (!donnees || !donnees->hasImage() || !donnees->text().trimmed().isEmpty())
      return QString();
    const QImage image = qvariant_cast<QImage>(donnees->imageData());
    if (image.isNull() || !QDir().mkpath(dossier))
      return QString();
    const QString base = QDir(dossier).filePath(QStringLiteral("collee-")
                                                + QUuid::createUuid().toString(QUuid::WithoutBraces));
    // PNG, net pour une capture ; au-delà de 10 Mo, que l'envoi refuserait,
    // JPEG.
    QString chemin = base + QStringLiteral(".png");
    if (!image.save(chemin, "PNG"))
      return QString();
    if (QFileInfo(chemin).size() > 10 * 1024 * 1024) {
      QFile::remove(chemin);
      chemin = base + QStringLiteral(".jpg");
      if (!image.convertToFormat(QImage::Format_RGB32).save(chemin, "JPEG", 90))
        return QString();
    }
    return QUrl::fromLocalFile(chemin).toString();
  }

  /// Dépose l'image d'un fichier dans le presse-papier : pour le scénario
  /// d'essai « coller-image », qui n'a pas d'écran d'où copier.
  Q_INVOKABLE bool deposerImage(const QString& fichier) const
  {
    QClipboard* presse = QGuiApplication::clipboard();
    const QImage image(fichier);
    if (!presse || image.isNull())
      return false;
    presse->setImage(image);
    return true;
  }

  /// Fichiers copiés dans l'explorateur, en adresses `file:`.
  Q_INVOKABLE QStringList fichiers() const
  {
    QStringList liste;
    QClipboard* presse = QGuiApplication::clipboard();
    const QMimeData* donnees = presse ? presse->mimeData() : nullptr;
    if (donnees && donnees->hasUrls())
      for (const QUrl& u : donnees->urls())
        if (u.isLocalFile())
          liste.append(u.toString());
    return liste;
  }

Q_SIGNALS:
  void change();
};
