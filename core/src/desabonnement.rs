// SPDX-License-Identifier: GPL-3.0-or-later
//! Désabonnement d'une lettre d'information ou d'une liste de diffusion
//! (0.6.30, suggestion d'un utilisateur) : un bouton « Se désabonner » dans la
//! lecture d'un message qui en offre le moyen.
//!
//! Par ordre de préférence :
//!
//! 1. **En un clic** (RFC 8058) — l'en-tête `List-Unsubscribe-Post:
//!    List-Unsubscribe=One-Click` accompagne une adresse `https:` de
//!    `List-Unsubscribe` : MMail envoie lui-même la demande (un POST, sans
//!    cookie ni identifiant), sans ouvrir de page.
//! 2. **Par l'en-tête** `List-Unsubscribe` (RFC 2369), à défaut : sa première
//!    adresse utilisable, dans l'ordre où l'expéditeur les donne — `mailto:`
//!    prépare un message de désabonnement, `https:` (ou `http:`) ouvre la page
//!    dans le navigateur.
//! 3. **Par le corps**, en dernier recours : un lien dont le texte (à défaut,
//!    l'adresse) parle de désinscription, ouvert dans le navigateur. Jamais
//!    pour une réponse ou un transfert : le lien d'une lettre transférée
//!    désabonnerait celui qui l'a reçue d'abord.
//!
//! Rien ne part à l'affichage du message : l'interface demande toujours une
//! confirmation, qui nomme le serveur ou l'adresse visés. Un message dont le
//! serveur de réception a noté un échec DMARC n'en propose aucun : l'adresse
//! de l'expéditeur y est peut-être usurpée, et se désabonner d'un pourriel lui
//! confirme seulement que l'adresse est lue.

use mail_parser::{Message, MessageParser};
use serde::Serialize;

/// Le moyen de se désabonner que porte un message.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Desabonnement {
    /// « un-clic » : MMail envoie la demande (RFC 8058) ; « courriel » : un
    /// message à préparer ; « lien » : une page à ouvrir dans le navigateur.
    pub methode: &'static str,
    /// « entete » (`List-Unsubscribe`) ou « corps » (lien repéré dans le
    /// message).
    pub source: &'static str,
    /// Adresse visée : `https:` (un clic), `http(s):` (lien) ou `mailto:`.
    pub url: String,
    /// Serveur visé, ou adresse de courriel : ce que la confirmation nomme.
    pub hote: String,
    /// Courriel : destinataire, objet et texte demandés par la liste.
    pub adresse: String,
    pub objet: String,
    pub texte: String,
}

/// Le moyen de se désabonner offert par un message entier, s'il y en a un.
pub fn trouver(brut: &[u8]) -> Option<Desabonnement> {
    if crate::index::dmarc_echoue(brut) {
        return None;
    }
    let message = MessageParser::default().parse(brut)?;
    if let Some(valeur) = message.header_raw("List-Unsubscribe") {
        let un_clic = message.header_raw("List-Unsubscribe-Post").is_some_and(un_clic_annonce);
        if let Some(d) = depuis_l_entete(&uris_de_l_entete(valeur), un_clic) {
            return Some(d);
        }
    }
    if reponse_ou_transfert(&message) {
        return None;
    }
    lien_du_corps(&message)
}

/// Adresses d'un en-tête `List-Unsubscribe` : chacune entre chevrons, séparées
/// par des virgules (RFC 2369). Lignes repliées réunies, blancs ignorés dans
/// une adresse, comme le veut la RFC ; au plus huit.
pub fn uris_de_l_entete(valeur: &str) -> Vec<String> {
    let mut uris = Vec::new();
    let mut reste = valeur;
    while let Some(debut) = reste.find('<') {
        let apres = &reste[debut + 1..];
        let Some(fin) = apres.find('>') else { break };
        let uri: String = apres[..fin].chars().filter(|c| !c.is_whitespace()).collect();
        if !uri.is_empty() {
            uris.push(uri);
        }
        if uris.len() == 8 {
            break;
        }
        reste = &apres[fin + 1..];
    }
    uris
}

/// Vrai si `List-Unsubscribe-Post` annonce le désabonnement en un clic
/// (RFC 8058 : la valeur exacte `List-Unsubscribe=One-Click`).
pub fn un_clic_annonce(valeur: &str) -> bool {
    let compacte: String = valeur.chars().filter(|c| !c.is_whitespace()).collect();
    compacte.eq_ignore_ascii_case("List-Unsubscribe=One-Click")
}

