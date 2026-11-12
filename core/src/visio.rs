// SPDX-License-Identifier: GPL-3.0-or-later
//! Réunions sur le service de visio d'un compte : le nom d'une réunion propre
//! au domaine de la boîte, son lien, et la reconnaissance d'un lien déjà posé
//! dans un événement.
//!
//! Règle commune avec le service qui reçoit les organisateurs :
//!
//! - **préfixe** : le domaine de l'adresse du compte, en minuscules, chaque
//!   point remplacé par un tiret (`exemple.fr` → `exemple-fr`) ;
//! - **nom** : `<préfixe>-<suffixe>`, le suffixe en trois groupes de quatre
//!   signes de [`ALPHABET`] séparés de tirets, tirés d'un aléa
//!   cryptographique ; 64 signes au plus ;
//! - le service n'accepte l'organisateur que si le préfixe du nom est **égal**
//!   au domaine de sa boîte : une réunion d'un autre domaine lui est refusée.
//!
//! Le service lui-même est un réglage du compte (`accounts.visio`) : une
//! adresse HTTPS, vide par défaut. Une seule exception : une boîte hébergée
//! par MMEDIA (serveur [`HOTE_MMEDIA`]) a d'office le service de MMEDIA
//! ([`SERVICE_MMEDIA`]), même saisie à la main — sans quoi le bouton « Visio »
//! l'inviterait à demander ce qu'elle a déjà.

use aws_lc_rs::rand::{SecureRandom, SystemRandom};

/// Les 32 signes du suffixe : ni `l`, ni `o`, ni `0`, ni `1`, qui se
/// confondent à la lecture.
pub const ALPHABET: &[u8; 32] = b"abcdefghijkmnpqrstuvwxyz23456789";

/// Longueur maximale d'un nom de réunion.
pub const LONGUEUR_MAX: usize = 64;

/// Serveur des boîtes hébergées par MMEDIA.
pub const HOTE_MMEDIA: &str = "mail.mmedia.fr";

/// Service de visio de MMEDIA, d'office pour ses boîtes.
pub const SERVICE_MMEDIA: &str = "https://visio.mmedia.fr";

/// Service de visio d'un compte : celui qui est réglé, sinon celui de MMEDIA
/// pour une boîte de son serveur, sinon aucun (vide).
pub fn service_du_compte<'a>(reglage: &'a str, hote: &str) -> &'a str {
    if !reglage.is_empty() {
        reglage
    } else if hote.trim().eq_ignore_ascii_case(HOTE_MMEDIA) {
        SERVICE_MMEDIA
    } else {
        ""
    }
}

/// Préfixe des réunions d'une adresse : son domaine, en minuscules, points en
/// tirets. `None` pour une adresse sans domaine, ou dont le domaine n'est pas
/// fait de lettres ASCII, de chiffres, de points et de tirets (un domaine
/// accentué s'écrit en punycode).
pub fn prefixe(adresse: &str) -> Option<String> {
    let (_, domaine) = adresse.trim().rsplit_once('@')?;
    let domaine = domaine.trim().trim_end_matches('.').to_ascii_lowercase();
    let valide = !domaine.is_empty()
        && domaine.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !domaine.starts_with('.')
        && !domaine.contains("..");
    valide.then(|| domaine.replace('.', "-"))
}

/// Suffixe d'un nom de réunion, `xxxx-xxxx-xxxx`, tiré d'octets aléatoires.
/// 256 étant un multiple de 32, le reste de chaque octet donne un signe sans
/// biais.
fn suffixe_de(octets: &[u8; 12]) -> String {
    let signes: Vec<char> = octets.iter().map(|o| ALPHABET[(o % 32) as usize] as char).collect();
    signes.chunks(4).map(|g| g.iter().collect::<String>()).collect::<Vec<_>>().join("-")
}

