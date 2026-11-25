// SPDX-License-Identifier: GPL-3.0-or-later
//
// Connexion d'un compte Gmail par les services Google (0.7.0, cf.
// Jetons.java). Activité transparente, lancée par Jetons.connecter : elle
// demande l'accès au courrier (AuthorizationClient) ; Google montre, au
// besoin, le choix du compte et son écran d'accord, dont le résultat revient
// ici (onActivityResult) — QtActivity n'a pas à le relayer. L'adresse du
// compte est lue chez Google (userinfo) avec le jeton obtenu, puis l'issue
// part vers l'interface (Jetons.remettre) et l'activité se ferme.
package fr.mmedia.mmail;

import android.accounts.Account;
import android.app.Activity;
import android.content.Intent;
import android.content.IntentSender;
import android.os.Bundle;

import com.google.android.gms.auth.api.identity.AuthorizationRequest;
import com.google.android.gms.auth.api.identity.AuthorizationResult;
import com.google.android.gms.auth.api.identity.Identity;
import com.google.android.gms.common.api.ApiException;
import com.google.android.gms.common.api.Scope;

import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;

public class ConnexionGoogle extends Activity {
    private static final int ACCORD = 4711;
    private static final String USERINFO = "https://openidconnect.googleapis.com/v1/userinfo";

    private boolean remise;

    @Override
    protected void onCreate(Bundle etat) {
        super.onCreate(etat);
        if (etat != null)
            return; // recréée pendant l'accord : le résultat reviendra
        String indice = getIntent().getStringExtra("indice");
        AuthorizationRequest.Builder demande = AuthorizationRequest.builder().setRequestedScopes(Jetons.GOOGLE_PORTEES);
        if (indice != null && !indice.isEmpty())
            demande.setAccount(new Account(indice, "com.google"));
        Identity.getAuthorizationClient(this)
                .authorize(demande.build())
                .addOnSuccessListener(this::issue)
                .addOnFailureListener(e -> finir(Jetons.erreur("google", e.getMessage(), "android")));
    }

    private void issue(AuthorizationResult r) {
        if (!r.hasResolution()) {
            aboutir(r);
            return;
        }
        try {
            startIntentSenderForResult(r.getPendingIntent().getIntentSender(), ACCORD, null, 0, 0, 0);
        } catch (IntentSender.SendIntentException e) {
            finir(Jetons.erreur("google", e.getMessage(), "android"));
        }
    }

    @Override
    protected void onActivityResult(int demande, int resultat, Intent donnees) {
        super.onActivityResult(demande, resultat, donnees);
        if (demande != ACCORD)
            return;
        if (resultat != RESULT_OK) {
            finir(Jetons.annulee("google"));
            return;
        }
        try {
            aboutir(Identity.getAuthorizationClient(this).getAuthorizationResultFromIntent(donnees));
        } catch (ApiException e) {
            finir(Jetons.erreur("google", e.getMessage(), "android"));
        }
    }

    /** Accès accordé : contrôle de la portée, adresse du compte, puis l'issue. */
    private void aboutir(AuthorizationResult r) {
        String jeton = r.getAccessToken();
        if (jeton == null || jeton.isEmpty()) {
            finir(Jetons.erreur("google", "Google n'a pas rendu de jeton d'accès", "android"));
            return;
        }
        boolean courrier = false;
        if (r.getGrantedScopes() != null)
            for (String p : r.getGrantedScopes())
                courrier |= Jetons.GOOGLE_MAIL.equals(p);
        if (!courrier) {
            finir(Jetons.erreur("google", "l'accès à la messagerie n'a pas été accordé", "portee"));
            return;
        }
        new Thread(() -> {
            JSONObject o;
            try {
                String adresse = adresse(jeton);
                o = new JSONObject();
                o.put("service", "google");
                o.put("adresse", adresse);
                o.put("compte", adresse);
                o.put("acces", jeton);
            } catch (Exception e) {
                o = Jetons.erreur("google", "adresse du compte illisible : " + e.getMessage(), "adresse");
            }
            JSONObject issue = o;
            runOnUiThread(() -> finir(issue));
        }, "MMail-google").start();
    }

    /** L'adresse du compte, chez Google, avec le jeton qu'il vient de donner. */
    private static String adresse(String jeton) throws Exception {
        HttpURLConnection c = (HttpURLConnection) new URL(USERINFO).openConnection();
        try {
            c.setConnectTimeout(30000);
            c.setReadTimeout(30000);
            c.setRequestProperty("Authorization", "Bearer " + jeton);
            if (c.getResponseCode() != 200)
                throw new Exception("HTTP " + c.getResponseCode());
            ByteArrayOutputStream lu = new ByteArrayOutputStream();
            try (InputStream in = c.getInputStream()) {
                byte[] tampon = new byte[4096];
                for (int n; (n = in.read(tampon)) > 0 && lu.size() < 65536; )
                    lu.write(tampon, 0, n);
            }
            String adresse = new JSONObject(new String(lu.toByteArray(), StandardCharsets.UTF_8)).optString("email");
            if (!adresse.contains("@"))
                throw new Exception("pas d'adresse");
            return adresse;
        } finally {
            c.disconnect();
        }
    }

    private void finir(JSONObject issue) {
        if (!remise) {
            remise = true;
            Jetons.remettre(issue);
        }
        finish();
    }

    @Override
    protected void onDestroy() {
        // Fermée sans issue (Retour pendant l'attente) : connexion abandonnée.
        if (!remise && isFinishing()) {
            remise = true;
            Jetons.remettre(Jetons.annulee("google"));
        }
        super.onDestroy();
    }
}
