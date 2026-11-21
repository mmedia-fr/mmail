// SPDX-License-Identifier: GPL-3.0-or-later
//
// Enregistrer une pièce jointe à l'emplacement choisi dans la fenêtre
// d'enregistrement d'Android, depuis cpp/lanceur.h. Sous Android, cette fenêtre
// (ACTION_CREATE_DOCUMENT, ouverte par le FileDialog de Qt) rend l'adresse
// content:// d'un document déjà créé, vide, chez son fournisseur
// (Téléchargements, stockage du téléphone, Drive…), et non un chemin : le noyau,
// qui écrit dans un fichier, répondait « emplacement d'enregistrement
// invalide », pour toute pièce jointe (signalé le 21/11/2026). La pièce est donc
// écrite dans un dossier de travail de MMail, puis recopiée ici dans le
// document, par le ContentResolver.
package fr.mmedia.mmail;

import android.content.Context;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;
import android.util.Log;

import java.io.FileInputStream;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;

public final class Enregistrer {
    private static final String TAG = "MMail";

    private Enregistrer() {}

    /**
     * Recopie le fichier `source` dans le document `adresse` (content://),
     * dont le contenu est remplacé. Rend une chaîne vide si tout est écrit,
     * sinon la raison de l'échec.
     */
    public static String copier(Context contexte, String source, String adresse) {
        Uri document = Uri.parse(adresse);
        try (InputStream entree = new FileInputStream(source);
             OutputStream sortie = ouvrir(contexte, document)) {
            if (sortie == null)
                return "le fournisseur du document n'a rien ouvert";
            byte[] tampon = new byte[64 * 1024];
            int lus;
            while ((lus = entree.read(tampon)) > 0)
                sortie.write(tampon, 0, lus);
            sortie.flush();
            return "";
        } catch (IOException | SecurityException | IllegalArgumentException e) {
            Log.e(TAG, "enregistrement de " + source + " dans " + adresse + " impossible : " + e);
            return e.getMessage() != null ? e.getMessage() : e.toString();
        }
    }

    /**
     * Nom du document `adresse` tel que son fournisseur l'affiche
     * (« rapport.pdf », ou « rapport (1).pdf » si le nom était pris) ; vide
     * s'il ne le dit pas.
     */
    public static String nom(Context contexte, String adresse) {
        try (Cursor c = contexte.getContentResolver().query(Uri.parse(adresse),
                new String[] { OpenableColumns.DISPLAY_NAME }, null, null, null)) {
            if (c != null && c.moveToFirst() && !c.isNull(0))
                return c.getString(0);
        } catch (RuntimeException e) {
            Log.w(TAG, "nom de " + adresse + " illisible : " + e);
        }
        return "";
    }

    // « wt » remplace le contenu ; un fournisseur qui ne connaît pas « t »
    // refuse ce mode, et « w » sur un document neuf, vide, revient au même.
    private static OutputStream ouvrir(Context contexte, Uri document) throws FileNotFoundException {
        try {
            return contexte.getContentResolver().openOutputStream(document, "wt");
        } catch (FileNotFoundException | IllegalArgumentException e) {
            return contexte.getContentResolver().openOutputStream(document, "w");
        }
    }
}
