// SPDX-License-Identifier: GPL-3.0-or-later
//! Comptes Microsoft (Office 365, Outlook.com) : connexion par OAuth2, envoi
//! par Microsoft Graph.
//!
//! Office 365 n'accepte plus de mot de passe en IMAP : il faut un jeton
//! d'accès, que Microsoft délivre une fois la personne connectée dans son
//! navigateur et son consentement donné. MMail est une application publique —
//! sans secret — inscrite une fois pour toutes dans un annuaire Entra, ouverte
//! à toutes les organisations et aux comptes personnels : rien n'est à inscrire
//! chez le client, qui consent seulement — l'utilisateur lui-même, ou son
//! administrateur une fois pour toute l'organisation (`url_consentement`).
//!
//! Flux « code d'autorisation » avec PKCE (RFC 7636), retour sur une écoute
//! locale `http://localhost:<port>` (RFC 8252 § 7.3) : la connexion se fait
//! dans le navigateur du système, MMail ne voit jamais le mot de passe.
//!
//! Le jeton de rafraîchissement tient lieu de mot de passe : il va au coffre du
//! système, sous la même clé. Les jetons d'accès (une heure) ne vivent qu'en
//! mémoire, un par ressource — la lecture (IMAP) et l'envoi (Graph).
//!
//! L'envoi passe par Graph et non par SMTP : le SMTP authentifié est coupé par
//! défaut dans Office 365, et le rétablir est l'affaire de l'administrateur de
//! chaque organisation. Graph range lui-même la copie dans « Éléments
//! envoyés ». Une requête Graph pèse 4 Mo au plus : un message plus lourd
//! passe par un brouillon, auquel les pièces jointes s'ajoutent à part
//! (`Mail.ReadWrite`), avant d'être envoyé.

use std::time::Duration;

use crate::http::{self, Demande};
#[cfg(test)]
use crate::imap::Erreur;
use crate::oauth::{encoder, Fournisseur};
#[cfg(test)]
use crate::oauth::{base64url, defi_de, parametres, verrouiller};
#[cfg(test)]
use std::io::{Read, Write};
#[cfg(test)]
use std::net::TcpStream;

// Le commun aux comptes OAuth2 (cf. `oauth`), sous les noms que le reste de
// MMail emploie pour Microsoft.
pub use crate::oauth::{
    a_conserver, adresse_id_token, etat, jeton_acces, lire_reponse_jeton, lire_requete, oublier, pkce, retenir,
    xoauth2, Ecoute, ErreurOAuth, Jetons, Partage, Pkce, ReponseJeton, DELAI_CONNEXION,
};

/// Écoute locale du retour de Microsoft : `http://localhost:<port>`.
pub fn ecouter() -> std::io::Result<Ecoute> {
    crate::oauth::ecouter("localhost")
}

/// Attend le retour du navigateur (cf. `oauth::attendre_code`).
pub fn attendre_code(ecoute: &Ecoute, etat: &str, delai: Duration) -> Result<String, ErreurOAuth> {
    crate::oauth::attendre_code(ecoute, etat, delai, "Microsoft")
}

/// Jetons d'un compte Microsoft, partagés entre ses sessions.
pub fn partager(rafraichissement: &str) -> Partage {
    crate::oauth::partager(Fournisseur::Microsoft, rafraichissement)
}

/// Point d'entrée de Microsoft pour toutes les organisations et les comptes
/// personnels : le rafraîchissement des jetons, et la connexion sans adresse.
const AUTORITE: &str = "https://login.microsoftonline.com/common/oauth2/v2.0";

/// Adresse de retour de la connexion sous Android : la page intégrée à MMail
/// (cf. ConnexionIntegree.qml) l'intercepte sans la charger — rien n'y écoute.
/// La même que sur le bureau, déjà déclarée dans l'application Entra « MMail ».
pub const REDIRECTION_ANDROID: &str = "http://localhost";

/// Domaines des comptes personnels de Microsoft (Outlook.com).
const PARTICULIERS: [&str; 5] = ["outlook", "hotmail", "live", "msn", "passport"];

/// Point d'entrée de la connexion pour une adresse. Retour du mainteneur du
/// 18/10 : avec le point d'entrée commun, un téléphone déjà connecté à un
/// compte Microsoft personnel partait sur la connexion des particuliers
/// (login.live.com) pour une adresse d'entreprise. Une adresse d'Outlook.com,
/// Hotmail, Live ou MSN va aux comptes personnels, toute autre aux comptes
/// d'organisation ; sans adresse, le point d'entrée commun.
pub fn autorite(adresse: &str) -> String {
    let domaine = adresse.trim().rsplit_once('@').map(|(_, d)| d.to_ascii_lowercase()).unwrap_or_default();
    let quoi = if domaine.is_empty() {
        "common"
    } else if PARTICULIERS.iter().any(|p| domaine.starts_with(&format!("{p}."))) {
        "consumers"
    } else {
        "organizations"
    };
    format!("https://login.microsoftonline.com/{quoi}/oauth2/v2.0")
}

/// Serveur IMAP d'Office 365 et d'Outlook.com.
pub const HOTE_IMAP: &str = "outlook.office365.com";

/// Lecture : IMAP pour le compte de la personne connectée.
pub const PORTEE_IMAP: &str = "offline_access https://outlook.office.com/IMAP.AccessAsUser.All";

/// Envoi : Graph, `Mail.Send`, et `Mail.ReadWrite` pour le brouillon d'un
/// message trop lourd pour une seule requête. IMAP donne déjà accès à toute
/// la boîte : ce droit n'ouvre rien de plus sur les messages.
pub const PORTEE_ENVOI: &str =
    "offline_access https://graph.microsoft.com/Mail.Send https://graph.microsoft.com/Mail.ReadWrite";

/// Ce que la connexion demande : les deux ressources d'un coup, pour un seul
/// consentement ; `openid email` donnent l'adresse du compte. Le code, lui,
/// ne s'échange que contre un jeton d'une seule ressource (`PORTEE_CODE`).
const PORTEE_AUTORISATION: &str = "offline_access openid email \
     https://outlook.office.com/IMAP.AccessAsUser.All https://graph.microsoft.com/Mail.Send \
     https://graph.microsoft.com/Mail.ReadWrite";
const PORTEE_CODE: &str = "offline_access openid email https://outlook.office.com/IMAP.AccessAsUser.All";

/// Réglages de la boîte (0.9.0) : la liste des catégories d'Outlook
/// (`masterCategories`), lue pour leurs couleurs, écrite quand une catégorie
/// est créée, recolorée ou renommée dans MMail. Demandé à part, quand on
/// relie les catégories à Outlook (décision du mainteneur du 04/11 : chaque
/// autorisation nouvelle à part) : la connexion d'un compte n'en dépend pas,
/// et un refus n'éteint que cette fonction.
pub const PORTEE_REGLAGES: &str = "offline_access https://graph.microsoft.com/MailboxSettings.ReadWrite";
const PORTEE_AUTORISATION_REGLAGES: &str = "offline_access openid https://graph.microsoft.com/MailboxSettings.ReadWrite";

/// Agendas (0.9.0) : lus par Graph (`agenda`), Exchange Online n'offrant pas
/// CalDAV. Demandé à part, comme les réglages, depuis l'agenda : sans lui, un
/// compte Microsoft 365 n'a pas d'agenda dans MMail, rien d'autre ne change.
/// `ReadWrite` dès la lecture : l'écriture suivra sans nouvelle demande.
pub const PORTEE_AGENDA: &str = "offline_access https://graph.microsoft.com/Calendars.ReadWrite";
const PORTEE_AUTORISATION_AGENDA: &str = "offline_access openid https://graph.microsoft.com/Calendars.ReadWrite";

/// Identifiant de l'application « MMail » dans l'annuaire Entra où elle est
/// inscrite (éditeur `mmedia.fr`). Public par nature : application sans
/// secret, que toute page de connexion affiche. Depuis la 0.6.19, celle du
/// tenant partenaire `mmedia67478`, à l'éditeur vérifié « MMEDIA » (MPN
/// 3820658) : Microsoft n'avertit plus d'un éditeur non vérifié.
const MICROSOFT_CLIENT_ID: &str = "6f263490-3351-4167-908b-10d701fc2f1c";

/// L'inscription d'avant la 0.6.19, toujours en place : un compte connecté
/// avec elle garde son jeton de rafraîchissement, que seule elle accepte.
/// Il passe à la nouvelle à sa prochaine reconnexion.
const MICROSOFT_CLIENT_ID_AVANT: &str = "7b6c8863-79ca-42b3-8149-313df37a3993";

/// Graph, pour le compte de la personne connectée.
const GRAPH: &str = "https://graph.microsoft.com/v1.0/me";

pub mod agenda;

/// Corps d'une requête Graph, message en base64 compris : 4 Mo annoncés, sans
/// dire s'il s'agit de millions d'octets ou de Mio — d'où la marge.
const ENVOI_MAX: usize = 3_900_000;

/// Morceau d'une pièce jointe déposée par une session de dépôt : un multiple
/// de 320 Kio, sous la limite d'une requête.
const MORCEAU: usize = 10 * 320 * 1024;

/// Pièce jointe la plus lourde que Graph accepte.
const PIECE_MAX: usize = 150 * 1024 * 1024;

/// Réponse attendue de Graph : un accusé, ou la description d'un message —
/// son corps compris, pour un brouillon.
const REPONSE_MAX: usize = 256 * 1024;
const BROUILLON_MAX: usize = 8 * 1024 * 1024;

/// Identifiant de l'application : `MMAIL_MICROSOFT_CLIENT_ID` s'il est donné
/// (essais, autre inscription), sinon celui du programme. Vide : la connexion
/// Microsoft est indisponible.
pub fn identifiant_client() -> String {
    std::env::var("MMAIL_MICROSOFT_CLIENT_ID")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| MICROSOFT_CLIENT_ID.to_string())
}

pub fn disponible() -> bool {
    !identifiant_client().is_empty()
}

/// Application à qui demander de nouveaux jetons pour un compte :
/// `MMAIL_MICROSOFT_CLIENT_ID` s'il est donné, sinon celle qui a délivré son
/// jeton de rafraîchissement (`memorisee`, cf. `du_coffre`) ; aucune — compte
/// connecté avant la 0.6.19 — c'est l'ancienne inscription.
pub fn client_du_compte(memorisee: &str) -> String {
    if let Some(v) = std::env::var("MMAIL_MICROSOFT_CLIENT_ID").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
        return v;
    }
    if memorisee.is_empty() { MICROSOFT_CLIENT_ID_AVANT.to_string() } else { memorisee.to_string() }
}

/// Ce que le coffre garde d'un compte Microsoft : l'application, une espace,
/// le jeton de rafraîchissement — un jeton ne vaut que pour l'application qui
/// l'a délivré, et aucun ne contient d'espace.
pub fn pour_le_coffre(client: &str, rafraichissement: &str) -> String {
    if client.is_empty() { rafraichissement.to_string() } else { format!("{client} {rafraichissement}") }
}

/// L'inverse de `pour_le_coffre` : (application, jeton). Un jeton seul, écrit
/// avant la 0.6.19, rend une application vide.
pub fn du_coffre(texte: &str) -> (String, String) {
    let texte = texte.trim();
    match texte.split_once(' ') {
        Some((client, jeton)) if est_identifiant(client) => (client.to_string(), jeton.trim().to_string()),
        _ => (String::new(), texte.to_string()),
    }
}

