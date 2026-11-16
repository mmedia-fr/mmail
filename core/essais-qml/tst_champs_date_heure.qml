// SPDX-License-Identifier: GPL-3.0-or-later
// Épreuves des champs de date et d'heure de la fenêtre d'événement
// (ChampDate.qml, ChampHeure.qml, mmail#175) : lecture souple, remise en forme
// en sortant du champ, flèches haut et bas, molette, liste des heures,
// calendrier. Jour de référence des dates sans année : le 16/11/2026.
import QtQuick
import QtQuick.Controls
import QtTest
import "../qml" as Interface

Item {
    id: racine
    width: 400
    height: 400

    Interface.ChampHeure {
        id: heure
        x: 10
        y: 10
        width: 100
    }
    Interface.ChampDate {
        id: date
        x: 10
        y: 60
        width: 150
        reference: new Date(2026, 10, 16)
    }
    TextField {
        id: ailleurs
        x: 10
        y: 120
        width: 100
    }
    SignalSpy {
        id: espionHeure
        target: heure
        signalName: "valeurChangee"
    }
    SignalSpy {
        id: espionDate
        target: date
        signalName: "valeurChangee"
    }
    SignalSpy {
        id: espionCalendrier
        target: date
        signalName: "calendrierDemande"
    }

    TestCase {
        name: "ChampsDateHeure"
        when: windowShown

        function enfant(nom, depuis) {
            for (var i = 0; i < depuis.children.length; ++i) {
                var e = depuis.children[i]
                if (e.objectName === nom)
                    return e
                var trouve = enfant(nom, e)
                if (trouve)
                    return trouve
            }
            return null
        }
        // Le focus d'abord : un champ qui le perd envoie `valeurChangee`.
        function init() {
            ailleurs.forceActiveFocus()
            espionHeure.clear()
            espionDate.clear()
            espionCalendrier.clear()
        }

        function test_lecture_heure_data() {
            return [
                { tag: "13:56", tape: "13:56", attendu: "13:56" },
                { tag: "13,56", tape: "13,56", attendu: "13:56" },
                { tag: "13.56", tape: "13.56", attendu: "13:56" },
                { tag: "13h56", tape: "13h56", attendu: "13:56" },
                { tag: "13H56", tape: "13H56", attendu: "13:56" },
                { tag: "13 h 56", tape: " 13 h 56 ", attendu: "13:56" },
                { tag: "13 56", tape: "13 56", attendu: "13:56" },
                { tag: "13-56", tape: "13-56", attendu: "13:56" },
                { tag: "1356", tape: "1356", attendu: "13:56" },
                { tag: "956", tape: "956", attendu: "09:56" },
                { tag: "9h56", tape: "9h56", attendu: "09:56" },
                { tag: "13h5", tape: "13h5", attendu: "13:05" },
                { tag: "13h", tape: "13h", attendu: "13:00" },
                { tag: "13", tape: "13", attendu: "13:00" },
                { tag: "9", tape: "9", attendu: "09:00" },
                { tag: "0", tape: "0", attendu: "00:00" },
                { tag: "2400", tape: "2400", attendu: "" },
                { tag: "25:00", tape: "25:00", attendu: "" },
                { tag: "13:60", tape: "13:60", attendu: "" },
                { tag: "13:56:00", tape: "13:56:00", attendu: "" },
                { tag: "12345", tape: "12345", attendu: "" },
                { tag: "midi", tape: "midi", attendu: "" },
                { tag: "vide", tape: "", attendu: "" }
            ]
        }
        function test_lecture_heure(d) {
            compare(heure.lire(d.tape), d.attendu)
        }

        function test_lecture_date_data() {
            return [
                { tag: "17/11/2026", tape: "17/11/2026", attendu: "17/11/2026" },
                { tag: "17/11/26", tape: "17/11/26", attendu: "17/11/2026" },
                { tag: "17-11-2026", tape: "17-11-2026", attendu: "17/11/2026" },
                { tag: "17.11.2026", tape: "17.11.2026", attendu: "17/11/2026" },
                { tag: "17 11 2026", tape: "17 11 2026", attendu: "17/11/2026" },
                { tag: "171126", tape: "171126", attendu: "17/11/2026" },
                { tag: "17112026", tape: "17112026", attendu: "17/11/2026" },
                { tag: "17/11", tape: "17/11", attendu: "17/11/2026" },
                { tag: "17 11", tape: "17 11", attendu: "17/11/2026" },
                { tag: "1711", tape: "1711", attendu: "17/11/2026" },
                { tag: "17", tape: "17", attendu: "17/11/2026" },
                { tag: "7/1/2027", tape: "7/1/2027", attendu: "07/01/2027" },
                { tag: "3112", tape: "3112", attendu: "31/12/2026" },
                // Passée de plus de six mois : l'année suivante.
                { tag: "0501", tape: "0501", attendu: "05/01/2027" },
                { tag: "0105", tape: "0105", attendu: "01/05/2027" },
                // Passée de moins de six mois : l'année en cours.
                { tag: "0106", tape: "0106", attendu: "01/06/2026" },
                { tag: "31/02/2026", tape: "31/02/2026", attendu: "" },
                { tag: "32/01", tape: "32/01", attendu: "" },
                { tag: "17/13", tape: "17/13", attendu: "" },
                { tag: "00/11", tape: "00/11", attendu: "" },
                { tag: "123", tape: "123", attendu: "" },
                { tag: "17/11/202", tape: "17/11/202", attendu: "" },
                { tag: "demain", tape: "demain", attendu: "" },
                { tag: "vide", tape: "", attendu: "" }
            ]
        }
        function test_lecture_date(d) {
            compare(date.lire(d.tape), d.attendu)
        }

        // En sortant du champ : remis en forme s'il se lit, laissé tel quel
        // sinon ; `valeurChangee` part dans les deux cas.
        function test_remise_en_forme() {
            heure.text = "1356"
            heure.forceActiveFocus()
            ailleurs.forceActiveFocus()
            compare(heure.text, "13:56")
            verify(espionHeure.count >= 1)
            heure.text = "abc"
            heure.forceActiveFocus()
            ailleurs.forceActiveFocus()
            compare(heure.text, "abc")
            date.text = "1711"
            date.forceActiveFocus()
            ailleurs.forceActiveFocus()
            compare(date.text, "17/11/2026")
            verify(espionDate.count >= 1)
        }

        function test_fleches_heure() {
            heure.text = "13:56"
            heure.forceActiveFocus()
            keyClick(Qt.Key_Up)
            compare(heure.text, "14:00")
            keyClick(Qt.Key_Up)
            compare(heure.text, "14:15")
            keyClick(Qt.Key_Down)
            compare(heure.text, "14:00")
            keyClick(Qt.Key_Down)
            compare(heure.text, "13:45")
            compare(espionHeure.count, 4)
            // Ni avant minuit, ni après 23:45.
            heure.text = "23:50"
            keyClick(Qt.Key_Up)
            compare(heure.text, "23:50")
            heure.text = "00:00"
            keyClick(Qt.Key_Down)
            compare(heure.text, "00:00")
            // Sans heure lisible, rien.
            heure.text = "abc"
            keyClick(Qt.Key_Up)
            compare(heure.text, "abc")
        }

        function test_fleches_date() {
            date.text = "30/11/2026"
            date.forceActiveFocus()
            keyClick(Qt.Key_Up)
            compare(date.text, "01/12/2026")
            keyClick(Qt.Key_Down)
            keyClick(Qt.Key_Down)
            compare(date.text, "29/11/2026")
            compare(espionDate.count, 3)
        }

        // Un cran de molette (120) : un pas ; un pavé tactile en fractions.
        function test_molette() {
            heure.text = "10:00"
            mouseWheel(heure, 20, heure.height / 2, 0, 120)
            compare(heure.text, "10:15")
            mouseWheel(heure, 20, heure.height / 2, 0, -120)
            compare(heure.text, "10:00")
            mouseWheel(heure, 20, heure.height / 2, 0, 60)
            compare(heure.text, "10:00")
            mouseWheel(heure, 20, heure.height / 2, 0, 60)
            compare(heure.text, "10:15")
            date.text = "17/11/2026"
            mouseWheel(date, 20, date.height / 2, 0, 120)
            compare(date.text, "18/11/2026")
            mouseWheel(date, 20, date.height / 2, 0, -240)
            compare(date.text, "16/11/2026")
        }

        function test_liste_heures() {
            heure.text = "10:20"
            var bouton = enfant("boutonHeures", heure)
            verify(bouton !== null)
            mouseClick(bouton)
            tryVerify(function() { return heure.liste.opened })
            heure.choisirQuart(38)
            compare(heure.text, "09:30")
            tryVerify(function() { return !heure.liste.opened })
            verify(espionHeure.count >= 1)
        }

        function test_calendrier() {
            var bouton = enfant("boutonCalendrier", date)
            verify(bouton !== null)
            mouseClick(bouton)
            compare(espionCalendrier.count, 1)
        }
    }
}
