// SPDX-License-Identifier: GPL-3.0-or-later
//! Agendas Microsoft 365 (0.9.0, lot 3) : lecture par Graph.
//!
//! Exchange Online n'offre pas CalDAV. Graph donne les agendas de la boîte
//! (`/me/calendars`, ceux partagés avec elle compris) et, pour chacun, les
//! occurrences d'une période déjà calculées par Exchange (`calendarView`) :
//! séries développées, exceptions appliquées. Chaque occurrence entre dans
//! l'index comme un `VEVENT` simple — l'identifiant Graph tient lieu
//! d'adresse, `changeKey` d'ETag —, et le calcul des vues de `agenda` ne
//! change pas. Une occurrence de série garde son début d'origine
//! (`RECURRENCE-ID`).
//!
//! La période suit le jour : six mois en arrière, dix-huit en avant. Graph n'a
//! pas d'équivalent de `getctag` : à chaque synchronisation, la liste des
//! occurrences de la période est relue (identifiant et `changeKey`
//! seulement), et seules les nouvelles ou les modifiées sont demandées en
//! entier.
//!
//! L'écriture viendra dans un second temps : les agendas Microsoft 365 sont
//! rangés en lecture seule.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::Value;

use super::{appeler_avec, reponse_json, Transport, GRAPH};
use crate::agenda::{echapper, Composant, Propriete};
use crate::http::{self, Demande};
use crate::magasin::{Magasin, ObjetAgenda};
use crate::oauth::{encoder, ErreurOAuth};

/// Préfixe de l'adresse d'un agenda Microsoft 365 dans l'index, devant son
/// identifiant Graph : une adresse CalDAV est une URL.
pub const PREFIXE: &str = "graph:";

/// L'agenda d'adresse `adresse` vient-il de Graph ?
pub fn est_graph(adresse: &str) -> bool {
    adresse.starts_with(PREFIXE)
}

/// Période lue autour du jour : six mois en arrière, dix-huit en avant.
const JOURS_AVANT: i64 = 183;
const JOURS_APRES: i64 = 548;

/// Occurrences par page, et pages au plus par agenda (25 000 occurrences sur
/// deux ans) : au-delà, l'agenda n'est pas touché plutôt que vidé à tort.
const PAGE: usize = 250;
const PAGES_MAX: usize = 100;

/// Au-delà de ce nombre d'occurrences à relire, la période est relue en
/// entier plutôt qu'une à une.
const UNE_A_UNE: usize = 20;

/// Taille d'une page de réponse : la liste légère, puis les occurrences
/// complètes, descriptions comprises.
const PAGE_LEGERE: usize = 2 * 1024 * 1024;
const PAGE_COMPLETE: usize = 16 * 1024 * 1024;

/// Description gardée au plus (une invitation Teams en porte des pages).
const DESCRIPTION_MAX: usize = 32 * 1024;

/// Ce qui est demandé d'une occurrence.
const CHAMPS: &str = "id,iCalUId,changeKey,lastModifiedDateTime,subject,location,body,start,end,isAllDay,\
                      isCancelled,showAs,type,originalStart,isOrganizer,responseStatus,organizer,attendees,\
                      isReminderOn,reminderMinutesBeforeStart";

/// Un agenda de la boîte, tel que Graph le décrit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaGraph {
    pub id: String,
    pub nom: String,
    /// `#RRGGBB`, ou vide.
    pub couleur: String,
}

/// Ce qu'a fait une synchronisation. Les agendas illisibles n'arrêtent pas
/// les autres : leur motif est dans `erreurs`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Bilan {
    pub change: bool,
    pub agendas: usize,
    pub lus: usize,
    pub retires: usize,
    pub erreurs: Vec<String>,
}

/// Synchronise les agendas Microsoft 365 du compte `compte` (boîte
/// `adresse`) avec l'index. `jeton` : un jeton de `PORTEE_AGENDA`.
pub fn synchroniser(magasin: &Magasin, compte: i64, adresse: &str, jeton: &str, maintenant: DateTime<Utc>) -> Result<Bilan, String> {
    synchroniser_par(&mut |d: &Demande| http::envoyer(d), magasin, compte, adresse, jeton, maintenant)
}