/// Forme d'un identifiant d'application Entra : un GUID.
fn est_identifiant(texte: &str) -> bool {
    let groupes: Vec<&str> = texte.split('-').collect();
    groupes.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12]) && groupes.iter().all(|g| g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Lien que l'administrateur d'une organisation ouvre une fois : MMail est
/// alors autorisé pour tous ses utilisateurs, sans qu'aucun n'ait à consentir.
pub fn url_consentement(client: &str) -> String {
    format!("https://login.microsoftonline.com/organizations/adminconsent?client_id={}", encoder(client))
}

/// Adresse de la page de connexion de Microsoft. `indice` : l'adresse du
/// compte quand on le reconnecte, pour ne pas la redemander.
pub fn url_autorisation(client: &str, redirection: &str, defi: &str, etat: &str, indice: &str) -> String {
    url_autorisation_de(client, redirection, defi, etat, indice, PORTEE_AUTORISATION, "&prompt=select_account")
}

/// Page de Microsoft qui demande l'autorisation de lire les réglages de la
/// boîte d'un compte déjà connecté (`PORTEE_REGLAGES`) : sans choix du
/// compte, désigné par `adresse`.
pub fn url_autorisation_reglages(client: &str, redirection: &str, defi: &str, etat: &str, adresse: &str) -> String {
    url_autorisation_de(client, redirection, defi, etat, adresse, PORTEE_AUTORISATION_REGLAGES, "")
}

/// Page de Microsoft qui demande l'autorisation de lire les agendas de la
/// boîte d'un compte déjà connecté (`PORTEE_AGENDA`).
pub fn url_autorisation_agenda(client: &str, redirection: &str, defi: &str, etat: &str, adresse: &str) -> String {
    url_autorisation_de(client, redirection, defi, etat, adresse, PORTEE_AUTORISATION_AGENDA, "")
}

fn url_autorisation_de(client: &str, redirection: &str, defi: &str, etat: &str, indice: &str, portee: &str, invite: &str) -> String {
    let mut url = format!(
        "{}/authorize?client_id={}&response_type=code&redirect_uri={}&response_mode=query\
         &scope={}&state={}&code_challenge={}&code_challenge_method=S256{invite}",
        autorite(indice),
        encoder(client),
        encoder(redirection),
        encoder(portee),
        encoder(etat),
        encoder(defi),
    );
    if !indice.trim().is_empty() {
        url.push_str(&format!("&login_hint={}", encoder(indice.trim())));
    }
    url
}

/// Échange le code reçu par l'écoute locale contre les premiers jetons.
/// `adresse` : celle de la connexion, pour le même point d'entrée.
pub fn echanger_code(client: &str, code: &str, verificateur: &str, redirection: &str, adresse: &str) -> Result<ReponseJeton, ErreurOAuth> {
    echanger_code_pour(client, code, verificateur, redirection, adresse, PORTEE_CODE)
}

/// Comme `echanger_code`, pour le jeton de lecture des réglages
/// (`PORTEE_REGLAGES`), après l'autorisation demandée à part.
pub fn echanger_code_reglages(client: &str, code: &str, verificateur: &str, redirection: &str, adresse: &str) -> Result<ReponseJeton, ErreurOAuth> {
    echanger_code_pour(client, code, verificateur, redirection, adresse, PORTEE_REGLAGES)
}

/// Comme `echanger_code`, pour le jeton des agendas (`PORTEE_AGENDA`).
pub fn echanger_code_agenda(client: &str, code: &str, verificateur: &str, redirection: &str, adresse: &str) -> Result<ReponseJeton, ErreurOAuth> {
    echanger_code_pour(client, code, verificateur, redirection, adresse, PORTEE_AGENDA)
}

fn echanger_code_pour(
    client: &str,
    code: &str,
    verificateur: &str,
    redirection: &str,
    adresse: &str,
    portee: &str,
) -> Result<ReponseJeton, ErreurOAuth> {
    crate::oauth::demander_jeton(&format!("{}/token", autorite(adresse)), &format!(
        "client_id={}&grant_type=authorization_code&code={}&redirect_uri={}&code_verifier={}&scope={}",
        encoder(client),
        encoder(code),
        encoder(redirection),
        encoder(verificateur),
        encoder(portee),
    ))
}

/// Jeton d'accès à une ressource (`PORTEE_IMAP`, `PORTEE_ENVOI`), par le jeton
/// de rafraîchissement.
pub fn rafraichir(client: &str, rafraichissement: &str, portee: &str) -> Result<ReponseJeton, ErreurOAuth> {
    demander_jeton(&format!(
        "client_id={}&grant_type=refresh_token&refresh_token={}&scope={}",
        encoder(client),
        encoder(rafraichissement),
        encoder(portee),
    ))
}

fn demander_jeton(corps: &str) -> Result<ReponseJeton, ErreurOAuth> {
    crate::oauth::demander_jeton(&format!("{AUTORITE}/token"), corps)
}

// -------------------------------------------------------------------- envoi

/// Ce qui porte les requêtes : `http::envoyer`, ou un faux Graph dans les
/// essais.
type Transport<'a> = dyn FnMut(&Demande) -> crate::imap::Resultat<http::Reponse> + 'a;

/// Envoie un message par Graph : le MIME complet, en base64 (forme documentée
/// de `sendMail`). Graph lit les destinataires dans les en-têtes : ceux de
/// l'enveloppe qui n'y figurent pas — les Cci — y sont ajoutés en `Bcc`, que
/// Microsoft retire à la remise.
///
/// Trop lourd pour une requête, le message passe par un brouillon : créé sans
/// ses plus grosses pièces jointes, complété de celles-ci une à une — par
/// morceaux au-delà de la limite d'une requête —, puis envoyé. Le brouillon
/// d'un envoi qui échoue en route est supprimé.
pub fn envoyer(jeton: &str, octets: &[u8], destinataires: &[String]) -> Result<(), ErreurOAuth> {
    envoyer_par(&mut |d: &Demande| http::envoyer(d), jeton, octets, destinataires)
}

fn envoyer_par(t: &mut Transport, jeton: &str, octets: &[u8], destinataires: &[String]) -> Result<(), ErreurOAuth> {
    let message = avec_cci(octets, destinataires);
    if longueur_base64(message.len()) <= ENVOI_MAX {
        let corps = crate::smtp::base64(&message);
        let r = appeler(t, jeton, "POST", &format!("{GRAPH}/sendMail"), "text/plain", corps.as_bytes(), REPONSE_MAX)?;
        return succes(&r);
    }
    let (reduit, pieces) = alleger(&message)?;
    let corps = crate::smtp::base64(&reduit);
    let r = appeler(t, jeton, "POST", &format!("{GRAPH}/messages"), "text/plain", corps.as_bytes(), BROUILLON_MAX)?;
    let brouillon = reponse_json(&r)?;
    let id = brouillon
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ErreurOAuth::Reseau("brouillon créé sans identifiant".into()))?
        .to_string();
    let adresse = format!("{GRAPH}/messages/{}", encoder(&id));
    let issue = completer(t, jeton, &adresse, &brouillon, destinataires, &pieces);
    if issue.is_err() {
        // Un brouillon à moitié fait n'a rien à faire dans la boîte : le
        // message reste à envoyer dans MMail.
        let _ = appeler(t, jeton, "DELETE", &adresse, "", b"", REPONSE_MAX);
    }
    issue
}

/// Complète le brouillon — Cci manquants, pièces jointes — et l'envoie.
fn completer(
    t: &mut Transport,
    jeton: &str,
    adresse: &str,
    brouillon: &serde_json::Value,
    destinataires: &[String],
    pieces: &[Piece],
) -> Result<(), ErreurOAuth> {
    // Que Graph tire les Cci de l'en-tête `Bcc` d'un brouillon n'est pas
    // documenté : ce que porte le brouillon fait foi, et les manquants sont
    // ajoutés.
    let liste = |cle: &str| -> Vec<String> {
        brouillon
            .get(cle)
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|d| d.pointer("/emailAddress/address").and_then(|a| a.as_str()))
            .map(|a| a.trim().to_lowercase())
            .collect()
    };
    let presents: Vec<String> = ["toRecipients", "ccRecipients", "bccRecipients"].iter().flat_map(|c| liste(c)).collect();
    let manquants: Vec<String> = destinataires
        .iter()
        .map(|d| d.trim().to_lowercase())
        .filter(|d| !presents.contains(d))
        .collect();
    if !manquants.is_empty() {
        let cci: Vec<serde_json::Value> = liste("bccRecipients")
            .into_iter()
            .chain(manquants)
            .map(|a| serde_json::json!({ "emailAddress": { "address": a } }))
            .collect();
        let corps = serde_json::json!({ "bccRecipients": cci }).to_string();
        succes(&appeler(t, jeton, "PATCH", adresse, "application/json", corps.as_bytes(), BROUILLON_MAX)?)?;
    }
    for p in pieces {
        joindre(t, jeton, adresse, p)?;
    }
    succes(&appeler(t, jeton, "POST", &format!("{adresse}/send"), "", b"", REPONSE_MAX)?)
}

/// Ajoute une pièce jointe au brouillon : en une requête si elle y tient,
/// sinon par une session de dépôt, morceau par morceau.
fn joindre(t: &mut Transport, jeton: &str, brouillon: &str, p: &Piece) -> Result<(), ErreurOAuth> {
    let pieces = format!("{brouillon}/attachments");
    if longueur_base64(p.contenu.len()) + p.nom.len() + 1024 <= ENVOI_MAX {
        let corps = serde_json::json!({
            "@odata.type": "#microsoft.graph.fileAttachment",
            "name": p.nom,
            "contentType": p.type_mime,
            "contentBytes": crate::smtp::base64(&p.contenu),
        })
        .to_string();
        return succes(&appeler(t, jeton, "POST", &pieces, "application/json", corps.as_bytes(), REPONSE_MAX)?);
    }
    let corps = serde_json::json!({
        "AttachmentItem": {
            "attachmentType": "file",
            "name": p.nom,
            "size": p.contenu.len(),
            "contentType": p.type_mime,
        }
    })
    .to_string();
    let r = appeler(t, jeton, "POST", &format!("{pieces}/createUploadSession"), "application/json", corps.as_bytes(), REPONSE_MAX)?;
    let session = reponse_json(&r)?;
    let depot = session
        .get("uploadUrl")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErreurOAuth::Reseau("session de dépôt sans adresse".into()))?;
    let total = p.contenu.len();
    for (i, morceau) in p.contenu.chunks(MORCEAU).enumerate() {
        let debut = i * MORCEAU;
        let entetes = [
            ("Content-Type", "application/octet-stream".to_string()),
            ("Content-Range", format!("bytes {debut}-{}/{total}", debut + morceau.len() - 1)),
        ];
        // L'adresse de dépôt porte sa propre autorisation : Graph refuse
        // qu'on y joigne le jeton.
        let r = t(&Demande {
            methode: "PUT",
            url: depot,
            accepte: "application/json",
            entetes: &entetes,
            corps: morceau,
            taille_max: REPONSE_MAX,
        })
        .map_err(|e| ErreurOAuth::Reseau(e.to_string()))?;
        succes(&r)?;
    }
    Ok(())
}

/// Une requête à Graph, avec le jeton.
fn appeler(
    t: &mut Transport,
    jeton: &str,
    methode: &str,
    url: &str,
    type_contenu: &str,
    corps: &[u8],
    taille_max: usize,
) -> Result<http::Reponse, ErreurOAuth> {
    appeler_avec(t, jeton, methode, url, type_contenu, corps, taille_max, &[])
}

