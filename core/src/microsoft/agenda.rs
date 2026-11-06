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
//! L'écriture passe par Graph aussi (`ecrire`) : création dans un agenda
//! dont la boîte a le droit (`canEdit`), modification ou suppression d'une
//! occurrence ou d'un événement, de toute une série (sans toucher à son
//! horaire ni à sa répétition, qui se changent dans Outlook), réponse aux
//! réunions — Exchange envoie lui-même invitations et réponses.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::Value;

use super::{appeler_avec, reponse_json, Transport, GRAPH};
use crate::agenda::{echapper, Composant, Propriete};
use crate::http::{self, Demande};
use crate::magasin::{Magasin, ObjetAgenda};
use crate::saisie::Saisie;
use crate::oauth::{encoder, ErreurOAuth};

/// Préfixe de l'adresse d'un agenda Microsoft 365 dans l'index, devant son
/// identifiant Graph : une adresse CalDAV est une URL.
pub const PREFIXE: &str = "graph:";

/// L'agenda d'adresse `adresse` vient-il de Graph ?
pub fn est_graph(adresse: &str) -> bool {
    adresse.starts_with(PREFIXE)
}

/// L'identifiant Graph d'un agenda, d'après son adresse dans l'index.
pub fn identifiant(adresse: &str) -> &str {
    adresse.strip_prefix(PREFIXE).unwrap_or(adresse)
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
                      isCancelled,showAs,type,originalStart,seriesMasterId,isOrganizer,responseStatus,organizer,\
                      attendees,isReminderOn,reminderMinutesBeforeStart";

/// Propriété qui garde l'identifiant Graph de la série d'une occurrence :
/// l'écriture en aura besoin pour modifier ou supprimer toute la série.
pub const PROPRIETE_SERIE: &str = "X-MMAIL-GRAPH-SERIE";

/// Un agenda de la boîte, tel que Graph le décrit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaGraph {
    pub id: String,
    pub nom: String,
    /// `#RRGGBB`, ou vide.
    pub couleur: String,
    /// La boîte peut y écrire (`canEdit`).
    pub ecriture: bool,
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
    let descriptions: Vec<(String, String, String, bool)> =
        distants.iter().map(|a| (format!("{PREFIXE}{}", a.id), a.nom.clone(), a.couleur.clone(), a.ecriture)).collect();
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
    let depart = format!("{GRAPH}/calendars?$select=id,name,color,hexColor,canEdit&$top=100");
    Ok(lister(t, jeton, depart, PAGE_LEGERE)?
        .iter()
        .filter_map(|a| {
            let id = a.get("id").and_then(Value::as_str).filter(|v| !v.is_empty())?;
            Some(AgendaGraph {
                id: id.to_string(),
                nom: a.get("name").and_then(Value::as_str).unwrap_or_default().trim().to_string(),
                couleur: couleur_de(a),
                ecriture: a.get("canEdit").and_then(Value::as_bool).unwrap_or(false),
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
    // Une occurrence de série, déplacée ou non : son début d'origine, et sa
    // série.
    if matches!(texte("type").as_str(), "occurrence" | "exception") {
        let serie = texte("seriesMasterId");
        if !serie.is_empty() {
            poser(PROPRIETE_SERIE, &[], &echapper(&serie));
        }
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


// ------------------------------------------------------------- écriture

/// Nom du fuseau `tz` pour Exchange : son nom Windows s'il est connu, sinon
/// son nom IANA.
fn nom_exchange(tz: chrono_tz::Tz) -> String {
    crate::agenda::nom_windows(tz).map(str::to_string).unwrap_or_else(|| tz.name().to_string())
}

/// Un instant du formulaire pour Graph : l'heure du mur, dans le fuseau du
/// poste.
fn moment_graph(mur: NaiveDateTime, fuseau: &str) -> Value {
    serde_json::json!({ "dateTime": mur.format("%Y-%m-%dT%H:%M:%S").to_string(), "timeZone": fuseau })
}

const JOURS_GRAPH: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

/// La répétition du formulaire en `patternedRecurrence` ; `None` sans
/// répétition. `AUTRE` n'en est pas une que le formulaire sache décrire.
fn recurrence(s: &Saisie, premier: NaiveDate) -> Result<Option<Value>, String> {
    use chrono::Datelike;
    let jour = JOURS_GRAPH[premier.weekday().num_days_from_monday() as usize];
    let motif = match s.repetition.as_str() {
        "" => return Ok(None),
        "DAILY" => serde_json::json!({ "type": "daily", "interval": 1 }),
        "WEEKDAYS" => serde_json::json!({ "type": "weekly", "interval": 1, "firstDayOfWeek": "monday",
                                          "daysOfWeek": ["monday", "tuesday", "wednesday", "thursday", "friday"] }),
        "WEEKLY" => serde_json::json!({ "type": "weekly", "interval": 1, "firstDayOfWeek": "monday", "daysOfWeek": [jour] }),
        "MONTHLY" => serde_json::json!({ "type": "absoluteMonthly", "interval": 1, "dayOfMonth": premier.day() }),
        "YEARLY" => serde_json::json!({ "type": "absoluteYearly", "interval": 1, "dayOfMonth": premier.day(), "month": premier.month() }),
        autre => return Err(format!("répétition « {autre} » inconnue")),
    };
    let debut = premier.format("%Y-%m-%d").to_string();
    let etendue = match s.jusqu_au.trim() {
        "" => serde_json::json!({ "type": "noEnd", "startDate": debut }),
        fin => {
            let fin = NaiveDate::parse_from_str(fin, "%Y-%m-%d").map_err(|_| format!("date de fin de la répétition illisible : {fin}"))?;
            if fin < premier {
                return Err("la répétition finit avant de commencer".into());
            }
            serde_json::json!({ "type": "endDate", "startDate": debut, "endDate": fin.format("%Y-%m-%d").to_string() })
        }
    };
    Ok(Some(serde_json::json!({ "pattern": motif, "range": etendue })))
}

/// Le formulaire en événement Graph, dans le fuseau `tz` du poste.
/// `horaire` : avec son début, sa fin et sa répétition (nouvel événement,
/// occurrence) ; sans, pour toute une série, dont l'horaire ne change que
/// dans Outlook.
pub fn evenement_de(s: &Saisie, tz: chrono_tz::Tz, horaire: bool) -> Result<Value, String> {
    let fuseau = nom_exchange(tz);
    let mut e = serde_json::json!({
        "subject": s.titre.trim(),
        "location": { "displayName": s.lieu.trim() },
        "body": { "contentType": "text", "content": s.description },
        "isReminderOn": s.rappel >= 0,
    });
    if s.rappel >= 0 {
        e["reminderMinutesBeforeStart"] = s.rappel.into();
    }
    if horaire {
        let premier = if s.journee {
            let lire = |t: &str| NaiveDate::parse_from_str(t.trim(), "%Y-%m-%d").map_err(|_| format!("date illisible : {t}"));
            let (d, f) = (lire(&s.debut)?, lire(&s.fin)?);
            if f < d {
                return Err("la fin précède le début".into());
            }
            let minuit = |j: NaiveDate| j.and_hms_opt(0, 0, 0).unwrap();
            e["isAllDay"] = true.into();
            e["start"] = moment_graph(minuit(d), &fuseau);
            // Le dernier jour est inclus dans le formulaire, exclu pour Graph.
            e["end"] = moment_graph(minuit(f + Duration::days(1)), &fuseau);
            d
        } else {
            let lire = |t: &str| NaiveDateTime::parse_from_str(t.trim(), "%Y-%m-%dT%H:%M").map_err(|_| format!("heure illisible : {t}"));
            let (d, f) = (lire(&s.debut)?, lire(&s.fin)?);
            if f < d {
                return Err("la fin précède le début".into());
            }
            e["isAllDay"] = false.into();
            e["start"] = moment_graph(d, &fuseau);
            e["end"] = moment_graph(f, &fuseau);
            d.date()
        };
        if let Some(r) = recurrence(s, premier)? {
            e["recurrence"] = r;
        }
    }
    if let Some(liste) = &s.participants {
        let invites: Vec<Value> = liste
            .iter()
            .filter_map(|p| crate::saisie::lire_adresse(p))
            .filter(|(_, adresse)| !adresse.eq_ignore_ascii_case(s.organisateur.trim()))
            .map(|(nom, adresse)| serde_json::json!({ "type": "required", "emailAddress": { "address": adresse, "name": nom } }))
            .collect();
        e["attendees"] = invites.into();
    }
    Ok(e)
}

/// La série Graph d'une occurrence rangée dans l'index, s'il y en a une.
pub fn serie_de(ical: &str) -> Option<String> {
    let racine = crate::agenda::analyser(ical)?;
    let v = racine.enfants.iter().find(|c| c.nom == "VEVENT")?;
    Some(v.texte(PROPRIETE_SERIE)).filter(|s| !s.is_empty())
}

/// L'objet d'une occurrence de Graph tel que le formulaire le lit : un
/// événement seul, sans `RECURRENCE-ID` (le formulaire chercherait sinon le
/// composant maître de la série, que l'index n'a pas).
pub fn pour_le_formulaire(ical: &str) -> Option<String> {
    let mut racine = crate::agenda::analyser(ical)?;
    for c in racine.enfants.iter_mut().filter(|c| c.nom == "VEVENT") {
        c.retirer("RECURRENCE-ID");
    }
    Some(racine.ecrire())
}

/// Ce que l'interface demande d'écrire dans un agenda Microsoft 365.
#[derive(Debug, Clone)]
pub enum Ecriture {
    /// Un nouvel événement dans l'agenda d'identifiant Graph donné.
    Creer { agenda: String, saisie: Saisie },
    /// Une occurrence (ou un événement simple) : `href`, son identifiant.
    Modifier { href: String, saisie: Saisie },
    /// Toute la série d'identifiant Graph `serie` : titre, lieu,
    /// description, rappel et participants ; l'horaire ne change pas ici.
    ModifierSerie { serie: String, saisie: Saisie },
    /// Retirer l'événement, l'occurrence ou la série d'identifiant donné.
    Supprimer { id: String },
    /// Répondre à une réunion : `ACCEPTED`, `TENTATIVE` ou `DECLINED`.
    Repondre { id: String, reponse: String },
}

/// Écrit dans un agenda Microsoft 365. `tz` : le fuseau du poste.
pub fn ecrire(jeton: &str, ecriture: &Ecriture, tz: chrono_tz::Tz) -> Result<(), String> {
    ecrire_par(&mut |d: &Demande| http::envoyer(d), jeton, ecriture, tz)
}

fn ecrire_par(t: &mut Transport, jeton: &str, ecriture: &Ecriture, tz: chrono_tz::Tz) -> Result<(), String> {
    let (methode, url, corps) = match ecriture {
        Ecriture::Creer { agenda, saisie } => {
            ("POST", format!("{GRAPH}/calendars/{}/events", encoder(agenda)), Some(evenement_de(saisie, tz, true)?))
        }
        Ecriture::Modifier { href, saisie } => {
            ("PATCH", format!("{GRAPH}/events/{}", encoder(href)), Some(evenement_de(saisie, tz, true)?))
        }
        Ecriture::ModifierSerie { serie, saisie } => {
            ("PATCH", format!("{GRAPH}/events/{}", encoder(serie)), Some(evenement_de(saisie, tz, false)?))
        }
        Ecriture::Supprimer { id } => ("DELETE", format!("{GRAPH}/events/{}", encoder(id)), None),
        Ecriture::Repondre { id, reponse } => {
            let action = match reponse.as_str() {
                "ACCEPTED" => "accept",
                "TENTATIVE" => "tentativelyAccept",
                "DECLINED" => "decline",
                autre => return Err(format!("réponse « {autre} » inconnue")),
            };
            ("POST", format!("{GRAPH}/events/{}/{action}", encoder(id)), Some(serde_json::json!({ "sendResponse": true })))
        }
    };
    let corps = corps.map(|c| c.to_string()).unwrap_or_default();
    let type_contenu = if corps.is_empty() { "" } else { "application/json" };
    let r = appeler_avec(t, jeton, methode, &url, type_contenu, corps.as_bytes(), PAGE_LEGERE, &[]).map_err(|e| e.to_string())?;
    // Déjà retiré ailleurs : c'est fait.
    if methode == "DELETE" && r.statut == 404 {
        return Ok(());
    }
    super::succes(&r).map_err(|e| match &e {
        ErreurOAuth::Refus { code, .. } if code == "ErrorAccessDenied" || code == "HTTP 403" => {
            "cet agenda n'accepte pas l'écriture".to_string()
        }
        _ => e.to_string(),
    })
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
            "originalStart": "2026-11-17T09:00:00Z", "seriesMasterId": "MAITRE/1",
            "start": {"dateTime": "2026-11-17T10:00:00.0000000", "timeZone": "UTC"},
            "end": {"dateTime": "2026-11-17T10:30:00.0000000", "timeZone": "UTC"}
        });
        let ical = vevent(&o, "").unwrap();
        assert!(ical.contains("X-MMAIL-GRAPH-SERIE:MAITRE/1\r\n"), "{ical}");
        let liste = occurrences(&ical);
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
                serde_json::json!({"id": "CAL/1=", "name": "Calendrier", "color": "auto", "hexColor": "", "canEdit": true}),
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
        assert_eq!(noms, vec![("graph:CAL/1=", "Calendrier", "", true), ("graph:CAL2", "Équipe", "#3FB6B2", false)]);
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

    fn saisie() -> Saisie {
        Saisie {
            agenda: 1,
            titre: "Revue".into(),
            lieu: "Salle 3".into(),
            description: "Ligne".into(),
            debut: "2026-11-10T09:30".into(),
            fin: "2026-11-10T10:00".into(),
            rappel: 10,
            organisateur: "moi@exemple.fr".into(),
            ..Default::default()
        }
    }

    #[test]
    fn formulaire_en_evenement_graph() {
        let tz: chrono_tz::Tz = "Europe/Paris".parse().unwrap();
        let e = evenement_de(&saisie(), tz, true).unwrap();
        assert_eq!(e["subject"], "Revue");
        assert_eq!(e["location"]["displayName"], "Salle 3");
        assert_eq!(e["start"], serde_json::json!({"dateTime": "2026-11-10T09:30:00", "timeZone": "Romance Standard Time"}));
        assert_eq!(e["end"]["dateTime"], "2026-11-10T10:00:00");
        assert_eq!(e["isAllDay"], serde_json::json!(false));
        assert_eq!((e["isReminderOn"].clone(), e["reminderMinutesBeforeStart"].clone()), (serde_json::json!(true), serde_json::json!(10)));
        assert!(e.get("recurrence").is_none() && e.get("attendees").is_none());
        // Journée entière (dernier jour inclus dans le formulaire), chaque
        // semaine jusqu'au 1er décembre, sans rappel, deux invités dont
        // l'organisateur, qui n'est pas son propre invité.
        let s = Saisie {
            journee: true,
            debut: "2026-11-12".into(),
            fin: "2026-11-13".into(),
            repetition: "WEEKLY".into(),
            jusqu_au: "2026-12-01".into(),
            rappel: -1,
            participants: Some(vec!["Alice <Alice@Exemple.fr>".into(), "moi@exemple.fr".into()]),
            ..saisie()
        };
        let e = evenement_de(&s, tz, true).unwrap();
        assert_eq!(e["isAllDay"], serde_json::json!(true));
        assert_eq!((e["start"]["dateTime"].as_str(), e["end"]["dateTime"].as_str()), (Some("2026-11-12T00:00:00"), Some("2026-11-14T00:00:00")));
        assert_eq!(e["isReminderOn"], serde_json::json!(false));
        assert!(e.get("reminderMinutesBeforeStart").is_none());
        assert_eq!(
            e["recurrence"],
            serde_json::json!({
                "pattern": {"type": "weekly", "interval": 1, "firstDayOfWeek": "monday", "daysOfWeek": ["thursday"]},
                "range": {"type": "endDate", "startDate": "2026-11-12", "endDate": "2026-12-01"}
            })
        );
        assert_eq!(e["attendees"], serde_json::json!([{"type": "required", "emailAddress": {"address": "alice@exemple.fr", "name": "Alice"}}]));
        // Les autres répétitions.
        let motif = |r: &str| evenement_de(&Saisie { repetition: r.into(), ..saisie() }, tz, true).unwrap()["recurrence"]["pattern"].clone();
        assert_eq!(motif("DAILY")["type"], "daily");
        assert_eq!(motif("WEEKDAYS")["daysOfWeek"].as_array().map(|l| l.len()), Some(5));
        assert_eq!((motif("MONTHLY")["type"].clone(), motif("MONTHLY")["dayOfMonth"].clone()), (serde_json::json!("absoluteMonthly"), serde_json::json!(10)));
        assert_eq!(motif("YEARLY")["month"], serde_json::json!(11));
        assert_eq!(evenement_de(&saisie(), tz, true).unwrap().get("recurrence"), None);
        // Toute une série : ni horaire ni répétition.
        let e = evenement_de(&s, tz, false).unwrap();
        assert!(e.get("start").is_none() && e.get("recurrence").is_none() && e.get("isAllDay").is_none());
        // Refus : fin avant le début, répétition inconnue ou qui finit avant.
        assert!(evenement_de(&Saisie { fin: "2026-11-10T09:00".into(), ..saisie() }, tz, true).is_err());
        assert!(evenement_de(&Saisie { repetition: "HOURLY".into(), ..saisie() }, tz, true).is_err());
        assert!(evenement_de(&Saisie { repetition: "DAILY".into(), jusqu_au: "2026-11-01".into(), ..saisie() }, tz, true).is_err());
        // Le temps universel garde son nom.
        assert_eq!(evenement_de(&saisie(), chrono_tz::UTC, true).unwrap()["start"]["timeZone"], "UTC");
    }

    #[test]
    fn occurrence_lue_par_le_formulaire() {
        let o = serde_json::json!({
            "id": "S2", "subject": "Point", "type": "exception", "seriesMasterId": "M1",
            "originalStart": "2026-11-17T09:00:00Z",
            "start": {"dateTime": "2026-11-17T10:00:00.0000000", "timeZone": "UTC"},
            "end": {"dateTime": "2026-11-17T10:30:00.0000000", "timeZone": "UTC"}
        });
        let ical = vevent(&o, "").unwrap();
        assert_eq!(serie_de(&ical).as_deref(), Some("M1"));
        let f = pour_le_formulaire(&ical).unwrap();
        assert!(!f.contains("RECURRENCE-ID"), "{f}");
        let s = crate::saisie::saisie_de(&f, None, chrono_tz::UTC).unwrap();
        assert_eq!((s.titre.as_str(), s.debut.as_str(), s.fin.as_str(), s.repetition.as_str()), ("Point", "2026-11-17T10:00", "2026-11-17T10:30", ""));
        assert_eq!(serie_de(&vevent(&reunion(), "").unwrap()), None);
    }

    #[test]
    fn ecritures_par_graph() {
        let journal: RefCell<Vec<(String, String, String)>> = RefCell::new(Vec::new());
        let mut faux = |d: &Demande| -> crate::imap::Resultat<http::Reponse> {
            journal.borrow_mut().push((d.methode.to_string(), d.url.to_string(), String::from_utf8_lossy(d.corps).to_string()));
            let (statut, corps) = if d.url.contains("interdit") {
                (403, r#"{"error":{"code":"ErrorAccessDenied","message":"Access is denied."}}"#)
            } else if d.url.contains("absent") {
                (404, r#"{"error":{"code":"ErrorItemNotFound","message":""}}"#)
            } else {
                (match d.methode { "POST" if d.url.ends_with("/events") => 201, "POST" => 202, "DELETE" => 204, _ => 200 }, "{}")
            };
            Ok(http::Reponse {
                statut,
                type_contenu: "application/json".into(),
                redirection: None,
                corps: corps.as_bytes().to_vec(),
                adresse: String::new(),
                entetes: Vec::new(),
            })
        };
        let tz = chrono_tz::UTC;
        let creer = Ecriture::Creer { agenda: "CAL/1=".into(), saisie: saisie() };
        ecrire_par(&mut faux, "J", &creer, tz).unwrap();
        ecrire_par(&mut faux, "J", &Ecriture::Modifier { href: "E1".into(), saisie: saisie() }, tz).unwrap();
        ecrire_par(&mut faux, "J", &Ecriture::ModifierSerie { serie: "S1".into(), saisie: saisie() }, tz).unwrap();
        ecrire_par(&mut faux, "J", &Ecriture::Supprimer { id: "absent".into() }, tz).unwrap();
        ecrire_par(&mut faux, "J", &Ecriture::Repondre { id: "S1".into(), reponse: "TENTATIVE".into() }, tz).unwrap();
        assert!(ecrire_par(&mut faux, "J", &Ecriture::Repondre { id: "S1".into(), reponse: "PEUT-ETRE".into() }, tz).is_err());
        let refus = ecrire_par(&mut faux, "J", &Ecriture::Modifier { href: "interdit".into(), saisie: saisie() }, tz);
        assert_eq!(refus, Err("cet agenda n'accepte pas l'écriture".to_string()));
        let j = journal.borrow();
        let g = "https://graph.microsoft.com/v1.0/me";
        assert_eq!((j[0].0.as_str(), j[0].1.clone()), ("POST", format!("{g}/calendars/CAL%2F1%3D/events")));
        assert!(j[0].2.contains(r#""subject":"Revue""#) && j[0].2.contains(r#""timeZone":"UTC""#), "{}", j[0].2);
        assert_eq!((j[1].0.as_str(), j[1].1.clone()), ("PATCH", format!("{g}/events/E1")));
        assert!(j[1].2.contains(r#""start""#));
        assert_eq!((j[2].0.as_str(), j[2].1.clone()), ("PATCH", format!("{g}/events/S1")));
        assert!(!j[2].2.contains(r#""start""#), "{}", j[2].2);
        assert_eq!((j[3].0.as_str(), j[3].1.clone(), j[3].2.as_str()), ("DELETE", format!("{g}/events/absent"), ""));
        assert_eq!((j[4].0.as_str(), j[4].1.clone(), j[4].2.as_str()), ("POST", format!("{g}/events/S1/tentativelyAccept"), r#"{"sendResponse":true}"#));
        assert_eq!(j.len(), 6, "la réponse inconnue ne part pas");
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