fn synchroniser_par(
    t: &mut Transport,
    magasin: &Magasin,
    compte: i64,
    adresse: &str,
    jeton: &str,
    maintenant: DateTime<Utc>,
) -> Result<Bilan, String> {
    let distants = agendas_par(t, jeton).map_err(|e| e.to_string())?;
    // En lecture seule tant que l'écriture par Graph n'est pas là.
    let descriptions: Vec<(String, String, String, bool)> =
        distants.iter().map(|a| (format!("{PREFIXE}{}", a.id), a.nom.clone(), a.couleur.clone(), false)).collect();
    let mut bilan = Bilan { agendas: distants.len(), ..Bilan::default() };
    bilan.change |= magasin.poser_agendas(compte, &descriptions).map_err(|e| e.to_string())?;
    let (de, a) = (maintenant - Duration::days(JOURS_AVANT), maintenant + Duration::days(JOURS_APRES));
    let locaux = magasin.agendas().map_err(|e| e.to_string())?;
    for distant in &distants {
        let cle = format!("{PREFIXE}{}", distant.id);
        let Some(local) = locaux.iter().find(|l| l.compte == compte && l.adresse == cle) else { continue };
        match synchroniser_agenda(t, magasin, local.id, &distant.id, adresse, jeton, de, a) {
            Ok((lus, retires)) => {
                bilan.lus += lus;
                bilan.retires += retires;
                bilan.change |= lus + retires > 0;
            }
            Err(e) => bilan.erreurs.push(format!("agenda « {} » : {e}", distant.nom)),
        }
    }
    Ok(bilan)
}

/// Les agendas de la boîte.
fn agendas_par(t: &mut Transport, jeton: &str) -> Result<Vec<AgendaGraph>, ErreurOAuth> {
    let depart = format!("{GRAPH}/calendars?$select=id,name,color,hexColor&$top=100");
    Ok(lister(t, jeton, depart, PAGE_LEGERE)?
        .iter()
        .filter_map(|a| {
            let id = a.get("id").and_then(Value::as_str).filter(|v| !v.is_empty())?;
            Some(AgendaGraph {
                id: id.to_string(),
                nom: a.get("name").and_then(Value::as_str).unwrap_or_default().trim().to_string(),
                couleur: couleur_de(a),
            })
        })
        .collect())
}

/// Couleur d'un agenda : celle choisie dans Outlook (`hexColor`), sinon une
/// teinte proche de la couleur nommée (`color`) ; `auto` : aucune, MMail en
/// donne une de sa palette.
fn couleur_de(a: &Value) -> String {
    if let Some(h) = a.get("hexColor").and_then(Value::as_str).map(str::trim) {
        if h.len() == 7 && h.starts_with('#') && h[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            return h.to_ascii_uppercase();
        }
    }
    match a.get("color").and_then(Value::as_str).unwrap_or_default() {
        "lightBlue" => "#4F8FD9",
        "lightGreen" => "#5FB35F",
        "lightOrange" => "#F0A04B",
        "lightGray" => "#9AA0A6",
        "lightYellow" => "#E8C547",
        "lightTeal" => "#3FB6B2",
        "lightPink" => "#E07BB0",
        "lightBrown" => "#B08560",
        "lightRed" => "#E06666",
        _ => "",
    }
    .to_string()
}

/// La période d'un agenda, champs `champs`.
fn vue(agenda: &str, de: DateTime<Utc>, a: DateTime<Utc>, champs: &str) -> String {
    format!(
        "{GRAPH}/calendars/{}/calendarView?startDateTime={}&endDateTime={}&$select={champs}&$top={PAGE}",
        encoder(agenda),
        de.format("%Y-%m-%dT%H:%M:%SZ"),
        a.format("%Y-%m-%dT%H:%M:%SZ"),
    )
}