/// Comme `appeler`, avec des en-têtes en plus (`Prefer`).
#[allow(clippy::too_many_arguments)]
fn appeler_avec(
    t: &mut Transport,
    jeton: &str,
    methode: &str,
    url: &str,
    type_contenu: &str,
    corps: &[u8],
    taille_max: usize,
    autres: &[(&'static str, String)],
) -> Result<http::Reponse, ErreurOAuth> {
    let mut entetes = vec![("Authorization", format!("Bearer {jeton}"))];
    entetes.extend(autres.iter().cloned());
    if !type_contenu.is_empty() {
        entetes.push(("Content-Type", type_contenu.to_string()));
    }
    t(&Demande { methode, url, accepte: "application/json", entetes: &entetes, corps, taille_max })
        .map_err(|e| ErreurOAuth::Reseau(e.to_string()))
}

fn succes(r: &http::Reponse) -> Result<(), ErreurOAuth> {
    if (200..300).contains(&r.statut) {
        Ok(())
    } else {
        Err(erreur_graph(r.statut, &r.corps))
    }
}

fn reponse_json(r: &http::Reponse) -> Result<serde_json::Value, ErreurOAuth> {
    succes(r)?;
    serde_json::from_slice(&r.corps)
        .map_err(|_| ErreurOAuth::Reseau(format!("réponse illisible de Graph (HTTP {})", r.statut)))
}

fn longueur_base64(n: usize) -> usize {
    n.div_ceil(3) * 4
}

/// Pièce jointe retirée du message, à joindre au brouillon.
#[derive(Debug, Clone, PartialEq)]
struct Piece {
    nom: String,
    type_mime: String,
    contenu: Vec<u8>,
}

/// Retire du message ses plus grosses pièces jointes, jusqu'à ce qu'il tienne
/// dans une requête. Seules les pièces jointes en partent : les images du
/// corps y restent, à leur place.
fn alleger(message: &[u8]) -> Result<(Vec<u8>, Vec<Piece>), ErreurOAuth> {
    use mail_parser::MimeHeaders;
    let trop = |texte: String| ErreurOAuth::refus("taille", &texte);
    let analyse = mail_parser::MessageParser::default()
        .parse(message)
        .ok_or_else(|| ErreurOAuth::refus("message", "message illisible"))?;
    let mut candidats: Vec<(std::ops::Range<usize>, &mail_parser::MessagePart)> = analyse
        .parts
        .iter()
        .skip(1)
        .filter(|p| !p.is_multipart() && p.content_disposition().is_some_and(|d| d.is_attachment()))
        .filter_map(|p| Some((plage(message, p)?, p)))
        .collect();
    candidats.sort_by_key(|(r, _)| std::cmp::Reverse(r.len()));
    let mut reste = message.len();
    let mut retires = Vec::new();
    for (r, p) in candidats {
        if longueur_base64(reste) <= ENVOI_MAX {
            break;
        }
        reste -= r.len();
        let piece = piece(message, p).ok_or_else(|| ErreurOAuth::refus("message", "pièce jointe illisible"))?;
        if piece.contenu.len() > PIECE_MAX {
            return Err(trop(format!(
                "pièce jointe « {} » trop volumineuse pour Microsoft : {} Mo, 150 Mo au plus",
                piece.nom,
                piece.contenu.len().div_ceil(1_048_576)
            )));
        }
        retires.push((r, piece));
    }
    if longueur_base64(reste) > ENVOI_MAX {
        return Err(trop(format!(
            "message trop volumineux pour l'envoi par Microsoft : le texte et les images du corps pèsent {} Mo, 3 Mo au plus",
            reste.div_ceil(1_048_576)
        )));
    }
    retires.sort_by_key(|(r, _)| r.start);
    let mut reduit = Vec::with_capacity(reste);
    let mut position = 0;
    for (r, _) in &retires {
        reduit.extend_from_slice(&message[position..r.start]);
        position = r.end;
    }
    reduit.extend_from_slice(&message[position..]);
    Ok((reduit, retires.into_iter().map(|(_, p)| p).collect()))
}

/// Ce qu'occupe une partie dans le message : du saut de ligne qui précède sa
/// ligne de séparation (`--frontière`) jusqu'à celui qui précède la suivante.
/// La retirer laisse un multipart bien formé.
fn plage(message: &[u8], p: &mail_parser::MessagePart) -> Option<std::ops::Range<usize>> {
    let (tete, corps) = (p.offset_header as usize, p.offset_body as usize);
    let avant = message.get(..tete)?.strip_suffix(b"\n")?;
    let avant = avant.strip_suffix(b"\r").unwrap_or(avant);
    let ligne = avant.iter().rposition(|&c| c == b'\n')? + 1;
    let separation = message[ligne..avant.len()].trim_ascii_end();
    if separation.len() < 3 || !separation.starts_with(b"--") {
        return None;
    }
    let debut = if ligne >= 2 && message[ligne - 2] == b'\r' { ligne - 2 } else { ligne - 1 };
    let mut motif = b"\n".to_vec();
    motif.extend_from_slice(separation);
    let mut depuis = corps;
    let suivante = loop {
        let i = depuis + message.get(depuis..)?.windows(motif.len()).position(|f| f == motif.as_slice())?;
        // La frontière entière, pas le début d'une autre plus longue.
        match message.get(i + motif.len()) {
            None | Some(b'\r' | b'\n' | b'-' | b' ' | b'\t') => break i,
            _ => depuis = i + 1,
        }
    };
    let fin = if suivante > 0 && message[suivante - 1] == b'\r' { suivante - 1 } else { suivante };
    (debut < fin).then_some(debut..fin)
}

/// Nom, type et contenu décodé d'une pièce jointe. Décodée depuis le message
/// brut, et non lue chez l'analyseur : il convertit en UTF-8 le texte d'un
/// fichier, que Microsoft doit recevoir tel quel.
fn piece(message: &[u8], p: &mail_parser::MessagePart) -> Option<Piece> {
    use mail_parser::decoders::{base64::base64_decode, quoted_printable::quoted_printable_decode};
    use mail_parser::{Encoding, MimeHeaders};
    let brut = message.get(p.offset_body as usize..p.offset_end as usize)?;
    let contenu = match p.encoding {
        Encoding::Base64 => base64_decode(brut)?,
        Encoding::QuotedPrintable => quoted_printable_decode(brut)?,
        Encoding::None => brut.to_vec(),
    };
    let nom = p.attachment_name().filter(|n| !n.trim().is_empty()).unwrap_or("piece-jointe").to_string();
    let type_mime = p
        .content_type()
        .map(|t| match t.subtype() {
            Some(s) => format!("{}/{}", t.ctype(), s),
            None => t.ctype().to_string(),
        })
        .unwrap_or_else(|| "application/octet-stream".into());
    Some(Piece { nom, type_mime, contenu })
}

/// Refus de Graph : `{"error": {"code", "message"}}`.
fn erreur_graph(statut: u16, corps: &[u8]) -> ErreurOAuth {
    let json: serde_json::Value = serde_json::from_slice(corps).unwrap_or_default();
    let erreur = json.get("error");
    let texte = |cle: &str| erreur.and_then(|e| e.get(cle)).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let code = texte("code");
    ErreurOAuth::Refus {
        code: if code.is_empty() { format!("HTTP {statut}") } else { code },
        description: texte("message"),
    }
}

// ------------------------------------------------------------- catégories

/// Catégories d'Outlook (0.9.0) : sur un compte Microsoft 365, le serveur IMAP
/// ne garde pas les mots-clés, mais Graph donne et pose les catégories des
/// messages (`categories`, des noms) avec le droit `Mail.ReadWrite` que
/// l'envoi demande déjà. Les couleurs du catalogue d'Outlook
/// (`masterCategories`) demanderaient `MailboxSettings.Read` : pas encore.
///
/// Identifiant Graph du dossier IMAP `chemin` : la boîte de réception par son
/// nom connu, les autres en descendant l'arborescence de Graph segment par
/// segment, d'après les noms (décodés de l'UTF-7 de l'IMAP).
pub fn dossier_graph(jeton: &str, chemin: &str, separateur: &str) -> Result<String, ErreurOAuth> {
    dossier_graph_par(&mut |d: &Demande| http::envoyer(d), jeton, chemin, separateur)
}

fn dossier_graph_par(t: &mut Transport, jeton: &str, chemin: &str, separateur: &str) -> Result<String, ErreurOAuth> {
    let segments: Vec<String> = if separateur.is_empty() || separateur.eq_ignore_ascii_case("NIL") {
        vec![chemin.to_string()]
    } else {
        chemin.split(separateur).map(crate::protocole::decoder_utf7).collect()
    };
    let mut parent: Option<String> = None;
    for (i, nom) in segments.iter().enumerate() {
        // La boîte de réception par son nom connu, mais sous son vrai
        // identifiant : c'est lui que les messages portent dans
        // `parentFolderId`, auquel `poser_categorie` les compare.
        if i == 0 && nom.eq_ignore_ascii_case("INBOX") {
            let dossier = reponse_json(&appeler(t, jeton, "GET", &format!("{GRAPH}/mailFolders/inbox?$select=id"), "", b"", REPONSE_MAX)?)?;
            let id = dossier.get("id").and_then(|v| v.as_str()).filter(|v| !v.is_empty());
            parent = Some(id.ok_or_else(|| ErreurOAuth::refus("DossierInconnu", "boîte de réception sans identifiant dans Graph"))?.to_string());
            continue;
        }
        let url = match &parent {
            None => format!("{GRAPH}/mailFolders?$select=id,displayName&$top=250&includeHiddenFolders=true"),
            Some(id) => format!("{GRAPH}/mailFolders/{}/childFolders?$select=id,displayName&$top=250", encoder(id)),
        };
        let mut suivante = Some(url);
        let mut trouve = None;
        while let (Some(url), None) = (suivante.take(), trouve.as_ref()) {
            let page = reponse_json(&appeler(t, jeton, "GET", &url, "", b"", REPONSE_MAX)?)?;
            trouve = page.get("value").and_then(|v| v.as_array()).and_then(|l| {
                l.iter().find(|f| f.get("displayName").and_then(|n| n.as_str()).is_some_and(|n| n.eq_ignore_ascii_case(nom)))
                    .and_then(|f| f.get("id").and_then(|v| v.as_str()).map(str::to_string))
            });
            suivante = page.get("@odata.nextLink").and_then(|v| v.as_str()).map(str::to_string);
        }
        match trouve {
            Some(id) => parent = Some(id),
            None => return Err(ErreurOAuth::refus("DossierInconnu", &format!("dossier « {chemin} » introuvable dans Graph"))),
        }
    }
    parent.ok_or_else(|| ErreurOAuth::refus("DossierInconnu", "chemin vide"))
}

/// Pages lues au plus par passage (4 000 messages, une dizaine de secondes
/// pendant lesquelles le fil du compte ne fait rien d'autre) : au-delà, la
/// suite viendra au passage suivant — la veille, toutes les deux minutes —
/// à partir du lien rendu.
const PAGES_DELTA: usize = 20;

/// Catégories des messages d'un dossier par la requête « delta » de Graph : la
/// première fois (`lien` vide), tout le dossier ; ensuite, ce qui a changé
/// depuis le lien rendu la fois d'avant. Rend les `(Message-ID, catégories)`
/// vus — un message sans catégorie en rend une liste vide, pour effacer celles
/// qu'il avait — et le lien du prochain passage. Un lien périmé
/// (`SyncStateNotFound`, HTTP 410) fait repartir de zéro.
pub fn categories_delta(jeton: &str, dossier: &str, lien: &str) -> Result<(Vec<(String, Vec<String>)>, String), ErreurOAuth> {
    categories_delta_par(&mut |d: &Demande| http::envoyer(d), jeton, dossier, lien)
}

fn categories_delta_par(
    t: &mut Transport,
    jeton: &str,
    dossier: &str,
    lien: &str,
) -> Result<(Vec<(String, Vec<String>)>, String), ErreurOAuth> {
    let depart = format!("{GRAPH}/mailFolders/{}/messages/delta?$select=internetMessageId,categories", encoder(dossier));
    let mut url = if lien.is_empty() { depart.clone() } else { lien.to_string() };
    let mut vus = Vec::new();
    let mut repris = lien.is_empty();
    for _ in 0..PAGES_DELTA {
        let r = appeler_avec(t, jeton, "GET", &url, "", b"", REPONSE_MAX, &[("Prefer", "odata.maxpagesize=200".into())])?;
        if !repris && (r.statut == 410 || lien_perime(&r)) {
            repris = true;
            vus.clear();
            url = depart.clone();
            continue;
        }
        let page = reponse_json(&r)?;
        for m in page.get("value").and_then(|v| v.as_array()).into_iter().flatten() {
            if m.get("@removed").is_some() {
                continue;
            }
            let Some(id) = m.get("internetMessageId").and_then(|v| v.as_str()).map(sans_chevrons).filter(|v| !v.is_empty()) else {
                continue;
            };
            let noms = m
                .get("categories")
                .and_then(|v| v.as_array())
                .map(|l| l.iter().filter_map(|c| c.as_str()).map(str::to_string).collect())
                .unwrap_or_default();
            vus.push((id.to_string(), noms));
        }
        if let Some(fin) = page.get("@odata.deltaLink").and_then(|v| v.as_str()) {
            return Ok((vus, fin.to_string()));
        }
        match page.get("@odata.nextLink").and_then(|v| v.as_str()) {
            Some(suite) => url = suite.to_string(),
            None => return Ok((vus, String::new())),
        }
    }
    // Dossier très gros : le reste au passage suivant, depuis cette page.
    Ok((vus, url))
}

/// Un `Message-ID` sans ses chevrons, comme l'index le garde : Graph les
/// écrit (`internetMessageId` : « <id@domaine> »), l'enveloppe IMAP non.
pub(crate) fn sans_chevrons(id: &str) -> &str {
    id.trim().trim_start_matches('<').trim_end_matches('>').trim()
}

fn lien_perime(r: &http::Reponse) -> bool {
    if (200..300).contains(&r.statut) {
        return false;
    }
    let code = match erreur_graph(r.statut, &r.corps) {
        ErreurOAuth::Refus { code, .. } => code.to_ascii_lowercase(),
        _ => String::new(),
    };
    code.contains("syncstate") || code.contains("resyncrequired")
}

/// Pose ou retire la catégorie `nom` sur le message de `Message-ID`
/// `message_id`, dans le dossier Graph `dossier` s'il est donné (le même
/// message peut être en deux exemplaires, ailleurs). Rend combien de messages
/// ont changé.
pub fn poser_categorie(jeton: &str, message_id: &str, dossier: Option<&str>, nom: &str, present: bool) -> Result<usize, ErreurOAuth> {
    poser_categorie_par(&mut |d: &Demande| http::envoyer(d), jeton, message_id, dossier, nom, present)
}

fn poser_categorie_par(
    t: &mut Transport,
    jeton: &str,
    message_id: &str,
    dossier: Option<&str>,
    nom: &str,
    present: bool,
) -> Result<usize, ErreurOAuth> {
    // Graph écrit l'identifiant entre chevrons, l'index sans. OData : une
    // apostrophe se double dans une chaîne.
    let filtre = format!("internetMessageId eq '<{}>'", sans_chevrons(message_id).replace('\'', "''"));
    let url = format!("{GRAPH}/messages?$filter={}&$select=id,categories,parentFolderId&$top=10", encoder(&filtre));
    let trouves = reponse_json(&appeler(t, jeton, "GET", &url, "", b"", REPONSE_MAX)?)?;
    let mut changes = 0;
    for m in trouves.get("value").and_then(|v| v.as_array()).into_iter().flatten() {
        if let Some(d) = dossier {
            if m.get("parentFolderId").and_then(|v| v.as_str()) != Some(d) {
                continue;
            }
        }
        let Some(id) = m.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let actuelles: Vec<String> = m
            .get("categories")
            .and_then(|v| v.as_array())
            .map(|l| l.iter().filter_map(|c| c.as_str()).map(str::to_string).collect())
            .unwrap_or_default();
        let deja = actuelles.iter().any(|c| c.eq_ignore_ascii_case(nom));
        if deja == present {
            continue;
        }
        let mut nouvelles: Vec<String> = actuelles.into_iter().filter(|c| !c.eq_ignore_ascii_case(nom)).collect();
        if present {
            nouvelles.push(nom.to_string());
        }
        let corps = serde_json::json!({ "categories": nouvelles }).to_string();
        let r = appeler(t, jeton, "PATCH", &format!("{GRAPH}/messages/{}", encoder(id)), "application/json", corps.as_bytes(), REPONSE_MAX)?;
        succes(&r)?;
        changes += 1;
    }
    Ok(changes)
}

/// Renomme une catégorie sur tous les messages de la boîte qui la portent,
/// tous dossiers confondus (0.9.0) : ce que fait Outlook quand on renomme une
/// catégorie. Rend le nombre de messages changés.
pub fn renommer_categorie(jeton: &str, ancien: &str, nouveau: &str) -> Result<usize, ErreurOAuth> {
    renommer_categorie_par(&mut |d: &Demande| http::envoyer(d), jeton, ancien, nouveau)
}

/// Pages lues au plus pour un renommage (100 messages chacune).
const PAGES_RENOMMAGE: usize = 500;

fn renommer_categorie_par(t: &mut Transport, jeton: &str, ancien: &str, nouveau: &str) -> Result<usize, ErreurOAuth> {
    let filtre = format!("categories/any(c:c eq '{}')", ancien.replace('\'', "''"));
    let mut url = format!("{GRAPH}/messages?$filter={}&$select=id,categories&$top=100", encoder(&filtre));
    // Tout relever d'abord : changer les messages pendant la lecture
    // déplacerait les pages suivantes, qui portent sur le même filtre.
    let mut a_changer: Vec<(String, Vec<String>)> = Vec::new();
    for _ in 0..PAGES_RENOMMAGE {
        let page = reponse_json(&appeler(t, jeton, "GET", &url, "", b"", REPONSE_MAX)?)?;
        for m in page.get("value").and_then(|v| v.as_array()).into_iter().flatten() {
            let Some(id) = m.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let actuelles: Vec<String> = m
                .get("categories")
                .and_then(|v| v.as_array())
                .map(|l| l.iter().filter_map(|c| c.as_str()).map(str::to_string).collect())
                .unwrap_or_default();
            a_changer.push((id.to_string(), actuelles));
        }
        match page.get("@odata.nextLink").and_then(|v| v.as_str()) {
            Some(suivant) => url = suivant.to_string(),
            None => break,
        }
    }
    let mut changes = 0;
    for (id, actuelles) in a_changer {
        let mut nouvelles: Vec<String> = Vec::new();
        for c in actuelles {
            let c = if c.eq_ignore_ascii_case(ancien) { nouveau.to_string() } else { c };
            if !nouvelles.iter().any(|n| n.eq_ignore_ascii_case(&c)) {
                nouvelles.push(c);
            }
        }
        let corps = serde_json::json!({ "categories": nouvelles }).to_string();
        let r = appeler(t, jeton, "PATCH", &format!("{GRAPH}/messages/{}", encoder(&id)), "application/json", corps.as_bytes(), REPONSE_MAX)?;
        succes(&r)?;
        changes += 1;
    }
    Ok(changes)
}

/// Couleur d'une catégorie d'Outlook (`preset0` à `preset24`, Graph
/// `categoryColor`) : les teintes nommées par Microsoft — rouge, orange,
/// marron… puis leurs versions foncées —, rendues par les couleurs de la
/// palette Fluent qui les portent. `none` et l'inconnu : rien, la catégorie
/// garde sa couleur.
pub fn couleur_outlook(preset: &str) -> Option<&'static str> {
    let n: usize = preset.strip_prefix("preset")?.parse().ok()?;
    COULEURS_OUTLOOK.get(n).copied()
}

/// Couleurs des `preset0` à `preset24` d'Outlook (cf. `couleur_outlook`).
const COULEURS_OUTLOOK: [&str; 25] = [
        "#E81123", // rouge
        "#F7630C", // orange
        "#8E562E", // marron
        "#FFB900", // jaune
        "#13A10E", // vert
        "#00B7C3", // sarcelle
        "#7E8C3D", // olive
        "#0078D4", // bleu
        "#8764B8", // violet
        "#C30052", // canneberge
        "#69797E", // acier
        "#4A5459", // acier foncé
        "#A0A0A0", // gris
        "#6E6E6E", // gris foncé
        "#1F1F1F", // noir
        "#A4262C", // rouge foncé
        "#CA5010", // orange foncé
        "#603D30", // marron foncé
        "#C19C00", // jaune foncé
        "#107C10", // vert foncé
        "#038387", // sarcelle foncée
        "#5C6B2E", // olive foncé
        "#004E8C", // bleu foncé
        "#5C2E91", // violet foncé
        "#77004D", // canneberge foncée
    ];

/// La couleur d'Outlook (`presetN`) la plus proche d'une couleur de MMail
/// (`#RRGGBB`) : Outlook n'en connaît que vingt-cinq. Distance pondérée selon
/// la sensibilité de l'œil (rouge 2, vert 4, bleu 3). Illisible : `None`.
pub fn preset_proche(couleur: &str) -> Option<String> {
    let rgb = |h: &str| -> Option<[i64; 3]> {
        let h = h.strip_prefix('#')?;
        if h.len() != 6 || !h.is_ascii() {
            return None;
        }
        let c = |i: usize| i64::from_str_radix(&h[i..i + 2], 16).ok();
        Some([c(0)?, c(2)?, c(4)?])
    };
    let cible = rgb(couleur)?;
    COULEURS_OUTLOOK
        .iter()
        .enumerate()
        .filter_map(|(n, c)| rgb(c).map(|v| (n, v)))
        .min_by_key(|(_, v)| {
            let d = |i: usize| (v[i] - cible[i]).pow(2);
            2 * d(0) + 4 * d(1) + 3 * d(2)
        })
        .map(|(n, _)| format!("preset{n}"))
}

/// Une catégorie de la liste d'Outlook : identifiant Graph, nom, couleur
/// (`presetN`, ou `none`).
#[derive(Debug, Clone, PartialEq)]
struct CategorieOutlook {
    id: String,
    nom: String,
    preset: String,
}

fn liste_outlook_par(t: &mut Transport, jeton: &str) -> Result<Vec<CategorieOutlook>, ErreurOAuth> {
    let liste = reponse_json(&appeler(t, jeton, "GET", &format!("{GRAPH}/outlook/masterCategories"), "", b"", REPONSE_MAX)?)?;
    Ok(liste
        .get("value")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let texte = |cle: &str| c.get(cle).and_then(|v| v.as_str()).map(str::to_string);
            Some(CategorieOutlook { id: texte("id")?, nom: texte("displayName")?, preset: texte("color").unwrap_or_default() })
        })
        .collect())
}

