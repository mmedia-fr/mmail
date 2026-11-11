// SPDX-License-Identifier: GPL-3.0-or-later
// Épreuves du menu d'un message (MenuMessage.qml, barre compacte du 10/11) :
// une rangée de pictogrammes en tête (répondre, répondre à tous, transférer,
// suivi, supprimer), puis des libellés courts — sept lignes au lieu de onze ;
// un pictogramme ferme le menu et fait l'action.
import QtQuick
import QtQuick.Controls
import QtTest
import "../qml" as Interface

Item {
    id: racine
    width: 600
    height: 600

    FenetreEssai { id: fenetre }
    QtObject {
        id: boite
        property string annulation: ""
    }
    QtObject {
        id: impression
        property bool disponible: true
    }
    Interface.MenuMessage { id: menuMessage }

    TestCase {
        name: "MenuMessage"
        when: windowShown

        function trouver(racineObjet, nom) {
            if (!racineObjet)
                return null
            if (racineObjet.objectName === nom)
                return racineObjet
            var enfants = racineObjet.children || []
            for (var i = 0; i < enfants.length; ++i) {
                var o = trouver(enfants[i], nom)
                if (o)
                    return o
            }
            return null
        }
        function lignes() {
            var textes = []
            for (var i = 0; i < menuMessage.count; ++i) {
                var e = menuMessage.itemAt(i)
                if (!e || !e.visible || e.height <= 0 || e instanceof MenuSeparator)
                    continue
                textes.push(e.objectName === "rangeePictogrammes" ? "[pictogrammes]" : e.text)
            }
            return textes
        }
        function ouvrir() {
            menuMessage.popup(racine, 10, 10)
            tryCompare(menuMessage, "opened", true)
        }
        function cleanup() {
            menuMessage.close()
            tryCompare(menuMessage, "visible", false)
            fenetre.appels = []
            fenetre.dossierDeReprise = false
            boite.annulation = ""
        }

        function test_sept_lignes() {
            ouvrir()
            compare(lignes(), ["[pictogrammes]", "Marquer non lu", "Catégoriser", "Déplacer…", "Indésirable",
                               "Sélection", "Autres"])
            boite.annulation = "Annuler la suppression"
            compare(lignes().length, 8, "l'annulation, quand il y a de quoi")
        }

        function test_rangee_de_pictogrammes() {
            ouvrir()
            var rangee = trouver(menuMessage.contentItem, "rangeePictogrammes")
            verify(rangee, "rangée en tête")
            compare(menuMessage.itemAt(0), rangee)
            var attendus = [["menuRepondre", "Répondre (Ctrl+R)"], ["menuRepondreTous", "Répondre à tous (Ctrl+Maj+R)"],
                            ["menuTransferer", "Transférer (Ctrl+F)"], ["menuSuivi", "Poser un drapeau de suivi (Inser)"],
                            ["menuSupprimer", "Supprimer (Suppr)"]]
            for (var i = 0; i < attendus.length; ++i) {
                var b = trouver(rangee, attendus[i][0])
                verify(b && b.visible, attendus[i][0])
                compare(b.infobulle, attendus[i][1])
            }
        }

        function test_pictogramme_ferme_et_agit() {
            var cas = [["menuRepondre", "rediger:repondre"], ["menuTransferer", "rediger:transferer"],
                       ["menuSuivi", "suivi:true"], ["menuSupprimer", "supprimer"]]
            for (var i = 0; i < cas.length; ++i) {
                ouvrir()
                fenetre.appels = []
                mouseClick(trouver(menuMessage.contentItem, cas[i][0]))
                tryCompare(menuMessage, "visible", false)
                compare(fenetre.appels, [cas[i][1]], cas[i][0])
            }
        }

        function test_brouillon() {
            fenetre.dossierDeReprise = true
            ouvrir()
            var l = lignes()
            compare(l[1], "Reprendre")
            verify(!trouver(menuMessage.contentItem, "menuRepondre").visible)
            verify(l.indexOf("Indésirable") < 0)
        }
    }
}
