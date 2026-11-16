// SPDX-License-Identifier: GPL-3.0-or-later
// Champ d'une date (jj/mm/aaaa) de la fenêtre d'événement (mmail#175).
// - Saisie souple : « 17/11/26 », « 17-11-2026 », « 17.11.2026 », « 17 11 »,
//   « 171126 », « 17112026 », « 1711 » ou « 17 » se remettent en forme en
//   sortant du champ (17/11/2026). Sans année : l'année en cours, ou la
//   suivante si la date est passée de plus de six mois ; sans mois : le mois
//   en cours. Ce qui ne se lit pas reste tel quel : l'enregistrement le
//   signale.
// - À la souris : la molette, comme les flèches haut et bas, avance ou recule
//   d'un jour ; le pictogramme de calendrier demande le petit calendrier
//   (`calendrierDemande`), que la fenêtre d'événement porte.
// `valeurChangee` part après chaque changement, saisi ou réglé.
//
// Le menu du clic droit (MenuSaisie) se déclare là où le champ sert.
import QtQuick
import QtQuick.Controls

TextField {
    id: champ
    property bool tactile: false
    // Jour de référence des dates sans année ou sans mois ; aujourd'hui, sauf
    // dans les épreuves.
    property var reference: null
    property real cumulMolette: 0
    signal valeurChangee()
    signal calendrierDemande()

    placeholderText: qsTr("jj/mm/aaaa")
    rightPadding: boutonCalendrier.width + 4
    inputMethodHints: Qt.ImhPreferNumbers

    function deuxChiffres(n) {
        return (n < 10 ? "0" : "") + n
    }

    function texteDe(d) {
        return deuxChiffres(d.getDate()) + "/" + deuxChiffres(d.getMonth() + 1) + "/" + d.getFullYear()
    }

    // Texte saisi → « jj/mm/aaaa », ou "" si ce n'est pas une date.
    function lire(texte) {
        var t = (texte || "").trim()
        var ref = reference || new Date()
        var j, m, a = -1
        if (/^\d+$/.test(t)) {
            if (t.length <= 2) {
                j = Number(t)
                m = ref.getMonth() + 1
                a = ref.getFullYear()
            } else if (t.length === 4 || t.length === 6 || t.length === 8) {
                j = Number(t.slice(0, 2))
                m = Number(t.slice(2, 4))
                if (t.length === 6)
                    a = 2000 + Number(t.slice(4))
                else if (t.length === 8)
                    a = Number(t.slice(4))
            } else {
                return ""
            }
        } else {
            var p = /^(\d{1,2})\s*[\/.,\s-]\s*(\d{1,2})(?:\s*[\/.,\s-]\s*(\d{4}|\d{2}))?$/.exec(t)
            if (!p)
                return ""
            j = Number(p[1])
            m = Number(p[2])
            if (p[3])
                a = p[3].length === 2 ? 2000 + Number(p[3]) : Number(p[3])
        }
        if (m < 1 || m > 12 || j < 1)
            return ""
        if (a < 0) {
            a = ref.getFullYear()
            var limite = new Date(ref.getFullYear(), ref.getMonth() - 6, ref.getDate())
            if (new Date(a, m - 1, j) < limite)
                a += 1
        }
        var d = new Date(a, m - 1, j)
        if (d.getDate() !== j || d.getMonth() !== m - 1 || d.getFullYear() !== a)
            return ""
        return texteDe(d)
    }

    // Un jour plus tard (sens 1) ou plus tôt (-1) ; sans date lisible, rien.
    function decaler(sens) {
        var l = lire(text)
        if (l.length === 0)
            return
        var d = new Date(Number(l.substr(6, 4)), Number(l.substr(3, 2)) - 1, Number(l.substr(0, 2)) + sens)
        text = texteDe(d)
        valeurChangee()
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
        id: boutonCalendrier
        objectName: "boutonCalendrier"
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        height: parent.height - 4
        width: champ.tactile ? 36 : 26
        padding: 0
        focusPolicy: Qt.NoFocus
        ToolTip.visible: hovered
        ToolTip.delay: 500
        ToolTip.text: qsTr("Choisir dans le calendrier")
        Accessible.name: qsTr("Choisir dans le calendrier")
        onClicked: champ.calendrierDemande()
        // Une page de calendrier : cadre, bandeau du haut, deux anneaux et
        // trois rangées de jours.
        contentItem: Canvas {
            id: dessin
            implicitWidth: 16
            implicitHeight: 16
            property color encre: champ.palette.buttonText
            onEncreChanged: requestPaint()
            onPaint: {
                var ctx = getContext("2d")
                ctx.reset()
                var x = Math.round((width - 14) / 2) + 0.5
                var y = Math.round((height - 14) / 2) + 0.5
                ctx.strokeStyle = encre
                ctx.fillStyle = encre
                ctx.lineWidth = 1
                ctx.strokeRect(x, y + 2, 13, 11)
                ctx.fillRect(x, y + 2, 13, 3)
                ctx.fillRect(x + 3, y, 1.5, 3)
                ctx.fillRect(x + 8.5, y, 1.5, 3)
                for (var r = 0; r < 2; ++r)
                    for (var c = 0; c < 3; ++c)
                        ctx.fillRect(x + 2 + c * 3.5, y + 7 + r * 3, 2, 2)
            }
        }
    }
}
