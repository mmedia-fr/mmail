// SPDX-License-Identifier: GPL-3.0-or-later
// Menu contextuel d’un message de la liste.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « MenuMessage { id: menuMessage } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Menu {
    id: menuMessage
    font.pointSize: fenetre.tailleColonne("liste")
    // 0.6.14 (demande du mainteneur du 22/10, le menu devenait « un foutoir
    // illisible ») : dix lignes au plus — lu ou non lu et le drapeau en une
    // ligne chacun, qui fait ce qui change quelque chose ; la sélection et
    // les actions rares en sous-menus ; les raccourcis à droite. Onze avec
    // « Catégoriser » (0.6.15).
    property bool unNonLu: false
    property bool unSansDrapeau: false
    onAboutToShow: {
        var etat = fenetre.etatSelection()
        unNonLu = etat.nonLu
        unSansDrapeau = etat.sansDrapeau
        width = fenetre.largeurMenu(menuMessage)
    }
    // Barre compacte (choix du mainteneur, 10/11) : les actions que tout
    // logiciel de messagerie montre en pictogrammes, sur une rangée en tête du
    // menu, comme le menu contextuel de l'Explorateur de Windows 11 ; nom
    // complet et raccourci en infobulle. Le reste en libellés courts : sept
    // lignes au lieu de onze.
    Item {
        objectName: "rangeePictogrammes"
        width: menuMessage.width - menuMessage.leftPadding - menuMessage.rightPadding
        height: rangee.implicitHeight + 8
        implicitWidth: rangee.implicitWidth + 16
        RowLayout {
            id: rangee
            anchors.centerIn: parent
            spacing: 4
            BoutonPictogramme {
                objectName: "menuRepondre"
                visible: !fenetre.dossierDeReprise
                text: qsTr("Répondre")
                infobulle: qsTr("Répondre (Ctrl+R)")
                dessin: fenetre.dessinerRepondre
                onClicked: { menuMessage.close(); fenetre.rediger("repondre") }
            }
            BoutonPictogramme {
                objectName: "menuRepondreTous"
                visible: !fenetre.dossierDeReprise
                text: qsTr("Répondre à tous")
                infobulle: qsTr("Répondre à tous (Ctrl+Maj+R)")
                dessin: fenetre.dessinerRepondreTous
                onClicked: { menuMessage.close(); fenetre.rediger("repondre_tous") }
            }
            BoutonPictogramme {
                objectName: "menuTransferer"
                text: qsTr("Transférer")
                infobulle: qsTr("Transférer (Ctrl+F)")
                dessin: fenetre.dessinerTransferer
                onClicked: { menuMessage.close(); fenetre.rediger("transferer") }
            }
            BoutonPictogramme {
                objectName: "menuSuivi"
                text: menuMessage.unSansDrapeau ? qsTr("Poser un drapeau de suivi") : qsTr("Retirer le drapeau")
                infobulle: text + " (Inser)"
                dessin: fenetre.dessinerSuivi
                onClicked: { menuMessage.close(); fenetre.basculerSuivi(fenetre.uidsChoisis(), menuMessage.unSansDrapeau) }
            }
            BoutonPictogramme {
                objectName: "menuSupprimer"
                text: qsTr("Supprimer")
                infobulle: qsTr("Supprimer (Suppr)")
                dessin: fenetre.dessinerCorbeille
                onClicked: { menuMessage.close(); fenetre.supprimerSelection() }
            }
        }
    }
    EntreeMenu {
        visible: fenetre.dossierDeReprise
        text: qsTr("Reprendre")
        raccourci: qsTr("Ctrl+R")
        onTriggered: fenetre.rediger("brouillon")
    }
    MenuSeparator {}
    // Un message non lu parmi les choisis : « lu » ; sinon « non lu ».
    EntreeMenu {
        text: menuMessage.unNonLu ? qsTr("Marquer lu") : qsTr("Marquer non lu")
        raccourci: menuMessage.unNonLu ? qsTr("Ctrl+Q") : qsTr("Ctrl+U")
        onTriggered: fenetre.marquerSelection(menuMessage.unNonLu)
    }
    // 0.6.15 : les catégories de couleur, comme « Catégoriser » d'Outlook.
    MenuCategories {
        id: menuCategories
        font: menuMessage.font
    }
    MenuSeparator {}
    EntreeMenu {
        text: qsTr("Déplacer…")
        raccourci: qsTr("Ctrl+Maj+V")
        onTriggered: fenetre.ouvrirDeplacer()
    }
    EntreeMenu {
        visible: !fenetre.dossierDeReprise
        text: fenetre.roleCourant === "Junk" ? qsTr("Pas indésirable") : qsTr("Indésirable")
        raccourci: qsTr("Ctrl+Alt+J")
        onTriggered: fenetre.signalerIndesirable()
    }
    // « Annuler la suppression », « Annuler le déplacement » : ce que
    // Ctrl+Z défait, s'il y a quelque chose.
    EntreeMenu {
        visible: boite.annulation.length > 0
        text: boite.annulation
        raccourci: qsTr("Ctrl+Z")
        onTriggered: fenetre.annuler()
    }
    MenuSeparator {}
    // Dans « Toutes les BàL » et la recherche dans toutes les boîtes, un seul
    // message se choisit à la fois.
    Menu {
        id: menuSelection
        title: qsTr("Sélection")
        font: menuMessage.font
        onAboutToShow: width = fenetre.largeurMenu(menuSelection)
        EntreeMenu {
            text: qsTr("Tout sélectionner")
            raccourci: qsTr("Ctrl+A")
            enabled: !fenetre.listeMulti
            onTriggered: fenetre.toutChoisir()
        }
        EntreeMenu {
            text: qsTr("Inverser la sélection")
            enabled: !fenetre.listeMulti
            onTriggered: fenetre.inverserSelection()
        }
    }
    Menu {
        id: menuAutres
        title: qsTr("Autres")
        font: menuMessage.font
        onAboutToShow: width = fenetre.largeurMenu(menuAutres)
        // Le message lui-même en pièce jointe, en .eml nommé d'après son objet
        // (demande d'une utilisatrice, 10/11) : dans un nouveau message, ou dans
        // celui que l'on rédige — comme une pièce jointe reçue (cf. MenuPiece).
        // Ici et non au premier niveau, qui reste à dix lignes (0.6.14).
        // Bureau seulement pour l'instant.
        EntreeMenu {
            visible: !fenetre.tactile
            text: qsTr("Joindre à un nouveau message")
            onTriggered: fenetre.joindreMessages("nouveau")
        }
        EntreeMenu {
            readonly property var cible: fenetre.redactionJoignable(fenetre.redactionActive)
            visible: !fenetre.tactile && cible !== null
            text: cible ? qsTr("Joindre au message en cours « %1 »").arg(cible.objetActuel() || qsTr("sans objet")) : ""
            onTriggered: fenetre.joindreMessages(fenetre.redactionActive)
        }
        MenuSeparator {
            visible: !fenetre.tactile
            height: visible ? implicitHeight : 0
        }
        EntreeMenu {
            text: qsTr("Créer un filtre…")
            onTriggered: fenetre.creerFiltre()
        }
        MenuSeparator {}
        EntreeMenu {
            text: qsTr("Copier l'adresse de l'expéditeur")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: fenetre.copierChamp("adresse")
        }
        EntreeMenu {
            text: qsTr("Copier l'objet")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: fenetre.copierChamp("sujet")
        }
        MenuSeparator {}
        EntreeMenu {
            text: qsTr("Imprimer…")
            raccourci: qsTr("Ctrl+P")
            visible: impression.disponible
            enabled: fenetre.uidCourant > 0
            onTriggered: fenetre.imprimerMessage()
        }
        EntreeMenu {
            text: qsTr("Enregistrer en PDF…")
            visible: Qt.platform.os !== "android"
            enabled: fenetre.uidCourant > 0
            onTriggered: fenetre.enregistrerPdf()
        }
        EntreeMenu {
            text: qsTr("Afficher la source")
            enabled: Object.keys(fenetre.selection).length === 1
            onTriggered: {
                fenetre.sourceVisible = false
                fenetre.basculerSource()
            }
        }
    }
}
