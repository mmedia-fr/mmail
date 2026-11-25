// SPDX-License-Identifier: GPL-3.0-or-later
//! Comptes Microsoft et Google connectés par Android (0.7.0).
//!
//! Sous Android, la connexion passe par le système : MSAL, et l'application
//! Microsoft Authenticator quand elle est là (courtier : accès conditionnel,
//! appareil géré), pour Microsoft ; `AuthorizationClient` des services Google
//! pour Gmail. Ni l'un ni l'autre ne rend de jeton de rafraîchissement : ils
//! le gardent, et donnent un jeton d'accès à la demande
//! (`packaging/android/src/fr/mmedia/mmail/Jetons.java`).
//!
//! Le noyau n'en sait presque rien : un tel compte a pour « jeton de
//! rafraîchissement » un repère, `android:` suivi de son identifiant chez le
//! service (`DELEGUE`), et `oauth::Fournisseur::rafraichir` demande alors le
//! jeton d'accès à Java (`jeton`) au lieu du point de jeton. Le coffre, la
//! relève en arrière-plan, l'envoi passent sans changement. Hors d'Android —
//! une sauvegarde restaurée sur un poste —, le repère ne vaut rien : le
//! compte est à reconnecter.

use serde::Deserialize;

use crate::oauth::{ErreurOAuth, Fournisseur, ReponseJeton};

/// Préfixe du repère d'un compte connecté par Android, à la place du jeton de
/// rafraîchissement. Ni Microsoft (« 0.A… », « M.C… ») ni Google (« 1//… »)
/// ne délivrent de jeton qui commence ainsi.
pub const DELEGUE: &str = "android:";

/// Le repère d'un compte connecté par Android, `compte` étant son
/// identifiant chez le service (MSAL : celui du compte ; Google : l'adresse).
pub fn repere(compte: &str) -> String {
    format!("{DELEGUE}{compte}")
}

/// Le nom du service tel que Java l'attend.
pub fn nom_service(f: Fournisseur) -> &'static str {
    match f {
        Fournisseur::Microsoft => "microsoft",
        Fournisseur::Google => "google",
    }
}

fn service_de_nom(nom: &str) -> Option<Fournisseur> {
    match nom {
        "microsoft" => Some(Fournisseur::Microsoft),
        "google" => Some(Fournisseur::Google),
        _ => None,
    }
}

/// Réponse de Java à une demande de jeton : `{"acces", "duree"}`, ou
/// `{"erreur", "code"}` (code OAuth ou de MSAL, s'il en a un).
#[derive(Debug, Default, Deserialize)]
struct Reponse {
    #[serde(default)]
    acces: String,
    #[serde(default)]
    duree: u64,
    #[serde(default)]
    erreur: String,
    #[serde(default)]
    code: String,
    /// Panne de réseau, et non refus : la session reprendra.
    #[serde(default)]
    reseau: bool,
}

/// Lit la réponse de Java à une demande de jeton d'accès.
pub fn lire_jeton(texte: &str) -> Result<ReponseJeton, ErreurOAuth> {
    let r: Reponse = serde_json::from_str(texte).map_err(|e| ErreurOAuth::Reseau(format!("réponse d'Android illisible : {e}")))?;
    if !r.erreur.is_empty() || r.acces.is_empty() {
        let description = if r.erreur.is_empty() { "jeton d'accès absent".to_string() } else { r.erreur };
        if r.reseau {
            return Err(ErreurOAuth::Reseau(description));
        }
        let code = if r.code.is_empty() { "android".to_string() } else { r.code };
        return Err(ErreurOAuth::Refus { code, description });
    }
    Ok(ReponseJeton {
        acces: r.acces,
        rafraichissement: None,
        // Google ne dit pas la durée de ses jetons d'accès : une heure.
        duree: if r.duree == 0 { 3600 } else { r.duree },
        id_token: None,
        portees: None,
    })
}

/// Connexion menée par Android et aboutie : de quoi enregistrer le compte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connexion {
    pub service: Fournisseur,
    pub adresse: String,
    /// Identifiant du compte chez le service, pour le repère.
    pub compte: String,
    pub acces: String,
    pub duree: u64,
}

#[derive(Debug, Default, Deserialize)]
struct Retour {
    #[serde(default)]
    service: String,
    #[serde(default)]
    adresse: String,
    #[serde(default)]
    compte: String,
    #[serde(default)]
    acces: String,
    #[serde(default)]
    duree: u64,
    #[serde(default)]
    erreur: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    annule: bool,
}

/// Issue d'une connexion menée par Android (`Jetons.java`, retour remis par
/// `cpp/connexion_android.h`) : le service, puis la connexion ou le refus.
/// `None` : réponse illisible, ou d'un service inconnu.
pub fn lire_connexion(texte: &str) -> Option<(Fournisseur, Result<Connexion, ErreurOAuth>)> {
    let r: Retour = serde_json::from_str(texte).ok()?;
    let service = service_de_nom(&r.service)?;
    if r.annule {
        return Some((service, Err(ErreurOAuth::refus("access_denied", "connexion abandonnée"))));
    }
    if !r.erreur.is_empty() {
        let code = if r.code.is_empty() { "android".to_string() } else { r.code };
        return Some((service, Err(ErreurOAuth::Refus { code, description: r.erreur })));
    }
    let adresse = r.adresse.trim().to_lowercase();
    if adresse.is_empty() || !adresse.contains('@') {
        return Some((service, Err(ErreurOAuth::refus("adresse", "le service n'a pas donné l'adresse du compte"))));
    }
    if r.compte.is_empty() || r.acces.is_empty() {
        return Some((service, Err(ErreurOAuth::refus("android", "connexion incomplète"))));
    }
    let duree = if r.duree == 0 { 3600 } else { r.duree };
    Some((service, Ok(Connexion { service, adresse, compte: r.compte, acces: r.acces, duree })))
}

