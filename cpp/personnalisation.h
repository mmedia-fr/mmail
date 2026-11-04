// SPDX-License-Identifier: GPL-3.0-or-later
// Personnalisation d'une organisation (0.8.6) : nom du logiciel, couleurs,
// logo — et icône, lue par main.cpp.
//
// Le dossier `personnalisation` vient du poste (réglage `Personnalisation`,
// ou à côté du programme), ou du profil : sous Android, sans dossier
// d'installation, on l'y importe depuis l'affichage (`Personnalisation`,
// exposé à QML).
#pragma once

#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QFileInfo>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QObject>
#include <QtCore/QRegularExpression>
#include <QtCore/QSaveFile>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QUrl>
#include <QtCore/QVariantMap>

/// Taille au-delà de laquelle `personnalisation.json` est écarté.
constexpr qint64 TAILLE_MAX_PARAMETRES = 64 * 1024;
/// Taille au-delà de laquelle le logo est écarté.
constexpr qint64 TAILLE_MAX_LOGO = 2 * 1024 * 1024;

/// Ce que l'interface reprend de `personnalisation.json` :
/// - `nom` : celui de l'apparence de l'organisation (40 caractères au plus) ;
/// - `logiciel` : le nom du logiciel, en tête du bandeau et au titre de la
///   fenêtre, à la place de « MMail » (30 caractères au plus) ;
/// - `apparence` : JSON de la forme de l'apparence personnelle (0.6.37),
///   `{base, bandeau, accent, surbrillance, fond, lien}`, couleurs en
///   `#RRGGBB` ; une couleur invalide est écartée, une apparence sans `base`
///   aussi ;
/// - `imposer` : pas d'autre apparence au choix.
/// Vide si le texte n'est pas un objet JSON.
inline QVariantMap analyserParametres(const QByteArray& texte)
{
  QVariantMap p;
  QJsonParseError erreur;
  const QJsonDocument document = QJsonDocument::fromJson(texte, &erreur);
  if (!document.isObject()) {
    qWarning("personnalisation : personnalisation.json illisible (%s)", qUtf8Printable(erreur.errorString()));
    return p;
  }
  const QJsonObject o = document.object();
  const QString nom = o.value(QStringLiteral("nom")).toString().simplified().left(40);
  if (!nom.isEmpty())
    p.insert(QStringLiteral("nom"), nom);
  const QString logiciel = o.value(QStringLiteral("logiciel")).toString().simplified().left(30);
  if (!logiciel.isEmpty())
    p.insert(QStringLiteral("logiciel"), logiciel);
  const QJsonObject a = o.value(QStringLiteral("apparence")).toObject();
  QJsonObject propre;
  const QString base = a.value(QStringLiteral("base")).toString().trimmed();
  if (!base.isEmpty())
    propre.insert(QStringLiteral("base"), base);
  static const QRegularExpression forme(QStringLiteral("^#[0-9A-Fa-f]{6}$"));
  for (const QString& cle : {QStringLiteral("bandeau"), QStringLiteral("accent"), QStringLiteral("surbrillance"),
                             QStringLiteral("fond"), QStringLiteral("lien")}) {
    const QString couleur = a.value(cle).toString().trimmed();
    if (forme.match(couleur).hasMatch())
      propre.insert(cle, couleur);
    else if (!couleur.isEmpty())
      qWarning("personnalisation : couleur « %s » écartée pour %s", qUtf8Printable(couleur), qUtf8Printable(cle));
  }
  if (propre.contains(QStringLiteral("base"))) {
    p.insert(QStringLiteral("apparence"), QString::fromUtf8(QJsonDocument(propre).toJson(QJsonDocument::Compact)));
    p.insert(QStringLiteral("imposer"), o.value(QStringLiteral("imposer")).toBool(false));
  } else if (!a.isEmpty()) {
    qWarning("personnalisation : apparence sans « base », écartée");
  }
  return p;
}

/// La personnalisation du dossier `dossier` : les paramètres ci-dessus, plus
/// `logo` (`logo.svg` ou `logo.png`, 2 Mio au plus, en adresse de fichier),
/// `dossier`, et `origine` (« poste » ou « profil »). Vide sans dossier. Un
/// fichier illisible n'empêche rien : MMail garde ses couleurs.
inline QVariantMap lirePersonnalisation(const QString& dossier, const QString& origine)
{
  QVariantMap p;
  if (dossier.isEmpty())
    return p;
  const QDir d(dossier);
  QFile fichier(d.filePath(QStringLiteral("personnalisation.json")));
  if (fichier.exists()) {
    if (fichier.size() <= TAILLE_MAX_PARAMETRES && fichier.open(QIODevice::ReadOnly))
      p = analyserParametres(fichier.readAll());
    else
      qWarning("personnalisation : personnalisation.json illisible, ou de plus de 64 Kio");
  }
  p.insert(QStringLiteral("dossier"), dossier);
  p.insert(QStringLiteral("origine"), origine);
  for (const QString& nom : {QStringLiteral("logo.svg"), QStringLiteral("logo.png")}) {
    const QFileInfo info(d.filePath(nom));
    if (info.isFile() && info.size() <= TAILLE_MAX_LOGO) {
      p.insert(QStringLiteral("logo"), QUrl::fromLocalFile(info.absoluteFilePath()).toString());
      break;
    }
  }
  return p;
}