/// Reporte dans la liste d'Outlook un changement fait au catalogue de MMail
/// (0.9.0) : la catégorie `avant` et `apres` le changement, (nom, couleur) —
/// couleur `None` quand MMail n'en sait rien (gris d'attente). Créée ou
/// recolorée dans MMail, elle est créée ou recolorée dans Outlook. Renommée :
/// Graph ne renomme pas une catégorie de la liste, la nouvelle y est donc
/// créée et l'ancienne retirée (les messages, eux, se renomment à part,
/// `renommer_categorie`). Retirée de MMail, elle est retirée de la liste
/// d'Outlook, comme Outlook le fait quand on y supprime une catégorie : les
/// messages gardent son nom (décision du mainteneur du 06/11). Rend le nombre
/// d'écritures faites.
pub fn reporter_categorie(jeton: &str, avant: Option<(&str, Option<&str>)>, apres: Option<(&str, Option<&str>)>) -> Result<usize, ErreurOAuth> {
    reporter_categorie_par(&mut |d: &Demande| http::envoyer(d), jeton, avant, apres)
}

fn reporter_categorie_par(
    t: &mut Transport,
    jeton: &str,
    avant: Option<(&str, Option<&str>)>,
    apres: Option<(&str, Option<&str>)>,
) -> Result<usize, ErreurOAuth> {
    let Some((nom, couleur)) = apres else {
        // Retirée de MMail : retirée aussi de la liste d'Outlook, comme
        // Outlook le fait quand on y supprime une catégorie ; les messages
        // gardent son nom.
        let Some((ancien, _)) = avant else {
            return Ok(0);
        };
        let liste = liste_outlook_par(t, jeton)?;
        return match liste.iter().find(|c| c.nom.trim().eq_ignore_ascii_case(ancien.trim())) {
            Some(c) => {
                let url = format!("{GRAPH}/outlook/masterCategories/{}", encoder(&c.id));
                succes(&appeler(t, jeton, "DELETE", &url, "", b"", REPONSE_MAX)?)?;
                Ok(1)
            }
            None => Ok(0),
        };
    };
    let nom = nom.trim();
    if nom.is_empty() {
        return Ok(0);
    }
    let liste = liste_outlook_par(t, jeton)?;
    let trouver = |n: &str| liste.iter().find(|c| c.nom.trim().eq_ignore_ascii_case(n.trim())).cloned();
    let preset = couleur.and_then(preset_proche);
    let mut ecritures = 0;
    match trouver(nom) {
        Some(c) => {
            if let Some(preset) = preset.filter(|p| *p != c.preset) {
                let url = format!("{GRAPH}/outlook/masterCategories/{}", encoder(&c.id));
                let corps = serde_json::json!({ "color": preset }).to_string();
                succes(&appeler(t, jeton, "PATCH", &url, "application/json", corps.as_bytes(), REPONSE_MAX)?)?;
                ecritures += 1;
            }
        }
        None => {
            let corps = serde_json::json!({ "displayName": nom, "color": preset.as_deref().unwrap_or("none") }).to_string();
            let url = format!("{GRAPH}/outlook/masterCategories");
            succes(&appeler(t, jeton, "POST", &url, "application/json", corps.as_bytes(), REPONSE_MAX)?)?;
            ecritures += 1;
        }
    }
    if let Some((ancien, _)) = avant {
        if !ancien.trim().eq_ignore_ascii_case(nom) {
            if let Some(c) = trouver(ancien) {
                let url = format!("{GRAPH}/outlook/masterCategories/{}", encoder(&c.id));
                succes(&appeler(t, jeton, "DELETE", &url, "", b"", REPONSE_MAX)?)?;
                ecritures += 1;
            }
        }
    }
    Ok(ecritures)
}

/// Ajoute à la liste d'Outlook les catégories `voulues` (nom, couleur) qui
/// n'y sont pas, au moment où l'on relie les catégories de MMail à Outlook
/// (0.9.0). Rend le nombre de catégories ajoutées.
pub fn completer_liste(jeton: &str, voulues: &[(String, Option<String>)]) -> Result<usize, ErreurOAuth> {
    completer_liste_par(&mut |d: &Demande| http::envoyer(d), jeton, voulues)
}

fn completer_liste_par(t: &mut Transport, jeton: &str, voulues: &[(String, Option<String>)]) -> Result<usize, ErreurOAuth> {
    if voulues.is_empty() {
        return Ok(0);
    }
    let liste = liste_outlook_par(t, jeton)?;
    let mut ajoutees = 0;
    let mut vues: Vec<String> = liste.iter().map(|c| c.nom.trim().to_lowercase()).collect();
    for (nom, couleur) in voulues {
        let nom = nom.trim();
        if nom.is_empty() || vues.contains(&nom.to_lowercase()) {
            continue;
        }
        let preset = couleur.as_deref().and_then(preset_proche);
        let corps = serde_json::json!({ "displayName": nom, "color": preset.as_deref().unwrap_or("none") }).to_string();
        succes(&appeler(t, jeton, "POST", &format!("{GRAPH}/outlook/masterCategories"), "application/json", corps.as_bytes(), REPONSE_MAX)?)?;
        vues.push(nom.to_lowercase());
        ajoutees += 1;
    }
    Ok(ajoutees)
}

/// Les catégories de la liste d'Outlook (`masterCategories`) et leur couleur,
/// pour celles qui en ont une : `(nom, #RRGGBB)`. Demande `PORTEE_REGLAGES`.
pub fn couleurs_categories(jeton: &str) -> Result<Vec<(String, String)>, ErreurOAuth> {
    couleurs_categories_par(&mut |d: &Demande| http::envoyer(d), jeton)
}

fn couleurs_categories_par(t: &mut Transport, jeton: &str) -> Result<Vec<(String, String)>, ErreurOAuth> {
    Ok(liste_outlook_par(t, jeton)?
        .into_iter()
        .filter_map(|c| {
            let nom = c.nom.trim();
            let couleur = couleur_outlook(&c.preset)?;
            (!nom.is_empty()).then(|| (nom.to_string(), couleur.to_string()))
        })
        .collect())
}

/// Le message, avec un en-tête `Bcc` pour les destinataires de l'enveloppe
/// absents de `To` et `Cc`.
pub fn avec_cci(octets: &[u8], destinataires: &[String]) -> Vec<u8> {
    let message = mail_parser::MessageParser::default().parse_headers(octets);
    let mut presents: Vec<String> = Vec::new();
    if let Some(m) = &message {
        for liste in [m.to(), m.cc()].into_iter().flatten() {
            presents.extend(liste.iter().filter_map(|a| a.address()).map(|a| a.to_lowercase()));
        }
    }
    let caches: Vec<&String> =
        destinataires.iter().filter(|d| !presents.contains(&d.trim().to_lowercase())).collect();
    if caches.is_empty() {
        return octets.to_vec();
    }
    let liste: Vec<String> = caches.iter().map(|d| d.trim().to_string()).collect();
    let mut sortie = format!("Bcc: {}\r\n", liste.join(", ")).into_bytes();
    sortie.extend_from_slice(octets);
    sortie
}

// ---------------------------------------- règles de la boîte de réception

/// Début du nom des règles d'Outlook que MMail tient (0.9.0, lot 3) : il ne
/// touche jamais aux autres, celles qu'on a créées dans Outlook.
pub const PREFIXE_REGLE: &str = "MMail : ";

/// Numéro d'ordre de la première règle de MMail : après celles d'Outlook, qui
/// commencent à 1.
const SEQUENCE_REGLES: usize = 100;

/// Une règle d'Outlook tirée d'un filtre de MMail : son nom, son corps pour
/// Graph et, s'il déplace, le dossier (chemin IMAP, ou `indesirables`) dont
/// Graph veut l'identifiant.
#[derive(Debug, Clone, PartialEq)]
pub struct Regle {
    pub nom: String,
    pub corps: serde_json::Value,
    pub dossier: Option<String>,
}

/// Une condition de MMail en condition de règle d'Outlook (`messageRulePredicates`),
/// ou la raison pour laquelle Outlook ne sait pas la faire.
fn predicat(c: &crate::filtres::Condition) -> Result<(&'static str, serde_json::Value), String> {
    let valeur = c.valeur.trim();
    match (c.champ.as_str(), c.operateur.as_str()) {
        (_, "commence") => Err("Outlook ne sait pas « commence par »".into()),
        (_, "finit") => Err("Outlook ne sait pas « se termine par »".into()),
        ("a" | "cc", _) => Err("Outlook ne distingue pas « À » de « Cc » : choisissez « À ou Cc »".into()),
        ("expediteur", "contient") => Ok(("senderContains", valeur.into())),
        ("expediteur", "est") if valeur.contains('@') => {
            Ok(("fromAddresses", serde_json::json!({ "emailAddress": { "address": valeur } })))
        }
        ("expediteur", "est") => Err("Outlook ne compare exactement qu'une adresse d'expéditeur, pas un nom".into()),
        ("objet", "contient") => Ok(("subjectContains", valeur.into())),
        ("destinataires", "contient") => Ok(("recipientContains", valeur.into())),
        (_, "est") => Err("Outlook ne sait pas « est » sur ce champ : prenez « contient »".into()),
        (champ, operateur) => Err(format!("condition inconnue : {champ} {operateur}")),
    }
}

/// Traduit un filtre de MMail en règle d'Outlook pour la boîte `adresse`, ou
/// dit pourquoi Outlook ne sait pas faire la même chose — le filtre reste
/// alors appliqué par MMail seul. `adresse` vide : la seule traduction, sans
/// le contrôle de la boîte de destination.
pub fn regle_de(f: &crate::filtres::Filtre, adresse: &str) -> Result<Regle, String> {
    if f.conditions.is_empty() {
        return Err("un filtre sans condition ne part pas dans Outlook".into());
    }
    // Plusieurs valeurs d'une même condition d'Outlook : l'une ou l'autre ;
    // des conditions différentes : toutes.
    let mut conditions = serde_json::Map::new();
    for c in &f.conditions {
        let (cle, valeur) = predicat(c)?;
        let liste = conditions.entry(cle).or_insert_with(|| serde_json::Value::Array(Vec::new()));
        if let Some(l) = liste.as_array_mut() {
            l.push(valeur);
        }
    }
    let repetee = conditions.values().any(|v| v.as_array().is_some_and(|l| l.len() > 1));
    if f.tout && repetee {
        return Err("Outlook ne sait pas exiger deux conditions sur le même champ".into());
    }
    if !f.tout && conditions.len() > 1 {
        return Err("Outlook ne sait pas « une au moins » entre des champs différents".into());
    }
    if f.suivi {
        return Err("les règles d'Outlook ne savent pas mettre un drapeau de suivi".into());
    }
    let ailleurs = !f.compte_destination.trim().is_empty()
        && (adresse.is_empty() || !f.compte_destination.trim().eq_ignore_ascii_case(adresse.trim()));
    if f.destination == "dossier" && ailleurs {
        return Err("une règle d'Outlook ne déplace que vers un dossier de sa boîte".into());
    }
    let mut actions = serde_json::Map::new();
    if f.lu {
        actions.insert("markAsRead".into(), true.into());
    }
    let dossier = match f.destination.as_str() {
        "dossier" if f.dossier.trim().is_empty() => return Err("dossier de destination à choisir".into()),
        "dossier" => Some(f.dossier.clone()),
        "indesirables" => Some("indesirables".to_string()),
        "corbeille" => {
            actions.insert("delete".into(), true.into());
            None
        }
        _ => None,
    };
    if f.arreter {
        actions.insert("stopProcessingRules".into(), true.into());
    }
    if actions.is_empty() && dossier.is_none() {
        return Err("un filtre sans action ne part pas dans Outlook".into());
    }
    let nom = f.nom.trim();
    Ok(Regle {
        nom: format!("{PREFIXE_REGLE}{}", if nom.is_empty() { "filtre" } else { nom }),
        corps: serde_json::json!({ "isEnabled": f.actif, "conditions": conditions, "actions": actions }),
        dossier,
    })
}

