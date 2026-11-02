// SPDX-License-Identifier: GPL-3.0-or-later
// Bandeau « Se désabonner » (0.6.30, suggestion d'un utilisateur) : le
// message vient d'une liste de diffusion ou d'une lettre d'information qui
// offre le moyen de s'en retirer — en-tête `List-Unsubscribe`, ou lien de
// désinscription repéré dans le corps (cf. `desabonnement` du noyau). Le
// bouton ouvre une confirmation : rien ne part à l'affichage du message.
//
// Les noms employés sans être déclarés ici (fenetre, boite…) sont ceux de
// Main.qml ; le volet de lecture et la fenêtre d'un message en ont chacun un.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import fr.mmedia.mmail

Pane {
    id: bandeau
    objectName: "bandeauDesabonnement"
    // En-tête du message affiché (`entete_affichable` du noyau).
    property var entete: ({})
    // Compte du message.
    property int compte: 0
    // Fenêtre d'un message qui porte ce bandeau ; sans elle, la principale.
    property var pour: null
    property alias dialogue: dlg
    readonly property var d: entete && entete.desabonnement ? entete.desabonnement : null
    visible: d !== null
    // Comme les autres bandeaux, à la taille de l'apparence : le zoom du
    // volet ne grossit que le message.
    font.pointSize: fenetre.tailleBase
    padding: 4
    background: Rectangle { color: "#eef1f5" }
    // Au téléphone, le bouton passe sous le texte.
    GridLayout {
        width: parent.width
        columns: fenetre.tactile ? 1 : 2
        columnSpacing: 6
        Label {
            text: bandeau.d && bandeau.d.source === "corps"
                  ? qsTr("Ce message propose un lien de désinscription.")
                  : qsTr("Ce message vient d'une liste de diffusion ou d'une lettre d'information.")
            color: "#1c1f24"
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        Button {
            objectName: "boutonDesabonner"
            text: qsTr("Se désabonner…")
            onClicked: dlg.ouvrir(bandeau.d, bandeau.compte)
        }
    }
    DialogueDesabonnement {
        id: dlg
        pour: bandeau.pour
    }
}
