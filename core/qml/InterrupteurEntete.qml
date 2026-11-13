// SPDX-License-Identifier: GPL-3.0-or-later
// Interrupteur : un curseur allumé ou éteint, devant son libellé ; les
// non-lus en pastille après.
//
// Composant en ligne de Main.qml jusqu'à la 0.6.43 : sorti pour être
// éprouvé seul (core/essais-qml), un composant en ligne n'étant visible que
// du sien. Les noms employés sans être déclarés ici (fenetre, boite,
// reglages…) sont ceux de Main.qml.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import fr.mmedia.mmail

ToolButton {
    id: ie
    property bool sombre: fenetre.marque !== null
    property int compteur: 0
    // Le bouton qui glisse, lu par le scénario « lot-2 ».
    readonly property alias bouton: boutonRail
    checkable: true
    leftPadding: 6
    rightPadding: 8
    topPadding: 3
    bottomPadding: 3
    Accessible.role: Accessible.CheckBox
    contentItem: RowLayout {
        spacing: 7
        Rectangle {
            id: rail
            implicitWidth: 28
            implicitHeight: 16
            radius: height / 2
            Layout.alignment: Qt.AlignVCenter
            readonly property color eteint: fenetre.encreEntete(ie.sombre)
            color: ie.checked ? fenetre.accent : Qt.rgba(rail.eteint.r, rail.eteint.g, rail.eteint.b, 0.3)
            Rectangle {
                id: boutonRail
                width: 12
                height: 12
                radius: 6
                y: 2
                x: ie.checked ? rail.width - width - 2 : 2
                color: "#ffffff"
                // Cf. « dureeInterrupteur » : sous « Pomme », le garde du
                // mouvement ; ailleurs, le glissement d'avant la 0.6.21.
                Behavior on x {
                    enabled: fenetre.dureeInterrupteur > 0
                    NumberAnimation { duration: fenetre.dureeInterrupteur }
                }
            }
        }
        Label {
            text: ie.text
            font: ie.font
            color: fenetre.encreEntete(ie.sombre)
            Layout.alignment: Qt.AlignVCenter
        }
        CompteurNonLus {
            visible: ie.compteur > 0
            nombre: ie.compteur
            taille: fenetre.tailleBase
            Layout.alignment: Qt.AlignVCenter
        }
    }
    background: Rectangle {
        radius: 4
        color: ie.hovered ? fenetre.survolEntete(ie.sombre) : "transparent"
    }
}
