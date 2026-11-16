// SPDX-License-Identifier: GPL-3.0-or-later
// Remplaçant de la mise en forme native (cpp/mise_en_forme.h) pour les
// épreuves de la fenêtre de rédaction : le corps tenu en deux chaînes, ce que
// l'envoi et l'enregistrement en lisent.
import QtQuick

QtObject {
    property var document: null
    property int curseur: 0
    property int debut: 0
    property int fin: 0
    property bool gras: false
    property bool italique: false
    property bool souligne: false
    property bool puces: false
    property bool numeros: false
    property bool dansTableau: false
    property string police: ""
    property real taille: 0
    property color couleur: "#000000"
    property color surlignage: "transparent"
    property int alignement: 0
    property string contenuTexte: ""
    property string contenuHtml: ""
    signal documentChange()
    signal etatChange()
    signal couleurLienChange()
    function html() { return contenuHtml }
    function texte() { return contenuTexte }
    function poserHtml(h) { contenuHtml = h }
    function poserTexte(t) { contenuTexte = t }
    function inserer(position, t) { contenuTexte = t + contenuTexte }
    function insererHtml(position, h) {}
    function insererImage(url, largeurMax) {}
    function bornerImages(largeurMax) {}
    function enrichi() { return false }
    function imagesAnimees() { return [] }
}
