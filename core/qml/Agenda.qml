// SPDX-License-Identifier: GPL-3.0-or-later
// Agenda : les agendas CalDAV des boîtes, en lecture.
//
// Vues jour, semaine et mois, à la manière d'Outlook : à gauche le mois et
// les agendas à cocher, à droite la vue. Le noyau rend les données toutes
// prêtes (`Boite.vueAgenda`) — occurrences calculées, chevauchements répartis
// en colonnes, textes en français — ; il ne reste ici qu'à les placer. Tout se
// lit dans l'index local : l'agenda reste consultable hors connexion, et une
// synchronisation en arrière-plan le tient à jour.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Qt.labs.settings

Pane {
    id: agenda
    padding: 0

    required property var boite
    property bool compact: false
    // « jour », « semaine » ou « mois » ; la date autour de laquelle la vue
    // est construite.
    property string genre: compact ? "jour" : "semaine"
    property date jour: new Date()
    property var donnees: ({})
    property var agendas: []
    property string erreurs: ""
    // Mois montré par le petit calendrier : il se feuillette sans déplacer la
    // vue.
    property date moisMini: new Date()
    readonly property real hauteurHeure: 46
    // Minutes écoulées depuis minuit : la ligne de l'heure courante.
    property int minutesMaintenant: 0

    readonly property color trait: Qt.rgba(palette.windowText.r, palette.windowText.g, palette.windowText.b, 0.13)
    readonly property color traitLeger: Qt.rgba(palette.windowText.r, palette.windowText.g, palette.windowText.b, 0.06)

    Settings {
        id: reglagesAgenda
        category: "agenda"
        property string genre: ""
    }

    function iso(d) {
        return Qt.formatDate(d, "yyyy-MM-dd")
    }

    function dateDe(texte) {
        var p = texte.split("-")
        return new Date(Number(p[0]), Number(p[1]) - 1, Number(p[2]))
    }

    function relire() {
        agendas = JSON.parse(boite.agendas())
        donnees = JSON.parse(boite.vueAgenda(genre, iso(jour)))
    }

    function decaler(sens) {
        var d = new Date(jour.getFullYear(), jour.getMonth(), jour.getDate())
        if (genre === "jour")
            d.setDate(d.getDate() + sens)
        else if (genre === "semaine")
            d.setDate(d.getDate() + 7 * sens)
        else {
            d.setDate(1)
            d.setMonth(d.getMonth() + sens)
        }
        jour = d
    }

    function allerAuJour(texte) {
        jour = dateDe(texte)
        genre = "jour"
    }

    function montrerFiche(indice) {
        if (!donnees.fiches || indice < 0 || indice >= donnees.fiches.length)
            return
        popupFiche.f = donnees.fiches[indice]
        popupFiche.open()
    }

    // Texte lisible sur une couleur : noir sur une couleur claire, blanc sinon.
    function encre(c) {
        return 0.299 * c.r + 0.587 * c.g + 0.114 * c.b > 0.6 ? "#000000" : "#ffffff"
    }

    function heureCourante() {
        var d = new Date()
        minutesMaintenant = d.getHours() * 60 + d.getMinutes()
    }

    onGenreChanged: {
        reglagesAgenda.genre = genre
        relire()
    }
    onJourChanged: {
        moisMini = jour
        relire()
    }
    onVisibleChanged: {
        if (visible) {
            relire()
            boite.synchroniserAgendas(false)
        }
    }
    Component.onCompleted: {
        if (reglagesAgenda.genre.length > 0)
            genre = reglagesAgenda.genre
        heureCourante()
        relire()
    }

    Timer {
        interval: 60 * 1000
        running: agenda.visible
        repeat: true
        onTriggered: agenda.heureCourante()
    }

    Connections {
        target: agenda.boite
        function onAgendasSynchronises(change, erreurs) {
            agenda.erreurs = erreurs
            if (change)
                agenda.relire()
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // --- volet gauche : le mois, les agendas ---------------------------
        ColumnLayout {
            visible: !agenda.compact
            Layout.preferredWidth: 236
            Layout.maximumWidth: 236
            Layout.fillHeight: true
            Layout.margins: 8
            spacing: 4

            RowLayout {
                Layout.fillWidth: true
                ToolButton {
                    text: "‹"
                    onClicked: agenda.moisMini = new Date(agenda.moisMini.getFullYear(), agenda.moisMini.getMonth() - 1, 1)
                }
                Label {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    font.bold: true
                    text: {
                        var t = Qt.locale("fr_FR").standaloneMonthName(agenda.moisMini.getMonth(), Locale.LongFormat)
                        return t.charAt(0).toUpperCase() + t.slice(1) + " " + agenda.moisMini.getFullYear()
                    }
                }
                ToolButton {
                    text: "›"
                    onClicked: agenda.moisMini = new Date(agenda.moisMini.getFullYear(), agenda.moisMini.getMonth() + 1, 1)
                }
            }
            DayOfWeekRow {
                Layout.fillWidth: true
                locale: Qt.locale("fr_FR")
                delegate: Label {
                    required property string shortName
                    text: shortName.charAt(0).toUpperCase()
                    horizontalAlignment: Text.AlignHCenter
                    opacity: 0.6
                    font.pixelSize: 11
                }
            }
            MonthGrid {
                id: miniMois
                Layout.fillWidth: true
                month: agenda.moisMini.getMonth()
                year: agenda.moisMini.getFullYear()
                locale: Qt.locale("fr_FR")
                delegate: Label {
                    required property var model
                    readonly property bool choisi: model.year === agenda.jour.getFullYear()
                                                   && model.month === agenda.jour.getMonth()
                                                   && model.day === agenda.jour.getDate()
                    text: model.day
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.pixelSize: 12
                    font.bold: model.today
                    opacity: model.month === miniMois.month ? 1 : 0.4
                    color: model.today ? agenda.palette.highlight : agenda.palette.windowText
                    background: Rectangle {
                        radius: height / 2
                        color: parent.choisi ? Qt.rgba(agenda.palette.highlight.r, agenda.palette.highlight.g,
                                                       agenda.palette.highlight.b, 0.22) : "transparent"
                    }
                }
                onClicked: function(date) { agenda.jour = date }
            }

            MenuSeparator { Layout.fillWidth: true }

            Loader {
                Layout.fillWidth: true
                Layout.fillHeight: true
                sourceComponent: listeAgendas
            }

            Label {
                Layout.fillWidth: true
                visible: agenda.erreurs.length > 0
                text: agenda.erreurs
                wrapMode: Text.Wrap
                color: "#b00020"
                font.pixelSize: 11
            }
        }

        Rectangle {
            visible: !agenda.compact
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            color: agenda.trait
        }

        // --- vue -----------------------------------------------------------
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            // Sur un écran étroit, le choix de la vue passe sur une seconde
            // ligne : le titre de la période garde la place de se lire.
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: 6
                Layout.bottomMargin: agenda.compact ? 0 : 6
                spacing: 4
                Button {
                    text: qsTr("Aujourd'hui")
                    onClicked: agenda.jour = new Date()
                }
                ToolButton {
                    text: "‹"
                    font.pixelSize: 18
                    onClicked: agenda.decaler(-1)
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Précédent")
                }
                ToolButton {
                    text: "›"
                    font.pixelSize: 18
                    onClicked: agenda.decaler(1)
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Suivant")
                }
                Label {
                    Layout.fillWidth: true
                    text: agenda.donnees.titre || ""
                    font.bold: true
                    font.pixelSize: 16
                    elide: Text.ElideRight
                }
                Loader {
                    active: !agenda.compact
                    visible: active
                    sourceComponent: choixGenre
                }
            }
            RowLayout {
                visible: agenda.compact
                Layout.fillWidth: true
                Layout.leftMargin: 6
                Layout.rightMargin: 6
                spacing: 4
                ToolButton {
                    text: qsTr("Agendas")
                    onClicked: popupAgendas.open()
                }
                Item { Layout.fillWidth: true }
                Loader {
                    active: agenda.compact
                    visible: active
                    sourceComponent: choixGenre
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 1
                color: agenda.trait
            }

            Loader {
                Layout.fillWidth: true
                Layout.fillHeight: true
                sourceComponent: agenda.genre === "mois" ? vueMois : vueGrille
            }

            Label {
                Layout.fillWidth: true
                Layout.margins: 6
                visible: agenda.agendas.length === 0
                wrapMode: Text.Wrap
                opacity: 0.75
                text: agenda.boite.agendaOccupe
                      ? qsTr("Recherche des agendas des boîtes…")
                      : qsTr("Aucun agenda pour l'instant. Ceux des boîtes connectées — Mailcow (SOGo) ou tout serveur CalDAV — apparaissent ici après leur synchronisation ; « Actualiser » la relance.")
            }
        }
    }

    Component {
        id: choixGenre
        RowLayout {
            spacing: 0
            ButtonGroup { id: groupeGenre }
            Repeater {
                model: [["jour", qsTr("Jour")], ["semaine", qsTr("Semaine")], ["mois", qsTr("Mois")]]
                delegate: ToolButton {
                    required property var modelData
                    text: modelData[1]
                    checkable: true
                    checked: agenda.genre === modelData[0]
                    ButtonGroup.group: groupeGenre
                    onClicked: agenda.genre = modelData[0]
                }
            }
        }
    }

    // --- agendas à cocher, groupés par boîte ---------------------------------
    Component {
        id: listeAgendas
        ListView {
            clip: true
            model: agenda.agendas
            spacing: 0
            boundsBehavior: Flickable.StopAtBounds
            // Le nom de la boîte au-dessus de son premier agenda.
            delegate: Column {
                id: ligneAgenda
                required property var modelData
                required property int index
                width: ListView.view.width
                Label {
                    visible: ligneAgenda.index === 0 || agenda.agendas[ligneAgenda.index - 1].boite !== ligneAgenda.modelData.boite
                    width: parent.width
                    text: ligneAgenda.modelData.boite
                    font.bold: true
                    font.pixelSize: 12
                    topPadding: 8
                    bottomPadding: 2
                    elide: Text.ElideRight
                }
                CheckDelegate {
                    width: parent.width
                    padding: 4
                    checked: ligneAgenda.modelData.affiche
                    onToggled: {
                        agenda.boite.afficherAgenda(ligneAgenda.modelData.id, checked)
                        agenda.relire()
                    }
                    contentItem: RowLayout {
                        spacing: 8
                        Rectangle {
                            Layout.preferredWidth: 12
                            Layout.preferredHeight: 12
                            radius: 3
                            color: ligneAgenda.modelData.couleur
                        }
                        Label {
                            Layout.fillWidth: true
                            text: ligneAgenda.modelData.nom
                            elide: Text.ElideRight
                        }
                    }
                }
            }
        }
    }

    Popup {
        id: popupAgendas
        anchors.centerIn: Overlay.overlay
        width: Math.min(agenda.width - 32, 360)
        height: Math.min(agenda.height - 64, 420)
        modal: true
        padding: 12
        contentItem: ColumnLayout {
            Loader {
                Layout.fillWidth: true
                Layout.fillHeight: true
                sourceComponent: listeAgendas
            }
            Label {
                Layout.fillWidth: true
                visible: agenda.erreurs.length > 0
                text: agenda.erreurs
                wrapMode: Text.Wrap
                color: "#b00020"
            }
            Button {
                Layout.alignment: Qt.AlignRight
                text: qsTr("Fermer")
                onClicked: popupAgendas.close()
            }
        }
    }

    // --- vue mois --------------------------------------------------------------
    Component {
        id: vueMois
        ColumnLayout {
            spacing: 0
            RowLayout {
                Layout.fillWidth: true
                spacing: 0
                Repeater {
                    model: 7
                    delegate: Label {
                        required property int index
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        horizontalAlignment: Text.AlignHCenter
                        padding: 4
                        opacity: 0.7
                        text: agenda.donnees.jours && agenda.donnees.jours.length > index ? agenda.donnees.jours[index].semaine : ""
                    }
                }
            }
            GridLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                columns: 7
                columnSpacing: 0
                rowSpacing: 0
                Repeater {
                    model: agenda.donnees.jours ? agenda.donnees.jours.length : 0
                    delegate: Rectangle {
                        id: caseMois
                        required property int index
                        readonly property var info: agenda.donnees.jours[index]
                        readonly property var entrees: agenda.donnees.entrees ? agenda.donnees.entrees[index] : []
                        // Lignes qui tiennent sous le numéro du jour.
                        readonly property int place: Math.max(1, Math.floor((height - 24) / 18))
                        readonly property int montrees: entrees.length > place ? place - 1 : entrees.length
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.preferredWidth: 1
                        Layout.preferredHeight: 1
                        color: info.horsMois ? Qt.darker(agenda.palette.base, 1.04) : agenda.palette.base
                        border.width: 1
                        border.color: agenda.traitLeger

                        MouseArea {
                            anchors.fill: parent
                            onDoubleClicked: agenda.allerAuJour(caseMois.info.date)
                        }
                        Rectangle {
                            x: parent.width - width - 4
                            y: 3
                            width: Math.max(20, numero.implicitWidth + 8)
                            height: 20
                            radius: 10
                            color: caseMois.info.aujourdhui ? agenda.palette.highlight : "transparent"
                            Label {
                                id: numero
                                anchors.centerIn: parent
                                text: caseMois.info.jour
                                font.bold: caseMois.info.aujourdhui
                                opacity: caseMois.info.horsMois ? 0.45 : 1
                                color: caseMois.info.aujourdhui ? agenda.palette.highlightedText : agenda.palette.windowText
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: agenda.allerAuJour(caseMois.info.date)
                            }
                        }
                        Column {
                            x: 3
                            y: 25
                            width: parent.width - 6
                            spacing: 1
                            Repeater {
                                model: caseMois.montrees
                                delegate: Rectangle {
                                    id: entree
                                    required property int index
                                    readonly property var e: caseMois.entrees[index]
                                    readonly property var f: agenda.donnees.fiches[e.fiche]
                                    readonly property color teinte: f.couleur
                                    width: parent.width
                                    height: 17
                                    radius: 3
                                    color: e.heure.length === 0 ? teinte : "transparent"
                                    Row {
                                        anchors.verticalCenter: parent.verticalCenter
                                        x: 3
                                        width: parent.width - 6
                                        spacing: 4
                                        Rectangle {
                                            visible: entree.e.heure.length > 0
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: 7
                                            height: 7
                                            radius: 4
                                            color: entree.teinte
                                        }
                                        Label {
                                            width: parent.width - (entree.e.heure.length > 0 ? 11 : 0)
                                            // Sur un écran étroit, le titre seul : l'heure
                                            // prendrait toute la case.
                                            text: (entree.e.heure.length > 0 && !agenda.compact ? entree.e.heure + " " : "") + entree.f.titre
                                            font.pixelSize: 11
                                            font.strikeout: entree.f.annule
                                            elide: Text.ElideRight
                                            color: entree.e.heure.length === 0 ? agenda.encre(entree.teinte) : agenda.palette.windowText
                                        }
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: agenda.montrerFiche(entree.e.fiche)
                                    }
                                }
                            }
                            Label {
                                visible: caseMois.entrees.length > caseMois.montrees
                                readonly property int reste: caseMois.entrees.length - caseMois.montrees
                                text: reste > 1 ? qsTr("+ %1 autres").arg(reste) : qsTr("+ 1 autre")
                                font.pixelSize: 11
                                color: agenda.palette.highlight
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: agenda.allerAuJour(caseMois.info.date)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // --- vues jour et semaine ----------------------------------------------------
    Component {
        id: vueGrille
        ColumnLayout {
            id: grille
            spacing: 0
            readonly property int nbJours: agenda.donnees.jours ? agenda.donnees.jours.length : 0
            // Colonne des heures, à gauche.
            readonly property real marge: 50
            readonly property real largeurJour: nbJours > 0 ? (width - marge - 12) / nbJours : 0
            readonly property real hh: agenda.hauteurHeure

            // En-têtes des jours.
            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: 28
                Repeater {
                    model: grille.nbJours
                    delegate: Label {
                        required property int index
                        readonly property var info: agenda.donnees.jours[index]
                        x: grille.marge + index * grille.largeurJour
                        width: grille.largeurJour
                        height: parent.height
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        text: info.semaine + " " + info.jour
                        font.bold: info.aujourdhui
                        color: info.aujourdhui ? agenda.palette.highlight : agenda.palette.windowText
                        MouseArea {
                            anchors.fill: parent
                            enabled: grille.nbJours > 1
                            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: agenda.allerAuJour(parent.info.date)
                        }
                    }
                }
            }

            // Bandeau des journées entières et des événements de plus d'un jour.
            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: (agenda.donnees.lignes || 0) * 22 + ((agenda.donnees.lignes || 0) > 0 ? 4 : 0)
                visible: (agenda.donnees.lignes || 0) > 0
                Repeater {
                    model: agenda.donnees.bandes || []
                    delegate: Rectangle {
                        id: bande
                        required property var modelData
                        readonly property var f: agenda.donnees.fiches[modelData.fiche]
                        readonly property color teinte: f.couleur
                        x: grille.marge + modelData.de * grille.largeurJour + 1
                        y: 2 + modelData.ligne * 22
                        width: (modelData.a - modelData.de) * grille.largeurJour - 2
                        height: 20
                        radius: 3
                        color: teinte
                        Label {
                            anchors.fill: parent
                            anchors.leftMargin: 6
                            anchors.rightMargin: 4
                            verticalAlignment: Text.AlignVCenter
                            text: bande.f.titre
                            font.pixelSize: 12
                            font.strikeout: bande.f.annule
                            elide: Text.ElideRight
                            color: agenda.encre(bande.teinte)
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: agenda.montrerFiche(bande.modelData.fiche)
                        }
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 1
                color: agenda.trait
            }

            Flickable {
                id: defilement
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: width
                contentHeight: 24 * grille.hh
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                // Au départ, la journée de travail : de 7 h 30, ou d'une
                // demi-heure avant le premier événement s'il commence plus tôt.
                // Période pour laquelle le défilement a été placé : une
                // synchronisation qui relit la même période ne le déplace pas.
                property string periodePlacee: ""
                function placer() {
                    var periode = agenda.genre + agenda.iso(agenda.jour)
                    if (periode === periodePlacee)
                        return
                    periodePlacee = periode
                    var depart = 7.5
                    var cases = agenda.donnees.cases || []
                    for (var i = 0; i < cases.length; ++i)
                        depart = Math.min(depart, cases[i].debut / 60 - 0.5)
                    contentY = Math.max(0, Math.min(contentHeight - height, depart * grille.hh))
                }
                Component.onCompleted: placer()
                Connections {
                    target: agenda
                    function onDonneesChanged() { defilement.placer() }
                }

                // Heures et demi-heures.
                Repeater {
                    model: 24
                    delegate: Item {
                        required property int index
                        y: index * grille.hh
                        width: defilement.width
                        height: grille.hh
                        Label {
                            visible: index > 0
                            x: 4
                            y: -height / 2
                            width: grille.marge - 10
                            horizontalAlignment: Text.AlignRight
                            text: (index < 10 ? "0" : "") + index + ":00"
                            font.pixelSize: 11
                            opacity: 0.65
                        }
                        Rectangle {
                            x: grille.marge
                            width: parent.width - grille.marge
                            height: 1
                            color: agenda.trait
                        }
                        Rectangle {
                            x: grille.marge
                            y: grille.hh / 2
                            width: parent.width - grille.marge
                            height: 1
                            color: agenda.traitLeger
                        }
                    }
                }
                // Séparations des jours, et le jour courant teinté.
                Repeater {
                    model: grille.nbJours
                    delegate: Rectangle {
                        required property int index
                        x: grille.marge + index * grille.largeurJour
                        width: grille.largeurJour
                        height: defilement.contentHeight
                        color: agenda.donnees.jours[index].aujourdhui
                               ? Qt.rgba(agenda.palette.highlight.r, agenda.palette.highlight.g, agenda.palette.highlight.b, 0.05)
                               : "transparent"
                        Rectangle {
                            width: 1
                            height: parent.height
                            color: agenda.trait
                        }
                    }
                }
                // Événements.
                Repeater {
                    model: agenda.donnees.cases || []
                    delegate: Rectangle {
                        id: caseHeure
                        required property var modelData
                        readonly property var f: agenda.donnees.fiches[modelData.fiche]
                        readonly property color teinte: f.couleur
                        readonly property real largeurColonne: (grille.largeurJour - 4) / modelData.colonnes
                        x: grille.marge + modelData.jour * grille.largeurJour + 2 + modelData.colonne * largeurColonne
                        y: modelData.debut / 60 * grille.hh + 1
                        width: largeurColonne - 2
                        height: Math.max(16, (modelData.fin - modelData.debut) / 60 * grille.hh - 2)
                        radius: 4
                        clip: true
                        color: Qt.tint(agenda.palette.base, Qt.rgba(teinte.r, teinte.g, teinte.b, 0.28))
                        border.width: 1
                        border.color: Qt.rgba(teinte.r, teinte.g, teinte.b, 0.7)
                        Rectangle {
                            width: 4
                            height: parent.height
                            radius: 2
                            color: caseHeure.teinte
                        }
                        Column {
                            x: 8
                            y: 2
                            width: parent.width - 10
                            Label {
                                width: parent.width
                                text: caseHeure.f.titre
                                font.bold: true
                                font.pixelSize: 12
                                font.strikeout: caseHeure.f.annule
                                elide: Text.ElideRight
                                maximumLineCount: caseHeure.height > 50 ? 2 : 1
                                wrapMode: Text.Wrap
                            }
                            Label {
                                width: parent.width
                                visible: caseHeure.height > 34 && caseHeure.modelData.heures.length > 0
                                text: caseHeure.modelData.heures
                                font.pixelSize: 11
                                opacity: 0.8
                                elide: Text.ElideRight
                            }
                            Label {
                                width: parent.width
                                visible: caseHeure.height > 52 && caseHeure.f.lieu.length > 0
                                text: caseHeure.f.lieu
                                font.pixelSize: 11
                                opacity: 0.8
                                elide: Text.ElideRight
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: agenda.montrerFiche(caseHeure.modelData.fiche)
                        }
                    }
                }
                // L'heure courante, sur la colonne du jour.
                Repeater {
                    model: grille.nbJours
                    delegate: Rectangle {
                        required property int index
                        visible: agenda.donnees.jours[index].aujourdhui
                        x: grille.marge + index * grille.largeurJour
                        y: agenda.minutesMaintenant / 60 * grille.hh
                        width: grille.largeurJour
                        height: 2
                        color: "#d9534f"
                        Rectangle {
                            x: -4
                            y: -3
                            width: 8
                            height: 8
                            radius: 4
                            color: "#d9534f"
                        }
                    }
                }
            }
        }
    }

    // --- fiche d'un événement ------------------------------------------------------
    Popup {
        id: popupFiche
        property var f: ({})
        anchors.centerIn: Overlay.overlay
        width: Math.min(agenda.width - 32, 480)
        modal: true
        focus: true
        padding: 16
        contentItem: ColumnLayout {
            spacing: 8
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Rectangle {
                    Layout.preferredWidth: 12
                    Layout.preferredHeight: 12
                    Layout.alignment: Qt.AlignTop
                    Layout.topMargin: 5
                    radius: 3
                    color: popupFiche.f.couleur || "transparent"
                }
                Label {
                    Layout.fillWidth: true
                    text: popupFiche.f.titre || ""
                    font.bold: true
                    font.pixelSize: 17
                    font.strikeout: popupFiche.f.annule || false
                    wrapMode: Text.Wrap
                }
            }
            Label {
                visible: popupFiche.f.annule || false
                text: qsTr("Annulé par l'organisateur")
                color: "#b00020"
            }
            Label {
                Layout.fillWidth: true
                text: popupFiche.f.quand || ""
                wrapMode: Text.Wrap
            }
            Label {
                Layout.fillWidth: true
                visible: (popupFiche.f.lieu || "").length > 0
                text: qsTr("Lieu : %1").arg(popupFiche.f.lieu || "")
                wrapMode: Text.Wrap
            }
            Label {
                Layout.fillWidth: true
                text: qsTr("Agenda : %1").arg(popupFiche.f.nomAgenda || "")
                opacity: 0.75
                wrapMode: Text.Wrap
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(texteFiche.implicitHeight, 260)
                visible: (popupFiche.f.description || "").length > 0
                TextArea {
                    id: texteFiche
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextArea.Wrap
                    text: popupFiche.f.description || ""
                    background: null
                }
            }
            Button {
                Layout.alignment: Qt.AlignRight
                text: qsTr("Fermer")
                onClicked: popupFiche.close()
            }
        }
    }
}