/// Jeton d'accès d'un compte connecté par Android, pour `portee` (portées
/// séparées par des espaces, comme pour le point de jeton).
pub fn jeton(service: Fournisseur, compte: &str, portee: &str) -> Result<ReponseJeton, ErreurOAuth> {
    #[cfg(target_os = "android")]
    {
        lire_jeton(&interface::demander(nom_service(service), compte, portee))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (compte, portee);
        Err(ErreurOAuth::refus(
            "android",
            &format!("compte {} connecté sur un téléphone Android : à reconnecter sur ce poste", service.nom()),
        ))
    }
}

/// Appel de Java, par le pont C de `cpp/jetons_android.cpp` : bloquant (le
/// fil d'une session, jamais celui de l'interface).
#[cfg(target_os = "android")]
mod interface {
    use std::ffi::{c_char, CStr, CString};

    extern "C" {
        fn mmail_jeton_android(service: *const c_char, compte: *const c_char, portee: *const c_char) -> *mut c_char;
        fn mmail_jeton_android_liberer(texte: *mut c_char);
    }

    pub fn demander(service: &str, compte: &str, portee: &str) -> String {
        let c = |t: &str| CString::new(t.replace('\0', "")).unwrap_or_default();
        let (s, c_, p) = (c(service), c(compte), c(portee));
        // SAFETY : chaînes terminées par un zéro, tenues le temps de l'appel ;
        // la réponse vient de strdup, rendue par mmail_jeton_android_liberer.
        unsafe {
            let r = mmail_jeton_android(s.as_ptr(), c_.as_ptr(), p.as_ptr());
            if r.is_null() {
                return r#"{"erreur": "Android n'a pas répondu", "reseau": true}"#.to_string();
            }
            let texte = CStr::from_ptr(r).to_string_lossy().into_owned();
            mmail_jeton_android_liberer(r);
            texte
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repere_d_un_compte() {
        assert_eq!(repere("00000000-0000-0000-0000-000000000001.abc"), "android:00000000-0000-0000-0000-000000000001.abc");
        assert!(repere("nom@gmail.com").starts_with(DELEGUE));
    }

    #[test]
    fn jeton_rendu_par_android() {
        let r = lire_jeton(r#"{"acces": "AT", "duree": 3599}"#).unwrap();
        assert_eq!((r.acces.as_str(), r.duree, r.rafraichissement), ("AT", 3599, None));
        // Google ne donne pas de durée : une heure.
        assert_eq!(lire_jeton(r#"{"acces": "AT"}"#).unwrap().duree, 3600);
    }

    #[test]
    fn refus_et_panne_d_android() {
        let e = lire_jeton(r#"{"erreur": "consentement requis", "code": "consent_required"}"#).unwrap_err();
        assert!(e.consentement());
        assert!(matches!(e.vers_erreur("Microsoft"), crate::imap::Erreur::Refuse(_)));
        let p = lire_jeton(r#"{"erreur": "pas de réseau", "reseau": true}"#).unwrap_err();
        assert!(matches!(p, ErreurOAuth::Reseau(_)));
        assert!(lire_jeton("pas du JSON").is_err());
        assert!(lire_jeton(r#"{"duree": 3599}"#).is_err(), "sans jeton");
    }

    #[test]
    fn connexion_menee_par_android() {
        let (s, c) = lire_connexion(
            r#"{"service": "microsoft", "adresse": "Jean.Dupont@Exemple.fr", "compte": "oid.tid", "acces": "AT", "duree": 3599}"#,
        )
        .unwrap();
        assert_eq!(s, Fournisseur::Microsoft);
        assert_eq!(
            c.unwrap(),
            Connexion { service: s, adresse: "jean.dupont@exemple.fr".into(), compte: "oid.tid".into(), acces: "AT".into(), duree: 3599 }
        );
        let (s, c) = lire_connexion(r#"{"service": "google", "annule": true}"#).unwrap();
        assert_eq!(s, Fournisseur::Google);
        assert!(c.is_err());
        let (_, c) = lire_connexion(r#"{"service": "google", "compte": "x", "acces": "AT"}"#).unwrap();
        assert!(c.is_err(), "sans adresse");
        assert!(lire_connexion(r#"{"service": "yahoo"}"#).is_none());
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn hors_d_android_a_reconnecter() {
        let e = jeton(Fournisseur::Google, "nom@gmail.com", "https://mail.google.com/").unwrap_err();
        assert!(matches!(e.vers_erreur("Google"), crate::imap::Erreur::Refuse(_)));
    }
}
