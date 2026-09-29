// SPDX-License-Identifier: GPL-3.0-or-later
//! Rédaction : fabriquer le message à envoyer, et préparer une réponse, un
//! transfert ou la reprise d'un brouillon à partir d'un message existant.
//!
//! Le message est fabriqué par `mail-builder`, le pendant de `mail-parser` :
//! encodage des en-têtes accentués, des noms de pièces jointes, du corps. Les
//! réponses et transferts suivent la présentation d'Outlook (décision 1) :
//! « RE : » et « TR : », et un bloc « De / Envoyé / À / Objet » au-dessus du
//! texte d'origine, plutôt que des lignes préfixées de « > ».

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use mail_builder::headers::address::Address as AdresseSortante;
use mail_builder::headers::date::Date;
use mail_builder::headers::raw::Raw;
use mail_builder::MessageBuilder;
use mail_parser::{Address, DateTime, MessageParser};
use serde::{Deserialize, Serialize};

/// Ce que la fenêtre de rédaction transmet au noyau, en JSON.
#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Redaction {
    /// Désigne la fenêtre de rédaction : les réponses du noyau le rappellent.
    pub jeton: String,
    /// Adresse de l'expéditeur (celle du compte), et son nom affiché.
    pub de: String,
    pub nom: String,
    /// Destinataires tels que saisis : « Nom <a@b.fr>; c@d.fr ».
    pub a: String,
    pub cc: String,
    pub cci: String,
    pub objet: String,
    pub texte: String,
    /// Corps HTML, vide pour un message en texte brut.
    pub html: String,
    /// Chemins des fichiers joints.
    pub pieces: Vec<String>,
    pub en_reponse_a: String,
    pub references: Vec<String>,
    /// Brouillon que cet envoi ou cet enregistrement remplace (0 : aucun).
    pub brouillon_uid: u32,
    /// Message auquel on répond ou que l'on transfère : il reçoit `\Answered`
    /// ou `$Forwarded` une fois l'envoi fait.
    pub origine_chemin: String,
    pub origine_uid: u32,
    pub origine_mode: String,
}

/// Une pièce jointe lue sur le disque, prête à être jointe.
pub struct Fichier {
    pub nom: String,
    pub contenu: Vec<u8>,
}

/// Le message fabriqué, en deux versions : celle qui part (sans `Bcc`, que
/// seule l'enveloppe porte) et celle que l'on garde dans « Éléments envoyés »
/// ou les brouillons (avec `Bcc`, pour s'en souvenir).
#[derive(Debug)]
pub struct Fabrique {
    pub envoi: Vec<u8>,
    pub copie: Vec<u8>,
    pub destinataires: Vec<String>,
    pub message_id: String,
}

/// Découpe une liste d'adresses saisie à la main : séparateurs `,` ou `;`,
/// hors guillemets et chevrons. Rend `(nom, adresse)`, ou l'entrée fautive.
pub fn adresses(liste: &str) -> Result<Vec<(String, String)>, String> {
    let mut entrees = Vec::new();
    let mut courante = String::new();
    let (mut guillemets, mut chevrons) = (false, false);
    for c in liste.chars() {
        match c {
            '"' if !chevrons => guillemets = !guillemets,
            '<' if !guillemets => chevrons = true,
            '>' if !guillemets => chevrons = false,
            ',' | ';' if !guillemets && !chevrons => {
                entrees.push(std::mem::take(&mut courante));
                continue;
            }
            _ => {}
        }
        courante.push(c);
    }
    entrees.push(courante);

    let mut sortie = Vec::new();
    for entree in entrees.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
        let (nom, adresse) = match (entree.rfind('<'), entree.ends_with('>')) {
            (Some(i), true) => (
                entree[..i].trim().trim_matches('"').trim().to_string(),
                entree[i + 1..entree.len() - 1].trim().to_string(),
            ),
            _ => (String::new(), entree.to_string()),
        };
        if !adresse_valide(&adresse) {
            return Err(entree.to_string());
        }
        sortie.push((nom, adresse));
    }
    Ok(sortie)
}

fn adresse_valide(adresse: &str) -> bool {
    let Some((local, domaine)) = adresse.rsplit_once('@') else {
        return false;
    };
    !local.is_empty()
        && domaine.contains('.')
        && !domaine.starts_with('.')
        && !domaine.ends_with('.')
        && !adresse.chars().any(|c| c.is_whitespace() || "<>,;\"()[]".contains(c))
}

