// SPDX-License-Identifier: GPL-3.0-or-later
// « Propriétés… » d'un dossier (0.6.21) : ce que MMail sait de lui, en lecture.
//
// Les noms employés sans être déclarés ici (fenetre, boite…) sont ceux de
// Main.qml, où « DialogueProprietesDossier { id: dlgProprietesDossier } »
// l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import fr.mmedia.mmail

Dialog {
    id: dlgProprietes
    property int compte: 0
    property string chemin: ""
    // Ligne de l'arborescence du dossier, et ce que l'index en sait.
    property var ligne: ({})
    property var index: ({})
    property string texteSynchro: ""

    function ouvrir(c, ch) {
        compte = c
        chemin = ch
        ligne = ({})
        // Dans l'arborescence, ou parmi les favoris si son compte est replié.
        var modeles = [modeleArborescence, modeleFavoris]
        for (var m = 0; m < modeles.length && ligne.libelle === undefined; ++m) {
            for (var i = 0; i < modeles[m].count; ++i) {
                var l = modeles[m].get(i)
                if ((l.genre === "dossier" || l.genre === "favori") && l.compte === c && l.chemin === ch) {
                    ligne = { libelle: fenetre.libelleLigne({ genre: "dossier", nom: l.nom, role: l.role, chemin: l.chemin }),
                              adresse: l.adresse, role: l.role, messages: l.messages, nonLus: l.nonLus }
                    break
                }
            }
        }
        try { index = JSON.parse(boite.proprietesDossier(c, ch)) } catch (e) { index = ({}) }
        texteSynchro = derniereSynchro()
        open()
    }

    /// Nature d'un dossier spécial, ou vide.
    function nature(role) {
        return role === "Sent" ? qsTr("éléments envoyés")
             : role === "Drafts" ? qsTr("brouillons")
             : role === "Trash" ? qsTr("corbeille")
             : role === "Junk" ? qsTr("courrier indésirable")
             : role === "Archive" ? qsTr("archives")
             : ""
    }

    function derniereSynchro() {
        var quand = fenetre.synchrosDossiers[compte + "\u0001" + chemin]
        if (!quand)
            return qsTr("pas encore depuis l'ouverture de MMail")
        return fenetre.dateLongue(quand)
    }

    title: qsTr("Propriétés de « %1 »").arg(ligne.libelle || chemin)
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(520, fenetre.width - 24)
    standardButtons: Dialog.Close

    GridLayout {
        width: dlgProprietes.availableWidth
        columns: 2
        columnSpacing: 12
        rowSpacing: 6
        Label { text: qsTr("Compte :"); opacity: 0.7 }
        Label { text: ligne.adresse || ""; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Label { text: qsTr("Chemin :"); opacity: 0.7 }
        TextEdit {
            objectName: "cheminProprietes"
            text: dlgProprietes.chemin
            readOnly: true
            selectByMouse: true
            wrapMode: TextEdit.Wrap
            color: fenetre.palette.windowText
            font: fenetre.font
            Layout.fillWidth: true
        }
        Label { text: qsTr("Nature :"); opacity: 0.7; visible: nature(ligne.role || "").length > 0 }
        Label { text: nature(ligne.role || ""); visible: text.length > 0 }
        Label { text: qsTr("Messages :"); opacity: 0.7 }
        Label {
            objectName: "messagesProprietes"
            // Le serveur d'abord ; à défaut, l'index.
            text: (ligne.messages || index.messages || 0).toLocaleString(Qt.locale("fr_FR"), "f", 0)
        }
        Label { text: qsTr("Non lus :"); opacity: 0.7 }
        Label { text: (ligne.nonLus !== undefined ? ligne.nonLus : index.nonLus || 0).toLocaleString(Qt.locale("fr_FR"), "f", 0) }
        Label { text: qsTr("Taille :"); opacity: 0.7; visible: (index.taille || 0) > 0 }
        Label {
            visible: (index.taille || 0) > 0
            text: fenetre.tailleLisible(index.taille || 0)
                  + ((index.messages || 0) < (ligne.messages || 0)
                     ? qsTr(" (les %1 messages connus du poste)").arg(index.messages) : "")
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        Label { text: qsTr("Synchronisé :"); opacity: 0.7 }
        Label { text: dlgProprietes.texteSynchro; wrapMode: Text.Wrap; Layout.fillWidth: true }
    }
}
