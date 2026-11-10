// SPDX-License-Identifier: GPL-3.0-or-later
// Épreuves de la ligne de la liste des messages (LigneMessage.qml) dans la vue
// tactile : ce que déclenche, ou non, un glisser sur la ligne.
//
// Lancées par la CI (qmltestrunner, plateforme offscreen) :
//   qmltestrunner -input core/essais-qml -import core/essais-qml/modules
// `modules/` ne contient que des modules vides du même nom que ceux de MMail
// (fr.mmedia.mmail, fr.mmedia.mmail.natif) : la ligne les importe sans rien y
// prendre. Les noms qu'elle emprunte à Main.qml (fenetre, boite, colonneListe,
// menuMessage) sont posés ici, réduits à ce qu'elle lit.
//
// Les gestes sont des événements de souris : sous Android, la ligne (un
// SwipeDelegate) reçoit le doigt sous cette forme. Leurs horodatages avancent
// du délai donné à chaque événement : c'est sur eux que Qt mesure la vitesse.
import QtQuick
import QtQuick.Controls
import QtTest
import "../qml" as Interface

Item {
    id: racine
    width: 412
    height: 640

    QtObject {
        id: fenetre
        property bool tactile: true
        property bool compact: true
        property bool listeMulti: false
        property string cleChoisie: ""
        property var selection: ({})
        property string roleCourant: ""
        property bool dossierDeReprise: false
        property var marque: null
        property color couleurRubrique: "#00636e"
        property color accent: "#0071e3"
        property color couleurAlerte: "#c0392b"
        property color texteAccent: "#0071e3"
        property bool anime: false
        property int dureeTeinte: 0
        property real tailleListe: 10
        property real tailleMessage: 10
        property var palette: ({ windowText: "#000000", highlightedText: "#ffffff", highlight: "#3060c0" })
        property Item contentItem: racine
        property var glisse: null
        // Ce que la ligne a demandé : « rang:genre » pour chaque glisser parti.
        property var glissements: []
        property var choix: []
        function cleResultat(compte, chemin, uid) { return compte + "|" + chemin + "|" + uid }
        function dateCourte(date) { return "" }
        function pastilles(compte, chemin, motsCles, revision) { return [] }
        function valeurOu(cle, defaut) { return defaut }
        function accord(n, un, plusieurs) { return n + " " + (n > 1 ? plusieurs : un) }
        function dessinerCorbeille() {}
        function dessinerTrombone() {}
        function dessinerDrapeau() {}
        function glisserLigne(index, genre) { glissements = glissements.concat([index + ":" + genre]) }
        function choisir(index, modificateurs) { choix = choix.concat([index]) }
        function rediger(mode) {}
        function doubleClicMessage(index) {}
        function basculerSuiviLigne() {}
        function supprimerLigne(index) {}
    }
    QtObject {
        id: boite
        property int compteCourant: 1
        property string dossierCourant: "INBOX"
        property int revisionCategories: 0
    }
    QtObject {
        id: colonneListe
        function groupeReplie(groupe) { return false }
    }
    QtObject {
        id: menuMessage
        function popup() {}
    }

    ListModel { id: modele }
    ListView {
        id: vue
        anchors.fill: parent
        model: modele
        delegate: Interface.LigneMessage {}
    }

    TestCase {
        id: essai
        name: "LigneMessage"
        when: windowShown

        function initTestCase() {
            for (var i = 0; i < 12; ++i) {
                modele.append({ groupe: "", compte: 1, chemin: "INBOX", uid: 100 + i,
                                sujet: "Objet " + i, expediteur: "expediteur" + i + "@exemple.fr",
                                destinataires: "", date: 0, lu: true, importance: 0, pieces: false,
                                repondu: false, reponduTous: false, transfere: false, suivi: false,
                                role: "", motsCles: "", cheminAffiche: "" })
            }
            verify(vue.count === 12)
        }

        function init() {
            fenetre.glissements = []
            fenetre.choix = []
            for (var i = 0; i < vue.count; ++i) {
                var l = vue.itemAtIndex(i)
                if (l)
                    l.swipe.close()
            }
            wait(300)
        }

        function ligne(rang) {
            var l = vue.itemAtIndex(rang)
            verify(l, "ligne " + rang + " absente")
            compare(l.swipe.enabled, true)
            return l
        }

        // Un geste : appui en (x, y), puis chaque pas de `pas` [dx, dy]
        // tous les `delai` ms, lâcher au dernier point.
        function geste(l, x, y, pas, delai) {
            mousePress(l, x, y, Qt.LeftButton, Qt.NoModifier, delai)
            for (var i = 0; i < pas.length; ++i) {
                x += pas[i][0]
                y += pas[i][1]
                mouseMove(l, x, y, delai, Qt.LeftButton)
            }
            mouseRelease(l, x, y, Qt.LeftButton, Qt.NoModifier, delai)
        }

        // Ce qui partait aux indésirables sans qu'on l'ait voulu : 16 px vers la
        // droite en 40 ms, en faisant défiler la liste. Qt complète le
        // glisser (plus de 300 px/s) ; la ligne doit revenir, rien ne part.
        function test_coup_bref_vers_la_droite() {
            var l = ligne(2)
            geste(l, 150, 30, [[6, 1], [6, 1], [4, 1]], 10)
            tryCompare(l.swipe, "position", 0, 3000)
            wait(200)
            compare(fenetre.glissements, [])
        }

        function test_coup_bref_vers_la_gauche() {
            var l = ligne(3)
            geste(l, 250, 30, [[-6, 1], [-6, 1], [-4, 1]], 10)
            tryCompare(l.swipe, "position", 0, 3000)
            wait(200)
            compare(fenetre.glissements, [])
        }

        // Même un coup plus ample, tant qu'il reste en deçà du tiers de la
        // largeur : 120 px en 60 ms (2 000 px/s).
        function test_coup_lance_avant_le_tiers() {
            var l = ligne(4)
            geste(l, 100, 30, [[40, -3], [40, -3], [40, -3]], 20)
            tryCompare(l.swipe, "position", 0, 3000)
            wait(200)
            compare(fenetre.glissements, [])
        }

        // Porté au-delà du tiers puis ramené en deçà avant de lâcher : la
        // personne s'est ravisée, rien ne part.
        function test_ravise_avant_de_lacher() {
            var l = ligne(5)
            geste(l, 60, 30, [[60, 0], [60, 0], [60, 0], [-50, 0], [-50, 0]], 15)
            tryCompare(l.swipe, "position", 0, 3000)
            wait(200)
            compare(fenetre.glissements, [])
        }

        // Le geste voulu, lui, part toujours : lâchée passé le tiers.
        function test_glisser_vers_la_droite() {
            var l = ligne(2)
            geste(l, 60, 30, [[40, 0], [40, 0], [40, 0], [40, 0], [20, 0]], 40)
            tryVerify(function() { return fenetre.glissements.length === 1 }, 3000)
            compare(fenetre.glissements, ["2:indesirable"])
        }

        function test_glisser_vers_la_gauche() {
            var l = ligne(6)
            geste(l, 350, 30, [[-40, 0], [-40, 0], [-40, 0], [-40, 0], [-20, 0]], 40)
            tryVerify(function() { return fenetre.glissements.length === 1 }, 3000)
            compare(fenetre.glissements, ["6:corbeille"])
        }

        // Un glisser lent et ample, lâché passé le tiers, part aussi.
        function test_glisser_lent() {
            var l = ligne(7)
            var pas = []
            for (var i = 0; i < 18; ++i)
                pas.push([10, 0])
            geste(l, 60, 30, pas, 80)
            tryVerify(function() { return fenetre.glissements.length === 1 }, 3000)
            compare(fenetre.glissements, ["7:indesirable"])
        }

        // Depuis le bord gauche, là où Android reconnaît « retour » : la
        // ligne ne bouge pas, même portée sur toute la largeur.
        function test_bord_gauche() {
            var l = ligne(8)
            geste(l, 8, 30, [[60, 0], [60, 0], [60, 0], [60, 0]], 40)
            wait(500)
            compare(l.swipe.position, 0)
            compare(fenetre.glissements, [])
        }

        function test_bord_droit() {
            var l = ligne(9)
            geste(l, racine.width - 8, 30, [[-60, 0], [-60, 0], [-60, 0], [-60, 0]], 40)
            wait(500)
            compare(l.swipe.position, 0)
            compare(fenetre.glissements, [])
        }

        // Après un appui au bord, la ligne glisse de nouveau normalement.
        function test_apres_un_appui_au_bord() {
            var l = ligne(10)
            geste(l, 8, 30, [[60, 0], [60, 0]], 40)
            wait(300)
            geste(l, 60, 30, [[40, 0], [40, 0], [40, 0], [40, 0], [20, 0]], 40)
            tryVerify(function() { return fenetre.glissements.length === 1 }, 3000)
            compare(fenetre.glissements, ["10:indesirable"])
        }

        // Un toucher bref choisit toujours le message.
        function test_toucher_choisit() {
            var l = ligne(1)
            mouseClick(l, 150, 30)
            tryVerify(function() { return fenetre.choix.length === 1 }, 2000)
            compare(fenetre.choix, [1])
            compare(fenetre.glissements, [])
        }
    }
}
