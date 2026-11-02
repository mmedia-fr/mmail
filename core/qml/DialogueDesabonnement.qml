// SPDX-License-Identifier: GPL-3.0-or-later
// Confirmation « Se désabonner » (0.6.30) : rien ne part sans elle. Elle dit
// ce que MMail va faire, et vers quel serveur ou quelle adresse.
//
// Les noms employés sans être déclarés ici (fenetre, boite, messageEtat…)
// sont ceux de Main.qml ; chaque bandeau de désabonnement (volet de lecture,
// fenêtre d'un message) a le sien.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import fr.mmedia.mmail

Dialog {
    id: dlgDesabonnement
    // Fenêtre d'un message qui ouvre ce dialogue ; sans elle, la principale.
    property var pour: null
    // Compte du message : celui qui envoie le courriel de désabonnement.
    property int compte: 0
    // Le moyen de se désabonner, tel que le noyau l'a trouvé
    // (`desabonnement::Desabonnement`).
    property var d: ({})
    function ouvrir(desabonnement, c) {
        d = desabonnement || ({})
        compte = c
        open()
    }
    function explication() {
        if (d.methode === "un-clic")
            return qsTr("MMail va demander à %1 de retirer votre adresse de cette liste. La demande part seule, sans "
                        + "rien d'autre de vous, et aucune page ne s'ouvre.").arg(d.hote)
        if (d.methode === "courriel")
            return qsTr("Cette liste se quitte par un courriel à %1. MMail va le préparer : il ne part que lorsque "
                        + "vous cliquez sur « Envoyer ».").arg(d.hote)
        if (d.source === "corps")
            return qsTr("Le message propose un lien de désinscription vers %1. MMail va l'ouvrir dans votre "
                        + "navigateur : vérifiez-y que la page est bien celle de l'expéditeur.").arg(d.hote)
        return qsTr("Cette liste se quitte depuis une page de %1. MMail va l'ouvrir dans votre navigateur ; "
                    + "suivez-y les indications.").arg(d.hote)
    }
    /// Le désabonnement confirmé. Rend la rédaction ouverte pour un courriel
    /// (les scénarios d'essai la relisent), vrai ou faux sinon.
    function executer() {
        if (d.methode === "un-clic") {
            if (!boite.desabonner(d.url)) {
                messageEtat.texte = boite.erreur
                return false
            }
            messageEtat.texte = qsTr("Demande de désabonnement envoyée à %1…").arg(d.hote)
            return true
        }
        if (d.methode === "courriel") {
            var r = fenetre.ouvrirRedaction(compte)
            r.remplir({ a: d.adresse, objet: (d.objet || "").length > 0 ? d.objet : "unsubscribe",
                        texte: d.texte || "", mode: "nouveau" })
            messageEtat.texte = qsTr("Message de désabonnement préparé : relisez-le, puis envoyez-le.")
            return r
        }
        if (d.methode === "lien") {
            Qt.openUrlExternally(d.url)
            messageEtat.texte = qsTr("Page de désabonnement de %1 ouverte dans le navigateur.").arg(d.hote)
            return true
        }
        return false
    }
    title: qsTr("Se désabonner ?")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(480, (pour ? pour.width : fenetre.width) - 24)
    standardButtons: Dialog.Yes | Dialog.No
    ColumnLayout {
        width: dlgDesabonnement.availableWidth
        spacing: 8
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: dlgDesabonnement.explication()
        }
        // L'adresse entière, pour qui veut la vérifier.
        Label {
            Layout.fillWidth: true
            visible: dlgDesabonnement.d.methode !== "courriel"
            text: dlgDesabonnement.d.url || ""
            elide: Text.ElideMiddle
            opacity: 0.7
            font.pointSize: fenetre.tailleBase * 0.9
        }
    }
    onAccepted: executer()
}