/// Le moyen retenu parmi les adresses de `List-Unsubscribe` : la première
/// `https:` si le désabonnement en un clic est annoncé ; sinon la première
/// adresse utilisable, dans l'ordre de l'expéditeur.
pub fn depuis_l_entete(uris: &[String], un_clic: bool) -> Option<Desabonnement> {
    if un_clic {
        if let Some((url, hote)) = uris.iter().find_map(|u| https_valide(u).map(|h| (u.clone(), h))) {
            return Some(Desabonnement { methode: "un-clic", ..lien("entete", url, hote) });
        }
    }
    uris.iter().find_map(|u| moyen_de_l_adresse(u, "entete"))
}

/// Ce qu'une adresse permet : un courriel (`mailto:`) ou une page
/// (`http(s):`) ; rien pour un autre schéma.
fn moyen_de_l_adresse(uri: &str, source: &'static str) -> Option<Desabonnement> {
    if commence_par(uri, "mailto:") {
        let (adresse, objet, texte) = analyser_mailto(uri)?;
        return Some(Desabonnement {
            methode: "courriel",
            source,
            url: uri.to_string(),
            hote: adresse.clone(),
            adresse,
            objet,
            texte,
        });
    }
    let hote = https_valide(uri).or_else(|| http_valide(uri))?;
    Some(lien(source, uri.to_string(), hote))
}

fn lien(source: &'static str, url: String, hote: String) -> Desabonnement {
    Desabonnement {
        methode: "lien",
        source,
        url,
        hote,
        adresse: String::new(),
        objet: String::new(),
        texte: String::new(),
    }
}

fn commence_par(texte: &str, prefixe: &str) -> bool {
    texte.get(..prefixe.len()).is_some_and(|p| p.eq_ignore_ascii_case(prefixe))
}

/// Serveur d'une adresse `https:` que le client HTTPS de MMail accepte.
fn https_valide(uri: &str) -> Option<String> {
    if !commence_par(uri, "https://") {
        return None;
    }
    let normalisee = format!("https://{}", &uri[8..]);
    crate::http::analyser_url(&normalisee).ok().map(|c| c.hote)
}

/// Serveur d'une adresse `http:` : elle ne s'ouvre que dans le navigateur.
fn http_valide(uri: &str) -> Option<String> {
    if !commence_par(uri, "http://") {
        return None;
    }
    let reste = &uri[7..];
    let autorite = &reste[..reste.find(['/', '?', '#']).unwrap_or(reste.len())];
    let hote = autorite.rsplit_once(':').map(|(h, _)| h).unwrap_or(autorite);
    let valide = !hote.is_empty()
        && !autorite.contains('@')
        && hote.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !uri.chars().any(|c| c.is_control() || c.is_whitespace());
    valide.then(|| hote.to_ascii_lowercase())
}

/// Destinataire, objet et texte d'une adresse `mailto:` (RFC 6068). Le « + »
/// y reste un « + » ; un seul destinataire, le premier.
pub fn analyser_mailto(uri: &str) -> Option<(String, String, String)> {
    if !commence_par(uri, "mailto:") {
        return None;
    }
    let reste = &uri[7..];
    let (destinataires, requete) = reste.split_once('?').unwrap_or((reste, ""));
    let mut adresse = decoder(destinataires.split(',').next().unwrap_or(""))?.trim().to_string();
    let (mut objet, mut texte) = (String::new(), String::new());
    for champ in requete.split('&') {
        let (nom, valeur) = champ.split_once('=').unwrap_or((champ, ""));
        let valeur = decoder(valeur).unwrap_or_default();
        match nom.to_ascii_lowercase().as_str() {
            "subject" => objet = valeur,
            "body" => texte = valeur,
            "to" if adresse.is_empty() => adresse = valeur.split(',').next().unwrap_or("").trim().to_string(),
            _ => {}
        }
    }
    let valide = adresse.contains('@')
        && !adresse.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"'));
    if !valide {
        return None;
    }
    // Ni saut de ligne ni caractère de contrôle dans l'objet : il part dans un
    // en-tête.
    let objet: String = objet.chars().filter(|c| !c.is_control()).collect();
    Some((adresse, objet, texte))
}

/// Décode les « %XX » ; `None` si le résultat n'est pas de l'UTF-8.
fn decoder(texte: &str) -> Option<String> {
    let octets = texte.as_bytes();
    let mut decode = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        if octets[i] == b'%' {
            if let Some(v) = texte.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                decode.push(v);
                i += 3;
                continue;
            }
        }
        decode.push(octets[i]);
        i += 1;
    }
    String::from_utf8(decode).ok()
}

