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
// La tâche tourne dans un processus à part (« :releve », cf. le manifeste) :
// la bibliothèque de MMail ne se charge qu'après Qt Core, dont l'initialisation
// JNI retient l'activité et le chargeur de classes du moment — aucune ici. Dans
// le processus de l'application, un MMail ouvert ensuite serait parti sans
// activité. D'où aussi des fichiers, relus à chaque passage, plutôt que des
// préférences, qu'un processus garde en mémoire.
//
// MMail au premier plan, la tâche ne fait rien : MMail relève lui-même. Ses
// notifications s'effacent à chaque reprise de son activité (suivre).
package fr.mmedia.mmail;

import android.Manifest;
import android.app.Activity;
import android.app.ActivityManager;
import android.app.Application;
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
import android.os.Bundle;
import android.util.Log;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;

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

    private static boolean chargee;
    private static boolean suivi;

    private static native byte[] comptes(byte[] index);
    private static native byte[] relever(byte[] index, byte[] secrets, byte[] etat);

    /**
     * Programme la tâche pour le profil {@code index} (chemin de index.sqlite),
     * ou la retire ({@code actif} faux). Appelé par MMail à chaque lancement.
     */
    public static void programmer(Context contexte, String index, boolean actif) {
        try {
            JSONObject reglage = new JSONObject();
            reglage.put("index", index);
            reglage.put("actif", actif);
            ecrire(contexte, "reglage.json", reglage.toString());
        } catch (Exception e) {
            Log.w(JOURNAL, "relève : " + e);
            return;
        }
        JobScheduler ordonnanceur = contexte.getSystemService(JobScheduler.class);
        if (ordonnanceur == null)
            return;
        if (!actif) {
            ordonnanceur.cancel(TACHE);
            effacer(contexte);
            return;
        }
        suivre(contexte);
        if (ordonnanceur.getPendingJob(TACHE) != null)
            return;
        JobInfo tache = new JobInfo.Builder(TACHE, new ComponentName(contexte, Releve.class))
                .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                .setPeriodic(PERIODE)
                .setPersisted(true)
                .build();
        try {
            if (ordonnanceur.schedule(tache) != JobScheduler.RESULT_SUCCESS)
                Log.w(JOURNAL, "relève : tâche refusée par le système");
        } catch (RuntimeException e) {
            Log.w(JOURNAL, "relève : tâche refusée par le système : " + e);
        }
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

    /**
     * MMail au premier plan, ses notifications s'effacent : tout de suite, puis
     * à chaque reprise de son activité. Le cycle de vie d'Android, et non
     * l'état de l'application Qt : MMail rouvert depuis le lanceur après un
     * passage en arrière-plan, l'effacement qui en dépendait ne se faisait
     * pas (constaté sur l'émulateur, 03/11/2026).
     */
    private static synchronized void suivre(Context contexte) {
        effacer(contexte);
        if (suivi || !(contexte.getApplicationContext() instanceof Application))
            return;
        suivi = true;
        ((Application) contexte.getApplicationContext()).registerActivityLifecycleCallbacks(
                new Application.ActivityLifecycleCallbacks() {
                    @Override public void onActivityResumed(Activity activite) { effacer(activite); }
                    @Override public void onActivityCreated(Activity activite, Bundle etat) {}
                    @Override public void onActivityStarted(Activity activite) {}
                    @Override public void onActivityPaused(Activity activite) {}
                    @Override public void onActivityStopped(Activity activite) {}
                    @Override public void onActivitySaveInstanceState(Activity activite, Bundle etat) {}
                    @Override public void onActivityDestroyed(Activity activite) {}
                });
    }

    /** Le processus de l'application est-il au premier plan ? Vu d'ici, un autre processus. */
    private boolean mmailAuPremierPlan() {
        ActivityManager am = getSystemService(ActivityManager.class);
        if (am == null || am.getRunningAppProcesses() == null)
            return false;
        for (ActivityManager.RunningAppProcessInfo p : am.getRunningAppProcesses())
            if (getPackageName().equals(p.processName)
                    && p.importance <= ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND)
                return true;
        return false;
    }

    private static File dossier(Context contexte) {
        File d = new File(contexte.getFilesDir(), "releve");
        d.mkdirs();
        return d;
    }

    /** Écrit un fichier de la tâche d'un bloc (fichier temporaire, puis renommage). */
    private static void ecrire(Context contexte, String nom, String texte) throws Exception {
        File d = dossier(contexte);
        File provisoire = new File(d, nom + ".tmp");
        Files.write(provisoire.toPath(), texte.getBytes(StandardCharsets.UTF_8));
        Files.move(provisoire.toPath(), new File(d, nom).toPath(), StandardCopyOption.REPLACE_EXISTING,
                   StandardCopyOption.ATOMIC_MOVE);
    }

    private static String lire(Context contexte, String nom, String defaut) {
        try {
            return new String(Files.readAllBytes(new File(dossier(contexte), nom).toPath()), StandardCharsets.UTF_8);
        } catch (Exception e) {
            return defaut;
        }
    }

    private static void effacer(Context contexte) {
        NotificationManager n = contexte.getSystemService(NotificationManager.class);
        if (n != null)
            n.cancelAll();
    }

    @Override
    public boolean onStartJob(JobParameters parametres) {
        if (mmailAuPremierPlan())
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
     * La bibliothèque de MMail, sans que l'application Qt démarre. Qt Core
     * d'abord : son initialisation JNI (JNI_OnLoad) donne à QJniObject la
     * machine virtuelle, dont les initialiseurs statiques de la bibliothèque
     * de MMail ont besoin (constantes Java lues par QtKeychain) — sans elle, le
     * processus s'arrête au chargement (constaté sur l'émulateur, 02/11/2026).
     */
    private static synchronized void charger() {
        if (chargee)
            return;
        UnsatisfiedLinkError derniere = null;
        for (String abi : Build.SUPPORTED_ABIS) {
            try {
                System.loadLibrary("Qt6Core_" + abi);
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
        JSONObject reglage = new JSONObject(lire(this, "reglage.json", "{}"));
        String index = reglage.optString("index");
        if (!reglage.optBoolean("actif", true) || index.isEmpty() || !new File(index).isFile())
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
        String bilanTexte = texte(relever(octets(index), octets(secrets.toString()), octets(lire(this, "etat.json", "{}"))));
        JSONObject bilan = new JSONObject(bilanTexte);
        if (bilan.has("erreur")) {
            Log.w(JOURNAL, "relève : " + bilan.optString("erreur"));
            return;
        }
        ecrire(this, "etat.json", bilan.getJSONObject("etat").toString());
        // MMail revenu au premier plan pendant la relève : il montre lui-même.
        if (mmailAuPremierPlan())
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
