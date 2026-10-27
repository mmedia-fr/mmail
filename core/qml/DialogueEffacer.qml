// SPDX-License-Identifier: GPL-3.0-or-later
// Confirmation « Effacer définitivement » (0.6.24) : « Supprimer » dans la
// corbeille ou les indésirables, d'où un message n'a plus de corbeille où
// aller.
//
// Les noms employés sans être déclarés ici (fenetre, boite, messageEtat…)
// sont ceux de Main.qml, où « DialogueEffacer { id: dlgEffacer } »
// l'instancie ; la fenêtre d'un message a le sien.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import fr.mmedia.mmail

Dialog {
    id: dlgEffacer
    // Fenêtre d'un message qui ouvre ce dialogue pour son message ; sans elle,
    // la fenêtre principale.
    property var pour: null
    property int compte: 0
    property string chemin: ""
    property var uids: []
    // Ce que l'interface fait une fois l'effacement parti (retirer les
    // lignes), et à défaut (remettre une ligne glissée).
    property var fait: null
    property var annule: null
    function ouvrir(c, ch, u, f, a) {
        compte = c; chemin = ch; uids = u; fait = f || null; annule = a || null
        open()
    }
    title: qsTr("Effacer définitivement ?")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(480, (pour ? pour.width : fenetre.width) - 24)
    standardButtons: Dialog.Yes | Dialog.No
    RowLayout {
        width: dlgEffacer.availableWidth
        spacing: 12
        Avertissement {}
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: dlgEffacer.uids.length > 1
                  ? qsTr("Effacer définitivement ces %1 messages ? Ils sont retirés du serveur, sans repasser par une corbeille : l'action est irréversible.").arg(dlgEffacer.uids.length)
                  : qsTr("Effacer définitivement ce message ? Il est retiré du serveur, sans repasser par une corbeille : l'action est irréversible.")
        }
    }
    onAccepted: {
        if (!boite.effacerDans(compte, chemin, uids.join(","))) {
            messageEtat.texte = boite.erreur
            if (annule)
                annule()
            return
        }
        if (fait)
            fait()
        messageEtat.texte = qsTr("Effacement de %1…").arg(fenetre.accord(uids.length, qsTr("message"), qsTr("messages")))
    }
    onRejected: {
        if (annule)
            annule()
    }
}
