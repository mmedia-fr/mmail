// SPDX-License-Identifier: GPL-3.0-or-later
// Remplaçant de la fenêtre principale (Main.qml, id « fenetre ») pour les
// épreuves des barres d'actions : ce que les composants lisent, et des
// fonctions qui notent ce qu'on leur demande dans `appels` (« nom:argument »).
import QtQuick

QtObject {
    property bool tactile: false
    property bool compact: false
    property int vue: 1
    property string module: "courrier"
    property bool dossierDeReprise: false
    property string roleCourant: ""
    property bool sourceVisible: false
    property var etatChoix: ({ nonLu: false, nombre: 1, sansDrapeau: true })
    property var marque: null
    property color accent: "#0071e3"
    property color encreSurAccent: "#ffffff"
    property color couleurRubrique: "#00636e"
    property color couleurAlerte: "#c0392b"
    property bool anime: false
    property int dureeTeinte: 0
    property int dureeAppui: 0
    property int dureeInterrupteur: 0
    property real tailleBase: 10
    property real tailleArborescence: 10
    property bool enteteEtroite: false
    property bool enteteSerree: false
    property bool libellesBarres: false
    property var palette: ({ window: "#ffffff", base: "#ffffff", windowText: "#000000", buttonText: "#000000",
                             button: "#f0f0f0", highlight: "#3060c0", highlightedText: "#ffffff", text: "#000000" })
    property int nonLusReceptions: 4
    property bool vueUnifiee: false
    property bool listeMulti: false
    property var selection: ({ "12": true })
    property int uidCourant: 12
    property string redactionActive: ""
    property var appels: []
    function noter(nom, argument) {
        appels = appels.concat([nom + (argument === undefined ? "" : ":" + argument)])
    }
    function encreEntete(sombre) { return "#000000" }
    function survolEntete(sombre) { return "#e0e0e0" }
    function tailleColonne(colonne) { return 10 }
    function largeurMenu(menu) {
        var largeur = 0
        for (var i = 0; i < menu.count; ++i) {
            var entree = menu.itemAt(i)
            if (entree)
                largeur = Math.max(largeur, entree.implicitWidth)
        }
        return Math.max(200, largeur + menu.leftPadding + menu.rightPadding)
    }
    function etatSelection() { return { nonLu: etatChoix.nonLu, sansDrapeau: etatChoix.sansDrapeau } }
    function etatCategories() { return { liste: [], porte: {}, refusees: false, aucune: true } }
    function uidsChoisis() { return [12] }
    function redactionJoignable(jeton) { return null }
    // Dessins : tracés pour de bon (le Canvas les appelle), sans effet ici.
    function dessinerRepondre() {}
    function dessinerRepondreTous() {}
    function dessinerTransferer() {}
    function dessinerDeplacer() {}
    function dessinerIndesirable() {}
    function dessinerImprimer() {}
    function dessinerLu() {}
    function dessinerNonLu() {}
    function dessinerCorbeille() {}
    function dessinerAutres() {}
    function dessinerSuivi() {}
    function dessinerPlus() {}
    function dessinerActualiser() {}
    // Actions.
    function rediger(mode) { noter("rediger", mode) }
    function ouvrirDeplacer() { noter("deplacer") }
    function signalerIndesirable() { noter("indesirable") }
    function marquerSelection(lu) { noter("marquer", lu) }
    function supprimerSelection() { noter("supprimer") }
    function basculerSource() { noter("source") }
    function imprimerMessage() { noter("imprimer") }
    function basculerSuivi(uids, suivi) { noter("suivi", suivi) }
    function annuler() { noter("annuler") }
    function creerFiltre() { noter("filtre") }
    function copierChamp(champ) { noter("copier", champ) }
    function enregistrerPdf() {}
    function toutChoisir() {}
    function inverserSelection() {}
    function joindreMessages(cible) { noter("joindre", cible) }
    function basculerCategorie() {}
    function nouvelleCategorie() {}
    function gererCategories() {}
    function effacerCategories() {}
    function ouvrirBoitesUnifiees() { noter("bal", "ouvrir") }
    function quitterBoitesUnifiees() { noter("bal", "quitter") }
    function rafraichirArborescence() { noter("arborescence") }
}
