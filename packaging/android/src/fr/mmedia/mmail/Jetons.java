// SPDX-License-Identifier: GPL-3.0-or-later
//
// Comptes Microsoft et Google connectés par Android (0.7.0).
//
// Sous Android, ni la page de connexion intégrée (Microsoft : accès
// conditionnel, appareil géré) ni le client « bureau » de Google ne
// conviennent (revue du 19/10/2026). La connexion passe donc par le système :
// MSAL pour Microsoft — avec le courtier Microsoft Authenticator s'il est
// installé —, AuthorizationClient des services Google pour Gmail
// (ConnexionGoogle.java). Ni l'un ni l'autre ne rend de jeton de
// rafraîchissement : ils le gardent, et donnent un jeton d'accès à la
// demande.
//
// D'où deux chemins vers le noyau (core/src/android.rs) :
//  - connecter : connexion interactive, depuis l'interface ; son issue, en
//    JSON, revient par la méthode native termine (cpp/jetons_android.cpp,
//    cpp/connexion_android.h), puis Boite.terminerAndroid ;
//  - acces : jeton d'accès d'un compte déjà connecté, demandé par le noyau
//    (fil d'une session, ou tâche de relève) par le pont C
//    mmail_jeton_android. Bloquant : jamais sur le fil principal.
//
// Le compte est désigné par son identifiant chez le service : celui de MSAL
// (IAccount.getId), l'adresse pour Google.
package fr.mmedia.mmail;

import android.accounts.Account;
import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.util.Log;

import com.google.android.gms.auth.api.identity.AuthorizationRequest;
import com.google.android.gms.auth.api.identity.AuthorizationResult;
import com.google.android.gms.auth.api.identity.Identity;
import com.google.android.gms.common.api.Scope;
import com.google.android.gms.tasks.Tasks;

import com.microsoft.identity.client.AcquireTokenParameters;
import com.microsoft.identity.client.AcquireTokenSilentParameters;
import com.microsoft.identity.client.AuthenticationCallback;
import com.microsoft.identity.client.IAccount;
import com.microsoft.identity.client.IAuthenticationResult;
import com.microsoft.identity.client.IMultipleAccountPublicClientApplication;
import com.microsoft.identity.client.Prompt;
import com.microsoft.identity.client.PublicClientApplication;
import com.microsoft.identity.client.exception.MsalClientException;
import com.microsoft.identity.client.exception.MsalException;
import com.microsoft.identity.client.exception.MsalUiRequiredException;

import org.json.JSONObject;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Date;
import java.util.List;
import java.util.Map;
import java.util.concurrent.TimeUnit;

public class Jetons {
    private static final String JOURNAL = "MMail";

    /**
     * Portées de la connexion Microsoft : celles de core/src/microsoft.rs
     * (PORTEE_CODE, PORTEE_AUTORISATION). MSAL ajoute lui-même openid,
     * profile et offline_access. Lecture (IMAP) d'abord : c'est le jeton que
     * la connexion rend ; l'envoi (Graph) est accordé du même geste.
     */
    static final List<String> MICROSOFT_LECTURE =
            Arrays.asList("https://outlook.office.com/IMAP.AccessAsUser.All", "email");
    static final List<String> MICROSOFT_ENVOI =
            Arrays.asList("https://graph.microsoft.com/Mail.Send", "https://graph.microsoft.com/Mail.ReadWrite");

    /** Portées de Google : celles déclarées dans sa console (core/src/google.rs, PORTEE_AUTORISATION). */
    static final String GOOGLE_MAIL = "https://mail.google.com/";
    static final List<Scope> GOOGLE_PORTEES = Arrays.asList(
            new Scope("openid"), new Scope("https://www.googleapis.com/auth/userinfo.email"), new Scope(GOOGLE_MAIL));

    /** Délai d'une demande de jeton aux services du système. */
    private static final long DELAI = 60;

    private static Context contexte;
    private static IMultipleAccountPublicClientApplication msal;

    /** Donne au pont C de quoi appeler acces (machine virtuelle, cette classe). */
    private static native void inscrire();
    /** Remet l'issue d'une connexion interactive à l'interface. */
    private static native void termine(byte[] issue);