/// Type MIME d'une pièce jointe, d'après son extension.
pub fn type_de_fichier(nom: &str) -> &'static str {
    let ext = nom.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "htm" | "html" => "text/html",
        "ics" => "text/calendar",
        "zip" => "application/zip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

/// Identifiant de message neuf : `<horodatage.aléa@domaine>`, le domaine
/// étant celui de l'expéditeur.
fn nouvel_identifiant(de: &str, maintenant: i64) -> String {
    let mut hacheur = RandomState::new().build_hasher();
    hacheur.write_i64(maintenant);
    hacheur.write_usize(std::process::id() as usize);
    let domaine = de.rsplit_once('@').map(|(_, d)| d).unwrap_or("mmail.invalid");
    format!("{:x}.{:016x}@{domaine}", maintenant, hacheur.finish())
}

/// Fabrique le message. `destinataires` peut être vide pour un brouillon ;
/// l'envoi, lui, le refuse.
pub fn fabriquer(r: &Redaction, maintenant: i64, fichiers: &[Fichier]) -> Result<Fabrique, String> {
    let lire = |champ: &str, valeur: &str| {
        adresses(valeur).map_err(|fautive| format!("{champ} : adresse invalide « {fautive} »"))
    };
    let a = lire("À", &r.a)?;
    let cc = lire("Cc", &r.cc)?;
    let cci = lire("Cci", &r.cci)?;
    if !adresse_valide(&r.de) {
        return Err(format!("expéditeur invalide : {}", r.de));
    }
    let message_id = nouvel_identifiant(&r.de, maintenant);

    let mut destinataires: Vec<String> = Vec::new();
    for (_, adresse) in a.iter().chain(&cc).chain(&cci) {
        if !destinataires.iter().any(|d| d.eq_ignore_ascii_case(adresse)) {
            destinataires.push(adresse.clone());
        }
    }

    let construire = |avec_cci: bool| -> Result<Vec<u8>, String> {
        let liste = |v: &[(String, String)]| {
            AdresseSortante::new_list(
                v.iter()
                    .map(|(n, a)| {
                        AdresseSortante::new_address((!n.is_empty()).then(|| n.clone()), a.clone())
                    })
                    .collect(),
            )
        };
        let de = AdresseSortante::new_address((!r.nom.is_empty()).then(|| r.nom.clone()), r.de.clone());
        let mut m = MessageBuilder::new()
            .from(de)
            .subject(r.objet.clone())
            .date(Date::new(maintenant))
            .message_id(message_id.clone())
            .header("User-Agent", Raw::new(format!("MMail/{}", env!("CARGO_PKG_VERSION"))));
        if !a.is_empty() {
            m = m.to(liste(&a));
        }
        if !cc.is_empty() {
            m = m.cc(liste(&cc));
        }
        if avec_cci && !cci.is_empty() {
            m = m.bcc(liste(&cci));
        }
        if !r.en_reponse_a.is_empty() {
            m = m.in_reply_to(r.en_reponse_a.clone());
        }
        if !r.references.is_empty() {
            m = m.references(r.references.clone());
        }
        m = m.text_body(r.texte.clone());
        if !r.html.is_empty() {
            m = m.html_body(r.html.clone());
        }
        for f in fichiers {
            m = m.attachment(type_de_fichier(&f.nom), f.nom.clone(), f.contenu.clone());
        }
        m.write_to_vec().map_err(|e| format!("fabrication du message : {e}"))
    };
    Ok(Fabrique {
        envoi: construire(false)?,
        copie: construire(true)?,
        destinataires,
        message_id,
    })
}

// ------------------------------------------------ réponses et transferts

/// Ce que le noyau rend à la fenêtre de rédaction pour la pré-remplir.
#[derive(Debug, Default, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preparation {
    pub mode: String,
    pub a: String,
    pub cc: String,
    pub cci: String,
    pub objet: String,
    pub texte: String,
    pub html: String,
    pub en_reponse_a: String,
    pub references: Vec<String>,
    /// Chemins des pièces reprises (transfert, brouillon), remplis par le fil
    /// de travail après extraction.
    pub pieces: Vec<String>,
    pub brouillon_uid: u32,
    pub origine_chemin: String,
    pub origine_uid: u32,
    pub origine_mode: String,
}

