// SPDX-License-Identifier: GPL-3.0-or-later
// Menu « Réglages » de la barre de menus (0.6.20) : ce qui était dans
// « Comptes » sans être la gestion des comptes — nom et signature, filtres,
// carnet d'adresses, vidage des corbeilles, sauvegarde et restauration de la
// configuration. Demande de Manu du 26/10 : « Comptes » ne garde qu'ajouter,
// modifier, retirer.
//
// Les noms employés sans être déclarés ici (fenetre, boite, reglages…) sont
// ceux de Main.qml, où « MenuReglages { id: menuReglages } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Menu {
    id: menuReglages
    title: qsTr("Réglages")
    onAboutToShow: width = fenetre.largeurMenu(menuReglages)
    MenuItem {
        text: qsTr("Nom et signature…")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: dlgIdentite.ouvrir(boite.compteCourant)
    }
    MenuItem {
        text: qsTr("Filtres du courrier…")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: dlgFiltres.ouvrir(null)
    }
    MenuItem {
        text: qsTr("Expéditeurs de confiance…")
        onTriggered: dlgExpediteursConfiance.open()
    }
    // Messagerie que Windows sert aux autres logiciels (MAPI) : MMail,
    // Outlook… Windows seulement.
    MenuItem {
        text: qsTr("Messagerie par défaut…")
        visible: Qt.platform.os === "windows"
        height: visible ? implicitHeight : 0
        onTriggered: dlgMessagerie.open()
    }
    // Correcteur d'orthographe de la rédaction (0.6.36).
    MenuItem {
        text: qsTr("Vérifier l'orthographe en tapant")
        checkable: true
        checked: reglagesRedaction.orthographe
        onTriggered: reglagesRedaction.orthographe = checked
    }
    // Android (0.7.0) : MMail fermé, une tâche du système relève la boîte de
    // réception et notifie les messages arrivés (cf. cpp/releve.h).
    MenuItem {
        text: qsTr("Notifier les nouveaux messages, MMail fermé")
        visible: releve.disponible
        height: visible ? implicitHeight : 0
        checkable: true
        checked: releve.actif
        onTriggered: releve.actif = checked
    }
    MenuSeparator {}
    MenuItem {
        text: qsTr("Carnet d'adresses…")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: dlgCarnet.open()
    }
    MenuItem {
        text: qsTr("Nouveau contact…")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: dlgFiche.ouvrirNouveau({})
    }
    MenuItem {
        text: qsTr("Fiches vides et doublons…")
        enabled: fenetre.comptesConnus.length > 0 && !boite.contactsOccupe
        onTriggered: dlgNettoyage.ouvrir()
    }
    MenuSeparator {}
    MenuItem {
        text: qsTr("Vider les corbeilles et indésirables…")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: dlgViderCorbeilles.open()
    }
    MenuSeparator {}
    MenuItem {
        text: reglagesSauvegarde.date > 0
              ? qsTr("Sauvegarder la configuration (dernière : %1)").arg(fenetre.dateLongue(reglagesSauvegarde.date))
              : qsTr("Sauvegarder la configuration")
        enabled: fenetre.comptesConnus.length > 0
        onTriggered: fenetre.sauvegarderConfiguration(true)
    }
    MenuItem {
        text: qsTr("Restaurer une configuration sauvegardée…")
        enabled: fenetre.comptesConnus.length > 0 && !fenetre.rechercheSauvegarde
        onTriggered: fenetre.chercherSauvegardes(true)
    }
}
