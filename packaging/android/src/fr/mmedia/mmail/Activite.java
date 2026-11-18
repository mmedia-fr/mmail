// SPDX-License-Identifier: GPL-3.0-or-later
//
// Activité de MMail sous Android : celle de Qt (QtActivity), avec une seule
// différence, la garde du correctif amont « Android: Skip restoring bundle
// state on process restart » (qtbase 270d59f65 ; QTBUG-136077, QTBUG-135961,
// QTBUG-136497), qui manque à Qt 6.8.3.
//
// Le défaut : Android arrête MMail en arrière-plan, puis la personne le rouvre
// par l'icône. Android recrée l'activité dans un processus neuf et lui rend
// l'état qu'il avait enregistré. Qt 6.8.3 y relit « Started » à vrai
// (QtNative.setStarted) alors que Qt n'a pas encore démarré dans ce processus :
// l'écran reste noir, MMail tourne, et seule une fermeture depuis les
// applications récentes, qui efface cet état, en fait sortir. Signalé sur un
// téléphone Android le 18/11/2026.
//
// La garde : l'état ne se restaure que pour une recréation de l'activité dans le
// même processus, après un changement de configuration — seul cas où Qt tourne
// déjà. getLastNonConfigurationInstance() est alors non nul, puisque
// QtActivityBase.onRetainNonConfigurationInstance rend vrai.
//
// À retirer au passage à une version de Qt qui porte le correctif (6.8.4, 6.9.1
// et au-delà), en remettant au manifeste l'activité de Qt.
package fr.mmedia.mmail;

import android.os.Bundle;
import android.util.Log;

import org.qtproject.qt.android.bindings.QtActivity;

public class Activite extends QtActivity {
    private static final String TAG = "MMail";

    @Override
    protected void onRestoreInstanceState(Bundle etat) {
        if (getLastNonConfigurationInstance() == null) {
            Log.i(TAG, "processus relancé : état de l'activité précédente ignoré (QTBUG-136077)");
            return;
        }
        super.onRestoreInstanceState(etat);
    }
}