/// Nom d'une nouvelle réunion organisée par `adresse`. `None` si l'adresse
/// n'a pas de préfixe valable, si le nom dépasserait [`LONGUEUR_MAX`] signes,
/// ou si le système ne fournit pas d'aléa.
pub fn nom_reunion(adresse: &str) -> Option<String> {
    let prefixe = prefixe(adresse)?;
    let mut octets = [0u8; 12];
    SystemRandom::new().fill(&mut octets).ok()?;
    let nom = format!("{prefixe}-{}", suffixe_de(&octets));
    (nom.len() <= LONGUEUR_MAX).then_some(nom)
}

/// Adresse du service telle que le compte la garde : HTTPS, sans espace, ni
/// requête, ni ancre, sans barre finale. `Some("")` pour une adresse vide
/// (pas de service) ; `None` pour une adresse refusée.
pub fn normaliser_service(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return Some(String::new());
    }
    let reste = url.strip_prefix("https://")?;
    let hote = reste.split('/').next().unwrap_or("");
    let valide = !hote.is_empty()
        && !url.chars().any(|c| c.is_whitespace() || c == '?' || c == '#' || c == '"' || c == '<' || c == '>');
    valide.then(|| url.trim_end_matches('/').to_string())
}

/// Nom de la réunion du service `service` dont le lien figure dans `texte`
/// (le lieu d'un événement), s'il y en a un. Le lien de l'organisateur
/// (`<service>/organiser/…`) n'en est pas un.
pub fn reunion_dans(texte: &str, service: &str) -> Option<String> {
    if service.is_empty() {
        return None;
    }
    let debut = format!("{}/", service.to_ascii_lowercase());
    let minuscules = texte.to_ascii_lowercase();
    let mut depuis = 0;
    while let Some(i) = minuscules[depuis..].find(&debut) {
        let position = depuis + i + debut.len();
        let reste = &texte[position..];
        let nom: String = reste.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        if !nom.is_empty() && !reste[nom.len()..].starts_with('/') {
            return Some(nom.to_ascii_lowercase());
        }
        depuis = position;
    }
    None
}

/// Service de visio offert pour les événements d'un agenda : celui de son
/// compte, sauf sur un agenda Microsoft 365, dont les réunions passent par
/// Teams. Vide : pas de bouton « Visio ».
pub fn service_de_l_agenda(service_du_compte: &str, microsoft: bool) -> &str {
    if microsoft {
        ""
    } else {
        service_du_compte
    }
}

/// Lien d'une réunion du service.
pub fn lien(service: &str, nom: &str) -> String {
    format!("{service}/{nom}")
}

/// Lien qui ouvre la réunion en organisateur, après connexion avec le compte
/// de messagerie.
pub fn lien_organisateur(service: &str, nom: &str) -> String {
    format!("{service}/organiser/mailcow?salle={nom}")
}

/// Ligne ajoutée en tête de la description d'un événement.
pub fn ligne_description(lien: &str) -> String {
    format!("Rejoindre la visio : {lien}")
}

/// Lieu et description d'un événement une fois la visio posée, pour une
/// réunion organisée par `adresse` sur `service`.
///
/// Un lieu qui porte déjà un lien du service le garde : le lien d'une réunion
/// déjà envoyée ne doit pas changer. Seule la ligne « Rejoindre la visio »
/// s'ajoute, si la description ne porte pas ce lien. Un lieu sans lien garde
/// son texte après le lien. `None` si aucun nom ne peut être tiré (cf.
/// [`nom_reunion`]) ou sans service.
pub fn poser(lieu: &str, description: &str, service: &str, adresse: &str) -> Option<(String, String)> {
    if service.is_empty() {
        return None;
    }
    let (lieu, lien) = match reunion_dans(lieu, service) {
        Some(nom) => (lieu.to_string(), lien(service, &nom)),
        None => {
            let lien = lien(service, &nom_reunion(adresse)?);
            let lieu = lieu.trim();
            let lieu = if lieu.is_empty() { lien.clone() } else { format!("{lien} — {lieu}") };
            (lieu, lien)
        }
    };
    let description = if description.contains(&lien) {
        description.to_string()
    } else if description.trim().is_empty() {
        ligne_description(&lien)
    } else {
        format!("{}\n\n{description}", ligne_description(&lien))
    };
    Some((lieu, description))
}

