// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Certificats S/MIME » d'un compte (0.8.0) : les clés personnelles
// importées (`.p12`), qui déchiffrent les messages reçus et, plus tard,
// signent ceux qui partent ; les autorités de confiance ajoutées à celles du
// système — une autorité interne d'entreprise, en particulier sous Android,
// qui ne connaît que celles de Mozilla.
//
// Le fichier `.p12` est rangé dans le profil, toujours protégé par son mot de
// passe, que le coffre du système garde (`smime:<empreinte>`). Les anciennes
// clés restent : sans elles, les anciens messages chiffrés deviennent
// illisibles.
//
// Les noms employés sans être déclarés ici (fenetre, boite, coffre…) sont
// ceux de Main.qml, où « DialogueCertificats { id: dlgCertificats } »
// l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Dialog {
    id: dlg
    objectName: "dialogueCertificats"
    property int compte: 0
    property string adresse: ""
    property var cles: []
    property string message: ""
    property string fichierChoisi: ""
    function ouvrir(c, a) {
        compte = c
        adresse = a
        message = ""
        fichierChoisi = ""
        motDePasse.text = ""
        open()
    }
    function actualiser() {
        var toutes = []
        try { toutes = JSON.parse(boite.clesSmime()) } catch (e) {}
        cles = toutes.filter(function(k) { return k.compte === dlg.compte })
    }
    // Une clé ouverte ou retirée ailleurs (coffre lu au démarrage).
    Connections {
        target: fenetre
        function onVersionClesSmimeChanged() { if (dlg.visible) dlg.actualiser() }
    }
    onAboutToShow: actualiser()
    title: qsTr("Certificats S/MIME — %1").arg(adresse)
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(620, fenetre.width - 24)
    standardButtons: Dialog.Close

    function jour(secondes) {
        return new Date(secondes * 1000).toLocaleDateString(Qt.locale("fr_FR"), "dd/MM/yyyy")
    }

    function importer() {
        var donnees = lecteur.base64(fichierChoisi)
        if (donnees.length === 0) {
            message = qsTr("Fichier illisible, ou de plus de 1 Mo.")
            return
        }
        var r = JSON.parse(boite.importerCertificat(compte, donnees, motDePasse.text))
        if (!r.ok) {
            message = qsTr("Import impossible : %1.").arg(r.erreur)
            return
        }
        // Le mot de passe au coffre : la clé s'ouvrira d'elle-même aux
        // prochains démarrages.
        coffre.ecrire("smime:" + r.empreinte, motDePasse.text)
        motDePasse.text = ""
        fichierChoisi = ""
        message = (r.avertissement.length > 0 ? r.avertissement + "\n" : "")
                + qsTr("Certificat importé. La sauvegarde de la configuration l'emporte avec son mot de passe pour un compte à mot de passe ; pour un compte Microsoft ou Google, qu'elle ne chiffre pas, gardez le fichier .p12 et son mot de passe en lieu sûr.")
        fenetre.clesSmimeChangees()
        actualiser()
    }

    LecteurFichier { id: lecteur }

    FileDialog {
        id: choixP12
        title: qsTr("Certificat personnel (.p12, .pfx)")
        nameFilters: [qsTr("Certificats personnels (*.p12 *.pfx)"), qsTr("Tous les fichiers (*)")]
        onAccepted: {
            dlg.fichierChoisi = selectedFile.toString()
            dlg.message = ""
            motDePasse.forceActiveFocus()
        }
    }
    FileDialog {
        id: choixAutorite
        title: qsTr("Certificat d'une autorité (.cer, .crt, .pem)")
        nameFilters: [qsTr("Certificats (*.cer *.crt *.pem *.der)"), qsTr("Tous les fichiers (*)")]
        onAccepted: {
            var donnees = lecteur.base64(selectedFile.toString())
            if (donnees.length === 0) {
                dlg.message = qsTr("Fichier illisible, ou de plus de 1 Mo.")
                return
            }
            var r = JSON.parse(boite.ajouterAutorite(donnees))
            dlg.message = r.ok
                    ? qsTr("Autorité « %1 » ajoutée : les signatures qu'elle garantit sont désormais reconnues.").arg(r.titulaire ? r.titulaire.nom : "")
                    : qsTr("Ajout impossible : %1.").arg(r.erreur)
            if (r.ok)
                fenetre.clesSmimeChangees()
        }
    }
    Dialog {
        id: confirmationRetrait
        property string empreinte: ""
        title: qsTr("Retirer ce certificat ?")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(480, fenetre.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        Component.onCompleted: standardButton(Dialog.Ok).text = qsTr("Retirer")
        RowLayout {
            width: confirmationRetrait.availableWidth
            spacing: 10
            Avertissement {}
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Les messages chiffrés pour ce certificat deviendront illisibles dans MMail, sauf à l'importer de nouveau. Gardez-en le fichier .p12 si vous en avez encore besoin.")
            }
        }
        onAccepted: {
            var erreur = boite.retirerCle(empreinte)
            if (erreur.length === 0) {
                coffre.effacer("smime:" + empreinte)
                dlg.message = qsTr("Certificat retiré.")
                fenetre.clesSmimeChangees()
            } else {
                dlg.message = erreur
            }
            dlg.actualiser()
        }
    }

    ColumnLayout {
        width: dlg.availableWidth
        spacing: 6
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            opacity: 0.8
            text: dlg.cles.length === 0
                  ? qsTr("Aucun certificat pour ce compte : les messages chiffrés qu'il reçoit ne peuvent pas être lus. Les signatures des correspondants se vérifient sans certificat.")
                  : qsTr("Le dernier certificat importé sert aux nouveaux messages ; les anciens restent pour lire les messages qui leur ont été chiffrés.")
        }
        Repeater {
            model: dlg.cles
            delegate: RowLayout {
                required property var modelData
                Layout.fillWidth: true
                Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    readonly property var t: modelData.titulaire || ({})
                    text: qsTr("%1 — %2, émis par %3, valide jusqu'au %4%5")
                          .arg(t.nom || "").arg((t.adresses || []).join(", ")).arg(t.emetteur || "")
                          .arg(t.fin ? dlg.jour(t.fin) : "?")
                          .arg(modelData.deverrouillee ? "" : qsTr(" (fermé : mot de passe absent du coffre)"))
                }
                Button {
                    text: qsTr("Retirer…")
                    flat: true
                    onClicked: {
                        confirmationRetrait.empreinte = modelData.empreinte
                        confirmationRetrait.open()
                    }
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Button {
                text: qsTr("Choisir un certificat (.p12)…")
                onClicked: choixP12.open()
            }
            Label {
                Layout.fillWidth: true
                elide: Text.ElideMiddle
                text: dlg.fichierChoisi.length > 0 ? decodeURIComponent(dlg.fichierChoisi.replace(/^.*[\/\\]/, "")) : ""
            }
        }
        RowLayout {
            Layout.fillWidth: true
            visible: dlg.fichierChoisi.length > 0
            TextField {
                id: motDePasse
                Layout.fillWidth: true
                echoMode: TextInput.Password
                placeholderText: qsTr("Mot de passe du fichier")
                onAccepted: dlg.importer()
            }
            Button {
                text: qsTr("Importer")
                enabled: dlg.fichierChoisi.length > 0
                onClicked: dlg.importer()
            }
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: dlg.message.length > 0
            text: dlg.message
        }
        CheckBox {
            text: qsTr("Signer par défaut les messages de ce compte")
            enabled: dlg.cles.length > 0
            checked: fenetre.signeParDefaut(dlg.adresse)
            onToggled: fenetre.poserSigneParDefaut(dlg.adresse, checked)
        }
        MenuSeparator { Layout.fillWidth: true }
        Button {
            text: qsTr("Ajouter une autorité de confiance…")
            onClicked: choixAutorite.open()
            ToolTip.visible: hovered
            ToolTip.delay: 600
            ToolTip.text: qsTr("Pour une autorité de certification interne à une entreprise, que le système ne connaît pas.")
        }
    }
}
