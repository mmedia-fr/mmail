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
import QtQuick.Window
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
    // Envois différés : chaque minute, les messages arrivés à échéance partent.
    Timer {
        interval: 60000
        running: true
        repeat: true
        onTriggered: boite.envoyerDifferes()
    }
    Boite { id: boite }
    // Configuration automatique du serveur, et lien de configuration.
    Configuration {
        id: configuration
        onServeurDecouvert: function(adresse, hote) { fenetre.serveurDecouvert(adresse, hote) }
        onLienLu: function(contenu) { fenetre.appliquerLien(contenu) }
        onLienEchoue: function(message) {
            var lien = fenetre.lienEnCours
            fenetre.lienEnCours = ""
            dlgLien.ouvrir(lien, qsTr("Ce lien n'a pas pu être lu — %1").arg(message))
        }
    }
    PressePapier { id: pressePapier }
    ReglePressePapier { id: reglePresse }

    Settings {
        id: reglagesEdition
        category: "edition"
        // Comme dans MMdedit : toute sélection faite dans un message part au
        // presse-papier, sauf ce qu'un autre logiciel vient d'y déposer.
        property bool copieAuto: true
    }
    readonly property string noyau: socle.noyau
    // Comptes du profil, pour le menu « Comptes » : { compte, adresse, hote }.
    property var comptesConnus: []
    // Lien de configuration en cours de lecture, rendu au dialogue s'il échoue.
    property string lienEnCours: ""
    // Rôle SPECIAL-USE du dossier ouvert : « Drafts » change le double clic.
    property string roleCourant: ""
    // Dossier dont les messages se reprennent plutôt qu'on n'y répond :
    // brouillons, envois différés.
    readonly property bool dossierDeReprise: roleCourant === "Drafts" || roleCourant === "Differe"
    // Importance du message affiché, et adresse qui demande une confirmation
    // de lecture (vide si aucune, ou déjà répondu).
    property int importanceCourante: 0
    property string confirmationDemandee: ""
    // Rédactions ouvertes : jeton → contenu de la fenêtre de rédaction.
    property int compteurRedactions: 0
    property var redactions: ({})

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
    // Cumul des messages effacés par « Vider les corbeilles » (un compte par
    // signal), remis à zéro à chaque lancement.
    property int messagesVides: 0
    // Pièces jointes du message affiché, telles que le noyau les rend.
    property var pieces: []
    // Le message affiché est du HTML assaini par le noyau, rendu en texte
    // riche ; `htmlAffiche` le garde tel quel, avant la mise à l'échelle des
    // polices par le zoom de la colonne.
    property bool corpsHtml: false
    property string htmlAffiche: ""
    // Images distantes du message affiché laissées de côté.
    property int imagesBloquees: 0
    // Lien survolé dans le message : son adresse s'affiche dans la barre
    // d'information, comme dans un navigateur — un lien trompeur s'y voit.
    property string lienSurvole: ""
    // Fond de la rubrique Favoris : un cran plus soutenu que celui des
    // comptes, pour qu'elle s'en détache (retour de Manu, 01/10).
    readonly property color fondFavoris: palette.base.hslLightness > 0.5
            ? Qt.tint(Qt.darker(palette.base, 1.07), Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.10))
            : Qt.tint(palette.base, Qt.rgba(1, 1, 1, 0.08))

    // Rédaction : identité et signature de chaque compte, en JSON
    // (adresse → {nom, signature, nouveaux, reponses}) ; mise en forme par défaut.
    Settings {
        id: reglagesRedaction
        category: "redaction"
        property string identites: "{}"
        property bool miseEnForme: true
    }

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
            police: ["MS Shell Dlg 2", "Tahoma", "Microsoft Sans Serif", "Arial"], taille: 9,
            mono: ["Courier New", "Liberation Mono", "DejaVu Sans Mono"]
        },
        "moderne": {
            libelle: qsTr("Moderne (M-Media)"),
            fond: "#f6f7f8", texte: "#231f20", base: "#ffffff",
            bouton: "#ffffff", surbrillance: "#21abe3", texteSurbrillance: "#ffffff",
            police: ["Segoe UI", "Noto Sans", "DejaVu Sans", "Arial"], taille: 10,
            mono: ["Cascadia Mono", "Consolas", "DejaVu Sans Mono", "Courier New"]
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
            value: fenetre.jeu ? fenetre.valeurJeu(modelData.clef) : ""
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
    readonly property string policeMono: premierePolice(jeu ? jeu.mono
                                                            : ["Consolas", "DejaVu Sans Mono", "Courier New"])

    // Polices installées, relevées une fois : une police demandée qui manque
    // cède la place à la suivante de sa liste, plutôt qu'au choix arbitraire
    // de Qt. « MS Shell Dlg 2 », par exemple, n'est qu'un alias de Windows que
    // la base de polices de Qt 6.8 ne connaît pas.
    readonly property var famillesInstallees: Qt.fontFamilies()

    function premierePolice(liste) {
        if (typeof liste === "string")
            return liste
        for (var i = 0; i < liste.length; ++i)
            if (famillesInstallees.indexOf(liste[i]) >= 0)
                return liste[i]
        return liste.length > 0 ? liste[liste.length - 1] : ""
    }

    function valeurJeu(clef) {
        var v = jeu[clef]
        return Array.isArray(v) ? premierePolice(v) : v
    }

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
        function onRevisionChanged() {
            fenetre.rafraichirArborescence()
            if (!boite.occupe)
                fenetre.derniereSynchro = new Date()
        }
        function onDossierCourantChanged() {
            fenetre.roleCourant = boite.roleCourant()
            fenetre.majInfoDossier()
        }
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
                text: qsTr("Nouveau message")
                font.bold: true
                onClicked: fenetre.rediger("nouveau")
            }
            ToolButton {
                id: boutonComptes
                text: qsTr("Comptes")
                visible: !fenetre.compact || fenetre.vue === 0
                onClicked: menuComptes.popup(boutonComptes, 0, boutonComptes.height)
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
            ToolButton {
                id: boutonAide
                text: "?"
                font.bold: true
                onClicked: menuAide.popup(boutonAide, 0, boutonAide.height)
            }
            Label {
                text: fenetre.compact && fenetre.vue > 0 ? fenetre.libelleCourant() : ""
                elide: Text.ElideRight
                font.bold: true
                Layout.fillWidth: true
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
            // Ascenseurs toujours visibles dès qu'il y a de quoi défiler, et non
            // seulement pendant le défilement (demande Manu du 2026-09-29). Leur
            // place est réservée : posés par-dessus, ils masquaient les compteurs
            // de non-lus. La réserve suit `size`, pas `visible` : un ascenseur
            // qui n'a rien à faire défiler reste `visible`, seulement transparent.
            ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
            rightPadding: ScrollBar.vertical.size < 1 ? ScrollBar.vertical.width : 0
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

                // Un blanc et un trait au-dessus de chaque compte : la rubrique
                // Favoris et les boîtes ne se lisent plus comme une seule liste.
                section.property: "groupe"
                section.delegate: Item {
                    required property string section
                    width: vueArborescence.width
                    height: section === "favoris" ? 0 : 16
                    Rectangle {
                        visible: parent.section !== "favoris"
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.leftMargin: 6
                        anchors.rightMargin: 6
                        anchors.verticalCenter: parent.verticalCenter
                        height: 2
                        color: fenetre.palette.windowText
                        opacity: 0.3
                    }
                }

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
            ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
            rightPadding: ScrollBar.vertical.size < 1 ? ScrollBar.vertical.width : 0
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
                leftPadding: 8
                rightPadding: 8
                // Les actions sur leur ligne, au-dessus de l'objet, comme dans
                // le volet de lecture d'Outlook : à côté, elles tronquaient
                // l'objet et l'expéditeur. Elles passent à la ligne si la
                // colonne est étroite.
                ColumnLayout {
                    width: parent.width
                    spacing: 0
                    // De vrais boutons, encadrés et espacés, en trois groupes —
                    // répondre, ranger, voir la source — : des boutons plats de
                    // barre d'outils se lisaient mal (retour de Manu, 01/10).
                    Flow {
                        Layout.fillWidth: true
                        Layout.topMargin: 4
                        Layout.bottomMargin: 6
                        spacing: 6
                        // Un brouillon se reprend ; un message reçu reçoit une réponse.
                        Button {
                            visible: fenetre.dossierDeReprise
                            text: qsTr("Reprendre")
                            onClicked: fenetre.rediger("brouillon")
                        }
                        Button {
                            visible: !fenetre.dossierDeReprise
                            text: qsTr("Répondre")
                            onClicked: fenetre.rediger("repondre")
                        }
                        Button {
                            visible: !fenetre.dossierDeReprise
                            text: qsTr("Répondre à tous")
                            onClicked: fenetre.rediger("repondre_tous")
                        }
                        Button {
                            visible: !fenetre.dossierDeReprise
                            text: qsTr("Transférer")
                            onClicked: fenetre.rediger("transferer")
                        }
                        ToolSeparator { height: boutonDeplacer.height }
                        Button {
                            id: boutonDeplacer
                            text: qsTr("Déplacer…")
                            onClicked: fenetre.ouvrirDeplacer()
                        }
                        Button {
                            visible: !fenetre.dossierDeReprise
                            text: fenetre.roleCourant === "Junk" ? qsTr("Pas indésirable") : qsTr("Indésirable")
                            onClicked: fenetre.signalerIndesirable()
                            ToolTip.visible: hovered
                            ToolTip.delay: 600
                            ToolTip.text: fenetre.roleCourant === "Junk"
                                          ? qsTr("Remettre en boîte de réception ; le filtre du serveur apprend que le message est légitime (Ctrl+Alt+J).")
                                          : qsTr("Classer en courrier indésirable ; le filtre du serveur l'apprend (Ctrl+Alt+J).")
                        }
                        Button {
                            text: qsTr("Supprimer")
                            onClicked: fenetre.supprimerSelection()
                        }
                        ToolSeparator { height: boutonDeplacer.height }
                        // Décision 6 : voir le message brut, en-têtes compris, en un geste.
                        Button {
                            text: fenetre.sourceVisible ? qsTr("Message") : qsTr("Source")
                            onClicked: fenetre.basculerSource()
                        }
                    }
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
                    Label {
                        visible: fenetre.uidCourant > 0 && fenetre.importanceCourante !== 0
                        text: fenetre.importanceCourante > 0 ? qsTr("Importance haute") : qsTr("Importance basse")
                        color: fenetre.importanceCourante > 0 ? "#c42b1c" : "#1a4480"
                        font.bold: true
                    }
                }
            }

            // L'expéditeur demande une confirmation de lecture : on la propose,
            // sans l'envoyer d'office — comme Outlook.
            Pane {
                Layout.fillWidth: true
                visible: fenetre.uidCourant > 0 && !fenetre.sourceVisible && fenetre.confirmationDemandee.length > 0
                padding: 4
                background: Rectangle { color: "#fff8e1" }
                RowLayout {
                    width: parent.width
                    Label {
                        text: qsTr("L'expéditeur demande une confirmation de lecture (à %1).").arg(fenetre.confirmationDemandee)
                        color: "#1c1f24"
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Button {
                        text: qsTr("Envoyer")
                        onClicked: {
                            boite.repondreConfirmation(fenetre.uidCourant, true)
                            fenetre.confirmationDemandee = ""
                            messageEtat.texte = qsTr("Confirmation de lecture envoyée.")
                        }
                    }
                    Button {
                        text: qsTr("Ignorer")
                        onClicked: {
                            boite.repondreConfirmation(fenetre.uidCourant, false)
                            fenetre.confirmationDemandee = ""
                        }
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

            // Images distantes : rien ne se télécharge sans le demander — une
            // image distante dit à l'expéditeur que le message a été ouvert.
            Pane {
                Layout.fillWidth: true
                visible: fenetre.uidCourant > 0 && !fenetre.sourceVisible && fenetre.corpsHtml
                         && fenetre.imagesBloquees > 0
                font.pointSize: fenetre.tailleMessage
                padding: 4
                background: Rectangle { color: "#e8f0fb" }
                RowLayout {
                    width: parent.width
                    Label {
                        text: (fenetre.imagesBloquees > 1
                               ? qsTr("%1 images distantes non téléchargées").arg(fenetre.imagesBloquees)
                               : qsTr("1 image distante non téléchargée"))
                              + qsTr(" : l'expéditeur saurait que vous avez ouvert le message.")
                        color: "#1c1f24"
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Button {
                        text: qsTr("Télécharger les images")
                        onClicked: {
                            messageEtat.texte = qsTr("Téléchargement des images…")
                            boite.afficherImages(fenetre.uidCourant)
                        }
                    }
                }
            }

            ScrollView {
                id: cadreCorps
                Layout.fillWidth: true
                Layout.fillHeight: true
                ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                // Réserve fixe : le texte se replie, sa hauteur dépend donc de la
                // largeur, et une réserve qui suivrait le débordement bouclerait.
                rightPadding: ScrollBar.vertical.width
                // La source n'est pas repliée : elle défile aussi en largeur.
                ScrollBar.horizontal.policy: ScrollBar.horizontal.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                bottomPadding: fenetre.sourceVisible ? ScrollBar.horizontal.height : 0
                clip: true
                contentWidth: fenetre.sourceVisible ? -1 : availableWidth

                TextArea {
                    id: vueCorps
                    readOnly: true
                    selectByMouse: true
                    // Un message HTML en texte riche ; tout le reste — source,
                    // texte brut, attente — en texte simple, où une balise
                    // reste une balise.
                    textFormat: fenetre.corpsHtml && !fenetre.sourceVisible ? TextEdit.RichText : TextEdit.PlainText
                    wrapMode: fenetre.sourceVisible ? TextEdit.NoWrap : TextEdit.Wrap
                    font.pointSize: fenetre.tailleMessage
                    font.family: fenetre.sourceVisible ? fenetre.policeMono : fenetre.font.family
                    // Un message HTML se lit sur fond blanc, quelle que soit
                    // l'apparence : ses couleurs supposent ce fond.
                    color: fenetre.corpsHtml && !fenetre.sourceVisible ? "#1f1f1f" : fenetre.palette.text
                    background: Rectangle {
                        color: fenetre.corpsHtml && !fenetre.sourceVisible ? "#ffffff" : fenetre.palette.base
                    }
                    text: qsTr("Aucun message sélectionné.")

                    onLinkActivated: function(lien) { fenetre.ouvrirLien(lien) }
                    onLinkHovered: function(lien) { fenetre.lienSurvole = lien }
                    HoverHandler {
                        cursorShape: vueCorps.hoveredLink.length > 0 ? Qt.PointingHandCursor : Qt.IBeamCursor
                    }

                    onSelectedTextChanged: if (selectedText.length > 0) fenetre.selectionChangee()
                    // Un Ctrl+C explicite écrit par la même règle que la copie
                    // automatique : sans cela, la règle y verrait un dépôt venu
                    // d'ailleurs, et se protégerait contre MMail lui-même.
                    Keys.onPressed: function(touche) {
                        if (touche.matches(StandardKey.Copy) && selectedText.length > 0) {
                            fenetre.copierTexte(selectedText, fenetre.sourceVisible ? "source" : "message")
                            touche.accepted = true
                        }
                    }
                    TapHandler {
                        acceptedButtons: Qt.RightButton
                        onTapped: menuCorps.popup()
                    }
                    onPressAndHold: if (fenetre.compact) menuCorps.popup()

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

    // ------------------------------------------------------ barre d'information
    //
    // À gauche, ce qui est ouvert et ce qu'il contient ; au milieu, le dernier
    // message de l'application, qui s'efface de lui-même ; à droite, l'état du
    // presse-papier, des déplacements et de la synchronisation.
    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            spacing: 14
            Label {
                text: fenetre.lienSurvole.length > 0 ? fenetre.lienSurvole : fenetre.infoDossier
                color: fenetre.lienSurvole.length > 0 ? fenetre.palette.highlight : fenetre.palette.windowText
                elide: Text.ElideRight
                maximumLineCount: 1
                Layout.fillWidth: true
                Layout.minimumWidth: 120
            }
            Label {
                visible: text.length > 0
                text: fenetre.passager
                color: fenetre.passagerErreur ? "#b00020" : palette.windowText
                elide: Text.ElideRight
                maximumLineCount: 1
                Layout.maximumWidth: fenetre.width * 0.4
            }
            Label {
                visible: reglePresse.restant > 0
                text: qsTr("presse-papier protégé %1 s").arg(reglePresse.restant)
                opacity: 0.75
            }
            Label {
                visible: boite.enAttente > 0
                text: fenetre.accord(boite.enAttente, qsTr("déplacement en attente"), qsTr("déplacements en attente"))
            }
            Label {
                text: fenetre.etatSynchro()
                opacity: 0.75
            }
        }
    }

    // Message passager : il s'efface au bout de quelques secondes — plus
    // longtemps pour une erreur.
    property string passager: ""
    property bool passagerErreur: false
    property string infoDossier: ""
    property var derniereSynchro: null

    Timer {
        id: minuteurPassager
        onTriggered: fenetre.passager = ""
    }

    function annoncer(texte, erreur) {
        if (!texte || texte.length === 0)
            return
        passager = texte
        passagerErreur = erreur
        minuteurPassager.interval = erreur ? 12000 : 6000
        minuteurPassager.restart()
    }

    Connections {
        target: boite
        function onErreurChanged() {
            if (boite.erreur.length > 0)
                fenetre.annoncer(boite.erreur, true)
        }
    }

    /// « 1 message », « 3 messages » : qsTr ne sait accorder qu'avec un
    /// fichier de traduction, que l'application n'a pas (elle est en français).
    function accord(n, singulier, pluriel) {
        return n + " " + (n > 1 ? pluriel : singulier)
    }

    function etatSynchro() {
        if (boite.occupe)
            return qsTr("Synchronisation…")
        if (!derniereSynchro)
            return qsTr("Hors ligne")
        function deux(n) { return (n < 10 ? "0" : "") + n }
        return qsTr("À jour à %1").arg(deux(derniereSynchro.getHours()) + ":" + deux(derniereSynchro.getMinutes()))
    }

    /// Compte et dossier ouverts, leurs compteurs, et la sélection s'il y en a
    /// plusieurs.
    function majInfoDossier() {
        if (boite.dossierCourant.length === 0) {
            var n = listeComptes().length
            infoDossier = n === 0 ? qsTr("Aucun compte")
                                  : accord(n, qsTr("compte"), qsTr("comptes")) + qsTr(", aucun dossier ouvert")
            return
        }
        var ligne = null
        for (var i = 0; i < modeleArborescence.count; ++i) {
            var l = modeleArborescence.get(i)
            if (l.genre === "dossier" && l.compte === boite.compteCourant && l.chemin === boite.dossierCourant) {
                ligne = l
                break
            }
        }
        var texte = ligne ? ligne.adresse + " › " + libelleLigne(ligne) : boite.dossierCourant
        var total = ligne ? ligne.messages : modeleMessages.count
        texte += "  —  " + accord(total, qsTr("message"), qsTr("messages"))
        if (ligne && ligne.nonLus > 0)
            texte += ", " + accord(ligne.nonLus, qsTr("non lu"), qsTr("non lus"))
        var choisis = Object.keys(selection).length
        if (choisis > 1)
            texte += "  —  " + accord(choisis, qsTr("sélectionné"), qsTr("sélectionnés"))
        infoDossier = texte
    }

    onSelectionChanged: majInfoDossier()

    // ------------------------------------------------------- presse-papier
    Timer {
        id: minuteurSelection
        interval: 300
        onTriggered: fenetre.copierSelectionAuto()
    }
    Timer {
        interval: 1000
        repeat: true
        running: reglePresse.restant > 0
        onTriggered: reglePresse.rafraichir(fenetre.maintenant())
    }
    Connections {
        target: pressePapier
        function onChange() {
            reglePresse.depot(pressePapier.texte, fenetre.maintenant())
        }
    }

    // Heure en secondes, telle que l'attend la règle du presse-papier.
    function maintenant() {
        return Date.now() / 1000
    }

    function selectionChangee() {
        if (reglagesEdition.copieAuto)
            minuteurSelection.restart()
    }

    /// Copie automatique : la sélection du message part au presse-papier une
    /// fois stabilisée, sauf si un autre logiciel vient d'y déposer quelque
    /// chose — ce contenu-là est protégé une minute.
    function copierSelectionAuto() {
        var texte = vueCorps.selectedText
        if (!reglagesEdition.copieAuto || texte.length === 0)
            return
        if (!reglePresse.peutEcraser(maintenant())) {
            annoncer(qsTr("Presse-papier protégé : la sélection n'est pas copiée."), false)
            return
        }
        copierTexte(texte, sourceVisible ? "source" : "message")
    }

    /// Copie explicite, qui passe toujours : c'est l'utilisateur qui la demande.
    function copierTexte(texte, origine) {
        if (!texte || texte.length === 0)
            return
        // La règle apprend d'abord que l'écriture vient de nous : sous Linux,
        // le presse-papier signale le dépôt pendant même l'écriture, et la
        // règle y verrait sinon un dépôt étranger — MMail se protégerait
        // alors une minute contre sa propre copie.
        reglePresse.ecriture(texte, origine, maintenant())
        pressePapier.deposer(texte)
        annoncer(qsTr("Copié : %1.").arg(accord(texte.length, qsTr("caractère"), qsTr("caractères"))), false)
    }

    function copierChamp(champ) {
        var uids = uidsChoisis()
        if (uids.length !== 1)
            return
        var i = indexDe(uids[0])
        if (i >= 0)
            copierTexte(modeleMessages.get(i)[champ], "message")
    }

    // Les messages de l'application passent par la barre d'information.
    QtObject {
        id: messageEtat
        property string texte: ""
        onTexteChanged: fenetre.annoncer(texte, false)
    }

    // ------------------------------------------------------------- rédaction
    Component {
        id: composantFenetreRedaction
        // ApplicationWindow et non Window : c'est elle qui transmet police et
        // palette aux éléments flottants (listes, menus, dialogues) — une
        // simple Window les laisse aux valeurs par défaut du système.
        ApplicationWindow {
            id: fenetreRedaction
            font: fenetre.font
            palette: fenetre.palette
            width: Math.min(900, Screen.desktopAvailableWidth - 40)
            height: Math.min(700, Screen.desktopAvailableHeight - 60)
            minimumWidth: 480
            minimumHeight: 360
            // Fenêtre indépendante, qui peut passer derrière la principale.
            transientParent: null
            visible: true
            color: fenetre.palette.window
            title: redaction ? redaction.titre : ""
            property alias redaction: chargeurFenetre.item
            function fermerRedaction() {
                redaction.fermetureConfirmee = true
                fenetreRedaction.close()
            }
            Loader {
                id: chargeurFenetre
                anchors.fill: parent
                sourceComponent: composantRedaction
                onLoaded: item.conteneur = fenetreRedaction
            }
            onClosing: function(fermeture) {
                if (redaction && redaction.modifie && !redaction.fermetureConfirmee) {
                    fermeture.accepted = false
                    redaction.demanderFermeture()
                    return
                }
                fenetre.oublierRedaction(redaction ? redaction.jeton : "")
                Qt.callLater(function() { fenetreRedaction.destroy() })
            }
        }
    }

    Component {
        id: composantVoletRedaction
        Popup {
            id: voletRedaction
            parent: Overlay.overlay
            x: 0
            y: 0
            width: parent ? parent.width : 0
            height: parent ? parent.height : 0
            modal: true
            padding: 0
            closePolicy: Popup.NoAutoClose
            property alias redaction: chargeurVolet.item
            function fermerRedaction() {
                redaction.fermetureConfirmee = true
                voletRedaction.close()
            }
            Loader {
                id: chargeurVolet
                anchors.fill: parent
                sourceComponent: composantRedaction
                onLoaded: item.conteneur = voletRedaction
            }
            onClosed: {
                fenetre.oublierRedaction(redaction ? redaction.jeton : "")
                Qt.callLater(function() { voletRedaction.destroy() })
            }
        }
    }

    Component {
        id: composantRedaction
        Page {
            id: redac
            // Une fenêtre à part ne reçoit ni la palette ni la police de la
            // fenêtre principale : l'apparence choisie les lui transmet.
            palette: fenetre.palette
            font: fenetre.font

            property string jeton: ""
            property var conteneur: null
            property int brouillonUid: 0
            property string origineChemin: ""
            property int origineUid: 0
            property string origineMode: ""
            property string enReponseA: ""
            property var references: []
            // Fichiers joints : [{chemin, nom, taille}].
            property var pieces: []
            property bool modifie: false
            property bool occupe: false
            property bool chargement: false
            property bool fermetureConfirmee: false
            property bool fermerApresEnregistrement: false
            property bool afficherCopies: false
            property bool miseEnForme: reglagesRedaction.miseEnForme
            property string brouillonChemin: ""
            // Options d'envoi (menu « Options ») : importance 1 / 0 / -1,
            // accusés, heure d'envoi (secondes Unix, 0 : tout de suite).
            property int importance: 0
            property bool accuseRemise: false
            property bool confirmationLecture: false
            property real envoiDiffere: 0
            readonly property string resumeOptions: {
                var t = []
                if (importance > 0) t.push(qsTr("Importance haute"))
                if (importance < 0) t.push(qsTr("Importance basse"))
                if (accuseRemise) t.push(qsTr("Accusé de réception"))
                if (confirmationLecture) t.push(qsTr("Confirmation de lecture"))
                if (envoiDiffere > 0) t.push(qsTr("Envoi %1").arg(fenetre.heureEnvoi(envoiDiffere)))
                return t.join("  ·  ")
            }
            property string note: ""
            property bool noteErreur: false
            readonly property string titre: champObjet.text.length > 0 ? champObjet.text : qsTr("Nouveau message")

            function choisirCompte(compte) {
                for (var i = 0; i < fenetre.comptesConnus.length; ++i)
                    if (fenetre.comptesConnus[i].compte === compte)
                        choixCompte.currentIndex = i
            }
            function compteChoisi() {
                return fenetre.comptesConnus[choixCompte.currentIndex] || null
            }

            function identiteChoisie() {
                var c = compteChoisi()
                return fenetre.identite(c ? c.adresse : "")
            }

            function pret() {
                chargement = true
                var i = identiteChoisie()
                if (i.nouveaux && i.html.length > 0) {
                    // Signature avec images : elle exige la mise en forme.
                    miseEnForme = true
                    mef.poserTexte("\n\n")
                    mef.insererHtml(2, i.html)
                } else {
                    mef.poserTexte(i.nouveaux && i.signature.length > 0 ? "\n\n" + i.signature : "")
                }
                corpsRedaction.cursorPosition = 0
                chargement = false
                occupe = false
                modifie = false
                champA.forceActiveFocus()
            }

            /// HTML de la rédaction, tel qu'il partirait (scénarios d'essai).
            function htmlActuel() {
                return mef.html()
            }

            function insererSignature() {
                var i = identiteChoisie()
                if (i.html.length > 0) {
                    miseEnForme = true
                    mef.insererHtml(corpsRedaction.cursorPosition, i.html)
                    return
                }
                if (i.signature.length === 0) {
                    signaler(qsTr("Aucune signature pour ce compte : menu « Comptes », « Nom et signature… »."), true)
                    return
                }
                mef.inserer(corpsRedaction.cursorPosition, i.signature)
            }

            /// Mise en forme coupée : le texte est ramené au brut, puces et
            /// numéros compris, pour que ce qui s'affiche soit ce qui part.
            function basculerMiseEnForme(active) {
                miseEnForme = active
                if (!active && mef.enrichi())
                    mef.poserTexte(mef.texte())
            }

            function attendre(texte) {
                chargement = true
                occupe = true
                signaler(texte, false)
            }

            function signaler(texte, erreur) {
                note = texte
                noteErreur = erreur
            }

            /// Pré-remplit la rédaction (réponse, transfert, brouillon repris).
            function remplir(p) {
                chargement = true
                champA.text = p.a || ""
                champCc.text = p.cc || ""
                champCci.text = p.cci || ""
                champObjet.text = p.objet || ""
                if (p.html && p.html.length > 0) {
                    miseEnForme = true
                    mef.poserHtml(p.html)
                } else {
                    var i = identiteChoisie()
                    var avecSignature = p.mode !== "brouillon" && i.reponses
                    if (avecSignature && i.html.length > 0) {
                        // La signature entre les deux lignes vides et le texte cité.
                        miseEnForme = true
                        mef.poserTexte("\n\n" + (p.texte || ""))
                        mef.insererHtml(2, i.html)
                    } else {
                        var signature = avecSignature && i.signature.length > 0 ? "\n\n" + i.signature : ""
                        mef.poserTexte(signature + (p.texte || ""))
                    }
                }
                brouillonUid = p.brouillonUid || 0
                brouillonChemin = p.brouillonChemin || ""
                importance = p.importance || 0
                accuseRemise = p.accuseRemise === true
                confirmationLecture = p.confirmationLecture === true
                envoiDiffere = p.envoiDiffere || 0
                origineChemin = p.origineChemin || ""
                origineUid = p.origineUid || 0
                origineMode = p.origineMode || ""
                enReponseA = p.enReponseA || ""
                references = p.references || []
                var liste = []
                var chemins = p.pieces || []
                for (var i = 0; i < chemins.length; ++i) {
                    var d = boite.decrireFichier(chemins[i])
                    if (d.length > 0)
                        liste.push(JSON.parse(d))
                }
                pieces = liste
                afficherCopies = champCc.text.length > 0 || champCci.text.length > 0
                chargement = false
                occupe = false
                modifie = false
                signaler("", false)
                if (champA.text.length === 0) {
                    champA.forceActiveFocus()
                } else {
                    corpsRedaction.forceActiveFocus()
                    corpsRedaction.cursorPosition = 0
                }
            }

            function contenu() {
                var c = compteChoisi()
                return {
                    jeton: jeton, de: c ? c.adresse : "", nom: identiteChoisie().nom,
                    a: champA.text, cc: champCc.text, cci: champCci.text,
                    objet: champObjet.text, texte: mef.texte(), html: miseEnForme ? mef.html() : "",
                    pieces: pieces.map(function(f) { return f.chemin }),
                    enReponseA: enReponseA, references: references, brouillonUid: brouillonUid,
                    brouillonChemin: brouillonChemin,
                    origineChemin: origineChemin, origineUid: origineUid, origineMode: origineMode,
                    importance: importance, accuseRemise: accuseRemise,
                    confirmationLecture: confirmationLecture, envoiDiffere: Math.round(envoiDiffere)
                }
            }

            function ajouterFichiers(urls) {
                var liste = pieces.slice()
                for (var i = 0; i < urls.length; ++i) {
                    var d = boite.decrireFichier(urls[i].toString())
                    if (d.length === 0)
                        continue
                    var f = JSON.parse(d)
                    if (!liste.some(function(x) { return x.chemin === f.chemin }))
                        liste.push(f)
                }
                if (liste.length !== pieces.length) {
                    pieces = liste
                    modifie = true
                }
            }

            function retirerPiece(i) {
                var liste = pieces.slice()
                liste.splice(i, 1)
                pieces = liste
                modifie = true
            }

            function envoyer(sansObjet) {
                if (occupe)
                    return
                var c = compteChoisi()
                if (!c) {
                    signaler(qsTr("Aucun compte pour envoyer ce message."), true)
                    return
                }
                if ((champA.text + champCc.text + champCci.text).trim().length === 0) {
                    signaler(qsTr("Indiquez au moins un destinataire."), true)
                    champA.forceActiveFocus()
                    return
                }
                if (champObjet.text.trim().length === 0 && !sansObjet) {
                    dlgSansObjet.open()
                    return
                }
                if (envoiDiffere > 0 && envoiDiffere * 1000 <= Date.now())
                    envoiDiffere = 0
                occupe = true
                signaler(envoiDiffere > 0 ? qsTr("Mise en attente…") : qsTr("Envoi…"), false)
                if (!boite.envoyerMessage(c.compte, JSON.stringify(contenu())))
                    echouer(boite.erreur.length > 0 ? boite.erreur : qsTr("Envoi impossible."))
            }

            function enregistrer() {
                if (occupe)
                    return
                var c = compteChoisi()
                if (!c)
                    return
                occupe = true
                signaler(qsTr("Enregistrement du brouillon…"), false)
                if (!boite.enregistrerBrouillon(c.compte, JSON.stringify(contenu())))
                    echouer(boite.erreur.length > 0 ? boite.erreur : qsTr("Enregistrement impossible."))
            }

            function brouillonEnregistre(uid) {
                brouillonUid = uid
                occupe = false
                modifie = false
                signaler(qsTr("Brouillon enregistré à %1.").arg(Qt.formatTime(new Date(), "hh:mm")), false)
                if (fermerApresEnregistrement)
                    conteneur.fermerRedaction()
            }

            function echouer(message) {
                occupe = false
                chargement = false
                fermerApresEnregistrement = false
                signaler(message, true)
            }

            // ---- adresses proposées à la saisie
            function proposer(champ) {
                var avant = champ.text.substring(0, champ.cursorPosition)
                var coupure = Math.max(avant.lastIndexOf(","), avant.lastIndexOf(";"))
                var morceau = avant.substring(coupure + 1).trim()
                if (morceau.length < 2) {
                    suggestions.close()
                    return
                }
                var liste = JSON.parse(boite.adressesConnues(morceau))
                if (liste.length === 0) {
                    suggestions.close()
                    return
                }
                suggestions.champ = champ
                suggestions.liste = liste
                vueSuggestions.currentIndex = 0
                if (!suggestions.opened)
                    suggestions.open()
            }

            function affichable(s) {
                if (!s.nom || s.nom.toLowerCase() === s.adresse.toLowerCase())
                    return s.adresse
                var nom = s.nom.replace(/"/g, "")
                return /[,;<>]/.test(nom) ? "\"" + nom + "\" <" + s.adresse + ">" : nom + " <" + s.adresse + ">"
            }

            function accepterSuggestion(i) {
                var champ = suggestions.champ
                var s = suggestions.liste[i]
                suggestions.close()
                if (!champ || !s)
                    return
                var avant = champ.text.substring(0, champ.cursorPosition)
                var apres = champ.text.substring(champ.cursorPosition).replace(/^[^,;]*[,;]?\s*/, "")
                var coupure = Math.max(avant.lastIndexOf(","), avant.lastIndexOf(";"))
                var tete = avant.substring(0, coupure + 1)
                var nouveau = (tete.length > 0 ? tete + " " : "") + affichable(s) + "; "
                champ.text = nouveau + apres
                champ.cursorPosition = nouveau.length
                modifie = true
            }

            /// Flèches, Entrée, Tab et Échap pilotent la liste quand elle est ouverte.
            function toucheAdresse(ev) {
                if (!suggestions.opened)
                    return
                if (ev.key === Qt.Key_Down) {
                    vueSuggestions.incrementCurrentIndex()
                } else if (ev.key === Qt.Key_Up) {
                    vueSuggestions.decrementCurrentIndex()
                } else if (ev.key === Qt.Key_Return || ev.key === Qt.Key_Enter || ev.key === Qt.Key_Tab) {
                    accepterSuggestion(vueSuggestions.currentIndex)
                } else if (ev.key === Qt.Key_Escape) {
                    suggestions.close()
                } else {
                    return
                }
                ev.accepted = true
            }

            function demanderFermeture() {
                if (!modifie || fermetureConfirmee) {
                    conteneur.fermerRedaction()
                    return
                }
                dlgFermer.open()
            }

            header: ToolBar {
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 6
                    anchors.rightMargin: 6
                    ToolButton {
                        text: redac.envoiDiffere > 0 ? qsTr("Programmer") : qsTr("Envoyer")
                        font.bold: true
                        enabled: !redac.occupe
                        onClicked: redac.envoyer(false)
                    }
                    ToolButton {
                        text: qsTr("Enregistrer")
                        enabled: !redac.occupe
                        onClicked: redac.enregistrer()
                    }
                    ToolButton {
                        text: qsTr("Joindre…")
                        enabled: !redac.occupe
                        onClicked: choixFichiers.open()
                    }
                    ToolButton {
                        text: qsTr("Cc / Cci")
                        checkable: true
                        checked: redac.afficherCopies
                        onToggled: redac.afficherCopies = checked
                    }
                    ToolButton {
                        id: boutonOptions
                        text: qsTr("Options ▾")
                        onClicked: menuOptions.popup(boutonOptions, 0, boutonOptions.height)
                    }
                    Label {
                        text: redac.note
                        color: redac.noteErreur ? "#b00020" : fenetre.palette.windowText
                        opacity: redac.noteErreur ? 1 : 0.75
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignRight
                    }
                    ToolButton {
                        text: qsTr("Fermer")
                        onClicked: redac.demanderFermeture()
                    }
                }
            }

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 8
                spacing: 6

                GridLayout {
                    columns: 2
                    columnSpacing: 8
                    rowSpacing: 4
                    Layout.fillWidth: true
                    enabled: !redac.chargement

                    Label { text: qsTr("De :") }
                    ComboBox {
                        id: choixCompte
                        model: fenetre.comptesConnus
                        displayText: currentIndex >= 0 && fenetre.comptesConnus[currentIndex]
                                     ? fenetre.comptesConnus[currentIndex].adresse : ""
                        delegate: ItemDelegate {
                            required property var modelData
                            width: ListView.view ? ListView.view.width : implicitWidth
                            text: modelData.adresse
                        }
                        Layout.fillWidth: true
                        onActivated: redac.modifie = true
                    }
                    Label { text: qsTr("À :") }
                    TextField {
                        id: champA
                        onTextEdited: {
                            redac.modifie = true
                            redac.proposer(champA)
                        }
                        Keys.onPressed: function(ev) { redac.toucheAdresse(ev) }
                        onActiveFocusChanged: if (!activeFocus && suggestions.champ === champA) suggestions.close()
                        placeholderText: qsTr("nom@exemple.fr ; autre@exemple.fr")
                        inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                        Layout.fillWidth: true
                    }
                    Label { text: qsTr("Cc :"); visible: redac.afficherCopies }
                    TextField {
                        id: champCc
                        onTextEdited: {
                            redac.modifie = true
                            redac.proposer(champCc)
                        }
                        Keys.onPressed: function(ev) { redac.toucheAdresse(ev) }
                        onActiveFocusChanged: if (!activeFocus && suggestions.champ === champCc) suggestions.close()
                        visible: redac.afficherCopies
                        inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                        Layout.fillWidth: true
                    }
                    Label { text: qsTr("Cci :"); visible: redac.afficherCopies }
                    TextField {
                        id: champCci
                        onTextEdited: {
                            redac.modifie = true
                            redac.proposer(champCci)
                        }
                        Keys.onPressed: function(ev) { redac.toucheAdresse(ev) }
                        onActiveFocusChanged: if (!activeFocus && suggestions.champ === champCci) suggestions.close()
                        visible: redac.afficherCopies
                        inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                        Layout.fillWidth: true
                    }
                    Label { text: qsTr("Objet :") }
                    TextField {
                        id: champObjet
                        Layout.fillWidth: true
                        onTextEdited: redac.modifie = true
                    }
                }

                // Options d'envoi choisies, rappelées en clair.
                Label {
                    Layout.fillWidth: true
                    visible: redac.resumeOptions.length > 0
                    text: redac.resumeOptions
                    color: redac.importance > 0 ? "#c42b1c" : fenetre.palette.windowText
                    elide: Text.ElideRight
                }

                // Mise en forme : les boutons ne prennent pas le focus, pour que
                // la sélection du texte reste celle sur laquelle ils agissent.
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    CheckBox {
                        text: qsTr("Mise en forme")
                        checked: redac.miseEnForme
                        focusPolicy: Qt.NoFocus
                        onToggled: redac.basculerMiseEnForme(checked)
                    }
                    ToolButton {
                        visible: redac.miseEnForme
                        text: qsTr("G")
                        font.bold: true
                        checkable: true
                        checked: mef.gras
                        focusPolicy: Qt.NoFocus
                        onClicked: mef.basculerGras()
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Gras (Ctrl+B)")
                    }
                    ToolButton {
                        visible: redac.miseEnForme
                        text: qsTr("I")
                        font.italic: true
                        checkable: true
                        checked: mef.italique
                        focusPolicy: Qt.NoFocus
                        onClicked: mef.basculerItalique()
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Italique (Ctrl+I)")
                    }
                    ToolButton {
                        visible: redac.miseEnForme
                        text: qsTr("S")
                        font.underline: true
                        checkable: true
                        checked: mef.souligne
                        focusPolicy: Qt.NoFocus
                        onClicked: mef.basculerSouligne()
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("Souligné (Ctrl+U)")
                    }
                    ToolButton {
                        visible: redac.miseEnForme
                        text: qsTr("• Liste")
                        checkable: true
                        checked: mef.puces
                        focusPolicy: Qt.NoFocus
                        onClicked: mef.basculerListe(false)
                    }
                    ToolButton {
                        visible: redac.miseEnForme
                        text: qsTr("1. Liste")
                        checkable: true
                        checked: mef.numeros
                        focusPolicy: Qt.NoFocus
                        onClicked: mef.basculerListe(true)
                    }
                    ToolButton {
                        visible: redac.miseEnForme
                        text: qsTr("Lien…")
                        focusPolicy: Qt.NoFocus
                        onClicked: dlgAdresseLien.open()
                    }
                    ToolButton {
                        visible: redac.miseEnForme
                        text: qsTr("Effacer la mise en forme")
                        focusPolicy: Qt.NoFocus
                        onClicked: mef.effacerMiseEnForme()
                    }
                    Item { Layout.fillWidth: true }
                    ToolButton {
                        text: qsTr("Signature")
                        focusPolicy: Qt.NoFocus
                        onClicked: redac.insererSignature()
                    }
                }

                // Fichiers joints, chacun retirable.
                Flow {
                    Layout.fillWidth: true
                    spacing: 6
                    visible: redac.pieces.length > 0
                    Repeater {
                        model: redac.pieces
                        delegate: Rectangle {
                            required property var modelData
                            required property int index
                            radius: 4
                            color: fenetre.palette.base
                            border.color: fenetre.palette.mid
                            width: lignePiece.implicitWidth + 8
                            height: lignePiece.implicitHeight + 4
                            RowLayout {
                                id: lignePiece
                                anchors.centerIn: parent
                                spacing: 2
                                Label {
                                    text: modelData.nom + "  (" + fenetre.tailleLisible(modelData.taille) + ")"
                                    Layout.leftMargin: 6
                                }
                                ToolButton {
                                    text: "×"
                                    enabled: !redac.occupe
                                    onClicked: redac.retirerPiece(index)
                                }
                            }
                        }
                    }
                }

                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                    rightPadding: ScrollBar.vertical.width
                    TextArea {
                        id: corpsRedaction
                        textFormat: TextEdit.RichText
                        wrapMode: TextEdit.Wrap
                        selectByMouse: true
                        persistentSelection: true
                        font.pointSize: fenetre.tailleColonne("message")
                        enabled: !redac.chargement
                        placeholderText: qsTr("Votre message")
                        background: Rectangle { color: fenetre.palette.base }
                        onTextChanged: if (!redac.chargement) redac.modifie = true
                    }
                }
            }

            // Fichiers glissés depuis l'explorateur : joints.
            DropArea {
                anchors.fill: parent
                onEntered: function(glisse) { glisse.accepted = glisse.hasUrls }
                onDropped: function(depose) {
                    if (depose.hasUrls) {
                        redac.ajouterFichiers(depose.urls)
                        depose.accept(Qt.CopyAction)
                    }
                }
            }

            Menu {
                id: menuOptions
                onAboutToShow: width = fenetre.largeurMenu(menuOptions)
                MenuItem {
                    text: qsTr("Importance haute")
                    checkable: true
                    checked: redac.importance > 0
                    onTriggered: { redac.importance = checked ? 1 : 0; redac.modifie = true }
                }
                MenuItem {
                    text: qsTr("Importance basse")
                    checkable: true
                    checked: redac.importance < 0
                    onTriggered: { redac.importance = checked ? -1 : 0; redac.modifie = true }
                }
                MenuSeparator {}
                MenuItem {
                    text: qsTr("Demander un accusé de réception")
                    checkable: true
                    checked: redac.accuseRemise
                    onTriggered: { redac.accuseRemise = checked; redac.modifie = true }
                }
                MenuItem {
                    text: qsTr("Demander une confirmation de lecture")
                    checkable: true
                    checked: redac.confirmationLecture
                    onTriggered: { redac.confirmationLecture = checked; redac.modifie = true }
                }
                MenuSeparator {}
                MenuItem {
                    text: qsTr("Différer l'envoi…")
                    onTriggered: dlgDiffere.open()
                }
                MenuItem {
                    visible: redac.envoiDiffere > 0
                    height: visible ? implicitHeight : 0
                    text: qsTr("Envoyer sans attendre")
                    onTriggered: { redac.envoiDiffere = 0; redac.modifie = true }
                }
            }

            // Heure d'envoi : quelques choix courants, ou une date et une heure.
            Dialog {
                id: dlgDiffere
                title: qsTr("Différer l'envoi")
                modal: true
                anchors.centerIn: parent
                width: Math.min(460, redac.width - 24)
                standardButtons: Dialog.Ok | Dialog.Cancel
                function proposer(d) {
                    champJourDiffere.text = Qt.formatDate(d, "dd/MM/yyyy")
                    champHeureDiffere.text = Qt.formatTime(d, "HH:mm")
                    noteDiffere.text = ""
                }
                function lire() {
                    var j = champJourDiffere.text.trim().match(/^(\d{1,2})\/(\d{1,2})\/(\d{4})$/)
                    var h = champHeureDiffere.text.trim().match(/^(\d{1,2})[:h](\d{2})$/)
                    if (!j || !h)
                        return 0
                    var d = new Date(Number(j[3]), Number(j[2]) - 1, Number(j[1]), Number(h[1]), Number(h[2]))
                    return isNaN(d.getTime()) ? 0 : d.getTime() / 1000
                }
                onOpened: {
                    var d = redac.envoiDiffere > 0 ? new Date(redac.envoiDiffere * 1000)
                                                   : new Date(Date.now() + 3600 * 1000)
                    proposer(d)
                }
                onAccepted: {
                    var t = lire()
                    if (t * 1000 <= Date.now() + 30000) {
                        redac.signaler(qsTr("Heure d'envoi invalide ou déjà passée."), true)
                        return
                    }
                    redac.envoiDiffere = t
                    redac.modifie = true
                }
                ColumnLayout {
                    anchors.fill: parent
                    spacing: 6
                    Flow {
                        Layout.fillWidth: true
                        spacing: 6
                        Button {
                            text: qsTr("Dans une heure")
                            onClicked: dlgDiffere.proposer(new Date(Date.now() + 3600 * 1000))
                        }
                        Button {
                            text: qsTr("Demain 8 h")
                            onClicked: {
                                var d = new Date()
                                d.setDate(d.getDate() + 1)
                                d.setHours(8, 0, 0, 0)
                                dlgDiffere.proposer(d)
                            }
                        }
                        Button {
                            text: qsTr("Lundi 8 h")
                            onClicked: {
                                var d = new Date()
                                d.setDate(d.getDate() + ((8 - d.getDay()) % 7 || 7))
                                d.setHours(8, 0, 0, 0)
                                dlgDiffere.proposer(d)
                            }
                        }
                    }
                    GridLayout {
                        columns: 2
                        Label { text: qsTr("Jour :") }
                        TextField { id: champJourDiffere; placeholderText: "30/09/2026"; Layout.fillWidth: true }
                        Label { text: qsTr("Heure :") }
                        TextField { id: champHeureDiffere; placeholderText: "08:00"; Layout.fillWidth: true }
                    }
                    Label {
                        id: noteDiffere
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.75
                        font.pixelSize: 11
                        text: ""
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        opacity: 0.75
                        font.pixelSize: 11
                        text: qsTr("Le message attend sur le serveur, dans « Envoi différé ». Il part à l'heure dite si MMail est ouvert à ce moment-là, sur ce poste ou un autre ; sinon, à la prochaine ouverture.")
                    }
                }
            }

            MiseEnForme {
                id: mef
                document: corpsRedaction.textDocument
                curseur: corpsRedaction.cursorPosition
                debut: corpsRedaction.selectionStart
                fin: corpsRedaction.selectionEnd
            }

            Popup {
                id: suggestions
                property var champ: null
                property var liste: []
                x: champ ? champ.mapToItem(redac, 0, 0).x : 0
                y: champ ? champ.mapToItem(redac, 0, champ.height).y : 0
                width: champ ? champ.width : 200
                padding: 1
                closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
                contentItem: ListView {
                    id: vueSuggestions
                    implicitHeight: contentHeight
                    clip: true
                    model: suggestions.liste
                    delegate: ItemDelegate {
                        required property var modelData
                        required property int index
                        width: ListView.view.width
                        highlighted: ListView.isCurrentItem
                        // Sans focus : le champ garderait sinon la main… et
                        // fermerait la liste avant que le clic ne compte.
                        focusPolicy: Qt.NoFocus
                        text: modelData.nom.length > 0 ? modelData.nom + "  —  " + modelData.adresse : modelData.adresse
                        onClicked: redac.accepterSuggestion(index)
                    }
                }
            }

            Dialog {
                id: dlgAdresseLien
                title: qsTr("Insérer un lien")
                modal: true
                anchors.centerIn: parent
                width: Math.min(520, redac.width - 24)
                standardButtons: Dialog.Ok | Dialog.Cancel
                onOpened: {
                    champAdresseLien.text = "https://"
                    champAdresseLien.forceActiveFocus()
                    champAdresseLien.cursorPosition = champAdresseLien.text.length
                }
                onAccepted: mef.poserLien(champAdresseLien.text)
                ColumnLayout {
                    anchors.fill: parent
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: qsTr("Adresse du lien, posée sur le texte sélectionné — ou insérée telle quelle s'il n'y en a pas :")
                    }
                    TextField {
                        id: champAdresseLien
                        Layout.fillWidth: true
                        inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
                        onAccepted: dlgAdresseLien.accept()
                    }
                }
            }

            Shortcut {
                sequences: [StandardKey.Bold]
                enabled: redac.miseEnForme
                onActivated: mef.basculerGras()
            }
            Shortcut {
                sequences: [StandardKey.Italic]
                enabled: redac.miseEnForme
                onActivated: mef.basculerItalique()
            }
            Shortcut {
                sequences: [StandardKey.Underline]
                enabled: redac.miseEnForme
                onActivated: mef.basculerSouligne()
            }

            FileDialog {
                id: choixFichiers
                title: qsTr("Joindre des fichiers")
                fileMode: FileDialog.OpenFiles
                onAccepted: redac.ajouterFichiers(selectedFiles)
            }

            Dialog {
                id: dlgSansObjet
                title: qsTr("Envoyer sans objet ?")
                modal: true
                anchors.centerIn: parent
                width: Math.min(420, redac.width - 24)
                standardButtons: Dialog.Yes | Dialog.No
                Label {
                    width: dlgSansObjet.availableWidth
                    wrapMode: Text.Wrap
                    text: qsTr("Ce message n'a pas d'objet.")
                }
                onAccepted: redac.envoyer(true)
            }

            Dialog {
                id: dlgFermer
                title: qsTr("Fermer ce message ?")
                modal: true
                anchors.centerIn: parent
                width: Math.min(520, redac.width - 24)
                Label {
                    width: dlgFermer.availableWidth
                    wrapMode: Text.Wrap
                    text: qsTr("Le message a été modifié depuis son dernier enregistrement.")
                }
                footer: DialogButtonBox {
                    Button {
                        text: qsTr("Enregistrer le brouillon")
                        DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
                    }
                    Button {
                        text: qsTr("Abandonner")
                        DialogButtonBox.buttonRole: DialogButtonBox.DestructiveRole
                    }
                    Button {
                        text: qsTr("Continuer")
                        DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                    }
                }
                onAccepted: {
                    redac.fermerApresEnregistrement = true
                    redac.enregistrer()
                }
                onDiscarded: {
                    dlgFermer.close()
                    redac.conteneur.fermerRedaction()
                }
            }

            Shortcut {
                sequences: ["Ctrl+Return", "Ctrl+Enter"]
                onActivated: redac.envoyer(false)
            }
            Shortcut {
                sequences: [StandardKey.Save]
                onActivated: redac.enregistrer()
            }
        }
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

            /// Vrai si le clic tombe sur le chevron d'un dossier parent (marge
            /// comprise, pour le doigt comme pour la souris). Mesuré depuis le
            /// centre du chevron : le chevron d'un dossier déplié est tourné
            /// d'un quart de tour autour de ce centre, et son coin supérieur
            /// gauche, lui, se déplace — le clic sur un dossier déplié tombait à
            /// côté de la zone (retour de Manu, 01/10).
            function surChevron(x) {
                if (!chevronDossier.visible || !model.enfants)
                    return false
                var centre = chevronDossier.mapToItem(zoneLigne, chevronDossier.width / 2, chevronDossier.height / 2)
                return Math.abs(x - centre.x) <= chevronDossier.width / 2 + 6
            }

            // La rubrique Favoris sur un fond plus soutenu que celui des
            // comptes. Au-dessus du fond propre à la ligne (z -1, opaque dans
            // le style Fusion), sauf quand la ligne est choisie ou pressée.
            Rectangle {
                z: -0.5
                anchors.fill: parent
                visible: model.groupe === "favoris" && !ligne.highlighted && !ligne.down
                color: fenetre.fondFavoris
            }

            function menuLigne() {
                if (model.genre === "compte")
                    menuCompte.ouvrir(model.compte, model.adresse, model.hote, model.etat)
                else if (estDossier)
                    menuDossier.ouvrir(model.compte, model.chemin, model.favori, model.masque, model.nom, model.selectionnable)
            }

            // Cible de dépôt : des messages glissés depuis la liste (sur un
            // dossier), ou un dossier glissé depuis l'arborescence (sur la
            // rubrique Favoris ou l'un de ses favoris, décision 3).
            // Un compte glissé sur un autre compte prend sa place (moitié haute)
            // ou la suivante (moitié basse).
            DropArea {
                id: depot
                anchors.fill: parent
                keys: ["mmail/messages", "mmail/dossier", "mmail/compte"]
                onEntered: function(glisse) {
                    var dossierGlisse = glisse.keys.indexOf("mmail/dossier") >= 0
                    var compteGlisse = glisse.keys.indexOf("mmail/compte") >= 0
                    var accepte = compteGlisse
                            ? (model.genre === "compte" && glisse.source && glisse.source.compte !== model.compte)
                            : dossierGlisse
                            ? (model.genre === "rubrique" || model.genre === "favori"
                               || model.genre === "favori-vide")
                            : (ligne.estDossier && model.selectionnable)
                    glisse.accepted = accepte
                    ligne.survol = accepte
                }
                onExited: ligne.survol = false
                onDropped: function(depose) {
                    ligne.survol = false
                    if (depose.keys.indexOf("mmail/compte") >= 0) {
                        boite.placerCompte(depose.source.compte,
                                           depose.y < height / 2 ? model.compte : fenetre.compteSuivant(model.compte))
                    } else if (depose.keys.indexOf("mmail/dossier") >= 0) {
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
                Drag.keys: model.genre === "compte" ? ["mmail/compte"] : ["mmail/dossier"]
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
                // Un dossier (vers les Favoris) ou un en-tête de compte (pour
                // l'ordre des comptes) se glisse, et pas au doigt : glisser y
                // fait défiler l'arborescence.
                drag.target: (ligne.estDossier || model.genre === "compte") && !fenetre.compact
                             ? etiquetteDossier : null
                drag.threshold: 8
                onPressed: function(souris) {
                    etiquetteDossier.x = souris.x
                    etiquetteDossier.y = souris.y
                }
                onClicked: function(souris) {
                    if (souris.button === Qt.RightButton)
                        ligne.menuLigne()
                    else if (ligne.surChevron(souris.x))
                        boite.replierDossier(model.compte, model.chemin, !model.replie)
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
                    Layout.preferredWidth: model.genre === "dossier" ? model.profondeur * 14
                                         : model.genre === "favori" || model.genre === "favori-vide" ? 14 : 0
                }
                // Chevron d'un dossier qui a des sous-dossiers : un clic les
                // replie ou les déplie (retour de Manu, 01/10). La place est
                // gardée pour les autres, pour que les noms restent alignés.
                Label {
                    id: chevronDossier
                    visible: model.genre === "dossier"
                    text: model.enfants ? "›" : ""
                    color: fenetre.palette.windowText
                    font.bold: true
                    rotation: model.replie ? 0 : 90
                    opacity: 0.7
                    horizontalAlignment: Text.AlignHCenter
                    Layout.preferredWidth: 14
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
                    color: ligne.highlighted ? fenetre.palette.highlightedText
                         : model.genre === "rubrique" ? fenetre.palette.highlight
                         : fenetre.palette.windowText
                    font.bold: model.genre === "compte" || model.genre === "rubrique" || model.nonLus > 0
                    // Les en-têtes, rubrique et comptes, un cran au-dessus des dossiers.
                    font.pointSize: model.genre === "compte" || model.genre === "rubrique"
                                    ? fenetre.tailleArborescence * 1.12 : fenetre.tailleArborescence
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
                    // Le motif de l'erreur reste lisible au survol, une fois le
                    // message passager effacé.
                    HoverHandler { id: survolEtat }
                    ToolTip.visible: survolEtat.hovered && model.motif.length > 0
                    ToolTip.text: model.motif
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

            // Des lignes plus aérées, séparées d'un trait (retour de Fabienne,
            // 29/09 : les messages se distinguaient mal les uns des autres).
            topPadding: 7
            bottomPadding: 7
            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 1
                color: fenetre.palette.windowText
                opacity: 0.18
            }
            contentItem: ColumnLayout {
                spacing: 2
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
                    // Trombone : le message porte des pièces jointes.
                    Canvas {
                        visible: model.pieces === true
                        Layout.preferredHeight: dateLigne.implicitHeight
                        Layout.preferredWidth: Math.round(dateLigne.implicitHeight * 0.62)
                        property color encre: ligne.highlighted ? fenetre.palette.highlightedText
                                                                : fenetre.palette.windowText
                        onEncreChanged: requestPaint()
                        onWidthChanged: requestPaint()
                        onHeightChanged: requestPaint()
                        onPaint: fenetre.dessinerTrombone(getContext("2d"), width, height, encre)
                    }
                    Label {
                        id: dateLigne
                        text: fenetre.dateCourte(model.date)
                        color: ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.windowText
                        opacity: 0.7
                        font.pointSize: fenetre.tailleListe * 0.85
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: 10
                    // Place du drapeau, posé par-dessus en bout de ligne.
                    Layout.rightMargin: 8 + drapeauLigne.width
                    spacing: 4
                    // Importance annoncée par l'expéditeur : « ! » haute, « ↓ » basse.
                    Label {
                        visible: model.importance !== 0
                        text: model.importance > 0 ? "!" : "↓"
                        font.bold: true
                        color: ligne.highlighted ? fenetre.palette.highlightedText
                             : model.importance > 0 ? "#c42b1c" : "#1a4480"
                    }
                    Label {
                        text: model.sujet
                        font.bold: !model.lu
                        color: ligne.highlighted ? fenetre.palette.highlightedText
                             : model.lu ? fenetre.palette.windowText : fenetre.palette.highlight
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
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
                    text: fenetre.accord(Object.keys(fenetre.selection).length, qsTr("message"), qsTr("messages"))
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
                onDoubleClicked: function(souris) {
                    if (souris.button === Qt.LeftButton)
                        fenetre.rediger(fenetre.dossierDeReprise ? "brouillon" : "repondre")
                }
                onReleased: {
                    if (drag.active)
                        etiquette.Drag.drop()
                }
            }

            // Drapeau de suivi, en bout de seconde ligne, comme Outlook : plein
            // s'il est posé ; en creux au survol, pour montrer qu'un clic le pose.
            Canvas {
                id: drapeauLigne
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 7
                height: dateLigne.implicitHeight
                width: Math.round(height * 0.8)
                visible: model.suivi === true || ligne.hovered || survolDrapeau.containsMouse
                property color encre: model.suivi === true
                                      ? (ligne.highlighted ? fenetre.palette.highlightedText : "#c42b1c")
                                      : (ligne.highlighted ? fenetre.palette.highlightedText : fenetre.palette.windowText)
                property bool plein: model.suivi === true
                opacity: plein ? 1 : 0.45
                onEncreChanged: requestPaint()
                onPleinChanged: requestPaint()
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
                onPaint: fenetre.dessinerDrapeau(getContext("2d"), width, height, encre, plein)
                MouseArea {
                    id: survolDrapeau
                    anchors.fill: parent
                    anchors.margins: -4
                    hoverEnabled: true
                    onClicked: fenetre.basculerSuivi([model.uid], model.suivi !== true)
                }
            }
        }
    }

    // ---------------------------------------------------------------- menus
    Menu {
        id: menuComptes
        onAboutToShow: width = fenetre.largeurMenu(menuComptes)
        MenuItem {
            text: qsTr("Ajouter un compte…")
            onTriggered: fenetre.demanderCompte(null, "")
        }
        MenuItem {
            text: qsTr("Ajouter avec un lien de configuration…")
            onTriggered: dlgLien.ouvrir("", "")
        }
        MenuItem {
            text: qsTr("Nom et signature…")
            enabled: fenetre.comptesConnus.length > 0
            onTriggered: dlgIdentite.ouvrir(boite.compteCourant)
        }
        MenuSeparator {}
        // Le retrait est aussi au clic droit sur le compte ; il est repris ici
        // pour qu'on le trouve sans le connaître.
        Menu {
            id: menuRetrait
            title: qsTr("Retirer un compte")
            onAboutToShow: width = fenetre.largeurMenu(menuRetrait)
            Instantiator {
                model: fenetre.comptesConnus
                delegate: MenuItem {
                    required property var modelData
                    text: modelData.adresse + "…"
                    onTriggered: dlgRetrait.ouvrir(modelData.compte, modelData.adresse, modelData.hote)
                }
                onObjectAdded: function(indice, objet) { menuRetrait.insertItem(indice, objet) }
                onObjectRemoved: function(indice, objet) { menuRetrait.removeItem(objet) }
            }
            MenuItem {
                text: qsTr("Aucun compte")
                enabled: false
                visible: fenetre.comptesConnus.length === 0
                height: visible ? implicitHeight : 0
            }
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Vider les corbeilles et indésirables…")
            enabled: fenetre.comptesConnus.length > 0
            onTriggered: dlgViderCorbeilles.open()
        }
    }

    Menu {
        id: menuAide
        onAboutToShow: width = fenetre.largeurMenu(menuAide)
        MenuItem {
            text: qsTr("Aide (F1)")
            onTriggered: dlgAide.open()
        }
        MenuItem {
            text: qsTr("À propos de MMail…")
            onTriggered: dlgAPropos.open()
        }
    }

    Menu {
        id: menuAffichage
        onAboutToShow: width = fenetre.largeurMenu(menuAffichage)
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
        font.pointSize: fenetre.tailleColonne("message")
        onAboutToShow: width = fenetre.largeurMenu(menuPiece)
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
        id: menuCorps
        font.pointSize: fenetre.tailleColonne("message")
        onAboutToShow: width = fenetre.largeurMenu(menuCorps)
        MenuItem {
            text: qsTr("Copier")
            enabled: vueCorps.selectedText.length > 0
            onTriggered: fenetre.copierTexte(vueCorps.selectedText, fenetre.sourceVisible ? "source" : "message")
        }
        MenuItem {
            text: qsTr("Tout sélectionner")
            onTriggered: vueCorps.selectAll()
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Copier la sélection automatiquement")
            checkable: true
            checked: reglagesEdition.copieAuto
            onTriggered: reglagesEdition.copieAuto = checked
        }
    }

    Menu {
        id: menuMessage
        font.pointSize: fenetre.tailleColonne("liste")
        onAboutToShow: width = fenetre.largeurMenu(menuMessage)
        MenuItem {
            visible: fenetre.dossierDeReprise
            height: visible ? implicitHeight : 0
            text: qsTr("Reprendre")
            onTriggered: fenetre.rediger("brouillon")
        }
        MenuItem {
            visible: !fenetre.dossierDeReprise
            height: visible ? implicitHeight : 0
            text: qsTr("Répondre")
            onTriggered: fenetre.rediger("repondre")
        }
        MenuItem {
            visible: !fenetre.dossierDeReprise
            height: visible ? implicitHeight : 0
            text: qsTr("Répondre à tous")
            onTriggered: fenetre.rediger("repondre_tous")
        }
        MenuItem {
            text: qsTr("Transférer")
            onTriggered: fenetre.rediger("transferer")
        }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Poser un drapeau de suivi")
            onTriggered: fenetre.basculerSuivi(fenetre.uidsChoisis(), true)
        }
        MenuItem {
            text: qsTr("Retirer le drapeau")
            onTriggered: fenetre.basculerSuivi(fenetre.uidsChoisis(), false)
        }
        MenuSeparator {}
        MenuItem { text: qsTr("Déplacer vers…"); onTriggered: fenetre.ouvrirDeplacer() }
        MenuItem {
            visible: !fenetre.dossierDeReprise
            height: visible ? implicitHeight : 0
            text: fenetre.roleCourant === "Junk" ? qsTr("Pas indésirable") : qsTr("Courrier indésirable")
            onTriggered: fenetre.signalerIndesirable()
        }
        MenuSeparator {}
        MenuItem { text: qsTr("Marquer comme lu"); onTriggered: fenetre.marquerSelection(true) }
        MenuItem { text: qsTr("Marquer comme non lu"); onTriggered: fenetre.marquerSelection(false) }
        MenuSeparator {}
        MenuItem { text: qsTr("Supprimer"); onTriggered: fenetre.supprimerSelection() }
        MenuSeparator {}
        MenuItem {
            text: qsTr("Copier l'adresse de l'expéditeur")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: fenetre.copierChamp("adresse")
        }
        MenuItem {
            text: qsTr("Copier l'objet")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: fenetre.copierChamp("sujet")
        }
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
        font.pointSize: fenetre.tailleColonne("arborescence")
        onAboutToShow: width = fenetre.largeurMenu(menuDossier)
        property int compte: 0
        property string chemin: ""
        property bool favori: false
        property bool masque: false
        property string nom: ""
        property bool selectionnable: true
        function ouvrir(c, ch, f, m, n, sel) {
            compte = c; chemin = ch; favori = f; masque = m; nom = n; selectionnable = sel
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
        MenuSeparator {}
        MenuItem {
            text: qsTr("Vider ce dossier…")
            enabled: menuDossier.selectionnable
            onTriggered: dlgViderDossier.ouvrir(menuDossier.compte, menuDossier.chemin, menuDossier.nom)
        }
    }

    Menu {
        id: menuCompte
        font.pointSize: fenetre.tailleColonne("arborescence")
        onAboutToShow: width = fenetre.largeurMenu(menuCompte)
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
        // L'ordre des comptes se règle aussi en glissant leur en-tête.
        MenuItem {
            text: qsTr("Monter ce compte")
            onTriggered: boite.decalerCompte(menuCompte.compte, -1)
        }
        MenuItem {
            text: qsTr("Descendre ce compte")
            onTriggered: boite.decalerCompte(menuCompte.compte, 1)
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
    // Courrier indésirable, comme dans Outlook.
    Shortcut {
        sequence: "Ctrl+Alt+J"
        enabled: !fenetre.dialogueOuvert() && Object.keys(fenetre.selection).length > 0 && !fenetre.dossierDeReprise
        onActivated: fenetre.signalerIndesirable()
    }
    // Insertion : pose ou retire le drapeau, comme dans Outlook.
    Shortcut {
        sequence: "Ins"
        enabled: !fenetre.dialogueOuvert() && Object.keys(fenetre.selection).length > 0
        onActivated: {
            var uids = fenetre.uidsChoisis()
            var i = fenetre.indexDe(uids[0])
            fenetre.basculerSuivi(uids, !(i >= 0 && modeleMessages.get(i).suivi))
        }
    }

    // Raccourcis d'Outlook pour la rédaction.
    Shortcut {
        sequences: [StandardKey.New]
        enabled: !fenetre.dialogueOuvert()
        onActivated: fenetre.rediger("nouveau")
    }
    Shortcut {
        sequence: "Ctrl+R"
        enabled: !fenetre.dialogueOuvert() && fenetre.uidCourant > 0
        onActivated: fenetre.rediger(fenetre.dossierDeReprise ? "brouillon" : "repondre")
    }
    Shortcut {
        sequence: "Ctrl+Shift+R"
        enabled: !fenetre.dialogueOuvert() && fenetre.uidCourant > 0
        onActivated: fenetre.rediger("repondre_tous")
    }
    Shortcut {
        sequence: "Ctrl+F"
        enabled: !fenetre.dialogueOuvert() && fenetre.uidCourant > 0
        onActivated: fenetre.rediger("transferer")
    }
    Shortcut {
        sequences: [StandardKey.HelpContents]
        enabled: !fenetre.dialogueOuvert()
        onActivated: dlgAide.open()
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
        // Vrai dès que la personne a touché au serveur : la configuration
        // automatique ne réécrit plus ce champ.
        property bool hoteSaisi: false
        property string infoServeur: ""
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
                onEditingFinished: fenetre.chercherServeur()
            }
            Label { text: qsTr("Serveur IMAP :") }
            TextField {
                id: champHote
                placeholderText: "mail.exemple.fr"
                enabled: dlgCompte.compteId === 0
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
                onTextEdited: dlgCompte.hoteSaisi = text.length > 0
            }
            Label {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                visible: text.length > 0
                wrapMode: Text.Wrap
                opacity: 0.75
                font.pixelSize: 11
                text: dlgCompte.infoServeur
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
        id: dlgIdentite
        title: qsTr("Nom et signature")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(760, fenetre.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        property string note: ""
        function ouvrir(compte) {
            var i = 0
            for (var k = 0; k < fenetre.comptesConnus.length; ++k)
                if (fenetre.comptesConnus[k].compte === compte)
                    i = k
            choixIdentite.currentIndex = i
            charger()
            open()
        }
        function adresse() {
            var c = fenetre.comptesConnus[choixIdentite.currentIndex]
            return c ? c.adresse : ""
        }
        function charger() {
            var i = fenetre.identite(adresse())
            champNomAffiche.text = i.nom
            if (i.html.length > 0)
                mefSignature.poserHtml(i.html)
            else
                mefSignature.poserTexte(i.signature)
            champSignature.cursorPosition = 0
            caseSignatureNouveaux.checked = i.nouveaux
            caseSignatureReponses.checked = i.reponses
            note = ""
        }
        // Deux versions : le texte, pour les messages sans mise en forme, et
        // le HTML s'il y a de quoi — images, gras, liens.
        onAccepted: fenetre.poserIdentite(adresse(), {
            nom: champNomAffiche.text.trim(), signature: mefSignature.texte(),
            html: mefSignature.enrichi() ? mefSignature.html() : "",
            nouveaux: caseSignatureNouveaux.checked, reponses: caseSignatureReponses.checked })

        ColumnLayout {
            anchors.fill: parent
            spacing: 6
            ComboBox {
                id: choixIdentite
                Layout.fillWidth: true
                model: fenetre.comptesConnus
                displayText: currentIndex >= 0 && fenetre.comptesConnus[currentIndex]
                             ? fenetre.comptesConnus[currentIndex].adresse : ""
                delegate: ItemDelegate {
                    required property var modelData
                    width: ListView.view ? ListView.view.width : implicitWidth
                    text: modelData.adresse
                }
                onActivated: dlgIdentite.charger()
            }
            Label { text: qsTr("Nom affiché chez les destinataires :") }
            TextField {
                id: champNomAffiche
                Layout.fillWidth: true
                placeholderText: qsTr("Prénom Nom")
            }
            Label { text: qsTr("Signature :") }
            // Barre de mise en forme de la signature ; les boutons ne prennent
            // pas le focus, la sélection reste celle sur laquelle ils agissent.
            Flow {
                Layout.fillWidth: true
                spacing: 4
                ToolButton {
                    text: qsTr("G")
                    font.bold: true
                    checkable: true
                    checked: mefSignature.gras
                    focusPolicy: Qt.NoFocus
                    onClicked: mefSignature.basculerGras()
                }
                ToolButton {
                    text: qsTr("I")
                    font.italic: true
                    checkable: true
                    checked: mefSignature.italique
                    focusPolicy: Qt.NoFocus
                    onClicked: mefSignature.basculerItalique()
                }
                ToolButton {
                    text: qsTr("S")
                    font.underline: true
                    checkable: true
                    checked: mefSignature.souligne
                    focusPolicy: Qt.NoFocus
                    onClicked: mefSignature.basculerSouligne()
                }
                ToolButton {
                    text: qsTr("Lien…")
                    focusPolicy: Qt.NoFocus
                    onClicked: dlgLienSignature.open()
                }
                ToolButton {
                    text: qsTr("Image…")
                    focusPolicy: Qt.NoFocus
                    onClicked: choixImageSignature.open()
                }
                ToolButton {
                    text: qsTr("Importer une signature Outlook…")
                    focusPolicy: Qt.NoFocus
                    onClicked: choixImportSignature.open()
                    ToolTip.visible: hovered
                    ToolTip.delay: 600
                    ToolTip.text: qsTr("Le fichier .htm du dossier %APPDATA%\\Microsoft\\Signatures, avec ses images.")
                }
                ToolButton {
                    text: qsTr("Effacer la mise en forme")
                    focusPolicy: Qt.NoFocus
                    onClicked: mefSignature.effacerMiseEnForme()
                }
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(320, fenetre.height * 0.4)
                TextArea {
                    id: champSignature
                    textFormat: TextEdit.RichText
                    wrapMode: TextEdit.Wrap
                    selectByMouse: true
                    placeholderText: qsTr("Prénom Nom\nFonction — Société\nTéléphone")
                    color: "#1f1f1f"
                    background: Rectangle { color: "#ffffff"; border.color: fenetre.palette.mid }
                }
            }
            MiseEnForme {
                id: mefSignature
                document: champSignature.textDocument
                curseur: champSignature.cursorPosition
                debut: champSignature.selectionStart
                fin: champSignature.selectionEnd
            }
            Label {
                visible: dlgIdentite.note.length > 0
                text: dlgIdentite.note
                color: "#b00020"
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            CheckBox {
                id: caseSignatureNouveaux
                text: qsTr("L'ajouter aux nouveaux messages")
            }
            CheckBox {
                id: caseSignatureReponses
                text: qsTr("L'ajouter aux réponses et aux transferts")
            }
        }

        FileDialog {
            id: choixImageSignature
            title: qsTr("Insérer une image dans la signature")
            nameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.gif *.bmp *.webp)")]
            onAccepted: {
                var url = boite.imageSignature(dlgIdentite.adresse(), selectedFile)
                if (url.length > 0) {
                    mefSignature.insererImage(url, 640)
                    dlgIdentite.note = ""
                } else {
                    dlgIdentite.note = boite.erreur
                }
            }
        }
        FileDialog {
            id: choixImportSignature
            title: qsTr("Importer une signature Outlook")
            nameFilters: [qsTr("Signatures (*.htm *.html)")]
            onAccepted: {
                var html = boite.importerSignature(dlgIdentite.adresse(), selectedFile)
                if (html.length > 0) {
                    mefSignature.poserHtml(html)
                    champSignature.cursorPosition = 0
                    dlgIdentite.note = ""
                } else {
                    dlgIdentite.note = boite.erreur
                }
            }
        }
        Dialog {
            id: dlgLienSignature
            title: qsTr("Insérer un lien")
            modal: true
            anchors.centerIn: parent
            width: Math.min(520, dlgIdentite.width - 24)
            standardButtons: Dialog.Ok | Dialog.Cancel
            onOpened: {
                champLienSignature.text = "https://"
                champLienSignature.forceActiveFocus()
                champLienSignature.cursorPosition = champLienSignature.text.length
            }
            onAccepted: mefSignature.poserLien(champLienSignature.text)
            TextField {
                id: champLienSignature
                anchors.left: parent.left
                anchors.right: parent.right
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
                onAccepted: dlgLienSignature.accept()
            }
        }
    }

    Dialog {
        id: dlgLien
        title: qsTr("Ajouter avec un lien de configuration")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(520, fenetre.width - 24)
        standardButtons: Dialog.Ok | Dialog.Cancel
        function ouvrir(lien, note) {
            champLien.text = lien
            noteLien.text = note
            open()
        }
        onOpened: champLien.forceActiveFocus()
        onAccepted: fenetre.lireLien(champLien.text.trim())

        ColumnLayout {
            anchors.fill: parent
            spacing: 6
            Label {
                id: noteLien
                Layout.fillWidth: true
                visible: text.length > 0
                wrapMode: Text.Wrap
                color: "#b00020"
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Collez le lien de configuration que vous avez reçu :")
            }
            TextField {
                id: champLien
                placeholderText: "https://…"
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
                onAccepted: dlgLien.accept()
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.75
                font.pixelSize: 11
                text: qsTr("Le lien ne sert qu'une fois. Il apporte l'adresse, le serveur et le mot de passe de chaque compte ; le mot de passe va directement au coffre du système, sans être affiché.")
            }
        }
    }

    Dialog {
        id: dlgDeplacer
        title: qsTr("Déplacer %1 vers…").arg(fenetre.accord(Object.keys(fenetre.selection).length, qsTr("message"), qsTr("messages")))
        modal: true
        anchors.centerIn: Overlay.overlay
        // Zoom de la colonne d'où il est ouvert (liste, message), fixé à
        // l'ouverture ; les dimensions suivent, dans la limite de la fenêtre.
        property real zoom: 1
        font.pointSize: fenetre.tailleBase * zoom
        width: Math.min(520 * zoom, fenetre.width - 24)
        height: Math.min(560 * zoom, fenetre.height - 40)
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
                ScrollBar.vertical: ScrollBar {
                    policy: size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                }
                model: ListModel { id: modeleCibles }
                delegate: ItemDelegate {
                    width: ListView.view.width
                           - (vueCibles.ScrollBar.vertical.size < 1 ? vueCibles.ScrollBar.vertical.width : 0)
                    highlighted: ListView.isCurrentItem
                    onClicked: vueCibles.currentIndex = index
                    onDoubleClicked: dlgDeplacer.accept()
                    contentItem: RowLayout {
                        Label {
                            text: model.adresse
                            opacity: 0.6
                            Layout.preferredWidth: 170 * dlgDeplacer.zoom
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

    // Aide : ce que l'interface ne dit pas d'elle-même — gestes, raccourcis,
    // et ce qu'un geste fait ou ne fait pas sur le serveur.
    Dialog {
        id: dlgAide
        title: qsTr("Aide de MMail")
        modal: true
        anchors.centerIn: Overlay.overlay
        // Taille fixée, et non déduite du texte : même raison que plus bas pour
        // le dialogue de retrait.
        width: Math.min(620, fenetre.width - 24)
        height: Math.min(620, fenetre.height - 24)
        standardButtons: Dialog.Close
        ScrollView {
            id: defilementAide
            anchors.fill: parent
            ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
            // Réserve fixe, comme pour le corps du message : texte replié.
            rightPadding: ScrollBar.vertical.width
            contentWidth: availableWidth
            clip: true
            Label {
                width: defilementAide.availableWidth
                wrapMode: Text.Wrap
                textFormat: Text.StyledText
                text: qsTr(
                    "<b>Comptes</b><br>"
                    + "Menu « Comptes » : ajouter ou retirer un compte. À l'ajout, le serveur est "
                    + "cherché d'après l'adresse, si son domaine publie sa configuration. Un lien de "
                    + "configuration, reçu de votre administrateur, ajoute un ou plusieurs comptes "
                    + "sans rien saisir ; il ne sert qu'une fois. Retirer un compte efface de ce "
                    + "poste son index et son mot de passe mémorisé ; rien n'est supprimé sur le serveur. "
                    + "Clic droit sur un compte : se connecter, se déconnecter, oublier le mot de passe. "
                    + "Un clic sur le compte replie ou déplie ses dossiers.<br><br>"
                    + "<b>Rédaction</b><br>"
                    + "« Nouveau message » (Ctrl+N), « Répondre » (Ctrl+R), « Répondre à tous » "
                    + "(Ctrl+Maj+R), « Transférer » (Ctrl+F) — boutons au-dessus du message, ou clic "
                    + "droit. Un double clic répond ; dans les brouillons, il reprend le brouillon. "
                    + "Dans la fenêtre de rédaction : Ctrl+Entrée envoie, Ctrl+S enregistre le "
                    + "brouillon sur le serveur ; « Joindre… » ou un glisser-déposer de fichiers "
                    + "ajoute des pièces jointes. Une copie de chaque message envoyé est gardée dans "
                    + "« Éléments envoyés ». Mise en forme : gras (Ctrl+B), italique (Ctrl+I), souligné "
                    + "(Ctrl+U), listes, liens ; décochez « Mise en forme » pour un message en texte "
                    + "brut. En tapant un destinataire, les adresses connues sont proposées : flèches "
                    + "et Entrée pour choisir. Nom affiché et signature de chaque compte : menu "
                    + "« Comptes », « Nom et signature… » — la signature peut porter des images "
                    + "(« Image… ») ou reprendre celle d'Outlook (« Importer une signature "
                    + "Outlook… », le fichier .htm de %APPDATA%\\Microsoft\\Signatures). "
                    + "Menu « Options » : importance, accusé "
                    + "de réception, confirmation de lecture, envoi différé — le message attend "
                    + "dans « Envoi différé » et part à l'heure dite si MMail est ouvert.<br><br>"
                    + "<b>Suivi</b><br>"
                    + "Un clic au bout de la seconde ligne d'un message, ou la touche Insertion, pose "
                    + "ou retire un drapeau de suivi, que les autres logiciels de messagerie voient "
                    + "aussi. « ! » signale un message d'importance haute, « ↓ » d'importance basse. "
                    + "Quand un expéditeur demande une confirmation de lecture, MMail propose de "
                    + "l'envoyer ou de l'ignorer.<br><br>"
                    + "<b>Favoris</b><br>"
                    + "Glissez un dossier sur la rubrique Favoris pour l'y épingler ; glissez un favori "
                    + "sur un autre pour le placer avant lui. Clic droit sur un dossier : ajouter aux "
                    + "favoris ou les retirer, masquer le dossier. Le bouton « Dossiers masqués » les "
                    + "réaffiche. Le chevron d'un dossier replie ou déplie ses sous-dossiers.<br><br>"
                    + "<b>Comptes</b><br>"
                    + "Glissez l'en-tête d'un compte sur un autre pour changer leur ordre, ou clic droit "
                    + "sur le compte : « Monter », « Descendre ».<br><br>"
                    + "<b>Trier</b><br>"
                    + "Glissez un ou plusieurs messages sur un dossier, de n'importe quel compte. Clic "
                    + "droit sur un message, ou Ctrl+Maj+V : « Déplacer vers… », avec un filtre sur le "
                    + "nom du dossier. Entre deux boîtes, le message n'est retiré de la source qu'une "
                    + "fois déposé dans la cible ; un déplacement interrompu reprend à la connexion "
                    + "suivante.<br><br>"
                    + "<b>Courrier indésirable</b><br>"
                    + "« Indésirable » (Ctrl+Alt+J) range le message dans le dossier d'indésirables de "
                    + "sa boîte : le filtre du serveur l'apprend et reconnaîtra les messages semblables. "
                    + "Depuis ce dossier, « Pas indésirable » le remet en boîte de réception, appris "
                    + "comme légitime.<br><br>"
                    + "<b>Clavier</b><br>"
                    + "Suppr : envoyer à la corbeille de la boîte, rien n'est détruit · Ctrl+Q : marquer "
                    + "comme lu · Ctrl+U : marquer comme non lu · Ctrl+Maj+V : déplacer vers… · "
                    + "Ctrl+A : tout sélectionner · F5 : actualiser · F1 : cette aide. "
                    + "Ctrl+clic et Maj+clic sélectionnent plusieurs messages.<br><br>"
                    + "<b>Affichage</b><br>"
                    + "Chaque colonne a son propre zoom : Ctrl + molette sur la colonne, ou Ctrl +, "
                    + "Ctrl − et Ctrl 0 sur la dernière colonne survolée ; pincement au doigt. Le menu "
                    + "« Affichage » propose trois apparences.<br><br>"
                    + "<b>Message</b><br>"
                    + "Un message HTML s'affiche mis en page, sur fond blanc. Ses images distantes ne "
                    + "sont téléchargées qu'à la demande (« Télécharger les images ») : les télécharger "
                    + "dit à l'expéditeur que le message a été ouvert. L'adresse d'un lien survolé "
                    + "s'affiche dans la barre du bas. "
                    + "Le bouton « Source » affiche le message brut, en-têtes compris. Un texte "
                    + "sélectionné part au presse-papier, sauf ce qu'un autre logiciel vient d'y "
                    + "déposer, protégé une minute.<br><br>"
                    + "<b>Pièces jointes</b><br>"
                    + "Listées sous l'en-tête du message : « Ouvrir » avec le logiciel du système, "
                    + "« Enregistrer sous… ». Un programme ou un script ne s'ouvre pas depuis MMail, il "
                    + "s'enregistre.")
            }
        }
    }

    Dialog {
        id: dlgAPropos
        title: qsTr("À propos de MMail")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(460, fenetre.width - 24)
        standardButtons: Dialog.Close
        ColumnLayout {
            width: dlgAPropos.availableWidth
            spacing: 8
            Label {
                text: "MMail " + Qt.application.version
                font.bold: true
                font.pointSize: fenetre.font.pointSize * 1.4
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Client de messagerie IMAP pour Windows, Linux et Android.")
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("© M-Media — logiciel libre, distribué sous licence GNU GPL version 3.")
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.StyledText
                text: qsTr("Sources : %1").arg("<a href=\"https://github.com/mmedia-fr/mmail\">github.com/mmedia-fr/mmail</a>")
                onLinkActivated: function(lien) { Qt.openUrlExternally(lien) }
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                opacity: 0.7
                text: fenetre.noyau + " · Qt " + versionQt
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

    // Pictogramme d'avertissement (triangle ambre « ! »), dessiné pour ne
    // dépendre d'aucune police d'émojis. Réutilisé par les dialogues de vidage.
    component Avertissement: Canvas {
        implicitWidth: 30
        implicitHeight: 30
        Layout.alignment: Qt.AlignTop
        onPaint: {
            var c = getContext("2d")
            c.reset()
            c.fillStyle = "#E6A100"
            c.strokeStyle = "#9A6B00"
            c.lineWidth = 1
            c.beginPath()
            c.moveTo(15, 2); c.lineTo(29, 27); c.lineTo(1, 27); c.closePath()
            c.fill(); c.stroke()
            c.fillStyle = "#1A1A1A"
            c.font = "bold 19px sans-serif"
            c.textAlign = "center"; c.textBaseline = "middle"
            c.fillText("!", 15, 18)
        }
    }

    Dialog {
        id: dlgViderCorbeilles
        title: qsTr("Vider les corbeilles et indésirables ?")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(480, fenetre.width - 24)
        standardButtons: Dialog.Yes | Dialog.No
        RowLayout {
            width: dlgViderCorbeilles.availableWidth
            spacing: 12
            Avertissement {}
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Vider définitivement la corbeille et les dossiers d'indésirables (spam) de TOUS les comptes ? Les messages qui s'y trouvent sont effacés du serveur, sans repasser par une corbeille : l'action est irréversible.")
            }
        }
        onAccepted: {
            fenetre.messagesVides = 0
            boite.viderCorbeilles()
            messageEtat.texte = qsTr("Vidage des corbeilles et indésirables en cours…")
        }
    }

    Dialog {
        id: dlgViderDossier
        property int compte: 0
        property string chemin: ""
        property string nom: ""
        function ouvrir(c, ch, n) { compte = c; chemin = ch; nom = n; open() }
        title: qsTr("Vider ce dossier ?")
        modal: true
        anchors.centerIn: Overlay.overlay
        width: Math.min(480, fenetre.width - 24)
        standardButtons: Dialog.Yes | Dialog.No
        RowLayout {
            width: dlgViderDossier.availableWidth
            spacing: 12
            Avertissement {}
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Vider définitivement le dossier « %1 » ? Tous ses messages sont effacés du serveur, sans repasser par une corbeille : l'action est irréversible.").arg(dlgViderDossier.nom)
            }
        }
        onAccepted: {
            fenetre.messagesVides = 0
            boite.viderDossier(dlgViderDossier.compte, dlgViderDossier.chemin)
            messageEtat.texte = qsTr("Vidage de « %1 » en cours…").arg(dlgViderDossier.nom)
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
            fenetre.derniereSynchro = new Date()
            if (compte !== boite.compteCourant || chemin !== boite.dossierCourant)
                return
            fenetre.rafraichirListe()
            if (!veille)
                messageEtat.texte = qsTr("%1 : %2.").arg(fenetre.libelleCourant())
                        .arg(fenetre.accord(modeleMessages.count, qsTr("message"), qsTr("messages")))
            if (fenetre.essai && fenetre.essai.afficherPremier && modeleMessages.count > 0) {
                fenetre.essai.afficherPremier = false
                fenetre.choisir(0, 0)
            }
            if (fenetre.essai && fenetre.essai.scenario)
                fenetre.etapeScenario()
            if (fenetre.essai && fenetre.essai.scenario === "glisser")
                fenetre.etapeScenarioGlisser()
            if (fenetre.essai && fenetre.essai.scenario === "chevron")
                fenetre.etapeScenarioChevron()
            if (fenetre.essai && fenetre.essai.scenario.indexOf("signature") === 0)
                fenetre.etapeScenarioSignature()
        }

        function onDrapeauxModifies() {
            fenetre.rafraichirListe()
        }

        function onCorpsRecu(uid, texte, brut, html, bloquees, pieces, confirmation) {
            if (uid !== fenetre.uidCourant || brut !== fenetre.sourceVisible)
                return
            // Le format d'abord : le texte s'interprète selon lui.
            fenetre.lienSurvole = ""
            fenetre.corpsHtml = html
            fenetre.htmlAffiche = html ? texte : ""
            fenetre.imagesBloquees = bloquees
            vueCorps.text = html ? fenetre.htmlAuZoom(texte) : texte
            vueCorps.cursorPosition = 0
            if (!brut) {
                fenetre.confirmationDemandee = confirmation
                fenetre.pieces = JSON.parse(pieces)
                // Le message entier dit s'il porte des pièces : la ligne suit.
                var i = fenetre.indexDe(uid)
                if (i >= 0)
                    modeleMessages.setProperty(i, "pieces", fenetre.pieces.length > 0)
            }
            if (fenetre.essai && fenetre.essai.scenario === "pieces")
                fenetre.etapeScenarioPieces()
            // Scénario « images » : le bouton « Télécharger les images »,
            // pressé une fois.
            if (fenetre.essai && fenetre.essai.scenario === "images" && html && bloquees > 0
                    && !fenetre.essai.imagesDemandees) {
                fenetre.essai.imagesDemandees = true
                boite.afficherImages(uid)
            }
        }

        function onPreparation(jeton, contenu) {
            var r = fenetre.redactions[jeton]
            if (r)
                r.remplir(JSON.parse(contenu))
        }

        function onEnvoye(jeton, avertissement) {
            if (fenetre.essai && fenetre.essai.scenario)
                console.log("scenario: envoyé", avertissement)
            var r = fenetre.redactions[jeton]
            if (r)
                r.conteneur.fermerRedaction()
            messageEtat.texte = avertissement.length > 0
                    ? qsTr("Message envoyé — %1").arg(avertissement)
                    : qsTr("Message envoyé.")
            fenetre.rafraichirListe()
        }

        function onProgramme(jeton, echeance) {
            var r = fenetre.redactions[jeton]
            if (r)
                r.conteneur.fermerRedaction()
            messageEtat.texte = qsTr("Message programmé pour %1, dans « Envoi différé ». Il partira si MMail est ouvert à ce moment-là.")
                                .arg(fenetre.heureEnvoi(Number(echeance)))
        }

        function onDifferesEnvoyes(nombre, erreurs) {
            var texte = nombre > 0 ? qsTr("Envoi différé : %1.").arg(fenetre.accord(nombre, qsTr("message parti"), qsTr("messages partis"))) : ""
            if (erreurs.length > 0)
                texte += (texte.length > 0 ? " " : "") + qsTr("Échec : %1").arg(erreurs)
            if (texte.length > 0)
                messageEtat.texte = texte
            fenetre.rafraichirListe()
        }

        function onBrouillonEnregistre(jeton, uid) {
            var r = fenetre.redactions[jeton]
            if (!r)
                return
            r.brouillonEnregistre(uid)
        }

        function onEchecRedaction(jeton, message) {
            if (fenetre.essai && fenetre.essai.scenario)
                console.log("scenario: échec de la rédaction :", message)
            var r = fenetre.redactions[jeton]
            if (r)
                r.echouer(message)
            else
                messageEtat.texte = message
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
                messageEtat.texte = fenetre.accord(nombre, qsTr("message déplacé."), qsTr("messages déplacés."))
            if (fenetre.essai && fenetre.essai.scenario) {
                console.log("scenario: deplacement termine", nombre, erreurs)
                fenetre.etapeScenario()
                if (fenetre.essai.scenario === "glisser")
                    fenetre.etapeScenarioGlisser()
            }
        }

        function onCorbeillesVidees(nombre) {
            fenetre.messagesVides += nombre
            fenetre.rafraichirListe()
            messageEtat.texte = fenetre.accord(fenetre.messagesVides,
                qsTr("message effacé des corbeilles et indésirables."),
                qsTr("messages effacés des corbeilles et indésirables."))
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
                fenetre.afficherTexte("")
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

    /// Trombone de la liste des messages, tracé plutôt que pris d'une police :
    /// le caractère 📎 n'existe pas partout, et une icône en couleur ne
    /// suivrait ni l'apparence ni la surbrillance. Dessiné sur une grille de
    /// 10 × 16, mise à l'échelle de la ligne.
    function dessinerTrombone(ctx, w, h, encre) {
        var x = function(v) { return v * w / 10 }
        var y = function(v) { return v * h / 16 }
        ctx.reset()
        ctx.strokeStyle = encre
        ctx.lineWidth = Math.max(1.2, w * 0.13)
        ctx.lineCap = "round"
        ctx.lineJoin = "round"
        ctx.beginPath()
        ctx.moveTo(x(7.5), y(5))
        ctx.lineTo(x(7.5), y(11.5))
        ctx.arc(x(5), y(11.5), x(2.5), 0, Math.PI, false)
        ctx.lineTo(x(2.5), y(3.5))
        ctx.arc(x(4.25), y(3.5), x(1.75), Math.PI, 0, false)
        ctx.lineTo(x(6), y(10.5))
        ctx.arc(x(5), y(10.5), x(1), 0, Math.PI, false)
        ctx.lineTo(x(4), y(5.5))
        ctx.stroke()
    }

    /// Drapeau de suivi : une hampe et une flamme, pleine ou en creux.
    function dessinerDrapeau(ctx, w, h, encre, plein) {
        ctx.reset()
        ctx.strokeStyle = encre
        ctx.fillStyle = encre
        ctx.lineWidth = Math.max(1.2, w * 0.12)
        ctx.lineJoin = "round"
        ctx.beginPath()
        ctx.moveTo(w * 0.22, h * 0.08)
        ctx.lineTo(w * 0.22, h * 0.96)
        ctx.stroke()
        ctx.beginPath()
        ctx.moveTo(w * 0.22, h * 0.1)
        ctx.lineTo(w * 0.9, h * 0.3)
        ctx.lineTo(w * 0.22, h * 0.52)
        ctx.closePath()
        if (plein)
            ctx.fill()
        else
            ctx.stroke()
    }

    function basculerSuivi(uids, suivi) {
        if (uids.length === 0)
            return
        boite.marquerSuivi(uids.join(","), suivi)
        for (var i = 0; i < uids.length; ++i) {
            var j = indexDe(uids[i])
            if (j >= 0)
                modeleMessages.setProperty(j, "suivi", suivi)
        }
    }

    /// « mardi 30 septembre à 08:00 » pour une heure d'envoi (secondes Unix).
    function heureEnvoi(secondes) {
        var d = new Date(secondes * 1000)
        return d.toLocaleDateString(Qt.locale("fr_FR"), "dddd d MMMM") + qsTr(" à ")
             + Qt.formatTime(d, "hh:mm")
    }

    /// Taille de police d'une colonne, zoom compris : les menus et dialogues
    /// ouverts depuis une colonne la reprennent.
    function tailleColonne(colonne) {
        return tailleBase * zoomDe(colonne)
    }

    /// Largeur d'un menu : celle de sa plus longue entrée. Le style Fusion la
    /// fixe sinon à 200 pixels, et tronque ce qui dépasse. Appelée avant
    /// l'affichage : les entrées n'y sont pas encore visibles, d'où aucun tri
    /// sur `visible`.
    function largeurMenu(menu) {
        var largeur = 0
        for (var i = 0; i < menu.count; ++i) {
            var entree = menu.itemAt(i)
            if (entree)
                largeur = Math.max(largeur, entree.implicitWidth)
        }
        return Math.max(200, largeur + menu.leftPadding + menu.rightPadding)
    }

    function dialogueOuvert() {
        return dlgCompte.visible || dlgDeplacer.visible || dlgRetrait.visible
            || dlgAide.visible || dlgAPropos.visible || dlgLien.visible || dlgIdentite.visible
            // Sur téléphone, la rédaction couvre la fenêtre principale.
            || (compact && Object.keys(redactions).length > 0)
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
        dlgCompte.hoteSaisi = champHote.text.length > 0
        dlgCompte.infoServeur = ""
        dlgCompte.open()
    }

    /// Configuration automatique : le serveur se déduit de l'adresse, tant que
    /// la personne ne l'a pas saisi elle-même.
    function chercherServeur() {
        var adresse = champAdresse.text.trim()
        if (dlgCompte.compteId > 0 || dlgCompte.hoteSaisi || adresse.indexOf("@") < 1)
            return
        dlgCompte.infoServeur = qsTr("Recherche du serveur…")
        configuration.decouvrir(adresse)
    }

    function serveurDecouvert(adresse, hote) {
        // Réponse tardive : l'adresse a changé, ou le serveur a été saisi.
        if (!dlgCompte.visible || dlgCompte.hoteSaisi || champAdresse.text.trim() !== adresse)
            return
        if (hote.length > 0) {
            champHote.text = hote
            dlgCompte.infoServeur = qsTr("Serveur trouvé automatiquement.")
        } else {
            dlgCompte.infoServeur = qsTr("Serveur introuvable automatiquement : saisissez-le.")
        }
    }

    function lireLien(lien) {
        if (lien.length === 0)
            return
        lienEnCours = lien
        messageEtat.texte = qsTr("Lecture du lien de configuration…")
        configuration.lireLien(lien)
    }

    /// Document d'un lien de configuration (cf. README) :
    /// `{"mmail": 1, "comptes": [{"adresse", "hote", "motDePasse"}]}`.
    function appliquerLien(contenu) {
        var lien = lienEnCours
        lienEnCours = ""
        var document = null
        try {
            document = JSON.parse(contenu)
        } catch (e) {
            document = null
        }
        var comptes = document && document.mmail === 1 && Array.isArray(document.comptes)
                ? document.comptes : []
        var retenus = []
        var ecartes = []
        for (var i = 0; i < comptes.length; ++i) {
            var c = comptes[i]
            var valide = c !== null && typeof c === "object"
                    && typeof c.adresse === "string" && c.adresse.trim().indexOf("@") > 0
                    && typeof c.hote === "string" && c.hote.trim().length > 0
                    && typeof c.motDePasse === "string" && c.motDePasse.length > 0
                    && (c.port === undefined || c.port === 993)
            if (!valide) {
                ecartes.push(c && typeof c.adresse === "string" ? c.adresse : "?")
                continue
            }
            retenus.push({ adresse: c.adresse.trim(), hote: c.hote.trim().toLowerCase(),
                           motDePasse: c.motDePasse })
        }
        if (retenus.length === 0) {
            // Le lien est consommé : le rendre au dialogue ne servirait qu'à
            // montrer ce qui a été reçu.
            dlgLien.ouvrir("", qsTr("Ce lien ne contient aucun compte utilisable par MMail."))
            return
        }
        // Un compte venu d'un lien n'a pas de mot de passe connu de la
        // personne : il ne vit que par le coffre.
        reglages.memoriser = true
        for (var j = 0; j < retenus.length; ++j) {
            var r = retenus[j]
            secretsAConserver[r.adresse] = { cle: cle(r.adresse, r.hote), motDePasse: r.motDePasse }
            boite.connecter(r.hote, r.adresse, r.motDePasse)
        }
        var texte = qsTr("Lien de configuration : connexion de %1.")
                .arg(retenus.map(function(r) { return r.adresse }).join(", "))
        if (ecartes.length > 0)
            texte += " " + qsTr("Écarté : %1.").arg(ecartes.join(", "))
        messageEtat.texte = texte
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
            motif: l.motif || "",
            chemin: l.chemin || "",
            nom: l.nom || "",
            profondeur: l.profondeur || 0,
            role: l.role || "",
            selectionnable: l.selectionnable !== false,
            messages: l.messages || 0,
            nonLus: l.nonLus || 0,
            masque: l.masque === true,
            favori: l.favori === true,
            enfants: l.enfants === true,
            // Section de la liste : la rubrique Favoris, puis un groupe par compte.
            groupe: l.genre === "rubrique" || l.genre === "favori" || l.genre === "favori-vide"
                    ? "favoris" : "compte-" + (l.compte || 0)
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
        var comptes = lignes.filter(function(l) { return l.genre === "compte" })
                            .map(function(l) { return { compte: l.compte, adresse: l.adresse, hote: l.hote } })
        // Réaffecté seulement s'il change : le menu reconstruit ses entrées à
        // chaque affectation, et l'arborescence est relue à chaque veille.
        if (JSON.stringify(comptes) !== JSON.stringify(comptesConnus))
            comptesConnus = comptes
        majInfoDossier()
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
        afficherTexte(qsTr("Aucun message sélectionné."))
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
                if (avant.lu !== messages[j].lu || avant.repondu !== messages[j].repondu
                        || avant.pieces !== messages[j].pieces)
                    modeleMessages.set(j, messages[j])
            }
            majInfoDossier()
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
            afficherTexte(qsTr("Aucun message sélectionné."))
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
            messageEtat.texte = accord(nombre, qsTr("message sélectionné."), qsTr("messages sélectionnés."))
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

    // ---- rédaction

    /// Nom affiché et signature d'un compte, avec leurs valeurs par défaut.
    function identite(adresse) {
        var table = {}
        try { table = JSON.parse(reglagesRedaction.identites) } catch (e) { table = {} }
        var i = table[adresse] || {}
        return { nom: i.nom || "", signature: i.signature || "", html: i.html || "",
                 nouveaux: i.nouveaux !== false, reponses: i.reponses !== false }
    }

    function poserIdentite(adresse, valeur) {
        var table = {}
        try { table = JSON.parse(reglagesRedaction.identites) } catch (e) { table = {} }
        table[adresse] = valeur
        reglagesRedaction.identites = JSON.stringify(table)
    }

    /// Ouvre une rédaction : une fenêtre à part sur le bureau, comme Outlook ;
    /// en vue compacte (téléphone), un volet qui couvre la fenêtre principale.
    function ouvrirRedaction(compte) {
        compteurRedactions += 1
        var jeton = "r" + compteurRedactions
        var conteneur = compact ? composantVoletRedaction.createObject(fenetre)
                                : composantFenetreRedaction.createObject(fenetre)
        if (compact)
            conteneur.open()
        var r = conteneur.redaction
        r.jeton = jeton
        r.choisirCompte(compte)
        var table = redactions
        table[jeton] = r
        redactions = table
        return r
    }

    function oublierRedaction(jeton) {
        if (jeton && redactions[jeton]) {
            var table = redactions
            delete table[jeton]
            redactions = table
        }
    }

    /// `mode` : « nouveau », « repondre », « repondre_tous », « transferer »
    /// ou « brouillon » (reprise d'un brouillon du dossier ouvert).
    function rediger(mode) {
        var compte = boite.compteCourant > 0 ? boite.compteCourant
                   : (comptesConnus.length > 0 ? comptesConnus[0].compte : 0)
        if (mode === "nouveau") {
            ouvrirRedaction(compte).pret()
            return
        }
        var uids = uidsChoisis()
        var uid = uids.length === 1 ? uids[0] : uidCourant
        if (!uid) {
            messageEtat.texte = qsTr("Choisissez d'abord un message.")
            return
        }
        var r = ouvrirRedaction(compte)
        r.attendre(mode === "brouillon" ? qsTr("Reprise du brouillon…") : qsTr("Préparation…"))
        if (!boite.preparer(uid, mode, r.jeton))
            r.echouer(boite.erreur.length > 0 ? boite.erreur : qsTr("Préparation impossible."))
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
            importanceCourante = ligne.importance
            auteurAffiche.text = ligne.expediteur
                    + (ligne.adresse.length > 0 && ligne.adresse !== ligne.expediteur
                       ? " <" + ligne.adresse + ">" : "")
                    + "  ·  " + dateLongue(ligne.date)
        }
        confirmationDemandee = ""
        afficherTexte(qsTr("Chargement…"))
        boite.demanderCorps(uid)
        if (compact)
            vue = 2
    }

    /// Texte simple dans le volet du message : attente, absence, erreur.
    function afficherTexte(texte) {
        corpsHtml = false
        htmlAffiche = ""
        imagesBloquees = 0
        lienSurvole = ""
        vueCorps.text = texte
    }

    /// Le HTML du message, tailles de police mises à l'échelle du zoom de la
    /// colonne : celles que l'expéditeur a écrites ne suivraient pas, sinon,
    /// la taille du texte.
    function htmlAuZoom(html) {
        var z = reglages.zoomMessage
        if (Math.abs(z - 1) < 0.01)
            return html
        return html.replace(/((?:font-size|line-height)\s*:\s*)([0-9]*\.?[0-9]+)(pt|px)/gi,
                            function(tout, avant, valeur, unite) {
                                return avant + (Math.round(parseFloat(valeur) * z * 10) / 10) + unite
                            })
    }

    /// Réapplique le zoom à un message HTML affiché, sans perdre la position.
    function reposerHtml() {
        if (!corpsHtml || sourceVisible)
            return
        var position = cadreCorps.ScrollBar.vertical.position
        vueCorps.text = htmlAuZoom(htmlAffiche)
        cadreCorps.ScrollBar.vertical.position = position
    }
    onTailleMessageChanged: reposerHtml()

    /// Un lien du message : une adresse de courriel ouvre une rédaction dans
    /// MMail, une adresse web part au navigateur.
    function ouvrirLien(lien) {
        if (lien.toLowerCase().indexOf("mailto:") !== 0) {
            Qt.openUrlExternally(lien)
            return
        }
        var reste = lien.substring(7)
        var q = reste.indexOf("?")
        var adresse = q >= 0 ? reste.substring(0, q) : reste
        var objet = ""
        try {
            adresse = decodeURIComponent(adresse)
            var m = q >= 0 ? /(?:^|&)subject=([^&]*)/i.exec(reste.substring(q + 1)) : null
            if (m)
                objet = decodeURIComponent(m[1].replace(/\+/g, " "))
        } catch (e) {
        }
        var compte = boite.compteCourant > 0 ? boite.compteCourant
                   : (comptesConnus.length > 0 ? comptesConnus[0].compte : 0)
        ouvrirRedaction(compte).remplir({ a: adresse, objet: objet, mode: "nouveau" })
    }

    /// Signale la sélection comme indésirable — ou, depuis le dossier des
    /// indésirables, comme légitime. Le filtre du serveur l'apprend au
    /// déplacement (Mailcow : Rspamd, par IMAPSieve).
    function signalerIndesirable() {
        var uids = uidsChoisis()
        if (uids.length === 0)
            return
        var retour = roleCourant === "Junk"
        if (!boite.signalerIndesirable(uids.join(",")))
            return
        retirerDeLaListe(uids)
        var n = accord(uids.length, qsTr("message"), qsTr("messages"))
        messageEtat.texte = retour
                ? (uids.length > 1 ? qsTr("%1 remis en boîte de réception : le filtre du serveur apprend qu'ils sont légitimes.")
                                   : qsTr("%1 remis en boîte de réception : le filtre du serveur apprend qu'il est légitime.")).arg(n)
                : (uids.length > 1 ? qsTr("%1 classés indésirables : le filtre du serveur l'apprend.")
                                   : qsTr("%1 classé indésirable : le filtre du serveur l'apprend.")).arg(n)
    }

    /// Compte qui suit `compte` dans l'arborescence, ou 0 s'il est le dernier.
    function compteSuivant(compte) {
        var vu = false
        for (var i = 0; i < modeleArborescence.count; ++i) {
            var l = modeleArborescence.get(i)
            if (l.genre !== "compte")
                continue
            if (vu)
                return l.compte
            vu = l.compte === compte
        }
        return 0
    }

    function basculerSource() {
        if (uidCourant <= 0) {
            var uids = uidsChoisis()
            if (uids.length !== 1)
                return
            uidCourant = uids[0]
        }
        sourceVisible = !sourceVisible
        afficherTexte(qsTr("Chargement…"))
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
        dlgDeplacer.zoom = zoomDe(colonneActive)
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
        messageEtat.texte = qsTr("Déplacement de %1…").arg(accord(uids.length, qsTr("message"), qsTr("messages")))
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
        afficherTexte(qsTr("Aucun message sélectionné."))
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
                                       || identifiantsEssai.scenario === "pieces"
                                       || identifiantsEssai.scenario === "images"
                                       || identifiantsEssai.scenario.indexOf("signature") === 0,
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

    /// Scénario d'essai « signature » : importe la signature Outlook déposée
    /// dans le dossier de sortie (signature.htm et ses images), la pose sur le
    /// compte, rédige un message à `MMAIL_DESTINATAIRE` et l'envoie.
    function etapeScenarioSignature() {
        if (essai.signatureFaite)
            return
        essai.signatureFaite = true
        var a = comptesConnus[0]
        var html = boite.importerSignature(a.adresse, identifiantsEssai.sortie + "/signature.htm")
        console.log("scenario: signature importée :", html.length, "caractères", boite.erreur)
        poserIdentite(a.adresse, { nom: "Essai MMail", signature: "Essai MMail", html: html,
                                   nouveaux: true, reponses: true })
        if (essai.scenario === "signature-vue") {
            dlgIdentite.ouvrir(a.compte)
            return
        }
        var r = ouvrirRedaction(a.compte)
        essai.objetSignature = "Essai signature HTML " + Date.now()
        r.remplir({ a: identifiantsEssai.utilisateur2, objet: essai.objetSignature, mode: "nouveau" })
        var corps = r.htmlActuel()
        console.log("scenario: rédaction :", (corps.match(/<img[^>]*>/g) || []).join(" "))
        r.envoyer(false)
    }

    /// Scénario d'essai « chevron » : un clic sur le chevron du dossier
    /// « Essais », tel que la ligne le reçoit, doit replier ses sous-dossiers ;
    /// un second, les déplier.
    function etapeScenarioChevron() {
        if (essai.etapeChevron === undefined)
            essai.etapeChevron = 0
        // Trois clics, chacun à un tour de boucle d'événements du précédent :
        // replier « INBOX/Fournisseurs », le déplier, replier la boîte de
        // réception.
        var cibles = identifiantsEssai.dossier ? [identifiantsEssai.dossier, identifiantsEssai.dossier]
                                               : ["INBOX/Fournisseurs", "INBOX/Fournisseurs", "INBOX"]
        if (essai.etapeChevron >= cibles.length)
            return
        var present = function(chemin) {
            for (var k = 0; k < modeleArborescence.count; ++k) {
                var l = modeleArborescence.get(k)
                if (l.genre === "dossier" && l.chemin === chemin)
                    return k
            }
            return -1
        }
        var etat = function() {
            var noms = []
            for (var k = 0; k < modeleArborescence.count; ++k) {
                var l = modeleArborescence.get(k)
                if (l.genre === "dossier")
                    noms.push(l.chemin + (l.enfants ? (l.replie ? "[+]" : "[-]") : ""))
            }
            return noms.join(" | ")
        }
        var cible = cibles[essai.etapeChevron]
        var i = present(cible)
        var ligne = i >= 0 ? vueArborescence.itemAtIndex(i) : null
        // Un clic au milieu de la place du chevron, où le doigt se pose :
        // retrait du dossier plus la moitié de la largeur du chevron.
        var x = ligne ? ligne.leftPadding + modeleArborescence.get(i).profondeur * 14 + 7 : -1
        if (ligne && !ligne.surChevron(x))
            x = -1
        console.log("scenario: clic sur le chevron de", cible, "à x =", x)
        if (x < 0)
            return
        boite.replierDossier(modeleArborescence.get(i).compte, cible, !modeleArborescence.get(i).replie)
        console.log("scenario: arbre :", etat())
        essai.etapeChevron += 1
        Qt.callLater(etapeScenarioChevron)
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
        // Presse-papier : la copie explicite écrit, et la règle y reconnaît la
        // sienne — pas de protection contre MMail lui-même.
        copierTexte("essai de copie", "message")
        if (reglePresse.libelle !== "message")
            return "règle du presse-papier : origine « " + reglePresse.libelle + " »"
        if (!reglePresse.peutEcraser(maintenant()))
            return "règle du presse-papier : protection contre sa propre copie"
        if (premierePolice(["police-inexistante", "autre-inexistante"]) !== "autre-inexistante")
            return "repli de police"
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