#[cfg(test)]
mod essais {
    use super::*;

    const SERVICE: &str = "https://visio.exemple.fr";

    #[test]
    fn prefixe_des_vecteurs_d_essai() {
        assert_eq!(prefixe("nom@exemple.fr").as_deref(), Some("exemple-fr"));
        assert_eq!(prefixe("Nom@Sous.Exemple.FR").as_deref(), Some("sous-exemple-fr"));
        assert_eq!(prefixe("a@exemple-fr.com").as_deref(), Some("exemple-fr-com"));
        assert_eq!(prefixe("sans-domaine"), None);
        assert_eq!(prefixe("a@"), None);
        assert_eq!(prefixe("a@exemplé.fr"), None, "domaine accentué : punycode attendu");
        assert_eq!(prefixe("a@.exemple.fr"), None);
        assert_eq!(prefixe("a@exemple..fr"), None);
    }

    #[test]
    fn forme_du_nom() {
        for _ in 0..200 {
            let nom = nom_reunion("nom@exemple.fr").unwrap();
            let suffixe = nom.strip_prefix("exemple-fr-").expect(&nom);
            let groupes: Vec<&str> = suffixe.split('-').collect();
            assert_eq!(groupes.len(), 3, "{nom}");
            for g in groupes {
                assert_eq!(g.len(), 4, "{nom}");
                assert!(g.bytes().all(|o| ALPHABET.contains(&o)), "{nom}");
            }
            assert!(nom.len() <= LONGUEUR_MAX);
        }
        assert_ne!(nom_reunion("a@exemple.fr"), nom_reunion("a@exemple.fr"), "aléa");
    }

