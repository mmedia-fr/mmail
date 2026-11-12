// SPDX-License-Identifier: GPL-3.0-or-later
// Boutons et interrupteurs du courrier : sur la barre (« barre »), ou
// sur la bande dessous (« etages »).
//
// Composant en ligne de Main.qml jusqu'à la 0.6.43 : sorti pour être
// éprouvé seul (core/essais-qml), un composant en ligne n'étant visible que
// du sien. Les noms employés sans être déclarés ici (fenetre, boite,
// reglages…) sont ceux de Main.qml.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import fr.mmedia.mmail

// Barre compacte (11/11, choix du mainteneur) : pictogrammes encadrés pour les
// actions évidentes, noms courts pour celles propres à MMail, nom complet et
// raccourci en infobulle.
RowLayout {
    id: ae
    property bool sombre: fenetre.marque !== null
    spacing: 6
    BoutonOutil {
        sombre: ae.sombre
        principal: true
        objectName: "boutonNouveau"
        dessin: fenetre.dessinerPlus
        iconeSeule: true
        text: qsTr("Nouveau message")
        infobulle: qsTr("Nouveau message (Ctrl+N)")
        onClicked: fenetre.rediger("nouveau")
    }
    BoutonOutil {
        sombre: ae.sombre
        objectName: "boutonActualiser"
        dessin: fenetre.dessinerActualiser
        iconeSeule: true
        text: qsTr("Actualiser")
        infobulle: qsTr("Actualiser (F5)")
        onClicked: fenetre.module === "agenda" ? boite.synchroniserAgendas(true) : boite.actualiser()
    }
    // 0.8.1 : marquer la sélection lue (ou non lue, si tout y est lu), sans
    // passer par le menu.
    BoutonOutil {
        objectName: "boutonMarquerLuBarre"
        sombre: ae.sombre
        visible: fenetre.module !== "agenda"
        enabled: fenetre.etatChoix.nombre > 0
        readonly property bool versLu: fenetre.etatChoix.nonLu || fenetre.etatChoix.nombre === 0
        dessin: versLu ? fenetre.dessinerLu : fenetre.dessinerNonLu
        iconeSeule: true
        text: versLu ? qsTr("Marquer comme lu") : qsTr("Marquer comme non lu")
        infobulle: versLu ? qsTr("Marquer comme lu (Ctrl+Q)") : qsTr("Marquer comme non lu (Ctrl+U)")
        onClicked: fenetre.marquerSelection(fenetre.etatChoix.nonLu)
    }
    SeparateurEntete { sombre: ae.sombre }
    // Boîte de réception unifiée : les réceptions de tous les comptes,
    // avec leurs non-lus (emplacement choisi par le mainteneur le 10/10). Éteinte,
    // la liste revient au dossier.
    InterrupteurEntete {
        objectName: "interrupteurBoites"
        sombre: ae.sombre
        compteur: fenetre.nonLusReceptions
        text: qsTr("BàL")
        checked: fenetre.vueUnifiee
        onClicked: {
            checked = Qt.binding(function() { return fenetre.vueUnifiee })
            if (fenetre.vueUnifiee)
                fenetre.quitterBoitesUnifiees()
            else
                fenetre.ouvrirBoitesUnifiees()
        }
        ToolTip.visible: hovered
        ToolTip.delay: 500
        ToolTip.text: qsTr("Toutes les BàL : les boîtes de réception de tous les comptes, en une liste")
    }
    InterrupteurEntete {
        objectName: "interrupteurMasques"
        sombre: ae.sombre
        visible: (!fenetre.compact || fenetre.vue === 0) && fenetre.module === "courrier"
        checked: reglages.afficherMasques
        text: qsTr("Masqués")
        onToggled: {
            reglages.afficherMasques = checked
            fenetre.rafraichirArborescence()
        }
        ToolTip.visible: hovered
        ToolTip.delay: 500
        ToolTip.text: qsTr("Dossiers masqués : les montrer dans la liste des dossiers")
    }
}
