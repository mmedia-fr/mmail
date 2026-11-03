// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Supprimer ce dossier » (0.8.5, comme Outlook) : le dossier et
// ses sous-dossiers vont dans la corbeille du compte, messages compris, et
// Ctrl+Z les remet à leur place. Dans la corbeille, le même geste les efface
// pour de bon — cela ne se défait pas, d'où la confirmation.
//
// Les noms employés sans être déclarés ici (fenetre, boite, messageEtat…)
// sont ceux de Main.qml, où « DialogueSupprimerDossier { id: dlgSupprimerDossier } »
// l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import fr.mmedia.mmail

Dialog {
    id: dlg
    objectName: "dialogueSupprimerDossier"
    property int compte: 0
    property string chemin: ""
    property string nom: ""
    property bool definitif: false
    property string motif: ""
    function ouvrir(c, ch, n, d) {
        compte = c; chemin = ch; nom = n; definitif = d; motif = ""
        open()
    }
    /// Rend vrai si c'est lancé ; le motif d'un refus reste dans le dialogue.
    function executer() {
        motif = definitif ? boite.effacerDossier(compte, chemin) : boite.supprimerDossier(compte, chemin)
        if (motif.length > 0)
            return false
        messageEtat.texte = definitif
                ? qsTr("Effacement de « %1 » en cours…").arg(nom)
                : qsTr("« %1 » va dans la corbeille ; Ctrl+Z le remet à sa place.").arg(nom)
        close()
        return true
    }
    title: definitif ? qsTr("Effacer définitivement ce dossier ?") : qsTr("Supprimer ce dossier ?")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(520, fenetre.width - 24)
    footer: DialogButtonBox {
        Button {
            text: dlg.definitif ? qsTr("Effacer") : qsTr("Supprimer")
            // Pas « Accepter » : un refus garde le dialogue ouvert, motif affiché.
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dlg.executer()
        }
        Button {
            text: qsTr("Annuler")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dlg.close()
        }
    }
    RowLayout {
        width: dlg.availableWidth
        spacing: 10
        Avertissement { visible: dlg.definitif }
        ColumnLayout {
            Layout.fillWidth: true
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: dlg.definitif
                      ? qsTr("« %1 », ses sous-dossiers et tous leurs messages seront effacés du serveur. Cela ne se défait pas.").arg(dlg.nom)
                      : qsTr("« %1 » et ses sous-dossiers, messages compris, vont dans la corbeille. Ctrl+Z les remet à leur place.").arg(dlg.nom)
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                visible: dlg.motif.length > 0
                color: "#c62828"
                text: dlg.motif
            }
        }
    }
}
