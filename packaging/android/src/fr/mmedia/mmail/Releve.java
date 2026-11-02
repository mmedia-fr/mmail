// SPDX-License-Identifier: GPL-3.0-or-later
//
// Relève en arrière-plan de MMail sous Android (0.7.0).
//
// MMail quitté, ou passé en arrière-plan — Android gèle alors l'application —,
// cette tâche du système (JobScheduler : toutes les quinze minutes au mieux,
// réseau présent, gardée au redémarrage du téléphone) regarde la boîte de
// réception de chaque compte et montre en notification les messages arrivés.
// Le travail est celui du noyau (core/src/releve.rs), appelé par des méthodes
// natives (cpp/releve_android.cpp) ; ici, la programmation, les secrets du
// coffre (Coffre.java) et les notifications.
//
// MMail au premier plan, la tâche ne fait rien : MMail relève lui-même, et ses
// notifications s'effacent (premierPlan, appelé par cpp/releve.h).
package fr.mmedia.mmail;

import android.Manifest;
import android.app.Activity;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.job.JobInfo;
import android.app.job.JobParameters;
import android.app.job.JobScheduler;
import android.app.job.JobService;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.os.Build;
import android.util.Log;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.nio.charset.StandardCharsets;

public class Releve extends JobService {
    private static final String JOURNAL = "MMail";
    /** Numéro de la tâche auprès de JobScheduler. */
    private static final int TACHE = 4710;
    /** Période demandée : le minimum d'Android, qui la décale à sa guise. */
    private static final long PERIODE = 15 * 60 * 1000L;
    private static final String CANAL = "courrier";
    private static final String PREFERENCES = "releve";
    /** Lignes détaillées au plus dans une notification. */
    private static final int LIGNES = 5;

    private static volatile boolean premierPlan;
    private static boolean chargee;

    private static native byte[] comptes(byte[] index);
    private static native byte[] relever(byte[] index, byte[] secrets, byte[] etat);

