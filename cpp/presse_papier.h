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

#ifdef Q_OS_WIN
#  include <windows.h>
#  include <shlobj.h>  // FILEGROUPDESCRIPTORW, CFSTR_FILEDESCRIPTORW, CFSTR_FILECONTENTS
#endif

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

  /// Fichiers « virtuels » du presse-papier (0.8.4) : ceux que le bureau à
  /// distance apporte du poste local dans une session RDS, ou que copie un
  /// logiciel qui n'a pas de fichier sur le disque (pièce jointe d'Outlook,
  /// fichier d'une archive). Windows les décrit (`FileGroupDescriptorW`) et en
  /// livre le contenu à la demande (`FileContents`), sans chemin : Qt ne les
  /// voit pas. Écrits dans `dossier`, chacun dans son sous-dossier ; rend leurs
  /// adresses `file:`. Vide ailleurs que sous Windows.
  Q_INVOKABLE QStringList fichiersVirtuels(const QString& dossier) const
  {
    QStringList liste;
#ifdef Q_OS_WIN
    IDataObject* donnees = nullptr;
    if (FAILED(OleGetClipboard(&donnees)) || !donnees)
      return liste;
    const CLIPFORMAT formatDescripteur = static_cast<CLIPFORMAT>(RegisterClipboardFormatW(CFSTR_FILEDESCRIPTORW));
    const CLIPFORMAT formatContenu = static_cast<CLIPFORMAT>(RegisterClipboardFormatW(CFSTR_FILECONTENTS));
    FORMATETC fd = { formatDescripteur, nullptr, DVASPECT_CONTENT, -1, TYMED_HGLOBAL };
    STGMEDIUM md = {};
    if (SUCCEEDED(donnees->GetData(&fd, &md))) {
      const auto* groupe = static_cast<const FILEGROUPDESCRIPTORW*>(GlobalLock(md.hGlobal));
      if (groupe && QDir().mkpath(dossier)) {
        // 50 fichiers et 100 Mo chacun au plus : au-delà, l'envoi les
        // refuserait de toute façon.
        const qint64 plafond = 100LL * 1024 * 1024;
        const UINT nombre = groupe->cItems < 50 ? groupe->cItems : 50;
        for (UINT i = 0; i < nombre; ++i) {
          const FILEDESCRIPTORW& f = groupe->fgd[i];
          if ((f.dwFlags & FD_ATTRIBUTES) && (f.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY))
            continue;
          // Un nom, sans chemin : « dossier\fichier » pour le contenu d'un
          // dossier copié.
          QString nom = QString::fromWCharArray(f.cFileName);
          nom = nom.mid(nom.lastIndexOf(QLatin1Char('\\')) + 1);
          nom = nom.mid(nom.lastIndexOf(QLatin1Char('/')) + 1);
          if (nom.isEmpty() || nom == QStringLiteral(".") || nom == QStringLiteral(".."))
            continue;
          FORMATETC fc = { formatContenu, nullptr, DVASPECT_CONTENT, static_cast<LONG>(i),
                           static_cast<DWORD>(TYMED_ISTREAM | TYMED_HGLOBAL) };
          STGMEDIUM mc = {};
          if (FAILED(donnees->GetData(&fc, &mc)))
            continue;
          QByteArray octets;
          bool trop = false;
          if (mc.tymed == TYMED_HGLOBAL) {
            const void* p = GlobalLock(mc.hGlobal);
            qint64 taille = static_cast<qint64>(GlobalSize(mc.hGlobal));
            if (f.dwFlags & FD_FILESIZE) {
              const qint64 annoncee = (static_cast<qint64>(f.nFileSizeHigh) << 32) | f.nFileSizeLow;
              if (annoncee < taille)
                taille = annoncee;
            }
            trop = taille > plafond;
            if (p && !trop)
              octets = QByteArray(static_cast<const char*>(p), static_cast<qsizetype>(taille));
            GlobalUnlock(mc.hGlobal);
          } else if (mc.tymed == TYMED_ISTREAM && mc.pstm) {
            char tampon[65536];
            ULONG lus = 0;
            while (SUCCEEDED(mc.pstm->Read(tampon, sizeof tampon, &lus)) && lus > 0) {
              octets.append(tampon, static_cast<qsizetype>(lus));
              if (octets.size() > plafond) {
                trop = true;
                break;
              }
            }
          }
          ReleaseStgMedium(&mc);
          if (trop)
            continue;
          const QString sous = QDir(dossier).filePath(QUuid::createUuid().toString(QUuid::WithoutBraces));
          if (!QDir().mkpath(sous))
            continue;
          QFile fichier(QDir(sous).filePath(nom));
          if (fichier.open(QIODevice::WriteOnly) && fichier.write(octets) == octets.size()) {
            fichier.close();
            liste.append(QUrl::fromLocalFile(fichier.fileName()).toString());
          }
        }
      }
      if (groupe)
        GlobalUnlock(md.hGlobal);
      ReleaseStgMedium(&md);
    }
    donnees->Release();
#else
    Q_UNUSED(dossier);
#endif
    return liste;
  }

Q_SIGNALS:
  void change();
};
