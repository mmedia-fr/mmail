// SPDX-License-Identifier: GPL-3.0-or-later
//
// Accès au presse-papier du système, que QML n'expose pas : il ne sait que
// copier depuis un champ de texte, pas lire ce qui s'y trouve ni savoir qu'un
// tiers vient d'y déposer quelque chose.
//
// Cet objet ne décide de rien : la règle — protection, écho, origine — est dans
// le noyau Rust (core/src/presse_papier.rs), où elle est testée.
#pragma once

#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>

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

Q_SIGNALS:
  void change();
};
