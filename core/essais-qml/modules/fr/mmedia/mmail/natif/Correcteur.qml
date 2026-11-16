// SPDX-License-Identifier: GPL-3.0-or-later
// Remplaçant du correcteur natif (cpp/correcteur.h) : aucun mot souligné.
import QtQuick

QtObject {
    property var document: null
    property var orthographe: null
    property bool actif: false
    property int curseur: 0
    property var zones: []
    function rafraichir() {}
}