    #[test]
    fn alphabet_sans_signes_ambigus() {
        assert_eq!(ALPHABET.len(), 32);
        for o in [b'l', b'o', b'0', b'1'] {
            assert!(!ALPHABET.contains(&o));
        }
        // Chaque reste d'octet tombe sur un signe, chaque signe sur 8 octets.
        let mut comptes = [0u32; 32];
        for o in 0..=255u8 {
            comptes[(o % 32) as usize] += 1;
        }
        assert!(comptes.iter().all(|&n| n == 8));
        assert_eq!(suffixe_de(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 31]), "abcd-efgh-ijk9");
    }

    #[test]
    fn nom_trop_long() {
        let domaine = format!("{}.fr", "a".repeat(47)); // préfixe de 50 signes
        assert_eq!(nom_reunion(&format!("x@{domaine}")), None, "50 + 15 > 64");
        let domaine = format!("{}.fr", "a".repeat(46)); // 49 + 15 = 64
        assert_eq!(nom_reunion(&format!("x@{domaine}")).unwrap().len(), 64);
    }

    #[test]
    fn adresse_du_service() {
        assert_eq!(normaliser_service("").as_deref(), Some(""));
        assert_eq!(normaliser_service("  https://visio.exemple.fr/ ").as_deref(), Some(SERVICE));
        assert_eq!(normaliser_service("https://exemple.fr/visio").as_deref(), Some("https://exemple.fr/visio"));
        assert_eq!(normaliser_service("http://visio.exemple.fr"), None, "HTTPS seulement");
        assert_eq!(normaliser_service("visio.exemple.fr"), None);
        assert_eq!(normaliser_service("https://"), None);
        assert_eq!(normaliser_service("https://visio.exemple.fr/?a=b"), None);
        assert_eq!(normaliser_service("https://visio exemple.fr"), None);
    }

    #[test]
    fn reunion_reconnue_dans_le_lieu() {
        let lieu = "https://visio.exemple.fr/exemple-fr-k3mz-8qhw-tp2a";
        assert_eq!(reunion_dans(lieu, SERVICE).as_deref(), Some("exemple-fr-k3mz-8qhw-tp2a"));
        assert_eq!(
            reunion_dans("Salle B — HTTPS://Visio.Exemple.fr/exemple-fr-k3mz-8qhw-tp2a, 2e étage", SERVICE).as_deref(),
            Some("exemple-fr-k3mz-8qhw-tp2a")
        );
        assert_eq!(reunion_dans("Salle B", SERVICE), None);
        assert_eq!(reunion_dans(lieu, ""), None, "sans service");
        assert_eq!(reunion_dans("https://visio.autre.fr/exemple-fr-k3mz-8qhw-tp2a", SERVICE), None);
        assert_eq!(reunion_dans("https://visio.exemple.fr/organiser/mailcow?salle=x", SERVICE), None);
        assert_eq!(reunion_dans("https://visio.exemple.fr/", SERVICE), None);
    }

    #[test]
    fn service_d_office_des_boites_mmedia() {
        assert_eq!(service_du_compte("", "mail.mmedia.fr"), SERVICE_MMEDIA);
        assert_eq!(service_du_compte("", " Mail.MMEDIA.fr "), SERVICE_MMEDIA);
        assert_eq!(service_du_compte(SERVICE, "mail.mmedia.fr"), SERVICE, "le réglage l'emporte");
        assert_eq!(service_du_compte("", "imap.orange.fr"), "", "autre hébergeur");
        assert_eq!(service_du_compte(SERVICE, "imap.orange.fr"), SERVICE);
    }

    #[test]
    fn pas_de_visio_sur_un_agenda_microsoft() {
        assert_eq!(service_de_l_agenda(SERVICE, false), SERVICE);
        assert_eq!(service_de_l_agenda(SERVICE, true), "", "agenda Microsoft 365");
        assert_eq!(service_de_l_agenda("", false), "", "compte sans service");
    }

    #[test]
    fn liens() {
        assert_eq!(lien(SERVICE, "exemple-fr-k3mz-8qhw-tp2a"), "https://visio.exemple.fr/exemple-fr-k3mz-8qhw-tp2a");
        assert_eq!(
            lien_organisateur(SERVICE, "exemple-fr-k3mz-8qhw-tp2a"),
            "https://visio.exemple.fr/organiser/mailcow?salle=exemple-fr-k3mz-8qhw-tp2a"
        );
    }

    #[test]
    fn poser_dans_un_evenement_vide() {
        let (lieu, description) = poser("", "", SERVICE, "nom@exemple.fr").unwrap();
        let nom = reunion_dans(&lieu, SERVICE).unwrap();
        assert!(nom.starts_with("exemple-fr-"));
        assert_eq!(lieu, lien(SERVICE, &nom));
        assert_eq!(description, format!("Rejoindre la visio : {lieu}"));
    }

    #[test]
    fn poser_garde_le_lieu_et_la_description() {
        let (lieu, description) = poser("Salle B", "Ordre du jour", SERVICE, "nom@exemple.fr").unwrap();
        let nom = reunion_dans(&lieu, SERVICE).unwrap();
        let lien = lien(SERVICE, &nom);
        assert_eq!(lieu, format!("{lien} — Salle B"));
        assert_eq!(description, format!("Rejoindre la visio : {lien}\n\nOrdre du jour"));
    }

    #[test]
    fn lien_deja_pose_non_regenere() {
        let lieu = "https://visio.exemple.fr/exemple-fr-k3mz-8qhw-tp2a";
        let description = format!("Rejoindre la visio : {lieu}\n\nOrdre du jour");
        let (l, d) = poser(lieu, &description, SERVICE, "nom@exemple.fr").unwrap();
        assert_eq!((l.as_str(), d.as_str()), (lieu, description.as_str()), "rien ne change");
        // Lien dans le lieu, description effacée : la ligne revient, même lien.
        let (l, d) = poser(lieu, "", SERVICE, "nom@exemple.fr").unwrap();
        assert_eq!(l, lieu);
        assert_eq!(d, format!("Rejoindre la visio : {lieu}"));
    }

    #[test]
    fn poser_sans_service_ou_sans_domaine() {
        assert_eq!(poser("", "", "", "nom@exemple.fr"), None);
        assert_eq!(poser("", "", SERVICE, "sans-domaine"), None);
    }
}
