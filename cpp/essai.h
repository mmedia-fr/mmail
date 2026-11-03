// SPDX-License-Identifier: GPL-3.0-or-later
// Gestes simulés pour les scénarios d'essai (MMAIL_SCENARIO) : ce que QML ne
// sait pas produire lui-même — un cran de molette, un clic sur un élément,
// remis à la fenêtre comme le système le ferait, pour éprouver la liste telle
// qu'un utilisateur la voit. Exposé à QML seulement quand les variables
// d'essai sont là (cf. main.cpp) ; jamais en usage courant.
#pragma once

#include <QtCore/QCoreApplication>
#include <QtCore/QObject>
#include <QtCore/QPointF>
#include <QtGui/QMouseEvent>
#include <QtGui/QWheelEvent>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>

class GestesEssai : public QObject {
  Q_OBJECT

 public:
  using QObject::QObject;

  // Molette sur `element`, au point (x, y) de ses coordonnées : `angle` en
  // huitièmes de degré (120 par cran, négatif vers le bas, comme une souris
  // à crans), `pixels` la distance d'un pavé tactile (0 pour une souris).
  // Vrai si quelqu'un a pris l'événement.
  Q_INVOKABLE bool molette(QQuickItem* element, qreal x, qreal y, int angle, int pixels) {
    if (!element || !element->window())
      return false;
    QQuickWindow* fenetre = element->window();
    const QPointF scene = element->mapToScene(QPointF(x, y));
    const QPointF global = QPointF(fenetre->mapToGlobal(scene.toPoint()));
    QWheelEvent evenement(scene, global, QPoint(0, pixels), QPoint(0, angle), Qt::NoButton, Qt::NoModifier,
                          Qt::NoScrollPhase, false);
    QCoreApplication::sendEvent(fenetre, &evenement);
    return evenement.isAccepted();
  }

  // Clic (appui puis relâche, sur place) de `bouton` (Qt.LeftButton,
  // Qt.RightButton) au point (x, y) de `element`.
  Q_INVOKABLE void clic(QQuickItem* element, qreal x, qreal y, int bouton) {
    if (!element || !element->window())
      return;
    QQuickWindow* fenetre = element->window();
    const QPointF scene = element->mapToScene(QPointF(x, y));
    const QPointF global = QPointF(fenetre->mapToGlobal(scene.toPoint()));
    const auto b = static_cast<Qt::MouseButton>(bouton);
    QMouseEvent appui(QEvent::MouseButtonPress, scene, scene, global, b, b, Qt::NoModifier);
    QCoreApplication::sendEvent(fenetre, &appui);
    QMouseEvent relache(QEvent::MouseButtonRelease, scene, scene, global, b, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(fenetre, &relache);
  }
};
