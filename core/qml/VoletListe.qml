// SPDX-License-Identifier: GPL-3.0-or-later
// Volet de la liste des messages : recherche, historique, tri, liste.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages, modeleMessages…) sont ceux de
// Main.qml, où « VoletListe { id: colonneListe } » l'instancie dans les volets.
import QtQuick
import QtQml.Models
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// --- liste des messages -------------------------------------------
ColumnLayout {
    id: colonneListe
    // Désignés hors de ce volet (Main.qml, fichiers voisins) : colonneListe.<nom>.
    property alias champRecherche: champRecherche
    property alias panneauHistorique: panneauHistorique
    property alias porteeRecherche: porteeRecherche
    property alias nonLusRecherche: nonLusRecherche
    property alias vueMessages: vueMessages
    property alias molette: molette
    property alias menuGroupes: menuGroupes

    // --- groupes repliés ------------------------------------------
    // Replier un groupe de dates, comme dans Outlook (demande d'un
    // utilisateur, 03/11/2026) : un clic sur son intitulé. La vue ne montre
    // que les lignes du groupe « visibles » du modèle filtré ; d'un groupe
    // replié, seule la première y reste, sans hauteur, pour porter
    // l'intitulé. `modeleMessages` garde toutes les lignes : sélection,
    // insertions et retraits n'en savent rien. Un dossier de 40 000 messages
    // replié en « Plus ancien » ne crée pas pour autant 40 000 lignes vides.
    //
    // Groupes repliés par vue (dossier, « Toutes les BàL », recherche), le
    // temps de la session : { cleVue: { intitulé: true } }.
    property var replis: ({})
    readonly property string cleVue: (fenetre.listeMulti ? "multi" : boite.compteCourant + "|" + boite.dossierCourant)
                                     + (fenetre.recherche.length > 0 ? "|recherche" : "")
    readonly property var replisVue: replis[cleVue] || ({})
    readonly property bool replisActifs: fenetre.groupesVisibles && Object.keys(replisVue).length > 0
    onReplisActifsChanged: appliquerReplis(0)
    onCleVueChanged: if (replisActifs || groupeVisibles.count !== modeleMessages.count) appliquerReplis(0)

    function groupeReplie(groupe) {
        return replisActifs && replisVue[groupe] === true
    }
    function basculerGroupe(groupe) {
        var nouveaux = {}
        for (var g in replisVue)
            nouveaux[g] = true
        if (nouveaux[groupe])
            delete nouveaux[groupe]
        else
            nouveaux[groupe] = true
        poserReplis(nouveaux)
    }
    /// Tous les groupes, chargés ou non (un gros dossier arrive par paquets).
    function replierTout(replier) {
        var nouveaux = {}
        if (replier) {
            var bornes = fenetre.bornesGroupes()
            for (var i = 0; i < bornes.length; ++i)
                nouveaux[bornes[i][1]] = true
            nouveaux[fenetre.groupeDate(0, bornes)] = true
            for (var k = 0; k < modeleMessages.count; ++k)
                nouveaux[modeleMessages.get(k).groupe] = true
        }
        poserReplis(nouveaux)
    }
    function poserReplis(nouveaux) {
        var tous = {}
        for (var cle in replis)
            tous[cle] = replis[cle]
        if (Object.keys(nouveaux).length > 0)
            tous[cleVue] = nouveaux
        else
            delete tous[cleVue]
        replis = tous
        appliquerReplis(0)
    }

    /// Pose l'appartenance au groupe « visibles » des lignes à partir de
    /// `depuis` (celles d'avant sont justes) : un groupe est d'un seul tenant,
    /// la liste étant triée par date quand les groupes se montrent.
    function appliquerReplis(depuis) {
        var n = modeleMessages.count
        var tout = vueFiltree.items
        if (!replisActifs) {
            if (n > 0 && groupeVisibles.count !== n)
                tout.addGroups(0, n, "visibles")
            return
        }
        var replis = replisVue
        var i = Math.max(0, depuis)
        while (i < n) {
            var g = modeleMessages.get(i).groupe
            var j = i + 1
            while (j < n && modeleMessages.get(j).groupe === g)
                ++j
            if (replis[g] === true) {
                var tete = i === 0 || modeleMessages.get(i - 1).groupe !== g
                var premier = tete ? i + 1 : i
                if (tete)
                    tout.addGroups(i, 1, "visibles")
                if (j > premier)
                    tout.removeGroups(premier, j - premier, "visibles")
            } else {
                tout.addGroups(i, j - i, "visibles")
            }
            i = j
        }
    }
    // Lignes ajoutées ou retirées pendant qu'un groupe est replié : reprises
    // à partir de la première touchée, une fois la modification finie — un
    // paquet de 500 lignes ajouté en fin de liste ne relit que lui.
    property int replisDepuis: -1
    function replisChanges(retires, inseres) {
        if (!replisActifs)
            return
        var plusBas = replisDepuis < 0 ? modeleMessages.count : replisDepuis
        var listes = [retires, inseres]
        for (var l = 0; l < listes.length; ++l)
            for (var k = 0; k < listes[l].length; ++k)
                plusBas = Math.min(plusBas, listes[l][k].index)
        replisDepuis = plusBas
        Qt.callLater(reprendreReplis)
    }
    function reprendreReplis() {
        if (replisDepuis < 0)
            return
        var depuis = replisDepuis
        replisDepuis = -1
        appliquerReplis(depuis)
    }
    Connections {
        target: fenetre
        function onGroupesVisiblesChanged() { colonneListe.appliquerReplis(0) }
    }

    /// Rang dans la vue d'une ligne du modèle (-1 si elle est cachée), et
    /// l'inverse : les mêmes tant qu'aucun groupe n'est replié.
    function rangVue(i) {
        if (i < 0 || i >= modeleMessages.count || groupeVisibles.count === modeleMessages.count)
            return i
        var e = vueFiltree.items.get(i)
        return e.inVisibles ? e.visiblesIndex : -1
    }
    function rangModele(v) {
        if (v < 0 || v >= groupeVisibles.count || groupeVisibles.count === modeleMessages.count)
            return v
        return groupeVisibles.get(v).itemsIndex
    }
    /// Vrai si la ligne du modèle ne se voit pas : son groupe est replié.
    function ligneCachee(i) {
        return replisActifs && i >= 0 && i < modeleMessages.count && replisVue[modeleMessages.get(i).groupe] === true
    }

    DelegateModel {
        id: vueFiltree
        model: modeleMessages
        delegate: ligneMessage
        groups: [
            DelegateModelGroup {
                id: groupeVisibles
                name: "visibles"
                includeByDefault: true
            }
        ]
        filterOnGroup: "visibles"
        items.onChanged: function(retires, inseres) { colonneListe.replisChanges(retires, inseres) }
    }

    // Clic droit sur un intitulé de groupe.
    Menu {
        id: menuGroupes
        font.pointSize: fenetre.tailleColonne("liste")
        MenuItem {
            text: qsTr("Replier tous les groupes")
            onTriggered: colonneListe.replierTout(true)
        }
        MenuItem {
            text: qsTr("Déplier tous les groupes")
            onTriggered: colonneListe.replierTout(false)
        }
    }
    function menuDesGroupes(element, x, y) {
        var p = element.mapToItem(vueMessages, x, y)
        menuGroupes.popup(vueMessages, p.x, p.y)
    }

    // --- molette --------------------------------------------------
    // Demande d'un utilisateur, 03/11/2026 : la liste descendait trop peu par
    // cran — le défilement propre de Flickable, une impulsion vite freinée.
    // Comme Outlook et les listes du système : par cran, autant de messages
    // que le réglage du poste (« Nombre de lignes à faire défiler » sous
    // Windows, 3 par défaut), sans élan ; « un écran à la fois » fait une
    // page. Un pavé tactile ou une souris à défilement fin (pixelDelta) garde
    // la distance qu'il donne.
    function defiler(angle, pixels) {
        var v = vueMessages
        var pas = 0
        if (pixels !== 0) {
            pas = -pixels
        } else {
            var hauteur = hauteurLigne()
            var lignes = Qt.styleHints.wheelScrollLines
            var page = Math.max(hauteur, v.height - hauteur)
            var parCran = lignes > 0 ? Math.min(lignes * hauteur, page) : page
            pas = -angle / 120 * parCran
        }
        var haut = v.originY
        var bas = v.originY + Math.max(0, v.contentHeight - v.height)
        v.contentY = Math.max(haut, Math.min(bas, v.contentY + pas))
    }
    /// Hauteur d'une ligne de message : la première du haut de la vue — sous
    /// un intitulé de groupe s'il y en a un —, sinon la moyenne de la liste.
    function hauteurLigne() {
        var v = vueMessages
        for (var y = v.contentY + 1; y < v.contentY + v.height; y += 8) {
            var ligne = v.itemAt(v.width / 2, y)
            if (ligne && ligne.height > 8)
                return ligne.height
        }
        return v.count > 0 ? Math.max(20, v.contentHeight / v.count) : 40
    }
    visible: !fenetre.compact || fenetre.vue === 1
    SplitView.preferredWidth: fenetre.compact ? fenetre.width : reglages.largeurListe
    SplitView.minimumWidth: 240
    SplitView.fillWidth: fenetre.compact
    spacing: 0
    onWidthChanged: if (!fenetre.compact && visible) reglages.largeurListe = width

    // Recherche, comme dans Outlook : dans le dossier ouvert (objet,
    // expéditeur, puis texte entier par le serveur), ou dans toutes les
    // boîtes (index local).
    // Vue tactile : la recherche se montre par la loupe de la barre
    // d'application, et se range une fois vide et quittée.
    property bool rechercheOuverte: false
    // Colonne étroite (et toujours au doigt) : le champ et
    // ✕ en haut, « Non lus » et la portée dessous. Sur une seule ligne, la
    // portée gardait sa largeur, et le champ et la case se recouvraient
    // (0.8.2, 0.8.3).
    ColumnLayout {
        id: blocRecherche
        objectName: "blocRecherche"
        // Largeur d'une seule ligne : 150 points au champ, plus la case et
        // la portée à leur largeur.
        readonly property real seuil: 150 + nonLusRecherche.implicitWidth + porteeRecherche.implicitWidth + 24
        readonly property bool deuxLignes: fenetre.tactile || colonneListe.width < seuil
        visible: !fenetre.tactile || colonneListe.rechercheOuverte || champRecherche.text.length > 0
        Layout.fillWidth: true
        Layout.margins: 4
        spacing: 4
        RowLayout {
            id: ligneRecherche
            objectName: "ligneRecherche"
            Layout.fillWidth: true
            spacing: 4
            TextField {
                id: champRecherche
                MenuSaisie {}
                Layout.fillWidth: true
                placeholderText: fenetre.tactile ? qsTr("Rechercher") : qsTr("Rechercher (Ctrl+E)")
                selectByMouse: true
                // Mis en valeur (demande de Manu, 08/10) : une loupe, un
                // cadre de la couleur d'accent sur un fond teinté ; le cadre
                // s'épaissit quand le champ a la main.
                leftPadding: loupe.width + 13
                background: Rectangle {
                    implicitHeight: 30
                    radius: fenetre.marque ? height / 2 : 4
                    color: champRecherche.activeFocus ? fenetre.palette.base
                           : Qt.rgba(fenetre.accent.r, fenetre.accent.g, fenetre.accent.b, 0.08)
                    border.width: champRecherche.activeFocus ? 2 : 1
                    border.color: fenetre.accent
                }
                Canvas {
                    id: loupe
                    anchors.left: parent.left
                    anchors.leftMargin: 7
                    anchors.verticalCenter: parent.verticalCenter
                    height: Math.round(parent.height * 0.5)
                    width: height
                    property color encre: fenetre.accent
                    onEncreChanged: requestPaint()
                    onWidthChanged: requestPaint()
                    onPaint: fenetre.dessinerLoupe(getContext("2d"), width, height, encre)
                }
                // La frappe ne cherche plus : elle propose les dernières
                // recherches, que l'on choisit ; Entrée lance la recherche
                // (demande de Manu, 09/10).
                onActiveFocusChanged: {
                    if (activeFocus)
                        fenetre.proposerHistorique()
                    else if (text.length === 0)
                        colonneListe.rechercheOuverte = false
                }
                // Le mot effacé au clavier, la liste de départ revient, comme par
                // ✕ (0.6.12, demande de Fabienne : elle restait filtrée).
                onTextEdited: {
                    if (text.length === 0 && fenetre.recherche.length > 0)
                        fenetre.quitterRecherche()
                    else
                        fenetre.proposerHistorique()
                }
                onAccepted: {
                    if (panneauHistorique.opened && vueHistorique.currentIndex >= 0)
                        fenetre.lancerRecherche(panneauHistorique.entrees[vueHistorique.currentIndex])
                    else
                        fenetre.lancerRecherche()
                }
                Keys.onDownPressed: function(ev) {
                    if (panneauHistorique.opened)
                        vueHistorique.incrementCurrentIndex()
                    else
                        fenetre.proposerHistorique()
                }
                Keys.onUpPressed: if (panneauHistorique.opened) vueHistorique.decrementCurrentIndex()
                Keys.onEscapePressed: {
                    if (panneauHistorique.opened)
                        panneauHistorique.close()
                    else
                        fenetre.quitterRecherche()
                }
                // Dernières recherches sous le champ : dix visibles au plus,
                // un ascenseur au-delà.
                Popup {
                    id: panneauHistorique
                    property var entrees: []
                    y: champRecherche.height + 2
                    width: champRecherche.width
                    padding: 1
                    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
                    onAboutToShow: vueHistorique.currentIndex = -1
                    background: Rectangle {
                        color: fenetre.palette.base
                        border.color: fenetre.accent
                        radius: 3
                    }
                    contentItem: ListView {
                        id: vueHistorique
                        implicitHeight: Math.min(panneauHistorique.entrees.length, 10) * 30
                        clip: true
                        model: panneauHistorique.entrees
                        currentIndex: -1
                        ScrollBar.vertical: ScrollBar {
                            policy: panneauHistorique.entrees.length > 10 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
                        }
                        delegate: ItemDelegate {
                            width: ListView.view.width
                            height: 30
                            text: modelData
                            highlighted: ListView.isCurrentItem
                            onClicked: fenetre.lancerRecherche(modelData)
                        }
                    }
                }
            }
            ToolButton {
                text: "✕"
                visible: champRecherche.text.length > 0 || fenetre.recherche.length > 0
                onClicked: fenetre.quitterRecherche()
                ToolTip.visible: hovered
                ToolTip.text: qsTr("Effacer la recherche (Échap)")
            }
        }
        // « Non lus » et la portée : sous le champ, ou au bout de sa ligne
        // quand elle a la place — la rangée entière change de parent, et
        // garde son ordre.
        RowLayout {
            id: ligneOptionsRecherche
            objectName: "ligneOptionsRecherche"
            parent: blocRecherche.deuxLignes ? blocRecherche : ligneRecherche
            Layout.fillWidth: blocRecherche.deuxLignes
            spacing: 4
            // Non lus seulement (0.8.2), dans la portée choisie.
            CheckBox {
                id: nonLusRecherche
                objectName: "nonLusRecherche"
                text: qsTr("Non lus")
                onToggled: if (champRecherche.text.trim().length > 0) fenetre.lancerRecherche()
                ToolTip.visible: hovered
                ToolTip.delay: 600
                ToolTip.text: qsTr("Ne garder que les messages non lus")
            }
            ComboBox {
                id: porteeRecherche
                // « Cette boîte » (0.8.3) : tous les dossiers et sous-dossiers du
                // compte du dossier ouvert.
                readonly property bool partout: currentIndex === 2
                readonly property bool boiteSeule: currentIndex === 1
                readonly property bool globale: currentIndex >= 1
                model: [qsTr("Dossier actif"), qsTr("Cette boîte (tous dossiers)"), qsTr("Toutes les boîtes")]
                implicitContentWidthPolicy: ComboBox.WidestText
                // Le champ s'étire, la portée garde sa largeur : « Toutes les
                // boîtes » était tronqué.
                // Sur deux lignes, elle prend la place qui reste, quitte à
                // abréger son libellé.
                Layout.minimumWidth: blocRecherche.deuxLignes ? 0 : Math.max(implicitWidth, 150)
                Layout.fillWidth: blocRecherche.deuxLignes
                onActivated: if (champRecherche.text.trim().length > 0) fenetre.lancerRecherche()
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: 8
        Layout.rightMargin: 4
        Layout.bottomMargin: 4
        spacing: 6
        visible: fenetre.recherche.length > 0
        Label { text: qsTr("Trier par") }
        ComboBox {
            id: choixTri
            model: fenetre.trisRecherche
            textRole: "libelle"
            implicitContentWidthPolicy: ComboBox.WidestText
            function suivre() {
                var i = fenetre.trisRecherche.findIndex(function(t) { return t.cle === fenetre.triRecherche })
                if (i < 0) {
                    // « Dossier » n'a plus cours hors de toutes les boîtes.
                    fenetre.triRecherche = "recents"
                    i = 0
                }
                currentIndex = i
            }
            onModelChanged: suivre()
            Component.onCompleted: suivre()
            onActivated: fenetre.trierRecherche(fenetre.trisRecherche[currentIndex].cle)
        }
        Item { Layout.fillWidth: true }
    }

    ScrollView {
        id: voletListe
        Layout.fillWidth: true
        Layout.fillHeight: true
        ScrollBar.vertical.policy: ScrollBar.vertical.size < 1 ? ScrollBar.AlwaysOn : ScrollBar.AsNeeded
        rightPadding: ScrollBar.vertical.size < 1 ? ScrollBar.vertical.width : 0
        clip: true
        contentWidth: availableWidth
        font.pointSize: fenetre.tailleListe
        background: Rectangle { color: fenetre.palette.base }

        ListView {
            id: vueMessages
            // Les lignes des groupes repliés n'y sont pas (cf. vueFiltree) :
            // ses rangs sont ceux de la vue, cf. rangVue et rangModele.
            model: vueFiltree
            // Les lignes qui sortent de l'écran servent à celles qui entrent,
            // au lieu d'être détruites et recréées à chaque défilement — tant
            // qu'aucun groupe n'est replié (0.6.37). DelegateModel rebranche
            // une ligne recyclée sur son rang dans la vue filtrée, et non sur
            // son rang dans le modèle (Qt 6.8 à 6.10, `reuseItem(cacheItem,
            // index, …)` dans qqmldelegatemodel.cpp) : sous un groupe replié,
            // elle montre un autre message et en prend le groupe. Des lignes
            // d'un groupe déplié se cachaient, un groupe entier perdait son
            // intitulé (signalement d'un utilisateur, 04/11/2026). Sans groupe
            // replié, la vue montre tout le modèle et les deux rangs se valent.
            reuseItems: !colonneListe.replisActifs
            boundsBehavior: Flickable.StopAtBounds
            currentIndex: -1
            focus: true
            keyNavigationEnabled: false

            // Groupes par date, comme Outlook (0.6.29) : un intitulé au-dessus
            // du premier message de chaque groupe (cf. fenetre.bornesGroupes).
            // Un clic le replie ou le déplie ; le clic droit, tous.
            section.property: fenetre.groupesVisibles ? "groupe" : ""
            section.criteria: ViewSection.FullString
            section.delegate: Rectangle {
                id: enTeteGroupe
                required property string section
                readonly property bool replie: colonneListe.groupeReplie(section)
                width: vueMessages.width
                height: intituleGroupe.implicitHeight + 12
                color: fenetre.palette.base
                Accessible.role: Accessible.Button
                Accessible.name: replie ? qsTr("%1, replié").arg(section) : qsTr("%1, déplié").arg(section)
                Accessible.onPressAction: colonneListe.basculerGroupe(section)
                // « › » comme les dossiers de l'arborescence : présent dans
                // toutes les polices.
                Label {
                    id: chevronGroupe
                    anchors.left: parent.left
                    anchors.leftMargin: 8
                    anchors.verticalCenter: intituleGroupe.verticalCenter
                    width: 12
                    horizontalAlignment: Text.AlignHCenter
                    text: "›"
                    color: fenetre.texteAccent
                    font.bold: true
                    font.pointSize: intituleGroupe.font.pointSize
                    rotation: enTeteGroupe.replie ? 0 : 90
                }
                Label {
                    id: intituleGroupe
                    anchors.left: chevronGroupe.right
                    anchors.leftMargin: 4
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 4
                    text: enTeteGroupe.section
                    color: fenetre.texteAccent
                    font.bold: true
                    font.pointSize: fenetre.tailleListe * 0.85
                    elide: Text.ElideRight
                }
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: 1
                    color: fenetre.texteAccent
                    opacity: 0.35
                }
                HoverHandler { cursorShape: Qt.PointingHandCursor }
                // Vue tactile : un appui long ouvre le menu des groupes.
                TapHandler {
                    acceptedButtons: Qt.LeftButton
                    onTapped: colonneListe.basculerGroupe(enTeteGroupe.section)
                    onLongPressed: if (fenetre.tactile) colonneListe.menuDesGroupes(enTeteGroupe, 0, enTeteGroupe.height)
                }
                TapHandler {
                    acceptedButtons: Qt.RightButton
                    onTapped: function(point) {
                        colonneListe.menuDesGroupes(enTeteGroupe, point.position.x, point.position.y)
                    }
                }
            }

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
            // La molette seule : cf. colonneListe.defiler.
            WheelHandler {
                id: molette
                target: null
                acceptedModifiers: Qt.NoModifier
                orientation: Qt.Vertical
                onWheel: function(roue) {
                    colonneListe.defiler(roue.angleDelta.y, roue.pixelDelta.y)
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

}
