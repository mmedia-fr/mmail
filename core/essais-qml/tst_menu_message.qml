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
            fenetre.tactile = false
            fenetre.sourceVisible = false
            boite.annulation = ""
        }
        function sousMenu(titre) {
            for (var i = 0; i < menuMessage.count; ++i) {
                var e = menuMessage.itemAt(i)
                if (e && e.subMenu && e.subMenu.title === titre)
                    return e.subMenu
            }
            return null
        }
        function entreeNommee(menu, nom) {
            for (var i = 0; i < menu.count; ++i) {
                var e = menu.itemAt(i)
                if (e && e.objectName === nom)
                    return e
            }
            return null
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

        // Au doigt, « Source » n'est plus dans la barre (12/11) : « Autres ›
        // Afficher la source » y mène, et en ramène.
        function test_source_au_doigt() {
            var autres = sousMenu("Autres")
            verify(autres, "sous-menu « Autres »")
            var s = entreeNommee(autres, "entreeSource")
            verify(s, "entrée de la source")
            compare(s.text, "Afficher la source")
            fenetre.sourceVisible = true
            compare(s.text, "Afficher la source", "au bureau, le bouton « Message » ramène au message")
            fenetre.tactile = true
            compare(s.text, "Afficher le message", "au doigt, retour au message par le menu")
            s.triggered()
            compare(fenetre.appels, ["source"])
            verify(fenetre.sourceVisible, "la bascule part de la source affichée")
            fenetre.sourceVisible = false
            compare(s.text, "Afficher la source")
            fenetre.appels = []
            s.triggered()
            compare(fenetre.appels, ["source"])
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
