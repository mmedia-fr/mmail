// SPDX-License-Identifier: GPL-3.0-or-later
// Dialogue « Filtres du courrier » (0.6.0 ; décision 11, tranchée par le mainteneur le
// 18/10/2026) : des règles appliquées par MMail lui-même aux messages qui
// arrivent dans la boîte de réception — sur tous les comptes —, ou à toute la
// boîte sur demande. La liste part avec la sauvegarde de la configuration.
//
// Les noms employés sans être déclarés ici (fenetre, boite…) sont ceux de
// Main.qml, où « DialogueFiltres { id: dlgFiltres } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Dialog {
    id: dlgFiltres
    title: qsTr("Filtres du courrier")
    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(760, fenetre.width - 16)
    height: Math.min(680, fenetre.height - 24)
    padding: 10

    property var liste: []
    property int choisi: -1
    property var cibles: []
    property string message: ""
    // Règles d'Outlook (0.9.0) : les comptes Microsoft 365 du profil, et ceux
    // dont un filtre attend l'autorisation des réglages de la boîte.
    property var adressesMicrosoft: []
    property var aAutoriser: []
    readonly property var filtre: choisi >= 0 && choisi < liste.length ? liste[choisi] : null
    readonly property var adresses: fenetre.comptesConnus.map(function(c) { return c.adresse })

    // À, Cc et « À ou Cc » : 0.6.33.
    readonly property var champs: ["expediteur", "objet", "a", "cc", "destinataires"]
    readonly property var nomsChamps: [qsTr("Expéditeur"), qsTr("Objet"), qsTr("À"), qsTr("Cc"), qsTr("À ou Cc")]
    readonly property var operateurs: ["contient", "est", "commence", "finit"]
    readonly property var nomsOperateurs: [qsTr("contient"), qsTr("est"), qsTr("commence par"), qsTr("se termine par")]
    readonly property var destinations: ["", "dossier", "corbeille", "indesirables"]
    readonly property var nomsDestinations: [qsTr("le laisser où il est"), qsTr("le déplacer vers un dossier"),
                                             qsTr("le mettre à la corbeille"), qsTr("le classer indésirable")]

    /// Ouvre la liste ; `nouveau` (facultatif) : un filtre à ajouter, déjà
    /// rempli — « Créer un filtre… » sur un message.
    function ouvrir(nouveau) {
        cibles = JSON.parse(boite.dossiersCibles())
        adressesMicrosoft = fenetre.listeComptes()
            .filter(function(c) { return c.auth === "microsoft" })
            .map(function(c) { return c.adresse })
        aAutoriser = []
        var l = []
        try { l = JSON.parse(boite.filtres() || "[]") } catch (e) { l = [] }
        if (nouveau)
            l.push(nouveau)
        liste = l
        choisi = nouveau ? l.length - 1 : (l.length > 0 ? 0 : -1)
        message = ""
        open()
    }
    function vierge() {
        return { nom: qsTr("Nouveau filtre"), actif: true, compte: "", tout: true,
                 conditions: [{ champ: "expediteur", operateur: "contient", valeur: "" }],
                 lu: false, suivi: false, destination: "", dossier: "", compte_destination: "", arreter: false,
                 serveur: false }
    }
    /// Le filtre peut partir dans les règles d'Outlook : il vise un compte
    /// Microsoft 365, ou tous les comptes quand le profil en a un.
    function versOutlook(f) {
        if (!f)
            return false
        return f.compte ? adressesMicrosoft.indexOf(f.compte) >= 0 : adressesMicrosoft.length > 0
    }
    /// Ce qu'Outlook ne sait pas faire de ce filtre ; vide s'il le peut.
    function raisonOutlook(f) {
        return f && f.serveur === true ? boite.regleOutlook(JSON.stringify(f)) : ""
    }
    /// Issue de l'accord des règles d'Outlook (ReactionsNoyau.qml).
    function reglesRecues(compte, texte, autoriser) {
        if (texte.length > 0)
            message = texte
        if (autoriser && aAutoriser.indexOf(compte) < 0)
            aAutoriser = aAutoriser.concat([compte])
    }
    /// La même autorisation que « Relier à Outlook… » (catégories) : lire et
    /// modifier les réglages de la boîte. Au retour, les règles partent.
    function autoriserOutlook() {
        var compte = aAutoriser[0]
        aAutoriser = aAutoriser.slice(1)
        var url = boite.autoriserCouleursOutlook(compte)
        if (url.length === 0) {
            message = qsTr("Autorisation impossible : %1").arg(boite.erreur)
            return
        }
        Qt.openUrlExternally(url)
        message = qsTr("Autorisation dans votre navigateur : MMail attend son retour…")
    }
    function modifier(cle, valeur) {
        if (!filtre)
            return
        var l = liste.slice()
        var f = Object.assign({}, l[choisi])
        f[cle] = valeur
        l[choisi] = f
        liste = l
    }
    function modifierCondition(i, cle, valeur) {
        var conditions = filtre.conditions.slice()
        var c = Object.assign({}, conditions[i])
        c[cle] = valeur
        conditions[i] = c
        modifier("conditions", conditions)
    }
    function deplacer(sens) {
        var j = choisi + sens
        if (choisi < 0 || j < 0 || j >= liste.length)
            return
        var l = liste.slice()
        var f = l[choisi]
        l[choisi] = l[j]
        l[j] = f
        liste = l
        choisi = j
    }
    function resume(f) {
        var actions = []
        if (f.lu)
            actions.push(qsTr("lu"))
        if (f.suivi)
            actions.push(qsTr("suivi"))
        if (f.destination === "dossier")
            actions.push("→ " + (f.compte_destination ? f.compte_destination + " › " : "") + (f.dossier || "?"))
        else if (f.destination === "corbeille")
            actions.push("→ " + qsTr("corbeille"))
        else if (f.destination === "indesirables")
            actions.push("→ " + qsTr("indésirables"))
        return (f.compte || qsTr("tous les comptes")) + " · " + actions.join(", ")
               + (f.actif ? "" : " · " + qsTr("désactivé"))
    }
    function dossiersDuCompte(adresse) {
        return cibles.filter(function(c) { return c.adresse === adresse })
    }
    /// Boîte du dossier de destination (0.6.33) : celle du message, ou une
    /// autre du profil ; vide : celle du message.
    function boiteDestination(f) {
        return f ? (f.compte_destination || f.compte || "") : ""
    }
    /// Boîtes proposées comme destination : « sa boîte » (le compte visé),
    /// puis les autres ; pour tous les comptes, une boîte nommée seulement.
    function boitesDestination(f) {
        if (!f)
            return []
        var autres = adresses.filter(function(a) { return a !== f.compte })
        return f.compte ? [""].concat(autres) : autres
    }
    function enregistrer() {
        dlgFiltres.forceActiveFocus()
        if (boite.poserFiltres(JSON.stringify(liste))) {
            message = qsTr("Filtres enregistrés.")
            return true
        }
        message = boite.erreur
        return false
    }
    function appliquer() {
        if (!enregistrer())
            return
        var touches = 0
        for (var i = 0; i < fenetre.comptesConnus.length; ++i)
            touches += boite.appliquerFiltres(fenetre.comptesConnus[i].compte)
        message = touches > 0
                ? qsTr("Filtres appliqués : %1.").arg(fenetre.accord(touches, qsTr("message touché"), qsTr("messages touchés")))
                : qsTr("Aucun message des boîtes de réception ne répond aux filtres.")
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6

        // ------------------------------------------------- la liste
        Frame {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(150, Math.max(64, vueFiltres.contentHeight + 12))
            padding: 2
            ListView {
                id: vueFiltres
                anchors.fill: parent
                clip: true
                model: dlgFiltres.liste
                currentIndex: dlgFiltres.choisi
                ScrollBar.vertical: ScrollBar {}
                delegate: ItemDelegate {
                    required property var modelData
                    required property int index
                    width: ListView.view.width
                    highlighted: index === dlgFiltres.choisi
                    onClicked: dlgFiltres.choisi = index
                    contentItem: ColumnLayout {
                        spacing: 0
                        Label {
                            text: modelData.nom || qsTr("(sans nom)")
                            font.bold: true
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                            opacity: modelData.actif ? 1 : 0.55
                        }
                        Label {
                            text: dlgFiltres.resume(modelData)
                            elide: Text.ElideRight
                            font.pointSize: dlgFiltres.font.pointSize * 0.9
                            opacity: 0.75
                            Layout.fillWidth: true
                        }
                    }
                }
                Label {
                    anchors.centerIn: parent
                    visible: dlgFiltres.liste.length === 0
                    text: qsTr("Aucun filtre.")
                    opacity: 0.6
                }
            }
        }
        Flow {
            Layout.fillWidth: true
            spacing: 4
            Button {
                text: qsTr("Ajouter")
                onClicked: {
                    dlgFiltres.liste = dlgFiltres.liste.concat([dlgFiltres.vierge()])
                    dlgFiltres.choisi = dlgFiltres.liste.length - 1
                }
            }
            Button {
                text: qsTr("Supprimer")
                enabled: dlgFiltres.filtre !== null
                onClicked: {
                    var l = dlgFiltres.liste.slice()
                    l.splice(dlgFiltres.choisi, 1)
                    dlgFiltres.liste = l
                    dlgFiltres.choisi = Math.min(dlgFiltres.choisi, l.length - 1)
                }
            }
            Button {
                text: qsTr("Monter")
                enabled: dlgFiltres.choisi > 0
                onClicked: dlgFiltres.deplacer(-1)
            }
            Button {
                text: qsTr("Descendre")
                enabled: dlgFiltres.choisi >= 0 && dlgFiltres.choisi < dlgFiltres.liste.length - 1
                onClicked: dlgFiltres.deplacer(1)
            }
        }

        // ------------------------------------------------- le filtre choisi
        ScrollView {
            id: defilement
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: dlgFiltres.filtre !== null
            contentWidth: availableWidth
            clip: true
            ColumnLayout {
                width: defilement.availableWidth
                spacing: 6
                RowLayout {
                    Layout.fillWidth: true
                    TextField {
                        id: champNom
                        MenuSaisie {}
                        Layout.fillWidth: true
                        placeholderText: qsTr("Nom du filtre")
                        text: dlgFiltres.filtre ? dlgFiltres.filtre.nom : ""
                        onEditingFinished: dlgFiltres.modifier("nom", text)
                    }
                    CheckBox {
                        text: qsTr("Actif")
                        checked: dlgFiltres.filtre ? dlgFiltres.filtre.actif : false
                        onToggled: dlgFiltres.modifier("actif", checked)
                    }
                }
                Label { text: qsTr("Pour les messages qui arrivent dans la boîte de réception de") }
                ComboBox {
                    Layout.fillWidth: true
                    model: [qsTr("tous les comptes")].concat(dlgFiltres.adresses)
                    currentIndex: dlgFiltres.filtre ? dlgFiltres.adresses.indexOf(dlgFiltres.filtre.compte) + 1 : 0
                    onActivated: function(i) { dlgFiltres.modifier("compte", i === 0 ? "" : dlgFiltres.adresses[i - 1]) }
                }
                RowLayout {
                    Layout.fillWidth: true
                    Label { text: qsTr("et qui remplissent") }
                    ComboBox {
                        Layout.fillWidth: true
                        model: [qsTr("toutes les conditions"), qsTr("une condition au moins")]
                        currentIndex: dlgFiltres.filtre && !dlgFiltres.filtre.tout ? 1 : 0
                        onActivated: function(i) { dlgFiltres.modifier("tout", i === 0) }
                    }
                }
                Repeater {
                    model: dlgFiltres.filtre ? dlgFiltres.filtre.conditions : []
                    delegate: GridLayout {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        columns: fenetre.compact ? 2 : 4
                        ComboBox {
                            model: dlgFiltres.nomsChamps
                            currentIndex: Math.max(0, dlgFiltres.champs.indexOf(modelData.champ))
                            onActivated: function(i) { dlgFiltres.modifierCondition(index, "champ", dlgFiltres.champs[i]) }
                            Layout.preferredWidth: 140
                        }
                        ComboBox {
                            model: dlgFiltres.nomsOperateurs
                            currentIndex: Math.max(0, dlgFiltres.operateurs.indexOf(modelData.operateur))
                            onActivated: function(i) { dlgFiltres.modifierCondition(index, "operateur", dlgFiltres.operateurs[i]) }
                            Layout.preferredWidth: 160
                            Layout.fillWidth: fenetre.compact
                        }
                        TextField {
                            MenuSaisie {}
                            Layout.fillWidth: true
                            placeholderText: qsTr("texte")
                            text: modelData.valeur
                            onEditingFinished: dlgFiltres.modifierCondition(index, "valeur", text)
                        }
                        Button {
                            text: qsTr("Retirer")
                            enabled: dlgFiltres.filtre && dlgFiltres.filtre.conditions.length > 1
                            onClicked: {
                                var conditions = dlgFiltres.filtre.conditions.slice()
                                conditions.splice(index, 1)
                                dlgFiltres.modifier("conditions", conditions)
                            }
                        }
                    }
                }
                Button {
                    text: qsTr("Ajouter une condition")
                    onClicked: dlgFiltres.modifier("conditions", dlgFiltres.filtre.conditions.concat(
                                                       [{ champ: "objet", operateur: "contient", valeur: "" }]))
                }
                Label { text: qsTr("Alors :") ; font.bold: true }
                CheckBox {
                    text: qsTr("le marquer comme lu")
                    checked: dlgFiltres.filtre ? dlgFiltres.filtre.lu : false
                    onToggled: dlgFiltres.modifier("lu", checked)
                }
                CheckBox {
                    text: qsTr("le suivre (drapeau)")
                    checked: dlgFiltres.filtre ? dlgFiltres.filtre.suivi : false
                    onToggled: dlgFiltres.modifier("suivi", checked)
                }
                ComboBox {
                    Layout.fillWidth: true
                    model: dlgFiltres.nomsDestinations
                    currentIndex: dlgFiltres.filtre ? Math.max(0, dlgFiltres.destinations.indexOf(dlgFiltres.filtre.destination)) : 0
                    onActivated: function(i) { dlgFiltres.modifier("destination", dlgFiltres.destinations[i]) }
                }
                ComboBox {
                    id: choixBoite
                    readonly property var boites: dlgFiltres.boitesDestination(dlgFiltres.filtre)
                    visible: dlgFiltres.filtre !== null && dlgFiltres.filtre.destination === "dossier"
                    Layout.fillWidth: true
                    model: boites.map(function(b) {
                        return b === "" ? qsTr("dans sa boîte (%1)").arg(dlgFiltres.filtre.compte)
                                        : qsTr("dans la boîte %1").arg(b)
                    })
                    currentIndex: dlgFiltres.filtre ? boites.indexOf(dlgFiltres.filtre.compte_destination || "") : -1
                    displayText: currentIndex < 0 ? qsTr("Choisir la boîte…") : currentText
                    onActivated: function(i) {
                        if ((dlgFiltres.filtre.compte_destination || "") === boites[i])
                            return
                        dlgFiltres.modifier("compte_destination", boites[i])
                        dlgFiltres.modifier("dossier", "")
                    }
                }
                ComboBox {
                    id: choixDossier
                    readonly property var dossiers: dlgFiltres.filtre ? dlgFiltres.dossiersDuCompte(dlgFiltres.boiteDestination(dlgFiltres.filtre)) : []
                    visible: dlgFiltres.filtre !== null && dlgFiltres.filtre.destination === "dossier" && dlgFiltres.boiteDestination(dlgFiltres.filtre) !== ""
                    Layout.fillWidth: true
                    model: dossiers.map(function(d) { return fenetre.libelleLigne(d) + "  (" + d.chemin + ")" })
                    currentIndex: dlgFiltres.filtre ? dossiers.map(function(d) { return d.chemin }).indexOf(dlgFiltres.filtre.dossier) : -1
                    displayText: currentIndex < 0 ? qsTr("Choisir le dossier…") : currentText
                    onActivated: function(i) { dlgFiltres.modifier("dossier", dossiers[i].chemin) }
                }
                Label {
                    visible: dlgFiltres.filtre !== null && dlgFiltres.filtre.destination === "dossier" && dlgFiltres.boiteDestination(dlgFiltres.filtre) === ""
                    text: qsTr("Pour déplacer vers un dossier, choisissez la boîte où il se trouve.")
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                    opacity: 0.75
                }
                CheckBox {
                    text: qsTr("ne pas passer aux filtres suivants")
                    checked: dlgFiltres.filtre ? dlgFiltres.filtre.arreter : false
                    onToggled: dlgFiltres.modifier("arreter", checked)
                }
                CheckBox {
                    id: caseOutlook
                    visible: dlgFiltres.versOutlook(dlgFiltres.filtre)
                    text: qsTr("l'envoyer aussi à Outlook (Microsoft 365) : il trie même quand MMail est fermé")
                    checked: dlgFiltres.filtre ? dlgFiltres.filtre.serveur === true : false
                    onToggled: dlgFiltres.modifier("serveur", checked)
                    ToolTip.visible: hovered
                    ToolTip.delay: 500
                    ToolTip.text: qsTr("À l'enregistrement, le filtre devient une règle de la boîte de réception d'Outlook, nommée « MMail : » suivi de son nom. MMail ne touche pas aux règles créées dans Outlook. Décocher la case retire la règle d'Outlook.")
                }
                Label {
                    readonly property string raison: dlgFiltres.raisonOutlook(dlgFiltres.filtre)
                    visible: caseOutlook.visible && raison.length > 0
                    text: qsTr("Outlook ne sait pas faire ce filtre (%1) : MMail continue de l'appliquer seul.").arg(raison)
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                    opacity: 0.75
                }
            }
        }

        // ------------------------------------------------- en bas
        Label {
            Layout.fillWidth: true
            visible: dlgFiltres.message.length > 0
            text: dlgFiltres.message
            wrapMode: Text.Wrap
            font.bold: true
        }
        Label {
            Layout.fillWidth: true
            text: qsTr("Les filtres agissent sur les messages qui arrivent pendant qu'un MMail est ouvert. Pour un tri permanent sur une boîte du serveur de MMEDIA, le webmail a aussi ses filtres ; sur un compte Microsoft 365, un filtre peut aussi partir dans les règles d'Outlook.")
            wrapMode: Text.Wrap
            font.pointSize: dlgFiltres.font.pointSize * 0.9
            opacity: 0.7
        }
        Flow {
            Layout.fillWidth: true
            spacing: 4
            Button {
                text: qsTr("Appliquer aux boîtes de réception")
                enabled: dlgFiltres.liste.length > 0
                onClicked: dlgFiltres.appliquer()
            }
            Button {
                text: qsTr("Enregistrer")
                highlighted: true
                onClicked: dlgFiltres.enregistrer()
            }
            Button {
                visible: dlgFiltres.aAutoriser.length > 0 && Qt.platform.os !== "android"
                text: qsTr("Autoriser Outlook…")
                onClicked: dlgFiltres.autoriserOutlook()
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: qsTr("Microsoft vous demandera, dans votre navigateur, d'autoriser MMail à lire et modifier les réglages de votre boîte : ses règles, et la liste des catégories d'Outlook, qui est alors reliée à celle de MMail.")
            }
            Button {
                text: qsTr("Fermer")
                onClicked: dlgFiltres.close()
            }
        }
    }
}
