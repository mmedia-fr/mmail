// SPDX-License-Identifier: GPL-3.0-or-later
//! Requêtes HTTPS minimales : configuration automatique d'un compte et lien de
//! configuration (cf. `configuration`).
//!
//! Écrit sur la pile TLS du client IMAP plutôt que pris d'une bibliothèque :
//! deux requêtes sans état, des réponses de quelques kilo-octets, et surtout le
//! même fournisseur cryptographique et les mêmes autorités de certification
//! sur les trois cibles — une seconde pile TLS en apporterait d'autres, qu'il
//! faudrait régler à nouveau pour Android.
//!
//! Seul HTTPS est accepté, redirections comprises.

use std::io::{ErrorKind, Read, Write};
use std::sync::Arc;

use crate::imap::{configuration_tls, joindre, Erreur, Resultat, DELAI};

/// Taille au-delà de laquelle une réponse est refusée : un document de
/// configuration tient en quelques kilo-octets.
const TAILLE_MAX: usize = 256 * 1024;

/// Redirections suivies au plus, pour une requête GET.
const REDIRECTIONS_MAX: usize = 3;

#[derive(Debug, PartialEq)]
pub struct Reponse {
    pub statut: u16,
    pub type_contenu: String,
    /// En-tête `Location`, s'il y en a un.
    pub redirection: Option<String>,
    pub corps: Vec<u8>,
}

/// Une adresse HTTPS découpée : ce qu'il faut pour se connecter et demander.
#[derive(Debug, PartialEq)]
pub struct Cible {
    pub hote: String,
    pub port: u16,
    /// Chemin et requête, tels qu'ils partent sur la ligne de demande.
    pub chemin: String,
}

/// Découpe une adresse `https://hote[:port]/chemin?requete`. Tout le reste —
/// `http://`, identifiants dans l'adresse, espaces — est refusé.
pub fn analyser_url(url: &str) -> Resultat<Cible> {
    let invalide = || Erreur::Protocole(format!("adresse invalide, « https:// » attendu : {url}"));
    let reste = url.trim().strip_prefix("https://").ok_or_else(invalide)?;
    let reste = reste.split('#').next().unwrap_or("");
    let (autorite, chemin) = match reste.find(['/', '?']) {
        Some(i) => (&reste[..i], &reste[i..]),
        None => (reste, "/"),
    };
    let chemin = if chemin.starts_with('?') { format!("/{chemin}") } else { chemin.to_string() };
    if autorite.contains('@') || chemin.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalide());
    }
    let (hote, port) = match autorite.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().map_err(|_| invalide())?),
        None => (autorite, 443),
    };
    let hote_valide = !hote.is_empty()
        && hote.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if !hote_valide {
        return Err(invalide());
    }
    Ok(Cible { hote: hote.to_ascii_lowercase(), port, chemin })
}

/// Adresse désignée par un en-tête `Location`, relative ou absolue.
pub fn resoudre(depart: &str, location: &str) -> Resultat<String> {
    if location.starts_with("https://") {
        return Ok(location.to_string());
    }
    if location.starts_with('/') && !location.starts_with("//") {
        let cible = analyser_url(depart)?;
        let autorite = if cible.port == 443 {
            cible.hote
        } else {
            format!("{}:{}", cible.hote, cible.port)
        };
        return Ok(format!("https://{autorite}{location}"));
    }
    Err(Erreur::Protocole(format!("redirection refusée : {location}")))
}

/// Envoie une requête sans corps et rend la réponse. Un GET suit les
/// redirections vers HTTPS ; un POST, jamais — il n'a pas à être rejoué ailleurs.
pub fn requete(methode: &str, url: &str, accepte: &str) -> Resultat<Reponse> {
    requete_bornee(methode, url, accepte, TAILLE_MAX)
}

/// Comme `requete`, avec une autre limite de taille du corps : une image
/// distante d'un message pèse plus qu'un document de configuration.
pub fn requete_bornee(methode: &str, url: &str, accepte: &str, taille_max: usize) -> Resultat<Reponse> {
    let mut url = url.trim().to_string();
    for _ in 0..=REDIRECTIONS_MAX {
        let reponse = une_requete(methode, &url, accepte, taille_max)?;
        let redirige = matches!(reponse.statut, 301 | 302 | 303 | 307 | 308);
        match (&reponse.redirection, methode == "GET" && redirige) {
            (Some(location), true) => url = resoudre(&url, location)?,
            _ => return Ok(reponse),
        }
    }
    Err(Erreur::Protocole("trop de redirections".into()))
}

