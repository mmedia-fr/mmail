// SPDX-License-Identifier: GPL-3.0-or-later
//
// Correcteur d'orthographe de la rédaction (0.6.36) : souligne en rouge les
// mots que le dictionnaire ne connaît pas, comme Outlook. Le soulignement est
// un format d'affichage (QSyntaxHighlighter) : il ne touche pas le texte, ni
// le HTML qui part. Le dictionnaire et ses suggestions sont ceux du noyau
// (`orthographe.rs`, objet QML « Orthographe »), joint par son nom de
// méthode : `fautes(texte)` rend « début:longueur » en unités UTF-16.
//
// Le mot sous le curseur n'est pas souligné tant qu'on le tape : il l'est
// quand le curseur le quitte.
//
// Le dessin des vagues revient à l'interface (0.6.33) : Qt Quick ne trace ni
// le soulignement « orthographe » ni le soulignement ondulé d'un format, et
// dessine un soulignement simple dans la couleur du texte — les fautes étaient
// trouvées, rien ne se voyait. Le surligneur garde donc les fautes de chaque
// paragraphe (données du bloc) et publie leurs positions dans « zones », sous
// lesquelles Redaction.qml trace les vagues rouges.
#pragma once

#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QList>
#include <QtCore/QMetaObject>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QPair>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QTimer>
#include <QtCore/QVariantList>
#include <QtCore/QVariantMap>
#include <QtGui/QColor>
#include <QtGui/QSyntaxHighlighter>
#include <QtGui/QTextBlock>
#include <QtGui/QTextCharFormat>
#include <QtGui/QTextCursor>
#include <QtGui/QTextDocument>
#include <QtQuick/QQuickTextDocument>

/// Fautes d'un paragraphe : début et longueur, relatifs au paragraphe.
class FautesBloc : public QTextBlockUserData
{
public:
  QList<QPair<int, int>> fautes;
};

class Correcteur : public QSyntaxHighlighter
{
  Q_OBJECT
  Q_PROPERTY(QQuickTextDocument* document READ documentQuick WRITE setDocumentQuick NOTIFY documentChange)
  Q_PROPERTY(QObject* orthographe READ orthographe WRITE setOrthographe NOTIFY orthographeChange)
  Q_PROPERTY(bool actif READ actif WRITE setActif NOTIFY actifChange)
  Q_PROPERTY(int curseur READ curseur WRITE setCurseur NOTIFY curseurChange)
  /// Mots à souligner, `{debut, fin}` en positions du document : ceux que
  /// l'interface marque d'une vague rouge.
  Q_PROPERTY(QVariantList zones READ zones NOTIFY zonesChange)

public:
  explicit Correcteur(QObject* parent = nullptr)
    : QSyntaxHighlighter(parent)
  {
    // Une frappe relit un paragraphe, un collage plusieurs : une seule
    // publication des zones pour l'ensemble.
    m_publication.setSingleShot(true);
    m_publication.setInterval(120);
    connect(&m_publication, &QTimer::timeout, this, &Correcteur::zonesChange);
  }

  QQuickTextDocument* documentQuick() const { return m_document; }
  void setDocumentQuick(QQuickTextDocument* document)
  {
    if (m_document == document)
      return;
    m_document = document;
    setDocument(document ? document->textDocument() : nullptr);
    Q_EMIT documentChange();
  }

  QObject* orthographe() const { return m_orthographe; }
  void setOrthographe(QObject* orthographe)
  {
    if (m_orthographe == orthographe)
      return;
    m_orthographe = orthographe;
    rehighlight();
    Q_EMIT orthographeChange();
  }

  bool actif() const { return m_actif; }
  void setActif(bool actif)
  {
    if (m_actif == actif)
      return;
    m_actif = actif;
    rehighlight();
    Q_EMIT actifChange();
  }

  int curseur() const { return m_curseur; }
  void setCurseur(int curseur)
  {
    if (m_curseur == curseur)
      return;
    const int ancien = m_curseur;
    m_curseur = curseur;
    // Le paragraphe quitté, et celui où l'on entre : le mot tapé n'est
    // souligné qu'une fois quitté.
    if (QTextDocument* d = document()) {
      const QTextBlock avant = d->findBlock(ancien);
      const QTextBlock apres = d->findBlock(curseur);
      if (avant.isValid())
        rehighlightBlock(avant);
      if (apres.isValid() && apres != avant)
        rehighlightBlock(apres);
    }
    Q_EMIT curseurChange();
  }

