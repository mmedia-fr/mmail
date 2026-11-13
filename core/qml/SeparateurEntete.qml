// SPDX-License-Identifier: GPL-3.0-or-later

//
// Composant en ligne de Main.qml jusqu'à la 0.6.43 : sorti pour être
// éprouvé seul (core/essais-qml), un composant en ligne n'étant visible que
// du sien. Les noms employés sans être déclarés ici (fenetre, boite,
// reglages…) sont ceux de Main.qml.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import fr.mmedia.mmail

Rectangle {
    property bool sombre: fenetre.marque !== null
    readonly property color encre: fenetre.encreEntete(sombre)
    implicitWidth: 1
    Layout.preferredHeight: 20
    Layout.alignment: Qt.AlignVCenter
    Layout.leftMargin: fenetre.enteteEtroite ? 2 : 6
    Layout.rightMargin: Layout.leftMargin
    color: Qt.rgba(encre.r, encre.g, encre.b, 0.25)
}
