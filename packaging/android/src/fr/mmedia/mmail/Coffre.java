// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lecture, depuis la tâche de relève (Releve.java), des secrets que MMail a
// mis au coffre par QtKeychain (cpp/coffre.h, service « MMail ») : un seul
// coffre, pas de copie des mots de passe pour la tâche. La tâche peut tourner
// sans que Qt ait démarré : QtKeychain n'est alors pas utilisable, d'où cette
// lecture directe de son format (QtKeychain 0.17.0, keychain_android.cpp) :
//
// - le secret chiffré est dans les réglages de Qt (QSettings « MMail », au
//   format INI de Qt), groupe <clé du compte>, valeur « data », un QByteArray ;
// - « QKCA », longueur de la clé chiffrée (4 octets, gros-boutiste), clé AES
//   chiffrée en RSA (PKCS#1) par la clé « MMail/<clé du compte> » du magasin
//   de clés d'Android, IV (12 octets), puis le secret en AES-GCM ;
// - format antérieur (sans « QKCA ») : le secret chiffré en RSA, d'un bloc.
//
// À un changement de version de QtKeychain (CMakeLists.txt), revoir ce format.
package fr.mmedia.mmail;

import android.content.Context;
import android.util.Log;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.security.KeyStore;
import java.security.PrivateKey;
import java.util.Arrays;

import javax.crypto.Cipher;
import javax.crypto.spec.GCMParameterSpec;
import javax.crypto.spec.SecretKeySpec;

final class Coffre {
    private static final String SERVICE = "MMail";
    private static final byte[] HYBRIDE = {'Q', 'K', 'C', 'A'};

    private final String reglages;
    private KeyStore magasin;

    Coffre(Context contexte) {
        reglages = lireReglages(contexte.getFilesDir());
    }

    /**
     * Fichier des réglages de QtKeychain. Qt sous Android les écrit dans
     * files/settings, ou dans files/.config si ce dossier existait déjà
     * (cf. make_user_path de qsettings.cpp).
     */
    private static String lireReglages(File fichiers) {
        for (String dossier : new String[] {".config", "settings"}) {
            File f = new File(new File(fichiers, dossier), SERVICE + ".conf");
            if (!f.isFile())
                continue;
            try {
                return new String(Files.readAllBytes(f.toPath()), StandardCharsets.UTF_8);
            } catch (Exception e) {
                Log.w("MMail", "coffre : " + e);
            }
        }
        return "";
    }

    /** Secret du compte {@code cle} ; {@code null} s'il n'y en a pas ou s'il est illisible. */
    String lire(String cle) {
        try {
            byte[] chiffre = donnee(reglages, cle);
            if (chiffre == null)
                return null;
            if (magasin == null) {
                magasin = KeyStore.getInstance("AndroidKeyStore");
                magasin.load(null);
            }
            KeyStore.Entry entree = magasin.getEntry(SERVICE + "/" + cle, null);
            if (!(entree instanceof KeyStore.PrivateKeyEntry))
                return null;
            PrivateKey cleRsa = ((KeyStore.PrivateKeyEntry) entree).getPrivateKey();
            return new String(dechiffrer(chiffre, cleRsa), StandardCharsets.UTF_8);
        } catch (Exception e) {
            Log.w("MMail", "coffre : " + e);
            return null;
        }
    }

    private static byte[] dechiffrer(byte[] d, PrivateKey cleRsa) throws Exception {
        Cipher rsa = Cipher.getInstance("RSA/ECB/PKCS1Padding");
        rsa.init(Cipher.DECRYPT_MODE, cleRsa);
        if (d.length < HYBRIDE.length || !Arrays.equals(Arrays.copyOf(d, HYBRIDE.length), HYBRIDE))
            return rsa.doFinal(d);
        int n = ((d[4] & 0xFF) << 24) | ((d[5] & 0xFF) << 16) | ((d[6] & 0xFF) << 8) | (d[7] & 0xFF);
        int debut = 8;
        if (n <= 0 || d.length < debut + n + 12 + 16)
            throw new IllegalArgumentException("secret chiffré trop court");
        byte[] cleAes = rsa.doFinal(Arrays.copyOfRange(d, debut, debut + n));
        byte[] iv = Arrays.copyOfRange(d, debut + n, debut + n + 12);
        Cipher aes = Cipher.getInstance("AES/GCM/NoPadding");
        aes.init(Cipher.DECRYPT_MODE, new SecretKeySpec(cleAes, "AES"), new GCMParameterSpec(128, iv));
        return aes.doFinal(Arrays.copyOfRange(d, debut + n + 12, d.length));
    }