/// Les règles d'Outlook voulues pour la boîte `adresse` : ses filtres marqués
/// « aussi dans Outlook », dans l'ordre (le numéro d'ordre suit), avec un nom
/// distinct pour chacun ; et, à part, les filtres qu'Outlook ne sait pas
/// faire, avec la raison.
pub fn regles_voulues(filtres: &[crate::filtres::Filtre], adresse: &str) -> (Vec<Regle>, Vec<(String, String)>) {
    let mut regles: Vec<Regle> = Vec::new();
    let mut refus = Vec::new();
    for f in filtres {
        let visee = f.compte.trim().is_empty() || f.compte.trim().eq_ignore_ascii_case(adresse.trim());
        if !f.serveur || !visee {
            continue;
        }
        match regle_de(f, adresse) {
            Ok(mut r) => {
                let base = r.nom.clone();
                let mut n = 2;
                while regles.iter().any(|x| x.nom.eq_ignore_ascii_case(&r.nom)) {
                    r.nom = format!("{base} ({n})");
                    n += 1;
                }
                r.corps["displayName"] = r.nom.clone().into();
                r.corps["sequence"] = (SEQUENCE_REGLES + regles.len()).into();
                regles.push(r);
            }
            Err(raison) => refus.push((f.nom.clone(), raison)),
        }
    }
    (regles, refus)
}

/// Accorde les règles de la boîte de réception d'Outlook aux règles voulues
/// (`regles_voulues`) : créées, mises à jour, et retirées quand le filtre ne
/// part plus dans Outlook — seulement celles que MMail tient (`PREFIXE_REGLE`).
/// `separateur` : celui des chemins IMAP de la boîte. Rend le nombre
/// d'écritures faites.
pub fn accorder_regles(jeton: &str, voulues: &[Regle], separateur: &str) -> Result<usize, ErreurOAuth> {
    accorder_regles_par(&mut |d: &Demande| http::envoyer(d), jeton, voulues, separateur)
}

