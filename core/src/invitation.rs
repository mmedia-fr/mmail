// SPDX-License-Identifier: GPL-3.0-or-later
//! Invitations (iMIP, RFC 6047) : la partie `text/calendar` d'un courriel —
//! invitation (`REQUEST`), réponse (`REPLY`), annulation (`CANCEL`) — et ce
//! qu'il faut écrire dans l'agenda pour y donner suite.
//!
//! MMail n'envoie aucun courriel d'invitation : un serveur qui annonce
//! `calendar-auto-schedule` (RFC 6638), comme SOGo, les envoie lui-même. Il
//! transmet l'invitation, sa mise à jour ou son annulation quand l'organisateur
//! écrit l'événement dans son agenda, et la réponse quand un invité écrit la
//! sienne. Une seule chose lui échappe : la réponse reçue par l'organisateur,
//! qu'il envoie par courriel sans la reporter dans la copie de l'organisateur.
//! MMail l'y inscrit à l'ouverture du courriel — ce qui, vérifié sur SOGo, ne
//! renvoie rien aux invités.

use chrono::TimeZone;
use mail_parser::{MessageParser, MimeHeaders};
use serde::Serialize;

use crate::agenda::{self, analyser, personnes, Composant, Personne};

/// La partie calendrier d'un courriel : sa méthode et son texte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extrait {
    pub methode: String,
    pub ical: String,
}

/// Cherche la partie calendrier d'un courriel : `text/calendar` (ou une pièce
/// `.ics`) portant une méthode d'invitation. Un simple `.ics` joint, sans
/// méthode ou en `PUBLISH`, n'en est pas une.
pub fn extraire(octets: &[u8]) -> Option<Extrait> {
    let message = MessageParser::default().parse(octets)?;
    let mut trouves: Vec<(bool, Extrait)> = Vec::new();
    for partie in &message.parts {
        let Some(ct) = partie.content_type() else { continue };
        let calendrier = ct.ctype().eq_ignore_ascii_case("text") && ct.subtype().is_some_and(|s| s.eq_ignore_ascii_case("calendar"));
        let piece = (ct.ctype().eq_ignore_ascii_case("application") && ct.subtype().is_some_and(|s| s.eq_ignore_ascii_case("ics")))
            || partie.attachment_name().is_some_and(|n| n.to_ascii_lowercase().ends_with(".ics"));
        if !calendrier && !piece {
            continue;
        }
        let texte = String::from_utf8_lossy(partie.contents()).into_owned();
        let Some(racine) = analyser(&texte) else { continue };
        let methode = ct
            .attribute("method")
            .map(str::to_string)
            .or_else(|| racine.propriete("METHOD").map(|p| p.valeur.clone()))
            .unwrap_or_default()
            .trim()
            .to_ascii_uppercase();
        if matches!(methode.as_str(), "REQUEST" | "REPLY" | "CANCEL") {
            trouves.push((calendrier, Extrait { methode, ical: texte }));
        }
    }
    // La partie `text/calendar` du corps d'abord ; la pièce jointe en double,
    // comme l'envoie Outlook, ensuite.
    trouves.sort_by_key(|(calendrier, _)| !calendrier);
    trouves.into_iter().next().map(|(_, e)| e)
}

/// Ce que montre le bandeau d'une invitation.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Invitation {
    pub methode: String,
    pub uid: String,
    pub sequence: i64,
    pub titre: String,
    pub quand: String,
    pub repete: bool,
    pub lieu: String,
    pub organisateur: Option<Personne>,
    pub participants: Vec<Personne>,
}

fn sequence(c: &Composant) -> i64 {
    c.propriete("SEQUENCE").and_then(|p| p.valeur.trim().parse().ok()).unwrap_or(0)
}

/// Le VEVENT qui décrit l'invitation : la série, sinon le premier.
fn principal(racine: &Composant) -> Option<&Composant> {
    let mut evenements = racine.enfants.iter().filter(|c| c.nom == "VEVENT");
    let premier = evenements.clone().next()?;
    Some(evenements.find(|c| c.propriete("RECURRENCE-ID").is_none()).unwrap_or(premier))
}

/// Décrit une invitation dans le fuseau `tz`.
pub fn decrire<T: TimeZone>(extrait: &Extrait, tz: &T) -> Option<Invitation> {
    let racine = analyser(&extrait.ical)?;
    let e = principal(&racine)?;
    let (organisateur, participants) = personnes(e);
    let (quand, repete) = agenda::quand_de(&extrait.ical, tz).unwrap_or_default();
    Some(Invitation {
        methode: extrait.methode.clone(),
        uid: e.texte("UID"),
        sequence: sequence(e),
        titre: e.texte("SUMMARY"),
        quand,
        repete,
        lieu: e.texte("LOCATION"),
        organisateur,
        participants,
    })
}

