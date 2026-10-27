// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Expéditeurs de confiance » (0.6.24, demande de Guillaume LUDGER
// du 26/10, ordre de Manu du 27/10) : les adresses dont les images distantes
// se téléchargent d'office, posées par « Toujours pour cet expéditeur » ;
// « Retirer » rend l'expéditeur au téléchargement à la demande.
//
// Les noms employés sans être déclarés ici (fenetre, boite, reglages…) sont
// ceux de Main.qml, où « DialogueExpediteursConfiance { id:
// dlgExpediteursConfiance } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Dialog {
    id: dlgExpediteursConfiance
    property var liste: []
    function actualiser() {
        liste = fenetre.expediteursConfiance()
    }
    onAboutToShow: actualiser()
    title: qsTr("Expéditeurs de confiance")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(560, fenetre.width - 24)
    standardButtons: Dialog.Close
    ColumnLayout {
        width: dlgExpediteursConfiance.availableWidth
        spacing: 4
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.8
            text: dlgExpediteursConfiance.liste.length === 0
                  ? qsTr("Aucun : les images distantes ne se téléchargent qu'à la demande. Pour un expéditeur : « Toujours pour cet expéditeur », dans le bandeau des images de l'un de ses messages.")
                  : qsTr("Les images distantes de ces expéditeurs se téléchargent d'office, sauf quand le serveur n'a pas pu authentifier l'expéditeur (DMARC).")
        }
        ListView {
            id: vueExpediteurs
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, fenetre.height * 0.5)
            clip: true
            model: dlgExpediteursConfiance.liste
            ScrollBar.vertical: ScrollBar {
                policy: vueExpediteurs.contentHeight > vueExpediteurs.height ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
            }
            delegate: RowLayout {
                required property var modelData
                width: vueExpediteurs.width - 12
                Label {
                    text: modelData
                    elide: Text.ElideMiddle
                    Layout.fillWidth: true
                }
                Button {
                    text: qsTr("Retirer")
                    flat: true
                    onClicked: {
                        fenetre.retenirExpediteurConfiance(modelData, false)
                        dlgExpediteursConfiance.actualiser()
                    }
                }
            }
        }
    }
}