  /// Le mot à la position `position` du document, en JSON `{debut, fin,
  /// mot, faute}` ; `faute` : souligné. Vide hors d'un mot.
  Q_INVOKABLE QString motA(int position) const
  {
    QTextDocument* d = document();
    if (!d || !m_orthographe)
      return {};
    const QTextBlock bloc = d->findBlock(position);
    if (!bloc.isValid())
      return {};
    const QString texte = bloc.text();
    const int relatif = position - bloc.position();
    for (const auto& [debut, longueur] : fautes(texte)) {
      if (relatif >= debut && relatif <= debut + longueur) {
        QJsonObject o;
        o.insert(QStringLiteral("debut"), bloc.position() + debut);
        o.insert(QStringLiteral("fin"), bloc.position() + debut + longueur);
        o.insert(QStringLiteral("mot"), texte.mid(debut, longueur));
        o.insert(QStringLiteral("faute"), true);
        return QString::fromUtf8(QJsonDocument(o).toJson(QJsonDocument::Compact));
      }
    }
    return {};
  }

  /// Remplace le texte de `debut` à `fin` (une suggestion choisie), dans le
  /// format du premier caractère remplacé.
  Q_INVOKABLE void remplacer(int debut, int fin, const QString& texte)
  {
    QTextDocument* d = document();
    if (!d || fin < debut)
      return;
    QTextCursor c(d);
    c.setPosition(debut);
    c.setPosition(fin, QTextCursor::KeepAnchor);
    c.insertText(texte);
  }

  /// Relit tout le document : un mot ajouté au dictionnaire.
  Q_INVOKABLE void rafraichir() { rehighlight(); }

  QVariantList zones() const
  {
    QVariantList liste;
    QTextDocument* d = document();
    if (!d || !m_actif)
      return liste;
    for (QTextBlock b = d->begin(); b.isValid(); b = b.next()) {
      const auto* donnees = dynamic_cast<const FautesBloc*>(b.userData());
      if (!donnees)
        continue;
      for (const auto& [debut, longueur] : donnees->fautes) {
        QVariantMap z;
        z.insert(QStringLiteral("debut"), b.position() + debut);
        z.insert(QStringLiteral("fin"), b.position() + debut + longueur);
        liste.append(z);
      }
    }
    return liste;
  }

Q_SIGNALS:
  void documentChange();
  void orthographeChange();
  void actifChange();
  void curseurChange();
  void zonesChange();

protected:
  void highlightBlock(const QString& texte) override
  {
    auto* donnees = new FautesBloc;
    if (m_actif && m_orthographe && !texte.isEmpty()) {
      const int curseurDansBloc = m_curseur - currentBlock().position();
      for (const auto& [debut, longueur] : fautes(texte)) {
        // Le mot en train d'être tapé : pas encore.
        if (curseurDansBloc >= debut && curseurDansBloc <= debut + longueur)
          continue;
        donnees->fautes.append({ debut, longueur });
      }
    }
    setCurrentBlockUserData(donnees);
    m_publication.start();
  }

private:
  QList<QPair<int, int>> fautes(const QString& texte) const
  {
    QList<QPair<int, int>> liste;
    QString rendu;
    if (!QMetaObject::invokeMethod(m_orthographe, "fautes", Qt::DirectConnection, Q_RETURN_ARG(QString, rendu),
                                   Q_ARG(QString, texte)))
      return liste;
    const QStringList morceaux = rendu.split(QLatin1Char(' '), Qt::SkipEmptyParts);
    for (const QString& m : morceaux) {
      const int deux = m.indexOf(QLatin1Char(':'));
      if (deux > 0)
        liste.append({ m.left(deux).toInt(), m.mid(deux + 1).toInt() });
    }
    return liste;
  }

  QPointer<QQuickTextDocument> m_document;
  QPointer<QObject> m_orthographe;
  bool m_actif = true;
  int m_curseur = -1;
  QTimer m_publication;
};