fn accorder_regles_par(t: &mut Transport, jeton: &str, voulues: &[Regle], separateur: &str) -> Result<usize, ErreurOAuth> {
    let base = format!("{GRAPH}/mailFolders/inbox/messageRules");
    let mut tenues: Vec<(String, String)> = Vec::new();
    let mut suivante = Some(format!("{base}?$select=id,displayName"));
    while let Some(url) = suivante.take() {
        let page = reponse_json(&appeler(t, jeton, "GET", &url, "", b"", REPONSE_MAX)?)?;
        for r in page.get("value").and_then(|v| v.as_array()).into_iter().flatten() {
            let texte = |cle: &str| r.get(cle).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let nom = texte("displayName");
            if nom.starts_with(PREFIXE_REGLE) {
                tenues.push((texte("id"), nom));
            }
        }
        suivante = page.get("@odata.nextLink").and_then(|v| v.as_str()).map(str::to_string);
    }
    // Tous les dossiers d'abord : un dossier introuvable n'a encore rien
    // changé dans Outlook.
    let mut corps = Vec::new();
    for r in voulues {
        let mut c = r.corps.clone();
        if let Some(chemin) = &r.dossier {
            let id = if chemin == "indesirables" {
                let d = reponse_json(&appeler(t, jeton, "GET", &format!("{GRAPH}/mailFolders/junkemail?$select=id"), "", b"", REPONSE_MAX)?)?;
                d.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string()
            } else {
                dossier_graph_par(t, jeton, chemin, separateur)?
            };
            c["actions"]["moveToFolder"] = id.into();
        }
        corps.push((r.nom.clone(), c.to_string()));
    }
    let mut ecritures = 0;
    for (nom, c) in &corps {
        match tenues.iter().find(|(_, n)| n.eq_ignore_ascii_case(nom)) {
            Some((id, _)) => succes(&appeler(t, jeton, "PATCH", &format!("{base}/{}", encoder(id)), "application/json", c.as_bytes(), REPONSE_MAX)?)?,
            None => succes(&appeler(t, jeton, "POST", &base, "application/json", c.as_bytes(), REPONSE_MAX)?)?,
        }
        ecritures += 1;
    }
    for (id, nom) in &tenues {
        if !corps.iter().any(|(n, _)| n.eq_ignore_ascii_case(nom)) {
            succes(&appeler(t, jeton, "DELETE", &format!("{base}/{}", encoder(id)), "", b"", REPONSE_MAX)?)?;
            ecritures += 1;
        }
    }
    Ok(ecritures)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defi_pkce_de_la_rfc() {
        // RFC 7636, annexe B.
        assert_eq!(
            defi_de("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let p = pkce().unwrap();
        assert_eq!(p.verificateur.len(), 43, "32 octets en base64url");
        assert!(p.verificateur.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
        assert_eq!(p.defi, defi_de(&p.verificateur));
        assert_ne!(etat().unwrap(), etat().unwrap());
        // Une panne passagère n'est pas un refus de la personne.
        let busy = ErreurOAuth::refus("temporarily_unavailable", "");
        assert!(matches!(busy.vers_erreur("Microsoft"), crate::imap::Erreur::Reseau(_)));
        let refus = ErreurOAuth::refus("invalid_grant", "AADSTS70008");
        assert!(matches!(refus.vers_erreur("Microsoft"), crate::imap::Erreur::Refuse(_)));
    }

    #[test]
    fn adresse_de_connexion() {
        let url = url_autorisation("abc-123", "http://localhost:4321", "DEFI", "ETAT", "");
        assert!(url.starts_with("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?client_id=abc-123&"));
        assert!(url.contains("&response_type=code&"));
        assert!(url.contains("&redirect_uri=http%3A%2F%2Flocalhost%3A4321&"));
        assert!(url.contains("&state=ETAT&code_challenge=DEFI&code_challenge_method=S256&prompt=select_account"));
        assert!(url.contains(
            "&scope=offline_access%20openid%20email%20https%3A%2F%2Foutlook.office.com%2FIMAP.AccessAsUser.All%20\
             https%3A%2F%2Fgraph.microsoft.com%2FMail.Send%20https%3A%2F%2Fgraph.microsoft.com%2FMail.ReadWrite&"
        ));
        assert!(!url.contains("login_hint"));
        let url = url_autorisation("abc", "http://localhost:1", "d", "e", " jean@exemple.fr ");
        assert!(url.starts_with("https://login.microsoftonline.com/organizations/oauth2/v2.0/authorize?"));
        assert!(url.ends_with("&login_hint=jean%40exemple.fr"));
        // Points d'entrée : organisation, particuliers, commun.
        assert_eq!(autorite("fabienne.velozo@pollenergie.fr"), "https://login.microsoftonline.com/organizations/oauth2/v2.0");
        assert_eq!(autorite("Paul@Hotmail.fr"), "https://login.microsoftonline.com/consumers/oauth2/v2.0");
        assert_eq!(autorite("p@outlook.com"), "https://login.microsoftonline.com/consumers/oauth2/v2.0");
        assert_eq!(autorite("p@live.fr"), "https://login.microsoftonline.com/consumers/oauth2/v2.0");
        assert_eq!(autorite("p@outlookfr.com"), "https://login.microsoftonline.com/organizations/oauth2/v2.0");
        assert_eq!(autorite(""), "https://login.microsoftonline.com/common/oauth2/v2.0");
        assert_eq!(
            url_consentement("abc-123"),
            "https://login.microsoftonline.com/organizations/adminconsent?client_id=abc-123"
        );
    }

    #[test]
    fn chaine_xoauth2() {
        let attendu = crate::smtp::base64(b"user=jean@exemple.fr\x01auth=Bearer JETON\x01\x01");
        assert_eq!(xoauth2("jean@exemple.fr", "JETON"), attendu);
        assert_eq!(
            crate::rendu::base64(&xoauth2("a@b.fr", "t")).unwrap(),
            b"user=a@b.fr\x01auth=Bearer t\x01\x01"
        );
    }

    #[test]
    fn reponses_du_point_de_jeton() {
        let r = lire_reponse_jeton(
            200,
            br#"{"token_type":"Bearer","scope":"x","expires_in":3599,"access_token":"AT","refresh_token":"RT","id_token":"a.b.c"}"#,
        )
        .unwrap();
        assert_eq!(r.acces, "AT");
        assert_eq!(r.rafraichissement.as_deref(), Some("RT"));
        assert_eq!(r.duree, 3599);
        assert_eq!(r.id_token.as_deref(), Some("a.b.c"));
        let r = lire_reponse_jeton(200, br#"{"access_token":"AT","expires_in":"60"}"#).unwrap();
        assert_eq!((r.rafraichissement, r.duree), (None, 60));

        let e = lire_reponse_jeton(
            400,
            br#"{"error":"invalid_grant","error_description":"AADSTS65001: The user or administrator has not consented to use the application."}"#,
        )
        .unwrap_err();
        assert!(e.consentement());
        assert!(e.to_string().contains("AADSTS65001"));
        let e = lire_reponse_jeton(400, br#"{"error":"invalid_grant","error_description":"AADSTS70008: expired"}"#)
            .unwrap_err();
        assert!(!e.consentement());
        assert!(matches!(Erreur::from(e), Erreur::Refuse(_)));
        assert!(matches!(lire_reponse_jeton(502, b"<html>").unwrap_err(), ErreurOAuth::Reseau(_)));
        assert!(matches!(lire_reponse_jeton(200, b"{}").unwrap_err(), ErreurOAuth::Reseau(_)));
    }

    #[test]
    fn adresse_tiree_de_l_id_token() {
        let jeton = |charge: &str| format!("e30.{}.sig", base64url(charge.as_bytes()));
        assert_eq!(
            adresse_id_token(&jeton(r#"{"email":"Jean@Exemple.FR","preferred_username":"autre@x.fr"}"#)),
            Some("jean@exemple.fr".into())
        );
        assert_eq!(
            adresse_id_token(&jeton(r#"{"preferred_username":"jean@exemple.onmicrosoft.com"}"#)),
            Some("jean@exemple.onmicrosoft.com".into())
        );
        assert_eq!(adresse_id_token(&jeton(r#"{"preferred_username":"+33600000000"}"#)), None);
        assert_eq!(adresse_id_token("illisible"), None);
    }

    #[test]
    fn retour_du_navigateur() {
        let ok = lire_requete("GET /?code=0.AXo-abc%2Fdef&state=E1&session_state=x HTTP/1.1", "E1");
        assert_eq!(ok, Some(Ok("0.AXo-abc/def".into())));
        let etranger = lire_requete("GET /?code=C&state=AUTRE HTTP/1.1", "E1").unwrap().unwrap_err();
        assert!(matches!(etranger, ErreurOAuth::Refus { ref code, .. } if code == "state"));
        let refus = lire_requete(
            "GET /?error=access_denied&error_description=AADSTS65004%3a+User+declined+to+consent.&state=E1 HTTP/1.1",
            "E1",
        )
        .unwrap()
        .unwrap_err();
        assert_eq!(
            refus,
            ErreurOAuth::Refus { code: "access_denied".into(), description: "AADSTS65004: User declined to consent.".into() }
        );
        let admin = lire_requete(
            "GET /?error=consent_required&error_description=AADSTS90094%3A+Admin+approval&state=E1 HTTP/1.1",
            "E1",
        )
        .unwrap()
        .unwrap_err();
        assert!(admin.consentement());
        assert_eq!(lire_requete("GET /favicon.ico HTTP/1.1", "E1"), None);
        assert_eq!(lire_requete("GET / HTTP/1.1", "E1"), None);
        assert_eq!(lire_requete("POST /?code=C&state=E1 HTTP/1.1", "E1"), None);
    }

    #[test]
    fn ecoute_locale_de_bout_en_bout() {
        let ecoute = ecouter().unwrap();
        let port = ecoute.redirection.rsplit(':').next().unwrap().parse::<u16>().unwrap();
        let navigateur = std::thread::spawn(move || {
            let mut pages = Vec::new();
            for requete in ["GET /favicon.ico HTTP/1.1\r\n\r\n", "GET /?code=CODE&state=E1 HTTP/1.1\r\nHost: x\r\n\r\n"] {
                let mut flux = TcpStream::connect(("127.0.0.1", port)).unwrap();
                flux.write_all(requete.as_bytes()).unwrap();
                let mut page = String::new();
                flux.read_to_string(&mut page).unwrap();
                pages.push(page);
            }
            pages
        });
        assert_eq!(attendre_code(&ecoute, "E1", Duration::from_secs(10)), Ok("CODE".into()));
        let pages = navigateur.join().unwrap();
        assert!(pages[0].starts_with("HTTP/1.1 404"));
        assert!(pages[1].starts_with("HTTP/1.1 200") && pages[1].contains("revenir à MMail"));
    }

    #[test]
    fn jetons_partages() {
        let p = partager("RT");
        retenir(&p, PORTEE_IMAP, "AT", 3600);
        assert_eq!(jeton_acces(&p, PORTEE_IMAP), Ok("AT".into()), "encore bon : pas de demande");
        assert_eq!(a_conserver(&p), None);
        oublier(&p, PORTEE_IMAP);
        verrouiller(&p).renouvele = true;
        assert_eq!(a_conserver(&p), Some("RT".into()));
        assert_eq!(a_conserver(&p), None, "une fois");
    }

    #[test]
    fn cci_ajoutes_pour_graph() {
        let message = b"From: a@x.fr\r\nTo: B <b@x.fr>\r\nCc: c@x.fr\r\nSubject: s\r\n\r\ncorps\r\n";
        let tous = ["b@x.fr".to_string(), "C@x.fr".to_string()];
        assert_eq!(avec_cci(message, &tous), message.to_vec(), "rien de caché : inchangé");
        let avec = ["b@x.fr".to_string(), "c@x.fr".to_string(), "d@x.fr".to_string(), "e@x.fr".to_string()];
        let sortie = String::from_utf8(avec_cci(message, &avec)).unwrap();
        assert!(sortie.starts_with("Bcc: d@x.fr, e@x.fr\r\nFrom: a@x.fr\r\n"));
    }

    /// Contenu sans motif, que la compression ni le hasard n'arrangent.
    fn octets(n: usize, graine: u32) -> Vec<u8> {
        let mut x = graine;
        (0..n)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (x >> 16) as u8
            })
            .collect()
    }

    fn message_avec_pieces(pieces: &[(&str, &str, Vec<u8>)]) -> Vec<u8> {
        let mut m = mail_builder::MessageBuilder::new()
            .from(("Jean", "jean@exemple.fr"))
            .to(vec![("Paul", "paul@exemple.fr")])
            .subject("Essai")
            .text_body("Bonjour")
            .html_body("<p>Bonjour</p>");
        for (nom, type_mime, contenu) in pieces {
            m = m.attachment(type_mime.to_string(), nom.to_string(), contenu.clone());
        }
        m.write_to_vec().unwrap()
    }

    fn noms_des_pieces(message: &[u8]) -> Vec<String> {
        use mail_parser::MimeHeaders;
        let m = mail_parser::MessageParser::default().parse(message).expect("message lisible");
        m.attachments().map(|p| p.attachment_name().unwrap_or_default().to_string()).collect()
    }

    #[test]
    fn pieces_jointes_retirees_du_message() {
        // Encodées en base64 dans le message, puis encodées de nouveau pour
        // Graph : 2,5 et 1,5 Mo de fichiers n'y tiennent pas ensemble.
        let a = octets(2_500_000, 1);
        let b = octets(1_500_000, 2);
        let message = message_avec_pieces(&[
            ("a.bin", "application/octet-stream", a.clone()),
            ("b.pdf", "application/pdf", b.clone()),
            ("c.txt", "text/plain", b"petit".to_vec()),
        ]);
        let (reduit, pieces) = alleger(&message).unwrap();
        // La plus grosse part, et elle seule : le reste tient.
        assert_eq!(pieces, vec![Piece { nom: "a.bin".into(), type_mime: "application/octet-stream".into(), contenu: a }]);
        assert!(longueur_base64(reduit.len()) <= ENVOI_MAX);
        assert_eq!(noms_des_pieces(&reduit), ["b.pdf", "c.txt"]);
        let lu = mail_parser::MessageParser::default().parse(&reduit).unwrap();
        assert_eq!(lu.body_text(0).as_deref(), Some("Bonjour"));
        assert_eq!(lu.attachment(0).unwrap().contents(), b.as_slice());

        // Les images du corps ne partent pas : trop lourd sans pièce jointe,
        // le message est refusé.
        let image = octets(4_000_000, 3);
        let lourd = mail_builder::MessageBuilder::new()
            .from("jean@exemple.fr")
            .to("paul@exemple.fr")
            .html_body("<img src=\"cid:i1\">")
            .inline("image/png", "i1", image)
            .write_to_vec()
            .unwrap();
        let e = alleger(&lourd).unwrap_err();
        assert!(matches!(e, ErreurOAuth::Refus { ref code, .. } if code == "taille"), "{e}");
    }

    #[test]
    fn piece_jointe_rendue_telle_quelle() {
        // Du Latin-1 en quoted-printable, du texte brut : les octets du
        // fichier, sans conversion en UTF-8.
        let message = b"From: a@x.fr\r\nTo: b@x.fr\r\nSubject: s\r\nMIME-Version: 1.0\r\n\
            Content-Type: multipart/mixed; boundary=\"B\"\r\n\r\n\
            --B\r\nContent-Type: text/plain; charset=utf-8\r\n\r\ncorps\r\n\
            --B\r\nContent-Type: text/plain; charset=iso-8859-1\r\n\
            Content-Disposition: attachment; filename=\"a.txt\"\r\n\
            Content-Transfer-Encoding: quoted-printable\r\n\r\nd=E9j=E0\r\n\
            --B\r\nContent-Type: text/csv\r\nContent-Disposition: attachment; filename=\"b.csv\"\r\n\r\nx;y\r\n1;2\r\n\
            --B--\r\n";
        let lu = mail_parser::MessageParser::default().parse(message).unwrap();
        let a = piece(message, lu.attachment(0).unwrap()).unwrap();
        assert_eq!((a.nom.as_str(), a.type_mime.as_str(), a.contenu.as_slice()), ("a.txt", "text/plain", b"d\xe9j\xe0".as_slice()));
        let b = piece(message, lu.attachment(1).unwrap()).unwrap();
        assert_eq!((b.nom.as_str(), b.contenu.as_slice()), ("b.csv", b"x;y\r\n1;2".as_slice()));

        // Retirer les deux laisse un multipart bien formé.
        let (r1, r2) = (plage(message, lu.attachment(0).unwrap()).unwrap(), plage(message, lu.attachment(1).unwrap()).unwrap());
        let mut reste = message[..r1.start].to_vec();
        reste.extend_from_slice(&message[r1.end..r2.start]);
        reste.extend_from_slice(&message[r2.end..]);
        assert!(String::from_utf8_lossy(&reste).ends_with("\r\n\r\n--B\r\nContent-Type: text/plain; charset=utf-8\r\n\r\ncorps\r\n--B--\r\n"));
        assert!(noms_des_pieces(&reste).is_empty());
    }

    struct Requete {
        methode: String,
        url: String,
        entetes: Vec<(String, String)>,
        corps: Vec<u8>,
    }

    impl Requete {
        fn entete(&self, nom: &str) -> Option<&str> {
            self.entetes.iter().find(|(n, _)| n == nom).map(|(_, v)| v.as_str())
        }
    }

    fn reponse(statut: u16, corps: &str) -> http::Reponse {
        http::Reponse {
            statut,
            type_contenu: "application/json".into(),
            redirection: None,
            corps: corps.as_bytes().to_vec(),
            adresse: String::new(),
            entetes: Vec::new(),
        }
    }

    /// Un faux Graph : il note les requêtes et répond comme le vrai — sauf
    /// au dépôt des morceaux, s'il doit échouer.
    fn graph(journal: &mut Vec<Requete>, depot_refuse: bool) -> impl FnMut(&Demande) -> crate::imap::Resultat<http::Reponse> + '_ {
        move |d: &Demande| {
            journal.push(Requete {
                methode: d.methode.into(),
                url: d.url.into(),
                entetes: d.entetes.iter().map(|(n, v)| (n.to_string(), v.clone())).collect(),
                corps: d.corps.to_vec(),
            });
            Ok(match d.methode {
                "POST" if d.url == format!("{GRAPH}/sendMail") => reponse(202, ""),
                "POST" if d.url == format!("{GRAPH}/messages") => reponse(
                    201,
                    r#"{"id":"AAMk/x=","toRecipients":[{"emailAddress":{"name":"Paul","address":"Paul@exemple.fr"}}],"bccRecipients":[]}"#,
                ),
                "POST" if d.url.ends_with("/createUploadSession") => {
                    reponse(201, r#"{"uploadUrl":"https://outlook.office.com/api/v2.0/Users('u')/Messages('m')/AttachmentSessions('s')?authtoken=abc"}"#)
                }
                "PUT" if depot_refuse => reponse(500, r#"{"error":{"code":"ErrorInternalServerError","message":"panne"}}"#),
                "PUT" => reponse(200, r#"{"nextExpectedRanges":[]}"#),
                "POST" if d.url.ends_with("/send") => reponse(202, ""),
                "PATCH" | "DELETE" => reponse(200, "{}"),
                _ => reponse(400, r#"{"error":{"code":"Inattendu","message":"requête inattendue"}}"#),
            })
        }
    }

    #[test]
    fn petit_message_en_une_requete() {
        let message = message_avec_pieces(&[("c.txt", "text/plain", b"petit".to_vec())]);
        let mut journal = Vec::new();
        envoyer_par(&mut graph(&mut journal, false), "JETON", &message, &["paul@exemple.fr".into()]).unwrap();
        assert_eq!(journal.len(), 1);
        assert_eq!((journal[0].methode.as_str(), journal[0].url.as_str()), ("POST", "https://graph.microsoft.com/v1.0/me/sendMail"));
        assert_eq!(journal[0].entete("Authorization"), Some("Bearer JETON"));
        assert_eq!(crate::rendu::base64(std::str::from_utf8(&journal[0].corps).unwrap()).unwrap(), message);
    }

    #[test]
    fn gros_message_par_un_brouillon() {
        let gros = octets(5_000_000, 7);
        let message = message_avec_pieces(&[
            ("photo.jpg", "image/jpeg", gros.clone()),
            ("devis.pdf", "application/pdf", b"%PDF-1.4".to_vec()),
        ]);
        let destinataires = ["paul@exemple.fr".to_string(), "cache@exemple.fr".to_string()];
        let mut journal = Vec::new();
        envoyer_par(&mut graph(&mut journal, false), "JETON", &message, &destinataires).unwrap();

        let brouillon = format!("{GRAPH}/messages/AAMk%2Fx%3D");
        let etapes: Vec<(&str, &str)> = journal.iter().map(|r| (r.methode.as_str(), r.url.as_str())).collect();
        let depot = "https://outlook.office.com/api/v2.0/Users('u')/Messages('m')/AttachmentSessions('s')?authtoken=abc";
        assert_eq!(
            etapes,
            [
                ("POST", format!("{GRAPH}/messages").as_str()),
                ("PATCH", brouillon.as_str()),
                ("POST", format!("{brouillon}/attachments/createUploadSession").as_str()),
                ("PUT", depot),
                ("PUT", depot),
                ("POST", format!("{brouillon}/send").as_str()),
            ]
        );
        // Le brouillon : le message sans la photo, avec la Cci en en-tête.
        let cree = crate::rendu::base64(std::str::from_utf8(&journal[0].corps).unwrap()).unwrap();
        assert_eq!(noms_des_pieces(&cree), ["devis.pdf"]);
        assert!(String::from_utf8_lossy(&cree).starts_with("Bcc: cache@exemple.fr\r\n"));
        // Graph ne l'a pas reprise : elle est ajoutée.
        let cci: serde_json::Value = serde_json::from_slice(&journal[1].corps).unwrap();
        assert_eq!(cci, serde_json::json!({ "bccRecipients": [{ "emailAddress": { "address": "cache@exemple.fr" } }] }));
        let session: serde_json::Value = serde_json::from_slice(&journal[2].corps).unwrap();
        assert_eq!(
            session,
            serde_json::json!({ "AttachmentItem": { "attachmentType": "file", "name": "photo.jpg", "size": 5_000_000, "contentType": "image/jpeg" } })
        );
        // Les morceaux : sans le jeton, bornés, la photo entière.
        assert_eq!(journal[3].entete("Content-Range"), Some("bytes 0-3276799/5000000"));
        assert_eq!(journal[4].entete("Content-Range"), Some("bytes 3276800-4999999/5000000"));
        assert!(journal[3..5].iter().all(|r| r.entete("Authorization").is_none()));
        assert_eq!([journal[3].corps.as_slice(), journal[4].corps.as_slice()].concat(), gros);
        assert!(journal.iter().filter(|r| r.methode != "PUT").all(|r| r.entete("Authorization") == Some("Bearer JETON")));
    }

    #[test]
    fn brouillon_supprime_quand_l_envoi_echoue() {
        let message = message_avec_pieces(&[("photo.jpg", "image/jpeg", octets(5_000_000, 9))]);
        let mut journal = Vec::new();
        let e = envoyer_par(&mut graph(&mut journal, true), "JETON", &message, &["paul@exemple.fr".into()]).unwrap_err();
        assert_eq!(e, ErreurOAuth::Refus { code: "ErrorInternalServerError".into(), description: "panne".into() });
        let derniere = journal.last().unwrap();
        assert_eq!((derniere.methode.as_str(), derniere.url.as_str()), ("DELETE", format!("{GRAPH}/messages/AAMk%2Fx%3D").as_str()));
        assert!(!journal.iter().any(|r| r.url.ends_with("/send")));
    }

    #[test]
    fn encodages() {
        assert_eq!(encoder("a b/c?d=é"), "a%20b%2Fc%3Fd%3D%C3%A9");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(
            parametres("a=1+2&b=%C3%A9&c"),
            vec![("a".into(), "1 2".into()), ("b".into(), "é".into()), ("c".into(), "".into())]
        );
    }

    #[test]
    fn identifiant_de_l_application() {
        // La variable d'environnement prime ; vide, c'est la constante.
        std::env::remove_var("MMAIL_MICROSOFT_CLIENT_ID");
        assert_eq!(identifiant_client(), MICROSOFT_CLIENT_ID);
        assert_eq!(MICROSOFT_CLIENT_ID, "6f263490-3351-4167-908b-10d701fc2f1c", "nouvelles connexions : l'appli vérifiée");
        // Un compte d'avant la 0.6.19 rafraîchit par l'ancienne inscription,
        // un compte connecté depuis par celle qui a délivré son jeton.
        assert_eq!(client_du_compte(""), "7b6c8863-79ca-42b3-8149-313df37a3993");
        assert_eq!(client_du_compte(MICROSOFT_CLIENT_ID), MICROSOFT_CLIENT_ID);
        std::env::set_var("MMAIL_MICROSOFT_CLIENT_ID", " 11111111-2222-3333-4444-555555555555 ");
        assert_eq!(identifiant_client(), "11111111-2222-3333-4444-555555555555");
        assert_eq!(client_du_compte(""), "11111111-2222-3333-4444-555555555555", "la variable prime");
        assert_eq!(client_du_compte(MICROSOFT_CLIENT_ID), "11111111-2222-3333-4444-555555555555");
        assert!(disponible());
        std::env::remove_var("MMAIL_MICROSOFT_CLIENT_ID");
    }

    #[test]
    fn jeton_au_coffre() {
        let ecrit = pour_le_coffre(MICROSOFT_CLIENT_ID, "0.AXsA-b_c*d!e$f");
        assert_eq!(ecrit, "6f263490-3351-4167-908b-10d701fc2f1c 0.AXsA-b_c*d!e$f");
        assert_eq!(du_coffre(&ecrit), (MICROSOFT_CLIENT_ID.to_string(), "0.AXsA-b_c*d!e$f".to_string()));
        // Jeton écrit avant la 0.6.19 : seul, sans application.
        assert_eq!(du_coffre(" M.C5_BAY.0.U.-Cx*y "), (String::new(), "M.C5_BAY.0.U.-Cx*y".to_string()));
        assert_eq!(du_coffre("pas-un-guid jeton"), (String::new(), "pas-un-guid jeton".to_string()));
        assert_eq!(pour_le_coffre("", "RT"), "RT");
    }

    // ------------------------------------------------------- catégories

    /// Un faux Graph pour les catégories : il note les requêtes et répond
    /// d'après le début de l'adresse.
    fn graph_categories<'a>(
        journal: &'a mut Vec<Requete>,
        reponses: &'a [(&'a str, &'a str, u16, &'a str)],
    ) -> impl FnMut(&Demande) -> crate::imap::Resultat<http::Reponse> + 'a {
        move |d: &Demande| {
            journal.push(Requete {
                methode: d.methode.into(),
                url: d.url.into(),
                entetes: d.entetes.iter().map(|(n, v)| (n.to_string(), v.clone())).collect(),
                corps: d.corps.to_vec(),
            });
            for (methode, debut, statut, corps) in reponses {
                if d.methode == *methode && d.url.starts_with(debut) {
                    return Ok(reponse(*statut, corps));
                }
            }
            Ok(reponse(400, r#"{"error":{"code":"Inattendu","message":"requête inattendue"}}"#))
        }
    }

    #[test]
    fn dossier_graph_par_les_noms() {
        let mut journal = Vec::new();
        let reponses = [
            ("GET", "https://graph.microsoft.com/v1.0/me/mailFolders?", 200,
             r#"{"value":[{"id":"I","displayName":"Boîte de réception"},{"id":"P1","displayName":"Projets"}]}"#),
            ("GET", "https://graph.microsoft.com/v1.0/me/mailFolders/P1/childFolders", 200,
             r#"{"value":[{"id":"C1","displayName":"Clients été"}]}"#),
            ("GET", "https://graph.microsoft.com/v1.0/me/mailFolders/inbox?", 200, r#"{"id":"I"}"#),
            ("GET", "https://graph.microsoft.com/v1.0/me/mailFolders/I/childFolders", 200,
             r#"{"value":[{"id":"S1","displayName":"Suivi"}]}"#),
        ];
        let mut t = graph_categories(&mut journal, &reponses);
        // Le vrai identifiant, celui des `parentFolderId` des messages.
        assert_eq!(dossier_graph_par(&mut t, "J", "INBOX", "/").unwrap(), "I");
        // Le nom IMAP est en UTF-7 modifié : « Clients été ».
        assert_eq!(dossier_graph_par(&mut t, "J", "Projets/Clients &AOk-t&AOk-", "/").unwrap(), "C1");
        assert_eq!(dossier_graph_par(&mut t, "J", "INBOX/Suivi", "/").unwrap(), "S1");
        assert!(dossier_graph_par(&mut t, "J", "Absent", "/").is_err());
        drop(t);
        assert_eq!(journal[0].entete("Authorization"), Some("Bearer J"));
    }

    #[test]
    fn categories_par_delta_page_a_page() {
        let mut journal = Vec::new();
        let reponses = [
            ("GET", "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages/delta", 200,
             r#"{"value":[{"id":"m1","internetMessageId":"<a@x>","categories":["Devis"]},{"id":"m2","internetMessageId":"<b@x>","categories":[]}],
                 "@odata.nextLink":"https://graph.microsoft.com/v1.0/me/suite?page=2"}"#),
            ("GET", "https://graph.microsoft.com/v1.0/me/suite?page=2", 200,
             r#"{"value":[{"id":"m3","@removed":{"reason":"deleted"}},{"id":"m4","internetMessageId":"<c@x>","categories":["Urgent","Devis"]}],
                 "@odata.deltaLink":"https://graph.microsoft.com/v1.0/me/delta?jeton=D1"}"#),
        ];
        let (vus, lien) = categories_delta_par(&mut graph_categories(&mut journal, &reponses), "J", "inbox", "").unwrap();
        assert_eq!(
            vus,
            vec![
                ("a@x".to_string(), vec!["Devis".to_string()]),
                ("b@x".to_string(), vec![]),
                ("c@x".to_string(), vec!["Urgent".to_string(), "Devis".to_string()]),
            ]
        );
        assert_eq!(lien, "https://graph.microsoft.com/v1.0/me/delta?jeton=D1");
        assert!(journal[0].url.contains("$select=internetMessageId,categories"));
        assert_eq!(journal[0].entete("Prefer"), Some("odata.maxpagesize=200"));
    }

    #[test]
    fn delta_perime_repart_de_zero() {
        let mut journal = Vec::new();
        let reponses = [
            ("GET", "https://graph.microsoft.com/v1.0/me/delta?jeton=VIEUX", 410,
             r#"{"error":{"code":"SyncStateNotFound","message":"périmé"}}"#),
            ("GET", "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages/delta", 200,
             r#"{"value":[{"id":"m1","internetMessageId":"<a@x>","categories":["Devis"]}],"@odata.deltaLink":"https://graph.microsoft.com/v1.0/me/delta?jeton=NEUF"}"#),
        ];
        let (vus, lien) = categories_delta_par(
            &mut graph_categories(&mut journal, &reponses),
            "J",
            "inbox",
            "https://graph.microsoft.com/v1.0/me/delta?jeton=VIEUX",
        )
        .unwrap();
        assert_eq!(vus.len(), 1);
        assert!(lien.ends_with("NEUF"));
        assert_eq!(journal.len(), 2);
    }

    #[test]
    fn categorie_posee_par_message_id() {
        let mut journal = Vec::new();
        let reponses = [
            ("GET", "https://graph.microsoft.com/v1.0/me/messages?$filter=", 200,
             r#"{"value":[{"id":"M/1=","categories":["Urgent"],"parentFolderId":"inbox-id"},{"id":"M2","categories":[],"parentFolderId":"autre"}]}"#),
            ("PATCH", "https://graph.microsoft.com/v1.0/me/messages/", 200, "{}"),
        ];
        let n = poser_categorie_par(&mut graph_categories(&mut journal, &reponses), "J", "a'b@x", Some("inbox-id"), "Devis", true)
            .unwrap();
        assert_eq!(n, 1);
        // Chevrons rendus à l'identifiant de l'index, apostrophe doublée,
        // filtre encodé.
        assert!(journal[0].url.contains(&encoder("internetMessageId eq '<a''b@x>'")), "{}", journal[0].url);
        assert_eq!(journal[1].methode, "PATCH");
        assert!(journal[1].url.ends_with(&encoder("M/1=")), "{}", journal[1].url);
        let corps: serde_json::Value = serde_json::from_slice(&journal[1].corps).unwrap();
        assert_eq!(corps, serde_json::json!({"categories": ["Urgent", "Devis"]}));
        assert_eq!(journal[1].entete("Content-Type"), Some("application/json"));
        // Déjà posée : rien n'est écrit ; retirée : la liste sans elle.
        let mut journal = Vec::new();
        let reponses = [
            ("GET", "https://graph.microsoft.com/v1.0/me/messages?$filter=", 200,
             r#"{"value":[{"id":"M1","categories":["Urgent","devis"],"parentFolderId":"inbox-id"}]}"#),
            ("PATCH", "https://graph.microsoft.com/v1.0/me/messages/", 200, "{}"),
        ];
        assert_eq!(poser_categorie_par(&mut graph_categories(&mut journal, &reponses), "J", "<a@x>", None, "Devis", true).unwrap(), 0);
        assert_eq!(poser_categorie_par(&mut graph_categories(&mut journal, &reponses), "J", "<a@x>", None, "Devis", false).unwrap(), 1);
        let corps: serde_json::Value = serde_json::from_slice(&journal.last().unwrap().corps).unwrap();
        assert_eq!(corps, serde_json::json!({"categories": ["Urgent"]}));
    }

    #[test]
    fn categorie_renommee_sur_tous_les_messages() {
        let mut journal = Vec::new();
        let reponses = [
            ("GET", "https://graph.microsoft.com/v1.0/me/messages?$filter=", 200,
             r#"{"value":[{"id":"M1","categories":["Devis","Urgent"]}],"@odata.nextLink":"https://graph.microsoft.com/v1.0/me/messages?$skiptoken=2"}"#),
            ("GET", "https://graph.microsoft.com/v1.0/me/messages?$skiptoken=2", 200,
             r#"{"value":[{"id":"M2","categories":["devis","Commande"]}]}"#),
            ("PATCH", "https://graph.microsoft.com/v1.0/me/messages/", 200, "{}"),
        ];
        let n = renommer_categorie_par(&mut graph_categories(&mut journal, &reponses), "J", "Devis", "Commande").unwrap();
        assert_eq!(n, 2);
        assert!(journal[0].url.contains(&encoder("categories/any(c:c eq 'Devis')")), "{}", journal[0].url);
        // Les deux pages sont lues avant le premier changement.
        assert_eq!(journal.iter().map(|r| r.methode.as_str()).collect::<Vec<_>>(), ["GET", "GET", "PATCH", "PATCH"]);
        let corps = |i: usize| serde_json::from_slice::<serde_json::Value>(&journal[i].corps).unwrap();
        assert_eq!(corps(2), serde_json::json!({"categories": ["Commande", "Urgent"]}));
        // Déjà porteur du nouveau nom : il n'apparaît qu'une fois.
        assert_eq!(corps(3), serde_json::json!({"categories": ["Commande"]}));
    }

    #[test]
    fn couleur_d_outlook_la_plus_proche() {
        assert_eq!(preset_proche("#E81123").as_deref(), Some("preset0"));
        assert_eq!(preset_proche("#0078d4").as_deref(), Some("preset7"));
        assert_eq!(preset_proche("#FF0000").as_deref(), Some("preset0"));
        assert_eq!(preset_proche("#00FF00").as_deref(), Some("preset4"));
        assert_eq!(preset_proche("#000000").as_deref(), Some("preset14"));
        assert_eq!(preset_proche("rouge"), None);
        assert_eq!(preset_proche("#12345"), None);
        // Chaque couleur d'Outlook revient à elle-même.
        for n in 0..25 {
            assert_eq!(preset_proche(COULEURS_OUTLOOK[n]), Some(format!("preset{n}")));
        }
    }

    #[test]
    fn categorie_reportee_dans_la_liste_d_outlook() {
        const LISTE: &str = "https://graph.microsoft.com/v1.0/me/outlook/masterCategories";
        let liste = r#"{"value":[{"id":"id-devis","displayName":"Devis","color":"preset7"},
                                 {"id":"id-ancien","displayName":"Ancien","color":"preset1"}]}"#;
        let reponses = [("GET", LISTE, 200, liste), ("POST", LISTE, 201, "{}"), ("PATCH", LISTE, 200, "{}"), ("DELETE", LISTE, 204, "")];
        let jouer = |avant: Option<(&str, Option<&str>)>, apres: Option<(&str, Option<&str>)>| {
            let mut journal = Vec::new();
            let n = reporter_categorie_par(&mut graph_categories(&mut journal, &reponses), "J", avant, apres).unwrap();
            let vu: Vec<(String, String, String)> = journal
                .iter()
                .map(|r| (r.methode.clone(), r.url.trim_start_matches(LISTE).to_string(), String::from_utf8_lossy(&r.corps).to_string()))
                .collect();
            (n, vu)
        };
        let corps = |v: &str| serde_json::from_str::<serde_json::Value>(v).unwrap();

        // Créée dans MMail : créée dans Outlook, à la couleur la plus proche.
        let (n, vu) = jouer(None, Some(("Litige", Some("#FF0000"))));
        assert_eq!(n, 1);
        assert_eq!(vu.len(), 2);
        assert_eq!((vu[1].0.as_str(), vu[1].1.as_str()), ("POST", ""));
        assert_eq!(corps(&vu[1].2), serde_json::json!({"displayName": "Litige", "color": "preset0"}));
        // Sans couleur connue : « none ».
        let (_, vu) = jouer(None, Some(("Litige", None)));
        assert_eq!(corps(&vu[1].2), serde_json::json!({"displayName": "Litige", "color": "none"}));
        // Déjà là, même couleur (casse indifférente) : rien d'écrit.
        let (n, vu) = jouer(None, Some(("devis", Some("#0078D4"))));
        assert_eq!((n, vu.len()), (0, 1));
        // Déjà là, sans couleur connue dans MMail : la couleur d'Outlook reste.
        let (n, _) = jouer(Some(("Devis", None)), Some(("Devis", None)));
        assert_eq!(n, 0);
        // Recolorée.
        let (n, vu) = jouer(Some(("Devis", Some("#0078D4"))), Some(("Devis", Some("#13A10E"))));
        assert_eq!(n, 1);
        assert_eq!((vu[1].0.as_str(), vu[1].1.as_str()), ("PATCH", "/id-devis"));
        assert_eq!(corps(&vu[1].2), serde_json::json!({"color": "preset4"}));
        // Renommée : la nouvelle créée, l'ancienne retirée de la liste.
        let (n, vu) = jouer(Some(("Ancien", Some("#F7630C"))), Some(("Nouveau", Some("#F7630C"))));
        assert_eq!(n, 2);
        assert_eq!((vu[1].0.as_str(), vu[2].0.as_str(), vu[2].1.as_str()), ("POST", "DELETE", "/id-ancien"));
        assert_eq!(corps(&vu[1].2), serde_json::json!({"displayName": "Nouveau", "color": "preset1"}));
        // Retirée de MMail : retirée de la liste d'Outlook ; absente de la
        // liste, rien d'écrit ; ni avant ni après, rien du tout.
        let (n, vu) = jouer(Some(("devis", Some("#0078D4"))), None);
        assert_eq!((n, vu.len()), (1, 2));
        assert_eq!((vu[1].0.as_str(), vu[1].1.as_str()), ("DELETE", "/id-devis"));
        let (n, vu) = jouer(Some(("Absente", None)), None);
        assert_eq!((n, vu.len()), (0, 1));
        let (n, vu) = jouer(None, None);
        assert_eq!((n, vu.len()), (0, 0));
        // Refus de Graph : l'erreur remonte.
        let refus = [("GET", LISTE, 403, r#"{"error":{"code":"ErrorAccessDenied","message":"Access is denied."}}"#)];
        let mut journal = Vec::new();
        assert!(reporter_categorie_par(&mut graph_categories(&mut journal, &refus), "J", None, Some(("X", None))).is_err());
    }

    #[test]
    fn liste_d_outlook_completee() {
        const LISTE: &str = "https://graph.microsoft.com/v1.0/me/outlook/masterCategories";
        let reponses = [("GET", LISTE, 200, r#"{"value":[{"id":"1","displayName":"Devis","color":"preset7"}]}"#), ("POST", LISTE, 201, "{}")];
        let mut journal = Vec::new();
        let voulues = vec![
            ("devis".to_string(), Some("#13A10E".to_string())),
            ("Litige".to_string(), Some("#FF0000".to_string())),
            ("litige ".to_string(), None),
            ("Relance".to_string(), None),
        ];
        assert_eq!(completer_liste_par(&mut graph_categories(&mut journal, &reponses), "J", &voulues).unwrap(), 2);
        let corps: Vec<serde_json::Value> = journal.iter().filter(|r| r.methode == "POST").map(|r| serde_json::from_slice(&r.corps).unwrap()).collect();
        assert_eq!(corps, vec![
            serde_json::json!({"displayName": "Litige", "color": "preset0"}),
            serde_json::json!({"displayName": "Relance", "color": "none"}),
        ]);
        // Rien à ajouter : pas même la lecture.
        let mut journal = Vec::new();
        assert_eq!(completer_liste_par(&mut graph_categories(&mut journal, &reponses), "J", &[]).unwrap(), 0);
        assert!(journal.is_empty());
    }

    #[test]
    fn couleurs_de_la_liste_d_outlook() {
        assert_eq!(couleur_outlook("preset0"), Some("#E81123"));
        assert_eq!(couleur_outlook("preset24"), Some("#77004D"));
        assert_eq!(couleur_outlook("preset25"), None);
        assert_eq!(couleur_outlook("none"), None);
        let mut journal = Vec::new();
        let reponses = [(
            "GET",
            "https://graph.microsoft.com/v1.0/me/outlook/masterCategories",
            200,
            r#"{"value":[{"id":"1","displayName":"Devis","color":"preset7"},{"id":"2","displayName":"Sans couleur","color":"none"},
                        {"id":"3","displayName":"  ","color":"preset1"}]}"#,
        )];
        let couleurs = couleurs_categories_par(&mut graph_categories(&mut journal, &reponses), "J").unwrap();
        assert_eq!(couleurs, vec![("Devis".to_string(), "#0078D4".to_string())]);
        // Refus faute d'autorisation : l'erreur de Graph remonte.
        let reponses = [("GET", "https://graph.microsoft.com/v1.0/me/outlook/masterCategories", 403,
                         r#"{"error":{"code":"ErrorAccessDenied","message":"Access is denied."}}"#)];
        assert!(couleurs_categories_par(&mut graph_categories(&mut journal, &reponses), "J").is_err());
    }

    #[test]
    fn autorisation_des_reglages_sans_choix_du_compte() {
        let url = url_autorisation_reglages("C", "http://localhost:1", "D", "E", "a@exemple.fr");
        assert!(url.contains(&format!("scope={}", encoder(PORTEE_AUTORISATION_REGLAGES))), "{url}");
        assert!(url.contains("login_hint=a%40exemple.fr"), "{url}");
        assert!(!url.contains("prompt="), "{url}");
        // La connexion d'un compte, elle, ne change pas.
        let url = url_autorisation("C", "http://localhost:1", "D", "E", "");
        assert!(url.contains("&prompt=select_account") && url.contains(&encoder(PORTEE_AUTORISATION)), "{url}");
        // Les agendas : leur portée seule, demandée à part.
        let url = url_autorisation_agenda("C", "http://localhost:1", "D", "E", "a@exemple.fr");
        assert!(url.contains(&format!("scope={}", encoder(PORTEE_AUTORISATION_AGENDA))), "{url}");
        assert!(url.contains("Calendars.ReadWrite") && !url.contains("MailboxSettings") && !url.contains("prompt="), "{url}");
    }

    /// Un filtre tel quel, sans le contrôle de `filtres::lire` (qui refuse
    /// déjà, par exemple, un filtre sans action).
    fn filtre_json(json: &str) -> crate::filtres::Filtre {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn filtre_traduit_en_regle_d_outlook() {
        let f = filtre_json(r#"{"nom":"Factures","compte":"a@exemple.fr","conditions":[
            {"champ":"expediteur","operateur":"contient","valeur":" compta "},
            {"champ":"objet","operateur":"contient","valeur":"Facture"}],
            "lu":true,"destination":"dossier","dossier":"INBOX/Factures","arreter":true,"serveur":true}"#);
        let r = regle_de(&f, "a@exemple.fr").unwrap();
        assert_eq!(r.nom, "MMail : Factures");
        assert_eq!(r.dossier.as_deref(), Some("INBOX/Factures"));
        assert_eq!(r.corps, serde_json::json!({
            "isEnabled": true,
            "conditions": { "senderContains": ["compta"], "subjectContains": ["Facture"] },
            "actions": { "markAsRead": true, "stopProcessingRules": true },
        }));
        // « Une au moins » sur un même champ : l'une ou l'autre valeur.
        let f = filtre_json(r#"{"nom":"","tout":false,"conditions":[
            {"champ":"destinataires","operateur":"contient","valeur":"ventes"},
            {"champ":"destinataires","operateur":"contient","valeur":"devis"}],"destination":"corbeille","actif":false}"#);
        let r = regle_de(&f, "").unwrap();
        assert_eq!(r.nom, "MMail : filtre");
        assert_eq!(r.corps["conditions"], serde_json::json!({ "recipientContains": ["ventes", "devis"] }));
        assert_eq!(r.corps["actions"], serde_json::json!({ "delete": true }));
        assert_eq!(r.corps["isEnabled"], false);
        // Une adresse exacte ; les indésirables.
        let f = filtre_json(r#"{"conditions":[{"champ":"expediteur","operateur":"est","valeur":"pub@exemple.com"}],
            "destination":"indesirables"}"#);
        let r = regle_de(&f, "a@exemple.fr").unwrap();
        assert_eq!(r.corps["conditions"]["fromAddresses"][0]["emailAddress"]["address"], "pub@exemple.com");
        assert_eq!(r.dossier.as_deref(), Some("indesirables"));
        // Ce qu'Outlook ne sait pas faire.
        let refus = |json: &str| regle_de(&filtre_json(json), "a@exemple.fr").unwrap_err();
        assert!(refus(r#"{"conditions":[{"champ":"objet","operateur":"commence","valeur":"RE"}],"lu":true}"#).contains("commence"));
        assert!(refus(r#"{"conditions":[{"champ":"a","operateur":"contient","valeur":"x"}],"lu":true}"#).contains("À ou Cc"));
        assert!(refus(r#"{"conditions":[{"champ":"expediteur","operateur":"est","valeur":"Jean"}],"lu":true}"#).contains("adresse"));
        assert!(refus(r#"{"conditions":[{"champ":"objet","operateur":"contient","valeur":"a"},
            {"champ":"objet","operateur":"contient","valeur":"b"}],"lu":true}"#).contains("même champ"));
        assert!(refus(r#"{"tout":false,"conditions":[{"champ":"objet","operateur":"contient","valeur":"a"},
            {"champ":"expediteur","operateur":"contient","valeur":"b"}],"lu":true}"#).contains("une au moins"));
        assert!(refus(r#"{"conditions":[{"champ":"objet","operateur":"contient","valeur":"a"}],"suivi":true}"#).contains("drapeau"));
        assert!(refus(r#"{"conditions":[{"champ":"objet","operateur":"contient","valeur":"a"}],"destination":"dossier",
            "dossier":"Archives","compte_destination":"b@exemple.fr"}"#).contains("sa boîte"));
        assert!(refus(r#"{"conditions":[{"champ":"objet","operateur":"contient","valeur":"a"}]}"#).contains("sans action"));
        // La boîte de destination nommée, mais c'est la sienne : accepté.
        assert!(regle_de(&filtre_json(r#"{"conditions":[{"champ":"objet","operateur":"contient","valeur":"a"}],
            "destination":"dossier","dossier":"Archives","compte_destination":"A@exemple.fr"}"#), "a@exemple.fr").is_ok());
    }

    #[test]
    fn regles_voulues_pour_une_boite() {
        let filtres = crate::filtres::lire(r#"[
            {"nom":"Pub","conditions":[{"champ":"objet","operateur":"contient","valeur":"promo"}],"destination":"corbeille","serveur":true},
            {"nom":"Local","conditions":[{"champ":"objet","operateur":"contient","valeur":"x"}],"lu":true},
            {"nom":"Pub","compte":"a@exemple.fr","conditions":[{"champ":"objet","operateur":"contient","valeur":"soldes"}],"lu":true,"serveur":true},
            {"nom":"Autre boîte","compte":"b@exemple.fr","conditions":[{"champ":"objet","operateur":"contient","valeur":"x"}],"lu":true,"serveur":true},
            {"nom":"Suivi","conditions":[{"champ":"objet","operateur":"contient","valeur":"x"}],"suivi":true,"serveur":true}
        ]"#).unwrap();
        let (regles, refus) = regles_voulues(&filtres, "A@exemple.fr");
        let noms: Vec<&str> = regles.iter().map(|r| r.nom.as_str()).collect();
        assert_eq!(noms, ["MMail : Pub", "MMail : Pub (2)"]);
        assert_eq!(regles[0].corps["sequence"], 100);
        assert_eq!(regles[1].corps["sequence"], 101);
        assert_eq!(regles[1].corps["displayName"], "MMail : Pub (2)");
        assert_eq!(refus.len(), 1);
        assert_eq!(refus[0].0, "Suivi");
    }

    #[test]
    fn regles_accordees_dans_outlook() {
        const BASE: &str = "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messageRules";
        let existantes = r#"{"value":[{"id":"R-out","displayName":"Ma règle d'Outlook"},
                                      {"id":"R-pub","displayName":"MMail : Pub"},
                                      {"id":"R-vieux","displayName":"MMail : Ancien"}]}"#;
        let reponses = [
            ("GET", BASE, 200, existantes),
            ("GET", "https://graph.microsoft.com/v1.0/me/mailFolders/junkemail", 200, r#"{"id":"JUNK"}"#),
            ("GET", "https://graph.microsoft.com/v1.0/me/mailFolders?", 200, r#"{"value":[{"id":"A1","displayName":"Archives"}]}"#),
            ("PATCH", BASE, 200, "{}"),
            ("POST", BASE, 201, "{}"),
            ("DELETE", BASE, 204, ""),
        ];
        let filtres = crate::filtres::lire(r#"[
            {"nom":"Pub","conditions":[{"champ":"objet","operateur":"contient","valeur":"promo"}],"destination":"indesirables","serveur":true},
            {"nom":"Archives","compte":"a@exemple.fr","conditions":[{"champ":"expediteur","operateur":"contient","valeur":"x"}],"destination":"dossier","dossier":"Archives","serveur":true}
        ]"#).unwrap();
        let (voulues, _) = regles_voulues(&filtres, "a@exemple.fr");
        let mut journal = Vec::new();
        let n = accorder_regles_par(&mut graph_categories(&mut journal, &reponses), "J", &voulues, "/").unwrap();
        assert_eq!(n, 3);
        let vu: Vec<(String, String)> =
            journal.iter().map(|r| (r.methode.clone(), r.url.trim_start_matches(BASE).to_string())).collect();
        let ecrites: Vec<(&str, &str)> = vu.iter().filter(|(m, _)| m != "GET").map(|(m, u)| (m.as_str(), u.as_str())).collect();
        // Mise à jour de la sienne, création de la nouvelle, retrait de celle
        // qui ne part plus ; la règle d'Outlook n'est pas touchée.
        assert_eq!(ecrites, [("PATCH", "/R-pub"), ("POST", ""), ("DELETE", "/R-vieux")]);
        let corps = |i: usize| serde_json::from_slice::<serde_json::Value>(&journal.iter().filter(|r| r.methode != "GET").nth(i).unwrap().corps).unwrap();
        assert_eq!(corps(0)["actions"]["moveToFolder"], "JUNK");
        assert_eq!(corps(1)["actions"]["moveToFolder"], "A1");
        assert_eq!(corps(1)["displayName"], "MMail : Archives");
        // Un dossier introuvable : rien d'écrit.
        let mut journal = Vec::new();
        let sans = [("GET", BASE, 200, existantes), ("GET", "https://graph.microsoft.com/v1.0/me/mailFolders?", 200, r#"{"value":[]}"#)];
        assert!(accorder_regles_par(&mut graph_categories(&mut journal, &sans), "J", &voulues[1..], "/").is_err());
        assert!(journal.iter().all(|r| r.methode == "GET"));
    }
}
