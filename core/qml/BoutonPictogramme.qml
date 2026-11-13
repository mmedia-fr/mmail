// SPDX-License-Identifier: GPL-3.0-or-later
// Bouton à pictogramme des barres d'actions (barre compacte, choix du mainteneur
// du 10/11), encadré comme les boutons d'action — des boutons plats se
// lisaient mal (01/10) :
// le dessin seul, ou un libellé court ; le nom complet en infobulle (au
// doigt, à l'appui long) et pour les lecteurs d'écran. « avecLibelle » : le
// nom à côté du dessin (« Affichage › Boutons des barres », 12/11).
import QtQuick
import QtQuick.Controls

Button {
    id: bp
    // function(ctx, largeur, hauteur, encre), cf. Main.dessiner…
    property var dessin: null
    // À défaut de dessin : un libellé court.
    property string court: ""
    // Le nom (text) à côté du dessin.
    property bool avecLibelle: false
    property string infobulle: text
    readonly property bool libelleVisible: !bp.dessin || bp.avecLibelle
    implicitWidth: bp.libelleVisible ? contenu.implicitWidth + 18 : (fenetre.tactile ? 40 : 34)
    padding: 4
    implicitHeight: fenetre.tactile ? 40 : 30
    Accessible.name: bp.text
    ToolTip.visible: bp.hovered
    ToolTip.delay: 500
    ToolTip.text: bp.infobulle
    onPressAndHold: ToolTip.show(bp.infobulle, 1500)
    contentItem: Item {
        Row {
            id: contenu
            anchors.centerIn: parent
            spacing: 6
            Canvas {
                visible: !!bp.dessin
                anchors.verticalCenter: parent.verticalCenter
                width: 18
                height: 18
                opacity: bp.enabled ? 1 : 0.4
                property color encre: bp.palette.buttonText
                property var dessin: bp.dessin
                onEncreChanged: requestPaint()
                onDessinChanged: requestPaint()
                onPaint: if (bp.dessin) bp.dessin(getContext("2d"), width, height, encre)
            }
            Label {
                id: etiquette
                visible: bp.libelleVisible
                anchors.verticalCenter: parent.verticalCenter
                text: bp.dessin ? bp.text : bp.court
                color: bp.palette.buttonText
            }
        }
    }
}
