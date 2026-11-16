// SPDX-License-Identifier: GPL-3.0-or-later
// Épreuves de la fenêtre de rédaction (Redaction.qml, mmail#178), avec un
// faux noyau : « Envoyer » cliqué pendant l'enregistrement du brouillon n'est
// plus perdu. Le bouton le dit, l'indicateur tourne, et le message part dès la
// fin de l'enregistrement — en remplaçant le brouillon qui vient d'être
// déposé —, ou malgré son échec. Ce que la fenêtre demande au noyau est noté
// par le faux noyau (`enregistrements`, `envois`).
import QtQuick
import QtQuick.Controls
import QtTest
import "../qml" as Interface

Item {
    id: racine
    width: 1200
    height: 700

    // Ce que la fenêtre de rédaction lit de la fenêtre principale.
    QtObject {
        id: fenetre
        property bool tactile: false
        property real tailleBase: 10
        property real tailleRedaction: 11
        property string policeRedaction: "Sans"
        property font font: Qt.font({ family: "Sans", pointSize: 10 })
        property var palette: ({ window: "#ffffff", base: "#ffffff", windowText: "#000000", buttonText: "#000000",
                                 button: "#f0f0f0", highlight: "#3060c0", highlightedText: "#ffffff", text: "#000000" })
        property color couleurAlerte: "#c0392b"
        property bool fermetureApresEnvoi: false
        property var expediteursConnus: [{ compte: 1, adresse: "essai@exemple.fr", compteAdresse: "essai@exemple.fr" }]
        property int majs: 0
        function identiteExpediteur(c) {
            return { nom: "Essai", nouvelle: { signature: "", html: "" }, reponse: { signature: "", html: "" } }
        }
        function majEnvois() { majs += 1 }
        function annoncer(texte, erreur) {}
        function valeurOu(cle, defaut) { return defaut }
        function largeurMenu(menu) { return 200 }
        function zoomDe(zone) { return 1 }
        function tailleLisible(octets) { return octets + " o" }
        function signaturesNommees() { return [] }
        function heureEnvoi(t) { return "" }
        function logicielPour(nom) { return "" }
        function suggestionsOrthographe(mot) { return [] }
        function ajouterAuDictionnaire(mot) {}
        function poserZoom(zone, valeur) {}
        function dessinerTrombone(ctx, w, h, encre) {}
    }

    // Le faux noyau : il note ce qu'on lui demande, et l'issue vient de
    // l'épreuve (`brouillonEnregistre`, `echouer`).
    QtObject {
        id: boite
        property string erreur: ""
        property var enregistrements: []
        property var envois: []
        function enregistrerBrouillon(compte, texte) {
            enregistrements = enregistrements.concat([JSON.parse(texte)])
            return true
        }
        function envoyerMessage(compte, texte) {
            envois = envois.concat([JSON.parse(texte)])
            return true
        }
        function garderRedaction(id, etat) { return true }
        function oublierRedactionGardee(id) {}
        function supprimerDans(compte, chemin, uids) {}
        function decrireFichier(chemin) { return "" }
        function adressesConnues(debut) { return "[]" }
    }

    QtObject {
        id: conteneurEssai
        property int cachee: 0
        function cacherRedaction() { cachee += 1 }
        function montrerRedaction() {}
        function fermerRedaction() {}
    }

    Interface.Redaction {
        id: redac
        anchors.fill: parent
        jeton: "r1"
        conteneur: conteneurEssai
    }

    TestCase {
        name: "RedactionEnvoi"
        when: windowShown

        function chercher(nom, depuis) {
            if (!depuis)
                return null
            if (depuis.objectName === nom && depuis.visible)
                return depuis
            var enfants = depuis.children || []
            for (var i = 0; i < enfants.length; ++i) {
                var trouve = chercher(nom, enfants[i])
                if (trouve)
                    return trouve
            }
            return null
        }

        function envoyer() {
            var b = chercher("boutonEnvoyer", redac)
            verify(b !== null, "bouton « Envoyer »")
            return b
        }

        // Un brouillon repris, l'UID 7 dans « Drafts ».
        function reprendre(destinataire) {
            redac.enEnvoi = false
            redac.envoiDiffere = 0
            redac.remplir({ mode: "brouillon", de: "essai@exemple.fr", a: destinataire, objet: "Essai",
                            texte: "Bonjour", brouillonUid: 7, brouillonChemin: "Drafts", brouillonCompte: 1 })
        }

        function init() {
            boite.enregistrements = []
            boite.envois = []
            reprendre("destinataire@exemple.fr")
            verify(!redac.occupe)
            verify(!redac.enregistrementEnCours)
            verify(!redac.envoiApresEnregistrement)
        }

        function test_envoyer_pendant_l_enregistrement() {
            redac.enregistrer(true)
            compare(boite.enregistrements.length, 1)
            verify(redac.enregistrementEnCours)
            var indicateur = chercher("indicateurEnregistrement", redac)
            verify(indicateur !== null && indicateur.running, "l'enregistrement se voit")
            var b = envoyer()
            verify(b.enabled, "« Envoyer » reste cliquable pendant l'enregistrement")
            compare(b.text, "Envoyer")
            b.clicked()
            compare(boite.envois.length, 0, "rien ne part avant la fin de l'enregistrement")
            verify(redac.envoiApresEnregistrement)
            compare(b.text, "Envoi en attente…")
            verify(!b.enabled, "le bouton dit que l'envoi attend")
            verify(redac.note.indexOf("dès la fin de l'enregistrement") >= 0, redac.note)
            // Un second clic n'en fait pas un second envoi.
            redac.envoyer(false)
            compare(boite.envois.length, 0)
            // Le brouillon est déposé sous l'UID 12 : le message part, et c'est
            // ce brouillon-là qu'il remplace.
            redac.brouillonEnregistre(12, 1, "Drafts")
            compare(boite.envois.length, 1)
            compare(boite.envois[0].brouillonUid, 12)
            compare(boite.envois[0].a, "destinataire@exemple.fr")
            verify(!redac.envoiApresEnregistrement)
            verify(!redac.enregistrementEnCours)
            verify(!indicateur.running)
            verify(redac.enEnvoi)
            compare(conteneurEssai.cachee > 0, true)
        }

        function test_envoi_malgre_l_echec_de_l_enregistrement() {
            redac.enregistrer(true)
            envoyer().clicked()
            compare(boite.envois.length, 0)
            redac.echouer("connexion perdue")
            compare(boite.envois.length, 1, "l'envoi demandé part quand même")
            compare(boite.envois[0].brouillonUid, 7, "il remplace le brouillon d'avant")
            verify(!redac.envoiApresEnregistrement)
            verify(redac.enEnvoi)
        }

        function test_sans_destinataire_rien_n_attend() {
            reprendre("")
            redac.enregistrer(true)
            envoyer().clicked()
            verify(!redac.envoiApresEnregistrement, "un envoi impossible n'est pas mis en attente")
            verify(redac.note.indexOf("destinataire") >= 0, redac.note)
            redac.brouillonEnregistre(12, 1, "Drafts")
            compare(boite.envois.length, 0)
        }

        function test_envoi_immediat_hors_enregistrement() {
            var b = envoyer()
            verify(b.enabled)
            b.clicked()
            compare(boite.envois.length, 1)
            compare(boite.envois[0].brouillonUid, 7)
            compare(boite.enregistrements.length, 0)
        }

        function test_preparation_retient_toujours_l_envoi() {
            redac.attendre("Préparation…")
            var b = envoyer()
            verify(!b.enabled, "pendant une préparation, « Envoyer » reste grisé")
            redac.envoyer(false)
            compare(boite.envois.length, 0)
            verify(!redac.envoiApresEnregistrement)
        }
    }
}