/// UID et numéro de version d'un objet de l'agenda.
pub fn identite(ical: &str) -> Option<(String, i64)> {
    let racine = analyser(ical)?;
    let e = principal(&racine)?;
    Some((e.texte("UID"), sequence(e)))
}

/// Réponse de `moi` dans un objet de l'agenda (`ACCEPTED`…), vide s'il n'y
/// est pas invité.
pub fn reponse_de(ical: &str, moi: &str) -> String {
    let moi = moi.to_lowercase();
    analyser(ical)
        .and_then(|r| principal(&r).map(|e| personnes(e).1))
        .and_then(|ps| ps.into_iter().find(|p| p.adresse == moi))
        .map(|p| p.statut)
        .unwrap_or_default()
}

/// Pose `PARTSTAT` sur les lignes `ATTENDEE` de `moi`, en ajoutant la sienne
/// s'il n'y figure pas (invité par une liste ou un alias). Rend vrai si
/// quelque chose a changé.
fn poser_reponse(e: &mut Composant, moi: &str, statut: &str) -> bool {
    let mut trouve = false;
    let mut change = false;
    for p in e.proprietes.iter_mut().filter(|p| p.nom == "ATTENDEE") {
        if agenda::adresse_de(&p.valeur) != moi {
            continue;
        }
        trouve = true;
        if p.param("PARTSTAT").map(|s| s.to_ascii_uppercase()) != Some(statut.to_string()) {
            p.params.retain(|(n, _)| n != "PARTSTAT");
            p.params.push(("PARTSTAT".into(), statut.into()));
            change = true;
        }
    }
    if !trouve {
        e.proprietes.push(agenda::Propriete::nouvelle("ATTENDEE", &[("PARTSTAT", statut)], &format!("mailto:{moi}")));
        change = true;
    }
    change
}

/// Objet à écrire dans l'agenda de l'invité `moi` pour répondre `statut` à
/// une invitation : l'invitation elle-même, ou l'objet déjà présent s'il est
/// au moins aussi récent, avec la réponse posée. SOGo en tire le courriel de
/// réponse à l'organisateur.
pub fn repondre(invitation: &str, existant: Option<&str>, moi: &str, statut: &str) -> Result<String, String> {
    if !matches!(statut, "ACCEPTED" | "TENTATIVE" | "DECLINED") {
        return Err(format!("réponse inconnue : {statut}"));
    }
    let moi = moi.to_lowercase();
    let recue = analyser(invitation).ok_or("invitation illisible")?;
    let mut racine = match existant.and_then(analyser) {
        Some(e) if principal(&e).map(sequence) >= principal(&recue).map(sequence) => e,
        _ => recue,
    };
    // Une méthode n'a pas sa place dans un objet d'agenda (RFC 4791).
    racine.retirer("METHOD");
    let mut aucun = true;
    for e in racine.enfants.iter_mut().filter(|c| c.nom == "VEVENT") {
        poser_reponse(e, &moi, statut);
        aucun = false;
    }
    if aucun {
        return Err("invitation sans événement".into());
    }
    Ok(racine.ecrire())
}

/// Inscrit dans la copie de l'organisateur les réponses d'un courriel `REPLY` :
/// le `PARTSTAT` de chaque participant qui répond, sur la série ou sur
/// l'occurrence qu'il désigne. Rend `None` si rien ne change (réponse déjà
/// inscrite, participant inconnu de l'événement).
pub fn inscrire_reponse(organisateur: &str, reponse: &str) -> Result<Option<String>, String> {
    let mut racine = analyser(organisateur).ok_or("événement illisible")?;
    let recue = analyser(reponse).ok_or("réponse illisible")?;
    let mut change = false;
    for r in recue.enfants.iter().filter(|c| c.nom == "VEVENT") {
        let cible = crate::saisie::origine_de(&recue, r);
        let copie = racine.clone();
        let Some(e) = racine
            .enfants
            .iter_mut()
            .filter(|c| c.nom == "VEVENT")
            .find(|c| crate::saisie::origine_de(&copie, c) == cible)
        else {
            continue;
        };
        for p in personnes(r).1 {
            let present = e.proprietes.iter().any(|q| q.nom == "ATTENDEE" && agenda::adresse_de(&q.valeur) == p.adresse);
            if present && poser_reponse(e, &p.adresse, &p.statut) {
                change = true;
            }
        }
    }
    Ok(change.then(|| racine.ecrire()))
}

#[cfg(test)]
mod tests;
