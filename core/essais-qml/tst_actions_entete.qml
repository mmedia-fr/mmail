// SPDX-License-Identifier: GPL-3.0-or-later
// Épreuves de la barre du haut du bureau (ActionsEntete.qml, barre compacte du
// 10/11) : « Nouveau message », « Actualiser » et « Marquer comme lu » en
// pictogrammes encadrés, nom complet et raccourci en infobulle ; « BàL » et
// « Masqués », noms courts de ce qui est propre à MMail, au nom complet en
// infobulle ; le nom à côté du pictogramme avec « Pictogrammes et texte »,
// sauf sur une barre trop étroite (12/11).
import QtQuick
import QtQuick.Controls
import QtTest
import "../qml" as Interface

Item {
    id: racine
    width: 900
    height: 80

    FenetreEssai { id: fenetre }
    QtObject {
        id: boite
        property var appels: []
        function actualiser() { appels = appels.concat(["actualiser"]) }
        function synchroniserAgendas(force) { appels = appels.concat(["agendas"]) }
    }
    QtObject {
        id: reglages
        property bool afficherMasques: false
    }
    Interface.ActionsEntete { id: actions }

    TestCase {
        name: "ActionsEntete"
        when: windowShown

        function enfant(nom) {
            for (var i = 0; i < actions.children.length; ++i) {
                if (actions.children[i].objectName === nom)
                    return actions.children[i]
            }
            return null
        }
        function init() {
            fenetre.appels = []
            boite.appels = []
            fenetre.etatChoix = { nonLu: false, nombre: 1, sansDrapeau: true }
            fenetre.libellesBarres = false
            fenetre.enteteSerree = false
        }

        function test_pictogrammes() {
            var attendus = [["boutonNouveau", "Nouveau message", "Nouveau message (Ctrl+N)"],
                            ["boutonActualiser", "Actualiser", "Actualiser (F5)"],
                            ["boutonMarquerLuBarre", "Marquer comme non lu", "Marquer comme non lu (Ctrl+U)"]]
            for (var i = 0; i < attendus.length; ++i) {
                var b = enfant(attendus[i][0])
                verify(b, attendus[i][0])
                verify(b.iconeSeule, attendus[i][0] + " en pictogramme")
                compare(b.text, attendus[i][1])
                compare(b.Accessible.name, attendus[i][1])
                compare(b.infobulle, attendus[i][2])
                verify(b.width < 40, attendus[i][0] + " : " + b.width + " px")
            }
            fenetre.etatChoix = { nonLu: true, nombre: 1, sansDrapeau: true }
            compare(enfant("boutonMarquerLuBarre").infobulle, "Marquer comme lu (Ctrl+Q)")
        }

        function test_noms_courts() {
            var bal = enfant("interrupteurBoites")
            compare(bal.text, "BàL")
            verify(bal.ToolTip.text.indexOf("Toutes les BàL") === 0, bal.ToolTip.text)
            var masques = enfant("interrupteurMasques")
            compare(masques.text, "Masqués")
            verify(masques.ToolTip.text.indexOf("Dossiers masqués") === 0, masques.ToolTip.text)
            verify(actions.implicitWidth < 400, "barre du haut : " + actions.implicitWidth + " px")
        }

        function test_pictogrammes_et_texte() {
            var noms = ["boutonNouveau", "boutonActualiser", "boutonMarquerLuBarre"]
            var avant = actions.implicitWidth
            fenetre.libellesBarres = true
            wait(50)
            for (var i = 0; i < noms.length; ++i) {
                var b = enfant(noms[i])
                verify(!b.iconeSeule, noms[i] + " avec son nom")
                verify(b.width > 60, noms[i] + " : " + b.width + " px")
            }
            verify(actions.implicitWidth > avant, "plus large avec les noms : " + actions.implicitWidth + " px")
            // Sur une seule ligne trop étroite : les pictogrammes seuls.
            fenetre.enteteSerree = true
            wait(50)
            for (var j = 0; j < noms.length; ++j)
                verify(enfant(noms[j]).iconeSeule, noms[j] + " en pictogramme sur une barre serrée")
        }

        function test_actions() {
            mouseClick(enfant("boutonNouveau"))
            compare(fenetre.appels, ["rediger:nouveau"])
            mouseClick(enfant("boutonActualiser"))
            compare(boite.appels, ["actualiser"])
            fenetre.appels = []
            mouseClick(enfant("boutonMarquerLuBarre"))
            compare(fenetre.appels, ["marquer:false"])
            fenetre.appels = []
            mouseClick(enfant("interrupteurBoites"))
            compare(fenetre.appels, ["bal:ouvrir"])
            mouseClick(enfant("interrupteurMasques"))
            compare(reglages.afficherMasques, true)
        }
    }
}
