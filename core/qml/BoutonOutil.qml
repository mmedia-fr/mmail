// SPDX-License-Identifier: GPL-3.0-or-later
// Bouton : un cadre ; plein pour l'action principale. « dessin » : son
// icône (cf. dessinerPlus), « iconeSeule » : sans libellé.
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
    id: bo
    property bool principal: false
    property bool sombre: fenetre.marque !== null
    property var dessin: null
    property bool iconeSeule: false
    // Nom complet et raccourci, au survol (barre compacte, 11/11) : d'office le
    // libellé.
    property string infobulle: text
    readonly property color encre: bo.principal ? fenetre.encreSurAccent
                                                : fenetre.encreEntete(bo.sombre)
    font.bold: bo.principal
    leftPadding: bo.iconeSeule ? 7 : 12
    rightPadding: bo.iconeSeule ? 7 : 14
    topPadding: 3
    bottomPadding: 3
    ToolTip.visible: bo.hovered && (bo.iconeSeule || bo.infobulle !== bo.text)
    ToolTip.delay: 500
    ToolTip.text: bo.infobulle
    Accessible.name: bo.text
    contentItem: RowLayout {
        spacing: 6
        Canvas {
            visible: bo.dessin !== null
            Layout.preferredWidth: bo.iconeSeule ? 15 : 13
            Layout.preferredHeight: bo.iconeSeule ? 15 : 13
            Layout.alignment: Qt.AlignVCenter
            property color encre: bo.encre
            property var dessin: bo.dessin
            onEncreChanged: requestPaint()
            onDessinChanged: requestPaint()
            onWidthChanged: requestPaint()
            onPaint: if (bo.dessin) bo.dessin(getContext("2d"), width, height, encre)
        }
        Label {
            visible: !bo.iconeSeule
            text: bo.text
            font: bo.font
            color: bo.encre
            Layout.alignment: Qt.AlignVCenter
        }
    }
    background: Rectangle {
        implicitHeight: 24
        radius: fenetre.marque ? height / 2 : 3
        color: bo.principal ? (bo.down ? Qt.darker(fenetre.accent, 1.15) : bo.hovered ? Qt.lighter(fenetre.accent, 1.08) : fenetre.accent)
             : bo.down || bo.hovered ? fenetre.survolEntete(bo.sombre)
             : bo.sombre && fenetre.marque ? "transparent" : fenetre.palette.button
        border.width: bo.principal ? 0 : 1
        border.color: bo.sombre && fenetre.marque ? Qt.rgba(1, 1, 1, 0.45)
                    : Qt.rgba(fenetre.palette.windowText.r, fenetre.palette.windowText.g, fenetre.palette.windowText.b, 0.32)
        Behavior on color {
            enabled: fenetre.anime
            ColorAnimation { duration: fenetre.dureeTeinte }
        }
        // Sur le fond, jamais sur le contrôle : cf. BoutonEntete.
        scale: bo.down && fenetre.anime ? 0.97 : 1
        Behavior on scale {
            enabled: fenetre.anime
            NumberAnimation { duration: fenetre.dureeAppui; easing.type: Easing.OutQuad }
        }
    }
}
