// SPDX-License-Identifier: GPL-3.0-or-later
// Bandeau S/MIME (0.8.0) : le message affiché est signé ou chiffré (cf.
// `smime::Securite` du noyau). Vert : signature vérifiée ; orange : signée,
// mais l'identité n'est pas garantie (autorité inconnue, adresse différente,
// certificat expiré, SHA-1) ; rouge : message modifié depuis sa signature, ou
// chiffré et illisible. La révocation n'est pas interrogée : elle est dite
// « non vérifiée ».
//
// Les noms employés sans être déclarés ici (fenetre…) sont ceux de Main.qml.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: bandeau
    objectName: "bandeauSecurite"
    // `Securite` du noyau, en objet : {chiffre, illisible, ancien, signature}.
    property var securite: ({})
    readonly property var s: securite && securite.signature ? securite.signature : null
    readonly property bool chiffre: !!(securite && securite.chiffre)
    readonly property bool illisible: chiffre && (securite.illisible || "").length > 0
    readonly property string etat: s ? s.etat : ""
    readonly property bool bonne: etat === "verifiee"
    readonly property bool grave: illisible || etat === "modifiee" || etat === "invalide"
    property bool details: false
    visible: chiffre || s !== null
    font.pointSize: fenetre.tailleBase
    padding: 6
    background: Rectangle {
        readonly property color teinte: bandeau.grave ? "#c62828" : (bandeau.bonne || (!bandeau.s && !bandeau.illisible)) ? "#2e7d32" : "#ef6c00"
        color: Qt.tint(fenetre.palette.base, Qt.rgba(teinte.r, teinte.g, teinte.b, 0.12))
        border.color: teinte
        border.width: 1
        radius: 3
    }
    onSecuriteChanged: details = false
    // Secondes Unix → « 03/11/2026 ».
    function jour(secondes) {
        return new Date(secondes * 1000).toLocaleDateString(Qt.locale("fr_FR"), "dd/MM/yyyy")
    }

    ColumnLayout {
        width: parent.width
        spacing: 2
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: bandeau.chiffre
            font.bold: bandeau.illisible
            text: bandeau.illisible
                  ? qsTr("Message chiffré, illisible : %1.").arg(bandeau.securite.illisible)
                  : bandeau.securite.ancien
                    ? qsTr("Message chiffré (algorithme ancien, 3DES).")
                    : qsTr("Message chiffré.")
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: bandeau.s !== null
            font.bold: bandeau.grave
            text: bandeau.s ? bandeau.s.texte : ""
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            visible: bandeau.s !== null && bandeau.details
            text: {
                var t = bandeau.s && bandeau.s.titulaire ? bandeau.s.titulaire : null
                if (!t)
                    return ""
                var lignes = [qsTr("Titulaire : %1").arg(t.nom),
                              qsTr("Adresse : %1").arg((t.adresses || []).join(", ")),
                              qsTr("Émis par : %1").arg(t.emetteur),
                              qsTr("Valide du %1 au %2").arg(bandeau.jour(t.debut)).arg(bandeau.jour(t.fin))]
                if (bandeau.s.signeLe > 0)
                    lignes.push(qsTr("Signé le %1").arg(bandeau.jour(bandeau.s.signeLe)))
                lignes.push(qsTr("Empreinte SHA-256 : %1").arg(t.empreinte.toUpperCase().replace(/(..)(?!$)/g, "$1:")))
                lignes.push(qsTr("Révocation non vérifiée : l'interroger dirait à l'autorité que le message est lu."))
                return lignes.join("\n")
            }
        }
        Label {
            visible: bandeau.s !== null && !!bandeau.s.titulaire
            text: bandeau.details ? qsTr("<a href=\"#\">Masquer les détails</a>") : qsTr("<a href=\"#\">Détails du certificat</a>")
            textFormat: Text.RichText
            onLinkActivated: bandeau.details = !bandeau.details
            HoverHandler { cursorShape: Qt.PointingHandCursor }
        }
    }
}
