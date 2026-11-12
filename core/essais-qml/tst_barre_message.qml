// SPDX-License-Identifier: GPL-3.0-or-later
// Épreuves de la barre d'actions du message (BarreMessage.qml, barre compacte
// du 10/11), au bureau puis au doigt : une seule ligne, des pictogrammes au nom
// complet et au raccourci en infobulle, « Source » en toutes lettres au bureau
// et dans « … » au doigt, « Indésirable » juste avant « Supprimer » partout
// (12/11), « … » au doigt seulement ; chaque bouton fait ce que faisait le
// bouton à libellé. Ordre du 12/11 : répondre, répondre à tous, transférer |
// lu ou non lu, déplacer | indésirable, supprimer | source ; le nom à côté
// du pictogramme avec « Pictogrammes et texte », à la ligne s'il le faut.
import QtQuick
import QtQuick.Controls
import QtTest
import "../qml" as Interface

Item {
    id: racine
    width: 900
    height: 200

    FenetreEssai { id: fenetre }
    QtObject {
        id: menuMessage
        property var ouvert: null
        function popup(parent, x, y) { ouvert = parent }
    }
    Interface.BarreMessage {
        id: barre
        width: racine.width
    }

    TestCase {
        name: "BarreMessage"
        when: windowShown

        // Un élément de la barre, rangées des groupes comprises.
        function enfant(nom, depuis) {
            var parent = depuis || barre
            for (var i = 0; i < parent.children.length; ++i) {
                var e = parent.children[i]
                if (e.objectName === nom)
                    return e
                var trouve = enfant(nom, e)
                if (trouve)
                    return trouve
            }
            return null
        }
        // Position dans le repère de la barre.
        function pos(e) {
            return e.mapToItem(barre, 0, 0)
        }
        function init() {
            fenetre.tactile = false
            fenetre.dossierDeReprise = false
            fenetre.roleCourant = ""
            fenetre.sourceVisible = false
            fenetre.etatChoix = { nonLu: false, nombre: 1, sansDrapeau: true }
            fenetre.libellesBarres = false
            fenetre.appels = []
            menuMessage.ouvert = null
            racine.width = 900
            wait(50)
        }

        // Les éléments visibles, de gauche à droite, sur une même ligne.
        function dansLOrdre(noms) {
            for (var i = 1; i < noms.length; ++i) {
                var a = enfant(noms[i - 1])
                var b = enfant(noms[i])
                verify(a && a.visible, noms[i - 1])
                verify(b && b.visible, noms[i])
                compare(pos(b).y + b.height / 2, pos(a).y + a.height / 2, noms[i] + " sur la ligne de " + noms[i - 1])
                verify(pos(b).x > pos(a).x + a.width - 1, noms[i] + " après " + noms[i - 1])
            }
        }

        function test_bureau_une_ligne() {
            var noms = ["boutonRepondre", "boutonRepondreTous", "boutonTransferer", "boutonMarquerLu",
                        "boutonDeplacer", "boutonIndesirable", "boutonSupprimer", "boutonSource"]
            var y = -1
            for (var i = 0; i < noms.length; ++i) {
                var b = enfant(noms[i])
                verify(b, noms[i])
                verify(b.visible, noms[i] + " visible")
                if (y < 0)
                    y = pos(b).y
                compare(pos(b).y, y, noms[i] + " sur la même ligne")
            }
            verify(!enfant("boutonAutresActions").visible, "« … » seulement au doigt")
            verify(!enfant("boutonReprendre").visible)
            verify(barre.largeurBoutons < 420, "barre compacte : " + barre.largeurBoutons + " px")
            compare(barre.implicitHeight, enfant("boutonSupprimer").height, "une seule ligne")
            voisinDeSupprimer()
        }

        // Ordre du mainteneur (12/11), en quatre groupes séparés d'un trait.
        function test_ordre_bureau() {
            dansLOrdre(["boutonRepondre", "boutonRepondreTous", "boutonTransferer", "separateurRepondre",
                        "boutonMarquerLu", "boutonDeplacer", "separateurRanger",
                        "boutonIndesirable", "boutonSupprimer", "separateurSource", "boutonSource"])
            // Le trait est à la hauteur des boutons : la ligne n'en grandit pas.
            compare(enfant("separateurRanger").height, enfant("boutonSupprimer").height)
        }

        function test_ordre_au_doigt() {
            fenetre.tactile = true
            racine.width = 392
            wait(50)
            dansLOrdre(["boutonRepondre", "boutonRepondreTous", "boutonTransferer", "boutonMarquerLu",
                        "boutonDeplacer", "boutonIndesirable", "boutonSupprimer", "boutonAutresActions"])
            verify(!enfant("separateurRepondre").visible, "pas de trait au doigt")
            verify(!enfant("separateurRanger").visible, "pas de trait au doigt")
            verify(!enfant("separateurSource").visible, "pas de trait au doigt")
        }

        // Pictogrammes et texte : le nom à côté de chaque dessin ; « Source »
        // reste un nom court.
        function test_pictogrammes_et_texte() {
            var noms = ["boutonRepondre", "boutonRepondreTous", "boutonTransferer", "boutonMarquerLu",
                        "boutonDeplacer", "boutonIndesirable", "boutonSupprimer"]
            var largeurs = []
            for (var i = 0; i < noms.length; ++i) {
                verify(!enfant(noms[i]).libelleVisible, noms[i] + " : pictogramme seul d'office")
                largeurs.push(enfant(noms[i]).width)
            }
            fenetre.libellesBarres = true
            racine.width = 1400
            wait(50)
            for (var j = 0; j < noms.length; ++j) {
                var b = enfant(noms[j])
                verify(b.libelleVisible, noms[j] + " avec son nom")
                verify(!!b.dessin, noms[j] + " garde son dessin")
                verify(b.width > largeurs[j] + 20, noms[j] + " : " + b.width + " px")
            }
            compare(enfant("boutonSource").court, "Source")
            dansLOrdre(["boutonRepondre", "boutonRepondreTous", "boutonTransferer", "separateurRepondre",
                        "boutonMarquerLu", "boutonDeplacer", "separateurRanger",
                        "boutonIndesirable", "boutonSupprimer", "separateurSource", "boutonSource"])
            mouseClick(enfant("boutonDeplacer"))
            compare(fenetre.appels, ["deplacer"], "« Déplacer… » avec son nom")
        }

        // Un volet trop étroit pour les noms : la barre passe à la ligne, sans
        // rien couper ni déborder.
        function test_pictogrammes_et_texte_a_la_ligne() {
            fenetre.libellesBarres = true
            racine.width = 420
            wait(50)
            var noms = ["boutonRepondre", "boutonRepondreTous", "boutonTransferer", "boutonMarquerLu",
                        "boutonDeplacer", "boutonIndesirable", "boutonSupprimer", "boutonSource"]
            var lignes = {}
            for (var i = 0; i < noms.length; ++i) {
                var b = enfant(noms[i])
                verify(b.visible, noms[i])
                verify(pos(b).x + b.width <= barre.width, noms[i] + " dans le volet : " + (pos(b).x + b.width))
                lignes[pos(b).y] = true
            }
            verify(Object.keys(lignes).length > 1, "sur plusieurs lignes")
            // Jamais au milieu d'un groupe.
            var groupes = [["boutonRepondre", "boutonRepondreTous", "boutonTransferer"],
                           ["boutonMarquerLu", "boutonDeplacer"], ["boutonIndesirable", "boutonSupprimer"]]
            for (var g = 0; g < groupes.length; ++g)
                dansLOrdre(groupes[g])
            verify(barre.implicitHeight > enfant("boutonSupprimer").height, "la barre grandit : " + barre.implicitHeight)
        }

        // « Indésirable » juste avant « Supprimer », sans rien entre eux.
        function voisinDeSupprimer() {
            var j = enfant("boutonIndesirable")
            var d = enfant("boutonSupprimer")
            compare(pos(j).y, pos(d).y, "même ligne")
            compare(pos(d).x, pos(j).x + j.width + barre.spacing, "« Indésirable » à gauche de « Supprimer »")
        }

        function test_infobulles_et_noms() {
            var r = enfant("boutonRepondre")
            compare(r.text, "Répondre")
            compare(r.Accessible.name, "Répondre")
            compare(r.infobulle, "Répondre (Ctrl+R)")
            compare(enfant("boutonRepondreTous").infobulle, "Répondre à tous (Ctrl+Maj+R)")
            compare(enfant("boutonTransferer").infobulle, "Transférer (Ctrl+F)")
            compare(enfant("boutonSupprimer").infobulle, "Supprimer (Suppr)")
            compare(enfant("boutonMarquerLu").text, "Marquer comme non lu")
            compare(enfant("boutonMarquerLu").infobulle, "Marquer comme non lu (Ctrl+U)")
            fenetre.etatChoix = { nonLu: true, nombre: 1, sansDrapeau: true }
            compare(enfant("boutonMarquerLu").infobulle, "Marquer comme lu (Ctrl+Q)")
            compare(enfant("boutonIndesirable").infobulle, "Courrier indésirable ; le filtre du serveur l'apprend (Ctrl+Alt+J)")
            fenetre.roleCourant = "Junk"
            compare(enfant("boutonIndesirable").text, "Pas indésirable")
        }

        function test_source_en_toutes_lettres() {
            var s = enfant("boutonSource")
            compare(s.court, "Source")
            compare(s.dessin, null)
            fenetre.sourceVisible = true
            compare(s.court, "Message")
        }

        function test_actions() {
            var attendu = [["boutonRepondre", "rediger:repondre"], ["boutonRepondreTous", "rediger:repondre_tous"],
                           ["boutonTransferer", "rediger:transferer"], ["boutonDeplacer", "deplacer"],
                           ["boutonIndesirable", "indesirable"], ["boutonMarquerLu", "marquer:false"],
                           ["boutonSupprimer", "supprimer"], ["boutonSource", "source"]]
            for (var i = 0; i < attendu.length; ++i) {
                fenetre.appels = []
                mouseClick(enfant(attendu[i][0]))
                compare(fenetre.appels, [attendu[i][1]], attendu[i][0])
            }
        }

        function test_brouillon() {
            fenetre.dossierDeReprise = true
            verify(enfant("boutonReprendre").visible)
            verify(!enfant("boutonRepondre").visible)
            verify(!enfant("boutonIndesirable").visible)
            mouseClick(enfant("boutonReprendre"))
            compare(fenetre.appels, ["rediger:brouillon"])
        }

        function test_au_doigt() {
            fenetre.tactile = true
            racine.width = 392
            wait(50)
            var j = enfant("boutonIndesirable")
            verify(j.visible, "« Indésirable » dans la barre au doigt")
            verify(!!j.dessin, "« Indésirable » en pictogramme")
            voisinDeSupprimer()
            verify(!enfant("boutonSource").visible, "« Source » dans « … » au doigt")
            var autres = enfant("boutonAutresActions")
            verify(autres.visible)
            compare(autres.text, "Autres actions")
            var r = enfant("boutonRepondre")
            verify(r.width >= 40 && r.height >= 40, "cible du doigt : " + r.width + " × " + r.height)
            compare(pos(j).y, pos(r).y, "sur une ligne")
            compare(pos(autres).y, pos(r).y, "sur une ligne")
            verify(autres.x + autres.width <= 392, "« … » dans l'écran : " + (autres.x + autres.width))
            verify(autres.x + autres.width >= 392 - 4, "« … » au bord droit : " + (autres.x + autres.width))
            // Téléphone de 360 points, moins les 16 de marge du volet.
            verify(barre.largeurBoutons <= 344, "tient sur un téléphone : " + barre.largeurBoutons + " px")
            mouseClick(j)
            compare(fenetre.appels, ["indesirable"], "« Indésirable » au doigt")
            mouseClick(autres)
            compare(menuMessage.ouvert, autres, "« … » ouvre le menu du message")
        }
    }
}
