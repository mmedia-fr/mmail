// SPDX-License-Identifier: GPL-3.0-or-later
// Champ d'une heure (hh:mm) de la fenêtre d'événement (mmail#175).
// - Saisie souple : « 1356 », « 13h56 », « 13,56 », « 13 56 », « 9h » ou
//   « 13 » se remettent en forme en sortant du champ (13:56, 09:00, 13:00).
//   Ce qui ne se lit pas reste tel quel : l'enregistrement le signale.
// - À la souris : la molette, comme les flèches haut et bas, avance ou recule
//   d'un quart d'heure, calé sur le quart d'heure rond (13:56 → 14:00 →
//   14:15) ; « ▾ » propose les heures de la journée par quart d'heure.
// `valeurChangee` part après chaque changement, saisi ou choisi : la fenêtre
// d'événement y fait suivre la fin.
//
// Le menu du clic droit (MenuSaisie) se déclare là où le champ sert.
import QtQuick
import QtQuick.Controls

TextField {
    id: champ
    // Au doigt : bouton et lignes de la liste plus hauts.
    property bool tactile: false
    // Cumul de la molette : un cran de souris vaut 120 ; un pavé tactile
    // envoie des fractions.
    property real cumulMolette: 0
    // La liste des heures, pour les épreuves.
    readonly property alias liste: listeHeures
    signal valeurChangee()

    placeholderText: qsTr("hh:mm")
    rightPadding: boutonHeures.width + 4
    inputMethodHints: Qt.ImhPreferNumbers

    function deuxChiffres(n) {
        return (n < 10 ? "0" : "") + n
    }

    // Texte saisi → « hh:mm », ou "" si ce n'est pas une heure.
    function lire(texte) {
        var t = (texte || "").trim().toLowerCase()
        var h, mn
        if (/^\d{3,4}$/.test(t)) {
            h = Number(t.slice(0, t.length - 2))
            mn = Number(t.slice(t.length - 2))
        } else {
            var m = /^(\d{1,2})(?:\s*[:h.,\s-]\s*(\d{0,2}))?$/.exec(t)
            if (!m)
                return ""
            h = Number(m[1])
            mn = m[2] ? Number(m[2]) : 0
        }
        if (h > 23 || mn > 59)
            return ""
        return deuxChiffres(h) + ":" + deuxChiffres(mn)
    }

    // Minutes depuis minuit de l'heure du champ, -1 si elle ne se lit pas.
    function minutes() {
        var l = lire(text)
        return l.length > 0 ? Number(l.substr(0, 2)) * 60 + Number(l.substr(3, 2)) : -1
    }

    function poser(m) {
        text = deuxChiffres(Math.floor(m / 60)) + ":" + deuxChiffres(m % 60)
        valeurChangee()
    }

    // Un quart d'heure plus tard (sens 1) ou plus tôt (-1), sur le quart
    // d'heure rond, sans passer minuit ; sans heure lisible, rien.
    function decaler(sens) {
        var m = minutes()
        if (m < 0)
            return
        var q = sens > 0 ? (Math.floor(m / 15) + 1) * 15 : (Math.ceil(m / 15) - 1) * 15
        if (q >= 0 && q <= 23 * 60 + 45)
            poser(q)
    }

    // Le quart d'heure `n` de la liste (0 : 00:00, 95 : 23:45).
    function choisirQuart(n) {
        listeHeures.close()
        poser(n * 15)
    }

    onEditingFinished: {
        var l = lire(text)
        if (l.length > 0 && l !== text)
            text = l
        valeurChangee()
    }
    Keys.onUpPressed: decaler(1)
    Keys.onDownPressed: decaler(-1)

    WheelHandler {
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        onWheel: function(evenement) {
            champ.cumulMolette += evenement.angleDelta.y
            while (Math.abs(champ.cumulMolette) >= 120) {
                var sens = champ.cumulMolette > 0 ? 1 : -1
                champ.cumulMolette -= sens * 120
                champ.decaler(sens)
            }
        }
    }

    ToolButton {
        id: boutonHeures
        objectName: "boutonHeures"
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: parent.height - 4
        width: champ.tactile ? 36 : 24
        padding: 0
        focusPolicy: Qt.NoFocus
        text: "▾"
        ToolTip.visible: hovered
        ToolTip.delay: 500
        ToolTip.text: qsTr("Choisir l'heure")
        onClicked: listeHeures.opened ? listeHeures.close() : listeHeures.open()
    }

    Popup {
        id: listeHeures
        objectName: "listeHeures"
        y: champ.height
        width: Math.max(champ.width, 90)
        height: 260
        padding: 2
        onOpened: {
            var m = champ.minutes()
            vueHeures.positionViewAtIndex(m < 0 ? 36 : Math.floor(m / 15), ListView.Center)
        }
        ListView {
            id: vueHeures
            anchors.fill: parent
            clip: true
            model: 96
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            delegate: ItemDelegate {
                required property int index
                width: vueHeures.width
                height: champ.tactile ? 40 : 26
                text: champ.deuxChiffres(Math.floor(index / 4)) + ":" + champ.deuxChiffres(index % 4 * 15)
                highlighted: Math.floor(champ.minutes() / 15) === index
                onClicked: champ.choisirQuart(index)
            }
        }
    }
}