/// En-têtes des lectures d'occurrences : heures en temps universel,
/// description en texte.
fn preferences() -> [(&'static str, String); 2] {
    [("Prefer", "outlook.timezone=\"UTC\"".to_string()), ("Prefer", "outlook.body-content-type=\"text\"".to_string())]
}

/// Toutes les pages d'une liste de Graph. Une liste qui ne finit pas dans
/// `PAGES_MAX` pages est une erreur : incomplète, elle ferait retirer de
/// l'index ce qui n'y figure pas.
fn lister(t: &mut Transport, jeton: &str, depart: String, taille: usize) -> Result<Vec<Value>, ErreurOAuth> {
    let entetes = preferences();
    let mut sortie = Vec::new();
    let mut url = Some(depart);
    for _ in 0..PAGES_MAX {
        let Some(u) = url.take() else { break };
        let page = reponse_json(&appeler_avec(t, jeton, "GET", &u, "", b"", taille, &entetes)?)?;
        if let Some(liste) = page.get("value").and_then(Value::as_array) {
            sortie.extend(liste.iter().cloned());
        }
        url = page.get("@odata.nextLink").and_then(Value::as_str).map(str::to_string);
    }
    if url.is_some() {
        return Err(ErreurOAuth::refus("TropDOccurrences", "trop d'occurrences sur la période"));
    }
    Ok(sortie)
}

/// Une occurrence, par son identifiant ; `None` si elle a disparu entre-temps.
fn lire_une(t: &mut Transport, jeton: &str, id: &str) -> Result<Option<Value>, ErreurOAuth> {
    let url = format!("{GRAPH}/events/{}?$select={CHAMPS}", encoder(id));
    let r = appeler_avec(t, jeton, "GET", &url, "", b"", PAGE_COMPLETE, &preferences())?;
    if r.statut == 404 {
        return Ok(None);
    }
    reponse_json(&r).map(Some)
}

/// Compare les occurrences de la période à celles de l'index : relit les
/// nouvelles et les modifiées, retire celles qui ont disparu — de l'agenda,
/// ou de la période. Rend le nombre d'occurrences lues et retirées.
#[allow(clippy::too_many_arguments)]
fn synchroniser_agenda(
    t: &mut Transport,
    magasin: &Magasin,
    agenda: i64,
    id: &str,
    moi: &str,
    jeton: &str,
    de: DateTime<Utc>,
    a: DateTime<Utc>,
) -> Result<(usize, usize), String> {
    let legere = lister(t, jeton, vue(id, de, a, "id,changeKey"), PAGE_LEGERE).map_err(|e| e.to_string())?;
    let distants: HashMap<String, String> = legere
        .iter()
        .filter_map(|o| {
            let id = o.get("id").and_then(Value::as_str).filter(|v| !v.is_empty())?;
            Some((id.to_string(), o.get("changeKey").and_then(Value::as_str).unwrap_or_default().to_string()))
        })
        .collect();
    let locaux = magasin.etags(agenda).map_err(|e| e.to_string())?;
    let a_retirer: Vec<String> = locaux.keys().filter(|h| !distants.contains_key(*h)).cloned().collect();
    let a_lire: HashSet<&str> =
        distants.iter().filter(|(id, cle)| locaux.get(*id) != Some(*cle)).map(|(id, _)| id.as_str()).collect();
    magasin.retirer_evenements(agenda, &a_retirer).map_err(|e| e.to_string())?;
    if a_lire.is_empty() {
        return Ok((0, a_retirer.len()));
    }
    let completes: Vec<Value> = if a_lire.len() <= UNE_A_UNE {
        let mut liste = Vec::new();
        for id in &a_lire {
            if let Some(o) = lire_une(t, jeton, id).map_err(|e| e.to_string())? {
                liste.push(o);
            }
        }
        liste
    } else {
        lister(t, jeton, vue(id, de, a, CHAMPS), PAGE_COMPLETE)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|o| o.get("id").and_then(Value::as_str).is_some_and(|i| a_lire.contains(i)))
            .collect()
    };
    let objets: Vec<ObjetAgenda> = completes.iter().filter_map(|o| ranger(o, moi)).collect();
    magasin.poser_evenements(agenda, &objets).map_err(|e| e.to_string())?;
    Ok((objets.len(), a_retirer.len()))
}

/// Une occurrence prête à ranger. Illisible, elle est rangée sans texte : son
/// `changeKey` retenu, elle n'est pas redemandée tant qu'elle ne change pas.
fn ranger(o: &Value, moi: &str) -> Option<ObjetAgenda> {
    let href = o.get("id").and_then(Value::as_str).filter(|v| !v.is_empty())?.to_string();
    let etag = o.get("changeKey").and_then(Value::as_str).unwrap_or_default().to_string();
    let ical = vevent(o, moi).unwrap_or_default();
    let (debut, fin) = crate::agenda::etendue(&ical).unwrap_or((0, i64::MIN));
    Some(ObjetAgenda { href, etag, ical, debut, fin })
}

/// Un instant de Graph (`dateTimeTimeZone`) : heure du mur et fuseau, en
/// temps universel quand l'en-tête `Prefer` est suivi. Un autre fuseau —
/// nom Windows le plus souvent — est converti.
fn instant(v: &Value) -> Option<DateTime<Utc>> {
    let mur = mur(v)?;
    let fuseau = v.get("timeZone").and_then(Value::as_str).unwrap_or("UTC").trim();
    if fuseau.is_empty() || fuseau.eq_ignore_ascii_case("UTC") || fuseau.eq_ignore_ascii_case("Etc/UTC") {
        return Some(Utc.from_utc_datetime(&mur));
    }
    match crate::agenda::fuseau(fuseau, &[]) {
        Some(tz) => tz.from_local_datetime(&mur).earliest().map(|d| d.with_timezone(&Utc)),
        None => Some(Utc.from_utc_datetime(&mur)),
    }
}

