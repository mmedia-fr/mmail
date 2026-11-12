// SPDX-License-Identifier: GPL-3.0-or-later
// Barre d'actions du message affiché, au bureau comme sous Android (choix du
// mainteneur, 10/11) : sur une ligne, des pictogrammes encadrés pour les
// actions que tout logiciel de messagerie montre ainsi — des boutons plats se
// lisaient mal (01/10) —, un nom court pour ce qui est propre à MMail ; nom
// complet et raccourci en infobulle, à l'appui long au doigt. Elle tenait sur
// une à trois lignes de boutons à libellés.
//
// Ordre (choix du mainteneur, 12/11) : répondre, répondre à tous, transférer |
// lu ou non lu, déplacer | indésirable, supprimer | source.
//
// Au bureau, « Affichage › Boutons des barres › Pictogrammes et texte » met le
// nom à côté de chaque dessin (fenetre.libellesBarres) : d'où un Flow, qui
// passe à la ligne quand le volet est trop étroit. Au doigt, une seule ligne,
// « … » au bord droit.
//
// Les noms employés sans être déclarés ici (fenetre, menuMessage…) sont ceux
// de Main.qml ; les épreuves de core/essais-qml les posent.
import QtQuick
import QtQuick.Controls
import fr.mmedia.mmail

Flow {
    id: barre
    spacing: fenetre.tactile ? 2 : 4
    readonly property bool libelles: fenetre.libellesBarres
    // Ce qu'occupent les boutons visibles, sans le ressort qui pousse « … »
    // au bord : la largeur utile de la barre (cf. les épreuves).
    readonly property real largeurBoutons: {
        var l = 0
        var n = 0
        for (var i = 0; i < barre.children.length; ++i) {
            var e = barre.children[i]
            if (e === ressort || !e.visible)
                continue
            l += e.width
            n += 1
        }
        return n > 0 ? l + barre.spacing * (n - 1) : 0
    }
    // Un brouillon se reprend ; un message reçu reçoit une réponse.
    BoutonAction {
        objectName: "boutonReprendre"
        visible: fenetre.dossierDeReprise
        text: qsTr("Reprendre")
        onClicked: fenetre.rediger("brouillon")
    }
    BoutonPictogramme {
        objectName: "boutonRepondre"
        visible: !fenetre.dossierDeReprise
        avecLibelle: barre.libelles
        text: qsTr("Répondre")
        infobulle: qsTr("Répondre (Ctrl+R)")
        dessin: fenetre.dessinerRepondre
        onClicked: fenetre.rediger("repondre")
    }
    BoutonPictogramme {
        objectName: "boutonRepondreTous"
        visible: !fenetre.dossierDeReprise
        avecLibelle: barre.libelles
        text: qsTr("Répondre à tous")
        infobulle: qsTr("Répondre à tous (Ctrl+Maj+R)")
        dessin: fenetre.dessinerRepondreTous
        onClicked: fenetre.rediger("repondre_tous")
    }
    BoutonPictogramme {
        objectName: "boutonTransferer"
        visible: !fenetre.dossierDeReprise
        avecLibelle: barre.libelles
        text: qsTr("Transférer")
        infobulle: qsTr("Transférer (Ctrl+F)")
        dessin: fenetre.dessinerTransferer
        onClicked: fenetre.rediger("transferer")
    }
    ToolSeparator {
        objectName: "separateurRepondre"
        visible: !fenetre.tactile
        height: boutonSupprimer.height
    }
    // 0.8.1 : lu ou non lu, à côté du message ; Ctrl+Q, Ctrl+U.
    BoutonPictogramme {
        objectName: "boutonMarquerLu"
        avecLibelle: barre.libelles
        text: fenetre.etatChoix.nonLu ? qsTr("Marquer comme lu") : qsTr("Marquer comme non lu")
        infobulle: fenetre.etatChoix.nonLu ? qsTr("Marquer comme lu (Ctrl+Q)") : qsTr("Marquer comme non lu (Ctrl+U)")
        dessin: fenetre.etatChoix.nonLu ? fenetre.dessinerLu : fenetre.dessinerNonLu
        onClicked: fenetre.marquerSelection(fenetre.etatChoix.nonLu)
    }
    BoutonPictogramme {
        objectName: "boutonDeplacer"
        avecLibelle: barre.libelles
        text: qsTr("Déplacer…")
        infobulle: qsTr("Déplacer vers un dossier… (Ctrl+Maj+V)")
        dessin: fenetre.dessinerDeplacer
        onClicked: fenetre.ouvrirDeplacer()
    }
    ToolSeparator {
        objectName: "separateurRanger"
        visible: !fenetre.tactile
        height: boutonSupprimer.height
    }
    // À côté de « Supprimer », au bureau comme sous Android (choix du
    // mainteneur, 12/11) : relégué dans « … » sous Android le 10/11, il n'y
    // était plus qu'une ligne sans dessin. Au doigt, un appui malheureux se
    // rattrape : le message attend 5 s derrière « Annuler »
    // (Main.signalerIndesirable).
    BoutonPictogramme {
        objectName: "boutonIndesirable"
        visible: !fenetre.dossierDeReprise
        avecLibelle: barre.libelles
        text: fenetre.roleCourant === "Junk" ? qsTr("Pas indésirable") : qsTr("Indésirable")
        infobulle: fenetre.roleCourant === "Junk"
                   ? qsTr("Pas indésirable : remettre en boîte de réception ; le filtre du serveur l'apprend (Ctrl+Alt+J)")
                   : qsTr("Courrier indésirable ; le filtre du serveur l'apprend (Ctrl+Alt+J)")
        dessin: fenetre.dessinerIndesirable
        onClicked: fenetre.signalerIndesirable()
    }
    BoutonPictogramme {
        id: boutonSupprimer
        objectName: "boutonSupprimer"
        avecLibelle: barre.libelles
        text: qsTr("Supprimer")
        infobulle: qsTr("Supprimer (Suppr)")
        dessin: fenetre.dessinerCorbeille
        onClicked: fenetre.supprimerSelection()
    }
    ToolSeparator {
        objectName: "separateurSource"
        visible: !fenetre.tactile
        height: boutonSupprimer.height
    }
    // Décision 6 : voir le message brut, en-têtes compris, en un geste.
    // Propre à MMail : un nom court plutôt qu'un dessin. Au doigt, dans
    // « … › Autres » (choix du mainteneur, 12/11) : la place va à
    // « Indésirable », et la barre tient sur un téléphone de 360 points.
    BoutonPictogramme {
        objectName: "boutonSource"
        visible: !fenetre.tactile
        text: fenetre.sourceVisible ? qsTr("Message") : qsTr("Source")
        court: text
        infobulle: fenetre.sourceVisible ? qsTr("Revenir au message") : qsTr("Afficher la source, en-têtes compris")
        onClicked: fenetre.basculerSource()
    }
    // Au doigt, le ressort pousse « … » au bord droit, comme le faisait
    // l'espace extensible de la barre en ligne.
    Item {
        id: ressort
        visible: fenetre.tactile
        width: Math.max(0, Math.floor(barre.width - barre.largeurBoutons - barre.spacing) - 1)
        height: 1
    }
    // Vue tactile : le reste des actions du message, le menu de l'appui long
    // sur la ligne.
    BoutonPictogramme {
        id: boutonAutresActions
        objectName: "boutonAutresActions"
        visible: fenetre.tactile
        text: qsTr("Autres actions")
        dessin: fenetre.dessinerAutres
        onClicked: menuMessage.popup(boutonAutresActions, 0, boutonAutresActions.height)
    }
}
