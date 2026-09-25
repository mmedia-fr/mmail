// SPDX-License-Identifier: GPL-3.0-or-later
// Fenêtre principale de MMail : arborescence, liste des messages, message.
//
// Trois volets côte à côte, dans l'ordre de lecture d'Outlook, qui sert de
// référence de conception (décision 1) : l'arborescence tient un seul volet à
// gauche, tous comptes confondus (décision 2), la rubrique Favoris au-dessus
// (décision 3). Un message se déplace vers n'importe quel dossier de n'importe
// quelle boîte, par glisser-déposer ou par menu (décisions 5 et 12).
//
// Sur un écran étroit — un téléphone —, un seul volet à la fois, et le retour
// arrière remonte d'un cran.
//
// Écrit pour Qt 6.4, comme MMdedit : ce qui s'y compile passe aussi sur le
// Qt 6.8 de l'intégration continue.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

ApplicationWindow {
    id: fenetre

    width: 1280
    height: 800
    visible: true
    title: boite.dossierCourant.length > 0
           ? "MMail — " + libelleCourant()
           : "MMail"

    Socle { id: socle }
    Boite { id: boite }
    readonly property string noyau: socle.noyau

    // Un seul volet à la fois sous cette largeur : 0 arborescence, 1 liste,
    // 2 message.
    readonly property bool compact: width < 760
    property int vue: 0

    // Sélection de la liste : uid → vrai. Réaffectée à chaque changement pour
    // que les liaisons qui la lisent soient réévaluées.
    property var selection: ({})
    property int ancre: -1
    property int uidCourant: 0
    property bool sourceVisible: false
    // Messages retirés de la liste en attendant la fin de leur déplacement :
    // l'interface reflète le tri tout de suite, le serveur suit (décision 16).
    property var enDeplacement: ({})
    property int deplacementsEnVol: 0
    // Pièces jointes du message affiché, telles que le noyau les rend.
    property var pieces: []

    Settings {
        id: reglages
        category: "interface"
        property bool afficherMasques: false
        property bool memoriser: true
        property real largeurArborescence: 260
        property real largeurListe: 420
        // Zoom propre à chaque colonne, 1 = taille de l'apparence.
        property real zoomArborescence: 1
        property real zoomListe: 1
        property real zoomMessage: 1
    }

    // ---------------------------------------------------------- apparences
    //
    // Les trois apparences de MMdedit, reprises à l'identique : c'est la
    // palette et la police de la fenêtre qui changent, le style des contrôles
    // restant Fusion, seul à honorer une palette partout (cf. cpp/main.cpp).
    // « systeme » ne pose aucune surcharge : la palette reste celle du bureau.
    property string apparence: "moderne"

    Settings {
        category: "apparence"
        property alias theme: fenetre.apparence
    }

    readonly property var jeuxApparence: ({
        "classique": {
            libelle: qsTr("Classique (Windows 9x)"),
            fond: "#d4d0c8", texte: "#000000", base: "#ffffff",
            bouton: "#d4d0c8", surbrillance: "#000080", texteSurbrillance: "#ffffff",
            police: "MS Shell Dlg 2", taille: 9, mono: "Courier New"
        },
        "moderne": {
            libelle: qsTr("Moderne (M-Media)"),
            fond: "#f6f7f8", texte: "#231f20", base: "#ffffff",
            bouton: "#ffffff", surbrillance: "#21abe3", texteSurbrillance: "#ffffff",
            police: "Segoe UI", taille: 10, mono: "Cascadia Mono"
        },
        "systeme": { libelle: qsTr("Système (aspect natif)") }
    })
    readonly property var nomsApparence: ["classique", "moderne", "systeme"]

    // Nul pour « systeme » : c'est ce qui coupe les surcharges ci-dessous.
    readonly property var jeu: apparence === "systeme" || !jeuxApparence[apparence]
                               ? null : jeuxApparence[apparence]

    // Surcharges conditionnelles plutôt qu'affectations directes : quand
    // « when » redevient faux, RestoreBindingOrValue rend la valeur d'origine —
    // c'est ce qui permet de revenir à l'apparence du bureau sans redémarrer.
    Instantiator {
        model: [
            { propriete: "palette.window", clef: "fond" },
            { propriete: "palette.base", clef: "base" },
            { propriete: "palette.text", clef: "texte" },
            { propriete: "palette.windowText", clef: "texte" },
            { propriete: "palette.button", clef: "bouton" },
            { propriete: "palette.buttonText", clef: "texte" },
            { propriete: "palette.highlight", clef: "surbrillance" },
            { propriete: "palette.highlightedText", clef: "texteSurbrillance" },
            { propriete: "font.family", clef: "police" },
            { propriete: "font.pointSize", clef: "taille" }
        ]
        delegate: Binding {
            required property var modelData
            target: fenetre
            property: modelData.propriete
            value: fenetre.jeu ? fenetre.jeu[modelData.clef] : ""
            when: fenetre.jeu !== null
            restoreMode: Binding.RestoreBindingOrValue
        }
    }

    // Taille de police de l'apparence, en points, avant zoom. Celle du système
    // est relevée au démarrage : la fenêtre la perd dès qu'une apparence
    // impose la sienne.
    readonly property real tailleSysteme: Qt.application.font.pointSize > 0
                                          ? Qt.application.font.pointSize : 10
    readonly property real tailleBase: jeu ? jeu.taille : tailleSysteme
    readonly property real tailleArborescence: tailleBase * reglages.zoomArborescence
    readonly property real tailleListe: tailleBase * reglages.zoomListe
    readonly property real tailleMessage: tailleBase * reglages.zoomMessage
    readonly property string policeMono: jeu ? jeu.mono : "monospace"

    // --------------------------------------------------------------- zoom
    //
    // Chaque colonne a son zoom : Ctrl + molette sur elle, ou Ctrl + / Ctrl − /
    // Ctrl 0 sur la dernière colonne survolée ; au doigt, le pincement.
    property string colonneActive: "liste"
    readonly property real zoomMinimum: 0.6
    readonly property real zoomMaximum: 3

    function zoomDe(colonne) {
        if (colonne === "arborescence") return reglages.zoomArborescence
        if (colonne === "message") return reglages.zoomMessage
        return reglages.zoomListe
    }

    function poserZoom(colonne, valeur) {
        var z = Math.max(zoomMinimum, Math.min(zoomMaximum, Math.round(valeur * 100) / 100))
        if (colonne === "arborescence") reglages.zoomArborescence = z
        else if (colonne === "message") reglages.zoomMessage = z
        else reglages.zoomListe = z
        messageEtat.texte = qsTr("Zoom : %1 %").arg(Math.round(z * 100))
        return z
    }

    function zoomer(colonne, facteur) {
        return poserZoom(colonne, zoomDe(colonne) * facteur)
    }

    Connections {
        target: boite
        function onRevisionChanged() { fenetre.rafraichirArborescence() }
    }

    // ------------------------------------------------------------- en-tête
    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 6
            anchors.rightMargin: 6
            spacing: 6

            ToolButton {
                visible: fenetre.compact && fenetre.vue > 0
                text: "‹"
                font.pixelSize: 22
                onClicked: fenetre.vue = fenetre.vue - 1
            }
            ToolButton {
                text: qsTr("Ajouter un compte…")
                visible: !fenetre.compact || fenetre.vue === 0
                onClicked: fenetre.demanderCompte(null, "")
            }
            ToolButton {
                text: qsTr("Actualiser")
                onClicked: boite.actualiser()
            }
            ToolButton {
                visible: !fenetre.compact || fenetre.vue === 0
                checkable: true
                checked: reglages.afficherMasques
                text: qsTr("Dossiers masqués")
                onToggled: {
                    reglages.afficherMasques = checked
                    fenetre.rafraichirArborescence()
                }
            }
            ToolButton {
                id: boutonAffichage
                text: qsTr("Affichage")
                onClicked: menuAffichage.popup(boutonAffichage, 0, boutonAffichage.height)
            }
            Label {
                text: fenetre.compact && fenetre.vue > 0 ? fenetre.libelleCourant() : ""
                elide: Text.ElideRight
                font.bold: true
                Layout.fillWidth: true
            }
            Label {
                visible: boite.enAttente > 0
                text: qsTr("%n déplacement(s) en attente", "", boite.enAttente)
                opacity: 0.8
            }
            BusyIndicator {
                running: boite.occupe
                visible: running
                Layout.preferredHeight: 26
                Layout.preferredWidth: 26
            }
        }
    }

    // ------------------------------------------------------------- volets
    SplitView {
        id: volets
        anchors.fill: parent
        orientation: Qt.Horizontal

        // --- arborescence -------------------------------------------------
        ScrollView {
            id: voletArborescence
            visible: !fenetre.compact || fenetre.vue === 0
            SplitView.preferredWidth: fenetre.compact ? fenetre.width : reglages.largeurArborescence
            SplitView.minimumWidth: 160
            SplitView.fillWidth: fenetre.compact
            clip: true
            contentWidth: availableWidth
            font.pointSize: fenetre.tailleArborescence
            background: Rectangle { color: fenetre.palette.base }
            onWidthChanged: if (!fenetre.compact && visible) reglages.largeurArborescence = width

            // Les gestionnaires vivent dans la liste, pas dans le ScrollView :
            // celui-ci ne reconnaît sa liste comme contenu défilant que si elle
            // est son seul enfant — sinon elle prend une largeur nulle.
            ListView {
                id: vueArborescence
                model: ListModel { id: modeleArborescence }
                boundsBehavior: Flickable.StopAtBounds
                delegate: ligneArborescence

                HoverHandler { onHoveredChanged: if (hovered) fenetre.colonneActive = "arborescence" }

                WheelHandler {
                    acceptedModifiers: Qt.ControlModifier
                    onWheel: function(roue) {
                        fenetre.zoomer("arborescence", roue.angleDelta.y > 0 ? 1.1 : 1 / 1.1)
                    }
                }
                PinchHandler {
                    target: null
                    property real depart: 1
                    onActiveChanged: if (active) depart = reglages.zoomArborescence
                    onActiveScaleChanged: if (active) fenetre.poserZoom("arborescence", depart * activeScale)
                }
            }
        }

        // --- liste des messages -------------------------------------------
        ScrollView {
            id: voletListe
            visible: !fenetre.compact || fenetre.vue === 1
            SplitView.preferredWidth: fenetre.compact ? fenetre.width : reglages.largeurListe
            SplitView.minimumWidth: 240
            SplitView.fillWidth: fenetre.compact
            clip: true
            contentWidth: availableWidth
            font.pointSize: fenetre.tailleListe
            background: Rectangle { color: fenetre.palette.base }
            onWidthChanged: if (!fenetre.compact && visible) reglages.largeurListe = width

            ListView {
                id: vueMessages
                model: ListModel { id: modeleMessages }
                boundsBehavior: Flickable.StopAtBounds
                currentIndex: -1
                focus: true
                keyNavigationEnabled: false
                delegate: ligneMessage

                Keys.onUpPressed: function(touche) {
                    fenetre.deplacerCurseur(-1, touche.modifiers & Qt.ShiftModifier)
                }
                Keys.onDownPressed: function(touche) {
                    fenetre.deplacerCurseur(1, touche.modifiers & Qt.ShiftModifier)
                }

                HoverHandler { onHoveredChanged: if (hovered) fenetre.colonneActive = "liste" }

                WheelHandler {
                    acceptedModifiers: Qt.ControlModifier
                    onWheel: function(roue) {
                        fenetre.zoomer("liste", roue.angleDelta.y > 0 ? 1.1 : 1 / 1.1)
                    }
                }
                PinchHandler {
                    target: null
                    property real depart: 1
                    onActiveChanged: if (active) depart = reglages.zoomListe
                    onActiveScaleChanged: if (active) fenetre.poserZoom("liste", depart * activeScale)
                }

                Label {
                    anchors.centerIn: parent
                    visible: modeleMessages.count === 0
                    opacity: 0.6
                    text: boite.dossierCourant.length === 0
                          ? qsTr("Choisissez un dossier.")
                          : qsTr("Aucun message.")
                }
            }
        }

        // --- message ------------------------------------------------------
        ColumnLayout {
            visible: !fenetre.compact || fenetre.vue === 2
            SplitView.fillWidth: true
            spacing: 0

            HoverHandler { onHoveredChanged: if (hovered) fenetre.colonneActive = "message" }

            ToolBar {
                Layout.fillWidth: true
                visible: fenetre.uidCourant > 0
                font.pointSize: fenetre.tailleMessage
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        Label {
                            id: sujetAffiche
                            font.bold: true
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                        Label {
                            id: auteurAffiche
                            opacity: 0.8
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }
                    ToolButton {
                        text: qsTr("Déplacer…")
                        onClicked: fenetre.ouvrirDeplacer()
                    }
                    ToolButton {
                        text: qsTr("Supprimer")
                        onClicked: fenetre.supprimerSelection()
                    }
                    // Décision 6 : voir le message brut, en-têtes compris, en un geste.
                    ToolButton {
                        text: fenetre.sourceVisible ? qsTr("Message") : qsTr("Source")
                        onClicked: fenetre.basculerSource()
                    }
                }
            }

            // Pièces jointes du message affiché : un bouton par pièce, qui ouvre
            // son menu (Ouvrir, Enregistrer sous…).
            Pane {
                Layout.fillWidth: true
                visible: fenetre.uidCourant > 0 && !fenetre.sourceVisible && fenetre.pieces.length > 0
                font.pointSize: fenetre.tailleMessage
                padding: 4
                background: Rectangle { color: fenetre.palette.window }
                Flow {
                    width: parent.width
                    spacing: 4
                    Label {
                        text: qsTr("Pièces jointes :")
                        opacity: 0.7
                        height: boutonsPieces.count > 0 ? boutonsPieces.itemAt(0).height : implicitHeight
                        verticalAlignment: Text.AlignVCenter
                    }
                    Repeater {
                        id: boutonsPieces
                        model: fenetre.pieces
                        delegate: Button {
                            required property var modelData
                            flat: true
                            text: modelData.nom + "  (" + fenetre.tailleLisible(modelData.taille) + ")"
                            onClicked: menuPiece.ouvrir(modelData, this)
                            ToolTip.visible: hovered && modelData.risquee
                            ToolTip.text: qsTr("Programme ou script : il s'enregistre, il ne s'ouvre pas depuis MMail.")
                        }
                    }
                }
            }

            ScrollView {
                id: cadreCorps
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                contentWidth: fenetre.sourceVisible ? -1 : availableWidth

                TextArea {
                    id: vueCorps
                    readOnly: true
                    selectByMouse: true
                    wrapMode: fenetre.sourceVisible ? TextEdit.NoWrap : TextEdit.Wrap
                    font.pointSize: fenetre.tailleMessage
                    font.family: fenetre.sourceVisible ? fenetre.policeMono : fenetre.font.family
                    background: Rectangle { color: fenetre.palette.base }
                    text: qsTr("Aucun message sélectionné.")

                    WheelHandler {
                        acceptedModifiers: Qt.ControlModifier
                        onWheel: function(roue) {
                            fenetre.zoomer("message", roue.angleDelta.y > 0 ? 1.1 : 1 / 1.1)
                        }
                    }
                    PinchHandler {
                        target: null
                        property real depart: 1
                        onActiveChanged: if (active) depart = reglages.zoomMessage
                        onActiveScaleChanged: if (active) fenetre.poserZoom("message", depart * activeScale)
                    }
                }
            }
        }
    }

    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            Label {
                text: boite.erreur.length > 0 ? boite.erreur : messageEtat.texte
                color: boite.erreur.length > 0 ? "#b00020" : palette.windowText
                elide: Text.ElideRight
                maximumLineCount: 1
                Layout.fillWidth: true
            }
        }
    }

    QtObject {
        id: messageEtat
        property string texte: ""
    }

    // ------------------------------------------------ ligne d'arborescence
    Component {
        id: ligneArborescence

        ItemDelegate {
            id: ligne
            width: ListView.view.width
            readonly property bool estDossier: model.genre === "dossier" || model.genre === "favori"
            readonly property bool estCourant: estDossier
                    && model.compte === boite.compteCourant
                    && model.chemin === boite.dossierCourant
            // Vrai pendant qu'un glisser-déposer accepté survole la ligne.
            property bool survol: false
            highlighted: estCourant
            hoverEnabled: true

            function cliquer() {
                if (model.genre === "compte")
                    boite.replierCompte(model.compte, !model.replie)
                else if (estDossier && model.selectionnable)
                    fenetre.ouvrirDossier(model.compte, model.chemin)
            }

            function menuLigne() {
                if (model.genre === "compte")
                    menuCompte.ouvrir(model.compte, model.adresse, model.hote, model.etat)
                else if (estDossier)
                    menuDossier.ouvrir(model.compte, model.chemin, model.favori, model.masque)
            }

            // Cible de dépôt : des messages glissés depuis la liste (sur un
            // dossier), ou un dossier glissé depuis l'arborescence (sur la
            // rubrique Favoris ou l'un de ses favoris, décision 3).
            DropArea {
                id: depot
                anchors.fill: parent
                keys: ["mmail/messages", "mmail/dossier"]
                onEntered: function(glisse) {
                    var dossierGlisse = glisse.keys.indexOf("mmail/dossier") >= 0
                    var accepte = dossierGlisse
                            ? (model.genre === "rubrique" || model.genre === "favori"
                               || model.genre === "favori-vide")
                            : (ligne.estDossier && model.selectionnable)
                    glisse.accepted = accepte
                    ligne.survol = accepte
                }
                onExited: ligne.survol = false
                onDropped: function(depose) {
                    ligne.survol = false
                    if (depose.keys.indexOf("mmail/dossier") >= 0) {
                        var source = depose.source
                        boite.placerFavori(source.compte, source.chemin,
                                           model.genre === "favori" ? model.compte : 0,
                                           model.genre === "favori" ? model.chemin : "")
                    } else {
                        fenetre.deplacerVers(model.compte, model.chemin)
                    }
                    depose.accept(Qt.MoveAction)
                }
            }
            Rectangle {
                anchors.fill: parent
                visible: ligne.survol
                color: "transparent"
                border.color: fenetre.palette.highlight
                border.width: 2
                radius: 3
            }

            // Ce qui part quand on glisse un dossier : son nom, qui suit le
            // pointeur. Il porte de quoi désigner le dossier au dépôt.
            Rectangle {
                id: etiquetteDossier
                property int compte: model.compte
                property string chemin: model.chemin
                width: texteEtiquetteDossier.implicitWidth + 16
                height: texteEtiquetteDossier.implicitHeight + 8
                radius: 4
                color: fenetre.palette.highlight
                visible: zoneLigne.drag.active
                Drag.active: zoneLigne.drag.active
                Drag.keys: ["mmail/dossier"]
                Drag.hotSpot.x: 8
                Drag.hotSpot.y: height / 2
                Drag.supportedActions: Qt.MoveAction
                Label {
                    id: texteEtiquetteDossier
                    anchors.centerIn: parent
                    color: fenetre.palette.highlightedText
                    text: fenetre.libelleLigne(model)
                }
                states: State {
                    when: zoneLigne.drag.active
                    ParentChange { target: etiquetteDossier; parent: fenetre.contentItem }
                }
            }

            MouseArea {
                id: zoneLigne
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                // Seul un dossier se glisse, et pas au doigt : glisser y fait
                // défiler l'arborescence.
                drag.target: ligne.estDossier && !fenetre.compact ? etiquetteDossier : null
                drag.threshold: 8
                onPressed: function(souris) {
                    etiquetteDossier.x = souris.x
                    etiquetteDossier.y = souris.y
                }
                onClicked: function(souris) {
                    if (souris.button === Qt.RightButton)
                        ligne.menuLigne()
                    else
                        ligne.cliquer()
                }
                onPressAndHold: ligne.menuLigne()
                onReleased: {
                    if (drag.active)
                        etiquetteDossier.Drag.drop()
                }
            }

            contentItem: RowLayout {
                spacing: 6
                Item {
                    Layout.preferredWidth: model.genre === "dossier" ? 14 + model.profondeur * 14
                                         : model.genre === "favori" || model.genre === "favori-vide" ? 14 : 0
                }
                // « › » plutôt qu'un triangle : présent dans toutes les polices.
                Label {
                    visible: model.genre === "compte"
                    text: "›"
                    color: fenetre.palette.windowText
                    font.bold: true
                    rotation: model.replie ? 0 : 90
                    opacity: 0.7
                }
                Label {
                    text: fenetre.libelleLigne(model)
                    color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.windowText
                    font.bold: model.genre === "compte" || model.genre === "rubrique" || model.nonLus > 0
                    font.italic: model.masque || model.genre === "favori-vide"
                    opacity: model.masque || model.genre === "favori-vide"
                             || (ligne.estDossier && !model.selectionnable) ? 0.5 : 1
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
                Label {
                    visible: model.genre === "compte" && model.etat !== "pret"
                    text: model.etat === "connexion" ? qsTr("connexion…")
                        : model.etat === "erreur" ? qsTr("erreur") : qsTr("hors ligne")
                    color: model.etat === "erreur" ? "#b00020" : fenetre.palette.windowText
                    opacity: 0.7
                    font.pointSize: fenetre.tailleArborescence * 0.85
                }
                Label {
                    visible: ligne.estDossier && model.nonLus > 0
                    text: model.nonLus
                    font.bold: true
                    color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.highlight
                }
            }
        }
    }

    // ---------------------------------------------------- ligne de message
    Component {
        id: ligneMessage

        ItemDelegate {
            id: ligne
            width: ListView.view.width
            readonly property bool choisi: fenetre.selection[model.uid] === true
            highlighted: choisi
            padding: 0

            // Bandeau des non-lus, comme Outlook.
            Rectangle {
                width: 3
                height: parent.height
                visible: !model.lu
                color: fenetre.palette.highlight
            }

            topPadding: 5
            bottomPadding: 5
            contentItem: ColumnLayout {
                spacing: 1
                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    Layout.rightMargin: 8
                    Label {
                        text: model.expediteur
                        color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.windowText
                        font.bold: !model.lu
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Label {
                        text: fenetre.dateCourte(model.date)
                        color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.windowText
                        opacity: 0.7
                        font.pointSize: fenetre.tailleListe * 0.85
                    }
                }
                Label {
                    Layout.leftMargin: 10
                    Layout.rightMargin: 8
                    text: model.sujet
                    font.bold: !model.lu
                    color: ligne.highlighted ? fenetre.palette.highlightedText
                         : model.lu ? fenetre.palette.windowText : fenetre.palette.highlight
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }

            // Ce qui part quand on glisse : l'étiquette qui suit le pointeur.
            Rectangle {
                id: etiquette
                property string uids: ""
                width: texteEtiquette.implicitWidth + 16
                height: texteEtiquette.implicitHeight + 8
                radius: 4
                color: fenetre.palette.highlight
                visible: zone.drag.active
                Drag.active: zone.drag.active
                Drag.keys: ["mmail/messages"]
                Drag.hotSpot.x: 8
                Drag.hotSpot.y: height / 2
                Drag.supportedActions: Qt.MoveAction
                Label {
                    id: texteEtiquette
                    anchors.centerIn: parent
                    color: fenetre.palette.highlightedText
                    text: qsTr("%n message(s)", "", Object.keys(fenetre.selection).length)
                }
                states: State {
                    when: zone.drag.active
                    ParentChange { target: etiquette; parent: fenetre.contentItem }
                }
            }

            MouseArea {
                id: zone
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                // Au doigt, glisser fait défiler la liste : le tri passe par
                // l'appui long et le menu.
                drag.target: fenetre.compact ? null : etiquette
                drag.threshold: 8
                // Vrai si l'appui a déjà choisi la ligne : le clic n'a alors
                // plus rien à faire.
                property bool choisieAuPress: false

                onPressed: function(souris) {
                    choisieAuPress = souris.button === Qt.LeftButton && !ligne.choisi
                            && !(souris.modifiers & (Qt.ControlModifier | Qt.ShiftModifier))
                    // Glisser une ligne non choisie la choisit d'abord.
                    if (choisieAuPress)
                        fenetre.choisir(index, 0)
                    etiquette.x = souris.x
                    etiquette.y = souris.y
                }
                onClicked: function(souris) {
                    if (souris.button === Qt.RightButton) {
                        if (!ligne.choisi)
                            fenetre.choisir(index, 0)
                        menuMessage.popup()
                        return
                    }
                    if (!choisieAuPress)
                        fenetre.choisir(index, souris.modifiers)
                }
                onPressAndHold: {
                    if (!ligne.choisi)
                        fenetre.choisir(index, 0)
                    menuMessage.popup()
                }
                onReleased: {
                    if (drag.active)
                        etiquette.Drag.drop()
                }
            }
        }
    }

    // ---------------------------------------------------------------- menus
    Menu {
        id: menuAffichage
        Repeater {
            model: fenetre.nomsApparence
            MenuItem {
                required property string modelData
                text: fenetre.jeuxApparence[modelData].libelle
                checkable: true
                checked: fenetre.apparence === modelData
                onTriggered: fenetre.choisirApparence(modelData)
            }
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Agrandir la colonne survolée (Ctrl +)")
            onTriggered: fenetre.zoomer(fenetre.colonneActive, 1.1)
        }
        MenuItem {
            text: qsTr("Réduire la colonne survolée (Ctrl −)")
            onTriggered: fenetre.zoomer(fenetre.colonneActive, 1 / 1.1)
        }
        MenuItem {
            text: qsTr("Taille normale partout")
            onTriggered: {
                fenetre.poserZoom("arborescence", 1)
                fenetre.poserZoom("liste", 1)
                fenetre.poserZoom("message", 1)
            }
        }
    }

    Menu {
        id: menuPiece
        property var piece: null
        function ouvrir(p, bouton) {
            piece = p
            popup(bouton, 0, bouton.height)
        }
        MenuItem {
            text: qsTr("Ouvrir")
            enabled: menuPiece.piece !== null && !menuPiece.piece.risquee
            onTriggered: fenetre.ouvrirPiece(menuPiece.piece)
        }
        MenuItem {
            text: qsTr("Enregistrer sous…")
            // Sous Android, le dialogue rend une adresse « content:// » que le
            // noyau ne sait pas écrire : l'ouverture y suffit.
            visible: Qt.platform.os !== "android"
            height: visible ? implicitHeight : 0
            onTriggered: fenetre.enregistrerPiece(menuPiece.piece)
        }
    }

    FileDialog {
        id: dlgEnregistrerPiece
        property var piece: null
        property int uid: 0
        title: qsTr("Enregistrer la pièce jointe")
        fileMode: FileDialog.SaveFile
        onAccepted: boite.enregistrerPiece(uid, piece.indice, selectedFile.toString())
    }

    Menu {
        id: menuMessage
        MenuItem { text: qsTr("Déplacer vers…"); onTriggered: fenetre.ouvrirDeplacer() }
        MenuSeparator {}
        MenuItem { text: qsTr("Marquer comme lu"); onTriggered: fenetre.marquerSelection(true) }
        MenuItem { text: qsTr("Marquer comme non lu"); onTriggered: fenetre.marquerSelection(false) }
        MenuSeparator {}
        MenuItem { text: qsTr("Supprimer"); onTriggered: fenetre.supprimerSelection() }
        MenuItem {
            text: qsTr("Afficher la source")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: {
                fenetre.sourceVisible = false
                fenetre.basculerSource()
            }
        }
    }

    Menu {
        id: menuDossier
        property int compte: 0
        property string chemin: ""
        property bool favori: false
        property bool masque: false
        function ouvrir(c, ch, f, m) {
            compte = c; chemin = ch; favori = f; masque = m
            popup()
        }
        MenuItem {
            text: menuDossier.favori ? qsTr("Retirer des favoris") : qsTr("Ajouter aux favoris")
            onTriggered: boite.epinglerDossier(menuDossier.compte, menuDossier.chemin, !menuDossier.favori)
        }
        MenuItem {
            text: menuDossier.masque ? qsTr("Réafficher ce dossier") : qsTr("Masquer ce dossier")
            onTriggered: boite.masquerDossier(menuDossier.compte, menuDossier.chemin, !menuDossier.masque)
        }
    }

    Menu {
        id: menuCompte
        property int compte: 0
        property string adresse: ""
        property string hote: ""
        property string etat: ""
        function ouvrir(c, a, h, e) {
            compte = c; adresse = a; hote = h; etat = e
            popup()
        }
        MenuItem {
            text: menuCompte.etat === "pret" ? qsTr("Se déconnecter") : qsTr("Se connecter…")
            onTriggered: {
                if (menuCompte.etat === "pret")
                    boite.deconnecter(menuCompte.compte)
                else
                    fenetre.connecterCompte({ id: menuCompte.compte, adresse: menuCompte.adresse,
                                              hote: menuCompte.hote })
            }
        }
        MenuItem {
            text: qsTr("Oublier le mot de passe")
            onTriggered: coffre.effacer(fenetre.cle(menuCompte.adresse, menuCompte.hote))
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Retirer ce compte de MMail…")
            onTriggered: dlgRetrait.ouvrir(menuCompte.compte, menuCompte.adresse, menuCompte.hote)
        }
    }

    Shortcut {
        sequences: [StandardKey.Delete]
        enabled: !fenetre.dialogueOuvert() && Object.keys(fenetre.selection).length > 0
        onActivated: fenetre.supprimerSelection()
    }
    // Raccourcis d'Outlook : déplacer, marquer lu, marquer non lu.
    Shortcut {
        sequence: "Ctrl+Shift+V"
        enabled: !fenetre.dialogueOuvert() && Object.keys(fenetre.selection).length > 0
        onActivated: fenetre.ouvrirDeplacer()
    }
    Shortcut {
        sequence: "Ctrl+Q"
        enabled: !fenetre.dialogueOuvert()
        onActivated: fenetre.marquerSelection(true)
    }
    Shortcut {
        sequence: "Ctrl+U"
        enabled: !fenetre.dialogueOuvert()
        onActivated: fenetre.marquerSelection(false)
    }
    Shortcut {
        sequences: [StandardKey.Refresh]
        onActivated: boite.actualiser()
    }
    Shortcut {
        sequences: [StandardKey.ZoomIn, "Ctrl+="]
        onActivated: fenetre.zoomer(fenetre.colonneActive, 1.1)
    }
    Shortcut {
        sequences: [StandardKey.ZoomOut]
        onActivated: fenetre.zoomer(fenetre.colonneActive, 1 / 1.1)
    }
    Shortcut {
        sequence: "Ctrl+0"
        onActivated: fenetre.poserZoom(fenetre.colonneActive, 1)
    }
    Shortcut {
        sequences: [StandardKey.SelectAll]
        enabled: !fenetre.dialogueOuvert() && !vueCorps.activeFocus
        onActivated: fenetre.toutChoisir()
    }

    // ----------------------------------------------------------- dialogues
    Dialog {
        id: dlgCompte
        property int compteId: 0
        title: compteId > 0 ? qsTr("Mot de passe du compte") : qsTr("Ajouter un compte")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(460, fenetre.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: fenetre.validerCompte()
        onRejected: fenetre.demandeSuivante()
        onOpened: (champAdresse.text.length > 0 ? champMotDePasse : champAdresse).forceActiveFocus()

        GridLayout {
            anchors.fill: parent
            columns: 2
            columnSpacing: 8
            rowSpacing: 6

            Label {
                id: noteCompte
                Layout.columnSpan: 2
                Layout.fillWidth: true
                visible: text.length > 0
                wrapMode: Text.Wrap
                color: "#b00020"
            }
            Label { text: qsTr("Adresse :") }
            TextField {
                id: champAdresse
                placeholderText: "nom@exemple.fr"
                enabled: dlgCompte.compteId === 0
                inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
            }
            Label { text: qsTr("Serveur IMAP :") }
            TextField {
                id: champHote
                placeholderText: "mail.exemple.fr"
                enabled: dlgCompte.compteId === 0
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
            }
            Label { text: qsTr("Mot de passe :") }
            TextField {
                id: champMotDePasse
                echoMode: TextInput.Password
                Layout.fillWidth: true
                onAccepted: dlgCompte.accept()
            }
            CheckBox {
                id: caseMemoriser
                Layout.columnSpan: 2
                checked: reglages.memoriser
                text: qsTr("Mémoriser le mot de passe dans le coffre du système")
            }
            Label {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.75
                font.pixelSize: 11
                text: qsTr("Le mot de passe n'est jamais écrit par MMail : il est confié au coffre du système (Gestionnaire d'identifiants sous Windows, Secret Service ou KWallet sous Linux, Keystore sous Android). Connexion en IMAPS, port 993.")
            }
        }
    }

    Dialog {
        id: dlgDeplacer
        title: qsTr("Déplacer %n message(s) vers…", "", Object.keys(fenetre.selection).length)
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(520, fenetre.width - 24)
        height: Math.min(560, fenetre.height - 40)
        standardButtons: Dialog.Ok | Dialog.Cancel
        property var cibles: []
        onOpened: {
            champFiltre.text = ""
            filtrer()
            champFiltre.forceActiveFocus()
        }
        onAccepted: {
            if (vueCibles.currentIndex >= 0 && vueCibles.currentIndex < modeleCibles.count) {
                var c = modeleCibles.get(vueCibles.currentIndex)
                fenetre.deplacerVers(c.compte, c.chemin)
            }
        }
        function filtrer() {
            modeleCibles.clear()
            var filtre = champFiltre.text.toLowerCase()
            for (var i = 0; i < cibles.length; ++i) {
                var c = cibles[i]
                if (c.compte === boite.compteCourant && c.chemin === boite.dossierCourant)
                    continue
                var texte = (c.adresse + " " + c.chemin + " " + fenetre.libelleLigne(c)).toLowerCase()
                if (filtre.length === 0 || texte.indexOf(filtre) >= 0)
                    modeleCibles.append(c)
            }
            vueCibles.currentIndex = modeleCibles.count > 0 ? 0 : -1
        }

        ColumnLayout {
            anchors.fill: parent
            TextField {
                id: champFiltre
                Layout.fillWidth: true
                placeholderText: qsTr("Filtrer : nom du dossier ou de la boîte")
                onTextChanged: dlgDeplacer.filtrer()
                Keys.onDownPressed: vueCibles.incrementCurrentIndex()
                Keys.onUpPressed: vueCibles.decrementCurrentIndex()
                onAccepted: dlgDeplacer.accept()
            }
            ListView {
                id: vueCibles
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: ListModel { id: modeleCibles }
                delegate: ItemDelegate {
                    width: ListView.view.width
                    highlighted: ListView.isCurrentItem
                    onClicked: vueCibles.currentIndex = index
                    onDoubleClicked: dlgDeplacer.accept()
                    contentItem: RowLayout {
                        Label {
                            text: model.adresse
                            opacity: 0.6
                            Layout.preferredWidth: 170
                            elide: Text.ElideRight
                        }
                        Item { Layout.preferredWidth: model.profondeur * 12 }
                        Label {
                            text: fenetre.libelleLigne(model)
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }
                }
            }
        }
    }

    Dialog {
        id: dlgRetrait
        property int compteId: 0
        property string adresse: ""
        property string hote: ""
        function ouvrir(c, a, h) { compteId = c; adresse = a; hote = h; open() }
        title: qsTr("Retirer le compte")
        modal: true
        anchors.centerIn: Overlay.overlay
        // Largeur fixée ici, et non déduite du texte : sans quoi le texte, qui
        // se replie selon la largeur, et le dialogue se calculent l'un l'autre.
        width: Math.min(460, fenetre.width - 24)
        standardButtons: Dialog.Yes | Dialog.No
        Label {
            width: dlgRetrait.availableWidth
            wrapMode: Text.Wrap
            text: qsTr("Retirer %1 de MMail ? L'index local de ce compte et son mot de passe mémorisé sont effacés de ce poste. Rien n'est supprimé sur le serveur.").arg(dlgRetrait.adresse)
        }
        onAccepted: {
            coffre.effacer(fenetre.cle(adresse, hote))
            boite.retirerCompte(compteId)
            fenetre.viderListe()
        }
    }

    // ------------------------------------------------------ coffre et session
    Coffre {
        id: coffre

        onLu: function(cle, secret, trouve, erreur) {
            var compte = fenetre.compteParCle(cle)
            if (!compte)
                return
            if (trouve) {
                boite.connecter(compte.hote, compte.adresse, secret)
                return
            }
            if (erreur.length > 0)
                messageEtat.texte = qsTr("Coffre du système indisponible : %1").arg(erreur)
            fenetre.demanderCompte(compte, "")
        }
        onEcrit: function(cle, ok, erreur) {
            if (!ok)
                messageEtat.texte = qsTr("Le coffre du système a refusé le mot de passe : %1").arg(erreur)
        }
        onEfface: function(cle, ok, erreur) {
            messageEtat.texte = ok
                    ? qsTr("Mot de passe retiré du coffre du système.")
                    : qsTr("Retrait du coffre impossible : %1").arg(erreur)
        }
    }

    // Secrets à confier au coffre une fois la connexion acceptée par le
    // serveur : on n'y range jamais un mot de passe que le serveur a refusé.
    property var secretsAConserver: ({})
    // Comptes dont le mot de passe est à demander, l'un après l'autre.
    property var aDemander: []
    property var essai: null

    Connections {
        target: boite

        function onConnecte(compte, adresse) {
            var secret = fenetre.secretsAConserver[adresse]
            if (secret) {
                coffre.ecrire(secret.cle, secret.motDePasse)
                delete fenetre.secretsAConserver[adresse]
            }
            messageEtat.texte = qsTr("%1 connecté.").arg(adresse)
            if (fenetre.essai && fenetre.essai.connecter2) {
                var e2 = fenetre.essai.connecter2
                fenetre.essai.connecter2 = null
                boite.connecter(e2.hote, e2.adresse, e2.motDePasse)
            }
            // Premier compte prêt : sa boîte de réception est le point de
            // départ naturel.
            if (boite.dossierCourant.length === 0)
                fenetre.ouvrirDossier(compte, "INBOX")
            else if (fenetre.essai && fenetre.essai.scenario) {
                fenetre.etapeScenario()
                if (fenetre.essai.scenario === "glisser")
                    fenetre.etapeScenarioGlisser()
            }
        }

        function onDossierOuvert(compte, chemin, veille) {
            if (compte !== boite.compteCourant || chemin !== boite.dossierCourant)
                return
            fenetre.rafraichirListe()
            if (!veille)
                messageEtat.texte = qsTr("%1 : %n message(s).", "", modeleMessages.count)
                        .arg(fenetre.libelleCourant())
            if (fenetre.essai && fenetre.essai.afficherPremier && modeleMessages.count > 0) {
                fenetre.essai.afficherPremier = false
                fenetre.choisir(0, 0)
            }
            if (fenetre.essai && fenetre.essai.scenario)
                fenetre.etapeScenario()
            if (fenetre.essai && fenetre.essai.scenario === "glisser")
                fenetre.etapeScenarioGlisser()
        }

        function onDrapeauxModifies() {
            fenetre.rafraichirListe()
        }

        function onCorpsRecu(uid, texte, brut, pieces) {
            if (uid !== fenetre.uidCourant || brut !== fenetre.sourceVisible)
                return
            vueCorps.text = texte
            vueCorps.cursorPosition = 0
            if (!brut)
                fenetre.pieces = JSON.parse(pieces)
            if (fenetre.essai && fenetre.essai.scenario === "pieces")
                fenetre.etapeScenarioPieces()
        }

        function onPieceEcrite(url, chemin, ouvrir) {
            if (ouvrir) {
                if (!Qt.openUrlExternally(url))
                    messageEtat.texte = qsTr("Aucun logiciel n'a pu ouvrir %1.").arg(chemin)
                else
                    messageEtat.texte = qsTr("Ouverture de %1…").arg(chemin)
            } else {
                messageEtat.texte = qsTr("Pièce jointe enregistrée : %1").arg(chemin)
            }
            if (fenetre.essai && fenetre.essai.scenario === "pieces")
                console.log("scenario: piece ecrite", chemin, ouvrir)
        }

        function onDeplacementTermine(nombre, erreurs) {
            fenetre.deplacementsEnVol = Math.max(0, fenetre.deplacementsEnVol - 1)
            if (fenetre.deplacementsEnVol === 0)
                fenetre.enDeplacement = ({})
            fenetre.rafraichirListe()
            if (erreurs.length === 0)
                messageEtat.texte = qsTr("%n message(s) déplacé(s).", "", nombre)
            if (fenetre.essai && fenetre.essai.scenario) {
                console.log("scenario: deplacement termine", nombre, erreurs)
                fenetre.etapeScenario()
                if (fenetre.essai.scenario === "glisser")
                    fenetre.etapeScenarioGlisser()
            }
        }

        function onEchec(compte, etape, message) {
            if (etape === "tri") {
                fenetre.deplacementsEnVol = Math.max(0, fenetre.deplacementsEnVol - 1)
                if (fenetre.deplacementsEnVol === 0)
                    fenetre.enDeplacement = ({})
                fenetre.rafraichirListe()
            } else if (etape === "identifiants") {
                // Mot de passe refusé : on redemande, sans effacer d'office
                // celui du coffre — il a pu changer côté serveur.
                var c = fenetre.compteParId(compte)
                fenetre.demanderCompte(c ? c : fenetre.dernierDemande, message)
            } else if (etape === "connexion" && fenetre.dernierDemande
                       && !fenetre.compteParId(compte)) {
                // Premier essai d'un compte nouveau : il n'a pas été conservé,
                // on rouvre le dialogue tel qu'il était.
                fenetre.demanderCompte(fenetre.dernierDemande, message)
            } else if (etape === "message") {
                vueCorps.text = ""
            }
        }
    }

    // ------------------------------------------------------------- logique
    property var dernierDemande: null

    function cle(adresse, hote) {
        return adresse + "@" + hote
    }

    function listeComptes() {
        return JSON.parse(boite.comptes())
    }

    function compteParCle(c) {
        var comptes = listeComptes()
        for (var i = 0; i < comptes.length; ++i)
            if (cle(comptes[i].adresse, comptes[i].hote) === c)
                return comptes[i]
        return null
    }

    function compteParId(id) {
        var comptes = listeComptes()
        for (var i = 0; i < comptes.length; ++i)
            if (comptes[i].id === id)
                return comptes[i]
        return null
    }

    function dialogueOuvert() {
        return dlgCompte.visible || dlgDeplacer.visible || dlgRetrait.visible
    }

    /// Ouvre le dialogue de compte : `compte` nul pour un nouveau compte.
    function demanderCompte(compte, note) {
        if (dlgCompte.visible) {
            if (compte)
                aDemander.push({ compte: compte, note: note })
            return
        }
        dernierDemande = compte
        dlgCompte.compteId = compte && compte.id ? compte.id : 0
        champAdresse.text = compte ? compte.adresse : ""
        champHote.text = compte ? compte.hote : ""
        champMotDePasse.text = ""
        caseMemoriser.checked = reglages.memoriser
        noteCompte.text = note
        dlgCompte.open()
    }

    function demandeSuivante() {
        if (aDemander.length > 0) {
            var suivante = aDemander.shift()
            demanderCompte(suivante.compte, suivante.note)
        }
    }

    function validerCompte() {
        var adresse = champAdresse.text.trim()
        var hote = champHote.text.trim()
        var motDePasse = champMotDePasse.text
        champMotDePasse.text = ""
        reglages.memoriser = caseMemoriser.checked
        dernierDemande = { id: dlgCompte.compteId, adresse: adresse, hote: hote }
        if (caseMemoriser.checked)
            secretsAConserver[adresse] = { cle: cle(adresse, hote), motDePasse: motDePasse }
        else
            coffre.effacer(cle(adresse, hote))
        messageEtat.texte = qsTr("Connexion de %1…").arg(adresse)
        boite.connecter(hote, adresse, motDePasse)
        demandeSuivante()
    }

    function connecterCompte(compte) {
        if (reglages.memoriser)
            coffre.lire(cle(compte.adresse, compte.hote))
        else
            demanderCompte(compte, "")
    }

    // ---- arborescence

    /// Toutes les lignes ont la même forme : un ListModel veut des rôles
    /// constants d'une ligne à l'autre.
    function normaliser(l) {
        return {
            genre: l.genre,
            compte: l.compte || 0,
            adresse: l.adresse || "",
            hote: l.hote || "",
            etat: l.etat || "",
            replie: l.replie === true,
            chemin: l.chemin || "",
            nom: l.nom || "",
            profondeur: l.profondeur || 0,
            role: l.role || "",
            selectionnable: l.selectionnable !== false,
            messages: l.messages || 0,
            nonLus: l.nonLus || 0,
            masque: l.masque === true,
            favori: l.favori === true
        }
    }

    function clefLigne(l) {
        return l.genre + "\u0001" + l.compte + "\u0001" + l.chemin
    }

    /// Relit l'arborescence. Même structure : les lignes sont mises à jour en
    /// place, sans que la vue ne revienne en haut.
    function rafraichirArborescence() {
        var lignes = JSON.parse(boite.arborescence(reglages.afficherMasques)).map(normaliser)
        var memeForme = lignes.length === modeleArborescence.count
        for (var i = 0; memeForme && i < lignes.length; ++i)
            memeForme = clefLigne(lignes[i]) === clefLigne(modeleArborescence.get(i))
        if (memeForme) {
            for (var j = 0; j < lignes.length; ++j)
                modeleArborescence.set(j, lignes[j])
        } else {
            modeleArborescence.clear()
            modeleArborescence.append(lignes)
        }
    }

    function libelleLigne(l) {
        if (l.genre === "rubrique")
            return qsTr("Favoris")
        if (l.genre === "favori-vide")
            return qsTr("Glissez un dossier ici")
        if (l.genre === "compte")
            return l.adresse
        var nom = l.nom
        if (l.role === "Sent") nom = qsTr("Éléments envoyés")
        else if (l.role === "Drafts") nom = qsTr("Brouillons")
        else if (l.role === "Trash") nom = qsTr("Éléments supprimés")
        else if (l.role === "Junk") nom = qsTr("Courrier indésirable")
        else if (l.role === "Archive") nom = qsTr("Archives")
        else if (l.chemin === "INBOX") nom = qsTr("Boîte de réception")
        if (l.genre === "favori")
            return nom + " — " + l.adresse
        return nom
    }

    function libelleCourant() {
        for (var i = 0; i < modeleArborescence.count; ++i) {
            var l = modeleArborescence.get(i)
            if (l.genre === "dossier" && l.compte === boite.compteCourant
                    && l.chemin === boite.dossierCourant)
                return libelleLigne(l)
        }
        return boite.dossierCourant
    }

    // ---- liste des messages

    function ouvrirDossier(compte, chemin) {
        if (compte !== boite.compteCourant || chemin !== boite.dossierCourant) {
            viderListe()
            if (boite.ouvrirDossier(compte, chemin)) {
                // L'index s'affiche tout de suite ; la synchronisation suit.
                rafraichirListe()
                messageEtat.texte = qsTr("Lecture de %1…").arg(libelleCourant())
            }
        } else {
            boite.ouvrirDossier(compte, chemin)
        }
        if (compact)
            vue = 1
    }

    function viderListe() {
        pieces = []
        modeleMessages.clear()
        selection = ({})
        ancre = -1
        uidCourant = 0
        sujetAffiche.text = ""
        auteurAffiche.text = ""
        vueCorps.text = qsTr("Aucun message sélectionné.")
    }

    /// Relit la liste depuis l'index. Mêmes messages dans le même ordre : mise
    /// à jour en place ; sinon reconstruction, en gardant la position.
    function rafraichirListe() {
        var messages = JSON.parse(boite.messages()).filter(function(m) {
            return fenetre.enDeplacement[m.uid] !== true
        })
        var memeForme = messages.length === modeleMessages.count
        for (var i = 0; memeForme && i < messages.length; ++i)
            memeForme = messages[i].uid === modeleMessages.get(i).uid
        if (memeForme) {
            for (var j = 0; j < messages.length; ++j) {
                var avant = modeleMessages.get(j)
                if (avant.lu !== messages[j].lu || avant.repondu !== messages[j].repondu)
                    modeleMessages.set(j, messages[j])
            }
            return
        }
        var position = vueMessages.contentY
        modeleMessages.clear()
        modeleMessages.append(messages)
        vueMessages.contentY = position
        // La sélection ne garde que les messages encore là.
        var presents = {}
        for (var k = 0; k < messages.length; ++k)
            presents[messages[k].uid] = true
        var reste = {}
        for (var uid in selection)
            if (presents[uid])
                reste[uid] = true
        selection = reste
        if (uidCourant > 0 && !presents[uidCourant]) {
            uidCourant = 0
            sujetAffiche.text = ""
            auteurAffiche.text = ""
            vueCorps.text = qsTr("Aucun message sélectionné.")
        }
    }

    function indexDe(uid) {
        for (var i = 0; i < modeleMessages.count; ++i)
            if (modeleMessages.get(i).uid === uid)
                return i
        return -1
    }

    /// Clic sur la ligne `index` : seul (et affiché), ajouté ou retiré (Ctrl),
    /// ou étendu depuis l'ancre (Maj).
    function choisir(index, modificateurs) {
        if (index < 0 || index >= modeleMessages.count)
            return
        var uid = modeleMessages.get(index).uid
        var nouvelle = {}
        if (modificateurs & Qt.ShiftModifier && ancre >= 0) {
            var debut = Math.min(ancre, index), fin = Math.max(ancre, index)
            for (var i = debut; i <= fin; ++i)
                nouvelle[modeleMessages.get(i).uid] = true
        } else if (modificateurs & Qt.ControlModifier) {
            for (var u in selection)
                nouvelle[u] = true
            if (nouvelle[uid])
                delete nouvelle[uid]
            else
                nouvelle[uid] = true
            ancre = index
        } else {
            nouvelle[uid] = true
            ancre = index
        }
        selection = nouvelle
        vueMessages.currentIndex = index
        vueMessages.forceActiveFocus()
        var nombre = Object.keys(nouvelle).length
        if (nombre === 1 && nouvelle[uid])
            afficherMessage(uid)
        else if (nombre > 1)
            messageEtat.texte = qsTr("%n message(s) sélectionné(s).", "", nombre)
    }

    function deplacerCurseur(pas, etendre) {
        var index = Math.max(0, Math.min(modeleMessages.count - 1, vueMessages.currentIndex + pas))
        choisir(index, etendre ? Qt.ShiftModifier : 0)
        vueMessages.positionViewAtIndex(index, ListView.Contain)
    }

    function toutChoisir() {
        var nouvelle = {}
        for (var i = 0; i < modeleMessages.count; ++i)
            nouvelle[modeleMessages.get(i).uid] = true
        selection = nouvelle
    }

    function uidsChoisis() {
        return Object.keys(selection).map(function(u) { return parseInt(u) })
    }

    function afficherMessage(uid) {
        uidCourant = uid
        sourceVisible = false
        pieces = []
        var i = indexDe(uid)
        if (i >= 0) {
            var ligne = modeleMessages.get(i)
            sujetAffiche.text = ligne.sujet
            auteurAffiche.text = ligne.expediteur
                    + (ligne.adresse.length > 0 && ligne.adresse !== ligne.expediteur
                       ? " <" + ligne.adresse + ">" : "")
                    + "  ·  " + dateLongue(ligne.date)
        }
        vueCorps.text = qsTr("Chargement…")
        boite.demanderCorps(uid)
        if (compact)
            vue = 2
    }

    function basculerSource() {
        if (uidCourant <= 0) {
            var uids = uidsChoisis()
            if (uids.length !== 1)
                return
            uidCourant = uids[0]
        }
        sourceVisible = !sourceVisible
        vueCorps.text = qsTr("Chargement…")
        if (sourceVisible)
            boite.demanderSource(uidCourant)
        else
            boite.demanderCorps(uidCourant)
    }

    function marquerSelection(lu) {
        var uids = uidsChoisis()
        if (uids.length > 0 && boite.marquerLu(uids.join(","), lu))
            rafraichirListe()
    }

    function ouvrirDeplacer() {
        if (uidsChoisis().length === 0)
            return
        dlgDeplacer.cibles = JSON.parse(boite.dossiersCibles())
        dlgDeplacer.open()
    }

    /// Déplace la sélection vers un dossier de n'importe quel compte.
    function deplacerVers(compte, chemin) {
        var uids = uidsChoisis()
        if (uids.length === 0)
            return
        if (!boite.deplacer(uids.join(","), compte, chemin))
            return
        retirerDeLaListe(uids)
        messageEtat.texte = qsTr("Déplacement de %n message(s)…", "", uids.length)
    }

    function supprimerSelection() {
        var uids = uidsChoisis()
        if (uids.length === 0)
            return
        if (boite.supprimer(uids.join(",")))
            retirerDeLaListe(uids)
    }

    function retirerDeLaListe(uids) {
        var retires = {}
        for (var u in enDeplacement)
            retires[u] = true
        for (var i = 0; i < uids.length; ++i)
            retires[uids[i]] = true
        enDeplacement = retires
        deplacementsEnVol += 1
        var suivant = -1
        for (var k = modeleMessages.count - 1; k >= 0; --k) {
            if (retires[modeleMessages.get(k).uid]) {
                modeleMessages.remove(k)
                suivant = k
            }
        }
        selection = ({})
        uidCourant = 0
        sujetAffiche.text = ""
        auteurAffiche.text = ""
        vueCorps.text = qsTr("Aucun message sélectionné.")
        // Comme Outlook : le message suivant prend la place.
        if (suivant >= 0 && modeleMessages.count > 0 && !compact)
            choisir(Math.min(suivant, modeleMessages.count - 1), 0)
        else if (compact)
            vue = 1
    }

    function choisirApparence(nom) {
        apparence = nom
        messageEtat.texte = qsTr("Apparence : %1.").arg(jeuxApparence[nom].libelle)
    }

    function ouvrirPiece(piece) {
        if (!piece || piece.risquee)
            return
        if (boite.ouvrirPiece(uidCourant, piece.indice))
            messageEtat.texte = qsTr("Préparation de %1…").arg(piece.nom)
    }

    function enregistrerPiece(piece) {
        if (!piece)
            return
        dlgEnregistrerPiece.piece = piece
        dlgEnregistrerPiece.uid = uidCourant
        dlgEnregistrerPiece.currentFolder = dossierTelechargements
        dlgEnregistrerPiece.selectedFile = dossierTelechargements + "/" + encodeURIComponent(piece.nom)
        dlgEnregistrerPiece.open()
    }

    // 1536 → « 1,5 Ko ».
    function tailleLisible(octets) {
        if (octets < 1024)
            return qsTr("%1 o").arg(octets)
        if (octets < 1024 * 1024)
            return qsTr("%1 Ko").arg((octets / 1024).toLocaleString(Qt.locale("fr_FR"), "f", 1))
        return qsTr("%1 Mo").arg((octets / 1024 / 1024).toLocaleString(Qt.locale("fr_FR"), "f", 1))
    }

    // « 2026-09-16T18:00:00+02:00 » → « 18:00 » aujourd'hui, « 16/09 18:00 »
    // cette année, « 16/09/2025 » avant. Les dates que le serveur n'a pas su
    // rendre en ISO sont affichées telles quelles.
    function dateCourte(date) {
        var d = new Date(date)
        if (isNaN(d.getTime()))
            return date
        function deux(n) { return (n < 10 ? "0" : "") + n }
        var maintenant = new Date()
        var heure = deux(d.getHours()) + ":" + deux(d.getMinutes())
        if (d.toDateString() === maintenant.toDateString())
            return heure
        var jour = deux(d.getDate()) + "/" + deux(d.getMonth() + 1)
        if (d.getFullYear() === maintenant.getFullYear())
            return jour + " " + heure
        return jour + "/" + d.getFullYear()
    }

    // « mer. 16/09/2026 14:54 », indépendamment de la langue du système.
    function dateLongue(date) {
        var d = new Date(date)
        if (isNaN(d.getTime()))
            return date
        function deux(n) { return (n < 10 ? "0" : "") + n }
        var jours = ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."]
        return jours[d.getDay()] + " " + deux(d.getDate()) + "/" + deux(d.getMonth() + 1) + "/"
                + d.getFullYear() + " " + deux(d.getHours()) + ":" + deux(d.getMinutes())
    }

    onClosing: function(fermeture) {
        // Sur téléphone, le retour arrière remonte d'un volet avant de quitter.
        if (compact && vue > 0) {
            fermeture.accepted = false
            vue = vue - 1
        }
    }

    Component.onCompleted: {
        if (typeof modeControle !== "undefined" && modeControle)
            return
        if (!boite.ouvrirProfil(cheminProfil))
            return
        rafraichirArborescence()
        // Session ouverte d'emblée quand l'environnement porte des identifiants
        // (cf. cpp/main.cpp) : sert aux captures et aux essais, jamais en usage
        // courant. Le coffre n'est alors pas touché.
        if (typeof identifiantsEssai !== "undefined" && identifiantsEssai
                && identifiantsEssai.hote) {
            essai = { afficherPremier: !identifiantsEssai.scenario
                                       || identifiantsEssai.scenario === "pieces",
                      connecter2: null, scenario: identifiantsEssai.scenario || "",
                      etape: 0, sujet: "", examines: 0 }
            if (identifiantsEssai.utilisateur2)
                essai.connecter2 = { hote: identifiantsEssai.hote,
                                     adresse: identifiantsEssai.utilisateur2,
                                     motDePasse: identifiantsEssai.motDePasse2 }
            boite.connecter(identifiantsEssai.hote, identifiantsEssai.utilisateur,
                            identifiantsEssai.motDePasse)
            return
        }
        // Comptes connus : l'index local s'affiche tout de suite, chaque
        // connexion suit dès que le coffre a répondu.
        var comptes = listeComptes()
        if (comptes.length === 0) {
            demanderCompte(null, "")
            return
        }
        for (var i = 0; i < comptes.length; ++i)
            connecterCompte(comptes[i])
    }

    /// Scénario d'essai « aller-retour » (MMAIL_SCENARIO) : le premier message
    /// de la boîte de réception du premier compte part dans celle du second par
    /// les fonctions mêmes de l'interface, puis revient. Chaque étape est tracée
    /// sur la console ; sert à éprouver le tri sans personne devant l'écran.
    function etapeScenario() {
        if (!essai || essai.scenario !== "aller-retour")
            return
        var comptes = listeComptes()
        if (comptes.length < 2 || comptes[0].etat !== "pret" || comptes[1].etat !== "pret")
            return
        var a = comptes[0], b = comptes[1]
        if (essai.etape === 0 && boite.compteCourant === a.id && modeleMessages.count > 0) {
            essai.etape = 1
            essai.sujet = modeleMessages.get(0).sujet
            console.log("scenario: aller", essai.sujet, "de", a.adresse, "vers", b.adresse)
            choisir(0, 0)
            deplacerVers(b.id, "INBOX")
        } else if (essai.etape === 1 && boite.compteCourant === a.id && deplacementsEnVol === 0) {
            essai.etape = 2
            ouvrirDossier(b.id, "INBOX")
        } else if (essai.etape === 2 && boite.compteCourant === b.id) {
            for (var i = 0; i < modeleMessages.count; ++i) {
                if (modeleMessages.get(i).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: trouve dans", b.adresse, "- retour")
                    choisir(i, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC, message absent de", b.adresse)
            essai.etape = 9
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 4
            ouvrirDossier(a.id, "INBOX")
        } else if (essai.etape === 4 && boite.compteCourant === a.id) {
            var revenu = false
            for (var j = 0; j < modeleMessages.count; ++j)
                revenu = revenu || modeleMessages.get(j).sujet === essai.sujet
            console.log(revenu ? "scenario: OK, message revenu dans " + a.adresse
                               : "scenario: ECHEC, message non revenu")
            essai.etape = 9
        }
    }

    /// Scénario d'essai « pieces » (MMAIL_SCENARIO) : dans la boîte de
    /// réception du premier compte, le premier message portant une pièce
    /// jointe est affiché, et sa première pièce enregistrée dans le dossier
    /// désigné par MMAIL_SORTIE. Sert à éprouver la chaîne sans écran.
    function etapeScenarioPieces() {
        if (essai.etape === 0 && pieces.length > 0) {
            essai.etape = 1
            console.log("scenario: pieces", JSON.stringify(pieces))
            boite.enregistrerPiece(uidCourant, pieces[0].indice,
                                   identifiantsEssai.sortie + "/" + encodeURIComponent(pieces[0].nom))
        } else if (essai.etape === 0 && essai.examines < modeleMessages.count - 1) {
            essai.examines += 1
            choisir(essai.examines, 0)
        }
    }

    // Sonde du scénario « glisser » : un objet de glisser-déposer Qt Quick,
    // déposé par programme sur une ligne de l'arborescence. Les zones de dépôt
    // le reçoivent exactement comme ce qu'on glisse à la souris.
    Item {
        id: sonde
        property int compte: 0
        property string chemin: ""
        width: 4; height: 4
        Drag.hotSpot.x: 2
        Drag.hotSpot.y: 2
    }

    /// Dépose la sonde sur la ligne `index` de l'arborescence.
    function deposerSonde(index, clef, compte, chemin) {
        vueArborescence.positionViewAtIndex(index, ListView.Contain)
        var ligne = vueArborescence.itemAtIndex(index)
        if (!ligne)
            return false
        var point = ligne.mapToItem(fenetre.contentItem, 20, ligne.height / 2)
        sonde.parent = fenetre.contentItem
        sonde.compte = compte
        sonde.chemin = chemin
        sonde.Drag.keys = [clef]
        sonde.x = point.x - 2
        sonde.y = point.y - 2
        sonde.Drag.active = true
        var action = sonde.Drag.drop()
        sonde.Drag.active = false
        return action !== Qt.IgnoreAction
    }

    function indexLigne(genre, compte, chemin) {
        for (var i = 0; i < modeleArborescence.count; ++i) {
            var l = modeleArborescence.get(i)
            if (l.genre === genre && (compte === 0 || l.compte === compte) && (chemin === "" || l.chemin === chemin))
                return i
        }
        return -1
    }

    function favorisActuels() {
        return JSON.parse(boite.arborescence(false))
                .filter(function(l) { return l.genre === "favori" })
                .map(function(l) { return l.adresse + ":" + l.chemin })
    }

    /// Scénario « glisser » (MMAIL_SCENARIO) : deux dossiers déposés sur la
    /// rubrique Favoris puis l'un sur l'autre, et un message déposé sur un
    /// dossier puis ramené. Chaque étape est tracée ; les favoris sont retirés
    /// à la fin.
    function etapeScenarioGlisser() {
        var comptes = listeComptes()
        if (comptes.length < 2 || comptes[0].etat !== "pret" || comptes[1].etat !== "pret")
            return
        var a = comptes[0], b = comptes[1]
        if (essai.etape === 0 && boite.compteCourant === a.id && modeleMessages.count > 0) {
            essai.etape = 1
            var ok1 = deposerSonde(indexLigne("rubrique", 0, ""), "mmail/dossier", a.id, "Essais")
            var ok2 = deposerSonde(indexLigne("favori", a.id, "Essais"), "mmail/dossier", b.id, "INBOX")
            var ordre = favorisActuels()
            console.log("scenario: favoris", ok1, ok2, JSON.stringify(ordre))
            console.log(ordre.length === 2 && ordre[0] === b.adresse + ":INBOX" && ordre[1] === a.adresse + ":Essais"
                        ? "scenario: OK favoris ordonnes" : "scenario: ECHEC favoris")
            // Un dossier déposé sur un dossier ordinaire ne fait rien.
            var refus = deposerSonde(indexLigne("dossier", a.id, "Archive"), "mmail/dossier", a.id, "Essais")
            console.log(refus ? "scenario: ECHEC dossier accepte hors Favoris" : "scenario: OK dossier refuse hors Favoris")
            // Un message déposé sur un dossier y part.
            essai.sujet = modeleMessages.get(0).sujet
            choisir(0, 0)
            var ok3 = deposerSonde(indexLigne("dossier", a.id, "Essais/Clients"), "mmail/messages", 0, "")
            console.log("scenario: message depose", ok3, essai.sujet)
        } else if (essai.etape === 1 && deplacementsEnVol === 0) {
            essai.etape = 2
            ouvrirDossier(a.id, "Essais/Clients")
        } else if (essai.etape === 2 && boite.dossierCourant === "Essais/Clients") {
            for (var i = 0; i < modeleMessages.count; ++i) {
                if (modeleMessages.get(i).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: OK message arrive dans Essais/Clients")
                    choisir(i, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC message absent de Essais/Clients")
            essai.etape = 9
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 9
            boite.epinglerDossier(a.id, "Essais", false)
            boite.epinglerDossier(b.id, "INBOX", false)
            console.log("scenario: message ramene, favoris retires :", JSON.stringify(favorisActuels()))
        }
    }

    /// Contrôle de fabrication : l'index local s'ouvre, s'écrit et se relit,
    /// sans toucher au réseau ; le coffre répond. Appelé par --smoke.
    function controleIndex() {
        if (!boite.ouvrirProfil(cheminProfil))
            return "profil : " + boite.erreur
        var arborescence = JSON.parse(boite.arborescence(true))
        if (!Array.isArray(arborescence))
            return "arborescence illisible"
        rafraichirArborescence()
        if (JSON.parse(boite.messages()).length !== 0)
            return "aucun dossier ouvert, donc aucun message"
        if (!Array.isArray(JSON.parse(boite.dossiersCibles())))
            return "cibles illisibles"
        if (coffre.service !== "MMail")
            return "coffre absent"
        // Un appel sans session ne doit ni bloquer ni réussir.
        if (boite.ouvrirDossier(0, "INBOX"))
            return "une commande a été acceptée sans compte"
        if (boite.deplacer("1", 0, "INBOX"))
            return "un déplacement a été accepté sans dossier ouvert"
        if (boite.ouvrirPiece(1, 0))
            return "une pièce jointe a été demandée sans dossier ouvert"
        // La rubrique Favoris est toujours là, même vide.
        if (arborescence.length === 0 || arborescence[0].genre !== "rubrique")
            return "rubrique Favoris absente"
        // Apparences : la palette suit le choix, et « systeme » la rend.
        var fondSysteme = palette.window.toString()
        var apparenceAvant = apparence
        choisirApparence("classique")
        if (palette.window.toString() !== "#d4d0c8")
            return "apparence classique non appliquée : " + palette.window
        choisirApparence("moderne")
        if (palette.window.toString() !== "#f6f7f8")
            return "apparence moderne non appliquée : " + palette.window
        choisirApparence("systeme")
        if (palette.window.toString() !== fondSysteme && apparenceAvant === "systeme")
            return "apparence système non rendue"
        choisirApparence(apparenceAvant)
        // Zoom borné, et propre à chaque colonne.
        var zoomAvant = [reglages.zoomArborescence, reglages.zoomListe, reglages.zoomMessage]
        if (poserZoom("liste", 10) !== zoomMaximum || poserZoom("liste", 0.01) !== zoomMinimum)
            return "zoom non borné"
        poserZoom("message", 2)
        if (reglages.zoomListe !== zoomMinimum || tailleMessage !== tailleBase * 2)
            return "zooms mêlés entre colonnes"
        reglages.zoomArborescence = zoomAvant[0]
        reglages.zoomListe = zoomAvant[1]
        reglages.zoomMessage = zoomAvant[2]
        return "ok"
    }
}