    /**
     * À appeler, une fois la bibliothèque de MMail chargée, dans chaque
     * processus où le noyau demandera des jetons : celui de l'application
     * (cpp/connexion_android.h), celui de la relève (Releve.java).
     */
    public static synchronized void preparer(Context c) {
        if (contexte != null)
            return;
        contexte = c.getApplicationContext();
        inscrire();
    }

    // ------------------------------------------------------- connexion

    /**
     * Connexion interactive d'un compte : {@code service} « microsoft » ou
     * « google », {@code indice} l'adresse d'un compte à reconnecter (vide
     * pour un nouveau). Appelé depuis l'interface (cpp/connexion_android.h) ;
     * l'issue revient par termine.
     */
    public static void connecter(Context c, String service, String indice) {
        preparer(c);
        if (!(c instanceof Activity)) {
            remettre(erreur(service, "pas d'activité pour la connexion", "android"));
            return;
        }
        Activity activite = (Activity) c;
        if ("google".equals(service)) {
            Intent i = new Intent(activite, ConnexionGoogle.class);
            i.putExtra("indice", indice == null ? "" : indice);
            activite.startActivity(i);
            return;
        }
        new Thread(() -> {
            try {
                AcquireTokenParameters.Builder p = new AcquireTokenParameters.Builder()
                        .startAuthorizationFromActivity(activite)
                        .withScopes(MICROSOFT_LECTURE)
                        .withOtherScopesToAuthorize(MICROSOFT_ENVOI)
                        .withPrompt(Prompt.SELECT_ACCOUNT)
                        .withCallback(new AuthenticationCallback() {
                            @Override
                            public void onSuccess(IAuthenticationResult r) {
                                remettre(connexionMicrosoft(r));
                            }

                            @Override
                            public void onError(MsalException e) {
                                remettre(erreur("microsoft", texte(e), code(e)));
                            }

                            @Override
                            public void onCancel() {
                                remettre(annulee("microsoft"));
                            }
                        });
                if (indice != null && !indice.isEmpty())
                    p.withLoginHint(indice);
                msal().acquireToken(p.build());
            } catch (Exception e) {
                remettre(erreur("microsoft", texte(e), code(e)));
            }
        }, "MMail-connexion").start();
    }

    private static JSONObject connexionMicrosoft(IAuthenticationResult r) {
        try {
            IAccount compte = r.getAccount();
            JSONObject o = new JSONObject();
            o.put("service", "microsoft");
            o.put("adresse", adresse(compte));
            o.put("compte", compte.getId());
            o.put("acces", r.getAccessToken());
            o.put("duree", duree(r.getExpiresOn()));
            return o;
        } catch (Exception e) {
            return erreur("microsoft", texte(e), "android");
        }
    }

    /**
     * L'adresse du compte : la revendication « email » du jeton
     * d'identité, comme sur le bureau ; à défaut, le nom de connexion.
     */
    private static String adresse(IAccount compte) {
        Map<String, ?> revendications = compte.getClaims();
        if (revendications != null) {
            for (String cle : new String[] {"email", "preferred_username"}) {
                Object v = revendications.get(cle);
                if (v != null && v.toString().contains("@"))
                    return v.toString();
            }
        }
        return compte.getUsername();
    }

    /** Issue d'une connexion, vers l'interface. */
    static void remettre(JSONObject issue) {
        try {
            termine(issue.toString().getBytes(StandardCharsets.UTF_8));
        } catch (Throwable e) {
            Log.w(JOURNAL, "connexion : " + e);
        }
    }

    static JSONObject erreur(String service, String message, String code) {
        JSONObject o = new JSONObject();
        try {
            o.put("service", service);
            o.put("erreur", message == null || message.isEmpty() ? "refus" : message);
            o.put("code", code == null ? "" : code);
        } catch (Exception ignoree) {
        }
        return o;
    }

    static JSONObject annulee(String service) {
        JSONObject o = new JSONObject();
        try {
            o.put("service", service);
            o.put("annule", true);
        } catch (Exception ignoree) {
        }
        return o;
    }

    // ------------------------------------------------------- jetons

