// SPDX-License-Identifier: GPL-3.0-or-later
//
// Ouvrir un fichier de MMail (une pièce jointe écrite pour être ouverte) avec
// une autre application, depuis cpp/lanceur.h. Android refuse une adresse
// file:// passée hors de l'application (FileUriExposedException, depuis
// Android 7) : c'est ce que faisait Qt.openUrlExternally, et rien ne
// s'ouvrait. Le fichier part donc par une adresse content:// du FileProvider
// déclaré au manifeste (${applicationId}.qtprovider, qui couvre le dossier
// files/ de l'application, où vivent le profil et les pièces jointes), avec
// le droit de lecture pour l'application qui l'ouvre.
package fr.mmedia.mmail;

import android.content.ActivityNotFoundException;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.util.Log;

import androidx.core.content.FileProvider;

import java.io.File;

public final class Ouvrir {
    private static final String TAG = "MMail";

    private Ouvrir() {}

    /**
     * Ouvre le fichier `chemin`, de type MIME `type` (vide : celui que le
     * FileProvider déduit de l'extension). Faux si aucune application ne sait
     * l'ouvrir, ou si le fichier est hors des dossiers du FileProvider.
     */
    public static boolean fichier(Context contexte, String chemin, String type) {
        try {
            Uri adresse = FileProvider.getUriForFile(contexte, contexte.getPackageName() + ".qtprovider", new File(chemin));
            String genre = type == null || type.isEmpty() ? contexte.getContentResolver().getType(adresse) : type;
            Intent vue = new Intent(Intent.ACTION_VIEW);
            if (genre == null || genre.isEmpty())
                vue.setData(adresse);
            else
                vue.setDataAndType(adresse, genre);
            vue.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_ACTIVITY_NEW_TASK);
            contexte.startActivity(vue);
            return true;
        } catch (ActivityNotFoundException e) {
            Log.w(TAG, "aucune application n'ouvre " + chemin + " (" + type + ")");
            return false;
        } catch (IllegalArgumentException | SecurityException e) {
            Log.e(TAG, "ouverture de " + chemin + " impossible : " + e);
            return false;
        }
    }
}
