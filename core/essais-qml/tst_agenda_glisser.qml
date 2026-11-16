// SPDX-License-Identifier: GPL-3.0-or-later
// Épreuves du déplacement et du retaillage des événements dans la grille de
// l'agenda (Agenda.qml, mmail#175), à la souris, avec un faux noyau. La
// semaine du 16/11/2026 porte trois événements :
// - 0, « Point » : mardi de 10:00 à 11:00, modifiable, sans participant ;
// - 1, « Invitation reçue » : mercredi de 11:00 à 12:00, non modifiable ;
// - 2, « Réunion » : jeudi de 14:00 à 15:00, modifiable, un participant.
// Ce que l'agenda enregistre est noté par le faux noyau (`enregistre`).
import QtQuick
import QtQuick.Controls
import QtTest
import "../qml" as Interface

Item {
    id: racine
    width: 1100
    height: 900

    FenetreEssai { id: fenetre }

    QtObject {
        id: noyau
        signal agendasSynchronises(bool change, string erreurs)
        signal agendasMicrosoftAutorises(string compte, string message)
        signal visioPrete(string lien, string message)
        signal evenementEnregistre(bool ok, string message)
        property string erreur: ""
        property var enregistre: []
        readonly property var fiches: [
            { titre: "Point", lieu: "", description: "", quand: "", agenda: 1, annule: false, nomAgenda: "Personnel",
              couleur: "#3060c0", objet: 11, occurrence: "", repete: false, modifiable: true, motif: "",
              organisateur: null, participants: [], maReponse: "" },
            { titre: "Invitation reçue", lieu: "", description: "", quand: "", agenda: 1, annule: false,
              nomAgenda: "Personnel", couleur: "#3060c0", objet: 12, occurrence: "", repete: false, modifiable: false,
              motif: "réunion organisée par Autre : seul l'organisateur la modifie",
              organisateur: { nom: "Autre", adresse: "autre@exemple.fr", statut: "" },
              participants: [{ nom: "", adresse: "essai@exemple.fr", statut: "NEEDS-ACTION" }], maReponse: "NEEDS-ACTION" },
            { titre: "Réunion", lieu: "", description: "", quand: "", agenda: 1, annule: false, nomAgenda: "Personnel",
              couleur: "#3060c0", objet: 13, occurrence: "", repete: false, modifiable: true, motif: "",
              organisateur: { nom: "", adresse: "essai@exemple.fr", statut: "" },
              participants: [{ nom: "Alice", adresse: "alice@exemple.fr", statut: "ACCEPTED" }], maReponse: "" }
        ]
        readonly property var saisies: ({
            "11": { participants: null, organisateur: "", nomOrganisateur: "", agenda: 1, objet: 11, occurrence: "",
                    titre: "Point", lieu: "", description: "", journee: false, debut: "2026-11-17T10:00",
                    fin: "2026-11-17T11:00", repetition: "", jusquAu: "", rappel: 15 },
            "13": { participants: ["Alice <alice@exemple.fr>"], organisateur: "essai@exemple.fr", nomOrganisateur: "",
                    agenda: 1, objet: 13, occurrence: "", titre: "Réunion", lieu: "", description: "", journee: false,
                    debut: "2026-11-19T14:00", fin: "2026-11-19T15:00", repetition: "", jusquAu: "", rappel: 15 }
        })
        function jours() {
            var noms = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."]
            var j = []
            for (var i = 0; i < 7; ++i)
                j.push({ date: "2026-11-" + (16 + i), jour: 16 + i, semaine: noms[i], aujourdhui: false, horsMois: false })
            return j
        }
        function agendas() {
            return JSON.stringify([{ id: 1, compte: 1, boite: "essai@exemple.fr", nom: "Personnel", couleur: "#3060c0",
                                     affiche: true, ecriture: true, invitations: true, microsoft: false, visio: "" }])
        }
        function agendasMicrosoft() { return "[]" }
        function vueAgenda(genre, jour) {
            return JSON.stringify({ genre: "semaine", titre: "16 – 22 novembre 2026", jours: jours(), nombre: 3,
                                    bandes: [], lignes: 0, fiches: fiches,
                                    cases: [
                                        { jour: 1, debut: 600, fin: 660, colonne: 0, colonnes: 1, heures: "10:00 – 11:00", fiche: 0 },
                                        { jour: 2, debut: 660, fin: 720, colonne: 0, colonnes: 1, heures: "11:00 – 12:00", fiche: 1 },
                                        { jour: 3, debut: 840, fin: 900, colonne: 0, colonnes: 1, heures: "14:00 – 15:00", fiche: 2 }
                                    ] })
        }
        function synchroniserAgendas(forcer) {}
        function afficherAgenda(agenda, affiche) {}
        function agendaOccupe(agenda) { return false }
        function lienVisio(agenda, lieu, organisateur) { return "" }
        function saisieEvenement(objet, occurrence, serie) {
            return saisies[String(objet)] ? JSON.stringify(saisies[String(objet)]) : ""
        }
        function enregistrerEvenement(texte) {
            enregistre = enregistre.concat([JSON.parse(texte)])
        }
        function adressesConnues(debut) { return "[]" }
    }

    Interface.Agenda {
        id: vue
        anchors.fill: parent
        boite: noyau
        accent: "#3060c0"
    }

    SignalSpy {
        id: annonces
        target: vue
        signalName: "annoncer"
    }

    TestCase {
        name: "AgendaGlisser"
        when: windowShown

        function chercher(nom, depuis) {
            if (!depuis)
                return null
            if (depuis.objectName === nom)
                return depuis
            var enfants = depuis.children || []
            for (var i = 0; i < enfants.length; ++i) {
                var trouve = chercher(nom, enfants[i])
                if (trouve)
                    return trouve
            }
            return null
        }
        function caseDe(fiche) {
            return chercher("caseHeure-" + fiche, vue)
        }
        // Largeur d'une colonne de jour : celle de la case, sans ses marges.
        function largeurJour(c) {
            return c.width + 6
        }
        function init() {
            noyau.enregistre = []
            annonces.clear()
            vue.ficheEvenement.close()
            vue.confirmationDeplacement.close()
            vue.ecritureEnCours = ""
            vue.relire()
            // La grille défile jusqu'à 7 h 30 : 15 h reste visible.
            waitForRendering(vue)
        }
        function dernier() {
            return noyau.enregistre[noyau.enregistre.length - 1]
        }

        function test_clic_ouvre_la_fiche() {
            var c = caseDe(0)
            verify(c !== null)
            mouseClick(c, c.width / 2, 10)
            tryVerify(function() { return vue.ficheEvenement.opened })
            compare(noyau.enregistre.length, 0)
        }

        // Une heure et demie plus bas et un jour plus loin : mercredi 11:30 –
        // 12:30, la durée gardée.
        function test_deplacer() {
            var c = caseDe(0)
            mouseDrag(c, c.width / 2, 10, largeurJour(c), 1.5 * vue.hauteurHeure)
            tryVerify(function() { return noyau.enregistre.length === 1 })
            var s = dernier()
            compare(s.objet, 11)
            compare(s.debut, "2026-11-18T11:30")
            compare(s.fin, "2026-11-18T12:30")
            compare(s.journee, false)
            compare(s.titre, "Point")
            compare(vue.ecritureEnCours, "deplacement")
            verify(!vue.ficheEvenement.opened)
            noyau.evenementEnregistre(true, "")
            compare(vue.ecritureEnCours, "")
            compare(annonces.signalArguments[annonces.count - 1][0], "Événement déplacé.")
        }

        // Un déplacement de moins d'un demi-quart d'heure ne change rien.
        function test_petit_geste_sans_effet() {
            var c = caseDe(0)
            mouseDrag(c, c.width / 2, 10, 0, 0.1 * vue.hauteurHeure)
            wait(50)
            compare(noyau.enregistre.length, 0)
        }

        // La poignée du bas : la fin seule, d'une demi-heure.
        function test_retailler() {
            var c = caseDe(0)
            mouseDrag(c, c.width / 2, c.height - 2, 0, 0.5 * vue.hauteurHeure)
            tryVerify(function() { return noyau.enregistre.length === 1 })
            compare(dernier().debut, "2026-11-17T10:00")
            compare(dernier().fin, "2026-11-17T11:30")
        }

        // Retailler ne descend pas sous un quart d'heure.
        function test_retailler_minimum() {
            var c = caseDe(0)
            mouseDrag(c, c.width / 2, c.height - 2, 0, -2 * vue.hauteurHeure)
            tryVerify(function() { return noyau.enregistre.length === 1 })
            compare(dernier().debut, "2026-11-17T10:00")
            compare(dernier().fin, "2026-11-17T10:15")
        }

        // Une invitation reçue ne bouge pas : la barre d'état dit pourquoi.
        function test_non_modifiable() {
            var c = caseDe(1)
            mouseDrag(c, c.width / 2, 10, 0, vue.hauteurHeure)
            wait(50)
            compare(noyau.enregistre.length, 0)
            verify(annonces.count >= 1)
            verify(annonces.signalArguments[0][0].indexOf("Non modifiable ici") === 0)
            compare(annonces.signalArguments[0][1], true)
        }

        // Une réunion avec participants : confirmation d'abord, puis
        // l'enregistrement, participants gardés.
        function test_reunion_confirmation() {
            var c = caseDe(2)
            mouseDrag(c, c.width / 2, 10, 0, vue.hauteurHeure)
            tryVerify(function() { return vue.confirmationDeplacement.opened })
            compare(noyau.enregistre.length, 0)
            var texte = chercher("texteDeplacement", vue.confirmationDeplacement.contentItem)
            verify(texte.text.indexOf("15:00 – 16:00") > 0)
            var bouton = chercher("boutonDeplacer", vue.confirmationDeplacement.contentItem)
            mouseClick(bouton)
            tryVerify(function() { return noyau.enregistre.length === 1 })
            compare(dernier().debut, "2026-11-19T15:00")
            compare(dernier().fin, "2026-11-19T16:00")
            compare(dernier().participants.length, 1)
            compare(dernier().participants[0], "Alice <alice@exemple.fr>")
        }

        // Au doigt : un appui long, puis glisser, déplace l'événement.
        function test_doigt_appui_long() {
            vue.tactile = true
            var c = caseDe(0)
            mousePress(c, c.width / 2, 10)
            wait(Qt.styleHints.mousePressAndHoldInterval + 200)
            mouseMove(c, c.width / 2, 10 + vue.hauteurHeure / 2)
            mouseMove(c, c.width / 2, 10 + vue.hauteurHeure)
            mouseRelease(c, c.width / 2, 10 + vue.hauteurHeure)
            vue.tactile = false
            tryVerify(function() { return noyau.enregistre.length === 1 })
            compare(dernier().debut, "2026-11-17T11:00")
            compare(dernier().fin, "2026-11-17T12:00")
        }

        // Au doigt, un glissement simple ne déplace rien (il fait défiler).
        function test_doigt_glissement_simple() {
            vue.tactile = true
            var c = caseDe(0)
            mouseDrag(c, c.width / 2, 10, 0, vue.hauteurHeure)
            vue.tactile = false
            wait(50)
            compare(noyau.enregistre.length, 0)
        }

        // Pendant une écriture, un nouveau geste n'enregistre rien.
        function test_ecriture_en_cours() {
            vue.ecritureEnCours = "deplacement"
            var c = caseDe(0)
            mouseDrag(c, c.width / 2, 10, 0, vue.hauteurHeure)
            wait(50)
            compare(noyau.enregistre.length, 0)
        }
    }
}
