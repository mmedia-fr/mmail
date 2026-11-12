// SPDX-License-Identifier: GPL-3.0-or-later
// Épreuves de la barre d'actions du message (BarreMessage.qml, barre compacte
// du 10/11), au bureau puis au doigt : une seule ligne, des pictogrammes au nom
// complet et au raccourci en infobulle, « Source » en toutes lettres au bureau
// et dans « … » au doigt, « Indésirable » juste avant « Supprimer » partout
// (12/11), « … » au doigt seulement ; chaque bouton fait ce que faisait le
// bouton à libellé.
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

        function enfant(nom) {
            for (var i = 0; i < barre.children.length; ++i) {
                if (barre.children[i].objectName === nom)
                    return barre.children[i]
            }
            return null
        }
        function init() {
            fenetre.tactile = false
            fenetre.dossierDeReprise = false
            fenetre.roleCourant = ""
            fenetre.sourceVisible = false
            fenetre.etatChoix = { nonLu: false, nombre: 1, sansDrapeau: true }
            fenetre.appels = []
            menuMessage.ouvert = null
            racine.width = 900
            wait(50)
        }

        function test_bureau_une_ligne() {
            var noms = ["boutonRepondre", "boutonRepondreTous", "boutonTransferer", "boutonDeplacer",
                        "boutonMarquerLu", "boutonIndesirable", "boutonSupprimer", "boutonSource"]
            var y = -1
            for (var i = 0; i < noms.length; ++i) {
                var b = enfant(noms[i])
                verify(b, noms[i])
                verify(b.visible, noms[i] + " visible")
                if (y < 0)
                    y = b.y
                compare(b.y, y, noms[i] + " sur la même ligne")
            }
            verify(!enfant("boutonAutresActions").visible, "« … » seulement au doigt")
            verify(!enfant("boutonReprendre").visible)
            verify(barre.implicitWidth < 420, "barre compacte : " + barre.implicitWidth + " px")
            voisinDeSupprimer()
        }

        // « Indésirable » juste avant « Supprimer », sans rien entre eux.
        function voisinDeSupprimer() {
            var j = enfant("boutonIndesirable")
            var d = enfant("boutonSupprimer")
            compare(j.y, d.y, "même ligne")
            compare(d.x, j.x + j.width + barre.spacing, "« Indésirable » à gauche de « Supprimer »")
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
            compare(j.y, r.y, "sur une ligne")
            compare(autres.y, r.y, "sur une ligne")
            verify(autres.x + autres.width <= 392, "« … » dans l'écran : " + (autres.x + autres.width))
            // Téléphone de 360 points, moins les 16 de marge du volet.
            verify(barre.implicitWidth <= 344, "tient sur un téléphone : " + barre.implicitWidth + " px")
            mouseClick(j)
            compare(fenetre.appels, ["indesirable"], "« Indésirable » au doigt")
            mouseClick(autres)
            compare(menuMessage.ouvert, autres, "« … » ouvre le menu du message")
        }
    }
}