fn une_requete(methode: &str, url: &str, accepte: &str, taille_max: usize) -> Resultat<Reponse> {
    let cible = analyser_url(url)?;
    let config = configuration_tls()?;
    let nom = rustls::pki_types::ServerName::try_from(cible.hote.clone())
        .map_err(|_| Erreur::Reseau(format!("nom de serveur invalide : {}", cible.hote)))?;
    let connexion = rustls::ClientConnection::new(Arc::new(config), nom)
        .map_err(|e| Erreur::Reseau(e.to_string()))?;
    let tcp = joindre(&cible.hote, cible.port)?;
    tcp.set_read_timeout(Some(DELAI))?;
    tcp.set_write_timeout(Some(DELAI))?;
    let mut flux = rustls::StreamOwned::new(connexion, tcp);

    let hote = if cible.port == 443 {
        cible.hote.clone()
    } else {
        format!("{}:{}", cible.hote, cible.port)
    };
    let demande = format!(
        "{methode} {} HTTP/1.1\r\nHost: {hote}\r\nUser-Agent: MMail/{}\r\nAccept: {accepte}\r\n\
         Content-Length: 0\r\nConnection: close\r\n\r\n",
        cible.chemin,
        env!("CARGO_PKG_VERSION")
    );
    flux.write_all(demande.as_bytes())?;
    flux.flush()?;

    let mut brut = Vec::new();
    let mut tampon = [0u8; 8192];
    loop {
        match flux.read(&mut tampon) {
            Ok(0) => break,
            Ok(n) => {
                brut.extend_from_slice(&tampon[..n]);
                // Marge pour les en-têtes : c'est le corps que borne la limite.
                if brut.len() > taille_max + 64 * 1024 {
                    return Err(Erreur::Refuse("réponse trop volumineuse".into()));
                }
                // Certains serveurs gardent la connexion ouverte malgré
                // « Connection: close » (Apache qui propose HTTP/2) : on
                // n'attend pas leur fermeture quand la réponse est complète.
                if reponse_complete(&brut) {
                    break;
                }
            }
            // Bien des serveurs ferment sans la notification TLS de fin : ce
            // qui a été lu reste valable, et le découpage HTTP dira s'il est
            // complet (longueur annoncée, dernier bloc).
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
    }
    analyser_reponse_bornee(&brut, taille_max)
}

/// Vrai si la réponse reçue est entière : corps de la longueur annoncée, ou
/// dernier bloc reçu. Sans l'une ni l'autre, seule la fermeture le dira.
fn reponse_complete(brut: &[u8]) -> bool {
    let Some(fin) = brut.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let tete = String::from_utf8_lossy(&brut[..fin]).to_ascii_lowercase();
    let entete = |nom: &str| {
        tete.split("\r\n").find_map(|l| l.split_once(':').filter(|(n, _)| n.trim() == nom).map(|(_, v)| v.trim().to_string()))
    };
    let corps = &brut[fin + 4..];
    if entete("transfer-encoding").map(|v| v.contains("chunked")).unwrap_or(false) {
        return corps.ends_with(b"\r\n0\r\n\r\n") || corps == b"0\r\n\r\n";
    }
    match entete("content-length").and_then(|v| v.parse::<usize>().ok()) {
        Some(n) => corps.len() >= n,
        None => false,
    }
}

/// Découpe une réponse HTTP/1.x complète : statut, en-têtes utiles, corps.
pub fn analyser_reponse(brut: &[u8]) -> Resultat<Reponse> {
    analyser_reponse_bornee(brut, TAILLE_MAX)
}

fn analyser_reponse_bornee(brut: &[u8], taille_max: usize) -> Resultat<Reponse> {
    let fin = brut
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| Erreur::Protocole("réponse HTTP incomplète".into()))?;
    let tete = std::str::from_utf8(&brut[..fin])
        .map_err(|_| Erreur::Protocole("en-têtes HTTP illisibles".into()))?;
    let mut lignes = tete.split("\r\n");
    let premiere = lignes.next().unwrap_or("");
    let mut parties = premiere.splitn(3, ' ');
    if !parties.next().unwrap_or("").starts_with("HTTP/1.") {
        return Err(Erreur::Protocole(format!("réponse inattendue : {premiere}")));
    }
    let statut: u16 = parties
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Erreur::Protocole(format!("statut illisible : {premiere}")))?;
    let entetes: Vec<(String, String)> = lignes
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let entete = |nom: &str| entetes.iter().find(|(n, _)| n == nom).map(|(_, v)| v.as_str());

    let reste = &brut[fin + 4..];
    let morcele = entete("transfer-encoding")
        .map(|v| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);
    let corps = if morcele {
        rassembler(reste, taille_max)?
    } else if let Some(longueur) = entete("content-length") {
        let n: usize = longueur
            .parse()
            .map_err(|_| Erreur::Protocole(format!("longueur illisible : {longueur}")))?;
        if n > taille_max {
            return Err(Erreur::Refuse("réponse trop volumineuse".into()));
        }
        if reste.len() < n {
            return Err(Erreur::Reseau("réponse tronquée".into()));
        }
        reste[..n].to_vec()
    } else {
        reste.to_vec()
    };
    if corps.len() > taille_max {
        return Err(Erreur::Refuse("réponse trop volumineuse".into()));
    }
    Ok(Reponse {
        statut,
        type_contenu: entete("content-type").unwrap_or("").to_string(),
        redirection: entete("location").map(str::to_string),
        corps,
    })
}

