// SPDX-License-Identifier: GPL-3.0-or-later
// « Vider partiellement… » (0.6.24) : effacer pour de bon, dans la corbeille
// ou les indésirables d'un compte, les messages reçus avant une date, ou dont
// l'expéditeur ou l'objet remplissent des conditions — celles des filtres.
// Le serveur compte d'abord ; seuls les messages comptés partent, sur
// confirmation.
//
// Les noms employés sans être déclarés ici (fenetre, boite, messageEtat…)
// sont ceux de Main.qml, où « DialogueViderPartiellement { id:
// dlgViderPartiellement } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import fr.mmedia.mmail

Dialog {
    id: dlgViderPartiellement
    property int compte: 0
    property string chemin: ""
    property string nom: ""
    property bool avecDate: true
    property string avant: ""
    property bool tout: true
    property var conditions: []
    // Décompte : -2 à faire, -1 en cours, sinon le nombre rendu par le serveur.
    property int nombre: -2
    property string motif: ""

    readonly property var champs: ["expediteur", "objet"]
    readonly property var nomsChamps: [qsTr("Expéditeur"), qsTr("Objet")]
    readonly property var operateurs: ["contient", "est", "commence", "finit"]
    readonly property var nomsOperateurs: [qsTr("contient"), qsTr("est"), qsTr("commence par"), qsTr("se termine par")]

    function ouvrir(c, ch, n) {
        compte = c; chemin = ch; nom = n
        avecDate = true
        avant = "01/01/" + new Date().getFullYear()
        tout = true
        conditions = []
        nombre = -2
        motif = ""
        open()
    }
    // Les conditions telles qu'à l'écran : leur texte se lit dans les champs,
    // la liste ne se recrée pas à chaque frappe (le champ perdrait la main).
    function relever() {
        var l = []
        for (var i = 0; i < conditions.length; ++i) {
            var c = Object.assign({}, conditions[i])
            var ligne = repConditions.itemAt(i)
            if (ligne)
                c.valeur = ligne.valeur
            l.push(c)
        }
        return l
    }
    function criteres() {
        return { avant: avecDate ? avant : "", tout: tout,
                 conditions: relever().filter(function(c) { return c.valeur.trim().length > 0 }) }
    }
    // Un critère change : le décompte affiché ne vaut plus.
    function changer() {
        nombre = -2
        motif = ""
    }
    function modifierCondition(i, cle, valeur) {
        var l = relever()
        var c = Object.assign({}, l[i])
        c[cle] = valeur
        l[i] = c
        conditions = l
        changer()
    }
    function compter() {
        var refus = boite.preparerPurge(compte, chemin, JSON.stringify(criteres()))
        if (refus.length > 0) {
            nombre = -2
            motif = refus
            return
        }
        motif = ""
        nombre = -1
    }
    // Réponse du serveur (ReactionsNoyau, purgePreparee) ; celle d'un
    // décompte que l'utilisateur a rendu caduc est ignorée.
    function decompte(c, ch, n, erreur) {
        if (!visible || c !== compte || ch !== chemin || nombre !== -1)
            return
        nombre = n
        motif = erreur
        if (fenetre.essai && fenetre.essai.scenario === "vider-partiellement")
            scenarios.decompteScenarioViderPartiellement(n, erreur)
    }

    title: qsTr("Vider partiellement « %1 »").arg(nom)
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(620, fenetre.width - 16)
    padding: 12
    // Un clic à côté ne fait pas perdre les critères.
    closePolicy: Popup.CloseOnEscape

    ColumnLayout {
        width: dlgViderPartiellement.availableWidth
        spacing: 8
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            text: qsTr("Effacer définitivement de « %1 » les messages :").arg(dlgViderPartiellement.nom)
        }
        RowLayout {
            Layout.fillWidth: true
            CheckBox {
                text: qsTr("reçus avant le")
                checked: dlgViderPartiellement.avecDate
                onToggled: { dlgViderPartiellement.avecDate = checked; dlgViderPartiellement.changer() }
            }
            TextField {
                MenuSaisie {}
                Layout.preferredWidth: 130
                enabled: dlgViderPartiellement.avecDate
                placeholderText: qsTr("JJ/MM/AAAA")
                text: dlgViderPartiellement.avant
                onTextEdited: { dlgViderPartiellement.avant = text; dlgViderPartiellement.changer() }
                Accessible.name: qsTr("Date de réception")
            }
            Item { Layout.fillWidth: true }
        }
        RowLayout {
            Layout.fillWidth: true
            visible: dlgViderPartiellement.conditions.length > 1
            Label { text: qsTr("et qui remplissent") }
            ComboBox {
                Layout.fillWidth: true
                model: [qsTr("toutes les conditions"), qsTr("une condition au moins")]
                currentIndex: dlgViderPartiellement.tout ? 0 : 1
                onActivated: function(i) { dlgViderPartiellement.tout = i === 0; dlgViderPartiellement.changer() }
            }
        }
        Repeater {
            id: repConditions
            model: dlgViderPartiellement.conditions
            delegate: GridLayout {
                required property var modelData
                required property int index
                property alias valeur: champValeur.text
                Layout.fillWidth: true
                columns: fenetre.compact ? 2 : 4
                ComboBox {
                    model: dlgViderPartiellement.nomsChamps
                    currentIndex: Math.max(0, dlgViderPartiellement.champs.indexOf(modelData.champ))
                    onActivated: function(i) { dlgViderPartiellement.modifierCondition(index, "champ", dlgViderPartiellement.champs[i]) }
                    Layout.preferredWidth: 140
                }
                ComboBox {
                    model: dlgViderPartiellement.nomsOperateurs
                    currentIndex: Math.max(0, dlgViderPartiellement.operateurs.indexOf(modelData.operateur))
                    onActivated: function(i) { dlgViderPartiellement.modifierCondition(index, "operateur", dlgViderPartiellement.operateurs[i]) }
                    Layout.preferredWidth: 160
                    Layout.fillWidth: fenetre.compact
                }
                TextField {
                    id: champValeur
                    MenuSaisie {}
                    Layout.fillWidth: true
                    placeholderText: qsTr("texte")
                    text: modelData.valeur
                    onTextEdited: dlgViderPartiellement.changer()
                }
                Button {
                    text: qsTr("Retirer")
                    onClicked: {
                        var l = dlgViderPartiellement.relever()
                        l.splice(index, 1)
                        dlgViderPartiellement.conditions = l
                        dlgViderPartiellement.changer()
                    }
                }
            }
        }
        Button {
            text: qsTr("Ajouter une condition (expéditeur, objet)")
            onClicked: {
                dlgViderPartiellement.conditions = dlgViderPartiellement.relever().concat(
                            [{ champ: "expediteur", operateur: "contient", valeur: "" }])
                dlgViderPartiellement.changer()
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 4
            spacing: 12
            Avertissement { visible: dlgViderPartiellement.nombre > 0 }
            Label {
                id: etatDecompte
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: dlgViderPartiellement.motif.length > 0 ? dlgViderPartiellement.motif
                      : dlgViderPartiellement.nombre === -1 ? qsTr("Décompte sur le serveur…")
                      : dlgViderPartiellement.nombre === 0 ? qsTr("Aucun message ne répond à ces critères.")
                      : dlgViderPartiellement.nombre > 0
                        ? qsTr("%1 répondent à ces critères. Ils seront retirés du serveur, sans repasser par une corbeille : l'action est irréversible.").arg(fenetre.accord(dlgViderPartiellement.nombre, qsTr("message"), qsTr("messages")))
                      : qsTr("« Compter » demande au serveur combien de messages répondent à ces critères ; rien n'est effacé avant votre confirmation.")
            }
        }
    }

    footer: DialogButtonBox {
        Button {
            text: qsTr("Compter")
            enabled: dlgViderPartiellement.nombre !== -1
            DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
            onClicked: dlgViderPartiellement.compter()
        }
        Button {
            text: dlgViderPartiellement.nombre > 0
                  ? qsTr("Effacer %1").arg(fenetre.accord(dlgViderPartiellement.nombre, qsTr("message"), qsTr("messages")))
                  : qsTr("Effacer")
            enabled: dlgViderPartiellement.nombre > 0
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
        }
        Button {
            text: qsTr("Fermer")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }
    onAccepted: {
        if (!boite.executerPurge(compte, chemin)) {
            messageEtat.texte = boite.erreur
            return
        }
        // Compté comme un déplacement : `messagesEffaces` le défait.
        fenetre.deplacementsEnVol += 1
        messageEtat.texte = qsTr("Effacement de %1 dans « %2 »…")
            .arg(fenetre.accord(nombre, qsTr("message"), qsTr("messages"))).arg(nom)
    }
}
