// SPDX-License-Identifier: GPL-3.0-or-later
// Scénarios d'essai (MMAIL_SCENARIO) : l'interface pilotée sans personne
// devant l'écran, contre les boîtes d'essai, chaque étape tracée sur la
// console (« scenario: … »). Jamais en usage courant : rien ne les appelle
// sans les variables d'environnement d'essai (cf. cpp/main.cpp).
//
// Sortis de Main.qml en 0.5.16 (découpage) : les noms qu'ils emploient sans
// les déclarer (fenetre, boite, essai, choisir…) sont ceux de Main.qml, où
// « Scenarios { id: scenarios } » les instancie ; une propriété de la
// fenêtre ne s'y écrit que qualifiée (fenetre.selection = …).
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

QtObject {
    id: scenarios

    /// Secondes écoulées depuis `depart` (Date.now()), arrondies à 5 : de
    /// quoi lire une attente sans rendre la trace différente d'un essai à
    /// l'autre.
    function secondesDepuis(depart) {
        return Math.round((Date.now() - depart) / 5000) * 5
    }

    /// Scénario « orthographe » (0.6.36) : un mot fautif du corps est repéré
    /// (et pas un mot juste, ni l'élision), le dictionnaire propose la bonne
    /// forme, la suggestion choisie remplace le mot.
    function etapeScenarioOrthographe(etape) {
        if (etape === 0) {
            var compte = boite.compteCourant > 0 ? boite.compteCourant : comptesConnus[0].compte
            var r = ouvrirRedaction(compte)
            r.pret()
            r.remplir({ a: "deux@essai.test", objet: "Orthographe", mode: "nouveau",
                        texte: "Bonjour, je vous envoie la factrue de l’école." })
            essai.redactionOrtho = r
            var pause = Qt.createQmlObject('import QtQuick; Timer { interval: 3000 }', fenetre)
            pause.triggered.connect(function() { etapeScenarioOrthographe(1) })
            pause.start()
            return
        }
        var r1 = essai.redactionOrtho
        var c = r1.correcteurOrtho
        var texte = r1.mefCorps.texte()
        var fautif = c.motA(texte.indexOf("factrue") + 2)
        console.log("scenario: mot fautif |", fautif)
        console.log("scenario: mot juste |", c.motA(texte.indexOf("Bonjour") + 2) === "",
                    "| élision |", c.motA(texte.indexOf("école") + 2) === "")
        console.log("scenario: suggestions |", JSON.stringify(suggestionsOrthographe("factrue")))
        // Les vagues dessinées (0.6.33) : une zone, celle de « factrue ».
        console.log("scenario: zones |", JSON.stringify(c.zones))
        if (fautif.length > 0) {
            var f = JSON.parse(fautif)
            c.remplacer(f.debut, f.fin, "facture")
        }
        console.log("scenario: corrigé |", r1.mefCorps.texte().indexOf("la facture de") >= 0, "|", c.motA(texte.indexOf("Bonjour") + 2) === "")
        Qt.quit()
    }

    /// Scénario « import-csv » (0.6.35, banc CARNET=1) : MMAIL_DOSSIER désigne
    /// un export CSV d'Outlook ; ses contacts rejoignent le carnet « Personnel »
    /// du serveur, sauf celui dont une adresse y est déjà.
    function lancerScenarioImportCsv() {
        var avant = 0
        var surRelu = function(change, erreurs) {
            boite.agendasSynchronises.disconnect(surRelu)
            var fiches = JSON.parse(boite.carnet(""))
            console.log("scenario: fiches |", avant, "→", fiches.length, "|",
                        JSON.stringify(fiches.map(function(f) { return f.nom }).sort()))
            var rene = JSON.parse(boite.carnet("rene.dupont"))
            if (rene.length > 0)
                console.log("scenario: fiche importée |", boite.ficheContact(rene[0].id))
            Qt.quit()
        }
        var surEcrit = function(ok, message, conflit) {
            boite.contactsEcrits.disconnect(surEcrit)
            console.log("scenario: écriture |", ok, "|", message)
            boite.agendasSynchronises.connect(surRelu)
            boite.synchroniserAgendas(true)
        }
        var surSynchro = function(change, erreurs) {
            boite.agendasSynchronises.disconnect(surSynchro)
            var carnets = JSON.parse(boite.carnetsModifiables())
            console.log("scenario: carnets modifiables |", carnets.map(function(c) { return c.nom }).join(", "))
            if (carnets.length === 0) {
                Qt.quit()
                return
            }
            avant = JSON.parse(boite.carnet("")).length
            boite.contactsEcrits.connect(surEcrit)
            console.log("scenario: lecture |", boite.importerContacts(carnets[0].id, identifiantsEssai.dossier))
        }
        boite.agendasSynchronises.connect(surSynchro)
        boite.synchroniserAgendas(true)
    }

    /// Scénario « filtre-autre-boite » (0.6.33) : un filtre de un@ sur le
    /// destinataire (À) et l'objet range « Essai 2 » dans « Essais » de deux@ ;
    /// appliqué à la boîte de réception, le message passe d'une boîte à l'autre.
    function etapeScenarioFiltreAutreBoite(etape) {
        var un = null, deux = null
        var comptes = fenetre.listeComptes()
        for (var i = 0; i < comptes.length; ++i) {
            if (comptes[i].adresse === "un@essai.test") un = comptes[i]
            if (comptes[i].adresse === "deux@essai.test") deux = comptes[i]
        }
        if (!un || !deux) {
            console.log("scenario: ECHEC deux comptes attendus")
            Qt.quit()
            return
        }
        var suite = function(n, delai) {
            var t = Qt.createQmlObject('import QtQuick; Timer { interval: ' + delai + ' }', fenetre)
            t.triggered.connect(function() { etapeScenarioFiltreAutreBoite(n) })
            t.start()
        }
        if (etape === 0) {
            var filtres = [{ nom: "Vers deux", compte: "un@essai.test", tout: true,
                             conditions: [{ champ: "a", operateur: "contient", valeur: "un@essai" },
                                          { champ: "objet", operateur: "est", valeur: "Essai 2" }],
                             destination: "dossier", compte_destination: "deux@essai.test", dossier: "Essais" }]
            console.log("scenario: filtres posés |", boite.poserFiltres(JSON.stringify(filtres)), "|", boite.erreur)
            console.log("scenario: filtres appliqués |", boite.appliquerFiltres(un.id), "message(s) touché(s)")
            suite(1, 10000)
        } else if (etape === 1) {
            ouvrirDossier(deux.id, "Essais")
            suite(2, 6000)
        } else if (etape === 2) {
            var sujets = []
            for (var k = 0; k < modeleMessages.count; ++k)
                sujets.push(modeleMessages.get(k).sujet)
            console.log("scenario: Essais de deux@ |", JSON.stringify(sujets))
            ouvrirDossier(un.id, "INBOX")
            suite(3, 6000)
        } else {
            var restants = []
            for (var j = 0; j < modeleMessages.count; ++j)
                restants.push(modeleMessages.get(j).sujet)
            console.log("scenario: boîte de réception de un@ |", JSON.stringify(restants))
            Qt.quit()
        }
    }

    /// Scénario « compte-erreur » (0.6.31) : une connexion automatique en
    /// échec — secret absent du coffre au démarrage, mot de passe refusé —
    /// laisse le compte en « erreur » sans ouvrir sa fenêtre ; un appui sur
    /// la mention l'ouvre ; après un geste de la personne, un refus la fait
    /// revenir, comme avant ; le bon mot de passe reconnecte le compte.
    function etapeScenarioCompteErreur(etape) {
        var comptes = fenetre.listeComptes()
        var c = null
        for (var i = 0; i < comptes.length; ++i)
            if (comptes[i].adresse === identifiantsEssai.utilisateur)
                c = comptes[i]
        if (!c) {
            console.log("scenario: ECHEC compte d'essai absent")
            Qt.quit()
            return
        }
        var ligne = function() {
            for (var k = 0; k < modeleArborescence.count; ++k) {
                var l = modeleArborescence.get(k)
                if (l.genre === "compte" && l.compte === c.id)
                    return l
            }
            return null
        }
        var decrire = function() {
            var l = ligne()
            return l ? l.etat + " | motif « " + l.motif + " »" : "ligne absente"
        }
        // Chaque étape attend l'état qu'elle vérifie (une seconde par essai,
        // 40 au plus) : un serveur retarde de plus en plus ses refus
        // d'identifiants successifs (Dovecot : 2 s, puis 6 s…).
        var suite = function(n, condition) {
            var essais = 40
            var t = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
            t.triggered.connect(function() {
                if (condition() || --essais <= 0) {
                    t.stop()
                    etapeScenarioCompteErreur(n)
                }
            })
            t.start()
        }
        var motif = function() {
            var l = ligne()
            return l ? l.motif : ""
        }
        if (etape === 0) {
            // Démarrage simulé : session fermée, secret relu au coffre — que
            // le banc n'a pas.
            boite.deconnecter(c.id)
            reglages.memoriser = true
            fenetre.connecterCompte(c)
            suite(1, function() { var l = ligne(); return l && l.etat === "erreur" })
        } else if (etape === 1) {
            console.log("scenario: démarrage sans secret | fenêtre", dlgCompte.visible, "|", decrire())
            // Refus du serveur pendant une connexion automatique.
            boite.connecter(c.hote, c.adresse, "mauvais-mot-de-passe")
            suite(2, function() { return motif().indexOf("identifiants") === 0 })
        } else if (etape === 2) {
            console.log("scenario: refus automatique | fenêtre", dlgCompte.visible, "|", decrire(),
                        "| barre « " + messageEtat.texte + " »")
            var l = ligne()
            fenetre.ouvrirErreurCompte(c.id, l ? l.motif : "")
            console.log("scenario: appui sur « erreur » | fenêtre", dlgCompte.visible,
                        "| note « " + dlgCompte.noteCompte.text + " »")
            // Geste de la personne : un mot de passe encore faux.
            dlgCompte.champMotDePasse.text = "encore-faux"
            dlgCompte.accept()
            suite(3, function() { return dlgCompte.visible })
        } else if (etape === 3) {
            console.log("scenario: refus après un geste | fenêtre", dlgCompte.visible,
                        "| note « " + dlgCompte.noteCompte.text + " »")
            dlgCompte.champMotDePasse.text = identifiantsEssai.motDePasse
            dlgCompte.accept()
            suite(4, function() { var l = ligne(); return l && l.etat === "pret" })
        } else {
            console.log("scenario: reconnecté | fenêtre", dlgCompte.visible, "|", decrire(),
                        "| gestes", JSON.stringify(fenetre.gestesCompte))
            Qt.quit()
        }
    }

    /// Scénario « hors-connexion » (0.6.34) : sans serveur d'envoi joignable,
    /// le message reste caché dans la boîte d'envoi au lieu de revenir en
    /// échec ; la barre d'état le compte. Le banc démarre ensuite son serveur
    /// d'envoi (« scenario: demarrer-smtp ») ; au nouvel essai, le message part
    /// et la rédaction se défait.
    function etapeScenarioHorsConnexion(etape) {
        var attendre = function(condition, ensuite) {
            var essais = 60
            var t = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
            t.triggered.connect(function() {
                if (condition() || --essais <= 0) {
                    t.stop()
                    ensuite()
                }
            })
            t.start()
        }
        if (etape === 0) {
            var compte = boite.compteCourant > 0 ? boite.compteCourant : comptesConnus[0].compte
            var r = ouvrirRedaction(compte)
            r.pret()
            r.remplir({ a: "deux@essai.test", objet: "Hors connexion", mode: "nouveau", texte: "Partira au retour du réseau." })
            essai.redactionAttente = r
            r.envoyer(false)
            attendre(function() { return r.enAttente || (!r.enEnvoi && r.conteneur.visible) },
                     function() { etapeScenarioHorsConnexion(1) })
        } else if (etape === 1) {
            var r1 = essai.redactionAttente
            console.log("scenario: sans serveur d'envoi | en attente", r1.enAttente, "| fenêtre", r1.conteneur.visible,
                        "| en cours", envoisEnCours, "| en attente", envoisEnAttente, "| barre « " + etatSynchro() + " »")
            console.log("scenario: demarrer-smtp")
            var pause = Qt.createQmlObject('import QtQuick; Timer { interval: 4000 }', fenetre)
            pause.triggered.connect(function() {
                renvoyerEnAttente(0)
                attendre(function() { return !redactions[r1.jeton] },
                         function() { etapeScenarioHorsConnexion(2) })
            })
            pause.start()
        } else {
            var r2 = essai.redactionAttente
            console.log("scenario: retour du réseau | rédaction défaite", !redactions[r2.jeton],
                        "| en attente", envoisEnAttente, "| en cours", envoisEnCours, "| barre « " + etatSynchro() + " »")
            Qt.quit()
        }
    }

    /// Scénario « double-clic » (0.6.16) : la fenêtre du message a reçu son
    /// corps. Marquage non lu puis lu depuis la fenêtre, fermeture ; puis
    /// l'option « Ouvrir une réponse », et les brouillons.
    function etapeScenarioDoubleClic(f) {
        if (essai.fenetreLue)
            return
        essai.fenetreLue = true
        console.log("scenario: fenêtre « " + f.title + " » | De :", f.entete.de, "| À :", f.entete.a,
                    "| Cc :", f.entete.cc || "—", "| html", f.html, "| pièces", f.pieces.length,
                    "| ligne lue", modeleMessages.get(0).lu)
        var suite = function() {
            f.marquer(false)
            console.log("scenario: depuis la fenêtre, non lu → ligne lue", modeleMessages.get(0).lu,
                        "| bouton", f.lu ? "Marquer comme non lu" : "Marquer comme lu")
            f.marquer(true)
            console.log("scenario: puis lu → ligne lue", modeleMessages.get(0).lu)
            f.close()
            Qt.callLater(function() {
                console.log("scenario: fenêtre fermée | fenêtres", fenetre.fenetresMessage.length)
                reglages.doubleClic = "reponse"
                var avant = Object.keys(fenetre.redactions).length
                fenetre.choisir(1, 0)
                fenetre.doubleClicMessage(1)
                console.log("scenario: option « Ouvrir une réponse » : rédactions", avant, "→",
                            Object.keys(fenetre.redactions).length, "| fenêtres de message",
                            fenetre.fenetresMessage.length)
                reglages.doubleClic = "message"
                essai.etape = "brouillons"
                fenetre.ouvrirDossier(boite.compteCourant, "Drafts")
            })
        }
        // MMAIL_DOSSIER « garder » : la fenêtre reste ouverte, pour une
        // capture (--capture, MMAIL_CAPTURE_FENETRE).
        if (identifiantsEssai.dossier !== "garder")
            suite()
    }

    /// `action` dans `ms` millisecondes, une fois.
    function plusTard(ms, action) {
        var minuterie = Qt.createQmlObject('import QtQuick; Timer {}', fenetre)
        minuterie.interval = ms
        minuterie.triggered.connect(function() {
            minuterie.destroy()
            action()
        })
        minuterie.start()
    }

    /// Scénario « vidage » : chaque issue, puis l'état des comptes cinq
    /// secondes après la dernière — le 01/10, ils étaient tous passés en
    /// « erreur » après un vidage.
    function bilanScenarioVidage(issue) {
        console.log("scenario: " + issue)
        if (!essai.bilanVidage) {
            essai.bilanVidage = Qt.createQmlObject('import QtQuick; Timer { interval: 5000 }', fenetre)
            essai.bilanVidage.triggered.connect(function() {
                JSON.parse(boite.arborescence(false))
                    .filter(function(l) { return l.genre === "compte" })
                    .forEach(function(l) {
                        console.log("scenario: compte", l.adresse, "| état :", l.etat, "| motif :", l.motif || "—")
                    })
                Qt.quit()
            })
        }
        essai.bilanVidage.restart()
    }

    /// Scénario « sauvegarde » : des réglages, une signature avec son image
    /// (MMAIL_DOSSIER : adresse d'une image) et un favori, puis la sauvegarde.
    /// Le scénario « restauration », sur un profil neuf, les retrouve.
    function etapeScenarioSauvegarde(compte, adresse) {
        reglages.zoomListe = 1.25
        reglagesApparence.theme = "classique"
        var image = boite.imageSignature(adresse, identifiantsEssai.dossier)
        var identites = {}
        identites[adresse] = { nom: "Essai Sauvegarde", signature: "<p>Cordialement</p><img src=\"" + image + "\">" }
        reglagesRedaction.identites = JSON.stringify(identites)
        boite.epinglerDossier(compte, "INBOX", true)
        console.log("scenario: image de signature", image)
        console.log("scenario: réglages", JSON.stringify(lireReglages()).length, "caractères")
        sauvegarderConfiguration(true)
    }

    /// Scénario « restauration » : ce que la sauvegarde a rapporté.
    function bilanScenarioRestauration() {
        var identites = JSON.parse(reglagesRedaction.identites)
        var adresses = Object.keys(identites)
        var signature = adresses.length > 0 ? identites[adresses[0]].signature : ""
        var image = (signature.match(/src="([^"]+)"/) || [])[1] || ""
        var favoris = 0
        for (var i = 0; i < modeleFavoris.count; ++i)
            if (modeleFavoris.get(i).genre === "favori")
                ++favoris
        console.log("scenario: bilan zoomListe", reglages.zoomListe, "apparence", fenetre.apparence,
                    "identités", adresses.join(","), "favoris", favoris)
        console.log("scenario: image", image)
    }

    function etapeScenarioGarde() {
        var r = ouvrirRedaction(comptesConnus.length > 0 ? comptesConnus[0].compte : 0)
        r.remplir({ a: "destinataire@exemple.fr", objet: "Essai de reprise",
                    texte: "Texte tapé avant la coupure.", mode: "brouillon" })
        r.modifie = true
        r.garder()
        console.log("scenario: gardée", r.idGarde, r.etatGarde.length > 0 ? "OK" : "ECHEC")
    }

    /// Scénario « garde-serveur » : l'enregistrement périodique sur le
    /// serveur efface la rédaction gardée sur le poste.
    function etapeScenarioGardeServeur() {
        var r = ouvrirRedaction(comptesConnus[0].compte)
        r.remplir({ a: "destinataire@exemple.fr", objet: "Essai d'enregistrement automatique",
                    texte: "Brouillon parti seul.", mode: "brouillon" })
        r.modifie = true
        r.garder()
        console.log("scenario: gardées avant", JSON.parse(boite.redactionsGardees()).length)
        r.enregistrer(true)
    }

    /// Scénario « coller-image » : l'image de MMAIL_DOSSIER (un fichier) passe
    /// par le presse-papier, se colle dans un message neuf, et le brouillon
    /// part sur le serveur — l'image y doit être intégrée.
    // Scénarios « mise-a-jour » et « mise-a-jour-fermeture » : MMail se croit
    // ancien (MMAIL_VERSION_ESSAI), installé par l'installeur ou lancé en
    // AppImage. La dernière publication est proposée, choisie « Maintenant »
    // (ou « À la fermeture »), téléchargée et contrôlée ; puis MMail redémarre
    // (ou se ferme), et le programme l'installe.
    function etapeScenarioMiseAJour(etape) {
        var fermeture = essai.scenario === "mise-a-jour-fermeture"
        console.log("scenario: mise à jour", etape, "| publiée", versionNouvelle, "| actuelle", miseAJour.versionActuelle(),
                    "| genre", genreMiseAJour, "| état", etatMiseAJour, "| fichier", fichierMiseAJour,
                    "| programmée", installeur.programme(), erreurMiseAJour)
        capturerBandeauVersion(etape)
        if (etape === "proposee" && etatMiseAJour === "proposee") {
            var choix = Qt.createQmlObject('import QtQuick; Timer { interval: 4000 }', fenetre)
            choix.triggered.connect(function() { choisirMiseAJour(fermeture ? "fermeture" : "maintenant") })
            choix.start()
        } else if (etape === "prete") {
            var pause = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
            pause.triggered.connect(function() {
                console.log("scenario: mise à jour —", fermeture ? "fermeture" : "redémarrage", "| pid", Qt.application.arguments)
                if (fermeture)
                    Qt.quit()
                else
                    redemarrerPourMettreAJour()
            })
            pause.start()
        } else if (etape === "echec") {
            Qt.quit()
        }
    }

    // Scénario « supprimer-dossier » (0.8.5) : « Archive/Projets » et
    // « Archive/Projets/2025 », créés ici — les dossiers du banc (« Essais »)
    // sont recréés d'office par Dovecot (auto = subscribe). Le dossier va
    // dans la corbeille avec son sous-dossier, Ctrl+Z l'en sort, puis, supprimé
    // de nouveau, il s'efface de la corbeille pour de bon.
    function lancerScenarioSupprimerDossier(essais) {
        var n = essais || 0
        var chemins = function() {
            var l = []
            for (var k = 0; k < modeleArborescence.count; ++k) {
                var d = modeleArborescence.get(k)
                if (d.genre === "dossier" && d.compte === boite.compteCourant)
                    l.push(d.chemin)
            }
            return l
        }
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var finir = function() {
            console.log("scenario: supprimer-dossier " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
            Qt.quit()
        }
        if ((boite.compteCourant <= 0 || chemins().indexOf("Archive") < 0) && n < 40) {
            plusTard(1000, function() { lancerScenarioSupprimerDossier(n + 1) })
            return
        }
        var compte = boite.compteCourant
        if (chemins().indexOf("Archive/Projets/2025") < 0) {
            if (n < 100) {
                if (chemins().indexOf("Archive/Projets") < 0)
                    boite.creerSousDossier(compte, "Archive", "Projets")
                else
                    boite.creerSousDossier(compte, "Archive/Projets", "2025")
                plusTard(3000, function() { lancerScenarioSupprimerDossier(100) })
                return
            }
            if (n === 100) {
                boite.creerSousDossier(compte, "Archive/Projets", "2025")
                plusTard(3000, function() { lancerScenarioSupprimerDossier(101) })
                return
            }
            bilan("dossiers d'essai créés : " + chemins().join(", "), false)
            finir()
            return
        }
        bilan("système protégé : « " + boite.supprimerDossier(compte, "INBOX") + " »",
              boite.supprimerDossier(compte, "INBOX").length > 0)
        bilan("pas dans la corbeille au départ", !boite.dansLaCorbeille(compte, "Archive/Projets"))
        var motif = boite.supprimerDossier(compte, "Archive/Projets")
        bilan("suppression lancée (« " + motif + " »)", motif === "")
        plusTard(4000, function() {
            var l = chemins()
            bilan("dans la corbeille avec son sous-dossier : " + l.join(", "),
                  l.indexOf("Archive/Projets") < 0 && l.indexOf("Trash/Projets") >= 0 && l.indexOf("Trash/Projets/2025") >= 0)
            bilan("Ctrl+Z proposé : « " + boite.annulation + " »", boite.annulation === qsTr("Annuler la suppression du dossier"))
            bilan("dans la corbeille désormais", boite.dansLaCorbeille(compte, "Trash/Projets"))
            fenetre.annuler()
            plusTard(4000, function() {
                var l2 = chemins()
                bilan("Ctrl+Z : remis à sa place : " + l2.join(", "),
                      l2.indexOf("Archive/Projets") >= 0 && l2.indexOf("Archive/Projets/2025") >= 0 && l2.indexOf("Trash/Projets") < 0)
                bilan("plus rien à annuler", boite.annulation === "")
                boite.supprimerDossier(compte, "Archive/Projets")
                plusTard(4000, function() {
                    var m = boite.effacerDossier(compte, "Trash/Projets")
                    bilan("effacement définitif lancé (« " + m + " »)", m === "")
                    plusTard(4000, function() {
                        var l3 = chemins()
                        bilan("effacé de la corbeille : " + l3.join(", "),
                              l3.indexOf("Trash/Projets") < 0 && l3.indexOf("Trash/Projets/2025") < 0 && l3.indexOf("Archive/Projets") < 0)
                        finir()
                    })
                })
            })
        })
    }

    function etapeScenarioCollerImage() {
        var r = ouvrirRedaction(comptesConnus[0].compte)
        r.pret()
        r.remplir({ a: "destinataire@exemple.fr", objet: "Essai image collée " + Date.now(),
                    texte: "Avant l'image.", mode: "brouillon" })
        console.log("scenario: image dans le presse-papier", pressePapier.deposerImage(identifiantsEssai.dossier))
        console.log("scenario: collée", r.collerImage(), "| mise en forme", r.miseEnForme,
                    "| image dans le corps", r.htmlActuel().indexOf("<img") >= 0,
                    "| parmi les images collées", r.htmlActuel().indexOf("images-redaction/collees/collee-") >= 0)
        essai.etape = 1
        r.enregistrer(false)
    }

    /// Scénario « coller-piece » (0.8.4), MMAIL_DOSSIER=<image PNG> : l'image
    /// du presse-papier devient pièce jointe par « Coller en pièce jointe »,
    /// sans entrer dans le corps ; rien à coller, le motif s'affiche.
    function etapeScenarioCollerPiece() {
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var r = ouvrirRedaction(comptesConnus[0].compte)
        r.pret()
        r.remplir({ a: "destinataire@exemple.fr", objet: "Essai pièce collée", texte: "Bonjour.", mode: "nouveau" })
        bilan("presse-papier vide de fichiers virtuels hors Windows",
              pressePapier.fichiersVirtuels(boite.dossierPiecesCollees()).length === 0)
        bilan("image déposée dans le presse-papier", pressePapier.deposerImage(identifiantsEssai.dossier))
        var bouton = objetNomme(r, "boutonCollerPiece")
        bilan("bouton « Coller en pièce jointe » présent", !!bouton)
        bouton.clicked()
        bilan("image jointe : " + JSON.stringify(r.pieces.map(function(p) { return p.nom })),
              r.pieces.length === 1 && /\.png$/.test(r.pieces[0].nom))
        bilan("rien dans le corps", r.htmlActuel().indexOf("<img") < 0)
        pressePapier.deposer("du texte seulement")
        bouton.clicked()
        bilan("rien à joindre : « " + r.note + " »", r.pieces.length === 1 && r.note.indexOf("Rien à joindre") === 0)
        console.log("scenario: coller-piece " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
        Qt.quit()
    }

    /// Scénario « brouillon-auto » : un message neuf que l'on ne touche pas ne
    /// passe pas pour modifié ; modifié, il est enregistré sur le serveur sans
    /// rien demander 30 s plus tard, puis une minute après la frappe suivante,
    /// la version précédente effacée (UID rendus par le serveur).
    function etapeScenarioBrouillonAuto(r, uid) {
        var ecoule = function() { return Math.round((Date.now() - essai.depart) / 1000) }
        if (essai.etape === 0) {
            essai.etape = 1
            var n = ouvrirRedaction(comptesConnus[0].compte)
            n.pret()
            var controle = Qt.createQmlObject('import QtQuick; Timer { interval: 3000 }', fenetre)
            controle.triggered.connect(function() {
                console.log("scenario: message neuf, modifié", n.modifie)
                n.remplir({ a: "destinataire@exemple.fr", objet: "Essai brouillon automatique " + Date.now(),
                            texte: "Première frappe.", mode: "brouillon" })
                n.modifie = true
                essai.depart = Date.now()
                var garde = Qt.createQmlObject('import QtQuick; Timer { interval: 150000 }', fenetre)
                garde.triggered.connect(function() {
                    if (essai.etape < 3)
                        console.log("scenario: ECHEC, étape", essai.etape, "| note", n.note)
                })
                garde.start()
            })
            controle.start()
        } else if (essai.etape === 1) {
            essai.etape = 2
            essai.premier = uid
            console.log("scenario: 1er enregistrement après", ecoule(), "s | uid", uid, "| modifiée", r.modifie,
                        "| note", r.note)
            // La frappe reprend un peu plus tard.
            var reprise = Qt.createQmlObject('import QtQuick; Timer { interval: 5000 }', fenetre)
            reprise.triggered.connect(function() {
                r.modifie = true
                essai.depart = Date.now()
            })
            reprise.start()
        } else if (essai.etape === 2) {
            essai.etape = 3
            console.log("scenario: 2e enregistrement après", ecoule(), "s | uid", uid, "| précédent", essai.premier,
                        "| modifiée", r.modifie)
        }
    }

    /// Scénario « brouillon-compte » (0.6.24), deux comptes : un brouillon
    /// enregistré sur le premier, puis « De » passé sur le second et le
    /// brouillon réenregistré. Le second compte ne doit rien perdre de ses
    /// brouillons ; celui du premier part à sa corbeille. Le contrôle des
    /// boîtes se fait ensuite sur le serveur. Les compteurs des dossiers
    /// Brouillons sont relevés à la fin.
    function etapeScenarioBrouillonCompte(r, uid, compte, chemin) {
        var comptes = fenetre.comptesConnus
        if (essai.etape === 0) {
            if (comptes.length < 2) {
                var encore = Qt.createQmlObject('import QtQuick; Timer { interval: 2000 }', fenetre)
                encore.triggered.connect(function() { etapeScenarioBrouillonCompte(null, 0, 0, "") })
                encore.start()
                return
            }
            essai.etape = 1
            essai.un = comptes[0].compte
            essai.deux = comptes[1].compte
            var n = fenetre.ouvrirRedaction(essai.un)
            n.pret()
            n.remplir({ a: "destinataire@essai.test", objet: "Essai changement de compte",
                        texte: "Brouillon commencé sur le premier compte.", mode: "brouillon" })
            n.modifie = true
            console.log("scenario: rédaction sur le compte", essai.un)
            n.enregistrer(false)
        } else if (essai.etape === 1) {
            essai.etape = 2
            console.log("scenario: 1er brouillon | uid", uid, "| compte", compte, "| dossier", chemin)
            r.choisirCompte(essai.deux)
            r.modifie = true
            console.log("scenario: « De » passé sur le compte", r.compteChoisi().compte)
            r.enregistrer(false)
        } else if (essai.etape === 2) {
            essai.etape = 3
            console.log("scenario: 2e brouillon | uid", uid, "| compte", compte, "| dossier", chemin,
                        "| porté par", r.brouillonCompte)
            var fin = Qt.createQmlObject('import QtQuick; Timer { interval: 6000 }', fenetre)
            fin.triggered.connect(function() {
                for (var i = 0; i < modeleArborescence.count; ++i) {
                    var l = modeleArborescence.get(i)
                    if (l.genre === "dossier" && l.role === "Drafts")
                        console.log("scenario: Brouillons du compte", l.compte, "| éléments", l.messages,
                                    "| non lus", l.nonLus, "| affiché", l.messages > 0 ? "[" + l.messages + "]" : "")
                }
                console.log("scenario: fin")
            })
            fin.start()
        }
    }

    /// Scénario « recherche-partout » : « Éléments envoyés » puis la boîte de
    /// réception passent par l'index ; le texte de MMAIL_DOSSIER est cherché
    /// dans toutes les boîtes ; un résultat d'un autre dossier est choisi — le
    /// noyau y passe, la liste reste celle des résultats ; Échap ramène au
    /// dossier de départ. MMAIL_SORTIE_PRECOCE absent : la fenêtre reste sur
    /// les résultats pour la capture.
    function etapeScenarioRecherchePartout(compte, chemin) {
        if (essai.etape === 0) {
            essai.etape = 1
            ouvrirDossier(compte, "Sent")
        } else if (essai.etape === 1 && chemin === "Sent") {
            essai.etape = 2
            ouvrirDossier(compte, "INBOX")
        } else if (essai.etape === 2 && chemin === "INBOX") {
            essai.etape = 3
            colonneListe.porteeRecherche.currentIndex = 2
            colonneListe.champRecherche.text = identifiantsEssai.dossier
            lancerRecherche()
            var dossiers = {}
            var autre = -1
            for (var i = 0; i < modeleMessages.count; ++i) {
                var r = modeleMessages.get(i)
                dossiers[r.chemin] = true
                if (autre < 0 && r.chemin !== "INBOX")
                    autre = i
            }
            console.log("scenario: résultats", modeleMessages.count, "| dossiers", Object.keys(dossiers).join(", "),
                        "| premier :", modeleMessages.count > 0 ? modeleMessages.get(0).cheminAffiche : "—",
                        "| historique", historiqueRecherches().slice(0, 3).join(" / "))
            if (autre < 0 || identifiantsEssai.sortie.length === 0)
                return
            choisir(autre, 0)
            var suite = Qt.createQmlObject('import QtQuick; Timer { interval: 3000 }', fenetre)
            suite.triggered.connect(function() {
                console.log("scenario: choisi", modeleMessages.get(autre).sujet, "| noyau sur", boite.dossierCourant,
                            "| affiché", colonneLecture.sujetAffiche.text, "| liste", modeleMessages.count, "| globale", rechercheGlobale)
                quitterRecherche()
                var fin = Qt.createQmlObject('import QtQuick; Timer { interval: 2000 }', fenetre)
                fin.triggered.connect(function() {
                    console.log("scenario: après Échap, noyau sur", boite.dossierCourant, "| liste", modeleMessages.count,
                                "| recherche", JSON.stringify(recherche), "| globale", rechercheGlobale)
                    Qt.quit()
                })
                fin.start()
            })
            suite.start()
        }
    }

    /// Scénario « recherche-objet » (0.6.38), banc MARGES=1 : « chantier »
    /// n'est que dans le corps du compte rendu de la réunion. Sans la case
    /// « Objet et expéditeur seulement », le serveur le trouve ; avec elle,
    /// rien — dans le dossier actif comme dans toutes les boîtes —, alors
    /// qu'un mot de l'objet et le nom de l'expéditeur trouvent le message. La
    /// case se coche à la souris, relance la recherche et reste dans les
    /// réglages.
    function lancerScenarioRechercheObjet(essais) {
        var n = essais || 0
        // Messages de la réunion dans la liste.
        var reunion = function() {
            var k = 0
            for (var i = 0; i < modeleMessages.count; ++i)
                if (String(modeleMessages.get(i).sujet).indexOf("réunion") >= 0)
                    ++k
            return k
        }
        var pret = comptesConnus.length >= 2 && reunion() > 0
        if (!pret && n < 40) {
            plusTard(1000, function() { lancerScenarioRechercheObjet(n + 1) })
            return
        }
        var s = { echecs: 0 }
        var cas = colonneListe.objetSeulRecherche
        var cliquer = function() { gestesEssai.clic(cas, 12, cas.height / 2, Qt.LeftButton) }
        reglages.rechercheObjetSeul = false
        colonneListe.porteeRecherche.currentIndex = 0
        colonneListe.nonLusRecherche.checked = false
        bilanEssai(s, "case décochée au départ, réglage faux", !cas.checked && !reglages.rechercheObjetSeul)
        fenetre.lancerRecherche("chantier")
        bilanEssai(s, "barre d'état : " + messageEtat.texte, messageEtat.texte.indexOf("dans le texte des messages") >= 0)
        plusTard(5000, function() {
            bilanEssai(s, "dossier actif, texte : " + sujetsDeLaListe(), reunion() === 1)
            cliquer()
            // La barre au moment du clic : le serveur du banc répond assez vite
            // pour la remplacer avant la prochaine étape.
            var barre = messageEtat.texte
            plusTard(300, function() {
                bilanEssai(s, "case cochée à la souris : réglage " + reglages.rechercheObjetSeul + ", recherche « "
                           + fenetre.recherche + " » relancée", cas.checked && reglages.rechercheObjetSeul
                           && fenetre.rechercheObjetSeul && fenetre.recherche === "chantier")
                bilanEssai(s, "barre d'état : " + barre, barre.indexOf("dans l'objet et l'expéditeur des messages") >= 0)
                plusTard(5000, function() {
                    bilanEssai(s, "dossier actif, objet et expéditeur : " + modeleMessages.count + " résultat(s)",
                               modeleMessages.count === 0)
                    bilanEssai(s, "bandeau : " + fenetre.infoDossier, fenetre.infoDossier.indexOf("objet et expéditeur seulement") >= 0)
                    fenetre.lancerRecherche("réunion nicolas")
                    plusTard(5000, function() {
                        bilanEssai(s, "objet et nom de l'expéditeur : " + sujetsDeLaListe(), reunion() === 1)
                        colonneListe.porteeRecherche.currentIndex = 2
                        fenetre.lancerRecherche("chantier")
                        plusTard(6000, function() {
                            bilanEssai(s, "toutes les boîtes, objet et expéditeur : " + modeleMessages.count + " résultat(s)",
                                       modeleMessages.count === 0)
                            bilanEssai(s, "bandeau : " + fenetre.infoDossier, fenetre.infoDossier.indexOf(", objet et expéditeur") >= 0)
                            cliquer()
                            plusTard(6000, function() {
                                bilanEssai(s, "toutes les boîtes, case décochée : " + sujetsDeLaListe()
                                           + " ; réglage " + reglages.rechercheObjetSeul,
                                           !cas.checked && !reglages.rechercheObjetSeul && reunion() === 1)
                                console.log("scenario: recherche-objet " + (s.echecs === 0 ? "OK" : "ECHEC (" + s.echecs + ")"))
                                Qt.quit()
                            })
                        })
                    })
                })
            })
        })
    }

    /// Scénario « recherche-entree » (0.6.37) : demande d'un utilisateur du
    /// 04/11/2026 — deux recherches passées contiennent le mot tapé ; Entrée
    /// doit chercher le texte tapé, et non la première suggestion. Les flèches
    /// choisissent toujours une suggestion. La frappe passe par le clavier
    /// (gestesEssai.touche), lettre par lettre, comme chez l'utilisateur.
    function lancerScenarioRechercheEntree() {
        var s = { echecs: 0 }
        var champ = colonneListe.champRecherche
        var panneau = colonneListe.panneauHistorique
        var vue = panneau.contentItem
        var taper = function(texte) {
            for (var i = 0; i < texte.length; ++i) {
                var c = texte[i]
                gestesEssai.touche(champ, c === " " ? Qt.Key_Space : Qt.Key_A + (c.charCodeAt(0) - 97), c)
            }
        }
        reglages.historiqueRecherches = JSON.stringify(["PAGES LACTALIS", "\"PAGES LACTALIS\""])
        plusTard(8000, function() {
            ouvrirDossier(comptesConnus[0].compte, "INBOX")
            plusTard(4000, function() {
                champ.forceActiveFocus()
                plusTard(300, function() {
                    taper("lactalis")
                    plusTard(400, function() {
                        bilanEssai(s, "frappe de « " + champ.text + " » : " + panneau.entrees.length + " suggestions"
                                   + (panneau.opened ? " ouvertes" : " fermées") + ", rang choisi " + vue.currentIndex,
                                   champ.text === "lactalis" && panneau.opened && panneau.entrees.length === 2
                                   && vue.currentIndex === -1)
                        gestesEssai.touche(champ, Qt.Key_Return, "\r")
                        plusTard(1500, function() {
                            bilanEssai(s, "Entrée : recherche « " + fenetre.recherche + " » (attendu « lactalis »)",
                                       fenetre.recherche === "lactalis")
                            fenetre.quitterRecherche()
                            plusTard(500, function() {
                                champ.forceActiveFocus()
                                taper("pages")
                                plusTard(400, function() {
                                    gestesEssai.touche(champ, Qt.Key_Down, "")
                                    plusTard(200, function() {
                                        var choisi = vue.currentIndex
                                        gestesEssai.touche(champ, Qt.Key_Return, "\r")
                                        plusTard(1500, function() {
                                            bilanEssai(s, "flèche bas puis Entrée : rang " + choisi + ", recherche « "
                                                       + fenetre.recherche + " » (attendu « PAGES LACTALIS »)",
                                                       choisi === 0 && fenetre.recherche === "PAGES LACTALIS")
                                            console.log("scenario: recherche-entree " + (s.echecs === 0 ? "OK" : "ECHEC (" + s.echecs + ")"))
                                            Qt.quit()
                                        })
                                    })
                                })
                            })
                        })
                    })
                })
            })
        })
    }

    function etapeScenarioReprise() {
        console.log("scenario: proposées", redactionsRetrouvees.length, "| dialogue ouvert", dlgReprise.opened)
        dlgReprise.close()
        rouvrirRedactions()
        var jetons = Object.keys(redactions)
        for (var k = 0; k < jetons.length; ++k) {
            var r = redactions[jetons[k]]
            var c = r.contenu()
            console.log("scenario: reprise", r.idGarde, "|", c.a, "|", c.objet, "|", JSON.stringify(c.texte),
                        "| modifiée", r.modifie, "| note", r.note)
            r.conteneur.fermerRedaction()
        }
        console.log("scenario: restantes", JSON.parse(boite.redactionsGardees()).length)
    }

    /// Scénario d'essai « horsligne » : sans connexion, la boîte de réception
    /// s'ouvre sur l'index et son premier message s'affiche depuis le poste.
    function etapeScenarioHorsLigne() {
        var c = comptesConnus[0]
        ouvrirDossier(c.compte, "INBOX")
        console.log("scenario: hors ligne,", modeleMessages.count, "messages dans la liste")
        if (modeleMessages.count > 0)
            choisir(0, 0)
        var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
        attente.triggered.connect(function() {
            console.log("scenario: affiché :", colonneLecture.sujetAffiche.text, "| html :", corpsHtml, "|",
                        colonneLecture.vueCorps.text.length, "caractères",
                        colonneLecture.vueCorps.text.indexOf("Chargement") === 0 || colonneLecture.vueCorps.text.length < 20 ? "ECHEC" : "OK")
        })
        attente.start()
    }

    /// Scénario d'essai « envoyes » : sans connexion, le dossier des envoyés
    /// du premier compte s'ouvre sur l'index ; sa première ligne doit nommer
    /// les destinataires (« À : … »), et non l'expéditeur.
    function etapeScenarioEnvoyes() {
        var c = comptesConnus[0]
        var envoyes = JSON.parse(boite.arborescence(false)).filter(function(l) {
            return l.genre === "dossier" && l.compte === c.compte && l.role === "Sent"
        })
        if (envoyes.length === 0) {
            console.log("scenario: envoyés : aucun dossier d'envoyés ECHEC")
            return
        }
        ouvrirDossier(c.compte, envoyes[0].chemin)
        var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 1500 }', fenetre)
        attente.triggered.connect(function() {
            var ligne = colonneListe.vueMessages.itemAtIndex(0)
            var texte = ligne ? ligne.auteurLigne : ""
            console.log("scenario: envoyés,", fenetre.roleCourant, "|", modeleMessages.count, "messages | 1re ligne :",
                        texte, texte.indexOf(qsTr("À : ")) === 0 ? "OK" : "ECHEC")
        })
        attente.start()
    }

    /// Scénario d'essai « suggestion » : sans connexion, l'entrée du menu « ? »
    /// ouvre une rédaction adressée à `adresseSuggestions`, version et système
    /// dans l'objet.
    function etapeScenarioSuggestion() {
        var r = envoyerSuggestion()
        var a = r ? r.contenu().a : ""
        var objet = r ? r.contenu().objet : ""
        console.log("scenario: suggestion,", a, "|", objet,
                    a === adresseSuggestions && objet.indexOf(Qt.application.version) > 0 ? "OK" : "ECHEC")
    }

    /// Scénario « mesure-recherche » : temps d'une recherche dans le dossier
    /// ouvert (index seul, hors connexion), du retour à la liste entière, et
    /// d'une recherche dans toutes les boîtes.
    function etapeScenarioMesureRecherche() {
        var c = comptesConnus[0]
        ouvrirDossier(c.compte, "INBOX")
        colonneListe.champRecherche.text = "Correspondant 12"
        var t = Date.now()
        lancerRecherche()
        console.log("mesure: recherche dans le dossier,", modeleMessages.count, "résultats,", Date.now() - t, "ms")
        t = Date.now()
        quitterRecherche()
        console.log("mesure: sortie de recherche,", modeleMessages.count, "messages,", Date.now() - t, "ms")
        colonneListe.porteeRecherche.currentIndex = 2
        colonneListe.champRecherche.text = "dossier 39"
        t = Date.now()
        lancerRecherche()
        console.log("mesure: recherche dans toutes les boîtes,", modeleMessages.count, "résultats,",
                    Date.now() - t, "ms")
        Qt.quit()
    }

    /// Scénario « mesure-liste » : temps d'ouverture d'INBOX depuis l'index,
    /// de trois rafraîchissements sans changement, puis d'un rafraîchissement
    /// après l'arrivée d'un message, que l'épreuve insère dans l'index pendant
    /// l'attente annoncée par « pret-insertion ».
    function etapeScenarioMesure() {
        var c = comptesConnus[0]
        fenetre.mesureSuite = { paquets: 0, plusLong: 0 }
        var t = Date.now()
        ouvrirDossier(c.compte, "INBOX")
        console.log("mesure: ouverture (première page),", modeleMessages.count, "messages,", Date.now() - t, "ms")
        // La suite, par paquets : la fenêtre ne doit jamais rester figée
        // longtemps entre deux.
        var suivi = Qt.createQmlObject('import QtQuick; Timer { interval: 10; repeat: true }', fenetre)
        suivi.triggered.connect(function() {
            if (minuteurSuite.running)
                return
            suivi.stop()
            console.log("mesure: liste complète,", modeleMessages.count, "messages en", Date.now() - t, "ms |",
                        mesureSuite.paquets, "paquets, le plus long", mesureSuite.plusLong, "ms")
            fenetre.mesureSuite = null
            var index = JSON.parse(boite.messages())
            var conforme = index.length === modeleMessages.count
            for (var i = 0; conforme && i < index.length; ++i)
                conforme = index[i].uid === modeleMessages.get(i).uid
            console.log("mesure: liste conforme à l'index :", conforme)
            // Relecture complète pendant qu'un message lointain est choisi : la
            // suite est lue d'un trait jusqu'à lui, il reste choisi.
            var loin = modeleMessages.get(30000).uid
            fenetre.uidCourant = loin
            fenetre.selection = ({ [loin]: true })
            var t3 = Date.now()
            rafraichirListe(true)
            console.log("mesure: relecture complète, message choisi au rang 30 000 :", Date.now() - t3, "ms |",
                        modeleMessages.count, "lignes | toujours choisi", uidCourant === loin && selection[loin] === true,
                        "| à son rang", indexDe(loin) === 30000)
            arreterSuite()
            fenetre.uidCourant = 0
            fenetre.selection = ({})
            rafraichirListe(true)
            completerListe()
            etapeScenarioMesureSuite()
        })
        suivi.start()
    }

    function etapeScenarioMesureSuite() {
        var t
        for (var k = 0; k < 3; ++k) {
            t = Date.now()
            rafraichirListe()
            console.log("mesure: rafraîchissement sans changement,", Date.now() - t, "ms")
        }
        // Ce que faisait la 0.4.4 à chaque rafraîchissement : la liste entière
        // relue, puis comparée ligne à ligne.
        for (k = 0; k < 3; ++k) {
            t = Date.now()
            var liste = JSON.parse(boite.messages())
            var memeForme = liste.length === modeleMessages.count
            for (var i = 0; memeForme && i < liste.length; ++i)
                memeForme = liste[i].uid === modeleMessages.get(i).uid
            for (var j = 0; memeForme && j < liste.length; ++j)
                modeleMessages.get(j).lu !== liste[j].lu
            console.log("mesure: ancien rafraîchissement sans changement,", Date.now() - t, "ms")
        }
        console.log("scenario: pret-insertion")
        var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 5000 }', fenetre)
        attente.triggered.connect(function() {
            var t2 = Date.now()
            rafraichirListe()
            console.log("mesure: rafraîchissement après une arrivée,", modeleMessages.count, "messages,",
                        Date.now() - t2, "ms | premier :", modeleMessages.get(0).sujet)
            // L'ancienne reconstruction, sur la même liste.
            t2 = Date.now()
            poserListe(JSON.parse(boite.messages()))
            console.log("mesure: ancienne reconstruction,", Date.now() - t2, "ms")
            Qt.quit()
        })
        attente.start()
    }

    /// Scénario d'essai « defilement » : l'arborescence, agrandie pour
    /// déborder et défilée tout en bas, doit remonter d'elle-même quand un
    /// élément glissé se tient contre son bord haut.
    function etapeScenarioDefilement() {
        if (essai.defilement)
            return
        essai.defilement = true
        reglages.zoomArborescence = 3
        // « favoris-fixes » : les comptes défilés tout en bas, sans glisser —
        // la capture montre les Favoris restés en haut.
        if (essai.scenario === "favoris-fixes") {
            var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 1000 }', fenetre)
            attente.triggered.connect(function() {
                var v = colonneArborescence.vueArborescence
                v.contentY = Math.max(v.originY, v.originY + v.contentHeight - v.height)
                console.log("scenario: comptes défilés à", v.contentY, "; Favoris visibles :", colonneArborescence.vueFavoris.visible, colonneArborescence.vueFavoris.count, "lignes")
            })
            attente.start()
            return
        }
        Qt.callLater(function() {
            var v = colonneArborescence.vueArborescence
            v.contentY = Math.max(v.originY, v.originY + v.contentHeight - v.height)
            var avant = v.contentY
            var sonde = Qt.createQmlObject('import QtQuick; Rectangle { width: 40; height: 16 }', fenetre.contentItem)
            var p = v.mapToItem(fenetre.contentItem, 20, 4)
            sonde.x = p.x
            sonde.y = p.y
            fenetre.glisse = sonde
            var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 3000 }', fenetre)
            attente.triggered.connect(function() {
                fenetre.glisse = null
                console.log("scenario: défilement de", avant, "à", v.contentY, "(haut :", v.originY + ")",
                            v.contentY <= v.originY + 1 ? "OK" : "ECHEC")
            })
            attente.start()
        })
    }

    /// Scénario d'essai « signature » : importe la signature Outlook déposée
    /// dans le dossier de sortie (signature.htm et ses images), la pose sur le
    /// compte, rédige un message à `MMAIL_DESTINATAIRE` et l'envoie.
    function etapeScenarioSignature() {
        if (essai.signatureFaite)
            return
        essai.signatureFaite = true
        var a = comptesConnus[0]
        var html = boite.importerSignature(a.adresse, identifiantsEssai.sortie + "/signature.htm")
        console.log("scenario: signature importée :", html.length, "caractères", boite.erreur)
        poserIdentite(a.adresse, { nom: "Essai MMail", signature: "Essai MMail", html: html,
                                   nouveaux: true, reponses: true })
        if (essai.scenario === "signature-vue") {
            dlgIdentite.ouvrir(a.compte)
            return
        }
        var r = ouvrirRedaction(a.compte)
        essai.objetSignature = "Essai signature HTML " + Date.now()
        r.remplir({ a: identifiantsEssai.utilisateur2, objet: essai.objetSignature, mode: "nouveau" })
        var corps = r.htmlActuel()
        console.log("scenario: rédaction :", (corps.match(/<img[^>]*>/g) || []).join(" "))
        r.envoyer(false)
    }

    /// Scénario d'essai « chevron » : un clic sur le chevron du dossier
    /// « Essais », tel que la ligne le reçoit, doit replier ses sous-dossiers ;
    /// un second, les déplier.
    function etapeScenarioChevron() {
        if (essai.etapeChevron === undefined)
            essai.etapeChevron = 0
        // Trois clics, chacun à un tour de boucle d'événements du précédent :
        // replier « INBOX/Fournisseurs », le déplier, replier la boîte de
        // réception.
        var cibles = identifiantsEssai.dossier ? [identifiantsEssai.dossier, identifiantsEssai.dossier]
                                               : ["INBOX/Fournisseurs", "INBOX/Fournisseurs", "INBOX"]
        if (essai.etapeChevron >= cibles.length)
            return
        var present = function(chemin) {
            for (var k = 0; k < modeleArborescence.count; ++k) {
                var l = modeleArborescence.get(k)
                if (l.genre === "dossier" && l.chemin === chemin)
                    return k
            }
            return -1
        }
        var etat = function() {
            var noms = []
            for (var k = 0; k < modeleArborescence.count; ++k) {
                var l = modeleArborescence.get(k)
                if (l.genre === "dossier")
                    noms.push(l.chemin + (l.enfants ? (l.replie ? "[+]" : "[-]") : ""))
            }
            return noms.join(" | ")
        }
        var cible = cibles[essai.etapeChevron]
        var i = present(cible)
        var ligne = i >= 0 ? colonneArborescence.vueArborescence.itemAtIndex(i) : null
        // Un clic au milieu de la place du chevron, où le doigt se pose :
        // retrait du dossier plus la moitié de la largeur du chevron.
        var x = ligne ? ligne.leftPadding + modeleArborescence.get(i).profondeur * 14 + 7 : -1
        if (ligne && !ligne.surChevron(x))
            x = -1
        console.log("scenario: clic sur le chevron de", cible, "à x =", x)
        if (x < 0)
            return
        boite.replierDossier(modeleArborescence.get(i).compte, cible, !modeleArborescence.get(i).replie)
        console.log("scenario: arbre :", etat())
        essai.etapeChevron += 1
        Qt.callLater(etapeScenarioChevron)
    }

    /// Scénario d'essai « aller-retour » (MMAIL_SCENARIO) : le premier message
    /// de la boîte de réception du premier compte part dans celle du second par
    /// les fonctions mêmes de l'interface, puis revient. Chaque étape est tracée
    /// sur la console ; sert à éprouver le tri sans personne devant l'écran.
    function etapeScenario() {
        if (!essai || essai.scenario !== "aller-retour")
            return
        var comptes = listeComptes()
        if (comptes.length < 2 || comptes[0].etat !== "pret" || comptes[1].etat !== "pret")
            return
        var a = comptes[0], b = comptes[1]
        if (essai.etape === 0 && boite.compteCourant === a.id && modeleMessages.count > 0) {
            essai.etape = 1
            essai.sujet = modeleMessages.get(0).sujet
            console.log("scenario: aller", essai.sujet, "de", a.adresse, "vers", b.adresse)
            choisir(0, 0)
            deplacerVers(b.id, "INBOX")
        } else if (essai.etape === 1 && boite.compteCourant === a.id && deplacementsEnVol === 0) {
            essai.etape = 2
            ouvrirDossier(b.id, "INBOX")
        } else if (essai.etape === 2 && boite.compteCourant === b.id) {
            for (var i = 0; i < modeleMessages.count; ++i) {
                if (modeleMessages.get(i).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: trouve dans", b.adresse, "- retour")
                    choisir(i, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC, message absent de", b.adresse)
            essai.etape = 9
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 4
            ouvrirDossier(a.id, "INBOX")
        } else if (essai.etape === 4 && boite.compteCourant === a.id) {
            var revenu = false
            for (var j = 0; j < modeleMessages.count; ++j)
                revenu = revenu || modeleMessages.get(j).sujet === essai.sujet
            console.log(revenu ? "scenario: OK, message revenu dans " + a.adresse
                               : "scenario: ECHEC, message non revenu")
            essai.etape = 9
        }
    }

    /// Scénario « images-confiance » (0.6.24), sur deux messages HTML à images
    /// distantes du même expéditeur, dans un ordre quelconque : A (DMARC
    /// réussi) et B (échec DMARC noté par le serveur). B : pas de « Toujours
    /// pour cet expéditeur ». A : « Toujours » ; puis B : rien d'office ; retour
    /// à A : images d'office. La confiance est retirée à la fin.
    function etapeScenarioImagesConfiance(uid, html, bloquees, cle) {
        var e = fenetre.enteteCourant
        var office = fenetre.imagesAutoPour === cle
        var bouton = fenetre.confianceProposee(e)
        console.log("scenario: étape", essai.etape, "| uid", uid, "| html", html, "| bloquées", bloquees,
                    "| expéditeur", e.expediteur, "| dmarcEchec", e.dmarcEchec,
                    "| confiance", fenetre.expediteurDeConfiance(e.expediteur), "| d'office", office,
                    "| bouton « Toujours »", bouton)
        if (!html)
            return
        var autre = fenetre.indexCourant === 0 ? 1 : 0
        if (e.dmarcEchec) {
            console.log(bouton ? "scenario: ECHEC, « Toujours » proposé malgré l'échec DMARC"
                               : "scenario: OK, « Toujours » absent (DMARC)")
            if (essai.etape === 2) {
                console.log(office ? "scenario: ECHEC, images d'office malgré l'échec DMARC"
                                   : "scenario: OK, rien d'office (DMARC)")
                essai.etape = 3
                fenetre.choisir(autre, 0)
            } else if (essai.etape === 0 && !essai.vuB) {
                essai.vuB = true
                fenetre.choisir(autre, 0)
            }
            return
        }
        if (essai.etape === 0 && bloquees > 0) {
            console.log(bouton ? "scenario: OK, « Toujours » proposé" : "scenario: ECHEC, « Toujours » absent")
            essai.etape = 1
            fenetre.retenirExpediteurConfiance(e.expediteur, true)
            fenetre.demanderImages()
        } else if (essai.etape === 1) {
            console.log(bloquees === 0 ? "scenario: OK, images téléchargées"
                                       : "scenario: ECHEC, images encore bloquées")
            console.log("scenario: liste", JSON.stringify(fenetre.expediteursConfiance()))
            essai.etape = 2
            fenetre.choisir(autre, 0)
        } else if (essai.etape === 3) {
            essai.etape = office ? 4 : 9
            console.log(office ? "scenario: OK, images d'office au retour"
                               : "scenario: ECHEC, pas d'images d'office au retour")
        } else if (essai.etape === 4) {
            console.log(bloquees === 0 ? "scenario: OK, images d'office téléchargées"
                                       : "scenario: ECHEC, images d'office encore bloquées")
            essai.etape = 9
            fenetre.retenirExpediteurConfiance(e.expediteur, false)
            console.log("scenario: confiance retirée, liste", JSON.stringify(fenetre.expediteursConfiance()))
            console.log("scenario: fin")
        }
    }

    /// Scénario d'essai « pieces » (MMAIL_SCENARIO) : dans la boîte de
    /// réception du premier compte, le premier message portant une pièce
    /// jointe est affiché, et sa première pièce enregistrée dans le dossier
    /// désigné par MMAIL_SORTIE. Sert à éprouver la chaîne sans écran.
    /// Scénario « ouvrir-avec » : MMAIL_DOSSIER désigne un programme d'essai.
    /// Retenu pour les .pdf, il ouvre le PDF du premier message ; « Ouvrir
    /// avec… » ponctuel sur le tableur ; puis le choix est retiré.
    function etapeScenarioOuvrirAvec() {
        if (essai.etape !== 0 || pieces.length === 0)
            return
        essai.etape = 1
        var programme = identifiantsEssai.dossier
        var de = function(ext) { return pieces.filter(function(p) { return extensionDe(p.nom) === ext })[0] }
        retenirLogiciel("pdf", programme)
        console.log("scenario: retenu pour .pdf :", lanceur.nom(logicielPour(de("pdf").nom)))
        ouvrirPiece(de("pdf"))
        var suite = Qt.createQmlObject('import QtQuick; Timer { interval: 2500 }', fenetre)
        suite.triggered.connect(function() {
            console.log("scenario: après le PDF :", messageEtat.texte)
            ouvrirPiece(de("xlsx"), programme)
            var fin = Qt.createQmlObject('import QtQuick; Timer { interval: 2500 }', fenetre)
            fin.triggered.connect(function() {
                console.log("scenario: après le tableur :", messageEtat.texte)
                retenirLogiciel("pdf", "")
                console.log("scenario: retenus après retrait :", JSON.stringify(logicielsPieces()))
                Qt.quit()
            })
            fin.start()
        })
        suite.start()
    }

    function etapeScenarioPieces() {
        if (essai.etape === 0 && pieces.length > 0) {
            essai.etape = 1
            console.log("scenario: pieces", JSON.stringify(pieces))
            boite.enregistrerPiece(uidCourant, pieces[0].indice,
                                   identifiantsEssai.sortie + "/" + encodeURIComponent(pieces[0].nom))
        } else if (essai.etape === 0 && essai.examines < modeleMessages.count - 1) {
            essai.examines += 1
            choisir(essai.examines, 0)
        }
    }

    /// Scénario « glisser » (MMAIL_SCENARIO) : deux dossiers déposés sur la
    /// rubrique Favoris puis l'un sur l'autre, et un message déposé sur un
    /// dossier puis ramené. Chaque étape est tracée ; les favoris sont retirés
    /// à la fin.
    function etapeScenarioGlisser() {
        var comptes = listeComptes()
        if (comptes.length < 2 || comptes[0].etat !== "pret" || comptes[1].etat !== "pret")
            return
        var a = comptes[0], b = comptes[1]
        if (essai.etape === 0 && boite.compteCourant === a.id && modeleMessages.count > 0) {
            essai.etape = 1
            var ok1 = deposerSonde(indexLigne("rubrique", 0, ""), "mmail/dossier", a.id, "Essais")
            var ok2 = deposerSonde(indexLigne("favori", a.id, "Essais"), "mmail/dossier", b.id, "INBOX")
            var ordre = favorisActuels()
            console.log("scenario: favoris", ok1, ok2, JSON.stringify(ordre))
            console.log(ordre.length === 2 && ordre[0] === b.adresse + ":INBOX" && ordre[1] === a.adresse + ":Essais"
                        ? "scenario: OK favoris ordonnes" : "scenario: ECHEC favoris")
            // Un dossier déposé sur un dossier ordinaire ne fait rien.
            var refus = deposerSonde(indexLigne("dossier", a.id, "Archive"), "mmail/dossier", a.id, "Essais")
            console.log(refus ? "scenario: ECHEC dossier accepte hors Favoris" : "scenario: OK dossier refuse hors Favoris")
            // Un message déposé sur un dossier y part.
            essai.sujet = modeleMessages.get(0).sujet
            choisir(0, 0)
            var ok3 = deposerSonde(indexLigne("dossier", a.id, "Essais/Clients"), "mmail/messages", 0, "")
            console.log("scenario: message depose", ok3, essai.sujet)
        } else if (essai.etape === 1 && deplacementsEnVol === 0) {
            essai.etape = 2
            ouvrirDossier(a.id, "Essais/Clients")
        } else if (essai.etape === 2 && boite.dossierCourant === "Essais/Clients") {
            for (var i = 0; i < modeleMessages.count; ++i) {
                if (modeleMessages.get(i).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: OK message arrive dans Essais/Clients")
                    choisir(i, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC message absent de Essais/Clients")
            essai.etape = 9
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 9
            boite.epinglerDossier(a.id, "Essais", false)
            boite.epinglerDossier(b.id, "INBOX", false)
            console.log("scenario: message ramene, favoris retires :", JSON.stringify(favorisActuels()))
        }
    }

    /// Scénario « corbeille-ligne » : dans la boîte de réception, le premier
    /// message affiché, la corbeille de la deuxième ligne l'envoie à la
    /// corbeille sans toucher au message affiché ni à la sélection ; il y est
    /// retrouvé, puis ramené.
    // Scénario « en-tete » : l'objet et l'expéditeur du message affiché se
    // sélectionnent et se copient ; « Enregistrer sous… » propose le dernier
    // dossier choisi s'il existe, sinon les téléchargements.
    function etapeScenarioEnTete() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret" || essai.etape || boite.dossierCourant !== "INBOX" || modeleMessages.count < 1)
            return
        essai.etape = 1
        choisir(0, 0)
        var ligne = modeleMessages.get(0)
        colonneLecture.sujetAffiche.selectAll()
        console.log("scenario: objet sélectionnable", colonneLecture.sujetAffiche.selectByMouse && colonneLecture.sujetAffiche.readOnly,
                    "| sélection = objet", colonneLecture.sujetAffiche.selectedText === ligne.sujet)
        menuEnTete.texteChoisi = colonneLecture.sujetAffiche.selectedText
        copierTexte(menuEnTete.texteChoisi, "message")
        console.log("scenario: copie de l'objet", pressePapier.texte === ligne.sujet)
        copierTexte(adresseAffichee, "message")
        console.log("scenario: copie de l'adresse", adresseAffichee.length > 0 && pressePapier.texte === ligne.adresse,
                    "|", adresseAffichee)
        colonneLecture.auteurAffiche.selectAll()
        console.log("scenario: expéditeur sélectionnable", colonneLecture.auteurAffiche.selectedText.indexOf(ligne.adresse) >= 0)
        reglagesPieces.dernierDossier = ""
        var d1 = dossierEnregistrement()
        reglagesPieces.dernierDossier = "file:///tmp"
        var d2 = dossierEnregistrement()
        reglagesPieces.dernierDossier = "file:///tmp/mmail-dossier-absent"
        var d3 = dossierEnregistrement()
        reglagesPieces.dernierDossier = ""
        console.log("scenario: dossier d'enregistrement", d1 === dossierTelechargements, d2 === "file:///tmp",
                    d3 === dossierTelechargements)
        Qt.quit()
    }

    // Scénario « accessibilite » : ce que les lecteurs d'écran recevront des
    // lignes de la liste et de l'arborescence (rôle, nom, sélection), lu sur
    // les objets eux-mêmes — la plateforme sans écran n'a pas de pont
    // d'accessibilité à interroger.
    function etapeScenarioAccessibilite() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret" || essai.etape || boite.dossierCourant !== "INBOX" || modeleMessages.count < 1)
            return
        essai.etape = 1
        choisir(0, 0)
        for (var i = 0; i < Math.min(3, modeleMessages.count); ++i) {
            var l = colonneListe.vueMessages.itemAtIndex(i)
            if (l)
                console.log("scenario: message", i, "| rôle", l.Accessible.role === Accessible.ListItem,
                            "| choisi", l.Accessible.selected, "|", l.Accessible.name)
        }
        for (var k = 0; k < Math.min(8, colonneArborescence.vueArborescence.count); ++k) {
            var d = colonneArborescence.vueArborescence.itemAtIndex(k)
            if (d)
                console.log("scenario: arborescence", k, "| rôle", d.Accessible.role === Accessible.TreeItem,
                            "| courant", d.Accessible.selected, "|", d.Accessible.name)
        }
        Qt.quit()
    }

    // Scénario « dialogues » : chaque dialogue et menu s'ouvre puis se referme,
    // l'un après l'autre, sans rien valider — leurs liaisons sont évaluées, une
    // référence cassée paraît sur la console. Sert à comparer deux versions.
    function etapeScenarioDialogues() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret" || essai.etape || boite.dossierCourant !== "INBOX" || modeleMessages.count < 1)
            return
        essai.etape = 1
        choisir(0, 0)
        var cibles = [["dlgIdentite", dlgIdentite], ["dlgAide", dlgAide], ["dlgDeplacer", dlgDeplacer],
                      ["dlgRestauration", dlgRestauration], ["dlgAPropos", dlgAPropos], ["dlgCompte", dlgCompte],
                      ["dlgNouveauDossier", dlgNouveauDossier], ["dlgRenommerDossier", dlgRenommerDossier],
                      ["dlgConsentement", dlgConsentement],
                      ["dlgOuvrirAvec", dlgOuvrirAvec], ["dlgLogiciels", dlgLogiciels], ["dlgExpediteursConfiance", dlgExpediteursConfiance], ["dlgReprise", dlgReprise],
                      ["dlgLien", dlgLien], ["dlgViderDossier", dlgViderDossier], ["popupRappels", popupRappels],
                      ["menuComptes", menuComptes], ["menuReglages", menuReglages], ["menuAffichage", menuAffichage], ["menuPiece", menuPiece],
                      ["menuMessage", menuMessage], ["menuDossier", menuDossier], ["menuCompte", menuCompte]]
        var i = 0
        var t = Qt.createQmlObject("import QtQuick; Timer { interval: 400; repeat: true }", fenetre)
        t.triggered.connect(function() {
            if (i > 0)
                cibles[i - 1][1].close()
            if (i >= cibles.length) {
                t.stop()
                console.log("scenario: dialogues et menus parcourus :", cibles.length)
                Qt.quit()
                return
            }
            var c = cibles[i]
            if (typeof c[1].popup === "function")
                c[1].popup()
            else
                c[1].open()
            console.log("scenario: ouvert", c[0], c[1].opened === true)
            i += 1
        })
        t.start()
    }

    // Scénario « tactile » : la vue du téléphone en images, sans téléphone
    // (MMAIL_TACTILE=1, MMAIL_TAILLE=412x915) — liste, tiroir, menu ⋮, message.
    function etapeScenarioTactile() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret" || essai.etape || boite.dossierCourant !== "INBOX" || modeleMessages.count < 1)
            return
        essai.etape = 1
        var racine = fenetre.contentItem
        while (racine.parent)
            racine = racine.parent
        function saisir(nom) {
            var chemin = lanceur.chemin(identifiantsEssai.sortie + "/tactile-" + nom + ".png")
            racine.grabToImage(function(r) { r.saveToFile(chemin) })
            console.log("scenario: tactile", nom, "| vue", vue, "| tiroir", tiroir.opened,
                        "| menu", menuPlus.opened || menuComptes.opened || menuReglages.opened, "| titre", boite.dossierCourant,
                        "| barre", entete.height)
        }
        var etapes = [
            function() { saisir("liste") },
            function() { tiroir.open() },
            function() { saisir("tiroir") },
            function() { tiroir.close(); menuPlus.popup(fenetre.width - 10, 0) },
            function() { saisir("menu") },
            function() { menuPlus.close(); menuComptes.popup(boutonPlus, boutonPlus.width - 260, boutonPlus.height) },
            function() { saisir("comptes") },
            function() { menuComptes.close(); menuReglages.popup(boutonPlus, boutonPlus.width - 260, boutonPlus.height) },
            function() { saisir("reglages") },
            function() { menuReglages.close(); bandeauGlissement.texte = qsTr("Message mis à la corbeille."); bandeauGlissement.open() },
            function() { saisir("annuler") },
            function() { bandeauGlissement.close(); choisir(0, 0) },
            function() { saisir("message") },
            function() { vue = 1; rediger("nouveau") },
            function() { saisir("redaction") },
            // Retour à la liste d’abord : depuis le message, la fermeture (et Qt.quit)
            // est le retour arrière d’Android, qui ramène à la liste sans quitter.
            function() { console.log("scenario: tactile fin | tactile", tactile, "| compact", compact); vue = 1; Qt.quit() }
        ]
        var i = 0
        var t = Qt.createQmlObject("import QtQuick; Timer { interval: 700; repeat: true }", fenetre)
        t.triggered.connect(function() {
            if (i < etapes.length)
                etapes[i++]()
        })
        t.start()
    }

    // Scénario « glisser-ligne » : une ligne glissée vers la corbeille, puis
    // annulée — elle revient ; glissée de nouveau et laissée partir — elle est
    // dans la corbeille ; ramenée enfin en boîte de réception.
    function etapeScenarioGlisserLigne() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret")
            return
        if (essai.etape === 0 && boite.dossierCourant === "INBOX" && modeleMessages.count >= 2) {
            essai.etape = 1
            essai.sujet = modeleMessages.get(1).sujet
            var avant = modeleMessages.count
            // Comme au doigt : la ligne est d'abord glissée (ouverte).
            var glissee = colonneListe.vueMessages.itemAtIndex(1)
            if (glissee)
                glissee.swipe.open(SwipeDelegate.Right)
            glisserLigne(1, "corbeille")
            console.log("scenario: glissée", avant, "->", modeleMessages.count, "| bandeau", bandeauGlissement.opened,
                        "|", bandeauGlissement.texte, "|", essai.sujet)
            annulerGlissement()
            var revenue = -1
            for (var i = 0; i < modeleMessages.count; ++i) {
                if (modeleMessages.get(i).sujet === essai.sujet)
                    revenue = i
            }
            console.log(revenue >= 0 && modeleMessages.count === avant && !bandeauGlissement.opened
                        ? "scenario: OK annulée, ligne revenue au rang " + revenue
                        : "scenario: ECHEC annulation " + modeleMessages.count + " / " + avant)
            // Retour de Manu du 18/10 : après un glisser, d'autres lignes
            // restaient décalées. Une fois la liste redessinée, aucune ligne
            // affichée ne doit l'être.
            essai.etape = 5  // en attente du contrôle : les autres étapes se taisent
            // La liste recycle ses lignes : faire défiler jusqu'au bout et
            // revenir reprend les lignes rangées, dont la ligne glissée.
            var vue = colonneListe.vueMessages
            var etapesControle = [
                function() { vue.positionViewAtEnd() },
                function() { vue.positionViewAtBeginning() },
                function() {
                    var decalees = []
                    for (var d = 0; d < modeleMessages.count; ++d) {
                        var l = vue.itemAtIndex(d)
                        if (l && l.visible && l.swipe.position !== 0)
                            decalees.push(d + ":" + l.swipe.position)
                    }
                    console.log(decalees.length === 0 ? "scenario: OK aucune ligne décalée après l'annulation"
                                                       : "scenario: ECHEC lignes décalées " + decalees.join(" "))
                    var rang = -1
                    for (var i = 0; i < modeleMessages.count; ++i) {
                        if (modeleMessages.get(i).sujet === essai.sujet)
                            rang = i
                    }
                    glisserLigne(rang, "corbeille")
                    essai.etape = 1
                    console.log("scenario: glissée de nouveau, en attente", Object.keys(enAttente).length)
                }
            ]
            var controle = Qt.createQmlObject("import QtQuick; Timer { interval: 600; repeat: true }", fenetre)
            controle.triggered.connect(function() {
                etapesControle.shift()()
                if (etapesControle.length === 0)
                    controle.stop()
            })
            controle.start()
        } else if (essai.etape === 1 && glissement === null && deplacementsEnVol === 0) {
            essai.etape = 2
            var corbeille = ""
            var parcourir = function(noeuds) {
                for (var i = 0; i < noeuds.length && !corbeille; ++i) {
                    if (noeuds[i].compte === a.id && noeuds[i].role === "Trash")
                        corbeille = noeuds[i].chemin
                    else if (noeuds[i].enfants)
                        parcourir(noeuds[i].enfants)
                }
            }
            parcourir(JSON.parse(boite.arborescence(true)))
            essai.corbeille = corbeille
            ouvrirDossier(a.id, corbeille)
        } else if (essai.etape === 2 && boite.dossierCourant === essai.corbeille) {
            for (var j = 0; j < modeleMessages.count; ++j) {
                if (modeleMessages.get(j).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: OK message dans la corbeille")
                    choisir(j, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC message absent de la corbeille")
            essai.etape = 9
            Qt.quit()
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 9
            console.log("scenario: message ramene en boite de reception")
            Qt.quit()
        }
    }

    // Scénario « vider-dossier » (0.6.19) : un message de la boîte de
    // réception part à la corbeille ; la corbeille, ouverte, est vidée par
    // « Vider ce dossier… », confirmé comme au clic. La liste doit être vide
    // dès l'issue du vidage — jusqu'en 0.6.18, elle gardait les messages
    // effacés jusqu'à la synchronisation suivante.
    function etapeScenarioViderDossier() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret")
            return
        if (essai.etape === 0 && boite.dossierCourant === "INBOX" && modeleMessages.count >= 1) {
            essai.etape = 1
            choisir(0, 0)
            supprimerLigne(0)
            console.log("scenario: message supprimé, en attente", deplacementsEnVol)
        } else if (essai.etape === 1 && deplacementsEnVol === 0) {
            essai.etape = 2
            var corbeille = ""
            var parcourir = function(noeuds) {
                for (var i = 0; i < noeuds.length && !corbeille; ++i) {
                    if (noeuds[i].compte === a.id && noeuds[i].role === "Trash")
                        corbeille = noeuds[i].chemin
                    else if (noeuds[i].enfants)
                        parcourir(noeuds[i].enfants)
                }
            }
            parcourir(JSON.parse(boite.arborescence(true)))
            essai.corbeille = corbeille
            ouvrirDossier(a.id, corbeille)
        } else if (essai.etape === 2 && boite.dossierCourant === essai.corbeille && modeleMessages.count >= 1) {
            essai.etape = 3
            console.log("scenario: corbeille ouverte,", modeleMessages.count, "message(s) listé(s)")
            dlgViderDossier.ouvrir(a.id, essai.corbeille, essai.corbeille)
            dlgViderDossier.accept()
        }
    }

    /// Issue du vidage du scénario « vider-dossier » : la liste telle que
    /// l'utilisateur la voit, et la barre d'état.
    function bilanScenarioViderDossier(nombre) {
        console.log("scenario: vidé,", nombre, "message(s) | liste :", modeleMessages.count,
                    modeleMessages.count === 0 ? "OK vide" : "ECHEC messages encore listés",
                    "| état :", messageEtat.texte)
        Qt.quit()
    }

    // Scénario « vider-partiellement » (0.6.24) : deux messages de la boîte
    // de réception partent à la corbeille. Dans la corbeille, « Supprimer »
    // demande confirmation et efface le premier pour de bon ; puis « Vider
    // partiellement… » compte les messages reçus avant le 01/01/2000 (aucun),
    // puis, avant le 01/01/2100, ceux qui ont l'objet du second (un seul), et
    // l'efface. Chaque fois, la liste ne doit plus montrer le message effacé.
    function etapeScenarioViderPartiellement() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret")
            return
        if (essai.etape === 0 && boite.dossierCourant === "INBOX" && modeleMessages.count >= 3) {
            essai.etape = 1
            essai.sujets = [modeleMessages.get(1).sujet, modeleMessages.get(2).sujet]
            var choix = {}
            choix[modeleMessages.get(1).uid] = true
            choix[modeleMessages.get(2).uid] = true
            fenetre.selection = choix
            console.log("scenario: effaçable, boîte de réception :", boite.effacable(a.id, "INBOX"))
            supprimerSelection()
            console.log("scenario: deux messages à la corbeille, confirmation demandée :", dlgEffacer.visible)
        } else if (essai.etape === 1 && deplacementsEnVol === 0) {
            essai.etape = 2
            var corbeille = ""
            var parcourir = function(noeuds) {
                for (var i = 0; i < noeuds.length && !corbeille; ++i) {
                    if (noeuds[i].compte === a.id && noeuds[i].role === "Trash")
                        corbeille = noeuds[i].chemin
                    else if (noeuds[i].enfants)
                        parcourir(noeuds[i].enfants)
                }
            }
            parcourir(JSON.parse(boite.arborescence(true)))
            essai.corbeille = corbeille
            ouvrirDossier(a.id, corbeille)
        } else if (essai.etape === 2 && boite.dossierCourant === essai.corbeille
                   && rangDuSujet(essai.sujets[0]) >= 0 && rangDuSujet(essai.sujets[1]) >= 0) {
            essai.etape = 3
            console.log("scenario: corbeille ouverte, effaçable :", boite.effacable(a.id, essai.corbeille))
            choisir(rangDuSujet(essai.sujets[0]), 0)
            supprimerSelection()
            console.log("scenario: Supprimer dans la corbeille | confirmation :", dlgEffacer.visible,
                        "|", dlgEffacer.title, "|", dlgEffacer.uids.length, "message(s)")
            dlgEffacer.accept()
        }
    }
    function rangDuSujet(sujet) {
        for (var i = 0; i < modeleMessages.count; ++i) {
            if (modeleMessages.get(i).sujet === sujet)
                return i
        }
        return -1
    }
    /// Messages effacés pour de bon (`messagesEffaces`).
    function effacementScenarioViderPartiellement(nombre, erreur) {
        var a = listeComptes()[0]
        var efface = essai.etape === 3 ? essai.sujets[0] : essai.sujets[1]
        console.log("scenario: effacés", nombre, erreur.length > 0 ? "| erreur : " + erreur : "",
                    "| encore listé :", rangDuSujet(efface) >= 0 ? "ECHEC oui" : "OK non",
                    "| état :", messageEtat.texte)
        if (essai.etape === 3) {
            essai.etape = 4
            dlgViderPartiellement.ouvrir(a.id, essai.corbeille, essai.corbeille)
            dlgViderPartiellement.avant = "01/01/2000"
            dlgViderPartiellement.compter()
            console.log("scenario: décompte avant le 01/01/2000 demandé |", dlgViderPartiellement.nombre)
        } else {
            Qt.quit()
        }
    }
    /// Décompte d'un vidage partiel (`purgePreparee`).
    function decompteScenarioViderPartiellement(nombre, erreur) {
        console.log("scenario: décompte", nombre, erreur, "|", dlgViderPartiellement.nombre)
        if (essai.etape === 4) {
            essai.etape = 5
            console.log(nombre === 0 ? "scenario: OK aucun message avant 2000" : "scenario: ECHEC " + nombre + " avant 2000")
            dlgViderPartiellement.avant = "01/01/2100"
            dlgViderPartiellement.conditions = [{ champ: "objet", operateur: "est", valeur: essai.sujets[1] }]
            dlgViderPartiellement.changer()
            dlgViderPartiellement.compter()
        } else if (essai.etape === 5) {
            essai.etape = 6
            console.log(nombre === 1 ? "scenario: OK un seul message de cet objet" : "scenario: ECHEC " + nombre + " de cet objet")
            dlgViderPartiellement.accept()
        }
    }

    // Scénario « filtres » (0.6.0) : un filtre incomplet refusé ; un filtre
    // sur l'objet d'un message de la boîte de réception, qui l'envoie dans
    // « Essais » quand on l'applique ; le message y est, on le ramène — et les
    // filtres ne le renvoient pas ; la liste est vidée à la fin.
    function etapeScenarioFiltres() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret")
            return
        if (essai.etape === 0 && boite.dossierCourant === "INBOX" && modeleMessages.count >= 2) {
            essai.etape = 1
            essai.sujet = modeleMessages.get(1).sujet
            var refus = boite.poserFiltres(JSON.stringify([{ nom: "Incomplet", conditions: [], lu: true }]))
            console.log("scenario: filtre incomplet", refus ? "ECHEC accepté" : "OK refusé : " + boite.erreur)
            var filtres = [{ nom: "Essai des filtres", actif: true, compte: a.adresse, tout: true,
                             conditions: [{ champ: "objet", operateur: "est", valeur: essai.sujet }],
                             lu: false, suivi: false, destination: "dossier", dossier: "Essais", arreter: false }]
            console.log("scenario: filtre posé", boite.poserFiltres(JSON.stringify(filtres)),
                        "| relu", JSON.parse(boite.filtres()).length)
            var touches = boite.appliquerFiltres(a.id)
            console.log(touches === 1 ? "scenario: OK un message touché par le filtre"
                                      : "scenario: ECHEC messages touchés " + touches)
        } else if (essai.etape === 1 && deplacementsEnVol === 0) {
            essai.etape = 2
            ouvrirDossier(a.id, "Essais")
        } else if (essai.etape === 2 && boite.dossierCourant === "Essais") {
            for (var j = 0; j < modeleMessages.count; ++j) {
                if (modeleMessages.get(j).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: OK message rangé dans Essais par le filtre")
                    choisir(j, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC message absent d'Essais")
            essai.etape = 9
            boite.poserFiltres("[]")
            Qt.quit()
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 4
            ouvrirDossier(a.id, "INBOX")
        } else if (essai.etape === 4 && boite.dossierCourant === "INBOX") {
            essai.etape = 9
            var encore = boite.appliquerFiltres(a.id)
            console.log(encore === 0 ? "scenario: OK message ramené laissé en boîte de réception"
                                     : "scenario: ECHEC filtre réappliqué au message ramené : " + encore)
            boite.poserFiltres("[]")
            console.log("scenario: filtres vidés", JSON.parse(boite.filtres() || "[]").length)
            Qt.quit()
        }
    }

    // Scénario « image-filtres » : le dialogue des filtres en image, avec un
    // filtre à l'essai (rien n'est enregistré).
    function etapeScenarioImageFiltres() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret" || essai.etape || boite.dossierCourant !== "INBOX" || modeleMessages.count < 1)
            return
        essai.etape = 1
        dlgFiltres.ouvrir({ nom: "Factures des fournisseurs", actif: true, compte: a.adresse, tout: false,
                            conditions: [{ champ: "objet", operateur: "contient", valeur: "facture" },
                                         { champ: "expediteur", operateur: "finit", valeur: "@fournisseur.fr" }],
                            lu: true, suivi: false, destination: "dossier", dossier: "Essais", arreter: true })
        var racine = fenetre.contentItem
        while (racine.parent)
            racine = racine.parent
        var t = Qt.createQmlObject("import QtQuick; Timer { interval: 900 }", fenetre)
        t.triggered.connect(function() {
            var chemin = lanceur.chemin(identifiantsEssai.sortie + "/filtres.png")
            racine.grabToImage(function(r) {
                r.saveToFile(chemin)
                console.log("scenario: dialogue des filtres en image", dlgFiltres.opened, dlgFiltres.liste.length)
                dlgFiltres.close()
                Qt.quit()
            })
        })
        t.start()
    }

    // Scénario « notification » (0.6.2) : un message à soi-même, d'objet
    // unique ; à son arrivée dans la boîte de réception, le noyau le signale
    // (nouveauCourrier) ; il est ensuite mis à la corbeille.
    function lancerScenarioNotification() {
        var c = comptesConnus[0]
        var n = ouvrirRedaction(c.compte)
        n.pret()
        essai.sujet = "Essai notification " + Date.now()
        n.remplir({ a: identifiantsEssai.utilisateur, objet: essai.sujet, mode: "nouveau", texte: "Arrivée signalée." })
        n.envoyer(false)
        console.log("scenario: message envoyé à soi-même")
    }
    function etapeScenarioNotification(nombre, expediteur, sujet, montrer, faite) {
        console.log("scenario: nouveau courrier", nombre, "|", expediteur, "|", sujet,
                    "| notification", montrer ? (faite ? "montrée" : "demandée, sans service de notification") : "non demandée (fenêtre au premier plan)")
        if (sujet !== essai.sujet || essai.etape)
            return
        essai.etape = 1
        console.log("scenario: OK arrivée signalée")
        var t = Qt.createQmlObject("import QtQuick; Timer { interval: 3000 }", fenetre)
        t.triggered.connect(function() {
            for (var i = 0; i < modeleMessages.count; ++i) {
                if (modeleMessages.get(i).sujet === essai.sujet) {
                    supprimerLigne(i)
                    console.log("scenario: message d'essai mis à la corbeille")
                    break
                }
            }
            var fin = Qt.createQmlObject("import QtQuick; Timer { interval: 4000 }", fenetre)
            fin.triggered.connect(function() { Qt.quit() })
            fin.start()
        })
        t.start()
    }

    // Scénario « impression » (0.6.3) : le premier message de la boîte de
    // réception, affiché, est enregistré en PDF dans le dossier de l'essai.
    function etapeScenarioImpression() {
        if (essai.etape || uidCourant <= 0 || colonneLecture.vueCorps.text === qsTr("Chargement…"))
            return
        essai.etape = 1
        var chemin = lanceur.chemin(identifiantsEssai.sortie + "/message.pdf")
        var ok = impression.enregistrerPdf(colonneLecture.vueCorps.textDocument, enteteImpression(), sujetCourant(), chemin)
        console.log("scenario: impression disponible", impression.disponible, "| PDF", ok, "|", sujetCourant())
        Qt.quit()
    }

    function etapeScenarioCorbeille() {
        var a = listeComptes()[0]
        if (!a || a.etat !== "pret")
            return
        if (essai.etape === 0 && boite.dossierCourant === "INBOX" && modeleMessages.count >= 2) {
            essai.etape = 1
            choisir(0, 0)
            var affiche = modeleMessages.get(0).uid
            var avant = modeleMessages.count
            essai.sujet = modeleMessages.get(1).sujet
            supprimerLigne(1)
            var garde = uidCourant === affiche && selection[affiche] === true && uidsChoisis().length === 1
            console.log("scenario: ligne retiree", avant, "->", modeleMessages.count, "|", essai.sujet)
            console.log(modeleMessages.count === avant - 1 && garde
                        ? "scenario: OK message affiche et selection inchanges"
                        : "scenario: ECHEC affiche " + uidCourant + " / " + affiche)
        } else if (essai.etape === 1 && deplacementsEnVol === 0) {
            essai.etape = 2
            var corbeille = ""
            var parcourir = function(noeuds) {
                for (var i = 0; i < noeuds.length && !corbeille; ++i) {
                    if (noeuds[i].compte === a.id && noeuds[i].role === "Trash")
                        corbeille = noeuds[i].chemin
                    else if (noeuds[i].enfants)
                        parcourir(noeuds[i].enfants)
                }
            }
            parcourir(JSON.parse(boite.arborescence(true)))
            essai.corbeille = corbeille
            console.log("scenario: corbeille", corbeille)
            ouvrirDossier(a.id, corbeille)
        } else if (essai.etape === 2 && boite.dossierCourant === essai.corbeille) {
            for (var j = 0; j < modeleMessages.count; ++j) {
                if (modeleMessages.get(j).sujet === essai.sujet) {
                    essai.etape = 3
                    console.log("scenario: OK message dans la corbeille")
                    choisir(j, 0)
                    deplacerVers(a.id, "INBOX")
                    return
                }
            }
            console.log("scenario: ECHEC message absent de la corbeille")
            essai.etape = 9
            Qt.quit()
        } else if (essai.etape === 3 && deplacementsEnVol === 0) {
            essai.etape = 9
            console.log("scenario: message ramene en boite de reception")
            Qt.quit()
        }
    }


    /// Scénario « non-lu » (0.6.7, demande de Gwenaëlle du 19/10) : un
    /// « Marquer comme non lu » doit tenir, sur la ligne, dans l'index et sur le
    /// serveur. A : message affiché puis marqué lu à l'affichage, puis non lu
    /// avant toute veille — la 0.6.6 laissait la ligne en « lu ». B : non lu
    /// demandé pendant le chargement du message — la 0.6.6 le remarquait lu
    /// sur le serveur. La boîte de réception doit compter deux messages.
    function lancerScenarioNonLu() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, a: 0, b: 0, echecs: 0 }
        var luDansLaListe = function(uid) {
            var i = indexDe(uid)
            return i >= 0 ? modeleMessages.get(i).lu : null
        }
        var luDansLIndex = function(uid) {
            var l = JSON.parse(boite.messagesParUid(String(uid)))
            return l.length > 0 ? l[0].lu : null
        }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                if (modeleMessages.count < 2) {
                    bilan("non-lu : " + modeleMessages.count + " message(s) en boîte de réception, il en faut deux", false)
                    suivi.n = 39
                    return
                }
                suivi.a = modeleMessages.get(0).uid
                suivi.b = modeleMessages.get(1).uid
                // Les deux non lus au départ.
                boite.marquerLu(suivi.a + "," + suivi.b, false)
                rafraichirListe()
            } else if (suivi.n === 10) {
                // A : affiché, donc marqué lu à l'affichage.
                choisir(indexDe(suivi.a), 0)
            } else if (suivi.n === 13) {
                bilan("A : marqué lu à l'affichage | liste " + luDansLaListe(suivi.a), luDansLaListe(suivi.a) === true)
                marquerSelection(false)
                bilan("A : « Marquer comme non lu » aussitôt | liste " + luDansLaListe(suivi.a)
                      + " | index " + luDansLIndex(suivi.a),
                      luDansLaListe(suivi.a) === false && luDansLIndex(suivi.a) === false)
            } else if (suivi.n === 15) {
                // B : choisi, le chargement demandé, et « non lu » dans le même
                // tour de la boucle d'événements — avant que le corps n'arrive.
                choisir(indexDe(suivi.b), 0)
                minuterieCorps.stop()
                boite.demanderCorps(suivi.b)
                marquerSelection(false)
            } else if (suivi.n === 19) {
                bilan("B : non lu pendant le chargement | liste " + luDansLaListe(suivi.b)
                      + " | index " + luDansLIndex(suivi.b),
                      luDansLaListe(suivi.b) === false && luDansLIndex(suivi.b) === false)
                // Le serveur : un autre dossier, puis la boîte de réception
                // relue — ses drapeaux reviennent du serveur.
                fenetre.selection = ({})
                uidCourant = 0
                ouvrirDossier(c.compte, "Essais")
            } else if (suivi.n === 22) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 24) {
                boite.actualiser()
            } else if (suivi.n === 30) {
                bilan("serveur : A " + luDansLIndex(suivi.a) + " | B " + luDansLIndex(suivi.b),
                      luDansLIndex(suivi.a) === false && luDansLIndex(suivi.b) === false)
                console.log("scenario: non-lu, " + (suivi.echecs === 0 ? "tout OK" : suivi.echecs + " échec(s)"))
                suivi.n = 39
            } else if (suivi.n >= 40) {
                tic.stop()
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « defilement-lecture » (0.6.10) : retirer la sélection du message ne
    /// ramène plus la lecture en haut. Sous Android, chaque début de
    /// glissement la retire ; le TextArea, dans son ScrollView, revenait alors
    /// à son curseur. Le témoin rejoue l'ancien appel, deselect() seul.
    function lancerScenarioDefilementLecture() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0 }
        var vue = colonneLecture.vueCorps
        var flick = colonneLecture.cadreCorps.contentItem
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | contentY " + Math.round(flick.contentY)
                        + " | sélection « " + vue.selectedText + " »")
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                if (modeleMessages.count < 1) {
                    console.log("scenario: ECHEC defilement-lecture : boîte de réception vide")
                    suivi.echecs += 1
                    suivi.n = 9
                    return
                }
                choisir(0, 0)
            } else if (suivi.n === 9) {
                // Un message long, en texte simple, à la place du corps reçu.
                var lignes = []
                for (var i = 1; i <= 300; ++i)
                    lignes.push("Ligne " + i + " du message d'essai, assez longue pour se replier dans la colonne de lecture.")
                vue.text = lignes.join("\n")
                vue.cursorPosition = 0
            } else if (suivi.n === 10) {
                console.log("scenario: hauteur du contenu " + Math.round(flick.contentHeight) + " | vue " + Math.round(flick.height))
                flick.contentY = 800
                vue.deselect()
                bilan("témoin : deselect() seul, sans sélection, ramène la vue au curseur (attendu : contentY < 800)",
                      flick.contentY < 800)
                flick.contentY = 800
                colonneLecture.deselectionner()
                bilan("sans sélection : la lecture reste à 800", Math.round(flick.contentY) === 800)
            } else if (suivi.n === 11) {
                vue.select(0, 5)
                flick.contentY = 800
                colonneLecture.deselectionner()
                bilan("sélection en haut, lecture plus bas : sélection retirée, la lecture reste à 800",
                      Math.round(flick.contentY) === 800 && vue.selectedText === "")
            } else if (suivi.n === 12) {
                tic.stop()
                console.log("scenario: defilement-lecture " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « recherche-effacee » (0.6.12, demande de Fabienne) : le mot
    /// effacé au clavier (champ vidé, textEdited) met fin à la recherche et
    /// rend la liste de départ — dans le dossier, puis dans toutes les boîtes.
    function lancerScenarioRechercheEffacee() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, total: 0, mot: "" }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | liste " + modeleMessages.count
                        + " | recherche " + JSON.stringify(recherche) + " | globale " + rechercheGlobale
                        + " | noyau sur " + boite.dossierCourant)
        }
        var effacer = function() {
            colonneListe.champRecherche.text = ""
            colonneListe.champRecherche.textEdited()
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 6) {
                suivi.total = modeleMessages.count
                if (suivi.total < 1) {
                    bilan("boîte de réception vide", false)
                    suivi.n = 12
                    return
                }
                var mots = modeleMessages.get(0).sujet.split(/\s+/).filter(function(m) { return m.length > 3 })
                suivi.mot = mots.length > 0 ? mots[0] : modeleMessages.get(0).sujet
                colonneListe.porteeRecherche.currentIndex = 0
                colonneListe.champRecherche.text = suivi.mot
                lancerRecherche()
                bilan("recherche « " + suivi.mot + " » dans le dossier (" + suivi.total + " messages avant)",
                      recherche === suivi.mot)
            } else if (suivi.n === 7) {
                effacer()
            } else if (suivi.n === 8) {
                bilan("mot effacé : recherche finie, liste de départ",
                      recherche === "" && modeleMessages.count === suivi.total)
                colonneListe.porteeRecherche.currentIndex = 2
                colonneListe.champRecherche.text = suivi.mot
                lancerRecherche()
                bilan("recherche « " + suivi.mot + " » dans toutes les boîtes", rechercheGlobale)
            } else if (suivi.n === 10) {
                effacer()
            } else if (suivi.n === 12) {
                bilan("mot effacé : retour au dossier de départ, liste entière",
                      recherche === "" && !rechercheGlobale && boite.dossierCourant === "INBOX"
                      && modeleMessages.count === suivi.total)
            } else if (suivi.n === 13) {
                tic.stop()
                console.log("scenario: recherche-effacee " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « majuscule » (0.6.12, demande de Fabienne) : frappe simulée
    /// dans le corps d'une rédaction, par Redaction.majuscule comme le fait
    /// Keys.onPressed — capitale en début de paragraphe et après « . », « ! »,
    /// « ? » ; rien au fil de la phrase, après « etc. », une initiale, des
    /// points de suspension ou un numéro en tête de ligne ; Ctrl+Z aussitôt
    /// après rend la minuscule ; réglage coupé, rien.
    function lancerScenarioMajuscule() {
        var t = Qt.createQmlObject('import QtQuick; Timer { interval: 4000 }', fenetre)
        var n = null
        t.triggered.connect(function() {
            if (!n) {
                n = ouvrirRedaction(comptesConnus[0].compte)
                n.pret()
                t.start()
                return
            }
            var corps = n.corps
            corps.text = ""
            corps.cursorPosition = 0
            var touche = function(texte, annuler) {
                var ev = { text: texte, modifiers: 0,
                           matches: function(k) { return annuler === true && k === StandardKey.Undo } }
                if (n.majuscule(ev) || annuler)
                    return
                var p = corps.cursorPosition
                n.mefCorps.inserer(p, texte)
                corps.cursorPosition = p + texte.length
            }
            var taper = function(texte) {
                for (var i = 0; i < texte.length; ++i)
                    touche(texte[i])
            }
            taper("bonjour, tout va bien. merci pour le devis, etc. et le reste. à demain! oui? non... peut-être. "
                  + "J. dupont viendra en 2026. puis (fin.) suite. ç")
            touche("", true)
            taper("a\nnouvelle ligne\n1. premier point")
            reglages.majusculeAuto = false
            taper(". non")
            reglages.majusculeAuto = true
            var attendu = "Bonjour, tout va bien. Merci pour le devis, etc. et le reste. À demain! Oui? Non... peut-être. "
                    + "J. dupont viendra en 2026. Puis (fin.) Suite. ça\nNouvelle ligne\n1. premier point. non"
            // getText rend les fins de paragraphe en U+2029.
            var obtenu = corps.getText(0, corps.length).replace(/[\u2028\u2029]/g, "\n")
            console.log("scenario: texte obtenu " + JSON.stringify(obtenu))
            console.log("scenario: majuscule " + (obtenu === attendu ? "OK" : "ECHEC, attendu " + JSON.stringify(attendu)))
            Qt.quit()
        })
        t.start()
    }

    /// Scénario « disposition » (0.6.14) : « une barre » puis « deux étages »
    /// et retour — la bande des outils ne se montre qu'en deux étages, les
    /// boutons sont sur la barre ou sur la bande, jamais les deux ; la barre
    /// garde sa hauteur ; le choix est retenu dans les réglages.
    function lancerScenarioDisposition() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, hauteur: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | barre " + entete.height
                        + " | bande " + bandeOutils.height + " | réglage " + reglagesApparence.disposition)
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            if (suivi.n === 3) {
                suivi.hauteur = entete.height
                bilan("une barre par défaut", disposition === "barre" && !bandeOutils.visible && !deuxEtages)
                choisirDisposition("etages")
            } else if (suivi.n === 4) {
                bilan("deux étages : bande montrée, barre de même hauteur",
                      bandeOutils.visible && bandeOutils.height > 0 && entete.height === suivi.hauteur
                      && reglagesApparence.disposition === "etages")
                choisirDisposition("barre")
            } else if (suivi.n === 5) {
                bilan("retour à une barre", !bandeOutils.visible && entete.height === suivi.hauteur
                      && reglagesApparence.disposition === "barre")
                tic.stop()
                console.log("scenario: disposition " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « inverser-selection » (0.6.14, demande de Manu du 22/10) :
    /// dans la boîte de réception (trois messages au moins), un message
    /// choisi, la sélection inversée, puis rétablie ; tout, puis rien ; le
    /// menu d'un message en dix lignes, « Marquer comme lu » s'il y a un non
    /// lu parmi les choisis.
    function lancerScenarioInverserSelection() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, total: 0, premier: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | choisis " + uidsChoisis().length
                        + " sur " + modeleMessages.count)
        }
        var lignesDuMenu = function() {
            var n = 0
            for (var i = 0; i < menuMessage.count; ++i) {
                var e = menuMessage.itemAt(i)
                if (e && e.visible && e.text !== undefined && e.text.length > 0)
                    n += 1
            }
            return n
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                suivi.total = modeleMessages.count
                if (suivi.total < 3) {
                    bilan("il faut trois messages en boîte de réception", false)
                    suivi.n = 19
                    return
                }
                suivi.premier = modeleMessages.get(0).uid
                choisir(0, 0)
                inverserSelection()
                bilan("un choisi, inversé : les autres", uidsChoisis().length === suivi.total - 1
                      && fenetre.selection[suivi.premier] !== true)
                inverserSelection()
                bilan("inversé de nouveau : le premier seul", uidsChoisis().length === 1
                      && fenetre.selection[suivi.premier] === true)
                toutChoisir()
                inverserSelection()
                bilan("tout, inversé : aucun", uidsChoisis().length === 0)
                toutChoisir()
                boite.marquerLu(uidsChoisis().join(","), true)
                rafraichirListe()
            } else if (suivi.n === 9) {
                choisir(0, 0)
                choisir(1, Qt.ControlModifier)
                boite.marquerLu(String(modeleMessages.get(1).uid), false)
                rafraichirListe()
            } else if (suivi.n === 11) {
                menuMessage.popup()
                // Dix lignes en 0.6.14, onze avec « Catégoriser » (0.6.15).
                bilan("menu du message : " + lignesDuMenu() + " lignes, « " + menuMessage.itemAt(5).text
                      + " », « " + menuMessage.itemAt(6).text + " »",
                      lignesDuMenu() === 11 && menuMessage.unNonLu && menuMessage.itemAt(5).text === qsTr("Marquer comme lu"))
                menuMessage.close()
                choisir(0, 0)
            } else if (suivi.n === 13) {
                menuMessage.popup()
                bilan("un message lu : « " + menuMessage.itemAt(5).text + " »",
                      !menuMessage.unNonLu && menuMessage.itemAt(5).text === qsTr("Marquer comme non lu"))
                menuMessage.close()
                ouvrirBoitesUnifiees()
            } else if (suivi.n === 15) {
                var avant = uidsChoisis().length
                inverserSelection()
                bilan("Toutes les BàL : pas d'inversion (un seul message à la fois)",
                      vueUnifiee && uidsChoisis().length === avant)
                quitterBoitesUnifiees()
            } else if (suivi.n === 17) {
                bilan("interrupteur éteint : retour au dossier", !vueUnifiee && boite.dossierCourant === "INBOX")
            } else if (suivi.n === 18) {
                tic.stop()
                console.log("scenario: inverser-selection " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Menu du clic droit d'un champ de saisie (cf. MenuSaisie) : le
    /// gestionnaire posé dans le champ.
    function menuDuChamp(champ) {
        for (var i = 0; i < champ.data.length; ++i) {
            var o = champ.data[i]
            if (o && o.menu !== undefined && o.agir !== undefined)
                return o
        }
        return null
    }

    /// Entrée d'un menu par son libellé.
    function entreeDuMenu(menu, libelle) {
        for (var i = 0; i < menu.count; ++i) {
            var e = menu.itemAt(i)
            if (e && e.text === libelle)
                return e
        }
        return null
    }

    /// Scénario « fenetres » (demande d'un utilisateur, 06/11) : la place et la
    /// taille de la fenêtre principale gardées puis rendues ; une place hors
    /// des écrans refusée, la taille rendue ; une taille nouvelle notée ;
    /// l'agrandissement noté sans perdre la taille d'avant. Les réglages
    /// d'avant sont remis à la fin.
    function lancerScenarioFenetres() {
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var avant = reglagesFenetres.principale
        fenetre.geometriesGardees = true
        var e = Qt.application.screens[0]
        console.log("scenario: écran " + e.virtualX + "," + e.virtualY + " " + e.width + "x" + e.height
                    + " | visibilité " + fenetre.visibility)
        reglagesFenetres.principale = JSON.stringify({ x: e.virtualX + 40, y: e.virtualY + 30, l: 900, h: 650, max: false })
        fenetre.poserGeometrie(fenetre, "principale")
        bilan("taille rendue : " + fenetre.width + "x" + fenetre.height, fenetre.width === 900 && fenetre.height === 650)
        bilan("place rendue : " + fenetre.x + "," + fenetre.y, fenetre.x === e.virtualX + 40 && fenetre.y === e.virtualY + 30)
        reglagesFenetres.principale = JSON.stringify({ x: -20000, y: -20000, l: 800, h: 600, max: false })
        fenetre.poserGeometrie(fenetre, "principale")
        bilan("place hors des écrans refusée, taille rendue : " + fenetre.x + "," + fenetre.y + " " + fenetre.width + "x" + fenetre.height,
              fenetre.x > -20000 && fenetre.y > -20000 && fenetre.width === 800 && fenetre.height === 600)
        fenetre.width = 1010
        fenetre.height = 700
        fenetre.noterGeometrie(fenetre, "principale")
        var g = fenetre.geometrieConnue("principale")
        bilan("taille nouvelle notée : " + JSON.stringify(g), g.l === 1010 && g.h === 700 && g.max === false)
        fenetre.visibility = Window.Maximized
        if (fenetre.visibility === Window.Maximized) {
            fenetre.noterGeometrie(fenetre, "principale")
            g = fenetre.geometrieConnue("principale")
            bilan("agrandissement noté, taille d'avant gardée : " + JSON.stringify(g), g.max === true && g.l === 1010)
            fenetre.visibility = Window.Windowed
        } else {
            console.log("scenario: agrandissement sans effet sur cette plateforme (" + fenetre.visibility + ")")
        }
        fenetre.geometriesGardees = false
        reglagesFenetres.principale = avant
        console.log("scenario: fenetres " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
        Qt.quit()
    }

    /// Scénario « menu-champ » (0.6.14, demande de Manu du 22/10) : le menu
    /// du clic droit du champ de recherche — Coller ce qu'un autre champ a
    /// copié, puis Tout sélectionner, Couper, Annuler ; Couper et Copier
    /// grisés sans sélection.
    function lancerScenarioMenuChamp() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0 }
        var mot = "IIC OCEAN - ILOT RF1.1"
        var champ = colonneListe.champRecherche
        var h = menuDuChamp(champ)
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | champ " + JSON.stringify(champ.text)
                        + " | focus " + champ.activeFocus)
        }
        // Le clic droit, comme onTapped.
        var clicDroit = function() {
            h.debut = champ.selectionStart
            h.fin = champ.selectionEnd
            h.agi = false
            h.menu.popup()
        }
        var choisirEntree = function(libelle) {
            entreeDuMenu(h.menu, libelle).triggered()
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            if (suivi.n === 3) {
                if (!h) {
                    bilan("pas de menu sur le champ de recherche", false)
                    suivi.n = 9
                    return
                }
                // Le presse-papier rempli par un autre champ.
                var source = Qt.createQmlObject('import QtQuick; TextInput {}', fenetre.contentItem)
                source.text = mot
                source.selectAll()
                source.copy()
                source.destroy()
                champ.text = ""
                clicDroit()
                bilan("menu ouvert, champ vide : Coller seul permis (Couper " + entreeDuMenu(h.menu, qsTr("Couper")).enabled
                      + ", Copier " + entreeDuMenu(h.menu, qsTr("Copier")).enabled + ", Coller "
                      + entreeDuMenu(h.menu, qsTr("Coller")).enabled + ")",
                      h.menu.opened && !entreeDuMenu(h.menu, qsTr("Couper")).enabled
                      && !entreeDuMenu(h.menu, qsTr("Copier")).enabled && entreeDuMenu(h.menu, qsTr("Coller")).enabled)
                choisirEntree(qsTr("Coller"))
            } else if (suivi.n === 4) {
                bilan("Coller : le texte copié dans la recherche, menu fermé, focus au champ",
                      champ.text === mot && !h.menu.visible && champ.activeFocus)
                clicDroit()
                choisirEntree(qsTr("Tout sélectionner"))
            } else if (suivi.n === 5) {
                bilan("Tout sélectionner", champ.selectedText === mot)
                clicDroit()
                bilan("sélection : Couper et Copier permis", entreeDuMenu(h.menu, qsTr("Couper")).enabled
                      && entreeDuMenu(h.menu, qsTr("Copier")).enabled)
                choisirEntree(qsTr("Couper"))
            } else if (suivi.n === 6) {
                bilan("Couper : champ vide", champ.text === "")
                clicDroit()
                choisirEntree(qsTr("Annuler"))
            } else if (suivi.n === 7) {
                bilan("Annuler : le texte revient", champ.text === mot)
                champ.text = ""
                champ.textEdited()
            } else if (suivi.n === 8) {
                tic.stop()
                console.log("scenario: menu-champ " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « m365-reel » (0.9.0) : essai sur une vraie boîte Microsoft
    /// 365, sans écran (scratchpad m365-reel/essai.sh). MMAIL_UTILISATEUR
    /// porte l'adresse ; le profil et le coffre sont gardés d'un passage à
    /// l'autre. MMAIL_DOSSIER choisit le passage :
    ///   - « ecrire » : connexion si le profil n'a pas le compte, catégorie
    ///     « Essai MMail » créée et posée sur le premier message, liste reliée
    ///     à Outlook, puis recolorée et renommée « Essai MMail renommée » ;
    ///     puis un filtre « aussi dans Outlook » (règle « MMail : Essai
    ///     MMail ») ; enfin la lecture des agendas autorisée, et les agendas
    ///     de la boîte lus ;
    ///   - « lire » : reconnexion par le coffre, les catégories posées dans
    ///     Outlook entre-temps relues, puis les catégories d'essai retirées
    ///     des messages et de la liste de MMail (Outlook garde la sienne), le
    ///     filtre d'essai retiré, sa règle avec ; enfin les agendas relus :
    ///     l'événement et la série posés dans Outlook entre-temps (demain et
    ///     après-demain) paraissent à leur heure ; puis MMail écrit : un
    ///     événement créé dans trois jours, celui de demain supprimé.
    /// Les adresses d'autorisation sont écrites au journal (« scenario: url »)
    /// pour le navigateur qui mène la connexion.
    function lancerScenarioM365Reel() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var adresse = (identifiantsEssai.utilisateur || "").toLowerCase()
        var passage = identifiantsEssai.dossier || "ecrire"
        var suivi = { n: 0, etape: 0, depuis: 0, echecs: 0, motCle: "", premier: 0, sujet: "", demande: false, etat: "",
                      synchros: 0, agendaDepuis: 0, erreursAgenda: "", ecrits: [], cible: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        // Le compte une fois sa session prête (« pret ») : l'arborescence le
        // montre dès le lancement, depuis l'index local.
        var compte = function() {
            for (var k = 0; k < modeleArborescence.count; ++k) {
                var l = modeleArborescence.get(k)
                if (l.genre === "compte" && (l.adresse || "").toLowerCase() === adresse && l.etat === "pret")
                    return { compte: l.compte, adresse: l.adresse }
            }
            return null
        }
        var catalogue = function(c) {
            return JSON.parse(boite.categories(c.compte, "INBOX"))
        }
        var noms = function(liste) {
            return liste.map(function(x) { return x.nom + " " + x.couleur }).join(", ")
        }
        var essais = function(liste) {
            return liste.filter(function(x) { return x.nom.indexOf("Essai ") === 0 })
        }
        var suivant = function() {
            suivi.etape += 1
            suivi.depuis = suivi.n
        }
        var finir = function() {
            tic.stop()
            console.log("scenario: m365-reel " + passage + " " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
            Qt.quit()
        }
        var autoriser = function(url, quoi) {
            bilan(quoi + " : adresse d'autorisation", url.length > 0)
            if (url.length === 0) {
                console.log("scenario: erreur " + boite.erreur)
                return false
            }
            console.log("scenario: url " + url)
            Qt.openUrlExternally(url)
            return true
        }
        // Agendas (0.9.0) : chaque synchronisation terminée.
        boite.agendasSynchronises.connect(function(change, erreurs) {
            suivi.synchros += 1
            suivi.erreursAgenda = erreurs
            console.log("scenario: agendas synchronisés | changé " + change + " | erreurs « " + erreurs + " »")
        })
        // Écritures dans l'agenda terminées : (réussite, motif).
        boite.evenementEnregistre.connect(function(ok, message) {
            suivi.ecrits.push([ok, message])
            console.log("scenario: écriture dans l'agenda | " + ok + " | « " + message + " »")
        })
        var jourDans = function(decalage) {
            var d = new Date()
            d.setDate(d.getDate() + decalage)
            return Qt.formatDate(d, "yyyy-MM-dd")
        }
        // Les fiches d'un jour décalé de `decalage` jours.
        var fichesDu = function(decalage) {
            var d = new Date()
            d.setDate(d.getDate() + decalage)
            return JSON.parse(boite.vueAgenda("jour", Qt.formatDate(d, "yyyy-MM-dd"))).fiches || []
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var ecoule = suivi.n - suivi.depuis
            if (messageEtat.texte !== suivi.etat) {
                suivi.etat = messageEtat.texte
                console.log("scenario: barre d'état « " + suivi.etat + " »")
            }
            var c = compte()
            if (suivi.etape === 0) {
                // Le compte : connu du profil (le coffre répond), sinon connexion.
                if (c) {
                    bilan("compte ouvert après " + suivi.n + " s", true)
                    ouvrirDossier(c.compte, "INBOX")
                    suivant()
                } else if (suivi.n === 8 && !suivi.demande) {
                    suivi.demande = true
                    var connus = listeComptes().filter(function(x) { return (x.adresse || "").toLowerCase() === adresse })
                    if (connus.length > 0) {
                        bilan("compte du profil sans session après 8 s", passage === "ecrire")
                    }
                    if (passage !== "ecrire") {
                        finir()
                        return
                    }
                    fenetre.marquerGeste(adresse)
                    if (!autoriser(boite.connecterMicrosoft(adresse), "connexion"))
                        finir()
                } else if (suivi.n > 420) {
                    bilan("compte jamais ouvert", false)
                    finir()
                }
            } else if (suivi.etape === 1) {
                // La boîte de réception, puis le temps de la relève par Graph.
                if (modeleMessages.count > 0 && ecoule >= 25) {
                    var cat = catalogue(c)
                    bilan("catalogue Microsoft 365 : " + noms(cat.liste) + " | microsoft " + cat.microsoft
                          + " | refusées " + cat.refusees, cat.microsoft === true && !cat.refusees)
                    for (var i = 0; i < Math.min(8, modeleMessages.count); ++i) {
                        var m = modeleMessages.get(i)
                        console.log("scenario: message " + i + " | " + m.uid + " | " + m.date + " | " + m.sujet + " | « " + m.motsCles + " »")
                    }
                    suivant()
                } else if (ecoule > 120) {
                    bilan("boîte de réception vide ou jamais chargée (" + modeleMessages.count + ")", false)
                    finir()
                }
            } else if (passage === "ecrire") {
                if (suivi.etape === 2) {
                    choisir(0, 0)
                    suivi.premier = modeleMessages.get(0).uid
                    suivi.sujet = modeleMessages.get(0).sujet
                    suivi.motCle = boite.poserCategorie(c.compte, "INBOX", "", "Essai MMail", "#0B8043")
                    bilan("créée : " + suivi.motCle, suivi.motCle.length > 0)
                    basculerCategorie(suivi.motCle, true)
                    console.log("scenario: posée sur « " + suivi.sujet + " » (" + suivi.premier + ")")
                    suivant()
                } else if (suivi.etape === 3 && ecoule >= 20) {
                    // Relier la liste à Outlook : l'autorisation à part.
                    if (!autoriser(boite.autoriserCouleursOutlook(c.compte), "liste d'Outlook"))
                        finir()
                    suivant()
                } else if (suivi.etape === 4) {
                    if (suivi.etat.indexOf(qsTr("Autorisation reçue")) === 0) {
                        bilan("autorisation des réglages reçue", true)
                        suivant()
                    } else if (suivi.etat.indexOf(qsTr("Catégories d'Outlook :")) === 0 || ecoule > 420) {
                        bilan("autorisation des réglages : « " + suivi.etat + " »", false)
                        finir()
                    }
                } else if (suivi.etape === 5 && ecoule >= 20) {
                    console.log("scenario: catalogue relié " + noms(catalogue(c).liste))
                    bilan("recolorée en rouge",
                          boite.poserCategorie(c.compte, "INBOX", suivi.motCle, "Essai MMail", "#D50000") === suivi.motCle)
                    suivant()
                } else if (suivi.etape === 6 && ecoule >= 20) {
                    bilan("renommée", boite.poserCategorie(c.compte, "INBOX", suivi.motCle, "Essai MMail renommée", "#D50000")
                          === suivi.motCle)
                    suivant()
                } else if (suivi.etape === 7 && ecoule >= 30) {
                    var i2 = indexDe(suivi.premier)
                    console.log("scenario: catalogue final " + noms(catalogue(c).liste))
                    bilan("le message garde la catégorie : « " + (i2 >= 0 ? modeleMessages.get(i2).motsCles : "?") + " »",
                          i2 >= 0 && (" " + modeleMessages.get(i2).motsCles + " ").indexOf(" " + suivi.motCle + " ") >= 0)
                    // Un filtre « aussi dans Outlook » sans effet sur le
                    // courrier : il marque lu un objet qu'aucun message n'a.
                    var filtre = { nom: "Essai MMail", actif: true, compte: c.adresse, tout: true,
                                   conditions: [{ champ: "objet", operateur: "contient", valeur: "Essai de règle MMail 0.9.0" }],
                                   lu: true, suivi: false, destination: "", dossier: "", compte_destination: "",
                                   arreter: false, serveur: true }
                    console.log("scenario: raison Outlook « " + boite.regleOutlook(JSON.stringify(filtre)) + " »")
                    bilan("filtre « aussi dans Outlook » enregistré", boite.poserFiltres(JSON.stringify([filtre])))
                    suivant()
                } else if (suivi.etape === 8) {
                    if (suivi.etat.indexOf(qsTr("Filtres :")) === 0) {
                        bilan("règle envoyée à Outlook", suivi.etat.indexOf("1 filtre(s) dans les règles d'Outlook") >= 0)
                        suivant()
                    } else if (ecoule > 60) {
                        bilan("pas de nouvelle des règles d'Outlook", false)
                        finir()
                    }
                } else if (suivi.etape === 9) {
                    // Les agendas : leur lecture autorisée à part.
                    suivi.agendaDepuis = suivi.synchros
                    if (!autoriser(boite.autoriserAgendasMicrosoft(c.compte), "agendas"))
                        finir()
                    suivant()
                } else if (suivi.etape === 10) {
                    if (suivi.etat.indexOf(qsTr("Autorisation reçue : MMail lit les agendas")) === 0) {
                        bilan("autorisation des agendas reçue", true)
                        suivant()
                    } else if (suivi.etat.indexOf(qsTr("Agendas Microsoft 365 :")) === 0 || ecoule > 420) {
                        bilan("autorisation des agendas : « " + suivi.etat + " »", false)
                        finir()
                    }
                } else if (suivi.etape === 11) {
                    if (suivi.synchros > suivi.agendaDepuis) {
                        var miens = JSON.parse(boite.agendas()).filter(function(a) { return a.microsoft && a.compte === c.compte })
                        console.log("scenario: agendas Microsoft 365 : " + miens.map(function(a) {
                            return a.nom + " " + a.couleur + (a.ecriture ? " écriture" : " lecture")
                        }).join(", "))
                        bilan(miens.length + " agenda(s) Microsoft 365 lu(s), sans erreur", miens.length > 0 && suivi.erreursAgenda.length === 0)
                        bilan("un agenda Microsoft 365 au moins accepte l'écriture", miens.some(function(a) { return a.ecriture }))
                        var etats = JSON.parse(boite.agendasMicrosoft())
                        bilan("lecture des agendas notée autorisée", etats.some(function(x) { return x.compte === c.compte && x.etat === "autorise" }))
                        finir()
                    } else if (ecoule > 120) {
                        bilan("pas de synchronisation des agendas", false)
                        finir()
                    }
                }
            } else {
                if (suivi.etape === 2) {
                    var liste = essais(catalogue(c).liste)
                    console.log("scenario: catégories d'essai relues " + noms(liste))
                    bilan("« Essai Outlook » relue d'Outlook", liste.some(function(x) { return x.nom === "Essai Outlook" }))
                    var porteurs = 0
                    for (var j = 0; j < modeleMessages.count; ++j) {
                        var mots = " " + modeleMessages.get(j).motsCles + " "
                        var a = liste.filter(function(x) { return mots.indexOf(" " + x.motCle + " ") >= 0 })
                        if (a.length === 0)
                            continue
                        porteurs += 1
                        console.log("scenario: porteur « " + modeleMessages.get(j).sujet + " » : " + mots.trim())
                        choisir(j, 0)
                        a.forEach(function(x) { basculerCategorie(x.motCle, false) })
                    }
                    bilan(porteurs + " message(s) porteurs, catégories retirées", porteurs >= 2)
                    suivant()
                } else if (suivi.etape === 3 && ecoule >= 20) {
                    essais(catalogue(c).liste).forEach(function(x) {
                        bilan("« " + x.nom + " » retirée de la liste de MMail", boite.retirerCategorie(c.compte, "INBOX", x.motCle))
                    })
                    suivant()
                } else if (suivi.etape === 4 && ecoule >= 15) {
                    console.log("scenario: catalogue final " + noms(catalogue(c).liste))
                    // Le filtre d'essai retiré : sa règle part d'Outlook.
                    bilan("filtres vidés", boite.poserFiltres("[]"))
                    suivant()
                } else if (suivi.etape === 5) {
                    if (suivi.etat.indexOf(qsTr("Filtres :")) === 0) {
                        bilan("règle retirée d'Outlook", suivi.etat.indexOf("règles de MMail retirées d'Outlook") >= 0)
                        suivant()
                    } else if (ecoule > 60) {
                        bilan("pas de nouvelle des règles d'Outlook", false)
                        finir()
                    }
                } else if (suivi.etape === 6) {
                    // Les agendas relus : l'autorisation du passage « ecrire »
                    // tient, sans nouvelle page de Microsoft.
                    suivi.agendaDepuis = suivi.synchros
                    boite.synchroniserAgendas(true)
                    suivant()
                } else if (suivi.etape === 8) {
                    if (suivi.ecrits.length > 0) {
                        var e = suivi.ecrits.shift()
                        bilan("événement créé par MMail dans l'agenda Microsoft 365", e[0])
                        var trois = fichesDu(3).filter(function(f) { return f.titre === "Essai d'écriture MMail 0.9.0" })
                        bilan("il paraît dans trois jours, de 15:00 à 15:45, modifiable",
                              trois.length === 1 && trois[0].quand.indexOf("15:00") >= 0 && trois[0].quand.indexOf("15:45") >= 0
                              && trois[0].modifiable)
                        var demain = fichesDu(1).filter(function(f) { return f.titre === "Essai d'agenda MMail 0.9.0" })
                        if (demain.length !== 1) {
                            bilan("l'événement de demain à supprimer", false)
                            finir()
                            return
                        }
                        boite.supprimerEvenement(demain[0].objet, demain[0].occurrence, false)
                        suivant()
                    } else if (ecoule > 90) {
                        bilan("pas de nouvelle de l'écriture", false)
                        finir()
                    }
                } else if (suivi.etape === 9) {
                    if (suivi.ecrits.length > 0) {
                        var s = suivi.ecrits.shift()
                        bilan("événement de demain supprimé par MMail", s[0])
                        bilan("il ne paraît plus", fichesDu(1).every(function(f) { return f.titre !== "Essai d'agenda MMail 0.9.0" }))
                        finir()
                    } else if (ecoule > 90) {
                        bilan("pas de nouvelle de la suppression", false)
                        finir()
                    }
                } else if (suivi.etape === 7) {
                    if (suivi.synchros > suivi.agendaDepuis) {
                        var f1 = fichesDu(1)
                        var f2 = fichesDu(2)
                        f1.concat(f2).forEach(function(f) {
                            console.log("scenario: fiche « " + f.titre + " » | " + f.quand + " | " + f.nomAgenda + " | répète "
                                        + f.repete + " | " + (f.modifiable ? "modifiable" : f.motif))
                        })
                        var simple = f1.filter(function(f) { return f.titre === "Essai d'agenda MMail 0.9.0" })
                        var un = simple.length === 1 ? simple[0] : null
                        bilan("événement d'Outlook lu demain, de 10:00 à 11:00",
                              un !== null && un.quand.indexOf("10:00") >= 0 && un.quand.indexOf("11:00") >= 0)
                        bilan("son lieu et sa description lus",
                              un !== null && un.lieu === "Salle d'essai" && un.description.indexOf("Ligne 2") >= 0)
                        bilan("modifiable dans MMail", un !== null && un.modifiable)
                        var serie = f1.concat(f2).filter(function(f) { return f.titre === "Essai de série MMail 0.9.0" })
                        bilan("série d'Outlook : une occurrence demain, une après-demain, marquées « répète »",
                              serie.length === 2 && serie.every(function(f) { return f.repete }))
                        // L'écriture : un événement nouveau dans trois jours,
                        // dans le premier agenda de la boîte qui l'accepte.
                        var cibles = JSON.parse(boite.agendas()).filter(function(a) {
                            return a.microsoft && a.compte === c.compte && a.ecriture
                        })
                        bilan("un agenda Microsoft 365 accepte l'écriture", cibles.length > 0)
                        if (cibles.length === 0) {
                            finir()
                            return
                        }
                        suivi.cible = cibles[0].id
                        boite.enregistrerEvenement(JSON.stringify({
                            agenda: suivi.cible, objet: 0, occurrence: "", titre: "Essai d'écriture MMail 0.9.0", lieu: "",
                            description: "Posé par MMail", journee: false, debut: jourDans(3) + "T15:00", fin: jourDans(3) + "T15:45",
                            repetition: "", jusquAu: "", rappel: 5, participants: null, organisateur: "", nomOrganisateur: ""
                        }))
                        suivant()
                    } else if (ecoule > 120) {
                        bilan("pas de synchronisation des agendas", false)
                        finir()
                    }
                }
            }
        })
        tic.start()
    }

    /// Scénario « categories » (0.6.15, demande de Manu du 22/10) : dans la
    /// boîte de réception (deux messages au moins), une catégorie d'origine
    /// et une catégorie créée posées ; l'état du sous-menu « Catégoriser » ;
    /// la recherche « catégorie: » dans l'index et sur le serveur (KEYWORD :
    /// la preuve que le serveur les garde) ; tout retiré, et toujours retiré
    /// après relecture du serveur ; la catégorie créée retirée du catalogue,
    /// une catégorie d'origine renommée puis rétablie. Avec PARTAGE=1, une
    /// boîte partagée a son propre catalogue.
    function lancerScenarioCategories() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, motCle: "", premier: 0, second: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var mots = function(uid) {
            var i = indexDe(uid)
            return i >= 0 ? (modeleMessages.get(i).motsCles || "") : "?"
        }
        var catalogue = function(compte, chemin) {
            return JSON.parse(boite.categories(compte, chemin))
        }
        var noms = function(liste) {
            return liste.map(function(x) { return x.nom }).join(", ")
        }
        var recherche = qsTr("catégorie") + ":\"Devis à faire\""
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus.filter(function(x) { return x.adresse === identifiantsEssai.utilisateur })[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                if (modeleMessages.count < 2) {
                    bilan("il faut deux messages en boîte de réception", false)
                    suivi.n = 29
                    return
                }
                suivi.premier = modeleMessages.get(0).uid
                suivi.second = modeleMessages.get(1).uid
                var depart = catalogue(0, "")
                bilan("catalogue de départ : " + noms(depart.liste) + " | boîte " + depart.boite
                      + " | serveur " + depart.serveur,
                      depart.liste.length === 5 && depart.liste[0].nom === "Important"
                      && depart.liste[4].nom === "Peut attendre" && depart.boite === "INBOX" && !depart.refusees)
                choisir(0, 0)
                basculerCategorie("$label1", true)
                bilan("Important posée : « " + mots(suivi.premier) + " » | en-tête " + noms(categoriesAffichees),
                      mots(suivi.premier) === "$label1" && noms(categoriesAffichees) === "Important")
                suivi.motCle = boite.poserCategorie(0, "", "", "Devis à faire", "#0B8043")
                bilan("créée : " + suivi.motCle + " | " + noms(catalogue(0, "").liste),
                      suivi.motCle === "devis_a_faire" && catalogue(0, "").liste.length === 6)
                choisir(1, Qt.ControlModifier)
                basculerCategorie(suivi.motCle, true)
                bilan("posée sur les deux : « " + mots(suivi.premier) + " » / « " + mots(suivi.second) + " »",
                      mots(suivi.premier) === "$label1 devis_a_faire" && mots(suivi.second) === "devis_a_faire")
                var etat = etatCategories()
                bilan("sous-menu : la créée cochée, Important en partie (" + JSON.stringify(etat.porte) + ")",
                      etat.porte["devis_a_faire"] === "tous" && etat.porte["$label1"] === "certains"
                      && etat.liste.length === 6 && !etat.refusees)
                menuMessage.popup()
                bilan("menu du message : « Catégoriser » présent", entreeDuMenu(menuMessage, qsTr("Catégoriser")) !== null)
                menuMessage.close()
            } else if (suivi.n === 10) {
                lancerRecherche(recherche)
                bilan("recherche dans l'index : " + modeleMessages.count + " résultat(s)", modeleMessages.count === 2)
            } else if (suivi.n === 14) {
                var trouves = uidsServeur.slice().sort().join(",")
                bilan("recherche sur le serveur (KEYWORD) : " + JSON.stringify(uidsServeur),
                      trouves === [suivi.premier, suivi.second].sort().join(","))
                quitterRecherche()
            } else if (suivi.n === 16) {
                choisir(Math.max(0, indexDe(suivi.premier)), 0)
                effacerCategories()
                bilan("effacées du premier : « " + mots(suivi.premier) + " »", mots(suivi.premier) === "")
                choisir(Math.max(0, indexDe(suivi.second)), 0)
                basculerCategorie(suivi.motCle, false)
                bilan("retirée du second : « " + mots(suivi.second) + " »", mots(suivi.second) === "")
                bilan("retirée du catalogue", boite.retirerCategorie(0, "", suivi.motCle)
                      && catalogue(0, "").liste.length === 5)
                bilan("Important renommée « Urgent »", boite.poserCategorie(0, "", "$label1", "Urgent", "#D50000") === "$label1"
                      && catalogue(0, "").liste[0].nom === "Urgent")
                bilan("puis rétablie", boite.retirerCategorie(0, "", "$label1") && catalogue(0, "").liste[0].nom === "Important")
                bilan("refus : nom vide", boite.poserCategorie(0, "", "", "  ", "#0B8043") === "")
                if (c.partagees && c.partagees.length > 0) {
                    var chemin = "Shared/" + c.partagees[0]
                    var avant = catalogue(c.compte, chemin)
                    var cree = boite.poserCategorie(c.compte, chemin, "", "Boîte partagée", "#C239B3")
                    var apres = catalogue(c.compte, chemin)
                    bilan("boîte partagée : catalogue de " + avant.boite + " (" + noms(apres.liste) + "), le sien intact ("
                          + catalogue(0, "").liste.length + ")",
                          avant.boite.indexOf("Shared/") === 0 && cree === "boite_partagee" && apres.liste.length === 6
                          && catalogue(0, "").liste.length === 5)
                    boite.retirerCategorie(c.compte, chemin, cree)
                }
            } else if (suivi.n === 18) {
                // Les drapeaux relus du serveur : rien n'y est resté.
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 21) {
                bilan("après relecture du serveur : « " + mots(suivi.premier) + " » / « " + mots(suivi.second) + " »",
                      mots(suivi.premier) === "" && mots(suivi.second) === "")
                var fin = catalogue(0, "")
                console.log("scenario: catalogue final " + noms(fin.liste) + " | serveur " + fin.serveur)
            } else if (suivi.n >= 22) {
                tic.stop()
                console.log("scenario: categories " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénarios « liste-categories », « menu-categories » et
    /// « gerer-categories » (0.6.15) : des catégories posées sur les
    /// messages de la boîte de réception, puis la liste, le sous-menu
    /// « Catégoriser » ouvert ou le dialogue « Gérer les catégories », pour
    /// une capture d'écran (--capture), sans quitter.
    function lancerScenarioCapturesCategories() {
        var t = Qt.createQmlObject('import QtQuick; Timer { interval: 4000 }', fenetre)
        t.triggered.connect(function() {
            ouvrirDossier(comptesConnus[0].compte, "INBOX")
            var t2 = Qt.createQmlObject('import QtQuick; Timer { interval: 2000 }', fenetre)
            t2.triggered.connect(function() {
                var devis = boite.poserCategorie(0, "", "", "Devis à faire", "#0B8043")
                var compta = boite.poserCategorie(0, "", "", "Comptabilité", "#0078D4")
                var poser = function(rang, motCle) {
                    if (rang >= modeleMessages.count)
                        return
                    choisir(rang, 0)
                    basculerCategorie(motCle, true)
                }
                poser(0, "$label1")
                poser(0, devis)
                poser(1, devis)
                poser(2, "$label2")
                poser(3, compta)
                choisir(0, 0)
                if (essai.scenario === "menu-categories") {
                    menuMessage.popup(colonneListe.x + 120, entete.height + bandeOutils.height + 90)
                    for (var i = 0; i < menuMessage.count; ++i) {
                        var e = menuMessage.itemAt(i)
                        if (e && e.text === qsTr("Catégoriser"))
                            e.triggered()
                    }
                } else if (essai.scenario === "gerer-categories") {
                    gererCategories()
                }
            })
            t2.start()
        })
        t.start()
    }

    /// Scénarios « menu-affichage », « menu-message » et « menu-saisie »
    /// (0.6.14) : le menu ouvert pour une capture d'écran (--capture), sans
    /// quitter.
    function lancerScenarioMenuOuvert() {
        var t = Qt.createQmlObject('import QtQuick; Timer { interval: 5000 }', fenetre)
        t.triggered.connect(function() {
            if (essai.scenario === "menu-affichage") {
                menuAffichage.popup(Math.max(0, fenetre.width - 330), entete.height)
                menuAffichage.itemAt(1).triggered()
                return
            }
            // 0.6.20 : « Comptes » et « Réglages », ouverts pour une capture ;
            // leurs entrées sur la console.
            if (essai.scenario === "menu-comptes" || essai.scenario === "menu-reglages") {
                var m = essai.scenario === "menu-comptes" ? menuComptes : menuReglages
                m.popup(Math.max(0, fenetre.width - 520), entete.height)
                var entrees = []
                for (var i = 0; i < m.count; ++i) {
                    var e = m.itemAt(i)
                    if (e && e.height > 0 && (e.text || (e.subMenu && e.subMenu.title)))
                        entrees.push((e.text || e.subMenu.title) + (e.enabled ? "" : " (grisé)"))
                }
                console.log("scenario: " + m.title + " : " + entrees.join(" | "))
                return
            }
            if (essai.scenario === "menu-saisie") {
                var champ = colonneListe.champRecherche
                var h = menuDuChamp(champ)
                champ.text = "IIC OCEAN"
                champ.forceActiveFocus()
                champ.select(0, 3)
                h.debut = 0
                h.fin = 3
                h.menu.popup(champ, 40, champ.height - 4)
                return
            }
            ouvrirDossier(comptesConnus[0].compte, "INBOX")
            var t2 = Qt.createQmlObject('import QtQuick; Timer { interval: 2000 }', fenetre)
            t2.triggered.connect(function() {
                choisir(0, 0)
                menuMessage.popup(colonneListe.x + 120, entete.height + bandeOutils.height + 90)
            })
            t2.start()
        })
        t.start()
    }

    /// Scénario « lu-differe » (0.6.9, demande de Gwenaëlle du 20/10) : le
    /// marquage « lu » attend le délai réglé, ne vise pas un message quitté
    /// avant, et n'a jamais lieu sur « Jamais ». La boîte de réception doit
    /// compter deux messages.
    function lancerScenarioLuDiffere() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, a: 0, b: 0, echecs: 0 }
        var lu = function(uid) {
            var i = indexDe(uid)
            var l = JSON.parse(boite.messagesParUid(String(uid)))
            return "liste " + (i >= 0 ? modeleMessages.get(i).lu : null) + " | index " + (l.length > 0 ? l[0].lu : null)
        }
        var bilan = function(texte, uid, attendu) {
            var etat = lu(uid)
            var bon = etat === "liste " + attendu + " | index " + attendu
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte + " | " + etat)
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                if (modeleMessages.count < 2) {
                    console.log("scenario: ECHEC lu-differe : " + modeleMessages.count + " message(s) en boîte de réception, il en faut deux")
                    suivi.echecs += 1
                    suivi.n = 39
                    return
                }
                suivi.a = modeleMessages.get(0).uid
                suivi.b = modeleMessages.get(1).uid
                boite.marquerLu(suivi.a + "," + suivi.b, false)
                rafraichirListe()
                reglages.delaiLu = 3
            } else if (suivi.n === 9) {
                choisir(indexDe(suivi.a), 0)
            } else if (suivi.n === 11) {
                bilan("A : 3 s, encore non lu après 2 s", suivi.a, false)
            } else if (suivi.n === 14) {
                bilan("A : 3 s, lu après 5 s", suivi.a, true)
            } else if (suivi.n === 15) {
                choisir(indexDe(suivi.b), 0)
            } else if (suivi.n === 16) {
                // B quitté avant la fin du délai.
                choisir(indexDe(suivi.a), 0)
            } else if (suivi.n === 21) {
                bilan("B : quitté au bout d'une seconde, toujours non lu", suivi.b, false)
                reglages.delaiLu = -1
            } else if (suivi.n === 22) {
                choisir(indexDe(suivi.b), 0)
            } else if (suivi.n === 28) {
                bilan("B : « Jamais », non lu après 6 s", suivi.b, false)
                reglages.delaiLu = 0
                boite.marquerLu(String(suivi.a), false)
                rafraichirListe()
            } else if (suivi.n === 29) {
                choisir(indexDe(suivi.a), 0)
            } else if (suivi.n === 31) {
                bilan("A : « À l'affichage », lu aussitôt", suivi.a, true)
                // Le serveur : un autre dossier, puis la boîte de réception
                // relue — ses drapeaux reviennent du serveur.
                fenetre.selection = ({})
                uidCourant = 0
                ouvrirDossier(c.compte, "Essais")
            } else if (suivi.n === 33) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 35) {
                boite.actualiser()
            } else if (suivi.n === 40) {
                bilan("serveur : A lu", suivi.a, true)
                bilan("serveur : B non lu", suivi.b, false)
                console.log("scenario: lu-differe, " + (suivi.echecs === 0 ? "tout OK" : suivi.echecs + " échec(s)"))
                suivi.n = 44
            } else if (suivi.n >= 45) {
                tic.stop()
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « adresses-envoi » (0.6.7) : les autres adresses d'envoi d'un
    /// compte dans le champ « De », celle d'une réponse ou d'un brouillon
    /// reprise, l'identité du compte reprise faute de la sienne. Sans réseau
    /// après la connexion.
    function etapeScenarioAdressesEnvoi() {
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var c = comptesConnus[0]
        var lues = lireAdressesEnvoi("Partagee@Exemple.fr ; alias@exemple.fr, pas-une-adresse ; partagee@exemple.fr "
                                     + c.adresse, c.adresse)
        bilan("saisie lue : " + JSON.stringify(lues),
              JSON.stringify(lues.adresses) === JSON.stringify(["Partagee@Exemple.fr", "alias@exemple.fr"])
              && JSON.stringify(lues.invalides) === JSON.stringify(["pas-une-adresse"]))
        poserAdressesEnvoi(c.adresse, lues.adresses)
        poserIdentite(c.adresse, { nom: "Personne Essai", signature: "Signature du compte", html: "",
                                   nouveaux: true, reponses: true })
        var de = expediteursConnus.filter(function(e) { return e.compte === c.compte })
                                  .map(function(e) { return e.adresse })
        bilan("champ « De » : " + de.join(" | "),
              JSON.stringify(de) === JSON.stringify([c.adresse, "Partagee@Exemple.fr", "alias@exemple.fr"]))
        var r = ouvrirRedaction(c.compte)
        r.pret()
        var v = r.contenu()
        bilan("nouveau message : " + v.de + " | " + v.nom, v.de === c.adresse && v.nom === "Personne Essai")
        // Réponse à un message adressé à la boîte partagée (casse différente).
        r.remplir({ mode: "repondre", de: "partagee@exemple.fr", a: "client@exemple.org", objet: "RE : essai" })
        v = r.contenu()
        bilan("réponse depuis la partagée : " + v.de + " | " + v.nom + " | signature du compte "
              + (r.htmlActuel().indexOf("Signature du compte") >= 0 || v.texte.indexOf("Signature du compte") >= 0),
              v.de === "Partagee@Exemple.fr" && v.nom === "Personne Essai")
        // Une adresse qui n'est pas du compte : le champ ne bouge pas.
        r.remplir({ mode: "brouillon", de: "inconnue@exemple.fr", a: "client@exemple.org" })
        bilan("adresse étrangère ignorée : " + r.contenu().de, r.contenu().de === "Partagee@Exemple.fr")
        r.conteneur.fermerRedaction()
        // Son identité propre l'emporte sur celle du compte.
        poserIdentite("alias@exemple.fr", { nom: "Accueil", signature: "", html: "", nouveaux: true, reponses: true })
        var r2 = ouvrirRedaction(c.compte)
        r2.remplir({ mode: "brouillon", de: "alias@exemple.fr", a: "client@exemple.org" })
        bilan("identité propre : " + r2.contenu().nom, r2.contenu().nom === "Accueil")
        r2.conteneur.fermerRedaction()
        // « Nom et signature » sur une adresse sans identité : proposée celle du
        // compte, rien d'écrit si l'on n'y touche pas.
        dlgIdentite.ouvrir(c.compte)
        dlgIdentite.choisirAdresse("Partagee@Exemple.fr")
        var nom = dlgIdentite.valeurs().nom
        dlgIdentite.accept()
        bilan("nom et signature repris du compte : " + nom + " | rien d'écrit " + !aUneIdentite("Partagee@Exemple.fr"),
              nom === "Personne Essai" && !aUneIdentite("Partagee@Exemple.fr"))
        // « Modifier le compte… » : le champ montre la liste, une saisie vide la retire.
        modifierCompte(c.compte)
        var affiche = dlgCompte.champAdressesEnvoi.text
        dlgCompte.champAdressesEnvoi.text = ""
        dlgCompte.accept()
        bilan("modifier le compte : « " + affiche + " » puis retirées -> " + JSON.stringify(adressesEnvoi(c.adresse)),
              affiche === "Partagee@Exemple.fr ; alias@exemple.fr" && adressesEnvoi(c.adresse).length === 0)
        console.log("scenario: adresses-envoi, " + (echecs === 0 ? "tout OK" : echecs + " échec(s)"))
        Qt.quit()
    }

    /// Scénario « partagees » (0.6.8) : la boîte de réception de deux@ est
    /// partagée avec un@ par le Dovecot d'essai. Seul compte du profil, un@ la
    /// voit proposée dans « De » ; deux@ aussi dans le profil, elle n'y est
    /// pas reprise sous un@ — elle a déjà sa ligne.
    function lancerScenarioPartagees() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var partagee = "deux@essai.test"
        var deuxComptes = identifiantsEssai.utilisateur2.length > 0
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus.filter(function(x) { return x.adresse === identifiantsEssai.utilisateur })[0]
            var pret = c !== undefined && c.partagees.length > 0 && (!deuxComptes || comptesConnus.length === 2)
            if (!pret && suivi.n < 30)
                return
            tic.stop()
            var de = function() {
                return expediteursConnus.filter(function(e) { return e.compte === c.compte })
                                        .map(function(e) { return e.adresse })
            }
            if (!pret) {
                bilan("rien de reconnu en 30 s : " + JSON.stringify(comptesConnus), false)
            } else if (deuxComptes) {
                bilan("reconnue : " + JSON.stringify(c.partagees), JSON.stringify(c.partagees) === JSON.stringify([partagee]))
                var lignes = expediteursConnus.filter(function(e) { return e.adresse === partagee })
                bilan("deux comptes, « De » de " + c.adresse + " : " + de().join(" | ") + " ; lignes de "
                      + partagee + " : " + lignes.length,
                      JSON.stringify(de()) === JSON.stringify([c.adresse]) && lignes.length === 1)
            } else {
                bilan("reconnue : " + JSON.stringify(c.partagees), JSON.stringify(c.partagees) === JSON.stringify([partagee]))
                bilan("champ « De » : " + de().join(" | "), JSON.stringify(de()) === JSON.stringify([c.adresse, partagee]))
                // Les saisies s'ajoutent ; la partagée saisie aussi n'est pas répétée.
                poserAdressesEnvoi(c.adresse, ["alias@exemple.fr", "Deux@Essai.test"])
                bilan("avec les saisies : " + de().join(" | "),
                      JSON.stringify(de()) === JSON.stringify([c.adresse, partagee, "alias@exemple.fr"]))
                var propres = autresAdresses(c.compte)
                bilan("adresses données à la préparation d'une réponse : " + JSON.stringify(propres),
                      JSON.stringify(propres) === JSON.stringify([partagee, "alias@exemple.fr"]))
                var r = ouvrirRedaction(c.compte)
                r.pret()
                r.remplir({ mode: "repondre", de: partagee, a: "client@exemple.org", objet: "RE : essai" })
                bilan("réponse depuis la partagée : " + r.contenu().de, r.contenu().de === partagee)
                r.conteneur.fermerRedaction()
                modifierCompte(c.compte)
                bilan("modifier le compte : " + JSON.stringify(dlgCompte.partagees) + " | « "
                      + dlgCompte.champAdressesEnvoi.text + " »",
                      JSON.stringify(dlgCompte.partagees) === JSON.stringify([partagee])
                      && dlgCompte.champAdressesEnvoi.text === "alias@exemple.fr ; Deux@Essai.test")
                dlgCompte.reject()
            }
            console.log("scenario: partagees, " + (suivi.echecs === 0 ? "tout OK" : suivi.echecs + " échec(s)"))
            Qt.quit()
        })
        tic.start()
    }

    /// Scénario « badge » (0.6.17) : le badge animé de la signature. Deux images
    /// fabriquées sur place dans MMAIL_SORTIE — un logo rond sur fond
    /// transparent, une « photo » en dégradé —, composées ; aperçu, insertion
    /// dans la signature (GIF du dossier des signatures, affiché à sa largeur),
    /// composition gardée pour l'adresse ; puis la signature dans une rédaction.
    /// « badge-composition » et « badge-redaction » s'arrêtent sur la fenêtre
    /// ouverte, pour une capture.
    function lancerScenarioBadge() {
        // Le compte d'abord : il n'est connu qu'une fois la session ouverte.
        var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 500; repeat: true }', fenetre)
        var essais = { n: 0 }
        attente.triggered.connect(function() {
            essais.n += 1
            var c = comptesConnus[0]
            if (c && listeComptes().length > 0 && listeComptes()[0].etat === "pret") {
                attente.stop()
                composerScenarioBadge(c)
            } else if (essais.n > 60) {
                attente.stop()
                console.log("scenario: ECHEC badge : aucun compte prêt")
                Qt.quit()
            }
        })
        attente.start()
    }

    function composerScenarioBadge(a) {
        var suivi = { n: 0, echecs: 0, etape: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        if (!identifiantsEssai.sortie) {
            console.log("scenario: ECHEC badge : MMAIL_SORTIE absent")
            Qt.quit()
            return
        }
        var dossier = lanceur.chemin(identifiantsEssai.sortie)
        var logo = Qt.createQmlObject('import QtQuick; Rectangle { width: 240; height: 240; color: "transparent";'
                                      + ' Rectangle { anchors.centerIn: parent; width: 150; height: 150; radius: 75; color: "#21abe3";'
                                      + ' Text { anchors.centerIn: parent; text: "MM"; color: "white"; font.pixelSize: 56; font.bold: true } } }',
                                      fenetre.contentItem)
        var photo = Qt.createQmlObject('import QtQuick; Rectangle { width: 320; height: 240;'
                                       + ' gradient: Gradient { GradientStop { position: 0; color: "#e61e29" }'
                                       + ' GradientStop { position: 1; color: "#0b0d2e" } }'
                                       + ' Rectangle { x: 120; y: 50; width: 80; height: 80; radius: 40; color: "#f2c9a0" } }',
                                       fenetre.contentItem)
        var b = dlgIdentite.badge
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 500; repeat: true }', fenetre)
        var composer = function() {
            logo.destroy()
            photo.destroy()
            dlgIdentite.ouvrir(a.compte)
            b.ouvrir(dlgIdentite.adresse())
            b.ajouter(lanceur.adresse(dossier + "/badge-logo.png"))
            b.ajouter(lanceur.adresse(dossier + "/badge-photo.png"))
            b.modifier(1, "genre", "photo")
            b.modifier(1, "zoom", 1.4)
            bilan("badge composé : " + b.visuels.map(function(v) { return v.genre }).join(", ")
                  + " | forme " + b.forme + " | lamelles " + b.reglages.lamelles,
                  b.visuels.length === 2 && b.visuels[1].genre === "photo")
            tic.start()
        }
        logo.grabToImage(function(r) {
            r.saveToFile(dossier + "/badge-logo.png")
            photo.grabToImage(function(r2) {
                r2.saveToFile(dossier + "/badge-photo.png")
                composer()
            })
        })
        tic.triggered.connect(function() {
            suivi.n += 1
            if (suivi.n > 120) {
                tic.stop()
                bilan("badge : délai dépassé à l'étape " + suivi.etape, false)
                console.log("scenario: badge ECHEC (" + suivi.echecs + ")")
                Qt.quit()
                return
            }
            if (suivi.etape === 0) {
                if (b.apercu.length === 0)
                    return
                // 2 images : 2 × (1 palier + 18 transitions).
                bilan("aperçu prêt : " + b.bilan.split(",")[0], b.bilan.indexOf("38 images") === 0 && b.note === "")
                suivi.etape = 1
                if (essai.scenario === "badge-composition") {
                    tic.stop()
                    return
                }
                b.inserer()
            } else if (suivi.etape === 1) {
                if (b.visible)
                    return
                var html = dlgIdentite.valeurs().html
                var image = (html.match(/<img[^>]*\.gif[^>]*>/) || [""])[0]
                bilan("badge inséré dans la signature : " + image.replace(/src="[^"]*\//, "src=\"…/").replace(/[0-9-]+\.gif/, "….gif"),
                      image.indexOf("signatures/") > 0 && image.indexOf("width=\"" + b.largeur + "\"") > 0)
                // 0.6.18 : il s'anime dans l'éditeur de la signature.
                var anim = dlgIdentite.animations.etat()
                bilan("éditeur de la signature : " + anim.length + " animation, "
                      + (anim.length > 0 ? anim[0].images + " images, " + anim[0].largeur + "×" + anim[0].hauteur : ""),
                      anim.length === 1 && anim[0].images === 38 && anim[0].largeur === b.largeur && anim[0].hauteur === b.largeur)
                dlgIdentite.accept()
                suivi.etape = 2
            } else if (suivi.etape === 2) {
                b.ouvrir(a.adresse)
                bilan("composition gardée pour l'adresse : " + b.visuels.length + " images, "
                      + b.visuels.map(function(v) { return v.genre + " ×" + v.zoom }).join(", "),
                      b.visuels.length === 2 && b.visuels[1].zoom === 1.4)
                b.close()
                var r = ouvrirRedaction(a.compte)
                r.remplir({ a: identifiantsEssai.utilisateur2, objet: "Badge animé", mode: "nouveau" })
                suivi.redaction = r
                suivi.etape = 3
            } else if (suivi.etape === 3) {
                var corps = suivi.redaction.htmlActuel()
                bilan("rédaction : la signature porte le badge",
                      /<img[^>]*signatures\/[^"]*\.gif"[^>]*width="138"/.test(corps))
                var animR = suivi.redaction.animations.etat()
                bilan("rédaction : " + animR.length + " animation, joue " + (animR.length > 0 && animR[0].joue),
                      animR.length === 1 && animR[0].joue)
                tic.stop()
                if (essai.scenario === "badge-redaction")
                    return
                console.log("scenario: badge " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
    }

    /// Scénario « gif-anime » (0.6.18, banc GIF=1) : un message porte un GIF
    /// animé et un GIF fixe. Dans le volet de lecture, puis dans la fenêtre du
    /// message, une animation — une seule — est posée sur le GIF animé, à la
    /// place que le texte lui réserve, et elle tourne.
    function lancerScenarioGifAnime() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var releve = Qt.createQmlObject('import QtQuick; Timer { interval: 100; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, index: -1, images: [], animations: null }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        releve.triggered.connect(function() {
            var e = suivi.animations ? suivi.animations.etat() : []
            if (e.length > 0)
                suivi.images.push(e[0].image)
        })
        var observer = function(animations) {
            suivi.animations = animations
            suivi.images = []
            releve.start()
        }
        // Place de l'animation comparée à celle du caractère de l'image dans le
        // texte, taille du GIF (60×40), images vues tourner.
        var verifier = function(nom, zone, animations) {
            releve.stop()
            var e = animations.etat()
            bilan(nom + " : " + e.length + " animation(s) pour un GIF animé et un fixe", e.length === 1)
            if (e.length !== 1)
                return
            var a = e[0]
            var p = zone.getText(0, zone.length).indexOf("\ufffc")
            var r1 = zone.positionToRectangle(p), r2 = zone.positionToRectangle(p + 1)
            var place = Math.abs(a.x - r1.x) < 0.5 && Math.abs(a.largeur - (r2.x - r1.x)) < 1
                    && a.y >= r1.y - 0.5 && a.y + a.hauteur <= r1.y + r1.height + 0.5
            bilan(nom + " : animation " + [a.x, a.y, a.largeur, a.hauteur].map(Math.round).join(",")
                  + " | caractère " + [r1.x, r1.y, r2.x - r1.x, r1.height].map(Math.round).join(","),
                  place && a.largeur === 60 && a.hauteur === 40)
            var vues = {}
            suivi.images.forEach(function(k) { vues[k] = true })
            bilan(nom + " : " + a.images + " images, vues " + Object.keys(vues).sort().join("/") + ", joue " + a.joue,
                  a.images === 2 && a.joue && Object.keys(vues).length === 2)
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                for (var i = 0; i < modeleMessages.count; ++i)
                    if (modeleMessages.get(i).sujet === "GIF anime")
                        suivi.index = i
                if (suivi.index < 0) {
                    bilan("message « GIF anime » absent (banc GIF=1 ?)", false)
                    suivi.n = 15
                    return
                }
                choisir(suivi.index, 0)
            } else if (suivi.n === 9) {
                observer(colonneLecture.animations)
            } else if (suivi.n === 11) {
                verifier("volet de lecture", colonneLecture.vueCorps, colonneLecture.animations)
                ouvrirFenetreMessage(suivi.index)
            } else if (suivi.n === 13) {
                if (fenetresMessage.length === 0) {
                    bilan("fenêtre du message non ouverte", false)
                    suivi.n = 15
                    return
                }
                observer(fenetresMessage[0].animations)
            } else if (suivi.n === 15) {
                if (fenetresMessage.length > 0)
                    verifier("fenêtre du message", fenetresMessage[0].corps, fenetresMessage[0].animations)
                if (essai.scenario === "gif-anime-capture") {
                    tic.stop()
                    return
                }
            } else if (suivi.n === 16) {
                tic.stop()
                console.log("scenario: gif-anime " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « veille-reception » (signalement du 30/10/2026 : les nouveaux
    /// messages n'apparaissaient plus, « Actualiser » sans effet, il fallait
    /// relancer MMail). Boîte de réception unifiée affichée ; le banc
    /// (LIVRAISON=1, SANS_IDLE=1 : serveur qui ne notifie pas) livre un
    /// message dans la boîte de réception de deux@ — compte que l'on ne
    /// regarde pas — quand le journal dit « scenario: livrer N », et coupe
    /// les sessions sur « scenario: couper ». 1 : la veille le montre ;
    /// 2 : « Actualiser » le montre ; 3 : après la coupure, la reconnexion
    /// automatique le montre.
    function lancerScenarioVeilleReception() {
        var suivi = { echecs: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        // Attend qu'une ligne de plus paraisse dans la liste, d'elle-même.
        var attendre = function(delai, suite) {
            var debut = Date.now()
            var avant = modeleMessages.count
            var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
            tic.triggered.connect(function() {
                var ecoule = Math.round((Date.now() - debut) / 1000)
                if (modeleMessages.count > avant || ecoule >= delai) {
                    tic.stop()
                    suite(modeleMessages.count > avant, ecoule)
                }
            })
            tic.start()
        }
        var dire = function(numero, bon, s) {
            return "message " + numero + (bon ? " affiché après " : " absent après ") + s + " s"
        }
        plusTard(12000, function() {
            fenetre.ouvrirBoitesUnifiees()
            console.log("scenario: unifiée,", modeleMessages.count, "messages")
            console.log("scenario: livrer 1")
            attendre(170, function(bon, s) {
                bilan("veille : " + dire(1, bon, s), bon)
                console.log("scenario: livrer 2")
                plusTard(3000, function() {
                    boite.actualiser()
                    attendre(30, function(bon2, s2) {
                        bilan("actualiser : " + dire(2, bon2, s2), bon2)
                        console.log("scenario: couper")
                        plusTard(8000, function() {
                            console.log("scenario: livrer 3")
                            attendre(260, function(bon3, s3) {
                                bilan("reconnexion : " + dire(3, bon3, s3), bon3)
                                console.log("scenario: veille-reception " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                                Qt.quit()
                            })
                        })
                    })
                })
            })
        })
    }

    /// Scénario « groupes-dates » (0.6.29) : la liste regroupée par date de
    /// réception. 1 : les groupes de dates choisies, pour trois jours de
    /// référence — un vendredi (la semaine du changement d'heure derrière
    /// lui), un dimanche, un lundi qui ouvre le mois ; 2 : la boîte de
    /// réception de un@ (banc DATES=1 : messages reçus de 1 à 400 jours plus
    /// tôt) — chaque ligne porte le groupe de sa date, chaque groupe est d'un
    /// seul tenant, leurs intitulés sont à l'écran ; 3 : « Regrouper les
    /// messages par date » les retire, puis les rend ; 4 : une recherche triée
    /// par expéditeur n'en a pas, triée par date si. Avec MMAIL_DOSSIER à
    /// « garder », la liste reste ouverte, groupée, pour une capture.
    function lancerScenarioGroupesDates() {
        var suivi = { echecs: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var vendredi = new Date(2026, 9, 30, 15, 0)
        var dimanche = new Date(2026, 10, 1, 11, 0)
        var lundi = new Date(2026, 10, 2, 9, 0)
        var cas = [
            [vendredi, new Date(2026, 9, 31, 8, 0), "Aujourd'hui"],
            [vendredi, new Date(2026, 9, 30, 0, 0), "Aujourd'hui"],
            [vendredi, new Date(2026, 9, 29, 23, 59), "Hier"],
            [vendredi, new Date(2026, 9, 28, 0, 0), "Avant-hier"],
            [vendredi, new Date(2026, 9, 27, 23, 59), "Mardi"],
            [vendredi, new Date(2026, 9, 26, 0, 0), "Lundi"],
            [vendredi, new Date(2026, 9, 25, 23, 59), "La semaine dernière"],
            [vendredi, new Date(2026, 9, 19, 0, 0), "La semaine dernière"],
            [vendredi, new Date(2026, 9, 18, 12, 0), "Il y a deux semaines"],
            [vendredi, new Date(2026, 9, 5, 0, 0), "Il y a trois semaines"],
            [vendredi, new Date(2026, 9, 4, 12, 0), "Plus tôt ce mois-ci"],
            [vendredi, new Date(2026, 8, 1, 0, 0), "Le mois dernier"],
            [vendredi, new Date(2026, 7, 31, 23, 59), "Plus ancien"],
            [vendredi, new Date(0), "Plus ancien"],
            [dimanche, new Date(2026, 9, 29, 12, 0), "Jeudi"],
            [dimanche, new Date(2026, 9, 26, 0, 0), "Lundi"],
            [dimanche, new Date(2026, 9, 25, 23, 0), "La semaine dernière"],
            [lundi, new Date(2026, 10, 1, 20, 0), "Hier"],
            [lundi, new Date(2026, 9, 31, 8, 0), "Avant-hier"],
            [lundi, new Date(2026, 9, 30, 8, 0), "La semaine dernière"],
            [lundi, new Date(2026, 9, 26, 0, 0), "La semaine dernière"],
            [lundi, new Date(2026, 9, 25, 12, 0), "Il y a deux semaines"],
            [lundi, new Date(2026, 9, 12, 0, 0), "Il y a trois semaines"],
            [lundi, new Date(2026, 9, 11, 23, 0), "Le mois dernier"],
            [lundi, new Date(2026, 8, 30, 12, 0), "Plus ancien"]
        ]
        var faux = []
        for (var i = 0; i < cas.length; ++i) {
            var g = groupeDate(cas[i][1].getTime() / 1000, bornesGroupes(cas[i][0]))
            if (g !== cas[i][2])
                faux.push(dateLongue(cas[i][1].toISOString()) + " le " + dateLongue(cas[i][0].toISOString())
                          + " : « " + g + " » au lieu de « " + cas[i][2] + " »")
        }
        bilan("bornes : " + (cas.length - faux.length) + "/" + cas.length + " dates à leur groupe"
              + (faux.length > 0 ? " — " + faux.join(" ; ") : ""), faux.length === 0)

        // Intitulés de groupe à l'écran, de haut en bas.
        var intitules = function() {
            var v = colonneListe.vueMessages
            var enfants = v.contentItem.children
            var vus = []
            for (var k = 0; k < enfants.length; ++k) {
                var e = enfants[k]
                if (e.section !== undefined && e.visible && e.y + e.height > v.contentY && e.y < v.contentY + v.height)
                    vus.push({ y: e.y, texte: e.section })
            }
            vus.sort(function(a, b) { return a.y - b.y })
            return vus.map(function(x) { return x.texte })
        }
        plusTard(8000, function() {
            ouvrirDossier(comptesConnus[0].compte, "INBOX")
            plusTard(5000, function() {
                var bornes = bornesGroupes()
                var suite = []
                var mal = 0
                for (var r = 0; r < modeleMessages.count; ++r) {
                    var l = modeleMessages.get(r)
                    if (l.groupe !== groupeDate(l.h, bornes))
                        mal += 1
                    if (suite.length === 0 || suite[suite.length - 1] !== l.groupe)
                        suite.push(l.groupe)
                }
                var uniques = suite.filter(function(x, n) { return suite.indexOf(x) === n })
                bilan("liste : " + modeleMessages.count + " messages, groupes " + suite.join(" | ")
                      + (mal > 0 ? " — " + mal + " ligne(s) au mauvais groupe" : ""),
                      mal === 0 && uniques.length === suite.length && uniques.length >= 6)
                var ecran = intitules()
                bilan("intitulés à l'écran : " + ecran.join(" | "),
                      ecran.length >= 3 && ecran.every(function(t, n) { return t === suite[n] }))
                reglages.groupesDates = false
                plusTard(1000, function() {
                    bilan("option retirée : section « " + colonneListe.vueMessages.section.property + " », "
                          + intitules().length + " intitulé(s)",
                          colonneListe.vueMessages.section.property === "" && intitules().length === 0)
                    reglages.groupesDates = true
                    plusTard(1000, function() {
                        bilan("option rendue : " + intitules().length + " intitulé(s)", intitules().length >= 3)
                        lancerRecherche("Essai")
                        trierRecherche("expediteur")
                        plusTard(3000, function() {
                            bilan("recherche triée par expéditeur : " + intitules().length + " intitulé(s)",
                                  !groupesVisibles && intitules().length === 0)
                            trierRecherche("recents")
                            plusTard(1000, function() {
                                bilan("recherche triée par date : " + intitules().join(" | "),
                                      groupesVisibles && intitules().length >= 1)
                                quitterRecherche()
                                plusTard(3000, function() {
                                    console.log("scenario: groupes-dates " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                                    if (identifiantsEssai.dossier !== "garder")
                                        Qt.quit()
                                })
                            })
                        })
                    })
                })
            })
        })
    }

    // --- Ctrl+Z, molette, groupes repliés (demandes d'un utilisateur, 03/11/2026)

    function bilanEssai(suivi, texte, bon) {
        if (!bon)
            suivi.echecs += 1
        console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
    }
    function sujetsDeLaListe() {
        var s = []
        for (var i = 0; i < modeleMessages.count; ++i)
            s.push(modeleMessages.get(i).sujet)
        return s.sort().join(" | ")
    }

    /// Scénario « annuler » (banc à deux comptes) : Ctrl+Z défait une
    /// suppression, un déplacement de deux messages, deux gestes l'un après
    /// l'autre, puis un déplacement vers l'autre boîte — où le même
    /// Message-ID est déjà (graines du banc) : c'est le message déplacé qui
    /// revient, pas l'autre.
    property var suiviAnnuler: null
    function lancerScenarioAnnuler() {
        suiviAnnuler = { etape: 0, echecs: 0, avant: "", compte: 0, autre: 0 }
        plusTard(8000, function() {
            var comptes = listeComptes()
            var un = comptes.filter(function(c) { return c.adresse === "un@essai.test" })[0]
            var deux = comptes.filter(function(c) { return c.adresse === "deux@essai.test" })[0]
            if (!un || !deux) {
                console.log("scenario: ECHEC comptes " + JSON.stringify(comptes))
                Qt.quit()
                return
            }
            suiviAnnuler.compte = un.id
            suiviAnnuler.autre = deux.id
            ouvrirDossier(un.id, "INBOX")
            plusTard(4000, function() { etapeScenarioAnnuler("depart") })
        })
    }
    function etapeScenarioAnnuler(genre, nombre, erreurs, detail) {
        var s = suiviAnnuler
        if (!s)
            return
        var bilan = function(texte, bon) { bilanEssai(s, texte, bon) }
        // Les messages revenus, la liste relue : comme au départ.
        var apresRetour = function(texte, suite) {
            plusTard(2500, function() {
                var liste = sujetsDeLaListe()
                bilan(texte + " : liste « " + liste + " », annulation suivante « " + boite.annulation + " »",
                      liste === s.avant)
                suite()
            })
        }
        if (genre === "depart" && s.etape === 0) {
            s.avant = sujetsDeLaListe()
            bilan("départ : " + modeleMessages.count + " messages, rien à annuler (« " + boite.annulation + " »)",
                  modeleMessages.count >= 3 && boite.annulation === "" && !raccourciAnnuler.enabled)
            s.etape = 1
            choisir(0, 0)
            supprimerSelection()
        } else if (genre === "deplacement" && s.etape === 1) {
            bilan("suppression : " + nombre + " message, annulable " + detail + ", « " + boite.annulation
                  + " », Ctrl+Z actif " + raccourciAnnuler.enabled + ", barre d'état « " + messageEtat.texte + " »",
                  nombre === 1 && detail === true && boite.annulation === "Annuler la suppression"
                  && raccourciAnnuler.enabled && messageEtat.texte.indexOf("Ctrl+Z") >= 0)
            s.etape = 2
            // Comme au clavier : ce que fait le raccourci.
            raccourciAnnuler.activated()
        } else if (genre === "annulation" && s.etape === 2) {
            bilan("suppression annulée : " + nombre + " rétabli, " + detail + " introuvable « " + erreurs + " »",
                  nombre === 1 && detail === 0 && erreurs === "")
            s.etape = 3
            apresRetour("après la suppression annulée", function() {
                choisir(0, 0)
                choisir(1, Qt.ControlModifier)
                deplacerVers(s.compte, "Essais")
            })
        } else if (genre === "deplacement" && s.etape === 3) {
            bilan("déplacement de " + nombre + " messages vers « Essais », « " + boite.annulation + " »",
                  nombre === 2 && boite.annulation === "Annuler le déplacement")
            s.etape = 4
            fenetre.annuler()
        } else if (genre === "annulation" && s.etape === 4) {
            bilan("déplacement annulé : " + nombre + " rétablis", nombre === 2 && erreurs === "")
            s.etape = 5
            apresRetour("après le déplacement annulé", function() {
                choisir(0, 0)
                supprimerSelection()
            })
        } else if (genre === "deplacement" && s.etape === 5) {
            s.etape = 6
            choisir(0, 0)
            deplacerVers(s.compte, "Archive")
        } else if (genre === "deplacement" && s.etape === 6) {
            bilan("deux gestes : le dernier, « " + boite.annulation + " »", boite.annulation === "Annuler le déplacement")
            s.etape = 7
            fenetre.annuler()
        } else if (genre === "annulation" && s.etape === 7) {
            bilan("premier Ctrl+Z : " + nombre + " rétabli, reste « " + boite.annulation + " »",
                  nombre === 1 && boite.annulation === "Annuler la suppression")
            s.etape = 8
            fenetre.annuler()
        } else if (genre === "annulation" && s.etape === 8) {
            bilan("second Ctrl+Z : " + nombre + " rétabli", nombre === 1 && erreurs === "")
            s.etape = 9
            apresRetour("après les deux gestes annulés", function() {
                bilan("plus rien à annuler : Ctrl+Z actif " + raccourciAnnuler.enabled + ", annuler() " + fenetre.annuler(),
                      boite.annulation === "" && !raccourciAnnuler.enabled)
                choisir(0, 0)
                s.sujet = modeleMessages.get(0).sujet
                deplacerVers(s.autre, "INBOX")
            })
        } else if (genre === "deplacement" && s.etape === 9) {
            bilan("vers l'autre boîte : " + nombre + " message, « " + boite.annulation + " »",
                  nombre === 1 && boite.annulation === "Annuler le déplacement")
            s.etape = 10
            fenetre.annuler()
        } else if (genre === "annulation" && s.etape === 10) {
            bilan("retour de l'autre boîte : " + nombre + " rétabli, " + detail + " introuvable « " + erreurs + " »",
                  nombre === 1 && detail === 0 && erreurs === "")
            s.etape = 11
            apresRetour("après le retour de l'autre boîte", function() {
                // Dans l'autre boîte, la graine au même Message-ID est restée.
                ouvrirDossier(s.autre, "INBOX")
                plusTard(3000, function() {
                    var meme = 0
                    for (var i = 0; i < modeleMessages.count; ++i)
                        if (modeleMessages.get(i).sujet === s.sujet)
                            meme += 1
                    bilan("l'autre boîte garde sa graine « " + s.sujet + " » : " + meme, meme === 1)
                    console.log("scenario: annuler " + (s.echecs === 0 ? "OK" : "ECHEC (" + s.echecs + ")"))
                    Qt.quit()
                })
            })
        }
    }

    /// Scénario « molette » (banc NOMBRE=60) : de vrais crans de molette sur
    /// la liste (gestesEssai) — d'abord sans la molette de MMail, pour
    /// mesurer le défilement propre de Qt, puis avec.
    function lancerScenarioMolette() {
        var s = { echecs: 0 }
        var bilan = function(texte, bon) { bilanEssai(s, texte, bon) }
        var v = colonneListe.vueMessages
        var cran = function(angle, pixels) {
            return gestesEssai.molette(v, v.width / 2, v.height / 2, angle, pixels)
        }
        plusTard(8000, function() {
            ouvrirDossier(comptesConnus[0].compte, "INBOX")
            plusTard(5000, function() {
                fenetre.completerListe()
                v.positionViewAtBeginning()
                var hauteur = colonneListe.hauteurLigne()
                var lignes = Qt.styleHints.wheelScrollLines
                console.log("scenario: " + modeleMessages.count + " messages, ligne de " + Math.round(hauteur)
                            + " px, vue de " + Math.round(v.height) + " px, réglage du poste : " + lignes + " lignes par cran")
                // Sans la molette de MMail : Flickable seul.
                colonneListe.molette.enabled = false
                var y0 = v.contentY
                cran(-120, 0)
                plusTard(1500, function() {
                    var natif = v.contentY - y0
                    console.log("scenario: avant (Qt seul) : un cran = " + Math.round(natif) + " px, "
                                + (natif / hauteur).toFixed(2) + " ligne(s)")
                    colonneListe.molette.enabled = true
                    v.positionViewAtBeginning()
                    plusTard(500, function() {
                        var attendu = Math.min(lignes * hauteur, Math.max(hauteur, v.height - hauteur))
                        y0 = v.contentY
                        cran(-120, 0)
                        var un = v.contentY - y0
                        bilan("un cran = " + Math.round(un) + " px (" + (un / hauteur).toFixed(2) + " lignes, attendu "
                              + Math.round(attendu) + ")", Math.abs(un - attendu) < 1)
                        var apresUn = v.contentY
                        // Une seconde plus tard, rien n'a bougé : pas d'élan de
                        // Flickable par-dessus.
                        plusTard(1000, function() {
                            bilan("sans élan : " + Math.round(v.contentY) + " une seconde après, " + Math.round(apresUn)
                                  + " au cran", Math.abs(v.contentY - apresUn) < 1)
                            y0 = v.contentY
                            cran(-120, 0); cran(-120, 0); cran(-120, 0)
                            var trois = v.contentY - y0
                            bilan("trois crans = " + Math.round(trois) + " px", Math.abs(trois - 3 * attendu) < 1)
                            y0 = v.contentY
                            cran(120, 0)
                            bilan("un cran vers le haut = " + Math.round(v.contentY - y0) + " px",
                                  Math.abs(v.contentY - y0 + attendu) < 1)
                            y0 = v.contentY
                            cran(-120, -40)
                            bilan("pavé tactile, 40 px : " + Math.round(v.contentY - y0) + " px", Math.abs(v.contentY - y0 - 40) < 1)
                            for (var k = 0; k < 80; ++k)
                                cran(-120, 0)
                            var bas = v.originY + v.contentHeight - v.height
                            bilan("tout en bas : contentY " + Math.round(v.contentY) + ", bas " + Math.round(bas),
                                  Math.abs(v.contentY - bas) < 1)
                            for (var j = 0; j < 80; ++j)
                                cran(120, 0)
                            bilan("tout en haut : contentY " + Math.round(v.contentY) + ", haut " + Math.round(v.originY),
                                  Math.abs(v.contentY - v.originY) < 1)
                            console.log("scenario: molette " + (s.echecs === 0 ? "OK" : "ECHEC (" + s.echecs + ")"))
                            Qt.quit()
                        })
                    })
                })
            })
        })
    }

    /// Scénario « groupes-replier » (banc DATES=1 NOMBRE=40) : un groupe se
    /// replie d'un clic sur son intitulé et se déplie ; le clavier et Maj+clic
    /// sautent ses messages ; un message qui arrive dans un groupe replié y
    /// reste caché ; tout replier, tout déplier ; l'état suit le dossier ; sans
    /// les groupes, tout se voit.
    function lancerScenarioGroupesReplier() {
        var s = { echecs: 0 }
        var bilan = function(texte, bon) { bilanEssai(s, texte, bon) }
        var v = colonneListe.vueMessages
        // Groupes de la liste, dans l'ordre : { nom, debut, taille }.
        var groupes = function() {
            var g = []
            for (var i = 0; i < modeleMessages.count; ++i) {
                var nom = modeleMessages.get(i).groupe
                if (g.length === 0 || g[g.length - 1].nom !== nom)
                    g.push({ nom: nom, debut: i, taille: 0 })
                g[g.length - 1].taille += 1
            }
            return g
        }
        var enTete = function(nom) {
            var enfants = v.contentItem.children
            for (var k = 0; k < enfants.length; ++k)
                if (enfants[k].section === nom && enfants[k].visible)
                    return enfants[k]
            return null
        }
        var visibles = function() { return v.count }
        plusTard(8000, function() {
            var compte = comptesConnus[0].compte
            ouvrirDossier(compte, "INBOX")
            plusTard(5000, function() {
                fenetre.completerListe()
                var n = modeleMessages.count
                var g = groupes()
                console.log("scenario: " + n + " messages, groupes " + g.map(function(x) { return x.nom + " (" + x.taille + ")" }).join(" | "))
                // Un groupe du milieu d'au moins deux messages, puis le suivant.
                var k = -1
                for (var i = 1; i < g.length - 1 && k < 0; ++i)
                    if (g[i].taille >= 2)
                        k = i
                if (k < 0) {
                    console.log("scenario: ECHEC pas de groupe du milieu à replier")
                    Qt.quit()
                    return
                }
                var cible = g[k]
                v.positionViewAtIndex(cible.debut, ListView.Beginning)
                plusTard(500, function() {
                    var tete = enTete(cible.nom)
                    bilan("intitulé « " + cible.nom + " » à l'écran, déplié", tete !== null && !tete.replie)
                    if (tete)
                        gestesEssai.clic(tete, 30, tete.height / 2, Qt.LeftButton)
                    plusTard(500, function() {
                        tete = enTete(cible.nom)
                        var representant = v.itemAtIndex(colonneListe.rangVue(cible.debut))
                        bilan("clic : « " + cible.nom + " » replié, " + visibles() + " lignes dans la vue sur " + n
                              + ", intitulé " + (tete ? (tete.replie ? "replié" : "déplié") : "absent")
                              + ", sa première ligne haute de " + (representant ? representant.height : "?") + " px",
                              visibles() === n - cible.taille + 1 && tete !== null && tete.replie
                              && representant !== null && representant.height === 0)
                        // Rangs : la ligne qui suit le groupe replié.
                        var apres = cible.debut + cible.taille
                        var rangApres = colonneListe.rangVue(apres)
                        var ligneApres = v.itemAtIndex(rangApres)
                        bilan("rangs : modèle " + apres + " = vue " + rangApres + ", retour " + colonneListe.rangModele(rangApres)
                              + ", la ligne dit " + (ligneApres ? ligneApres.rang : "?"),
                              rangApres === apres - cible.taille + 1 && colonneListe.rangModele(rangApres) === apres
                              && ligneApres !== null && ligneApres.rang === apres
                              && colonneListe.rangVue(cible.debut + 1) === -1)
                        // Clavier : de la dernière ligne d'avant à la première d'après.
                        var avant = cible.debut - 1
                        choisir(avant, 0)
                        deplacerCurseur(1, false)
                        var choisis = uidsChoisis()
                        bilan("flèche bas : du rang " + avant + " au message " + (choisis.length === 1 ? indexDe(choisis[0]) : "?")
                              + " (attendu " + apres + ")", choisis.length === 1 && indexDe(choisis[0]) === apres)
                        deplacerCurseur(-1, false)
                        choisis = uidsChoisis()
                        bilan("flèche haut : retour au rang " + (choisis.length === 1 ? indexDe(choisis[0]) : "?"),
                              choisis.length === 1 && indexDe(choisis[0]) === avant)
                        choisir(apres, Qt.ShiftModifier)
                        bilan("Maj+clic par-dessus le groupe : " + uidsChoisis().length + " messages choisis",
                              uidsChoisis().length === 2)
                        // Un message qui arrive dans le groupe replié, en tête
                        // puis en fin de groupe : il reste caché, l'intitulé reste.
                        var modele = modeleMessages.get(cible.debut)
                        var neuf = {}
                        var roles = ["uid", "h", "expediteur", "adresse", "destinataires", "sujet", "date", "taille", "lu",
                                     "repondu", "reponduTous", "transfere", "pieces", "suivi", "importance", "motsCles",
                                     "compte", "chemin", "dossier", "role", "cheminAffiche", "groupe"]
                        for (var r = 0; r < roles.length; ++r)
                            neuf[roles[r]] = modele[roles[r]]
                        neuf.uid = 999001
                        neuf.h = modele.h + 1
                        neuf.sujet = "Arrivé dans un groupe replié"
                        modeleMessages.insert(cible.debut, neuf)
                        plusTard(200, function() {
                            var premier = v.itemAtIndex(colonneListe.rangVue(cible.debut))
                            bilan("arrivée en tête du groupe replié : " + visibles() + " lignes (attendu " + (n - cible.taille + 1)
                                  + "), l'ancienne tête cachée " + (colonneListe.rangVue(cible.debut + 1) === -1),
                                  visibles() === n - cible.taille + 1 && colonneListe.rangVue(cible.debut + 1) === -1
                                  && premier !== null && premier.height === 0 && enTete(cible.nom) !== null)
                            // Puis elle repart : l'ancienne tête reprend l'intitulé.
                            modeleMessages.remove(cible.debut)
                            plusTard(200, function() {
                                bilan("retrait de la tête : " + visibles() + " lignes, intitulé " + (enTete(cible.nom) !== null),
                                      visibles() === n - cible.taille + 1 && colonneListe.rangVue(cible.debut) >= 0
                                      && enTete(cible.nom) !== null)
                                var tete2 = enTete(cible.nom)
                                if (tete2)
                                    gestesEssai.clic(tete2, 30, tete2.height / 2, Qt.RightButton)
                                plusTard(500, function() {
                                    bilan("clic droit : menu des groupes ouvert", colonneListe.menuGroupes.opened)
                                    colonneListe.menuGroupes.close()
                                    colonneListe.replierTout(true)
                                    plusTard(500, function() {
                                        bilan("tout replié : " + visibles() + " lignes pour " + g.length + " groupes",
                                              visibles() === g.length)
                                        colonneListe.replierTout(false)
                                        plusTard(300, function() {
                                            bilan("tout déplié : " + visibles() + " lignes sur " + n, visibles() === n)
                                            colonneListe.basculerGroupe(cible.nom)
                                            ouvrirDossier(compte, "Essais")
                                            plusTard(3000, function() {
                                                bilan("autre dossier : " + visibles() + " lignes sur " + modeleMessages.count,
                                                      visibles() === modeleMessages.count)
                                                ouvrirDossier(compte, "INBOX")
                                                plusTard(4000, function() {
                                                    fenetre.completerListe()
                                                    plusTard(500, function() {
                                                        bilan("retour au dossier : « " + cible.nom + " » toujours replié, "
                                                              + visibles() + " lignes", visibles() === n - cible.taille + 1)
                                                        reglages.groupesDates = false
                                                        plusTard(500, function() {
                                                            bilan("sans les groupes : " + visibles() + " lignes sur " + n, visibles() === n)
                                                            reglages.groupesDates = true
                                                            plusTard(500, function() {
                                                                bilan("groupes rendus : " + visibles() + " lignes",
                                                                      visibles() === n - cible.taille + 1)
                                                                colonneListe.basculerGroupe(cible.nom)
                                                                console.log("scenario: groupes-replier " + (s.echecs === 0 ? "OK" : "ECHEC (" + s.echecs + ")"))
                                                                Qt.quit()
                                                            })
                                                        })
                                                    })
                                                })
                                            })
                                        })
                                    })
                                })
                            })
                        })
                    })
                })
            })
        })
    }

    /// Scénario « groupes-minuit » (banc DATES=1) : signalement d'un
    /// utilisateur du 04/11/2026 — un groupe déplié sans ses messages, le
    /// groupe suivant sans intitulé, jusqu'à ce qu'on replie le premier.
    /// Toutes les combinaisons de groupes repliés, d'abord telles quelles,
    /// puis au passage de minuit (la liste regroupée comme la veille, puis
    /// `suivreJour`). Après chaque étape : chaque groupe a un intitulé, dans
    /// l'ordre, replié ou non selon l'état ; un groupe déplié montre toutes ses
    /// lignes, avec leur hauteur ; un groupe replié n'en garde qu'une, sans
    /// hauteur.
    function lancerScenarioGroupesMinuit() {
        var s = { echecs: 0 }
        var v = colonneListe.vueMessages
        var groupes = function() {
            var g = []
            for (var i = 0; i < modeleMessages.count; ++i) {
                var nom = modeleMessages.get(i).groupe
                if (g.length === 0 || g[g.length - 1].nom !== nom)
                    g.push({ nom: nom, debut: i, taille: 0 })
                g[g.length - 1].taille += 1
            }
            return g
        }
        var regrouper = function(maintenant) {
            var bornes = fenetre.bornesGroupes(maintenant)
            for (var i = 0; i < modeleMessages.count; ++i) {
                var g = fenetre.groupeDate(modeleMessages.get(i).h, bornes)
                if (modeleMessages.get(i).groupe !== g)
                    modeleMessages.setProperty(i, "groupe", g)
            }
            colonneListe.appliquerReplis(0)
        }
        // Ce que montre la vue : intitulés (dans l'ordre vertical) et lignes.
        var releve = function() {
            var enfants = v.contentItem.children
            var tetes = []
            for (var k = 0; k < enfants.length; ++k) {
                var e = enfants[k]
                if (e.section !== undefined && e.replie !== undefined && e.visible)
                    tetes.push({ nom: e.section, replie: e.replie, y: e.mapToItem(v.contentItem, 0, 0).y })
            }
            tetes.sort(function(a, b) { return a.y - b.y })
            var lignes = []
            for (var r = 0; r < v.count; ++r) {
                var it = v.itemAtIndex(r)
                lignes.push({ rang: colonneListe.rangModele(r), hauteur: it ? it.height : -1 })
            }
            return { tetes: tetes, lignes: lignes }
        }
        var controler = function(etiquette) {
            var g = groupes()
            var vu = releve()
            var replis = colonneListe.replisVue
            var ecarts = []
            var noms = vu.tetes.map(function(t) { return t.nom + (t.replie ? "(r)" : "") })
            var attendus = g.map(function(x) { return x.nom + (replis[x.nom] === true ? "(r)" : "") })
            if (noms.join("|") !== attendus.join("|"))
                ecarts.push("intitulés [" + noms.join(" | ") + "] au lieu de [" + attendus.join(" | ") + "]")
            for (var i = 0; i < g.length; ++i) {
                var rep = replis[g[i].nom] === true
                var montrees = vu.lignes.filter(function(l) { return l.rang >= g[i].debut && l.rang < g[i].debut + g[i].taille })
                if (rep) {
                    if (montrees.length !== 1 || montrees[0].hauteur !== 0)
                        ecarts.push(g[i].nom + " replié : " + montrees.length + " lignes, hauteurs " + montrees.map(function(l) { return l.hauteur }).join(","))
                } else {
                    var basses = montrees.filter(function(l) { return !(l.hauteur > 0) })
                    if (montrees.length !== g[i].taille || basses.length > 0)
                        ecarts.push(g[i].nom + " déplié : " + montrees.length + "/" + g[i].taille + " lignes, hauteurs " + montrees.map(function(l) { return l.hauteur }).join(","))
                }
            }
            bilanEssai(s, etiquette + (ecarts.length ? " : " + ecarts.join(" ; ") : ""), ecarts.length === 0)
        }
        var etapes = []
        plusTard(8000, function() {
            ouvrirDossier(comptesConnus[0].compte, "INBOX")
            plusTard(5000, function() {
                fenetre.completerListe()
                v.positionViewAtBeginning()
                var g = groupes()
                console.log("scenario: " + modeleMessages.count + " messages, groupes " + g.map(function(x) { return x.nom + " (" + x.taille + ")" }).join(" | "))
                var noms = g.map(function(x) { return x.nom })
                var n = Math.min(noms.length, 6)
                // 1. Toutes les combinaisons des six premiers groupes, sans changer de jour.
                for (var masque = 0; masque < (1 << n); ++masque) {
                    (function(m) {
                        etapes.push(function(suite) {
                            var etat = {}
                            for (var b = 0; b < n; ++b)
                                if (m & (1 << b))
                                    etat[noms[b]] = true
                            colonneListe.poserReplis(etat)
                            plusTard(250, function() {
                                controler("même jour, repliés {" + Object.keys(etat).join(", ") + "}")
                                suite()
                            })
                        })
                    })(masque)
                }
                // 2. Passage de minuit : la veille, telle combinaison ; puis le jour change.
                var hier = new Date(Date.now() - 86400000)
                var nomsHier = []
                etapes.push(function(suite) {
                    colonneListe.poserReplis({})
                    regrouper(hier)
                    nomsHier = groupes().map(function(x) { return x.nom })
                    console.log("scenario: la veille, groupes " + groupes().map(function(x) { return x.nom + " (" + x.taille + ")" }).join(" | "))
                    plusTard(250, function() { controler("la veille, rien de replié"); suite() })
                })
                for (var m2 = 0; m2 < (1 << 5); ++m2) {
                    (function(m) {
                        etapes.push(function(suite) {
                            colonneListe.poserReplis({})
                            regrouper(hier)
                            var etat = {}
                            for (var b = 0; b < Math.min(5, nomsHier.length); ++b)
                                if (m & (1 << b))
                                    etat[nomsHier[b]] = true
                            colonneListe.poserReplis(etat)
                            plusTard(250, function() {
                                controler("la veille, repliés {" + Object.keys(etat).join(", ") + "}")
                                fenetre.jourDesGroupes = hier.toDateString()
                                fenetre.suivreJour()
                                plusTard(300, function() {
                                    controler("après minuit, repliés {" + Object.keys(colonneListe.replisVue).join(", ") + "}")
                                    suite()
                                })
                            })
                        })
                    })(m2)
                }
                var k = 0
                var suivante = function() {
                    if (k >= etapes.length) {
                        console.log("scenario: groupes-minuit " + (s.echecs === 0 ? "OK" : "ECHEC (" + s.echecs + ")"))
                        Qt.quit()
                        return
                    }
                    var etape = etapes[k++]
                    etape(suivante)
                }
                suivante()
            })
        })
    }

    /// Scénario « marges » (0.6.23) : « Affichage › Marges du message », ses
    /// trois choix l'un après l'autre, dans le volet de lecture et dans la
    /// fenêtre du message. Le texte doit se décaler de la marge choisie, vers
    /// la droite et, depuis la 0.6.27, vers le bas. Avec
    /// MMAIL_DOSSIER à « aucune », « etroites » ou « larges », ce choix reste
    /// posé pour une capture (--capture). Le retrait d'une lettre centrée à sa
    /// largeur de conception s'ajoute à la marge, et en prend sa part.
    function lancerScenarioMarges() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, marges: {}, x0: -1, y0: -1, retrait0: 0 }
        var choix = ["aucune", "etroites", "larges"]
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var mesurer = function(valeur) {
            var v = colonneLecture.vueCorps
            var f = fenetresMessage.length > 0 ? fenetresMessage[0].corps : null
            var x = v.positionToRectangle(0).x
            var y = v.positionToRectangle(0).y
            if (valeur === "aucune") {
                suivi.x0 = x
                suivi.y0 = y
                suivi.retrait0 = v.retrait
            }
            suivi.marges[valeur] = fenetre.margeMessage
            bilan("marges « " + valeur + " » : " + fenetre.margeMessage + " px | volet " + v.leftPadding + "/"
                  + v.rightPadding + "/" + v.topPadding + "/" + v.bottomPadding + ", texte à "
                  + Math.round(x) + "," + Math.round(y) + ", retrait " + v.retrait + " | fenêtre "
                  + (f ? f.leftPadding + "/" + f.rightPadding + "/" + f.topPadding + "/" + f.bottomPadding
                         + ", retrait " + f.retrait
                       : "absente"),
                  v.leftPadding === v.padding + 4 + fenetre.margeMessage + v.retrait
                  && v.rightPadding === v.padding + fenetre.margeMessage + v.retrait
                  && v.topPadding === v.padding + fenetre.margeMessage
                  && v.bottomPadding === v.padding + fenetre.margeMessage
                  && Math.abs(x - suivi.x0 - fenetre.margeMessage - (v.retrait - suivi.retrait0)) < 1
                  && Math.abs(y - suivi.y0 - fenetre.margeMessage) < 1
                  && f !== null && f.leftPadding - f.retrait === v.leftPadding - v.retrait
                  && f.rightPadding - f.retrait === v.rightPadding - v.retrait
                  && f.topPadding === v.topPadding && f.bottomPadding === v.bottomPadding)
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            if (suivi.n === 4) {
                ouvrirDossier(comptesConnus[0].compte, "INBOX")
            } else if (suivi.n === 7) {
                // Le long message HTML du banc MARGES=1, ou celui de GIF=1,
                // s'il y est ; le premier sinon.
                var index = 0
                for (var i = modeleMessages.count - 1; i >= 0; --i)
                    if (modeleMessages.get(i).sujet === "GIF anime")
                        index = i
                for (var j = modeleMessages.count - 1; j >= 0; --j)
                    if (modeleMessages.get(j).sujet === "Compte rendu de la réunion")
                        index = j
                choisir(index, 0)
                ouvrirFenetreMessage(index)
            } else if (suivi.n >= 10 && suivi.n <= 10 + choix.length) {
                var k = suivi.n - 10
                if (k > 0)
                    mesurer(choix[k - 1])
                if (k < choix.length) {
                    reglages.margesMessage = choix[k]
                    return
                }
                bilan("marges croissantes : " + choix.map(function(c) { return suivi.marges[c] }).join(" < "),
                      suivi.marges.aucune === 0 && suivi.marges.etroites > 0
                      && suivi.marges.larges > suivi.marges.etroites)
                tic.stop()
                if (choix.indexOf(identifiantsEssai.dossier) >= 0) {
                    reglages.margesMessage = identifiantsEssai.dossier
                    return
                }
                console.log("scenario: marges " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }

    /// Scénario « corps-long » (0.6.20) : un long message HTML à tableau (banc
    /// LONG=1), descendu jusqu'en bas par pas de 40 px puis remonté. À chaque
    /// pas, le corps visible est capturé dans MMAIL_SORTIE
    /// (corps-<rang>-<sens>-<position>.png) : il restait blanc en remontant
    /// (cf. cpp/rendu_complet.h). Les captures blanches se comptent hors de MMail.
    function lancerScenarioCorpsLong() {
        if (!identifiantsEssai.sortie) {
            console.log("scenario: ECHEC corps-long : MMAIL_SORTIE absent")
            Qt.quit()
            return
        }
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, index: -1, pas: [], rang: 0 }
        var cadre = colonneLecture.cadreCorps.contentItem
        var suivant = function() {
            if (suivi.rang >= suivi.pas.length) {
                console.log("scenario: corps-long " + suivi.pas.length + " captures")
                Qt.quit()
                return
            }
            var p = suivi.pas[suivi.rang]
            cadre.contentY = p.y
            // Une image rendue avant la capture : le défilement a eu lieu.
            Qt.callLater(function() {
                var chemin = lanceur.chemin(identifiantsEssai.sortie + "/corps-" + ("00" + suivi.rang).slice(-3)
                                            + "-" + p.sens + "-" + Math.round(p.y) + ".png")
                colonneLecture.cadreCorps.grabToImage(function(r) {
                    r.saveToFile(chemin)
                    suivi.rang += 1
                    suivant()
                })
            })
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 7) {
                for (var i = 0; i < modeleMessages.count; ++i)
                    if (modeleMessages.get(i).sujet === "Corps long")
                        suivi.index = i
                if (suivi.index < 0) {
                    tic.stop()
                    console.log("scenario: ECHEC message « Corps long » absent (banc LONG=1 ?)")
                    Qt.quit()
                    return
                }
                choisir(suivi.index, 0)
            } else if (suivi.n === 10) {
                tic.stop()
                var course = cadre.contentHeight - cadre.height
                console.log("scenario: corps-long " + colonneLecture.vueCorps.text.length + " caractères, course "
                            + Math.round(course) + " px")
                var ys = []
                for (var y = 0; y < course; y += 40)
                    ys.push(y)
                ys.push(course)
                ys.forEach(function(y) { suivi.pas.push({ sens: "bas", y: y }) })
                ys.slice().reverse().forEach(function(y) { suivi.pas.push({ sens: "haut", y: y }) })
                suivant()
            }
        })
        tic.start()
    }

    /// Scénario « bibliotheque-signatures » (0.6.18) : la bibliothèque de signatures. Une
    /// identité posée à l'ancienne y est rangée ; « Nom et signature… » en
    /// crée une autre, choisie pour les réponses, en duplique et en supprime ;
    /// la rédaction prend l'une pour un nouveau message, l'autre pour une
    /// réponse ; une signature supprimée n'est plus choisie ; « Annuler » ne
    /// change rien ; chaque identité garde sa signature à l'ancienne.
    function lancerScenarioSignatures() {
        var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 500; repeat: true }', fenetre)
        var essais = { n: 0 }
        attente.triggered.connect(function() {
            essais.n += 1
            var c = comptesConnus[0]
            if (c && listeComptes().length > 0 && listeComptes()[0].etat === "pret") {
                attente.stop()
                etapesScenarioSignatures(c)
            } else if (essais.n > 60) {
                attente.stop()
                console.log("scenario: ECHEC bibliotheque-signatures : aucun compte prêt")
                Qt.quit()
            }
        })
        attente.start()
    }

    function etapesScenarioSignatures(c) {
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var identiteDe = function(a) { return lireJson(reglagesRedaction.identites, {})[a] || {} }
        var noms = function() { return signaturesNommees().map(function(s) { return s.nom }).join(" | ") }
        poserIdentite(c.adresse, { nom: "Personne Essai", signature: "Sig à l'ancienne", html: "",
                                   nouveaux: true, reponses: false })
        var l = signaturesNommees(), i = identiteDe(c.adresse)
        bilan("signature d'avant rangée : " + noms() + " | nouveaux « " + i.sigNouveaux + " », réponses « " + i.sigReponses + " »",
              l.length === 1 && l[0].nom === c.adresse && i.sigNouveaux === l[0].id && i.sigReponses === ""
              && i.signature === "Sig à l'ancienne" && i.nouveaux === true && i.reponses === false)

        dlgIdentite.ouvrir(c.compte)
        dlgIdentite.nouvelle(null)
        dlgIdentite.essaiSignature("Commerciale", "Service commercial")
        var idc = dlgIdentite.biblio[dlgIdentite.choisie].id
        dlgIdentite.essaiChoix("sigReponses", idc)
        dlgIdentite.nouvelle(dlgIdentite.biblio[dlgIdentite.choisie])
        var avec = dlgIdentite.biblio.map(function(s) { return s.nom }).join(" | ")
        dlgIdentite.supprimer()
        bilan("dupliquer puis supprimer : " + avec + " → " + dlgIdentite.biblio.length + " signatures",
              dlgIdentite.biblio.length === 2 && avec.indexOf("Commerciale (copie)") > 0)
        dlgIdentite.accept()
        l = signaturesNommees()
        i = identiteDe(c.adresse)
        bilan("bibliothèque : " + noms() + " | réponses → « " + i.sigReponses + " » | miroir « " + i.signature + " »",
              l.length === 2 && l[1].nom === "Commerciale" && l[1].signature === "Service commercial"
              && i.sigNouveaux === l[0].id && i.sigReponses === l[1].id && i.signature === "Sig à l'ancienne"
              && i.reponses === true)
        // Captures : la fenêtre ouverte sur la seconde signature, ou le menu
        // « Signature » de la rédaction.
        if (essai.scenario === "bibliotheque-vue") {
            dlgIdentite.ouvrir(c.compte)
            dlgIdentite.editer(1)
            return
        }

        var r = ouvrirRedaction(c.compte)
        r.pret()
        if (essai.scenario === "bibliotheque-menu") {
            r.proposerSignatures(r.boutonSignature)
            return
        }
        var t1 = r.contenu().texte
        r.remplir({ mode: "repondre", de: c.adresse, a: "client@exemple.org", objet: "RE : essai", texte: "\n> cité" })
        var t2 = r.contenu().texte
        bilan("rédaction : nouveau message avec l'ancienne " + (t1.indexOf("Sig à l'ancienne") >= 0)
              + ", réponse avec « Commerciale » " + (t2.indexOf("Service commercial") >= 0 && t2.indexOf("Sig à l'ancienne") < 0),
              t1.indexOf("Sig à l'ancienne") >= 0 && t2.indexOf("Service commercial") >= 0 && t2.indexOf("Sig à l'ancienne") < 0)
        r.insererSignature(l[0])
        bilan("« Signature » : l'autre insérée au curseur", r.contenu().texte.indexOf("Sig à l'ancienne") >= 0)
        r.conteneur.fermerRedaction()

        dlgIdentite.ouvrir(c.compte)
        dlgIdentite.editer(1)
        dlgIdentite.supprimer()
        dlgIdentite.accept()
        i = identiteDe(c.adresse)
        bilan("signature des réponses supprimée : " + noms() + " | réponses « " + i.sigReponses + " »",
              signaturesNommees().length === 1 && i.sigReponses === "" && i.reponses === false)

        dlgIdentite.ouvrir(c.compte)
        dlgIdentite.nouvelle(null)
        dlgIdentite.essaiSignature("Jetée", "À ne pas garder")
        dlgIdentite.reject()
        bilan("« Annuler » : " + noms(), signaturesNommees().length === 1)
        console.log("scenario: bibliotheque-signatures " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
        Qt.quit()
    }

    /// Scénario « editeur-tableau » (0.8.7, demande d'un utilisateur qui
    /// répond dans les tableaux cités, et voudrait colorer ses commentaires) :
    /// plus de case « Mise en forme », le HTML part toujours ; dans un tableau
    /// à la façon d'Excel (en-tête sur fond bleu, texte en Calibri, couleur),
    /// « + Colonne » ajoute une colonne dont les cases reprennent fond et
    /// police de leur voisine, et ce qui s'y écrit aussi ; « + Ligne » de même ;
    /// la couleur du texte et le surlignage se posent sur la sélection, et
    /// « Automatique » retire la couleur. MMAIL_DOSSIER « garder » : la
    /// rédaction reste ouverte, pour une capture.
    function etapeScenarioEditeurTableau() {
        var r = fenetre.ouvrirRedaction(comptesConnus.length > 0 ? comptesConnus[0].compte : 0)
        plusTard(1500, function() {
            var echecs = 0
            var bilan = function(texte, bon) {
                if (!bon)
                    ++echecs
                console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
            }
            var chercher = function(racine, test) {
                if (!racine)
                    return null
                if (test(racine))
                    return racine
                var enfants = racine.children || []
                for (var i = 0; i < enfants.length; ++i) {
                    var o = chercher(enfants[i], test)
                    if (o)
                        return o
                }
                return null
            }
            var mef = r.mefCorps
            var corps = r.corps
            var compter = function(texte, motif) { return (texte.match(motif) || []).length }
            // Les cases du tableau cité : le HTML de Qt enveloppe le message
            // dans un tableau « racine » d'une case (-qt-table-type: root).
            var cases = function(texte) { return compter(texte, /<td/g) - compter(texte, /-qt-table-type: root/g) }
            var position = function(mot) { return corps.getText(0, corps.length).indexOf(mot) }

            bilan("plus de case « Mise en forme »",
                  chercher(r, function(o) { return o.text === "Mise en forme" && o.visible }) === null)
            mef.poserTexte("Bonjour")
            bilan("un message sans mise en forme part aussi en HTML", r.contenu().html.indexOf("Bonjour") > 0)

            var cellule = function(fond, couleur, texte) {
                return "<td" + (fond ? " bgcolor=\"" + fond + "\"" : "") + "><span style=\"font-family:'Calibri'; "
                       + "font-size:11pt; color:" + couleur + "\">" + texte + "</span></td>"
            }
            mef.poserHtml("<p>Avant</p><table border=\"1\" cellspacing=\"0\" cellpadding=\"4\">"
                          + "<tr>" + cellule("#1f4e79", "#ffffff", "Point") + cellule("#1f4e79", "#ffffff", "Statut") + "</tr>"
                          + "<tr>" + cellule("", "#203864", "Livraison") + cellule("", "#203864", "En cours") + "</tr>"
                          + "</table><p>Après</p>")
            corps.forceActiveFocus()
            corps.cursorPosition = position("Avant") + 1
            bilan("hors du tableau : pas de « + Colonne »", !mef.dansTableau)
            corps.cursorPosition = position("En cours") + 2
            bilan("dans le tableau", mef.dansTableau)
            var boutonColonne = chercher(r, function(o) { return o.objectName === "boutonColonne" })
            bilan("« + Colonne » proposé", !!boutonColonne && boutonColonne.visible === !fenetre.tactile)

            bilan("colonne ajoutée", r.ajouterAuTableau("colonne"))
            mef.inserer(corps.cursorPosition, "OK pour moi")
            var h = mef.html()
            var tableau = /<table[\s\S]*<\/table>/.exec(h)
            if (cases(h) !== 6 || compter(h, /bgcolor="#1f4e79"/g) !== 3)
                console.log("scenario: tableau après la colonne :", tableau ? tableau[0] : h)
            bilan("3 colonnes, 6 cases : " + cases(h), cases(h) === 6)
            bilan("l'en-tête ajouté garde le fond bleu : " + compter(h, /bgcolor="#1f4e79"/g),
                  compter(h, /bgcolor="#1f4e79"/g) === 3)
            var texteAjoute = /<span style="[^"]*">OK pour moi<\/span>/.exec(h)
            console.log("scenario: écrit dans la colonne ajoutée :", texteAjoute ? texteAjoute[0] : h)
            bilan("ce qui s'y écrit prend la police et la couleur de la voisine",
                  !!texteAjoute && texteAjoute[0].indexOf("Calibri") > 0 && texteAjoute[0].indexOf("#203864") > 0)

            corps.cursorPosition = position("OK pour moi") + 1
            bilan("ligne ajoutée", r.ajouterAuTableau("ligne"))
            mef.inserer(corps.cursorPosition, "Relance")
            h = mef.html()
            bilan("3 lignes, 9 cases : " + cases(h), cases(h) === 9)
            var relance = /<span style="[^"]*">Relance<\/span>/.exec(h)
            bilan("la ligne ajoutée prend la police et la couleur de celle du dessus",
                  !!relance && relance[0].indexOf("Calibri") > 0 && relance[0].indexOf("#203864") > 0)
            bilan("pas de fond bleu dans la ligne ajoutée", compter(h, /bgcolor="#1f4e79"/g) === 3)

            var debut = position("OK pour moi")
            corps.select(debut, debut + "OK pour moi".length)
            r.paletteCouleurs.ouvrir("texte", chercher(r, function(o) { return o.objectName === "boutonCouleur" }) || corps)
            var cases = chercher(r.paletteCouleurs.contentItem, function(o) { return o.objectName === "couleur-#c00000" })
            bilan("palette ouverte, rouge foncé proposé", r.paletteCouleurs.opened || r.paletteCouleurs.visible)
            bilan("case du rouge foncé", !!cases)
            r.paletteCouleurs.choisir("#c00000")
            h = mef.html()
            var colore = /<span style="[^"]*">OK pour moi<\/span>/.exec(h)
            bilan("couleur posée : " + (colore ? colore[0] : ""), !!colore && colore[0].indexOf("color:#c00000") > 0)
            bilan("la couleur sous le curseur se lit : " + mef.couleur, mef.couleur === "#c00000")
            corps.select(debut, debut + "OK pour moi".length)
            r.paletteCouleurs.ouvrir("surlignage", corps)
            r.paletteCouleurs.choisir("#ffff00")
            h = mef.html()
            colore = /<span style="[^"]*">OK pour moi<\/span>/.exec(h)
            bilan("surlignage posé", !!colore && colore[0].indexOf("background-color:#ffff00") > 0)
            corps.select(debut, debut + "OK pour moi".length)
            mef.poserCouleur("")
            h = mef.html()
            colore = /<span style="[^"]*">OK pour moi<\/span>/.exec(h)
            bilan("« Automatique » retire la couleur, le surlignage reste",
                  !!colore && colore[0].indexOf(" color:") < 0 && colore[0].indexOf("background-color:#ffff00") > 0)
            bilan("le reste du tableau garde sa couleur", compter(h, /color:#203864/g) >= 2)
            var t = mef.texte()
            bilan("version en texte brut : " + JSON.stringify(t),
                  t.indexOf("OK pour moi") >= 0 && t.indexOf("Relance") >= 0 && t.indexOf("En cours") >= 0)
            corps.cursorPosition = position("Relance")
            console.log("scenario: editeur-tableau " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
            if (identifiantsEssai.dossier !== "garder")
                Qt.quit()
        })
    }

    /// Scénario « editeur-paragraphe » (0.8.7) : police, taille, alignement
    /// et retrait se posent, se relisent sous le curseur, et partent dans le
    /// HTML en CSS ordinaire (pas de `-qt-block-indent`) ; la barre du bureau
    /// va à la ligne dans une fenêtre étroite. MMAIL_DOSSIER « garder » : la
    /// rédaction reste ouverte, pour une capture.
    function etapeScenarioEditeurParagraphe() {
        var r = fenetre.ouvrirRedaction(comptesConnus.length > 0 ? comptesConnus[0].compte : 0)
        plusTard(1500, function() {
            var echecs = 0
            var bilan = function(texte, bon) {
                if (!bon)
                    ++echecs
                console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
            }
            var mef = r.mefCorps
            var corps = r.corps
            var position = function(mot) { return corps.getText(0, corps.length).indexOf(mot) }
            var bloc = function(h, mot) {
                var m = new RegExp("<p[^>]*>(?:(?!</p>).)*" + mot + "(?:(?!</p>).)*</p>").exec(h)
                return m ? m[0] : ""
            }
            mef.poserTexte("Un premier paragraphe\nDeux\nTrois\nQuatre")
            corps.forceActiveFocus()
            corps.select(position("Deux"), position("Deux") + 4)
            mef.poserPolice("Georgia")
            mef.poserTaille(16)
            bilan("police et taille relues : " + mef.police + " " + mef.taille, mef.police === "Georgia" && mef.taille === 16)
            var h = mef.html()
            bilan("police et taille dans le HTML : " + bloc(h, "Deux"),
                  bloc(h, "Deux").indexOf("font-family:'Georgia'") > 0 && bloc(h, "Deux").indexOf("font-size:16pt") > 0)
            corps.cursorPosition = position("Trois") + 1
            mef.poserAlignement(Qt.AlignHCenter)
            bilan("alignement relu : " + mef.alignement, mef.alignement === Qt.AlignHCenter)
            h = mef.html()
            bilan("centré dans le HTML : " + bloc(h, "Trois"), bloc(h, "Trois").indexOf('align="center"') > 0)
            corps.cursorPosition = position("Quatre") + 1
            mef.changerRetrait(1)
            mef.changerRetrait(1)
            h = mef.html()
            bilan("retrait de deux crans en CSS : " + bloc(h, "Quatre"), bloc(h, "Quatre").indexOf("margin-left:80px") > 0)
            bilan("pas de retrait propre à Qt", h.indexOf("-qt-block-indent:1") < 0 && h.indexOf("-qt-block-indent:2") < 0)
            mef.changerRetrait(-1)
            mef.changerRetrait(-1)
            mef.changerRetrait(-1)
            h = mef.html()
            bilan("le retrait ne descend pas sous zéro : " + bloc(h, "Quatre"), bloc(h, "Quatre").indexOf("margin-left:0px") > 0)
            corps.select(position("Deux"), position("Deux") + 4)
            mef.poserPolice("")
            mef.poserTaille(0)
            h = mef.html()
            bilan("police et taille par défaut rétablies : " + bloc(h, "Deux"),
                  bloc(h, "Deux").indexOf("Georgia") < 0 && bloc(h, "Deux").indexOf("16pt") < 0)
            var barre = objetNomme(r, "barreMiseEnForme")
            if (!fenetre.tactile && barre) {
                bilan("barre du bureau, police et taille visibles", objetNomme(r, "choixPolice").visible && objetNomme(r, "choixTaille").visible)
                // La barre va à la ligne quand ses boutons ne tiennent pas sur
                // une seule : rien ne dépasse, et elle prend alors plusieurs
                // lignes.
                var total = 0
                var haut = 0
                var hors = []
                for (var i = 0; i < barre.children.length; ++i) {
                    var b = barre.children[i]
                    if (!b.visible || b.width <= 0)
                        continue
                    total += b.width + barre.spacing
                    haut = Math.max(haut, b.height)
                    if (b.x + b.width > barre.width + 0.5)
                        hors.push(b.text || b.objectName)
                }
                bilan("rien ne dépasse de la barre (" + Math.round(barre.width) + " points) : " + JSON.stringify(hors), hors.length === 0)
                bilan("boutons sur " + Math.round(total) + " points pour " + Math.round(barre.width) + " : hauteur " + barre.implicitHeight
                      + " pour une ligne de " + haut, total <= barre.width || barre.implicitHeight > haut + 0.5)
            }
            console.log("scenario: editeur-paragraphe " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
            if (identifiantsEssai.dossier !== "garder")
                Qt.quit()
        })
    }

    /// Scénario « editeur-tableau-outils » (0.8.7, suite) : « Tableau ▾ »
    /// insère un tableau bordé sur chaque case, dont ce qui s'écrit prend la
    /// police du curseur ; Tab passe de case en case, Maj+Tab revient, et Tab
    /// depuis la dernière case ajoute une ligne, qu'un seul Ctrl+Z retire ;
    /// hors d'un tableau, Tab garde son effet ; « Supprimer la colonne »,
    /// « … la ligne », « … le tableau ». Les touches passent par
    /// gestesEssai.touche, comme une frappe de l'utilisateur.
    function etapeScenarioEditeurTableauOutils() {
        var r = fenetre.ouvrirRedaction(comptesConnus.length > 0 ? comptesConnus[0].compte : 0)
        plusTard(1500, function() {
            var echecs = 0
            var bilan = function(texte, bon) {
                if (!bon)
                    ++echecs
                console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
            }
            var mef = r.mefCorps
            var corps = r.corps
            var compter = function(texte, motif) { return (texte.match(motif) || []).length }
            // Le HTML de Qt enveloppe le message dans un tableau « racine »
            // d'une ligne et d'une case (-qt-table-type: root).
            var racine = function(h) { return compter(h, /-qt-table-type: root/g) }
            var tableaux = function(h) { return compter(h, /<table/g) - racine(h) }
            var lignes = function(h) { return compter(h, /<tr/g) - racine(h) }
            var cases = function(h) { return compter(h, /<td/g) - racine(h) }
            var clavier = typeof gestesEssai !== "undefined" && typeof gestesEssai.touche === "function"
            var tab = function(arriere) {
                if (clavier)
                    gestesEssai.touche(corps, arriere ? Qt.Key_Backtab : Qt.Key_Tab, "\t")
                else
                    r.tabulerTableau(arriere)
            }
            console.log("scenario: Tab " + (clavier ? "frappé au clavier" : "par la fonction (geste « touche » absent)"))
            var menu = r.menuTableau
            var entree = function(libelle) {
                for (var i = 0; i < menu.count; ++i) {
                    var e = menu.itemAt(i)
                    if (e && e.text === libelle)
                        return e
                }
                return null
            }

            if (!fenetre.tactile)
                bilan("« Tableau ▾ » dans la barre", !!objetNomme(r, "boutonTableau") && objetNomme(r, "boutonTableau").visible)
            else
                bilan("« Tableau » au bas de la rédaction", !!objetNomme(r, "boutonTableauTactile") && objetNomme(r, "boutonTableauTactile").visible)
            mef.poserTexte("Bonjour")
            corps.forceActiveFocus()
            corps.select(0, 7)
            mef.poserPolice("Georgia")
            corps.cursorPosition = 7
            bilan("hors d'un tableau, « Supprimer la ligne » grisé",
                  !!entree("Supprimer la ligne") && !entree("Supprimer la ligne").enabled && entree("Insérer un tableau…").enabled)

            // Fenêtre d'insertion : 4 colonnes tapées sans Entrée, 2 lignes
            // d'office.
            r.dialogueTableau.open()
            plusTard(400, function() {
                var champ = objetNomme(r.dialogueTableau.contentItem, "champColonnes")
                if (champ)
                    champ.contentItem.text = "4"
                r.dialogueTableau.accept()
                plusTard(400, function() {
                    var h = mef.html()
                    bilan("un tableau inséré, 2 lignes de 4 cases : " + tableaux(h) + " / " + lignes(h) + " / " + cases(h),
                          tableaux(h) === 1 && lignes(h) === 2 && cases(h) === 8)
                    bilan("sur toute la largeur", /<table[^>]*width="100%"/.test(h))
                    bilan("chaque case bordée d'un trait noir : " + compter(h, /border-left-style:solid/g),
                          compter(h, /border-left-style:solid/g) === 8 && compter(h, /border-left-color:#000000/g) === 8)
                    bilan("curseur dans la première case", mef.dansTableau)
                    mef.inserer(corps.cursorPosition, "Point")
                    h = mef.html()
                    var point = /<span style="[^"]*">Point<\/span>/.exec(h)
                    bilan("ce qui s'écrit dans le tableau prend la police du curseur : " + (point ? point[0] : ""),
                          !!point && point[0].indexOf("Georgia") > 0)

                    tab(false)
                    mef.inserer(corps.cursorPosition, "Statut")
                    var t = mef.texte()
                    bilan("Tab : case suivante, sans tabulation écrite : " + JSON.stringify(t.substring(0, 40)),
                          t.indexOf("Point\nStatut") >= 0 && t.indexOf("\t") < 0)
                    tab(true)
                    mef.inserer(corps.cursorPosition, " A")
                    t = mef.texte()
                    bilan("Maj+Tab : fin de la case précédente", t.indexOf("Point A\nStatut") >= 0)
                    for (var i = 0; i < 7; ++i)
                        tab(false)
                    mef.inserer(corps.cursorPosition, "Fin")
                    r.modifie = false
                    tab(false)
                    h = mef.html()
                    bilan("Tab depuis la dernière case : une ligne de plus (" + lignes(h) + " lignes, " + cases(h) + " cases)",
                          lignes(h) === 3 && cases(h) === 12)
                    bilan("message marqué modifié", r.modifie)
                    corps.undo()
                    h = mef.html()
                    bilan("un seul Ctrl+Z retire la ligne : " + lignes(h), lignes(h) === 2 && mef.texte().indexOf("Fin") >= 0)
                    corps.cursorPosition = corps.getText(0, corps.length).indexOf("Fin") + 3
                    tab(false)
                    mef.inserer(corps.cursorPosition, "Nouvelle")
                    h = mef.html()
                    var nouvelle = /<span style="[^"]*">Nouvelle<\/span>/.exec(h)
                    t = mef.texte()
                    bilan("la nouvelle ligne suit la dernière, à la police de celle du dessus : " + (nouvelle ? nouvelle[0] : ""),
                          t.indexOf("Fin\nNouvelle") >= 0 && !!nouvelle && nouvelle[0].indexOf("Georgia") > 0)
                    // MMAIL_DOSSIER « garder » : la rédaction reste ouverte sur
                    // le tableau, menu « Tableau » déplié, pour une capture.
                    if (identifiantsEssai.dossier === "garder") {
                        var bouton = objetNomme(r, fenetre.tactile ? "boutonTableauTactile" : "boutonTableau")
                        if (bouton)
                            r.menuTableau.popup(bouton, 0, fenetre.tactile ? -r.menuTableau.implicitHeight : bouton.height)
                        console.log("scenario: editeur-tableau-outils, rédaction gardée pour la capture "
                                    + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
                        return
                    }

                    corps.cursorPosition = 3
                    tab(false)
                    t = mef.texte()
                    if (clavier)
                        bilan("hors d'un tableau, Tab écrit une tabulation : " + JSON.stringify(t.substring(0, 12)), t.indexOf("Bon\tjour") === 0)
                    else
                        bilan("hors d'un tableau, Tab n'est pas pris", !r.tabulerTableau(false))

                    corps.cursorPosition = corps.getText(0, corps.length).indexOf("Statut") + 2
                    bilan("« Supprimer la colonne » proposé dans le tableau", !!entree("Supprimer la colonne") && entree("Supprimer la colonne").enabled)
                    r.supprimerDuTableau("colonne")
                    h = mef.html()
                    t = mef.texte()
                    bilan("colonne retirée : " + lignes(h) + " lignes, " + cases(h) + " cases",
                          lignes(h) === 3 && cases(h) === 9 && t.indexOf("Statut") < 0 && t.indexOf("Point A") >= 0)
                    corps.cursorPosition = corps.getText(0, corps.length).indexOf("Nouvelle") + 2
                    r.supprimerDuTableau("ligne")
                    h = mef.html()
                    t = mef.texte()
                    bilan("ligne retirée : " + lignes(h) + " lignes, " + cases(h) + " cases",
                          lignes(h) === 2 && cases(h) === 6 && t.indexOf("Nouvelle") < 0 && t.indexOf("Fin") >= 0)
                    corps.cursorPosition = corps.getText(0, corps.length).indexOf("Point") + 1
                    r.supprimerDuTableau("tableau")
                    h = mef.html()
                    t = mef.texte()
                    bilan("tableau retiré, le texte d'avant reste : " + JSON.stringify(t),
                          tableaux(h) === 0 && t.indexOf("Point") < 0 && t.indexOf("jour") >= 0 && !mef.dansTableau)
                    console.log("scenario: editeur-tableau-outils " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
                    if (identifiantsEssai.dossier !== "garder")
                        Qt.quit()
                })
            })
        })
    }

    /// Scénario « reponse-tableau » (0.8.7 ; banc EML=…/tableau-excel.eml) :
    /// répondre à un message dont le corps porte un tableau collé d'Excel —
    /// bordure en style sur chaque case, largeurs fixes, `nowrap` —, comme le
    /// fait l'utilisateur qui écrit ses réponses à la suite des lignes du
    /// tableau cité. Dans la rédaction, le tableau cité a une bordure de
    /// tableau (0.6.37) ; « + Colonne » au bout d'une ligne, Tab jusqu'à la
    /// case ajoutée de la ligne suivante ; ce qui s'y écrit part dans le HTML,
    /// dans les cases ajoutées, à la police du tableau. MMAIL_DOSSIER
    /// « garder » : la rédaction reste ouverte, pour une capture.
    function lancerScenarioReponseTableau() {
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                ++echecs
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var fin = function() {
            console.log("scenario: reponse-tableau " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
            if (identifiantsEssai.dossier !== "garder")
                Qt.quit()
        }
        plusTard(8000, function() {
            var compte = comptesConnus[0].compte
            ouvrirDossier(compte, "INBOX")
            plusTard(4000, function() {
                var i = -1
                for (var k = 0; k < modeleMessages.count; ++k)
                    if (modeleMessages.get(k).sujet === "Ramasses de la semaine")
                        i = k
                if (i < 0) {
                    bilan("« Ramasses de la semaine » absent (banc EML=…/tableau-excel.eml ?)", false)
                    fin()
                    return
                }
                choisir(i, 0)
                plusTard(3000, function() {
                    // Comme le bouton « Répondre » (rediger), la fenêtre gardée.
                    var r = ouvrirRedaction(compte)
                    r.attendre(qsTr("Préparation…"))
                    bilan("réponse préparée", boite.preparer(uidCourant, "repondre", r.jeton,
                                                             JSON.stringify(autresAdresses(compte))))
                    plusTard(4000, function() {
                        var mef = r.mefCorps
                        var corps = r.corps
                        var compter = function(texte, motif) { return (texte.match(motif) || []).length }
                        var racineQt = function(h) { return compter(h, /-qt-table-type: root/g) }
                        var cases = function(h) { return compter(h, /<td/g) - racineQt(h) }
                        var texte = function() { return corps.getText(0, corps.length) }
                        var h = mef.html()
                        bilan("tableau cité, 3 lignes de 3 cases : " + cases(h), cases(h) === 9)
                        bilan("le tableau cité a une bordure", /<table[^>]*border="1"/.test(h))
                        var t = texte()
                        var date = t.indexOf("04.11")
                        var quai = t.indexOf("3", date + 5)
                        bilan("ligne « Transports Alpha » trouvée", date > 0 && quai > date)
                        corps.forceActiveFocus()
                        corps.cursorPosition = quai + 1
                        bilan("dans le tableau cité", mef.dansTableau)
                        bilan("« + Colonne »", r.ajouterAuTableau("colonne"))
                        mef.inserer(corps.cursorPosition, "OK pour moi")
                        var clavier = typeof gestesEssai !== "undefined" && typeof gestesEssai.touche === "function"
                        for (var n = 0; n < 4; ++n) {
                            if (clavier)
                                gestesEssai.touche(corps, Qt.Key_Tab, "\t")
                            else
                                r.tabulerTableau(false)
                        }
                        mef.inserer(corps.cursorPosition, "Reporté")
                        h = mef.html()
                        var brut = mef.texte()
                        bilan("4 colonnes, 12 cases : " + cases(h), cases(h) === 12)
                        bilan("Tab " + (clavier ? "au clavier" : "par la fonction") + " : la réponse suit la ligne d'après — "
                              + JSON.stringify(brut.substring(brut.indexOf("Transports Alpha"), brut.indexOf("Merci"))),
                              brut.indexOf("3\nOK pour moi\nTransports Beta") >= 0 && brut.indexOf("5\nReporté") >= 0
                              && brut.indexOf("\t") < 0)
                        var ok = /<td[^>]*>(?:(?!<\/td>)[\s\S])*OK pour moi(?:(?!<\/td>)[\s\S])*<\/td>/.exec(h)
                        console.log("scenario: case ajoutée :", ok ? ok[0] : "")
                        bilan("ce qui s'écrit dans la colonne ajoutée prend la police du tableau",
                              !!ok && ok[0].indexOf("Calibri") > 0)
                        var envoi = r.contenu().html
                        bilan("le HTML envoyé porte les deux réponses dans le tableau",
                              /<table[^>]*border="1"[\s\S]*OK pour moi[\s\S]*Reporté[\s\S]*<\/table>/.test(envoi))
                        fin()
                    })
                })
            })
        })
    }

    /// Scénario « carnet » (0.6.20) : le carnet d'adresses modifiable, contre
    /// le Radicale du banc (CARNET=1) — carnets et droits, création,
    /// modification, conflit avec une écriture faite ailleurs entre-temps
    /// (le banc change la fiche sur le disque du serveur à l'annonce
    /// « modification sur le serveur »), correspondant ajouté depuis l'en-tête,
    /// suppression, carnet en lecture seule, fiches vides et doublons.
    function lancerScenarioCarnet() {
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var etapes = []
        var suite = function() {
            var e = etapes.shift()
            if (e)
                plusTard(300, e)
        }
        // Prochaine réponse du noyau à une écriture dans les carnets.
        var attenteEcriture = null
        boite.contactsEcrits.connect(function(ok, message, conflit) {
            var a = attenteEcriture
            attenteEcriture = null
            if (a)
                plusTard(200, function() { a(ok, message.replace(/: \/.*contacts-retires\/[0-9-]+\.vcf/, ": <copie>"), conflit) })
        })
        var attenteSynchro = null
        boite.agendasSynchronises.connect(function(change, erreurs) {
            var a = attenteSynchro
            attenteSynchro = null
            if (a)
                a(change, erreurs)
        })
        var trouver = function(texte) { return JSON.parse(boite.carnet(texte)) }
        var fiche = function(id) { return JSON.parse(boite.ficheContact(id) || "{}") }
        var id = 0

        etapes.push(function() {
            attenteSynchro = function(change, erreurs) {
                var carnets = JSON.parse(boite.carnetsModifiables())
                bilan("carnets modifiables : " + carnets.map(function(c) { return c.nom + (c.defaut ? " (défaut)" : "") }).join(", ")
                      + " | erreurs de un@ « " + erreurs.split("\n").filter(function(e) { return e.indexOf("un@") === 0 }).join(" ; ") + " »",
                      carnets.length === 1 && carnets[0].nom === "Personnel" && carnets[0].defaut)
                var tous = trouver("")
                var ro = trouver("protegee")
                bilan("fiches lues : " + tous.length + " | « Fiche Protegee » modifiable " + (ro.length && ro[0].modifiable)
                      + " (" + (ro.length ? ro[0].motif : "") + ")",
                      tous.length === 7 && ro.length === 1 && !ro[0].modifiable && ro[0].motif === "carnet en lecture seule sur le serveur")
                // « carnet-fiche », « carnet-nettoyage » : le dialogue ouvert,
                // pour une capture.
                if (essai.scenario === "carnet-fiche") {
                    var f = JSON.parse(boite.ficheContact(trouver("doublon")[0].id))
                    dlgFiche.ouvrirFiche(f.id)
                    return
                }
                if (essai.scenario === "carnet-nettoyage") {
                    dlgNettoyage.ouvrir()
                    return
                }
                suite()
            }
            boite.synchroniserAgendas(true)
        })
        // Création, le nom affiché suivant prénom et nom.
        etapes.push(function() {
            dlgFiche.ouvrirNouveau({})
            dlgFiche.essaiSaisir("prenom", "Marie")
            dlgFiche.essaiSaisir("nom", "Dupont")
            dlgFiche.essaiSaisir("organisation", "Transports, Martin; Accueil")
            dlgFiche.essaiSaisir("fonction", "Secrétaire")
            dlgFiche.essaiAjouter("courriels", "travail", "marie@martin.test")
            dlgFiche.essaiAjouter("telephones", "portable", "06 12 34 56 78")
            var s = dlgFiche.saisie()
            bilan("formulaire : nom affiché « " + s.nomAffiche + " » | " + s.courriels.length + " adresse(s), "
                  + s.telephones.length + " numéro(s) | carnet " + dlgFiche.carnets[0].nom,
                  s.nomAffiche === "Marie Dupont" && s.courriels.length === 1 && s.telephones.length === 1 && s.carnet > 0)
            attenteEcriture = function(ok, message) {
                var f = trouver("dupont")
                bilan("création : " + ok + " « " + message + " » | dialogue ouvert " + dlgFiche.visible + " | trouvée "
                      + f.length + (f.length ? " « " + f[0].nom + " » " + f[0].organisation + " " + f[0].courriels.join(",") + " "
                                    + f[0].telephones.join(",") : ""),
                      ok && !dlgFiche.visible && f.length === 1 && f[0].organisation === "Transports, Martin; Accueil"
                      && f[0].telephones[0] === "06 12 34 56 78")
                id = f.length ? f[0].id : 0
                suite()
            }
            dlgFiche.valider()
        })
        // Modification pendant qu'ailleurs on change la même fiche : rien
        // n'est écrasé ; la fiche du serveur revient avec la saisie.
        etapes.push(function() {
            dlgFiche.ouvrirFiche(id)
            var s = dlgFiche.saisie()
            bilan("fiche ouverte : « " + s.nomAffiche + " » " + s.fonction + " | " + s.courriels.map(function(c) { return c.genre + " " + c.valeur + " #" + c.cle }).join(","),
                  dlgFiche.visible && s.nomAffiche === "Marie Dupont" && s.fonction === "Secrétaire" && s.courriels[0].cle === 0)
            console.log("scenario: modification sur le serveur")
            plusTard(2500, function() {
                dlgFiche.essaiSaisir("fonction", "Comptable")
                dlgFiche.essaiSaisir("organisation", "Martin SAS")
                dlgFiche.essaiAjouter("telephones", "fax", "05 53 00 00 00")
                attenteEcriture = function(ok, message, conflit) {
                    var s = dlgFiche.saisie()
                    bilan("conflit : " + ok + " " + conflit + " | dialogue ouvert " + dlgFiche.visible + " | motif « " + dlgFiche.motif
                          + " » | note du serveur « " + s.note + " » | fonction " + s.fonction + " | organisation " + s.organisation,
                          !ok && conflit && dlgFiche.visible && dlgFiche.motif.indexOf("fonction") > 0 && s.note === "ajout serveur"
                          && s.fonction === "Comptable" && s.organisation === "Martin SAS" && s.telephones.length === 2)
                    attenteEcriture = function(ok, message) {
                        var f = fiche(id)
                        bilan("réenregistrée : " + ok + " | " + f.fonction + " | " + f.organisation + " | note « " + f.note + " » | "
                              + f.telephones.map(function(t) { return t.genre + " " + t.valeur }).join(", "),
                              ok && !dlgFiche.visible && f.fonction === "Comptable" && f.organisation === "Martin SAS"
                              && f.note === "ajout serveur" && f.telephones.length === 2)
                        suite()
                    }
                    dlgFiche.valider()
                }
                dlgFiche.valider()
            })
        })
        // Un correspondant ajouté depuis l'en-tête du message.
        etapes.push(function() {
            ouvrirDossier(comptesConnus[0].compte, "INBOX")
            plusTard(2500, function() {
                choisir(0, 0)
                plusTard(800, function() {
                    var c = correspondantsDe(colonneLecture.auteurAffiche.text)
                    var avant = c.length ? boite.ficheDeAdresse(c[0].adresse) : -1
                    bilan("en-tête « " + colonneLecture.auteurAffiche.text.split("  ·  ")[0] + " » : " + JSON.stringify(c) + " | fiche " + avant,
                          c.length === 1 && c[0].adresse === "essai@essai.test" && avant === 0)
                    menuEnTete.ligne = colonneLecture.auteurAffiche.text
                    menuEnTete.popup()
                    var libelle = menuEnTete.count > 0 ? menuEnTete.itemAt(menuEnTete.count - 1).text : ""
                    menuEnTete.close()
                    bilan("menu : « " + libelle + " »", libelle === "Ajouter Essai MMail aux contacts…")
                    dlgFiche.ouvrirNouveau(JSON.parse(boite.ficheCorrespondant(c[0].nom, c[0].adresse)))
                    var s = dlgFiche.saisie()
                    bilan("prérempli : prénom « " + s.prenom + " », nom « " + s.nom + " », " + s.courriels[0].valeur,
                          s.prenom === "Essai" && s.nom === "MMail" && s.courriels[0].valeur === "essai@essai.test")
                    attenteEcriture = function(ok, message) {
                        var apres = boite.ficheDeAdresse("essai@essai.test")
                        menuEnTete.ligne = colonneLecture.auteurAffiche.text
                        menuEnTete.popup()
                        var libelle = menuEnTete.itemAt(menuEnTete.count - 1).text
                        menuEnTete.close()
                        bilan("ajouté : " + ok + " | fiche " + (apres > 0) + " | menu : « " + libelle + " »",
                              ok && apres > 0 && libelle === "Voir Essai MMail dans le carnet d'adresses")
                        id = apres
                        suite()
                    }
                    dlgFiche.valider()
                })
            })
        })
        // Suppression, copie gardée ; carnet en lecture seule : refus.
        etapes.push(function() {
            dlgFiche.ouvrirFiche(id)
            attenteEcriture = function(ok, message) {
                bilan("suppression : " + ok + " « " + message + " » | fiche " + boite.ficheDeAdresse("essai@essai.test"),
                      ok && !dlgFiche.visible && message.indexOf("<copie>") > 0 && boite.ficheDeAdresse("essai@essai.test") === 0)
                attenteEcriture = function(ok, message) {
                    bilan("lecture seule : " + ok + " « " + message + " »", !ok && message.indexOf("lecture seule") > 0)
                    suite()
                }
                boite.supprimerContacts(JSON.stringify([trouver("protegee")[0].id]))
            }
            dlgFiche.supprimer()
        })
        // Fiches vides et doublons.
        etapes.push(function() {
            dlgNettoyage.ouvrir()
            var p = dlgNettoyage.plan()
            var resume = dlgNettoyage.lignes.filter(function(l) { return l.genre === "titre" }).map(function(l) { return l.texte })
            bilan("nettoyage : " + resume.join(" | ") + " | coché : " + p.retirer.length + " à retirer, "
                  + p.fusions.map(function(g) { return g.length }).join("+") + " à fusionner",
                  p.retirer.length === 2 && p.fusions.length === 1 && p.fusions[0].length === 3 && resume.length === 3)
            attenteEcriture = function(ok, message) {
                var d = trouver("doublon")
                var reste = dlgNettoyage.lignes.filter(function(l) { return l.genre === "choix" }).length
                bilan("nettoyé : " + ok + " « " + message + " » | doublon : " + d.length + " fiche, " + (d.length ? d[0].courriels.length : 0)
                      + " adresses, " + (d.length ? d[0].telephones.length : 0) + " numéros | reste à voir " + reste
                      + " | fiches " + trouver("").length,
                      ok && d.length === 1 && d[0].courriels.length === 2 && d[0].telephones.length === 2 && reste === 1
                      && trouver("").length === 4)
                dlgNettoyage.close()
                suite()
            }
            dlgNettoyage.appliquer(p)
        })
        etapes.push(function() {
            console.log("scenario: carnet " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
            Qt.quit()
        })

        var attente = Qt.createQmlObject('import QtQuick; Timer { interval: 500; repeat: true }', fenetre)
        var essais = { n: 0 }
        attente.triggered.connect(function() {
            essais.n += 1
            if (listeComptes().length > 0 && listeComptes()[0].etat === "pret") {
                attente.stop()
                suite()
            } else if (essais.n > 60) {
                attente.stop()
                console.log("scenario: ECHEC carnet : aucun compte prêt")
                Qt.quit()
            }
        })
        attente.start()
    }

    /// Scénario « desabonnement » (0.6.30, banc DESABO=1 : quatre lettres dans
    /// la boîte de réception de un@ et un serveur HTTPS sur localhost qui note
    /// les demandes). « Lettre un clic » : bandeau, demande envoyée après
    /// confirmation seulement, acceptée ; « Lettre par courriel » : rédaction
    /// préparée vers l'adresse de la liste ; « Lettre lien du corps » : lien
    /// repéré dans le corps ; « TR : Lettre transférée » et « Essai 1 » : pas
    /// de bandeau.
    // Scénario « marquer-lu » (0.8.1) : les boutons « Marquer comme lu / non
    // lu » du volet de lecture et de la barre du haut suivent l'état du
    // message choisi, et le basculent.
    function lancerScenarioMarquerLu(essais) {
        var n = essais || 0
        // Le compte connecté et sa boîte de réception rouverte : cette
        // réouverture vide la liste, et la sélection avec elle.
        if ((modeleMessages.count < 1 || boite.compteCourant <= 0) && n < 40) {
            plusTard(1000, function() { lancerScenarioMarquerLu(n + 1) })
            return
        }
        if (n < 40 && essais !== -1) {
            plusTard(6000, function() { lancerScenarioMarquerLu(-1) })
            return
        }
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var volet = function() { return objetNomme(colonneLecture, "boutonMarquerLu") }
        var barre = function() { return objetNomme(fenetre.menuBar, "boutonMarquerLuBarre") }
        choisir(0, 0)
        plusTard(2500, function() {
            console.log("scenario: sélection", JSON.stringify(Object.keys(selection)), "| état", JSON.stringify(fenetre.etatChoix))
            var v = volet(), b = barre()
            bilan("message affiché, marqué lu : boutons « " + (v ? v.text : "?") + " » / « " + (b ? b.text : "?") + " »",
                  modeleMessages.get(0).lu === true && !!v && v.text === qsTr("Marquer comme non lu")
                  && !!b && b.text === qsTr("Marquer comme non lu") && b.enabled)
            v.clicked()
            plusTard(2000, function() {
                bilan("« Marquer comme non lu » : ligne non lue, bouton « " + volet().text + " »",
                      modeleMessages.get(0).lu === false && volet().text === qsTr("Marquer comme lu")
                      && barre().text === qsTr("Marquer comme lu"))
                barre().clicked()
                plusTard(2000, function() {
                    bilan("« Marquer comme lu » de la barre : ligne lue, bouton « " + volet().text + " »",
                          modeleMessages.get(0).lu === true && volet().text === qsTr("Marquer comme non lu"))
                    console.log("scenario: marquer-lu " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
                    Qt.quit()
                })
            })
        })
    }

    function lancerScenarioDesabonnement() {
        var suivi = { echecs: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var bandeau = function() { return objetNomme(colonneLecture, "bandeauDesabonnement") }
        var rang = function(sujet) {
            for (var k = 0; k < modeleMessages.count; ++k)
                if (modeleMessages.get(k).sujet === sujet)
                    return k
            return -1
        }
        // Les messages, dans l'ordre ; chacun dit ce qu'il attend.
        var etapes = [
            { sujet: "Lettre un clic", methode: "un-clic", source: "entete", hote: "localhost" },
            { sujet: "Lettre par courriel", methode: "courriel", source: "entete", hote: "desabo@essai.test" },
            { sujet: "Lettre lien du corps", methode: "lien", source: "corps", hote: "localhost" },
            { sujet: "TR : Lettre transférée", methode: "" },
            { sujet: "Essai 1", methode: "" }
        ]
        var finir = function() {
            console.log("scenario: desabonnement " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
            Qt.quit()
        }
        var suivante = function(n) {
            if (n >= etapes.length) {
                finir()
                return
            }
            var e = etapes[n]
            var i = rang(e.sujet)
            if (i < 0) {
                bilan("« " + e.sujet + " » absent (banc DESABO=1 ?)", false)
                suivante(n + 1)
                return
            }
            choisir(i, 0)
            plusTard(3000, function() {
                var b = bandeau()
                var d = fenetre.enteteCourant.desabonnement || null
                console.log("scenario: « " + e.sujet + " » :", JSON.stringify(d))
                if (e.methode === "") {
                    bilan("« " + e.sujet + " » sans bandeau", d === null && b && !b.visible)
                    suivante(n + 1)
                    return
                }
                bilan("« " + e.sujet + " » : bandeau visible, " + e.methode + " (" + e.source + ") vers " + e.hote,
                      b && b.visible && d && d.methode === e.methode && d.source === e.source && d.hote === e.hote)
                if (!d || !b) {
                    suivante(n + 1)
                    return
                }
                b.dialogue.d = d
                b.dialogue.compte = comptesConnus[0].compte
                if (e.methode === "un-clic") {
                    console.log("scenario: demande de désabonnement (confirmation simulée)")
                    bilan("demande lancée", b.dialogue.executer() === true)
                    plusTard(5000, function() {
                        var r = fenetre.essai.desabonnement
                        bilan("réponse du serveur : " + JSON.stringify(r), r && r.ok && r.hote === "localhost")
                        suivante(n + 1)
                    })
                } else if (e.methode === "courriel") {
                    var r = b.dialogue.executer()
                    plusTard(1500, function() {
                        var a = r ? objetNomme(r, "champA") : null
                        var o = r ? objetNomme(r, "champObjet") : null
                        bilan("rédaction préparée : À « " + (a ? a.text : "?") + " », objet « " + (o ? o.text : "?") + " »",
                              a && a.text === "desabo@essai.test" && o && o.text === "Désabonnement demandé")
                        if (r)
                            r.conteneur.fermerRedaction()
                        suivante(n + 1)
                    })
                } else {
                    // Le navigateur ne s'ouvre pas au banc.
                    bilan("adresse du lien : " + d.url, d.url === "https://localhost/desinscription?u=7&l=2")
                    suivante(n + 1)
                }
            })
        }
        plusTard(8000, function() {
            ouvrirDossier(comptesConnus[0].compte, "INBOX")
            plusTard(4000, function() { suivante(0) })
        })
    }

    /// Scénario « recherche-etroite » : dans la colonne de la liste, le champ
    /// de recherche, ✕, « Non lus », « Objet et expéditeur seulement » (0.6.38)
    /// et la portée ne se chevauchent pas — sur une ligne dans une colonne
    /// large ; sinon les options sous le champ, côte à côte, la portée
    /// dessous, ou une par ligne (0.8.2 et 0.8.3 les mettaient sur une seule
    /// ligne, le champ et la case se recouvraient). Le champ garde une
    /// largeur où taper, la portée au moins 100 points.
    function etapeScenarioRechercheEtroite() {
        var echecs = 0
        var colonnesVues = {}
        var bilan = function(texte, bon) {
            if (!bon)
                ++echecs
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var libelle = function(e) {
            return e.text || e.displayText || e.placeholderText || e.objectName || "?"
        }
        // Éléments visibles d'un conteneur, rangés en lignes (même hauteur à
        // 2 points près), chaque ligne de gauche à droite.
        var lignesDe = function(conteneur) {
            var elements = []
            for (var i = 0; i < conteneur.children.length; ++i) {
                var e = conteneur.children[i]
                if (e.visible && e.width > 0)
                    elements.push(e)
            }
            elements.sort(function(a, b) { return Math.abs(a.y - b.y) > 2 ? a.y - b.y : a.x - b.x })
            var lignes = []
            elements.forEach(function(e) {
                var derniere = lignes.length > 0 ? lignes[lignes.length - 1] : null
                if (derniere && Math.abs(derniere[0].y - e.y) <= 2)
                    derniere.push(e)
                else
                    lignes.push([e])
            })
            return lignes
        }
        var controlerLignes = function(cas, nom, conteneur) {
            lignesDe(conteneur).forEach(function(ligne, r) {
                console.log("scenario: " + cas + " | " + nom + ", ligne " + (r + 1) + " : " + ligne.map(function(e) {
                    return libelle(e) + " (" + Math.round(e.x) + "+" + Math.round(e.width) + ")"
                }).join(", "))
                for (var j = 0; j < ligne.length; ++j) {
                    var el = ligne[j]
                    bilan(cas + " : " + nom + ", ligne " + (r + 1) + ", « " + libelle(el) + " » dans la largeur",
                          el.x >= -0.5 && el.x + el.width <= conteneur.width + 0.5)
                    if (j > 0)
                        bilan(cas + " : " + nom + ", ligne " + (r + 1) + ", « " + libelle(el) + " » après le précédent",
                              el.x >= ligne[j - 1].x + ligne[j - 1].width - 0.5)
                }
            })
        }
        var verifier = function(cas) {
            var bloc = objetNomme(colonneListe, "blocRecherche")
            var largeur = Math.round(colonneListe.width)
            colonnesVues[bloc.deuxLignes ? bloc.colonnesOptions : 0] = true
            bilan(cas + " : colonne de " + largeur + " (seuil " + Math.round(bloc.seuil) + "), deux lignes " + bloc.deuxLignes
                  + ", " + bloc.colonnesOptions + " colonne(s) d'options", bloc.deuxLignes === (largeur < bloc.seuil))
            var options = objetNomme(bloc, "ligneOptionsRecherche")
            // Ordre de lecture : « Non lus », la case de l'objet, la portée.
            var place = function(e) { var p = e.mapToItem(bloc, 0, 0); return Math.round(p.y) * 10000 + Math.round(p.x) }
            bilan(cas + " : « Non lus », « Objet et expéditeur seulement », puis la portée",
                  place(colonneListe.nonLusRecherche) < place(colonneListe.objetSeulRecherche)
                  && place(colonneListe.objetSeulRecherche) < place(colonneListe.porteeRecherche))
            bilan(cas + " : options " + (bloc.deuxLignes ? "sous le champ" : "au bout de sa ligne"),
                  bloc.deuxLignes ? options.parent === bloc : options.parent === objetNomme(bloc, "ligneRecherche"))
            controlerLignes(cas, "champ", objetNomme(bloc, "ligneRecherche"))
            if (bloc.deuxLignes)
                controlerLignes(cas, "options", options)
            bilan(cas + " : options dans la colonne", options.mapToItem(colonneListe, 0, 0).x + options.width <= colonneListe.width + 0.5)
            bilan(cas + " : champ de " + Math.round(colonneListe.champRecherche.width) + " points", colonneListe.champRecherche.width >= 100)
            bilan(cas + " : portée de " + Math.round(colonneListe.porteeRecherche.width) + " points", colonneListe.porteeRecherche.width >= 100)
        }
        var largeurs = [["étroite", 260], ["moyenne", 420], ["assez large", 560], ["large", 1000]]
        var suivante = function(k) {
            if (k >= largeurs.length) {
                // Une ligne, et les trois dispositions sous le champ, vues au
                // moins une fois chacune n'est pas exigé : la largeur atteinte
                // dépend de la fenêtre du banc. Une option par ligne, si.
                console.log("scenario: dispositions vues : " + Object.keys(colonnesVues).join(", "))
                bilan("une option par ligne dans la colonne étroite", colonnesVues[1] === true)
                console.log("scenario: recherche-etroite " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
                Qt.quit()
                return
            }
            reglages.largeurListe = largeurs[k][1]
            plusTard(800, function() {
                verifier(largeurs[k][0])
                suivante(k + 1)
            })
        }
        suivante(0)
    }

    /// Scénario « zoom-lecture » (0.6.29) : le zoom du volet de lecture grossit
    /// le message, pas les boutons. « Devis joint » (banc PIECE=1) affiché, le
    /// zoom passe à 1,5 : le corps et l'objet grossissent d'autant ; le bouton
    /// « Répondre » et le bandeau des pièces jointes gardent leur taille.
    // Scénario « recherche-portee » (0.8.2, 0.8.3), banc LOT1=1 : la portée
    // « Cette boîte (tous dossiers) » ne rend que les messages du compte, de
    // plusieurs dossiers ; « Toutes les boîtes » ajoute ceux de l'autre
    // compte ; « Non lus » écarte le message lu.
    function lancerScenarioRecherchePortee(essais) {
        var n = essais || 0
        var trouve = false
        for (var j = 0; j < modeleMessages.count; ++j)
            if (modeleMessages.get(j).sujet === "Essai 1")
                trouve = true
        if ((!trouve || comptesConnus.length < 2) && n < 40) {
            plusTard(1000, function() { lancerScenarioRecherchePortee(n + 1) })
            return
        }
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var un = boite.compteCourant
        var lignes = function() {
            var l = []
            for (var k = 0; k < modeleMessages.count; ++k)
                l.push(modeleMessages.get(k))
            return l
        }
        var resume = function(l) {
            return l.map(function(r) { return r.sujet + "@" + (r.compte || un) + ":" + (r.chemin || boite.dossierCourant) + (r.lu ? "(lu)" : "") }).join(" | ")
        }
        // « Essai 1 » affiché : il est lu.
        for (var i = 0; i < modeleMessages.count; ++i)
            if (modeleMessages.get(i).sujet === "Essai 1")
                choisir(i, 0)
        plusTard(2500, function() {
            colonneListe.porteeRecherche.currentIndex = 1
            colonneListe.nonLusRecherche.checked = false
            fenetre.lancerRecherche("essai")
            plusTard(5000, function() {
                var l = lignes()
                var chemins = {}
                l.forEach(function(r) { chemins[r.chemin] = true })
                bilan("cette boîte : " + resume(l),
                      l.length > 0 && l.every(function(r) { return r.compte === un }) && Object.keys(chemins).length >= 3)
                colonneListe.porteeRecherche.currentIndex = 2
                fenetre.lancerRecherche("essai")
                plusTard(5000, function() {
                    var l2 = lignes()
                    bilan("toutes les boîtes : " + resume(l2), l2.some(function(r) { return r.compte !== un }))
                    colonneListe.porteeRecherche.currentIndex = 1
                    colonneListe.nonLusRecherche.checked = true
                    fenetre.lancerRecherche("essai")
                    plusTard(5000, function() {
                        var l3 = lignes()
                        bilan("cette boîte, non lus : " + resume(l3),
                              l3.length > 0 && l3.every(function(r) { return !r.lu && r.compte === un })
                              && !l3.some(function(r) { return r.sujet === "Essai 1" })
                              && l3.some(function(r) { return r.sujet === "Essai 2" }))
                        bilan("bandeau : " + fenetre.infoDossier, fenetre.infoDossier.indexOf("tous dossiers") >= 0
                              && fenetre.infoDossier.indexOf("non lus") >= 0)
                        console.log("scenario: recherche-portee " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
                        Qt.quit()
                    })
                })
            })
        })
    }

    // Scénario « collage-signature » (banc : images distantes servies sur
    // https://localhost/, dossier de sortie avec signature-distante.htm et
    // anime.gif) : l'image distante d'une signature importée ou collée est
    // téléchargée, en GIF ; un GIF du presse-papier reste un GIF, dans
    // l'éditeur de signature aussi.
    function lancerScenarioSignatureCollee(essais) {
        var n = essais || 0
        if (comptesConnus.length === 0 && n < 40) {
            plusTard(1000, function() { lancerScenarioSignatureCollee(n + 1) })
            return
        }
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var images = function(h) { return (h.match(/<img[^>]*>/g) || []).join(" ") }
        var gifLocal = /src="file:[^"]*\.gif"/
        var a = comptesConnus[0]
        var importee = boite.importerSignature(a.adresse, identifiantsEssai.sortie + "/signature-distante.htm")
        bilan("import : " + images(importee), gifLocal.test(importee) && importee.indexOf("https://localhost") < 0)
        var collee = boite.rapatrierImagesSignature(a.adresse, '<p>Nom</p><img src="https://localhost/anime.gif" width="80" />')
        bilan("collée : " + images(collee), gifLocal.test(collee))
        pressePapier.deposerGif(identifiantsEssai.sortie + "/anime.gif")
        var gif = pressePapier.enregistrerImage(boite.dossierImagesCollees())
        bilan("GIF du presse-papier : " + gif, /\.gif$/.test(gif))
        dlgIdentite.ouvrir(a.compte)
        plusTard(1500, function() {
            pressePapier.deposerGif(identifiantsEssai.sortie + "/anime.gif")
            var fait = dlgIdentite.collerDansSignature()
            dlgIdentite.garderSignature()
            var sig = dlgIdentite.biblio[dlgIdentite.choisie] || {}
            bilan("éditeur, GIF collé : " + fait + " " + images(sig.html || ""), fait && gifLocal.test(sig.html || ""))
            console.log("scenario: collage-signature " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
            Qt.quit()
        })
    }

    function lancerScenarioZoomLecture() {
        var suivi = { echecs: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var mesurer = function() {
            var b = objetNomme(colonneLecture, "boutonRepondre")
            var p = objetNomme(colonneLecture, "bandeauPieces")
            return { bouton: b ? b.font.pointSize : -1, hauteur: b ? b.height : -1,
                     pieces: p && p.visible ? p.font.pointSize : -1,
                     corps: colonneLecture.vueCorps.font.pointSize,
                     objet: colonneLecture.sujetAffiche.font.pointSize }
        }
        var zoomAvant = reglages.zoomMessage
        plusTard(8000, function() {
            poserZoom("message", 1)
            ouvrirDossier(comptesConnus[0].compte, "INBOX")
            plusTard(4000, function() {
                var i = -1
                for (var k = 0; k < modeleMessages.count; ++k)
                    if (modeleMessages.get(k).sujet === "Devis joint")
                        i = k
                if (i < 0) {
                    bilan("« Devis joint » absent (banc PIECE=1 ?)", false)
                    Qt.quit()
                    return
                }
                choisir(i, 0)
                plusTard(4000, function() {
                    var avant = mesurer()
                    poserZoom("message", 1.5)
                    plusTard(1000, function() {
                        var apres = mesurer()
                        var dire = function(nom) { return nom + " " + avant[nom] + " → " + apres[nom] }
                        var proche = function(a, b) { return Math.abs(a - b) < 0.01 }
                        bilan("boutons : " + dire("bouton") + " pt, " + dire("hauteur") + " px",
                              avant.bouton > 0 && apres.bouton === avant.bouton && apres.hauteur === avant.hauteur)
                        bilan("pièces jointes : " + dire("pieces") + " pt", avant.pieces > 0 && apres.pieces === avant.pieces)
                        bilan("corps : " + dire("corps") + " pt", proche(apres.corps, avant.corps * 1.5))
                        bilan("objet : " + dire("objet") + " pt", proche(apres.objet, avant.objet * 1.5))
                        poserZoom("message", zoomAvant)
                        console.log("scenario: zoom-lecture " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                        if (identifiantsEssai.dossier !== "garder")
                            Qt.quit()
                    })
                })
            })
        })
    }

    /// Scénario « barre-redaction » (retour de Manu du 03/11 sous Android :
    /// boutons de la rédaction chevauchés, plus moyen d'envoyer ni de fermer) :
    /// les boutons visibles de la barre du haut ne se chevauchent pas, gardent
    /// leur largeur et restent dans la barre ; resserrée, elle montre Envoyer,
    /// Joindre…, « ⋮ » et Fermer, en icônes. Au banc : MMAIL_TACTILE=1, MMAIL_TAILLE=392x800
    /// (un téléphone) ; sans eux, la barre complète. MMAIL_DOSSIER « garder » :
    /// MMail reste ouvert, pour une capture.
    function etapeScenarioBarreRedaction() {
        var r = ouvrirRedaction(comptesConnus.length > 0 ? comptesConnus[0].compte : 0)
        plusTard(1500, function() {
            var echecs = 0
            var bilan = function(texte, bon) {
                if (!bon)
                    ++echecs
                console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
            }
            var barre = objetNomme(r, "barreRedaction")
            bilan("barre trouvée, largeur " + (barre ? Math.round(barre.width) : 0), !!barre)
            var boutons = []
            for (var i = 0; barre && i < barre.children.length; ++i) {
                var b = barre.children[i]
                if (b.visible && typeof b.clicked === "function")
                    boutons.push(b)
            }
            boutons.sort(function(a, b) { return a.x - b.x })
            console.log("scenario: tactile", fenetre.tactile, "| resserrée", r.barreCompacte, "| boutons",
                        boutons.map(function(b) { return b.text + " (" + Math.round(b.x) + "+" + Math.round(b.width) + ")" }).join(", "))
            for (var j = 0; j < boutons.length; ++j) {
                var c = boutons[j]
                bilan("« " + c.text + " » à sa largeur", c.width >= c.implicitWidth - 0.5)
                bilan("« " + c.text + " » dans la barre", c.x >= -0.5 && c.x + c.width <= barre.width + 0.5)
                if (j > 0)
                    bilan("« " + c.text + " » après « " + boutons[j - 1].text + " »", c.x >= boutons[j - 1].x + boutons[j - 1].width - 0.5)
            }
            var noms = boutons.map(function(b) { return b.objectName })
            bilan("Envoyer et Fermer visibles", noms.indexOf("boutonEnvoyer") >= 0 && noms.indexOf("boutonFermer") >= 0)
            if (fenetre.tactile) {
                bilan("resserrée sous le tactile, 4 boutons", r.barreCompacte && boutons.length === 4)
                bilan("en icônes", boutons.every(function(b) { return !!b.dessin }))
            }
            console.log("scenario: barre-redaction " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
            if (identifiantsEssai.dossier !== "garder")
                Qt.quit()
        })
    }

    /// Scénario « personnalisation » (0.8.6). MMAIL_DOSSIER :
    /// - « imposer » ou « libre » : le dossier de MMAIL_PERSONNALISATION donne
    ///   une apparence au nom de l'organisation, ses couleurs, son logo en
    ///   tête du bandeau, et (« imposer ») le nom du logiciel. « imposer » :
    ///   elle est la seule au choix, et l'emporte sur l'apparence déjà choisie
    ///   (le banc en range une) ; « libre » : elle s'ajoute, le choix rangé
    ///   reste ;
    /// - « importer:<dossier> » (Android, MMAIL_TACTILE=1) : sans dossier du
    ///   poste, la personnalisation de <dossier> est importée dans le profil,
    ///   puis retirée, puis importée de nouveau ;
    /// - « profil » : celle importée dans le profil est reprise au démarrage,
    ///   et l'apparence de l'organisation, choisie à l'importation (au banc,
    ///   THEME=organisation), reste en vigueur.
    function etapeScenarioPersonnalisation() {
        var echecs = 0
        var bilan = function(texte, bon) {
            if (!bon)
                ++echecs
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        var finir = function() {
            console.log("scenario: personnalisation " + (echecs === 0 ? "OK" : "ECHEC (" + echecs + ")"))
            if (identifiantsEssai.sortie === "" || identifiantsEssai.sortie === "file:")
                Qt.quit()
        }
        var mode = identifiantsEssai.dossier
        var p = fenetre.personnalisation
        console.log("scenario: personnalisation", JSON.stringify(p))
        var verifierOrganisation = function(libelle) {
            var org = fenetre.jeuxApparence["organisation"]
            bilan("apparence de l'organisation, « " + (org ? org.libelle : "") + " »", !!org && org.libelle === libelle)
            bilan("bandeau " + (org ? org.bandeau : ""), !!org && org.bandeau === "#1F4E79")
            bilan("couleur invalide écartée", !!org && org.surbrillance !== "rouge")
        }

        if (mode.indexOf("importer:") === 0) {
            var source = mode.substring(9)
            var adresse = function(nom) { return "file://" + source + "/" + nom }
            bilan("au départ, rien du poste : " + JSON.stringify(p), !p.origine && fenetre.nomLogiciel === "MMail")
            bilan("importation proposée", fenetre.personnalisationImportable)
            bilan("un fichier quelconque est refusé",
                  !fenetre.importerPersonnalisation([adresse("quelconque.txt")]) && !fenetre.personnalisation.origine)
            bilan("un logo seul est refusé",
                  !fenetre.importerPersonnalisation([adresse("logo.png")]) && !fenetre.personnalisation.origine)
            bilan("importée (paramètres et logo)",
                  fenetre.importerPersonnalisation([adresse("logo.png"), adresse("personnalisation.json")]))
            p = fenetre.personnalisation
            console.log("scenario: importée", JSON.stringify(p))
            bilan("rangée dans le profil : " + p.dossier,
                  p.origine === "profil" && String(p.dossier).indexOf("/profil/personnalisation") > 0)
            bilan("logo rangé : " + p.logo, String(p.logo).indexOf("/profil/personnalisation/logo.png") > 0)
            verifierOrganisation("Exemple essai")
            bilan("en vigueur (" + fenetre.apparence + ")", fenetre.apparence === "organisation")
            bilan("nom du logiciel : " + fenetre.nomLogiciel, fenetre.nomLogiciel === "Exemple Mail")
            bilan("titre : " + fenetre.title, fenetre.title.indexOf("Exemple Mail") === 0)
            bilan("retrait proposé", fenetre.personnalisation.origine === "profil")
            bilan("retirée", fenetre.retirerPersonnalisation())
            bilan("plus rien : " + JSON.stringify(fenetre.personnalisation),
                  !fenetre.personnalisation.origine && fenetre.jeuxApparence["organisation"] === undefined)
            bilan("MMail reprend ses couleurs (" + fenetre.apparence + ") et son nom (" + fenetre.nomLogiciel + ")",
                  fenetre.apparence === "mmedia" && fenetre.nomLogiciel === "MMail")
            // De nouveau, sans logo : l'importation remplace la précédente.
            bilan("importée de nouveau, sans logo", fenetre.importerPersonnalisation([adresse("personnalisation.json")]))
            bilan("sans logo : " + fenetre.personnalisation.logo, !fenetre.personnalisation.logo)
            finir()
            return
        }
        if (mode === "profil") {
            bilan("reprise du profil : " + p.dossier, p.origine === "profil")
            verifierOrganisation("Exemple essai")
            bilan("le choix rangé, l'organisation, tient (" + fenetre.apparence + ")", fenetre.apparence === "organisation")
            bilan("nom du logiciel : " + fenetre.nomLogiciel, fenetre.nomLogiciel === "Exemple Mail")
            bilan("retrait proposé", fenetre.personnalisationImportable)
            finir()
            return
        }

        verifierOrganisation("Exemple essai")
        bilan("du poste : " + p.origine, p.origine === "poste")
        bilan("ni importation ni retrait proposés", !fenetre.personnalisationImportable)
        if (mode === "imposer") {
            bilan("imposée : seule au choix " + JSON.stringify(fenetre.nomsApparence),
                  fenetre.nomsApparence.length === 1 && fenetre.nomsApparence[0] === "organisation")
            bilan("imposée : en vigueur (" + fenetre.apparence + ")", fenetre.apparence === "organisation")
            bilan("nom du logiciel : " + fenetre.nomLogiciel, fenetre.nomLogiciel === "Exemple Mail")
            bilan("titre : " + fenetre.title, fenetre.title.indexOf("Exemple Mail") === 0)
        } else {
            bilan("libre : au choix", fenetre.nomsApparence.indexOf("organisation") >= 0 && fenetre.nomsApparence.length > 1)
            bilan("libre : le choix rangé reste (" + fenetre.apparence + ")", fenetre.apparence !== "organisation")
            bilan("sans « logiciel », le nom reste MMail", fenetre.nomLogiciel === "MMail")
            fenetre.choisirApparence("organisation")
        }
        // Le bandeau est la `menuBar` de la fenêtre, hors de son contentItem.
        var signe = objetNomme(fenetre.menuBar, "signeEntete")
        var nom = objetNomme(fenetre.menuBar, "nomEntete")
        plusTard(800, function() {
            bilan("logo en tête : " + (signe ? signe.source : "absent"),
                  !!signe && String(signe.source) === p.logo && signe.visible)
            bilan("nom en tête : " + (nom ? nom.text : "absent"), !!nom && nom.visible && nom.text === fenetre.nomLogiciel)
            bilan("icône retenue", !!p.icone)
            finir()
        })
    }

    /// Premier objet nommé `nom` sous `racine` (objectName), ou null.
    function objetNomme(racine, nom) {
        if (!racine)
            return null
        if (racine.objectName === nom)
            return racine
        var enfants = racine.children || []
        for (var i = 0; i < enfants.length; ++i) {
            var o = objetNomme(enfants[i], nom)
            if (o)
                return o
        }
        return null
    }

    /// Scénario « lot-2 » (0.6.22, banc LOT2=1 : 3 000 messages de plus dans la
    /// boîte de réception, « Réunion de chantier », un faux serveur d'envoi
    /// qui refuse refus@essai.test et répond en 2 s ; MMAIL_ANIMATIONS=0) :
    /// la barre d'état dit compte, dossier et avancement ; l'interrupteur
    /// glisse en 120 ms sous l'apparence MMEDIA, et pas sous « Pomme » quand
    /// le poste coupe les animations ; « Envoyer » ferme la rédaction, qui se
    /// défait une fois le message parti et revient s'il est refusé ; une faute
    /// de frappe dans la recherche trouve quand même.
    function lancerScenarioLot2() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var releve = Qt.createQmlObject('import QtQuick; Timer { interval: 50; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, phrases: [], positions: [], envoi: "", refus: null }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        // Barre d'état et bouton de l'interrupteur, relevés toutes les 50 ms.
        releve.triggered.connect(function() {
            var p = boite.activiteEnCours()
            if (p.length > 0 && suivi.phrases.indexOf(p) < 0 && suivi.phrases.length < 40)
                suivi.phrases.push(p)
        })
        var interrupteur = function() { return objetNomme(fenetre.contentItem.parent, "interrupteurMasques") }
        // Bascule « Dossiers masqués » et relève la position du bouton toutes
        // les 10 ms pendant 400 ms ; `puis` reçoit le nombre de positions
        // intermédiaires vues (aucune : le bouton a sauté).
        var glisser = function(puis) {
            var ie = interrupteur()
            if (!ie) {
                bilan("interrupteur « Dossiers masqués » introuvable", false)
                puis(-1)
                return
            }
            var depart = ie.bouton.x
            var vues = []
            var echantillon = Qt.createQmlObject('import QtQuick; Timer { interval: 10; repeat: true }', fenetre)
            var debut = Date.now()
            echantillon.triggered.connect(function() {
                vues.push(ie.bouton.x)
                if (Date.now() - debut < 400)
                    return
                echantillon.stop()
                var arrivee = ie.bouton.x
                var entre = vues.filter(function(x) {
                    return x > Math.min(depart, arrivee) + 0.5 && x < Math.max(depart, arrivee) - 0.5
                }).length
                reglages.afficherMasques = !reglages.afficherMasques
                puis(arrivee !== depart ? entre : -1)
            })
            reglages.afficherMasques = !reglages.afficherMasques
            echantillon.start()
        }
        var visible = function(r) {
            return r && (fenetre.compact ? r.conteneur.opened : r.conteneur.visible)
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
            } else if (suivi.n === 8) {
                releve.stop()
                console.log("scenario: phrases relevées :\n  " + suivi.phrases.slice(0, 8).join("\n  "))
                bilan("barre d'état : compte et dossier nommés, avancement des en-têtes ou de la mise sur le poste",
                      suivi.phrases.some(function(p) {
                          return p.indexOf(c.adresse + " › Boîte de réception : ") === 0
                              && (p.indexOf("en-têtes ") > 0 || p.indexOf("mise sur le poste") > 0)
                      }))
                bilan("apparence « " + fenetre.apparence + " » : glissement de l'interrupteur " + fenetre.dureeInterrupteur + " ms",
                      fenetre.valeurOu("mouvement", false) !== true && fenetre.dureeInterrupteur === 120)
                glisser(function(entre) {
                    bilan("interrupteur sous « " + fenetre.apparence + " » : " + entre + " positions intermédiaires (il glisse)", entre > 0)
                })
            } else if (suivi.n === 10) {
                suivi.apparenceAvant = fenetre.apparence
                choisirApparence("pomme")
            } else if (suivi.n === 11) {
                bilan("« Pomme », animations coupées par le poste (MMAIL_ANIMATIONS=0) : durée " + fenetre.dureeInterrupteur,
                      animationsSysteme || fenetre.dureeInterrupteur === 0)
                glisser(function(entre) {
                    bilan("interrupteur sous « Pomme » sans animations : " + entre + " positions intermédiaires (il saute)",
                          animationsSysteme ? entre > 0 : entre === 0)
                    choisirApparence(suivi.apparenceAvant)
                })
            } else if (suivi.n === 13) {
                var r = ouvrirRedaction(c.compte)
                r.pret()
                r.remplir({ a: "deux@essai.test", objet: "Lot 2 : envoi", mode: "nouveau", texte: "Parti en fond." })
                suivi.envoi = r.jeton
                r.envoyer(false)
                bilan("« Envoyer » : rédaction fermée aussitôt, " + envoisEnCours + " envoi en cours, barre « " + etatSynchro() + " »",
                      !visible(r) && r.enEnvoi && envoisEnCours === 1 && etatSynchro().indexOf("en cours d'envoi") >= 0)
            } else if (suivi.n === 14) {
                var r2 = ouvrirRedaction(c.compte)
                r2.pret()
                r2.remplir({ a: "refus@essai.test", objet: "Lot 2 : refusé", mode: "nouveau", texte: "Ne partira pas." })
                suivi.refus = r2
                r2.envoyer(false)
                bilan("second envoi : rédaction fermée aussitôt", !visible(r2) && r2.enEnvoi)
            } else if (suivi.n === 16) {
                var r3 = suivi.refus
                bilan("refusé par le serveur : la rédaction revient avec le motif (« " + (r3 ? r3.note : "") + " »)",
                      r3 && visible(r3) && !r3.enEnvoi && r3.noteErreur && r3.note.indexOf("550") >= 0
                      && r3.titre === "Lot 2 : refusé")
                if (r3)
                    r3.conteneur.fermerRedaction()
            } else if (suivi.n === 26) {
                // Parti : la rédaction se défait une fois la copie rangée dans
                // « Éléments envoyés » — après le lot de mise sur le poste en
                // cours, sur ce banc chargé de 3 000 messages.
                bilan("message parti : rédaction défaite, plus d'envoi en cours (" + envoisEnCours + ")",
                      !redactions[suivi.envoi] && envoisEnCours === 0)
                colonneListe.porteeRecherche.currentIndex = 0
                colonneListe.champRecherche.text = "reunoin chantier"
                lancerRecherche()
            } else if (suivi.n === 28) {
                bilan("recherche « reunoin chantier » : « " + (modeleMessages.count > 0 ? modeleMessages.get(0).sujet : "")
                      + " », " + infoDossier + " / " + messageEtat.texte,
                      modeleMessages.count === 1 && modeleMessages.get(0).sujet === "Réunion de chantier"
                      && boite.rechercheApprochee() && infoDossier.indexOf("fautes de frappe tolérées") >= 0
                      && messageEtat.texte.indexOf("fautes de frappe") >= 0)
                colonneListe.champRecherche.text = "chantier"
                lancerRecherche()
            } else if (suivi.n === 30) {
                bilan("recherche exacte « chantier » : rien d'approché", modeleMessages.count === 1 && !boite.rechercheApprochee()
                      && infoDossier.indexOf("fautes de frappe") < 0)
                tic.stop()
                console.log("scenario: lot-2 " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        releve.start()
        tic.start()
    }

    /// Scénario « lot-1 » (0.6.21, banc LOT1=1) : À et Cc dans le volet de
    /// lecture, repliés puis dépliés ; liens posés sur un texte brut et dans un
    /// HTML ; recherche sans accents ; non-lus cumulés d'un dossier puis d'un
    /// compte repliés ; propriétés d'un dossier ; encre du survol du bandeau.
    /// Captures dans MMAIL_SORTIE (lot1-<étape>.png).
    function lancerScenarioLot1() {
        var tic = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; repeat: true }', fenetre)
        var suivi = { n: 0, echecs: 0, attendu: 0 }
        var bilan = function(texte, bon) {
            if (!bon)
                suivi.echecs += 1
            console.log("scenario: " + (bon ? "OK " : "ECHEC ") + texte)
        }
        // La suite seulement une fois l'image prise : la capture est
        // asynchrone, et l'action suivante changerait ce qu'elle montre.
        var capturer = function(item, nom, puis) {
            if (!identifiantsEssai.sortie || !item) {
                if (puis)
                    puis()
                return
            }
            var chemin = lanceur.chemin(identifiantsEssai.sortie + "/lot1-" + nom + ".png")
            item.grabToImage(function(r) {
                r.saveToFile(chemin)
                if (puis)
                    puis()
            })
        }
        var montrer = function(sujet) {
            for (var i = 0; i < modeleMessages.count; ++i)
                if (modeleMessages.get(i).sujet === sujet) {
                    choisir(i, 0)
                    return true
                }
            bilan("message « " + sujet + " » absent (banc LOT1=1 ?)", false)
            return false
        }
        var ligne = function(genre, compte, chemin) {
            for (var i = 0; i < modeleArborescence.count; ++i) {
                var l = modeleArborescence.get(i)
                if (l.genre === genre && l.compte === compte && (genre === "compte" || l.chemin === chemin))
                    return l
            }
            return null
        }
        tic.triggered.connect(function() {
            suivi.n += 1
            var c = comptesConnus[0]
            var html = fenetre.htmlAffiche
            if (suivi.n === 4) {
                ouvrirDossier(c.compte, "INBOX")
                // Survol du bandeau MMEDIA : une encre blanche, transparente.
                var s = fenetre.survolEntete(true)
                bilan("survol du bandeau (" + (fenetre.marque ? "marque" : "sans marque") + ") : rgba("
                      + [s.r, s.g, s.b, s.a].map(function(x) { return Math.round(x * 100) / 100 }).join(", ") + ")",
                      !isNaN(s.r) && (!fenetre.marque || (s.r === 1 && s.g === 1 && s.b === 1)))
            } else if (suivi.n === 7) {
                montrer("Destinataires nombreux")
            } else if (suivi.n === 9) {
                var la = objetNomme(colonneLecture, "destinatairesA")
                var lcc = objetNomme(colonneLecture, "destinatairesCc")
                bilan("À : " + (la ? la.noms.length : "?") + " destinataires, repliés sur 3 + « et 3 autres »",
                      la && la.visible && la.noms.length === 6 && la.repliable && !la.deplie)
                bilan("Cc : " + (lcc ? lcc.liste : "?"), lcc && lcc.visible && lcc.noms.length === 2 && !lcc.repliable)
                capturer(colonneLecture, "entete-replie")
            } else if (suivi.n === 10) {
                objetNomme(colonneLecture, "destinatairesA").deplie = true
            } else if (suivi.n === 11) {
                capturer(colonneLecture, "entete-deplie")
            } else if (suivi.n === 12) {
                montrer("Liens en clair")
            } else if (suivi.n === 14) {
                var la2 = objetNomme(colonneLecture, "destinatairesA")
                bilan("message suivant : À replié de nouveau, Cc caché",
                      la2 && !la2.deplie && !objetNomme(colonneLecture, "destinatairesCc").visible)
                bilan("texte brut : liens posés (" + html.length + " caractères)", fenetre.corpsHtml
                      && html.indexOf('<a href="https://exemple.test/a?b=1&amp;c=2">') >= 0
                      && html.indexOf('<a href="http://www.exemple.test/doc">www.exemple.test/doc</a>.') >= 0
                      && html.indexOf('<a href="mailto:contact@exemple.test">') >= 0
                      && html.indexOf('&lt;<a href="mailto:support@exemple.test">support@exemple.test</a>&gt;') >= 0)
                capturer(colonneLecture, "liens-texte", function() { montrer("Liens HTML") })
            } else if (suivi.n === 16) {
                bilan("HTML : liens posés sur le texte, lien sans protocole complété", fenetre.corpsHtml
                      && html.indexOf('<a href="http://www.exemple.test">www.exemple.test</a>') >= 0
                      && html.indexOf('<a href="mailto:jean@exemple.test">') >= 0
                      && html.indexOf('<a href="http://www.lien.test">') >= 0
                      && html.split('href="https://exemple.test/x"').length === 2)
                capturer(colonneLecture, "liens-html", function() { montrer("Essai 1") })
            } else if (suivi.n === 18) {
                bilan("texte brut sans adresse : reste du texte simple", !fenetre.corpsHtml)
                colonneListe.porteeRecherche.currentIndex = 0
                colonneListe.champRecherche.text = "reunion EQUIPE"
                lancerRecherche()
            } else if (suivi.n === 20) {
                bilan("recherche « reunion EQUIPE » : « " + (modeleMessages.count > 0 ? modeleMessages.get(0).sujet : "") + " »",
                      modeleMessages.count === 1 && modeleMessages.get(0).sujet === "Réunion d'équipe")
                capturer(colonneListe, "recherche", function() {
                    colonneListe.champRecherche.text = ""
                    colonneListe.champRecherche.textEdited()
                    boite.replierDossier(c.compte, "Essais", true)
                })
            } else if (suivi.n === 22) {
                var e = ligne("dossier", c.compte, "Essais")
                bilan("« Essais » replié : " + (e ? e.nonLus + " non lu, +" + e.nonLusSous + " dans ses sous-dossiers" : "absent"),
                      e && e.replie && e.nonLus === 1 && e.nonLusSous === 2)
                capturer(colonneArborescence, "arborescence-dossier-replie",
                         function() { boite.replierDossier(c.compte, "Essais", false) })
            } else if (suivi.n === 24) {
                // Non-lus attendus : ceux des dossiers du compte, ni corbeille
                // ni indésirables.
                suivi.attendu = 0
                for (var i = 0; i < modeleArborescence.count; ++i) {
                    var l = modeleArborescence.get(i)
                    if (l.genre === "dossier" && l.compte === c.compte && l.selectionnable
                            && l.role !== "Trash" && l.role !== "Junk")
                        suivi.attendu += l.nonLus
                }
                boite.replierCompte(c.compte, true)
            } else if (suivi.n === 26) {
                var co = ligne("compte", c.compte, "")
                bilan("compte replié : " + (co ? co.nonLusSous : "?") + " non lus (attendu " + suivi.attendu + ")",
                      co && co.replie && co.nonLusSous === suivi.attendu && suivi.attendu > 0)
                capturer(colonneArborescence, "arborescence-compte-replie",
                         function() { boite.replierCompte(c.compte, false) })
            } else if (suivi.n === 28) {
                dlgProprietesDossier.ouvrir(c.compte, "INBOX")
            } else if (suivi.n === 29) {
                var p = dlgProprietesDossier
                bilan("propriétés de la boîte de réception : " + JSON.stringify(p.index) + ", " + p.texteSynchro,
                      p.visible && p.index.messages >= 7 && p.ligne.libelle === "Boîte de réception"
                      && p.texteSynchro.indexOf("pas encore") < 0)
                capturer(fenetre.contentItem.parent, "proprietes")
            } else if (suivi.n === 30) {
                dlgProprietesDossier.close()
                tic.stop()
                console.log("scenario: lot-1 " + (suivi.echecs === 0 ? "OK" : "ECHEC (" + suivi.echecs + ")"))
                Qt.quit()
            }
        })
        tic.start()
    }
}