/// L'heure du mur d'un `dateTimeTimeZone`, sans son fuseau.
fn mur(v: &Value) -> Option<NaiveDateTime> {
    let texte = v.get("dateTime").and_then(Value::as_str)?.trim();
    NaiveDateTime::parse_from_str(texte, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(texte, "%Y-%m-%dT%H:%M:%S"))
        .ok()
}

/// Le jour d'une heure de minuit, arrondi au plus proche : une journée
/// entière est un minuit dans son propre fuseau, que la conversion en temps
/// universel peut avoir mis la veille au soir.
fn jour(d: NaiveDateTime) -> NaiveDate {
    (d + Duration::hours(12)).date()
}

fn date_ical(d: NaiveDate) -> String {
    d.format("%Y%m%d").to_string()
}

fn heure_ical(d: DateTime<Utc>) -> String {
    d.format("%Y%m%dT%H%M%SZ").to_string()
}

/// Réponse d'un participant dans Graph, en `PARTSTAT`.
fn partstat(reponse: &str) -> &'static str {
    match reponse {
        "accepted" | "organizer" => "ACCEPTED",
        "tentativelyAccepted" => "TENTATIVE",
        "declined" => "DECLINED",
        _ => "NEEDS-ACTION",
    }
}

/// Nom et adresse d'un `emailAddress` de Graph ; sans adresse, rien.
fn personne(v: &Value) -> Option<(String, String)> {
    let e = v.get("emailAddress")?;
    let adresse = e.get("address").and_then(Value::as_str).map(str::trim).filter(|a| !a.is_empty())?;
    let nom = e.get("name").and_then(Value::as_str).unwrap_or_default().trim();
    Some((nom.to_string(), adresse.to_lowercase()))
}

/// Une personne en propriété iCalendar (`ORGANIZER`, `ATTENDEE`).
fn propriete_personne(nom_propriete: &str, nom: &str, adresse: &str, autres: &[(&str, &str)]) -> Propriete {
    let mut params: Vec<(&str, &str)> = Vec::new();
    if !nom.is_empty() {
        params.push(("CN", nom));
    }
    params.extend_from_slice(autres);
    Propriete::nouvelle(nom_propriete, &params, &format!("mailto:{adresse}"))
}

