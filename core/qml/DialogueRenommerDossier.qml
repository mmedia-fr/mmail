// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Renommer le dossier ».
//
// Les noms employés sans être déclarés ici (fenetre, boite, messageEtat…)
// sont ceux de Main.qml, où « DialogueRenommerDossier { id: dlgRenommerDossier } »
// l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// Dossier renommé sur le serveur, ses sous-dossiers avec lui, puis montré
// sous son nouveau nom (signal `dossierRenomme`). Un refus immédiat — nom
// vide, interdit ou déjà pris, déplacement en cours, compte hors ligne —
// reste dans le dialogue.
Dialog {
    id: dlgRenommerDossier
    // Désignés hors de ce fichier (Main.qml, fichiers voisins) : dlgRenommerDossier.<nom>.
    property alias champNom: champNom
    property int compte: 0
    property string chemin: ""
    property string nomActuel: ""
    property string motif: ""
    function ouvrir(c, ch, n) {
        compte = c; chemin = ch; nomActuel = n; motif = ""
        champNom.text = n
        open()
        champNom.forceActiveFocus()
        champNom.selectAll()
    }
    function renommer() {
        motif = boite.renommerDossier(compte, chemin, champNom.text)
        if (motif.length > 0)
            return
        messageEtat.texte = qsTr("Renommage de « %1 » en « %2 » en cours…").arg(nomActuel).arg(champNom.text.trim())
        close()
    }
    title: qsTr("Renommer le dossier")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(420, fenetre.width - 24)
    closePolicy: Popup.CloseOnEscape
    // « Renommer » ne ferme pas d'office : un refus laisse corriger le nom.
    footer: DialogButtonBox {
        Button {
            text: qsTr("Renommer")
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dlgRenommerDossier.renommer()
        }
        Button {
            text: qsTr("Annuler")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }
    ColumnLayout {
        width: dlgRenommerDossier.availableWidth
        spacing: 6
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Nouveau nom de « %1 » — ses sous-dossiers le suivent :").arg(dlgRenommerDossier.nomActuel)
        }
        TextField {
            id: champNom
            MenuSaisie {}
            Layout.fillWidth: true
            placeholderText: qsTr("Nom du dossier")
            onTextEdited: dlgRenommerDossier.motif = ""
            Keys.onReturnPressed: dlgRenommerDossier.renommer()
            Keys.onEnterPressed: dlgRenommerDossier.renommer()
        }
        Label {
            Layout.fillWidth: true
            visible: dlgRenommerDossier.motif.length > 0
            wrapMode: Text.Wrap
            color: "#b00020"
            text: dlgRenommerDossier.motif
        }
    }
}