/// Préfixe « RE : » ou « TR : », sauf si l'objet le porte déjà, sous l'une de
/// ses formes courantes.
pub fn prefixer(objet: &str, prefixe: &str) -> String {
    let objet = objet.trim();
    let bas = objet.to_lowercase();
    let connus: &[&str] = if prefixe == "RE" {
        &["re:", "re :", "aw:", "rép:", "rép :"]
    } else {
        &["tr:", "tr :", "fw:", "fw :", "fwd:", "fwd :"]
    };
    if connus.iter().any(|p| bas.starts_with(p)) {
        objet.to_string()
    } else {
        format!("{prefixe} : {objet}")
    }
}

/// Une adresse telle qu'on la réécrit dans un champ : « Nom <a@b.fr> », le
/// nom entre guillemets s'il contient un séparateur.
fn affichable(nom: &str, adresse: &str) -> String {
    if nom.is_empty() || nom.eq_ignore_ascii_case(adresse) {
        adresse.to_string()
    } else if nom.contains([',', ';', '<', '>', '"']) {
        format!("\"{}\" <{adresse}>", nom.replace('"', ""))
    } else {
        format!("{nom} <{adresse}>")
    }
}

fn paires(adresses: Option<&Address<'_>>) -> Vec<(String, String)> {
    adresses
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let adresse = x.address()?.to_string();
                    Some((x.name().unwrap_or_default().to_string(), adresse))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn joindre(v: &[(String, String)]) -> String {
    v.iter().map(|(n, a)| affichable(n, a)).collect::<Vec<_>>().join("; ")
}

const JOURS: [&str; 7] = ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"];
const MOIS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre",
    "octobre", "novembre", "décembre",
];

/// « mardi 29 septembre 2026 10:33 », dans le fuseau de l'en-tête d'origine.
pub fn date_longue(d: &DateTime) -> String {
    let jours = crate::protocole::jours_depuis_epoque(d.year as i64, d.month as i64, d.day as i64);
    // Le 1er janvier 1970 était un jeudi.
    let jour = JOURS[(jours + 4).rem_euclid(7) as usize];
    let mois = MOIS.get((d.month as usize).wrapping_sub(1)).copied().unwrap_or("?");
    format!("{jour} {} {mois} {} {:02}:{:02}", d.day, d.year, d.hour, d.minute)
}

/// Prépare une réponse, une réponse à tous, un transfert ou la reprise d'un
/// brouillon. `propres` : les adresses de l'utilisateur, jamais remises en
/// destinataires d'une réponse à tous.
pub fn preparer(brut: &[u8], mode: &str, propres: &[String]) -> Preparation {
    let mut p = Preparation { mode: mode.to_string(), ..Default::default() };
    let Some(m) = MessageParser::default().parse(brut) else {
        return p;
    };
    let objet = m.subject().unwrap_or("").to_string();
    let message_id = m.message_id().unwrap_or("").to_string();
    let de = paires(m.from());
    let a = paires(m.to());
    let cc = paires(m.cc());
    let texte = crate::index::corps_affichable(brut);

    if mode == "brouillon" {
        p.a = joindre(&a);
        p.cc = joindre(&cc);
        p.cci = joindre(&paires(m.bcc()));
        p.objet = objet;
        p.texte = m.body_text(0).map(|t| t.into_owned()).unwrap_or_default();
        p.en_reponse_a = m.in_reply_to().as_text().unwrap_or("").to_string();
        p.references = references(&m);
        return p;
    }

    let citation = {
        let mut bloc = String::from("\n\n________________________________\n");
        bloc.push_str(&format!("De : {}\n", joindre(&de)));
        if let Some(date) = m.date() {
            bloc.push_str(&format!("Envoyé : {}\n", date_longue(date)));
        }
        bloc.push_str(&format!("À : {}\n", joindre(&a)));
        if !cc.is_empty() {
            bloc.push_str(&format!("Cc : {}\n", joindre(&cc)));
        }
        bloc.push_str(&format!("Objet : {objet}\n\n"));
        bloc.push_str(&texte);
        bloc
    };
    p.texte = citation;

    if mode == "transferer" {
        p.objet = prefixer(&objet, "TR");
        p.origine_mode = "transferer".into();
        return p;
    }

    p.objet = prefixer(&objet, "RE");
    p.origine_mode = "repondre".into();
    let cible = match paires(m.reply_to()) {
        v if !v.is_empty() => v,
        _ => de,
    };
    let est_propre = |adresse: &str| propres.iter().any(|x| x.eq_ignore_ascii_case(adresse));
    // Répondre à son propre message : on écrit à ses destinataires d'origine.
    let cible = if !cible.is_empty() && cible.iter().all(|(_, x)| est_propre(x)) { a.clone() } else { cible };
    p.a = joindre(&cible);
    if mode == "repondre_tous" {
        let mut autres: Vec<(String, String)> = Vec::new();
        for (n, x) in a.iter().chain(&cc) {
            let deja = cible.iter().chain(&autres).any(|(_, y)| y.eq_ignore_ascii_case(x));
            if !est_propre(x) && !deja {
                autres.push((n.clone(), x.clone()));
            }
        }
        p.cc = joindre(&autres);
    }
    if !message_id.is_empty() {
        p.en_reponse_a = message_id.clone();
        p.references = references(&m);
        if !p.references.iter().any(|r| r == &message_id) {
            p.references.push(message_id);
        }
    }
    p
}