    // ----------------------------------------------- format INI de QSettings

    /** Valeur « data » du groupe {@code cle}, comme QByteArray ; {@code null} si absente. */
    static byte[] donnee(String ini, String cle) {
        String groupe = "[" + cleEchappee(cle) + "]";
        boolean dedans = false;
        for (String ligne : ini.split("\r?\n")) {
            String l = ligne.trim();
            if (l.startsWith("[")) {
                dedans = l.equals(groupe);
                continue;
            }
            if (!dedans)
                continue;
            int egal = l.indexOf('=');
            if (egal < 0 || !l.substring(0, egal).trim().equals("data"))
                continue;
            String valeur = sansEchappement(l.substring(egal + 1));
            if (!valeur.startsWith("@ByteArray(") || !valeur.endsWith(")"))
                return null;
            String brut = valeur.substring(11, valeur.length() - 1);
            byte[] octets = new byte[brut.length()];
            for (int i = 0; i < brut.length(); i++)
                octets[i] = (byte) brut.charAt(i);
            return octets;
        }
        return null;
    }

    /** QSettingsPrivate::iniEscapedKey. */
    static String cleEchappee(String cle) {
        StringBuilder r = new StringBuilder();
        for (int i = 0; i < cle.length(); i++) {
            char c = cle.charAt(i);
            if (c == '/')
                r.append('\\');
            else if ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_' || c == '-' || c == '.')
                r.append(c);
            else if (c <= 0xFF)
                r.append(String.format("%%%02X", (int) c));
            else
                r.append(String.format("%%U%04X", (int) c));
        }
        return r.toString();
    }

    /** QSettingsPrivate::iniUnescapedStringList, pour une valeur simple. */
    static String sansEchappement(String s) {
        StringBuilder r = new StringBuilder();
        int i = 0;
        while (i < s.length() && (s.charAt(i) == ' ' || s.charAt(i) == '\t'))
            i++;
        boolean guillemets = false, cite = false;
        while (i < s.length()) {
            char c = s.charAt(i++);
            if (c == '"') {
                guillemets = !guillemets;
                cite = true;
            } else if (c == '\\' && i < s.length()) {
                char e = s.charAt(i++);
                switch (e) {
                    case 'a': r.append('\u0007'); break;
                    case 'b': r.append('\b'); break;
                    case 'f': r.append('\f'); break;
                    case 'n': r.append('\n'); break;
                    case 'r': r.append('\r'); break;
                    case 't': r.append('\t'); break;
                    case 'v': r.append('\u000B'); break;
                    case '"': case '?': case '\'': case '\\': r.append(e); break;
                    case 'x': {
                        int v = 0;
                        while (i < s.length() && Character.digit(s.charAt(i), 16) >= 0)
                            v = ((v << 4) + Character.digit(s.charAt(i++), 16)) & 0xFFFF;
                        r.append((char) v);
                        break;
                    }
                    default:
                        if (e >= '0' && e <= '7') {
                            int v = e - '0';
                            while (i < s.length() && s.charAt(i) >= '0' && s.charAt(i) <= '7')
                                v = ((v << 3) + (s.charAt(i++) - '0')) & 0xFFFF;
                            r.append((char) v);
                        }
                        // Autre caractère échappé : ignoré, comme Qt.
                }
            } else {
                r.append(c);
            }
        }
        if (!cite) {
            int n = r.length();
            while (n > 0 && (r.charAt(n - 1) == ' ' || r.charAt(n - 1) == '\t'))
                n--;
            r.setLength(n);
        }
        return r.toString();
    }
}