/// Vrai pour une réponse ou un transfert : un objet qui commence par « RE: »,
/// « TR: », « Fwd: »… ou un en-tête `In-Reply-To`.
fn reponse_ou_transfert(message: &Message<'_>) -> bool {
    if message.header_raw("In-Reply-To").is_some_and(|v| !v.trim().is_empty()) {
        return true;
    }
    let objet = message.subject().unwrap_or("").trim_start();
    let Some(deux_points) = objet.char_indices().take(8).find(|(_, c)| *c == ':').map(|(i, _)| i) else {
        return false;
    };
    let prefixe = objet[..deux_points].trim().to_lowercase();
    // Le marqueur, éventuellement suivi d'un compteur : « RE[2] ».
    let prefixe = prefixe.split('[').next().unwrap_or("");
    matches!(prefixe, "re" | "tr" | "fw" | "fwd" | "aw" | "wg" | "rv" | "sv" | "vs" | "enc" | "rif" | "i" | "r")
}

/// Texte de lien qui parle de désinscription, une fois en minuscules et sans
/// accents.
const MOTS_DU_TEXTE: &[&str] = &[
    "desinscri",
    "desabonn",
    "unsubscribe",
    "unsub",
    "opt out",
    "opt-out",
    "ne plus recevoir",
    "abmelden",
    "abbestellen",
    "darse de baja",
    "cancelar suscri",
];

/// Adresse de lien qui parle de désinscription.
const MOTS_DE_L_ADRESSE: &[&str] = &["unsubscribe", "desinscri", "desabonn", "optout", "opt-out", "unsub"];

/// Un lien de désinscription repéré dans le corps : d'abord par le texte du
/// lien, puis par son adresse. Un texte de plus de 80 caractères n'est pas un
/// lien de désinscription mais un bloc entier rendu cliquable.
fn lien_du_corps(message: &Message<'_>) -> Option<Desabonnement> {
    let mut html = String::new();
    let mut texte = String::new();
    for partie in message.html_bodies() {
        if let Some(contenu) = partie.text_contents() {
            if partie.is_text_html() {
                html.push_str(contenu);
            }
        }
    }
    if html.is_empty() {
        for partie in message.text_bodies() {
            if let Some(contenu) = partie.text_contents() {
                texte.push_str(contenu);
                texte.push('\n');
            }
        }
        return lien_du_texte(&texte);
    }
    let liens = liens_html(&html);
    let par_le_texte = liens.iter().find(|(adresse, libelle)| {
        let libelle = normaliser(libelle);
        libelle.chars().count() <= 80
            && MOTS_DU_TEXTE.iter().any(|m| libelle.contains(m))
            && moyen_de_l_adresse(adresse, "corps").is_some()
    });
    let par_l_adresse = || {
        liens.iter().find(|(adresse, _)| {
            let a = normaliser(adresse);
            MOTS_DE_L_ADRESSE.iter().any(|m| a.contains(m)) && moyen_de_l_adresse(adresse, "corps").is_some()
        })
    };
    let (adresse, _) = par_le_texte.or_else(par_l_adresse)?;
    moyen_de_l_adresse(adresse, "corps")
}

/// Liens d'un HTML : adresse (`href`, entités décodées) et texte affiché
/// (balises retirées, entités décodées, blancs résumés). Au plus 2 000, sur
/// les 2 premiers Mo.
pub fn liens_html(html: &str) -> Vec<(String, String)> {
    let mut liens = Vec::new();
    let borne = html.char_indices().nth(2 * 1024 * 1024).map(|(i, _)| i).unwrap_or(html.len());
    let html = &html[..borne];
    let minuscules = html.to_ascii_lowercase();
    let mut position = 0;
    while let Some(trouve) = minuscules[position..].find("<a") {
        let debut = position + trouve;
        position = debut + 2;
        // « <a » suivi d'un blanc ou de « > » : pas « <abbr », « <address ».
        if !minuscules[position..].starts_with(|c: char| c.is_whitespace() || c == '>') {
            continue;
        }
        let Some(fin_balise) = minuscules[position..].find('>').map(|f| position + f) else { break };
        let balise = &html[position..fin_balise];
        let fin_lien = minuscules[fin_balise..].find("</a").map(|f| fin_balise + f).unwrap_or(minuscules.len());
        let interieur = &html[fin_balise + 1..fin_lien.max(fin_balise + 1)];
        position = fin_lien.max(fin_balise + 1);
        let Some(href) = attribut(balise, "href") else { continue };
        let adresse = crate::index::decoder_entites(href.trim());
        let libelle = texte_de(interieur);
        liens.push((adresse, libelle));
        if liens.len() == 2000 {
            break;
        }
    }
    liens
}

