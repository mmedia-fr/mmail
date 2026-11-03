// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Gérer les catégories » (0.6.15) : la liste des catégories de la
// boîte — celles d'origine du webmail et celles créées —, à renommer, à
// recolorer, à retirer ; et d'où elle vient (serveur ou poste).
//
// Les noms employés sans être déclarés ici (fenetre, boite, messageEtat…)
// sont ceux de Main.qml, où « DialogueCategories { id: dlgCategories } »
// l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlgCategories
    property int compte: 0
    property string chemin: ""
    // Cf. Boite.categories.
    property var etat: ({ liste: [], serveur: false, boite: "", creees: 0, max: 15 })
    // Catégorie dont le retrait attend sa confirmation (second clic).
    property string aRetirer: ""

    function ouvrir(c, ch) {
        compte = c
        chemin = ch
        aRetirer = ""
        relire()
        open()
    }
    function relire() {
        etat = JSON.parse(boite.categories(compte, chemin))
    }
    // Le catalogue a changé — ici, ou relu du serveur.
    Connections {
        target: boite
        function onRevisionCategoriesChanged() {
            if (dlgCategories.visible)
                dlgCategories.relire()
        }
    }

    // Couleurs d'Outlook (Microsoft 365, 0.9.0) : l'autorisation de lire les
    // réglages de la boîte se donne dans le navigateur ; la suite revient par
    // `couleursOutlook` (ReactionsNoyau.qml).
    function reprendreCouleurs() {
        var url = boite.autoriserCouleursOutlook(compte)
        if (url.length === 0) {
            messageEtat.texte = qsTr("Couleurs d'Outlook : %1").arg(boite.erreur)
            return
        }
        Qt.openUrlExternally(url)
        messageEtat.texte = qsTr("Autorisation dans votre navigateur : MMail attend son retour…")
    }

    function retirer(c) {
        if (aRetirer !== c.motCle) {
            aRetirer = c.motCle
            return
        }
        aRetirer = ""
        if (!boite.retirerCategorie(compte, chemin, c.motCle)) {
            messageEtat.texte = boite.erreur
            return
        }
        messageEtat.texte = c.origine ? qsTr("Catégorie « %1 » rétablie.").arg(c.nom)
                                      : qsTr("Catégorie « %1 » retirée de la liste.").arg(c.nom)
    }

    title: qsTr("Catégories")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(520, fenetre.width - 24)
    height: Math.min(implicitHeight, fenetre.height - 24)
    closePolicy: Popup.CloseOnEscape
    footer: DialogButtonBox {
        Button {
            text: qsTr("Nouvelle catégorie…")
            enabled: dlgCategories.etat.creees < dlgCategories.etat.max
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dlgCategorie.ouvrir(dlgCategories.compte, dlgCategories.chemin, "", "",
                                           fenetre.couleurLibre(dlgCategories.etat.liste), false)
        }
        Button {
            text: qsTr("Fermer")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }
    ColumnLayout {
        width: dlgCategories.availableWidth
        spacing: 6
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.85
            text: dlgCategories.etat.microsoft === true
                  ? qsTr("Compte Microsoft 365 : la liste des catégories reste sur ce poste. Celles posées sur les messages sont dans Outlook, et s'y voient.")
                  : dlgCategories.etat.serveur
                  ? (dlgCategories.etat.boite === "INBOX"
                     ? qsTr("Catégories de cette boîte, gardées sur le serveur : tous les postes où elle est ouverte dans MMail les voient.")
                     : qsTr("Catégories de la boîte partagée, gardées sur le serveur : tous ceux qui l'ouvrent dans MMail voient les mêmes."))
                  : qsTr("Ce serveur ne garde pas la liste des catégories : elle reste sur ce poste. Les catégories posées sur les messages, elles, sont sur le serveur.")
        }
        RowLayout {
            Layout.fillWidth: true
            visible: dlgCategories.etat.microsoft === true && Qt.platform.os !== "android"
            spacing: 8
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.85
                text: qsTr("Les couleurs viennent d'Outlook une fois MMail autorisé à lire les réglages de la boîte.")
            }
            Button {
                text: qsTr("Couleurs d'Outlook…")
                onClicked: dlgCategories.reprendreCouleurs()
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: qsTr("Microsoft vous demandera, dans votre navigateur, d'autoriser MMail à lire les réglages de votre boîte, dont la liste des catégories d'Outlook. Sans cette autorisation, les catégories venues d'Outlook restent grises.")
            }
        }
        ListView {
            id: vueCategories
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, 360)
            clip: true
            model: dlgCategories.etat.liste
            ScrollBar.vertical: ScrollBar {}
            delegate: RowLayout {
                required property var modelData
                width: vueCategories.width
                spacing: 8
                PastilleCategorie {
                    taille: 14
                    nom: modelData.nom
                    couleur: modelData.couleur
                    Layout.alignment: Qt.AlignVCenter
                }
                Label {
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    text: modelData.origine ? qsTr("%1 (webmail)").arg(modelData.nom) : modelData.nom
                }
                Button {
                    text: qsTr("Modifier…")
                    flat: true
                    onClicked: dlgCategorie.ouvrir(dlgCategories.compte, dlgCategories.chemin, modelData.motCle,
                                                   modelData.nom, modelData.couleur, false)
                }
                // Retirer une catégorie d'origine lui rend son nom et sa couleur.
                Button {
                    text: dlgCategories.aRetirer === modelData.motCle
                          ? qsTr("Confirmer")
                          : (modelData.origine ? qsTr("Rétablir") : qsTr("Retirer"))
                    flat: dlgCategories.aRetirer !== modelData.motCle
                    onClicked: dlgCategories.retirer(modelData)
                    ToolTip.visible: hovered
                    ToolTip.delay: 500
                    ToolTip.text: modelData.origine
                                  ? qsTr("Reprendre le nom et la couleur du webmail.")
                                  : qsTr("Retirer de la liste, pour tous ceux qui ouvrent cette boîte. Les messages qui la portent la gardent, en gris.")
                }
            }
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.7
            text: qsTr("%1 catégorie(s) créée(s) sur %2 possibles, en plus des cinq du webmail.")
                  .arg(dlgCategories.etat.creees).arg(dlgCategories.etat.max)
        }
    }
}
