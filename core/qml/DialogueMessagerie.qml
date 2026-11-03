// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Messagerie par défaut » (Réglages, Windows) : la messagerie dans
// laquelle les autres logiciels créent un courriel — « Envoyer vers ›
// Destinataire » de l'Explorateur, l'envoi d'un document ou d'un état « par
// courriel » d'un logiciel de gestion (MAPI) —, au choix de la session,
// comme Outlook ou Thunderbird le proposent. Les liens « mailto: » relèvent
// des réglages de Windows, que le dialogue ouvre (cf. cpp/messagerie.h).
//
// Les noms employés sans être déclarés ici (fenetre…) sont ceux de Main.qml,
// où « DialogueMessagerie { id: dlgMessagerie } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import fr.mmedia.mmail.natif

Dialog {
    id: dlgMessagerie
    property var clients: []
    property string actuelle: ""
    property bool mmailInscrit: false
    property string note: ""

    Messagerie { id: messagerie }

    function actualiser() {
        clients = messagerie.clients()
        actuelle = messagerie.actuelle()
        mmailInscrit = messagerie.mmailInscrit()
        var rang = -1
        for (var i = 0; i < clients.length; ++i) {
            if (clients[i].nom.toLowerCase() === actuelle.toLowerCase())
                rang = i
        }
        choix.currentIndex = rang
    }
    onAboutToShow: {
        note = ""
        actualiser()
    }

    title: qsTr("Messagerie par défaut")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(560, fenetre.width - 24)
    standardButtons: Dialog.Close

    ColumnLayout {
        width: dlgMessagerie.availableWidth
        spacing: 8
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Messagerie dans laquelle les autres logiciels créent un courriel : « Envoyer vers › Destinataire » de l'Explorateur, envoi d'un document ou d'un état par courriel. Ce choix vaut pour votre session.")
        }
        RowLayout {
            Layout.fillWidth: true
            ComboBox {
                id: choix
                Layout.fillWidth: true
                model: dlgMessagerie.clients
                textRole: "libelle"
                displayText: currentIndex >= 0 ? currentText
                             : (dlgMessagerie.actuelle.length > 0 ? dlgMessagerie.actuelle : qsTr("Aucune"))
            }
            Button {
                text: qsTr("Choisir")
                enabled: choix.currentIndex >= 0
                         && dlgMessagerie.clients[choix.currentIndex].nom.toLowerCase() !== dlgMessagerie.actuelle.toLowerCase()
                onClicked: {
                    var nom = dlgMessagerie.clients[choix.currentIndex].nom
                    var ok = messagerie.choisir(nom)
                    dlgMessagerie.actualiser()
                    dlgMessagerie.note = ok ? qsTr("Les logiciels ouverts après ce choix créeront leurs courriels dans %1.")
                                              .arg(dlgMessagerie.clients[choix.currentIndex].libelle)
                                            : qsTr("Le choix n'a pas pu être enregistré.")
                }
            }
        }
        Label {
            Layout.fillWidth: true
            visible: !dlgMessagerie.mmailInscrit
            wrapMode: Text.Wrap
            color: "#b00020"
            text: qsTr("MMail n'est pas déclaré comme messagerie sur cet ordinateur : il doit être installé pour tous les utilisateurs, avec les droits d'administrateur.")
        }
        Label {
            Layout.fillWidth: true
            visible: dlgMessagerie.note.length > 0
            wrapMode: Text.Wrap
            opacity: 0.8
            text: dlgMessagerie.note
        }
        MenuSeparator { Layout.fillWidth: true }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.8
            text: qsTr("Les adresses de courriel cliquées dans un navigateur ou un document (liens « mailto: ») se règlent dans les paramètres de Windows, « Applications par défaut ».")
        }
        Button {
            text: qsTr("Ouvrir les applications par défaut de Windows")
            onClicked: Qt.openUrlExternally("ms-settings:defaultapps?registeredAppUser=MMail")
        }
    }
}