/// Valeur d'un attribut d'une balise (entre guillemets, apostrophes ou nue).
fn attribut<'a>(balise: &'a str, nom: &str) -> Option<&'a str> {
    let minuscules = balise.to_ascii_lowercase();
    let mut depart = 0;
    while let Some(i) = minuscules[depart..].find(nom).map(|i| depart + i) {
        depart = i + nom.len();
        // Le nom entier : précédé d'un blanc, suivi de « = ».
        let avant = minuscules[..i].chars().last();
        if !avant.is_some_and(|c| c.is_whitespace()) {
            continue;
        }
        let reste = minuscules[depart..].trim_start();
        if !reste.starts_with('=') {
            continue;
        }
        let decalage = minuscules.len() - reste.len() + 1;
        let valeur = &balise[decalage..];
        let blancs = valeur.len() - valeur.trim_start().len();
        let valeur = &valeur[blancs..];
        return match valeur.chars().next() {
            Some(q @ ('"' | '\'')) => valeur[1..].find(q).map(|f| &valeur[1..1 + f]),
            Some(_) => Some(&valeur[..valeur.find(|c: char| c.is_whitespace()).unwrap_or(valeur.len())]),
            None => None,
        };
    }
    None
}

/// Texte affiché d'un fragment HTML : balises retirées, entités décodées,
/// blancs résumés.
fn texte_de(fragment: &str) -> String {
    let mut texte = String::with_capacity(fragment.len());
    let mut dans_balise = false;
    for c in fragment.chars() {
        match c {
            '<' => dans_balise = true,
            '>' if dans_balise => {
                dans_balise = false;
                texte.push(' ');
            }
            _ if !dans_balise => texte.push(c),
            _ => {}
        }
    }
    crate::index::decoder_entites(&texte).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Un lien de désinscription dans un message en texte : une adresse web sur
/// la ligne qui en parle, ou sur l'une des deux suivantes.
fn lien_du_texte(texte: &str) -> Option<Desabonnement> {
    let lignes: Vec<&str> = texte.lines().collect();
    for (i, ligne) in lignes.iter().enumerate() {
        let normale = normaliser(ligne);
        if !MOTS_DU_TEXTE.iter().any(|m| normale.contains(m)) {
            continue;
        }
        for voisine in lignes.iter().skip(i).take(3) {
            for mot in voisine.split(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '(' | ')' | '"')) {
                if let Some(d) = (commence_par(mot, "https://") || commence_par(mot, "http://"))
                    .then(|| moyen_de_l_adresse(mot.trim_end_matches(['.', ',', ';']), "corps"))
                    .flatten()
                {
                    return Some(d);
                }
            }
        }
    }
    None
}

/// Minuscules, sans accents : « Désinscription » → « desinscription ».
fn normaliser(texte: &str) -> String {
    texte
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// Corps de la demande de désabonnement en un clic (RFC 8058).
pub const CORPS_UN_CLIC: &[u8] = b"List-Unsubscribe=One-Click";

/// Envoie la demande de désabonnement en un clic à `url` (`https:`) : un POST
/// au corps `List-Unsubscribe=One-Click`, sans cookie ni identifiant, comme le
/// veut la RFC 8058. Une réponse 2xx vaut accord ; une redirection aussi —
/// certains serveurs renvoient vers une page de confirmation, que l'on ne suit
/// pas (un POST ne se rejoue pas ailleurs). Bloquant : à appeler hors du fil
/// de l'interface.
pub fn demander(url: &str) -> Result<(), String> {
    let hote = https_valide(url).ok_or_else(|| format!("adresse refusée : {url}"))?;
    let entetes = [("Content-Type", "application/x-www-form-urlencoded".to_string())];
    let demande = crate::http::Demande {
        methode: "POST",
        url,
        accepte: "*/*",
        entetes: &entetes,
        corps: CORPS_UN_CLIC,
        taille_max: 2 * 1024 * 1024,
    };
    match crate::http::envoyer(&demande) {
        Ok(r) if (200..400).contains(&r.statut) => Ok(()),
        Ok(r) => Err(format!("{hote} a répondu {}", r.statut)),
        // Une page de réponse trop lourde : la demande est passée.
        Err(crate::imap::Erreur::Refuse(m)) if m == crate::http::TROP_VOLUMINEUSE => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests;