/// Importation d'une personnalisation dans le profil, depuis l'affichage :
/// la voie d'Android, qui n'a pas de dossier d'installation.
class Personnalisation : public QObject
{
  Q_OBJECT

public:
  using QObject::QObject;

  /// Dossier de la personnalisation importée : `personnalisation`, à côté
  /// de l'index du profil (`index`, cf. `cheminProfil`).
  static QString dossierDuProfil(const QString& index)
  {
    return QFileInfo(index).absoluteDir().filePath(QStringLiteral("personnalisation"));
  }

  /// Range dans le profil les fichiers choisis : `personnalisation.json`,
  /// et un logo PNG ou SVG s'il y en a un. Ils se reconnaissent à leur
  /// contenu : sous Android, l'adresse « content:// » ne porte pas toujours
  /// le nom du fichier. Rend la personnalisation importée (cf.
  /// `lirePersonnalisation`), ou `{erreur}`. Une importation remplace la
  /// précédente.
  Q_INVOKABLE QVariantMap importer(const QStringList& adresses, const QString& index)
  {
    QByteArray parametres;
    QByteArray logo;
    QString extension;
    for (const QString& adresse : adresses) {
      const QUrl url(adresse);
      QFile fichier(url.isLocalFile() ? url.toLocalFile() : adresse);
      if (!fichier.open(QIODevice::ReadOnly))
        return erreur(tr("Fichier illisible : %1").arg(fichier.errorString()));
      const QByteArray octets = fichier.read(TAILLE_MAX_LOGO + 1);
      if (octets.startsWith("\x89PNG\r\n\x1a\n") || estSvg(octets)) {
        if (octets.size() > TAILLE_MAX_LOGO)
          return erreur(tr("Le logo dépasse 2 Mio."));
        if (!logo.isEmpty())
          return erreur(tr("Deux images choisies : un seul logo, en PNG ou en SVG."));
        logo = octets;
        extension = estSvg(octets) ? QStringLiteral("svg") : QStringLiteral("png");
        continue;
      }
      if (octets.size() > TAILLE_MAX_PARAMETRES || !QJsonDocument::fromJson(octets).isObject())
        return erreur(tr("Ce fichier n'est ni le fichier personnalisation.json, ni un logo PNG ou SVG."));
      if (!parametres.isEmpty())
        return erreur(tr("Deux fichiers de paramètres choisis : un seul personnalisation.json."));
      parametres = octets;
    }
    if (parametres.isEmpty())
      return erreur(tr("Choisissez le fichier personnalisation.json, avec le logo s'il y en a un."));
    const QVariantMap lus = analyserParametres(parametres);
    if (!lus.contains(QStringLiteral("apparence")) && !lus.contains(QStringLiteral("logiciel"))
        && !lus.contains(QStringLiteral("nom")))
      return erreur(tr("personnalisation.json ne porte ni apparence, ni nom."));

    const QString dossier = dossierDuProfil(index);
    if (!QDir().mkpath(dossier))
      return erreur(tr("Le dossier %1 ne peut pas être créé.").arg(dossier));
    if (!ecrire(QDir(dossier).filePath(QStringLiteral("personnalisation.json")), parametres))
      return erreur(tr("personnalisation.json ne peut pas être écrit dans le profil."));
    for (const QString& ancien : {QStringLiteral("logo.png"), QStringLiteral("logo.svg")})
      QFile::remove(QDir(dossier).filePath(ancien));
    if (!logo.isEmpty() && !ecrire(QDir(dossier).filePath(QStringLiteral("logo.") + extension), logo))
      return erreur(tr("Le logo ne peut pas être écrit dans le profil."));
    qInfo("personnalisation : importée dans %s | logo %s", qUtf8Printable(dossier), logo.isEmpty() ? "non" : "oui");
    return lirePersonnalisation(dossier, QStringLiteral("profil"));
  }

  /// Retire la personnalisation importée dans le profil : ses fichiers, et
  /// le dossier s'il est vide. Vrai s'il ne reste rien.
  Q_INVOKABLE bool retirer(const QString& index)
  {
    const QDir dossier(dossierDuProfil(index));
    if (!dossier.exists())
      return true;
    for (const QString& nom : {QStringLiteral("personnalisation.json"), QStringLiteral("logo.png"),
                               QStringLiteral("logo.svg")})
      QFile::remove(dossier.filePath(nom));
    QDir().rmdir(dossier.absolutePath());
    return !QFileInfo::exists(dossier.filePath(QStringLiteral("personnalisation.json")));
  }

private:
  static QVariantMap erreur(const QString& texte) { return {{QStringLiteral("erreur"), texte}}; }

  /// Un SVG : une balise `<svg` dans le début du fichier, après l'éventuel
  /// prologue XML.
  static bool estSvg(const QByteArray& octets)
  {
    const QByteArray debut = octets.left(4096).trimmed();
    return debut.startsWith('<') && debut.contains("<svg");
  }

  static bool ecrire(const QString& chemin, const QByteArray& octets)
  {
    QSaveFile fichier(chemin);
    return fichier.open(QIODevice::WriteOnly) && fichier.write(octets) == octets.size() && fichier.commit();
  }
};