/// Une occurrence de Graph en objet iCalendar : un `VEVENT` seul, horaires en
/// temps universel (ou en jours, journée entière). `moi` : l'adresse de la
/// boîte, dont la réponse est celle de `responseStatus`.
pub fn vevent(o: &Value, moi: &str) -> Option<String> {
    let texte = |cle: &str| o.get(cle).and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let uid = Some(texte("iCalUId")).filter(|u| !u.is_empty()).or_else(|| Some(texte("id")).filter(|u| !u.is_empty()))?;
    let journee = o.get("isAllDay").and_then(Value::as_bool).unwrap_or(false);
    let debut = o.get("start")?;
    let fin = o.get("end");
    let mut v = Composant { nom: "VEVENT".into(), ..Default::default() };
    let mut poser = |nom: &str, params: &[(&str, &str)], valeur: &str| v.proprietes.push(Propriete::nouvelle(nom, params, valeur));
    poser("UID", &[], &echapper(&uid));
    if let Some(modifie) = o.get("lastModifiedDateTime").and_then(Value::as_str).and_then(|t| DateTime::parse_from_rfc3339(t).ok()) {
        poser("DTSTAMP", &[], &heure_ical(modifie.with_timezone(&Utc)));
    }
    if journee {
        let premier = jour(mur(debut)?);
        let dernier = fin.and_then(mur).map(jour).filter(|d| *d > premier).unwrap_or(premier + Duration::days(1));
        poser("DTSTART", &[("VALUE", "DATE")], &date_ical(premier));
        poser("DTEND", &[("VALUE", "DATE")], &date_ical(dernier));
    } else {
        let d = instant(debut)?;
        let f = fin.and_then(instant).filter(|f| *f >= d).unwrap_or(d);
        poser("DTSTART", &[], &heure_ical(d));
        poser("DTEND", &[], &heure_ical(f));
    }
    // Une occurrence de série, déplacée ou non : son début d'origine.
    if matches!(texte("type").as_str(), "occurrence" | "exception") {
        if let Some(origine) = o.get("originalStart").and_then(Value::as_str).and_then(|t| DateTime::parse_from_rfc3339(t).ok()) {
            let origine = origine.with_timezone(&Utc);
            if journee {
                poser("RECURRENCE-ID", &[("VALUE", "DATE")], &date_ical(jour(origine.naive_utc())));
            } else {
                poser("RECURRENCE-ID", &[], &heure_ical(origine));
            }
        }
    }
    let resume = texte("subject");
    if !resume.is_empty() {
        poser("SUMMARY", &[], &echapper(&resume));
    }
    let lieu = o.pointer("/location/displayName").and_then(Value::as_str).unwrap_or_default().trim();
    if !lieu.is_empty() {
        poser("LOCATION", &[], &echapper(lieu));
    }
    let mut description = o.pointer("/body/content").and_then(Value::as_str).unwrap_or_default().trim().to_string();
    if description.len() > DESCRIPTION_MAX {
        let mut coupe = DESCRIPTION_MAX;
        while !description.is_char_boundary(coupe) {
            coupe -= 1;
        }
        description.truncate(coupe);
        description.push('…');
    }
    if !description.is_empty() {
        poser("DESCRIPTION", &[], &echapper(&description));
    }
    if o.get("isCancelled").and_then(Value::as_bool).unwrap_or(false) {
        poser("STATUS", &[], "CANCELLED");
    }
    if texte("showAs") == "free" {
        poser("TRANSP", &[], "TRANSPARENT");
    }
    // Une réunion : son organisateur et ses participants, la réponse de la
    // boîte prise dans `responseStatus` (celle de la liste des participants
    // n'est pas toujours à jour).
    let mut participants: Vec<(String, String, &'static str, &'static str)> = o
        .get("attendees")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let (nom, adresse) = personne(p)?;
            let reponse = p.pointer("/status/response").and_then(Value::as_str).unwrap_or_default();
            let role = match p.get("type").and_then(Value::as_str).unwrap_or_default() {
                "optional" => "OPT-PARTICIPANT",
                "resource" => "NON-PARTICIPANT",
                _ => "REQ-PARTICIPANT",
            };
            Some((nom, adresse, partstat(reponse), role))
        })
        .collect();
    if !participants.is_empty() {
        let moi = moi.trim().to_lowercase();
        let organisateur = o.get("organizer").and_then(personne);
        let organise = o.get("isOrganizer").and_then(Value::as_bool).unwrap_or(false);
        if !organise && !moi.is_empty() {
            let ma_reponse = partstat(o.pointer("/responseStatus/response").and_then(Value::as_str).unwrap_or_default());
            match participants.iter_mut().find(|p| p.1 == moi) {
                Some(p) => p.2 = ma_reponse,
                None => participants.push((String::new(), moi.clone(), ma_reponse, "REQ-PARTICIPANT")),
            }
        }
        if let Some((nom, adresse)) = organisateur {
            v.proprietes.push(propriete_personne("ORGANIZER", &nom, &adresse, &[]));
        }
        for (nom, adresse, statut, role) in &participants {
            v.proprietes.push(propriete_personne("ATTENDEE", nom, adresse, &[("ROLE", *role), ("PARTSTAT", *statut)]));
        }
    }
    if o.get("isReminderOn").and_then(Value::as_bool).unwrap_or(false) {
        let minutes = o.get("reminderMinutesBeforeStart").and_then(Value::as_i64).unwrap_or(15).max(0);
        let mut alarme = Composant { nom: "VALARM".into(), ..Default::default() };
        alarme.proprietes.push(Propriete::nouvelle("ACTION", &[], "DISPLAY"));
        alarme.proprietes.push(Propriete::nouvelle("DESCRIPTION", &[], &echapper(if resume.is_empty() { "Rappel" } else { resume.as_str() })));
        alarme.proprietes.push(Propriete::nouvelle("TRIGGER", &[], &if minutes == 0 { "PT0S".to_string() } else { format!("-PT{minutes}M") }));
        v.enfants.push(alarme);
    }
    let mut racine = Composant { nom: "VCALENDAR".into(), ..Default::default() };
    racine.proprietes.push(Propriete::nouvelle("VERSION", &[], "2.0"));
    racine.proprietes.push(Propriete::nouvelle("PRODID", &[], "-//MMEDIA//MMail//FR"));
    racine.enfants.push(v);
    Some(racine.ecrire())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn utc(texte: &str) -> DateTime<Utc> {
        texte.parse().unwrap()
    }

    fn occurrences(ical: &str) -> Vec<crate::agenda::Occurrence> {
        crate::agenda::occurrences(ical, utc("2026-01-01T00:00:00Z"), utc("2028-01-01T00:00:00Z"))
    }

    fn reunion() -> Value {
        serde_json::json!({
            "id": "AAMk=1", "changeKey": "ck1", "iCalUId": "040000008200E001",
            "lastModifiedDateTime": "2026-11-02T10:00:00Z",
            "subject": "Budget, suite; point 2", "location": {"displayName": "Salle 2"},
            "body": {"contentType": "text", "content": "Ordre du jour\r\n1. Chiffres"},
            "start": {"dateTime": "2026-11-10T09:00:00.0000000", "timeZone": "UTC"},
            "end": {"dateTime": "2026-11-10T10:30:00.0000000", "timeZone": "UTC"},
            "isAllDay": false, "isCancelled": false, "showAs": "busy", "type": "singleInstance",
            "isOrganizer": false, "responseStatus": {"response": "tentativelyAccepted"},
            "organizer": {"emailAddress": {"name": "Organisatrice", "address": "Orga@Exemple.fr"}},
            "attendees": [
                {"type": "required", "status": {"response": "accepted"}, "emailAddress": {"name": "Alice", "address": "alice@exemple.fr"}},
                {"type": "optional", "status": {"response": "none"}, "emailAddress": {"name": "Moi", "address": "Moi@Exemple.fr"}}
            ],
            "isReminderOn": true, "reminderMinutesBeforeStart": 15
        })
    }

    #[test]
    fn reunion_en_vevent() {
        let ical = vevent(&reunion(), "moi@exemple.fr").unwrap();
        let o = occurrences(&ical);
        assert_eq!(o.len(), 1, "{ical}");
        let o = &o[0];
        assert_eq!(o.uid, "040000008200E001");
        assert_eq!(o.resume, "Budget, suite; point 2");
        assert_eq!(o.lieu, "Salle 2");
        assert_eq!(o.description, "Ordre du jour\n1. Chiffres");
        assert_eq!((o.debut, o.fin), (utc("2026-11-10T09:00:00Z"), utc("2026-11-10T10:30:00Z")));
        assert!(o.journee.is_none() && !o.annule && !o.repete);
        assert_eq!(o.rappels, vec![utc("2026-11-10T08:45:00Z")]);
        assert_eq!(o.organisateur.as_ref().map(|p| p.adresse.as_str()), Some("orga@exemple.fr"));
        // La réponse de la boîte vient de `responseStatus`.
        let statuts: Vec<(&str, &str)> = o.participants.iter().map(|p| (p.adresse.as_str(), p.statut.as_str())).collect();
        assert_eq!(statuts, vec![("alice@exemple.fr", "ACCEPTED"), ("moi@exemple.fr", "TENTATIVE")]);
        assert!(ical.contains("DTSTAMP:20261102T100000Z"));
    }

    #[test]
    fn journee_entiere_occurrence_et_annulation() {
        // Journée entière rendue à minuit en temps universel, ou convertie
        // depuis Paris (la veille à 23 h) : le même jour.
        for (d, f) in [("2026-11-12T00:00:00.0000000", "2026-11-14T00:00:00.0000000"), ("2026-11-11T23:00:00.0000000", "2026-11-13T23:00:00.0000000")] {
            let o = serde_json::json!({
                "id": "J", "subject": "Salon", "isAllDay": true, "type": "singleInstance",
                "start": {"dateTime": d, "timeZone": "UTC"}, "end": {"dateTime": f, "timeZone": "UTC"}
            });
            let liste = occurrences(&vevent(&o, "").unwrap());
            assert_eq!(liste.len(), 1);
            let attendu = (NaiveDate::from_ymd_opt(2026, 11, 12).unwrap(), NaiveDate::from_ymd_opt(2026, 11, 14).unwrap());
            assert_eq!(liste[0].journee, Some(attendu), "{d}");
        }
        // Occurrence déplacée d'une série : son début d'origine la désigne.
        let o = serde_json::json!({
            "id": "S2", "iCalUId": "SERIE-2", "subject": "Point hebdo", "type": "exception", "isCancelled": true,
            "originalStart": "2026-11-17T09:00:00Z",
            "start": {"dateTime": "2026-11-17T10:00:00.0000000", "timeZone": "UTC"},
            "end": {"dateTime": "2026-11-17T10:30:00.0000000", "timeZone": "UTC"}
        });
        let liste = occurrences(&vevent(&o, "").unwrap());
        assert_eq!(liste.len(), 1);
        assert!(liste[0].repete && liste[0].annule);
        assert_eq!(liste[0].origine, utc("2026-11-17T09:00:00Z"));
        assert_eq!(liste[0].debut, utc("2026-11-17T10:00:00Z"));
        assert!(liste[0].participants.is_empty() && liste[0].organisateur.is_none());
        // Un fuseau Windows, si `Prefer` n'est pas suivi.
        let o = serde_json::json!({
            "id": "W", "subject": "x", "type": "singleInstance",
            "start": {"dateTime": "2026-07-01T09:00:00", "timeZone": "Romance Standard Time"},
            "end": {"dateTime": "2026-07-01T10:00:00", "timeZone": "Romance Standard Time"}
        });
        assert_eq!(occurrences(&vevent(&o, "").unwrap())[0].debut, utc("2026-07-01T07:00:00Z"));
        // Sans début, rien.
        assert!(vevent(&serde_json::json!({"id": "X"}), "").is_none());
    }

    #[test]
    fn couleurs_des_agendas() {
        assert_eq!(couleur_de(&serde_json::json!({"hexColor": "#a1b2c3", "color": "lightRed"})), "#A1B2C3");
        assert_eq!(couleur_de(&serde_json::json!({"hexColor": "", "color": "lightGreen"})), "#5FB35F");
        assert_eq!(couleur_de(&serde_json::json!({"hexColor": "", "color": "auto"})), "");
    }

    /// Un faux Graph : les agendas, la liste légère de chaque période, les
    /// occurrences une à une ou en entier ; il note les adresses demandées.
    struct Faux {
        agendas: Vec<Value>,
        /// Par agenda : les occurrences complètes.
        periodes: HashMap<String, Vec<Value>>,
        journal: RefCell<Vec<String>>,
    }

    impl Faux {
        fn repondre(&self, d: &Demande) -> crate::imap::Resultat<http::Reponse> {
            self.journal.borrow_mut().push(d.url.to_string());
            let json = |statut: u16, v: Value| http::Reponse {
                statut,
                type_contenu: "application/json".into(),
                redirection: None,
                corps: v.to_string().into_bytes(),
                adresse: String::new(),
                entetes: Vec::new(),
            };
            let url = d.url;
            if url.contains("/calendars?") {
                return Ok(json(200, serde_json::json!({ "value": self.agendas })));
            }
            if let Some(reste) = url.split("/calendars/").nth(1).filter(|_| url.contains("/calendarView?")) {
                let agenda = reste.split('/').next().unwrap();
                let toutes = self.periodes.get(agenda).cloned().unwrap_or_default();
                let legere = url.contains("$select=id,changeKey&");
                let valeur: Vec<Value> = if legere {
                    toutes.iter().map(|o| serde_json::json!({"id": o["id"], "changeKey": o["changeKey"]})).collect()
                } else {
                    toutes
                };
                return Ok(json(200, serde_json::json!({ "value": valeur })));
            }
            if let Some(reste) = url.split("/events/").nth(1) {
                let id = reste.split('?').next().unwrap();
                let trouve = self.periodes.values().flatten().find(|o| encoder(o["id"].as_str().unwrap()) == id).cloned();
                return Ok(match trouve {
                    Some(o) => json(200, o),
                    None => json(404, serde_json::json!({"error": {"code": "ErrorItemNotFound", "message": ""}})),
                });
            }
            Ok(json(404, serde_json::json!({"error": {"code": "Inconnu", "message": url}})))
        }
    }

    fn simple(id: &str, cle: &str, titre: &str, jour: u32) -> Value {
        serde_json::json!({
            "id": id, "changeKey": cle, "iCalUId": format!("UID-{id}"), "subject": titre, "type": "singleInstance",
            "start": {"dateTime": format!("2026-11-{jour:02}T09:00:00.0000000"), "timeZone": "UTC"},
            "end": {"dateTime": format!("2026-11-{jour:02}T10:00:00.0000000"), "timeZone": "UTC"}
        })
    }

    fn titres(magasin: &Magasin) -> Vec<String> {
        let mut t: Vec<String> = magasin
            .evenements_periode(0, i64::MAX)
            .unwrap()
            .iter()
            .flat_map(|(_, _, ical)| occurrences(ical))
            .map(|o| o.resume)
            .collect();
        t.sort();
        t
    }

    #[test]
    fn synchronisations_successives() {
        let magasin = Magasin::en_memoire().unwrap();
        let compte = magasin.compte("moi@exemple.fr", "outlook.office365.com", 993, "moi@exemple.fr").unwrap();
        let mut faux = Faux {
            agendas: vec![
                serde_json::json!({"id": "CAL/1=", "name": "Calendrier", "color": "auto", "hexColor": ""}),
                serde_json::json!({"id": "CAL2", "name": "Équipe", "color": "lightTeal", "hexColor": ""}),
            ],
            periodes: HashMap::from([
                ("CAL%2F1%3D".to_string(), vec![simple("e1", "a", "Un", 9), simple("e2", "a", "Deux", 10)]),
                ("CAL2".to_string(), vec![simple("f1", "a", "Équipe", 11)]),
            ]),
            journal: RefCell::new(Vec::new()),
        };
        let maintenant = utc("2026-11-06T08:00:00Z");
        let bilan = synchroniser_par(&mut |d: &Demande| faux.repondre(d), &magasin, compte, "moi@exemple.fr", "J", maintenant).unwrap();
        assert_eq!((bilan.agendas, bilan.lus, bilan.retires, bilan.change), (2, 3, 0, true));
        assert!(bilan.erreurs.is_empty(), "{:?}", bilan.erreurs);
        assert_eq!(titres(&magasin), vec!["Deux", "Un", "Équipe"]);
        let agendas = magasin.agendas().unwrap();
        let noms: Vec<(&str, &str, &str, bool)> =
            agendas.iter().map(|a| (a.adresse.as_str(), a.nom.as_str(), a.couleur.as_str(), a.ecriture)).collect();
        assert_eq!(noms, vec![("graph:CAL/1=", "Calendrier", "", false), ("graph:CAL2", "Équipe", "#3FB6B2", false)]);
        // La période : six mois en arrière, dix-huit en avant, en temps
        // universel ; la liste légère d'abord.
        let journal = faux.journal.borrow().clone();
        assert!(journal.iter().any(|u| u.contains("/calendars/CAL%2F1%3D/calendarView?startDateTime=2026-05-07T08:00:00Z&endDateTime=2028-05-07T08:00:00Z&$select=id,changeKey&")), "{journal:?}");
        // Rien n'a changé : seules les listes légères sont relues.
        faux.journal.borrow_mut().clear();
        let bilan = synchroniser_par(&mut |d: &Demande| faux.repondre(d), &magasin, compte, "moi@exemple.fr", "J", maintenant).unwrap();
        assert_eq!((bilan.lus, bilan.retires, bilan.change), (0, 0, false));
        assert_eq!(faux.journal.borrow().len(), 3);
        // Une occurrence modifiée, une disparue : la première relue seule, la
        // seconde retirée ; l'agenda « Équipe » n'est plus partagé.
        faux.periodes.insert("CAL%2F1%3D".into(), vec![simple("e1", "b", "Un modifié", 9)]);
        faux.agendas.pop();
        faux.journal.borrow_mut().clear();
        let bilan = synchroniser_par(&mut |d: &Demande| faux.repondre(d), &magasin, compte, "moi@exemple.fr", "J", maintenant).unwrap();
        assert_eq!((bilan.agendas, bilan.lus, bilan.retires, bilan.change), (1, 1, 1, true));
        assert_eq!(titres(&magasin), vec!["Un modifié"]);
        assert!(faux.journal.borrow().iter().any(|u| u.contains("/events/e1?$select=id,iCalUId,changeKey,")));
        assert_eq!(magasin.agendas().unwrap().len(), 1);
    }

    #[test]
    fn beaucoup_de_changements_relus_en_une_fois() {
        let magasin = Magasin::en_memoire().unwrap();
        let compte = magasin.compte("moi@exemple.fr", "outlook.office365.com", 993, "moi@exemple.fr").unwrap();
        let liste: Vec<Value> = (1..=25).map(|i| simple(&format!("e{i}"), "a", &format!("n{i:02}"), (i % 28) + 1)).collect();
        let faux = Faux {
            agendas: vec![serde_json::json!({"id": "C", "name": "Calendrier"})],
            periodes: HashMap::from([("C".to_string(), liste)]),
            journal: RefCell::new(Vec::new()),
        };
        let bilan =
            synchroniser_par(&mut |d: &Demande| faux.repondre(d), &magasin, compte, "moi@exemple.fr", "J", utc("2026-11-06T08:00:00Z")).unwrap();
        assert_eq!(bilan.lus, 25);
        let journal = faux.journal.borrow();
        assert!(!journal.iter().any(|u| u.contains("/events/")), "pas une à une");
        assert_eq!(journal.iter().filter(|u| u.contains("/calendarView?")).count(), 2);
    }
}