/// Recompose un corps envoyé par blocs (`Transfer-Encoding: chunked`). Un
/// corps sans son bloc final de taille nulle est tenu pour tronqué.
fn rassembler(mut reste: &[u8], taille_max: usize) -> Resultat<Vec<u8>> {
    let tronque = || Erreur::Reseau("réponse tronquée".into());
    let mut corps = Vec::new();
    loop {
        let fin_ligne = reste.windows(2).position(|w| w == b"\r\n").ok_or_else(tronque)?;
        let ligne = std::str::from_utf8(&reste[..fin_ligne])
            .map_err(|_| Erreur::Protocole("taille de bloc illisible".into()))?;
        let hexa = ligne.split(';').next().unwrap_or("").trim();
        let taille = usize::from_str_radix(hexa, 16)
            .map_err(|_| Erreur::Protocole(format!("taille de bloc illisible : {hexa}")))?;
        reste = &reste[fin_ligne + 2..];
        if taille == 0 {
            return Ok(corps);
        }
        if taille > taille_max || corps.len() + taille > taille_max {
            return Err(Erreur::Refuse("réponse trop volumineuse".into()));
        }
        if reste.len() < taille + 2 {
            return Err(tronque());
        }
        if &reste[taille..taille + 2] != b"\r\n" {
            return Err(Erreur::Protocole("bloc mal terminé".into()));
        }
        corps.extend_from_slice(&reste[..taille]);
        reste = &reste[taille + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fin_de_reponse_reconnue() {
        assert!(!reponse_complete(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n"));
        assert!(!reponse_complete(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nab"));
        assert!(reponse_complete(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nabcd"));
        assert!(!reponse_complete(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nabcd\r\n"));
        assert!(reponse_complete(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nabcd\r\n0\r\n\r\n"));
        assert!(!reponse_complete(b"HTTP/1.1 200 OK\r\n\r\nsans longueur"));
    }

    #[test]
    fn adresses_acceptees() {
        assert_eq!(
            analyser_url("https://Exemple.fr/chemin?a=1#ancre").unwrap(),
            Cible { hote: "exemple.fr".into(), port: 443, chemin: "/chemin?a=1".into() }
        );
        assert_eq!(
            analyser_url("https://exemple.fr:8443").unwrap(),
            Cible { hote: "exemple.fr".into(), port: 8443, chemin: "/".into() }
        );
        assert_eq!(analyser_url("https://exemple.fr?q=1").unwrap().chemin, "/?q=1");
    }

    #[test]
    fn adresses_refusees() {
        for url in [
            "http://exemple.fr/",
            "ftp://exemple.fr/",
            "https://",
            "https://nom:secret@exemple.fr/",
            "https://exemple.fr/un chemin",
            "https://exemple.fr:port/",
            "https://[::1]/",
            "exemple.fr",
        ] {
            assert!(analyser_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn redirections() {
        assert_eq!(resoudre("https://a.fr/x", "/y?z").unwrap(), "https://a.fr/y?z");
        assert_eq!(resoudre("https://a.fr:8443/x", "/y").unwrap(), "https://a.fr:8443/y");
        assert_eq!(resoudre("https://a.fr/x", "https://b.fr/").unwrap(), "https://b.fr/");
        assert!(resoudre("https://a.fr/x", "http://b.fr/").is_err());
        assert!(resoudre("https://a.fr/x", "//b.fr/").is_err());
    }

    #[test]
    fn reponse_a_longueur_annoncee() {
        let r = analyser_reponse(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: 5\r\n\r\nbonjour",
        )
        .unwrap();
        assert_eq!(r.statut, 200);
        assert_eq!(r.type_contenu, "text/xml");
        assert_eq!(r.corps, b"bonjo");
        assert!(analyser_reponse(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nbonjour").is_err());
    }

    #[test]
    fn reponse_par_blocs() {
        let r = analyser_reponse(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nbonj\r\n3;x=y\r\nour\r\n0\r\n\r\n",
        )
        .unwrap();
        assert_eq!(r.corps, b"bonjour");
        // Sans le bloc final : tronquée.
        assert!(analyser_reponse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nbonj\r\n")
            .is_err());
    }

    #[test]
    fn reponse_redirigee_et_refus() {
        let r = analyser_reponse(b"HTTP/1.1 301 Moved\r\nLocation: /ailleurs\r\n\r\n").unwrap();
        assert_eq!(r.statut, 301);
        assert_eq!(r.redirection.as_deref(), Some("/ailleurs"));
        assert!(analyser_reponse(b"SSH-2.0-OpenSSH\r\n\r\n").is_err());
        let enorme = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", TAILLE_MAX + 1);
        assert!(analyser_reponse(enorme.as_bytes()).is_err());
    }
}
