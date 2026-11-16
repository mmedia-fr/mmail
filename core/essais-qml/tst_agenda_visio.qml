// SPDX-License-Identifier: GPL-3.0-or-later
// Épreuves de la fenêtre d'événement de l'agenda (Agenda.qml, mmail#175),
// avec un faux noyau : une visio ajoutée sans participant avertit au premier
// clic, curseur dans « Participants », et s'enregistre au second ; avec un
// participant, ou sans visio, ou pour une visio déjà en place, rien ne
// retient l'enregistrement ; les dates et les heures tapées librement sont
// lues à l'enregistrement. Ce que l'agenda enregistre est noté par le faux
// noyau (`enregistre`).
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
        readonly property string service: "https://visio.exemple.fr"
        readonly property string lien: service + "/exemple-fr-abcd-efgh-jkmn"
        property string erreur: ""
        property var enregistre: []
        function agendas() {
            return JSON.stringify([{ id: 1, compte: 1, boite: "essai@exemple.fr", nom: "Personnel", couleur: "#3060c0",
                                     affiche: true, ecriture: true, invitations: true, microsoft: false, visio: service }])
        }
        function agendasMicrosoft() { return "[]" }
        function vueAgenda(genre, jour) {
            var j = []
            for (var i = 0; i < 7; ++i)
                j.push({ date: "2026-11-" + (16 + i), jour: 16 + i, semaine: "", aujourdhui: false, horsMois: false })
            return JSON.stringify({ genre: "semaine", titre: "", jours: j, nombre: 0, bandes: [], lignes: 0,
                                    fiches: [], cases: [] })
        }
        function synchroniserAgendas(forcer) {}
        function afficherAgenda(agenda, affiche) {}
        function agendaOccupe(agenda) { return false }
        // Le lien d'une réunion du service, s'il est dans le lieu.
        function lienVisio(agenda, lieu, organisateur) {
            return lieu.indexOf(service + "/") >= 0 ? lien : ""
        }
        function visioEvenement(agenda, lieu, description) {
            return JSON.stringify({ lieu: lien + (lieu.length > 0 ? " — " + lieu : ""),
                                    description: "Rejoindre la visio : " + lien + (description.length > 0 ? "\n\n" + description : "") })
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

    TestCase {
        name: "AgendaVisio"
        when: windowShown

        property var saisie: vue.saisieEvenement

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
        function dans(nom) {
            return chercher(nom, saisie.contentItem)
        }
        // Clic sur un élément de la fenêtre, une fois placé : sous Qt 6.8, la
        // fenêtre est dite ouverte avant d'être mise en page.
        function cliquer(nom) {
            var b = dans(nom)
            verify(b !== null, nom)
            tryVerify(function() { return b.visible && b.width > 0 && b.height > 0 }, 2000, nom)
            mouseClick(b)
        }
        function initTestCase() {
            waitForRendering(vue)
        }
        function init() {
            noyau.enregistre = []
            vue.ecritureEnCours = ""
            saisie.enCours = false
            saisie.close()
            vue.relire()
        }
        // Nouvel événement mardi 17/11 de 10:00 à 11:00.
        function nouveau() {
            vue.nouvelEvenement("2026-11-17", 600, false, 60)
            tryVerify(function() { return saisie.opened })
            waitForItemPolished(saisie.contentItem)
            waitForRendering(saisie.contentItem)
            dans("champTitre").text = "Point"
        }
        function dernier() {
            return noyau.enregistre[noyau.enregistre.length - 1]
        }

        function test_visio_sans_participant() {
            nouveau()
            cliquer("boutonVisio")
            verify(dans("champLieu").text.indexOf(noyau.lien) === 0)
            cliquer("boutonEnregistrerEvenement")
            wait(50)
            compare(noyau.enregistre.length, 0)
            verify(saisie.opened)
            verify(saisie.erreur.indexOf("Aucun participant") === 0, saisie.erreur)
            verify(dans("champParticipants").activeFocus)
            verify(dans("conseilParticipants").visible)
            // Second clic : enregistré, sans participant.
            cliquer("boutonEnregistrerEvenement")
            tryVerify(function() { return noyau.enregistre.length === 1 })
            compare(dernier().participants.length, 0)
            verify(dernier().lieu.indexOf(noyau.lien) === 0)
        }

        // Une adresse saisie lève l'avertissement.
        function test_avertissement_leve() {
            nouveau()
            cliquer("boutonVisio")
            cliquer("boutonEnregistrerEvenement")
            verify(saisie.erreur.length > 0)
            var p = dans("champParticipants")
            p.forceActiveFocus()
            keyClick(Qt.Key_B)
            compare(saisie.erreur, "")
        }

        function test_visio_avec_participant() {
            nouveau()
            cliquer("boutonVisio")
            dans("champParticipants").text = "bob@exemple.fr"
            cliquer("boutonEnregistrerEvenement")
            tryVerify(function() { return noyau.enregistre.length === 1 })
            compare(dernier().participants, ["bob@exemple.fr"])
        }

        // « Rejoindre maintenant » avertit de même, sans ouvrir la réunion.
        function test_rejoindre_maintenant_avertit() {
            nouveau()
            cliquer("boutonVisio")
            cliquer("boutonRejoindre")
            wait(50)
            compare(noyau.enregistre.length, 0)
            verify(!saisie.rejoindreApres)
            verify(saisie.erreur.indexOf("Aucun participant") === 0)
        }

        function test_sans_visio_sans_avertissement() {
            nouveau()
            cliquer("boutonEnregistrerEvenement")
            tryVerify(function() { return noyau.enregistre.length === 1 })
            compare(dernier().lieu, "")
        }

        // Un événement dont le lieu portait déjà la visio : pas d'avertissement.
        function test_visio_deja_en_place() {
            saisie.ouvrir({ agenda: 1, objet: 21, occurrence: "", titre: "Point", lieu: noyau.lien,
                            description: "", journee: false, debut: "2026-11-17T10:00", fin: "2026-11-17T11:00",
                            repetition: "", jusquAu: "", rappel: 15, participants: [], organisateur: "",
                            nomOrganisateur: "" })
            tryVerify(function() { return saisie.opened })
            cliquer("boutonEnregistrerEvenement")
            tryVerify(function() { return noyau.enregistre.length === 1 })
            compare(dernier().objet, 21)
        }

        // Dates et heures tapées librement, enregistrées sans quitter le champ.
        function test_saisie_souple() {
            nouveau()
            dans("champDebutJour").text = "1811"
            dans("champDebutHeure").text = "9h30"
            dans("champFinJour").text = "18/11/26"
            dans("champFinHeure").text = "1045"
            saisie.enregistrer()
            tryVerify(function() { return noyau.enregistre.length === 1 })
            compare(dernier().debut, "2026-11-18T09:30")
            compare(dernier().fin, "2026-11-18T10:45")
        }

        // La fin suit le début, durée gardée, au réglage de l'heure (flèche
        // haut ; la molette fait le même pas, cf. tst_champs_date_heure).
        function test_fin_suit_le_debut() {
            nouveau()
            var debut = dans("champDebutHeure")
            debut.forceActiveFocus()
            keyClick(Qt.Key_Up)
            compare(debut.text, "10:15")
            compare(dans("champFinHeure").text, "11:15")
        }
    }
}
