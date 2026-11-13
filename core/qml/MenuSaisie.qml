// SPDX-License-Identifier: GPL-3.0-or-later
// Menu du clic droit d'un champ de saisie : Annuler, Couper, Copier, Coller,
// Tout sélectionner, comme dans les champs de Windows (0.6.14, demande du
// mainteneur du 22/10 : coller à la souris dans la recherche). Qt 6.8 n'en donne
// aucun d'office.
//
// Se déclare dans le champ lui-même — « MenuSaisie {} » — : le champ est
// l'élément qui le porte, TextField ou TextArea. Bureau seulement : au doigt,
// la barre d'Android s'en charge.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

TapHandler {
    id: clicDroit
    readonly property Item champ: parent
    // Collage propre au champ (le corps d'une rédaction colle aussi les
    // images) ; nul : celui du champ, permis s'il y a du texte à coller.
    property var coller: null
    // Correcteur d'orthographe du champ (0.6.36, cf. cpp/correcteur.h) : au
    // clic droit sur un mot souligné, ses suggestions en tête du menu.
    property var correcteur: null
    // Le mot souligné sous le clic : {debut, fin, mot}, ou nul.
    property var faute: null
    property var suggestions: []
    // Mot de passe : ni couper ni copier, comme le champ lui-même.
    readonly property bool masque: champ.echoMode !== undefined && champ.echoMode !== TextInput.Normal

    // Clic droit de la souris seulement (cf. VoletLecture) : un TapHandler ne
    // trie les boutons que pour la souris.
    acceptedButtons: Qt.RightButton
    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad | PointerDevice.Stylus
    enabled: !fenetre.tactile

    // Le choix du texte au moment du clic, rendu au champ avant d'agir : le
    // menu prend le focus, et le champ peut perdre sa sélection.
    property int debut: 0
    property int fin: 0
    property bool agi: false

    onTapped: function(point) {
        debut = champ.selectionStart
        fin = champ.selectionEnd
        agi = false
        faute = null
        suggestions = []
        if (correcteur && correcteur.actif) {
            var trouve = correcteur.motA(champ.positionAt(point.position.x, point.position.y))
            if (trouve.length > 0) {
                faute = JSON.parse(trouve)
                suggestions = fenetre.suggestionsOrthographe(faute.mot)
            }
        }
        menu.popup()
    }

    /// Remplace le mot souligné par la suggestion choisie.
    function corriger(texte) {
        var f = faute
        agi = true
        champ.forceActiveFocus()
        correcteur.remplacer(f.debut, f.fin, texte)
        champ.cursorPosition = f.debut + texte.length
    }

    function agir(action) {
        agi = true
        champ.forceActiveFocus()
        champ.select(debut, fin)
        action()
    }

    property Menu menu: Menu {
        id: menuChamp
        parent: clicDroit.champ
        onAboutToShow: width = fenetre.largeurMenu(menuChamp)
        // Le focus rendu au champ, pour continuer à la frappe (Entrée lance
        // la recherche collée).
        onClosed: if (clicDroit.agi) clicDroit.champ.forceActiveFocus()
        // Suggestions du correcteur, en tête (0.6.36).
        Instantiator {
            model: clicDroit.suggestions
            delegate: MenuItem {
                text: modelData
                font.bold: true
                onTriggered: clicDroit.corriger(modelData)
            }
            onObjectAdded: function(index, objet) { menuChamp.insertItem(index, objet) }
            onObjectRemoved: function(index, objet) { menuChamp.removeItem(objet) }
        }
        MenuItem {
            text: qsTr("Aucune suggestion")
            enabled: false
            visible: clicDroit.faute !== null && clicDroit.suggestions.length === 0
            height: visible ? implicitHeight : 0
        }
        MenuItem {
            text: qsTr("Ajouter « %1 » au dictionnaire").arg(clicDroit.faute ? clicDroit.faute.mot : "")
            visible: clicDroit.faute !== null
            height: visible ? implicitHeight : 0
            onTriggered: {
                clicDroit.agi = true
                fenetre.ajouterAuDictionnaire(clicDroit.faute.mot)
            }
        }
        MenuSeparator {
            visible: clicDroit.faute !== null
            height: visible ? implicitHeight : 0
        }
        EntreeMenu {
            text: qsTr("Annuler")
            raccourci: qsTr("Ctrl+Z")
            enabled: !clicDroit.champ.readOnly && clicDroit.champ.canUndo
            onTriggered: clicDroit.agir(function() { clicDroit.champ.undo() })
        }
        MenuSeparator {}
        EntreeMenu {
            text: qsTr("Couper")
            raccourci: qsTr("Ctrl+X")
            enabled: !clicDroit.champ.readOnly && !clicDroit.masque && clicDroit.fin > clicDroit.debut
            onTriggered: clicDroit.agir(function() { clicDroit.champ.cut() })
        }
        EntreeMenu {
            text: qsTr("Copier")
            raccourci: qsTr("Ctrl+C")
            enabled: !clicDroit.masque && clicDroit.fin > clicDroit.debut
            onTriggered: clicDroit.agir(function() { clicDroit.champ.copy() })
        }
        EntreeMenu {
            text: qsTr("Coller")
            raccourci: qsTr("Ctrl+V")
            enabled: !clicDroit.champ.readOnly && (clicDroit.coller !== null || clicDroit.champ.canPaste)
            onTriggered: clicDroit.agir(function() {
                if (clicDroit.coller !== null)
                    clicDroit.coller()
                else
                    clicDroit.champ.paste()
            })
        }
        MenuSeparator {}
        EntreeMenu {
            text: qsTr("Tout sélectionner")
            raccourci: qsTr("Ctrl+A")
            enabled: clicDroit.champ.length > 0
            onTriggered: clicDroit.agir(function() { clicDroit.champ.selectAll() })
        }
    }
}