    /**
     * Programme la tâche pour le profil {@code index} (chemin de index.sqlite),
     * ou la retire ({@code actif} faux). Appelé par MMail à chaque lancement.
     */
    public static void programmer(Context contexte, String index, boolean actif) {
        SharedPreferences p = contexte.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE);
        p.edit().putString("index", index).putBoolean("actif", actif).apply();
        JobScheduler ordonnanceur = contexte.getSystemService(JobScheduler.class);
        if (ordonnanceur == null)
            return;
        if (!actif) {
            ordonnanceur.cancel(TACHE);
            effacer(contexte);
            return;
        }
        if (ordonnanceur.getPendingJob(TACHE) != null)
            return;
        JobInfo tache = new JobInfo.Builder(TACHE, new ComponentName(contexte, Releve.class))
                .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                .setPeriodic(PERIODE)
                .setPersisted(true)
                .build();
        if (ordonnanceur.schedule(tache) != JobScheduler.RESULT_SUCCESS)
            Log.w(JOURNAL, "relève : tâche refusée par le système");
    }

    /** Android 13 et au-delà : notifier demande une permission, accordée par la personne. */
    public static void demanderPermission(Context contexte) {
        if (Build.VERSION.SDK_INT < 33 || !(contexte instanceof Activity))
            return;
        if (contexte.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED)
            return;
        SharedPreferences p = contexte.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE);
        // Demandée une fois : un refus est une réponse, la reposer à chaque
        // lancement serait du harcèlement. Le réglage d'Android reste.
        if (p.getBoolean("permissionDemandee", false))
            return;
        p.edit().putBoolean("permissionDemandee", true).apply();
        ((Activity) contexte).requestPermissions(new String[] {Manifest.permission.POST_NOTIFICATIONS}, TACHE);
    }

    /** MMail au premier plan (ou non) ; au premier plan, ses notifications s'effacent. */
    public static void premierPlan(Context contexte, boolean oui) {
        premierPlan = oui;
        if (oui)
            effacer(contexte);
    }

    private static void effacer(Context contexte) {
        NotificationManager n = contexte.getSystemService(NotificationManager.class);
        if (n != null)
            n.cancelAll();
    }

    @Override
    public boolean onStartJob(JobParameters parametres) {
        if (premierPlan)
            return false;
        new Thread(() -> {
            try {
                releverEtNotifier();
            } catch (Throwable e) {
                Log.w(JOURNAL, "relève : " + e);
            } finally {
                jobFinished(parametres, false);
            }
        }, "MMail-releve").start();
        return true;
    }

    @Override
    public boolean onStopJob(JobParameters parametres) {
        // Le fil finit son tour (chaque compte a ses délais) ; la période
        // suivante viendra d'elle-même.
        return false;
    }

    /**
     * La bibliothèque de MMail : celle que Qt charge au lancement, déjà là si
     * MMail tourne. Sinon, chargée ici, sans que Qt démarre.
     */
    private static synchronized void charger() {
        if (chargee)
            return;
        UnsatisfiedLinkError derniere = null;
        for (String abi : Build.SUPPORTED_ABIS) {
            try {
                System.loadLibrary("mmail_" + abi);
                chargee = true;
                return;
            } catch (UnsatisfiedLinkError e) {
                derniere = e;
            }
        }
        throw derniere != null ? derniere : new UnsatisfiedLinkError("bibliothèque de MMail introuvable");
    }

    private static byte[] octets(String texte) {
        return texte.getBytes(StandardCharsets.UTF_8);
    }

    private static String texte(byte[] octets) {
        return octets == null ? "" : new String(octets, StandardCharsets.UTF_8);
    }

    private void releverEtNotifier() throws Exception {
        SharedPreferences p = getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE);
        String index = p.getString("index", "");
        if (!p.getBoolean("actif", true) || index.isEmpty() || !new File(index).isFile())
            return;
        charger();
        String liste = texte(comptes(octets(index)));
        if (liste.startsWith("{")) {
            Log.w(JOURNAL, "relève : " + new JSONObject(liste).optString("erreur"));
            return;
        }
        JSONArray comptes = new JSONArray(liste);
        if (comptes.length() == 0)
            return;
        Coffre coffre = new Coffre(this);
        JSONObject secrets = new JSONObject();
        for (int i = 0; i < comptes.length(); i++) {
            String cle = comptes.getJSONObject(i).getString("cle");
            String secret = coffre.lire(cle);
            if (secret != null)
                secrets.put(cle, secret);
        }
        String bilanTexte = texte(relever(octets(index), octets(secrets.toString()), octets(p.getString("etat", "{}"))));
        JSONObject bilan = new JSONObject(bilanTexte);
        if (bilan.has("erreur")) {
            Log.w(JOURNAL, "relève : " + bilan.optString("erreur"));
            return;
        }
        p.edit().putString("etat", bilan.getJSONObject("etat").toString()).apply();
        // MMail revenu au premier plan pendant la relève : il montre lui-même.
        if (premierPlan)
            return;
        JSONArray arrivees = bilan.getJSONArray("comptes");
        for (int i = 0; i < arrivees.length(); i++) {
            JSONObject compte = arrivees.getJSONObject(i);
            String echec = compte.optString("echec");
            if (!echec.isEmpty())
                Log.i(JOURNAL, "relève de " + compte.optString("adresse") + " : " + echec);
            else
                notifier(compte);
        }
    }

    private void notifier(JSONObject compte) throws Exception {
        NotificationManager gestionnaire = getSystemService(NotificationManager.class);
        if (gestionnaire == null)
            return;
        if (Build.VERSION.SDK_INT >= 33
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED)
            return;
        gestionnaire.createNotificationChannel(
                new NotificationChannel(CANAL, "Nouveaux messages", NotificationManager.IMPORTANCE_DEFAULT));
        String adresse = compte.getString("adresse");
        int nombre = compte.getInt("nombre");
        JSONArray messages = compte.getJSONArray("messages");
        Notification.Builder n = new Notification.Builder(this, CANAL)
                .setSmallIcon(icone())
                .setAutoCancel(true)
                .setCategory(Notification.CATEGORY_EMAIL)
                .setSubText(adresse)
                .setNumber(nombre)
                .setContentIntent(ouvrirMMail());
        if (nombre == 1 && messages.length() == 1) {
            JSONObject m = messages.getJSONObject(0);
            n.setContentTitle(m.optString("de"))
             .setContentText(m.optString("objet"))
             .setStyle(new Notification.BigTextStyle().bigText(m.optString("objet")));
        } else {
            String titre = nombre + " nouveaux messages";
            Notification.InboxStyle style = new Notification.InboxStyle().setBigContentTitle(titre);
            for (int i = 0; i < messages.length() && i < LIGNES; i++) {
                JSONObject m = messages.getJSONObject(i);
                style.addLine(m.optString("de") + " — " + m.optString("objet"));
            }
            if (nombre > LIGNES)
                style.setSummaryText("et " + (nombre - LIGNES) + " autres");
            JSONObject premier = messages.length() > 0 ? messages.getJSONObject(0) : null;
            n.setContentTitle(titre)
             .setContentText(premier == null ? adresse : premier.optString("de") + " — " + premier.optString("objet"))
             .setStyle(style);
        }
        // Une notification par compte, remplacée à la relève suivante.
        gestionnaire.notify(adresse.hashCode(), n.build());
    }

    /** Icône de la barre d'état (res/drawable/ic_notification.xml), cherchée par son nom. */
    private int icone() {
        int id = getResources().getIdentifier("ic_notification", "drawable", getPackageName());
        return id != 0 ? id : android.R.drawable.ic_dialog_email;
    }

    private PendingIntent ouvrirMMail() {
        Intent ouvrir = getPackageManager().getLaunchIntentForPackage(getPackageName());
        if (ouvrir == null)
            ouvrir = new Intent();
        ouvrir.setFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED);
        return PendingIntent.getActivity(this, 0, ouvrir, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }
}