fn references(m: &mail_parser::Message<'_>) -> Vec<String> {
    let valeur = m.references();
    if let Some(liste) = valeur.as_text_list() {
        liste.iter().map(|r| r.to_string()).collect()
    } else {
        valeur.as_text().map(|r| vec![r.to_string()]).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mail_parser::MimeHeaders;

    const ORIGINAL: &str = "From: \"Martin, Hélène\" <helene@exemple.fr>\r\n\
        To: moi@exemple.fr, Noël <noel@exemple.fr>\r\n\
        Cc: compta@exemple.fr\r\n\
        Subject: =?UTF-8?Q?Devis_toiture?=\r\n\
        Date: Tue, 29 Sep 2026 10:33:00 +0200\r\n\
        Message-ID: <origine.1@exemple.fr>\r\n\
        References: <avant.0@exemple.fr>\r\n\
        Content-Type: text/plain; charset=utf-8\r\n\r\n\
        Bonjour,\r\nVoici le devis.\r\n";

    #[test]
    fn listes_d_adresses() {
        assert_eq!(
            adresses("a@b.fr; Noël Durand <noel@exemple.fr>, \"Martin, Hélène\" <h@x.fr>").unwrap(),
            vec![
                ("".into(), "a@b.fr".into()),
                ("Noël Durand".into(), "noel@exemple.fr".into()),
                ("Martin, Hélène".into(), "h@x.fr".into()),
            ]
        );
        assert_eq!(adresses(" ; , ").unwrap(), vec![]);
        assert_eq!(adresses("a@b.fr, pas une adresse").unwrap_err(), "pas une adresse");
        assert!(adresses("a@localhost").is_err());
        assert!(adresses("Nom <a b@c.fr>").is_err());
    }

    #[test]
    fn objets_prefixes_une_seule_fois() {
        assert_eq!(prefixer("Devis", "RE"), "RE : Devis");
        assert_eq!(prefixer("RE : Devis", "RE"), "RE : Devis");
        assert_eq!(prefixer("Re: Devis", "RE"), "Re: Devis");
        assert_eq!(prefixer("Devis", "TR"), "TR : Devis");
        assert_eq!(prefixer("Fwd: Devis", "TR"), "Fwd: Devis");
        assert_eq!(prefixer("RE : Devis", "TR"), "TR : RE : Devis");
    }

    #[test]
    fn reponse_simple_et_a_tous() {
        let propres = vec!["moi@exemple.fr".to_string()];
        let r = preparer(ORIGINAL.as_bytes(), "repondre", &propres);
        assert_eq!(r.a, "\"Martin, Hélène\" <helene@exemple.fr>");
        assert_eq!(r.cc, "");
        assert_eq!(r.objet, "RE : Devis toiture");
        assert_eq!(r.en_reponse_a, "origine.1@exemple.fr");
        assert_eq!(r.references, vec!["avant.0@exemple.fr", "origine.1@exemple.fr"]);
        assert!(r.texte.starts_with("\n\n________________________________\nDe : "));
        assert!(r.texte.contains("Envoyé : mardi 29 septembre 2026 10:33\n"));
        assert!(r.texte.contains("Objet : Devis toiture\n\nBonjour,"));
        assert_eq!(r.origine_mode, "repondre");

        let t = preparer(ORIGINAL.as_bytes(), "repondre_tous", &propres);
        assert_eq!(t.cc, "Noël <noel@exemple.fr>; compta@exemple.fr", "sans soi-même");
    }

    #[test]
    fn transfert() {
        let t = preparer(ORIGINAL.as_bytes(), "transferer", &[]);
        assert_eq!(t.a, "");
        assert_eq!(t.objet, "TR : Devis toiture");
        assert_eq!(t.en_reponse_a, "");
        assert_eq!(t.origine_mode, "transferer");
    }

    #[test]
    fn fabrication_relue_par_mail_parser() {
        let r = Redaction {
            de: "moi@exemple.fr".into(),
            nom: "Émilie Bernard".into(),
            a: "Noël <noel@exemple.fr>".into(),
            cc: "compta@exemple.fr".into(),
            cci: "archive@exemple.fr".into(),
            objet: "Réunion d'équipe — été".into(),
            texte: "Bonjour,\n.ligne commençant par un point\nÀ bientôt".into(),
            en_reponse_a: "origine.1@exemple.fr".into(),
            references: vec!["origine.1@exemple.fr".into()],
            ..Default::default()
        };
        let pj = [Fichier { nom: "Relevé n°12.pdf".into(), contenu: b"%PDF-1.4 essai".to_vec() }];
        let f = fabriquer(&r, 1_790_000_000, &pj).unwrap();
        assert_eq!(f.destinataires, vec!["noel@exemple.fr", "compta@exemple.fr", "archive@exemple.fr"]);

        let envoi = MessageParser::default().parse(&f.envoi).unwrap();
        assert_eq!(envoi.subject(), Some("Réunion d'équipe — été"));
        assert_eq!(envoi.from().unwrap().first().unwrap().name(), Some("Émilie Bernard"));
        assert!(envoi.bcc().is_none(), "le Cci ne part pas dans le message");
        assert_eq!(envoi.in_reply_to().as_text(), Some("origine.1@exemple.fr"));
        assert!(envoi.body_text(0).unwrap().contains(".ligne commençant par un point"));
        let piece = envoi.attachments().next().unwrap();
        assert_eq!(piece.attachment_name(), Some("Relevé n°12.pdf"));
        assert_eq!(piece.contents(), b"%PDF-1.4 essai");

        let copie = MessageParser::default().parse(&f.copie).unwrap();
        assert_eq!(copie.bcc().unwrap().first().unwrap().address(), Some("archive@exemple.fr"));
        assert_eq!(copie.message_id(), envoi.message_id());
    }

    #[test]
    fn fabrication_refusee() {
        let r = Redaction { de: "moi@exemple.fr".into(), a: "pas-une-adresse".into(), ..Default::default() };
        assert!(fabriquer(&r, 0, &[]).unwrap_err().contains("pas-une-adresse"));
        let r = Redaction { de: "".into(), ..Default::default() };
        assert!(fabriquer(&r, 0, &[]).is_err());
    }

    #[test]
    fn reprise_d_un_brouillon() {
        let r = Redaction {
            de: "moi@exemple.fr".into(),
            a: "noel@exemple.fr".into(),
            cci: "archive@exemple.fr".into(),
            objet: "Brouillon".into(),
            texte: "Texte en cours".into(),
            ..Default::default()
        };
        let f = fabriquer(&r, 1_790_000_000, &[]).unwrap();
        let b = preparer(&f.copie, "brouillon", &[]);
        assert_eq!(b.a, "noel@exemple.fr");
        assert_eq!(b.cci, "archive@exemple.fr");
        assert_eq!(b.objet, "Brouillon");
        assert_eq!(b.texte.trim_end(), "Texte en cours");
    }

    #[test]
    fn dates_en_francais() {
        let d = DateTime { year: 2026, month: 9, day: 29, hour: 8, minute: 5, second: 0, tz_before_gmt: false, tz_hour: 2, tz_minute: 0 };
        assert_eq!(date_longue(&d), "mardi 29 septembre 2026 08:05");
        let d = DateTime { year: 1970, month: 1, day: 1, hour: 0, minute: 0, second: 0, tz_before_gmt: false, tz_hour: 0, tz_minute: 0 };
        assert!(date_longue(&d).starts_with("jeudi 1 janvier 1970"));
    }
}