    /**
     * Jeton d'accès d'un compte connecté par Android, pour {@code portee}
     * (portées séparées par des espaces, comme pour le point de jeton).
     * Rend {"acces", "duree"} ou {"erreur", "code", "reseau"}, en UTF-8.
     * Appelé par le pont C, sur le fil d'une session.
     */
    public static byte[] acces(byte[] service, byte[] compte, byte[] portee) {
        String s = new String(service, StandardCharsets.UTF_8);
        String c = new String(compte, StandardCharsets.UTF_8);
        String p = new String(portee, StandardCharsets.UTF_8);
        JSONObject o;
        try {
            o = "google".equals(s) ? accesGoogle(c) : accesMicrosoft(c, p);
        } catch (Exception e) {
            o = new JSONObject();
            try {
                o.put("erreur", texte(e));
                o.put("code", code(e));
                o.put("reseau", reseau(e));
            } catch (Exception ignoree) {
            }
        }
        return o.toString().getBytes(StandardCharsets.UTF_8);
    }

    private static JSONObject accesMicrosoft(String compte, String portee) throws Exception {
        IMultipleAccountPublicClientApplication application = msal();
        IAccount a = application.getAccount(compte);
        JSONObject o = new JSONObject();
        if (a == null) {
            // Compte oublié de MSAL (données de l'application effacées) :
            // l'accord est à redonner, avec la personne.
            o.put("erreur", "compte inconnu de MSAL");
            o.put("code", "invalid_grant");
            return o;
        }
        IAuthenticationResult r = application.acquireTokenSilent(new AcquireTokenSilentParameters.Builder()
                .forAccount(a)
                .fromAuthority(a.getAuthority())
                .withScopes(portees(portee))
                .build());
        o.put("acces", r.getAccessToken());
        o.put("duree", duree(r.getExpiresOn()));
        return o;
    }

    private static JSONObject accesGoogle(String adresse) throws Exception {
        AuthorizationRequest demande = AuthorizationRequest.builder()
                .setRequestedScopes(GOOGLE_PORTEES)
                .setAccount(new Account(adresse, "com.google"))
                .build();
        AuthorizationResult r = Tasks.await(Identity.getAuthorizationClient(contexte).authorize(demande), DELAI, TimeUnit.SECONDS);
        JSONObject o = new JSONObject();
        if (r.hasResolution() || r.getAccessToken() == null) {
            // L'accord est à redonner, avec la personne : « Se reconnecter… ».
            o.put("erreur", "Google demande un nouvel accord");
            o.put("code", "invalid_grant");
            return o;
        }
        o.put("acces", r.getAccessToken());
        return o;
    }

    /** Les portées du noyau, sans celles que MSAL ajoute lui-même. */
    static List<String> portees(String portee) {
        List<String> l = new ArrayList<>();
        for (String q : portee.trim().split("\\s+"))
            if (!q.isEmpty() && !q.equals("offline_access") && !q.equals("openid") && !q.equals("profile"))
                l.add(q);
        return l;
    }

    private static long duree(Date fin) {
        if (fin == null)
            return 3600;
        return Math.max(60, (fin.getTime() - System.currentTimeMillis()) / 1000);
    }

    private static synchronized IMultipleAccountPublicClientApplication msal() throws Exception {
        if (msal == null) {
            int config = contexte.getResources().getIdentifier("msal_config", "raw", contexte.getPackageName());
            msal = PublicClientApplication.createMultipleAccountPublicClientApplication(contexte, config);
        }
        return msal;
    }

    private static String texte(Throwable e) {
        String m = e.getMessage();
        return m == null || m.isEmpty() ? e.getClass().getSimpleName() : m;
    }

    /**
     * Code de l'erreur pour le noyau : celui de MSAL, ramené aux codes OAuth
     * que le noyau connaît (consentement, accord à redonner).
     */
    private static String code(Throwable e) {
        String m = texte(e);
        if (m.contains("AADSTS65001") || m.contains("AADSTS90094"))
            return "consent_required";
        if (e instanceof MsalUiRequiredException)
            return "invalid_grant";
        if (e instanceof MsalException)
            return ((MsalException) e).getErrorCode();
        return "android";
    }

    /** Vrai pour une panne de réseau : la session reprendra, sans reconnexion. */
    private static boolean reseau(Throwable e) {
        if (e instanceof IOException || e instanceof java.util.concurrent.TimeoutException)
            return true;
        if (e instanceof MsalClientException) {
            String c = ((MsalClientException) e).getErrorCode();
            return "device_network_not_available".equals(c) || "io_error".equals(c);
        }
        Throwable cause = e.getCause();
        return cause != null && cause != e && reseau(cause);
    }
}
