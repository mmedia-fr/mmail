// SPDX-License-Identifier: GPL-3.0-or-later
// « Affichage › Apparence › Créer mon apparence… » (0.6.37) : chacun met
// MMail à ses couleurs. On part d'une apparence existante, on choisit le fond,
// le bandeau, l'accent, la sélection et, depuis la 0.6.32, la couleur des
// liens des messages ; les encres posées dessus (texte du
// bandeau, de la sélection, des repères) sont déduites pour rester lisibles
// (cf. Main.qml, `avecApparencePerso`). L'apparence « Personnelle » rejoint
// la liste ; le réglage suit la sauvegarde de la configuration.
//
// Les noms employés sans être déclarés ici (fenetre, reglagesApparence…) sont
// ceux de Main.qml, où « DialogueApparence { id: dlgApparence } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts

Dialog {
    id: dlgApparence
    title: qsTr("Créer mon apparence")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(520, fenetre.width - 24)
    standardButtons: Dialog.Ok | Dialog.Cancel

    readonly property var bases: ["mmedia", "pomme", "outlook", "moderne", "nuit"]
    property string base: "mmedia"
    // Couleurs choisies ; vide : celle de l'apparence de base.
    property var choix: ({ fond: "", bandeau: "", accent: "", surbrillance: "", lien: "" })
    // Ce qui était là à l'ouverture : « Annuler » le rend.
    property string persoAvant: ""
    property string apparenceAvant: ""
    property string rubriqueEnCours: ""

    readonly property var rubriques: [
        { cle: "fond", nom: qsTr("Fond des fenêtres") },
        { cle: "bandeau", nom: qsTr("Bandeau de l'en-tête") },
        { cle: "accent", nom: qsTr("Accent (filets, pastilles, boutons)") },
        { cle: "surbrillance", nom: qsTr("Sélection") },
        { cle: "lien", nom: qsTr("Liens des messages") }
    ]

    function ouvrir() {
        persoAvant = reglagesApparence.perso
        apparenceAvant = fenetre.apparence
        var p = null
        try { p = persoAvant ? JSON.parse(persoAvant) : null } catch (e) { p = null }
        base = p && bases.indexOf(p.base) >= 0 ? p.base : (bases.indexOf(fenetre.apparence) >= 0 ? fenetre.apparence : "mmedia")
        choix = p ? { fond: p.fond || "", bandeau: p.bandeau || "", accent: p.accent || "",
                      surbrillance: p.surbrillance || "", lien: p.lien || "" }
                  : { fond: "", bandeau: "", accent: "", surbrillance: "", lien: "" }
        open()
    }

    /// Couleur affichée pour une rubrique : la choisie, sinon celle de la base.
    function couleur(cle) {
        if (choix[cle])
            return choix[cle]
        var j = fenetre.jeuxDeBase[base]
        // Liens : ceux de la page blanche des messages (cf. couleurLienPage).
        if (cle === "lien")
            return !j.sombre && j.lien ? j.lien : "#0563c1"
        return j[cle] || (cle === "bandeau" ? "" : j.surbrillance)
    }

    /// Essai en direct : l'apparence « Personnelle » prend les couleurs du
    /// moment.
    function appliquer() {
        var p = { base: base }
        for (var cle in choix)
            if (choix[cle])
                p[cle] = choix[cle]
        reglagesApparence.perso = JSON.stringify(p)
        fenetre.apparence = "perso"
    }

    onAccepted: {
        appliquer()
        fenetre.choisirApparence("perso")
    }
    onRejected: {
        reglagesApparence.perso = persoAvant
        fenetre.apparence = apparenceAvant
    }

    ColorDialog {
        id: choixCouleur
        title: qsTr("Choisir une couleur")
        onAccepted: {
            var c = Object.assign({}, dlgApparence.choix)
            c[dlgApparence.rubriqueEnCours] = selectedColor.toString()
            dlgApparence.choix = c
            dlgApparence.appliquer()
        }
    }

    ColumnLayout {
        width: dlgApparence.availableWidth
        spacing: 8
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Partez d'une apparence, puis choisissez vos couleurs : MMail les essaie aussitôt et règle lui-même le texte posé dessus pour qu'il reste lisible.")
        }
        RowLayout {
            Layout.fillWidth: true
            Label { text: qsTr("À partir de :") }
            ComboBox {
                Layout.fillWidth: true
                model: dlgApparence.bases.map(function(b) { return fenetre.jeuxDeBase[b].libelle })
                currentIndex: dlgApparence.bases.indexOf(dlgApparence.base)
                onActivated: function(i) {
                    dlgApparence.base = dlgApparence.bases[i]
                    dlgApparence.appliquer()
                }
            }
        }
        Repeater {
            model: dlgApparence.rubriques
            RowLayout {
                required property var modelData
                Layout.fillWidth: true
                Rectangle {
                    width: 28
                    height: 20
                    radius: 3
                    color: dlgApparence.couleur(modelData.cle) || "transparent"
                    border.color: fenetre.palette.mid
                }
                Label {
                    Layout.fillWidth: true
                    text: modelData.nom + (dlgApparence.couleur(modelData.cle) ? "" : qsTr(" (aucun)"))
                }
                Button {
                    text: qsTr("Choisir…")
                    onClicked: {
                        dlgApparence.rubriqueEnCours = modelData.cle
                        choixCouleur.selectedColor = dlgApparence.couleur(modelData.cle) || "#21abe3"
                        choixCouleur.open()
                    }
                }
                Button {
                    text: qsTr("Par défaut")
                    enabled: dlgApparence.choix[modelData.cle] !== ""
                    onClicked: {
                        var c = Object.assign({}, dlgApparence.choix)
                        c[modelData.cle] = ""
                        dlgApparence.choix = c
                        dlgApparence.appliquer()
                    }
                }
            }
        }
    }
}
