// SPDX-License-Identifier: GPL-3.0-or-later
//! Agenda : lecture des objets iCalendar rendus par un serveur CalDAV, calcul
//! des occurrences d'une période et disposition dans les vues jour, semaine et
//! mois.
//!
//! Un serveur CalDAV rend chaque événement tel qu'il l'a reçu : règle de
//! répétition comprise — SOGo, celui de Mailcow, ne sait pas les développer
//! (`expand` est ignoré) — et fuseau nommé comme l'a nommé le logiciel
//! d'origine : « Europe/Paris », ou « Romance Standard Time » pour un
//! rendez-vous venu d'Outlook. Le calcul se fait donc ici : les répétitions par
//! `rrule`, les fuseaux par `chrono-tz`, les noms Windows ramenés aux noms
//! IANA, et un fuseau inconnu reconnu à ses décalages.
//!
//! L'objet iCalendar est une donnée venue d'ailleurs : une ligne illisible est
//! ignorée, une règle illisible ne rend que l'occurrence d'origine, et les
//! répétitions sont bornées.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use serde::Serialize;
use serde_json::json;

use crate::magasin::{AgendaLocal, Magasin};

/// Occurrences calculées au plus par événement répété et par période.
const REPETITIONS_MAX: u16 = 1000;

/// Profondeur d'imbrication des composants au-delà de laquelle un objet est
/// refusé.
const PROFONDEUR_MAX: usize = 16;

// ------------------------------------------------------------- iCalendar

/// Une propriété : `NOM;PARAM=valeur:valeur`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Propriete {
    pub nom: String,
    pub params: Vec<(String, String)>,
    pub valeur: String,
}

impl Propriete {
    fn param(&self, nom: &str) -> Option<&str> {
        self.params.iter().find(|(n, _)| n.eq_ignore_ascii_case(nom)).map(|(_, v)| v.as_str())
    }

    fn date_seule(&self) -> bool {
        self.param("VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE"))
    }
}

/// Un composant (`BEGIN:VEVENT` … `END:VEVENT`) et ses sous-composants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Composant {
    pub nom: String,
    pub proprietes: Vec<Propriete>,
    pub enfants: Vec<Composant>,
}

impl Composant {
    fn propriete(&self, nom: &str) -> Option<&Propriete> {
        self.proprietes.iter().find(|p| p.nom == nom)
    }

    fn toutes<'a>(&'a self, nom: &'a str) -> impl Iterator<Item = &'a Propriete> {
        self.proprietes.iter().filter(move |p| p.nom == nom)
    }

    fn texte(&self, nom: &str) -> String {
        self.propriete(nom).map(|p| desechapper(&p.valeur)).unwrap_or_default()
    }
}

/// Lignes logiques : une ligne qui commence par une espace ou une tabulation
/// continue la précédente (RFC 5545 § 3.1).
fn deplier(texte: &str) -> Vec<String> {
    let mut lignes: Vec<String> = Vec::new();
    for brute in texte.split('\n') {
        let brute = brute.strip_suffix('\r').unwrap_or(brute);
        if let (Some(' ' | '\t'), Some(derniere)) = (brute.chars().next(), lignes.last_mut()) {
            derniere.push_str(&brute[1..]);
        } else if !brute.is_empty() {
            lignes.push(brute.to_string());
        }
    }
    lignes
}

/// Une ligne logique en propriété. Un deux-points ou un point-virgule entre
/// guillemets (`TZID="(UTC+01:00) Bruxelles; Paris"`) ne coupe rien.
fn lire_propriete(ligne: &str) -> Option<Propriete> {
    let mut morceaux = vec![String::new()];
    let mut guillemets = false;
    let mut valeur = None;
    for (i, c) in ligne.char_indices() {
        match c {
            '"' => guillemets = !guillemets,
            ';' if !guillemets => morceaux.push(String::new()),
            ':' if !guillemets => {
                valeur = Some(&ligne[i + 1..]);
                break;
            }
            _ => morceaux.last_mut().unwrap().push(c),
        }
    }
    let valeur = valeur?.to_string();
    let nom = morceaux.remove(0).trim().to_ascii_uppercase();
    if nom.is_empty() {
        return None;
    }
    let params = morceaux
        .into_iter()
        .filter_map(|m| m.split_once('=').map(|(n, v)| (n.trim().to_ascii_uppercase(), v.trim().to_string())))
        .collect();
    Some(Propriete { nom, params, valeur })
}

/// Analyse un objet iCalendar et rend sa racine `VCALENDAR`, ou `None` s'il
/// n'en est pas un.
pub fn analyser(texte: &str) -> Option<Composant> {
    let mut pile: Vec<Composant> = Vec::new();
    for ligne in deplier(texte) {
        let Some(p) = lire_propriete(&ligne) else { continue };
        match p.nom.as_str() {
            "BEGIN" => {
                if pile.len() >= PROFONDEUR_MAX {
                    return None;
                }
                pile.push(Composant { nom: p.valeur.trim().to_ascii_uppercase(), ..Default::default() });
            }
            "END" => {
                let fini = pile.pop()?;
                match pile.last_mut() {
                    Some(parent) => parent.enfants.push(fini),
                    None => return Some(fini).filter(|r| r.nom == "VCALENDAR"),
                }
            }
            _ => {
                if let Some(c) = pile.last_mut() {
                    c.proprietes.push(p);
                }
            }
        }
    }
    None
}

/// Texte d'une propriété : `\n`, `\,`, `\;` et `\\` (RFC 5545 § 3.3.11).
fn desechapper(valeur: &str) -> String {
    let mut sortie = String::with_capacity(valeur.len());
    let mut echappe = false;
    for c in valeur.chars() {
        if echappe {
            sortie.push(if c == 'n' || c == 'N' { '\n' } else { c });
            echappe = false;
        } else if c == '\\' {
            echappe = true;
        } else {
            sortie.push(c);
        }
    }
    sortie.trim().to_string()
}

// ------------------------------------------------------------- fuseaux

/// Noms Windows des fuseaux (ceux d'Outlook et d'Exchange) : les plus
/// courants, ramenés au nom IANA de la ville de référence.
const FUSEAUX_WINDOWS: &[(&str, &str)] = &[
    ("Romance Standard Time", "Europe/Paris"),
    ("Romance", "Europe/Paris"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("E. Europe Standard Time", "Europe/Chisinau"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("Turkey Standard Time", "Europe/Istanbul"),
    ("Morocco Standard Time", "Africa/Casablanca"),
    ("W. Central Africa Standard Time", "Africa/Lagos"),
    ("South Africa Standard Time", "Africa/Johannesburg"),
    ("Eastern Standard Time", "America/New_York"),
    ("Central Standard Time", "America/Chicago"),
    ("Mountain Standard Time", "America/Denver"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("SA Pacific Standard Time", "America/Bogota"),
    ("SA Western Standard Time", "America/La_Paz"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("Canada Central Standard Time", "America/Regina"),
    ("China Standard Time", "Asia/Shanghai"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("India Standard Time", "Asia/Kolkata"),
    ("Arabian Standard Time", "Asia/Dubai"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("Mauritius Standard Time", "Indian/Mauritius"),
    ("Reunion Standard Time", "Indian/Reunion"),
    ("SE Asia Standard Time", "Asia/Bangkok"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("UTC", "UTC"),
    ("Coordinated Universal Time", "UTC"),
    ("GMT", "UTC"),
    ("Z", "UTC"),
];

/// Villes reconnues dans les libellés qu'Outlook écrit parfois en guise de
/// nom (« (UTC+01:00) Bruxelles, Copenhague, Madrid, Paris »).
const VILLES: &[(&str, &str)] = &[
    ("paris", "Europe/Paris"),
    ("bruxelles", "Europe/Paris"),
    ("brussels", "Europe/Paris"),
    ("madrid", "Europe/Paris"),
    ("amsterdam", "Europe/Berlin"),
    ("berlin", "Europe/Berlin"),
    ("rome", "Europe/Berlin"),
    ("london", "Europe/London"),
    ("londres", "Europe/London"),
    ("lisbon", "Europe/London"),
    ("lisbonne", "Europe/London"),
];

/// Fuseau d'un `TZID`. Dans l'ordre : nom IANA (préfixé ou non), nom Windows,
/// libellé d'Outlook ; puis, pour un nom inconnu (« Customized Time Zone »),
/// la définition `VTIMEZONE` de l'objet, reconnue à ses décalages.
pub fn fuseau(tzid: &str, definitions: &[Composant]) -> Option<chrono_tz::Tz> {
    let tzid = tzid.trim().trim_matches('"').trim();
    // « /mozilla.org/20050126_1/Europe/Paris », « /softwarestudio.org/…/Europe/Paris ».
    let mut candidat = tzid;
    if let Ok(tz) = candidat.parse::<chrono_tz::Tz>() {
        return Some(tz);
    }
    while let Some((_, reste)) = candidat.split_once('/') {
        candidat = reste;
        if let Ok(tz) = candidat.parse::<chrono_tz::Tz>() {
            return Some(tz);
        }
    }
    if let Some((_, iana)) = FUSEAUX_WINDOWS.iter().find(|(w, _)| w.eq_ignore_ascii_case(tzid)) {
        return iana.parse().ok();
    }
    let minuscules = tzid.to_lowercase();
    if let Some((_, iana)) = VILLES.iter().find(|(v, _)| minuscules.contains(v)) {
        return iana.parse().ok();
    }
    let definition = definitions
        .iter()
        .find(|d| d.nom == "VTIMEZONE" && d.propriete("TZID").is_some_and(|p| p.valeur.trim().trim_matches('"') == tzid))?;
    par_decalages(definition)
}

/// Décalage `+0100` en minutes.
fn lire_decalage(valeur: &str) -> Option<i32> {
    let v = valeur.trim();
    let (signe, chiffres) = match v.chars().next()? {
        '+' => (1, &v[1..]),
        '-' => (-1, &v[1..]),
        _ => (1, v),
    };
    let heures: i32 = chiffres.get(..2)?.parse().ok()?;
    let minutes: i32 = chiffres.get(2..4)?.parse().ok()?;
    Some(signe * (heures * 60 + minutes))
}

/// Fuseau IANA équivalent à une définition `VTIMEZONE` : par ses décalages
/// d'hiver et d'été, ou, sans heure d'été, par le fuseau fixe `Etc/GMT±N`.
fn par_decalages(definition: &Composant) -> Option<chrono_tz::Tz> {
    let decalage = |genre: &str| {
        definition
            .enfants
            .iter()
            .filter(|c| c.nom == genre)
            .filter_map(|c| c.propriete("TZOFFSETTO").and_then(|p| lire_decalage(&p.valeur)))
            .last()
    };
    let nom = match (decalage("STANDARD"), decalage("DAYLIGHT")) {
        (Some(60), Some(120)) => "Europe/Paris",
        (Some(0), Some(60)) => "Europe/London",
        (Some(120), Some(180)) => "Europe/Helsinki",
        (Some(-300), Some(-240)) => "America/New_York",
        (Some(-360), Some(-300)) => "America/Chicago",
        (Some(-420), Some(-360)) => "America/Denver",
        (Some(-480), Some(-420)) => "America/Los_Angeles",
        (Some(fixe), None) | (None, Some(fixe)) if fixe % 60 == 0 && (-720..=840).contains(&fixe) => {
            // Le signe des noms Etc/GMT est inversé : Etc/GMT-1 vaut UTC+1.
            return match fixe / 60 {
                0 => Some(chrono_tz::UTC),
                h => format!("Etc/GMT{:+}", -h).parse().ok(),
            };
        }
        _ => return None,
    };
    nom.parse().ok()
}

// ------------------------------------------------------------- moments

/// Moment d'un `DTSTART`, `DTEND`, `EXDATE`, `RDATE` ou `RECURRENCE-ID`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment {
    /// Journée entière (`VALUE=DATE`).
    Jour(NaiveDate),
    /// Instant précis : UTC, ou heure d'un fuseau connu.
    Instant(DateTime<Utc>),
    /// Heure « flottante » — ni fuseau ni Z — ou fuseau inconnu : l'heure du
    /// poste.
    Flottant(NaiveDateTime),
}

impl Moment {
    /// En temps universel. Une journée entière commence à minuit UTC : seule
    /// sa date compte, et l'arithmétique des journées reste exacte.
    fn utc(&self) -> DateTime<Utc> {
        match self {
            Moment::Jour(d) => Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()),
            Moment::Instant(i) => *i,
            Moment::Flottant(n) => local_vers_utc(&chrono::Local, n).unwrap_or_else(|| Utc.from_utc_datetime(n)),
        }
    }
}

/// Heure locale d'un fuseau vers le temps universel. Heure qui n'existe pas
/// (passage à l'heure d'été) : celle d'une heure plus tard ; heure doublée
/// (retour à l'heure d'hiver) : la première des deux.
fn local_vers_utc<T: TimeZone>(tz: &T, local: &NaiveDateTime) -> Option<DateTime<Utc>> {
    tz.from_local_datetime(local)
        .earliest()
        .or_else(|| tz.from_local_datetime(&(*local + Duration::hours(1))).earliest())
        .map(|l| l.with_timezone(&Utc))
}

fn lire_date(valeur: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(valeur.get(..8)?, "%Y%m%d").ok()
}

fn lire_date_heure(valeur: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(valeur.get(..15)?, "%Y%m%dT%H%M%S").ok()
}

/// Lit une date (`20261009`), une date-heure UTC (`20261007T150000Z`) ou
/// locale, celle-ci dans le fuseau donné.
fn lire_moment(valeur: &str, tz: Option<chrono_tz::Tz>, date_seule: bool) -> Option<Moment> {
    let valeur = valeur.trim();
    if date_seule || !valeur.contains('T') {
        return lire_date(valeur).map(Moment::Jour);
    }
    let local = lire_date_heure(valeur)?;
    if valeur.ends_with(['Z', 'z']) {
        return Some(Moment::Instant(Utc.from_utc_datetime(&local)));
    }
    match tz {
        Some(tz) => local_vers_utc(&tz, &local).map(Moment::Instant),
        None => Some(Moment::Flottant(local)),
    }
}

/// Durée d'un `DURATION` (`PT1H30M`, `P1D`, `-PT15M`).
fn lire_duree(valeur: &str) -> Option<Duration> {
    let v = valeur.trim();
    let (signe, v) = match v.strip_prefix('-') {
        Some(r) => (-1, r),
        None => (1, v.strip_prefix('+').unwrap_or(v)),
    };
    let mut total = Duration::zero();
    let mut nombre = String::new();
    let mut heure = false;
    for c in v.strip_prefix('P')?.chars() {
        match c {
            '0'..='9' => nombre.push(c),
            'T' => heure = true,
            'W' | 'D' | 'H' | 'M' | 'S' => {
                let n: i64 = std::mem::take(&mut nombre).parse().ok()?;
                total += match (c, heure) {
                    ('W', false) => Duration::weeks(n),
                    ('D', false) => Duration::days(n),
                    ('H', true) => Duration::hours(n),
                    ('M', true) => Duration::minutes(n),
                    ('S', true) => Duration::seconds(n),
                    _ => return None,
                };
            }
            _ => return None,
        }
    }
    Some(total * signe)
}

// ------------------------------------------------------------- événements

/// Un `VEVENT` lu.
#[derive(Debug, Clone)]
struct Evenement {
    uid: String,
    resume: String,
    lieu: String,
    description: String,
    debut: Moment,
    duree: Duration,
    fuseau: Option<chrono_tz::Tz>,
    regle: Option<String>,
    exclues: Vec<Moment>,
    ajoutees: Vec<Moment>,
    recurrence: Option<Moment>,
    annule: bool,
}

fn evenements(racine: &Composant) -> Vec<Evenement> {
    let definitions: Vec<Composant> = racine.enfants.iter().filter(|c| c.nom == "VTIMEZONE").cloned().collect();
    let tz_de = |p: &Propriete| p.param("TZID").and_then(|t| fuseau(t, &definitions));
    let moment = |p: &Propriete| lire_moment(&p.valeur, tz_de(p), p.date_seule());
    let moments = |p: &Propriete| -> Vec<Moment> {
        let tz = tz_de(p);
        p.valeur.split(',').filter_map(|v| lire_moment(v, tz, p.date_seule())).collect()
    };
    racine
        .enfants
        .iter()
        .filter(|c| c.nom == "VEVENT")
        .filter_map(|c| {
            let dtstart = c.propriete("DTSTART")?;
            let debut = moment(dtstart)?;
            let duree = match (c.propriete("DTEND").and_then(&moment), c.propriete("DURATION")) {
                (Some(fin), _) => fin.utc() - debut.utc(),
                (None, Some(d)) => lire_duree(&d.valeur).unwrap_or_else(Duration::zero),
                // Sans fin : une journée entière dure un jour, un instant rien.
                (None, None) if matches!(debut, Moment::Jour(_)) => Duration::days(1),
                (None, None) => Duration::zero(),
            };
            Some(Evenement {
                uid: c.texte("UID"),
                resume: c.texte("SUMMARY"),
                lieu: c.texte("LOCATION"),
                description: c.texte("DESCRIPTION"),
                debut,
                duree: duree.max(Duration::zero()),
                fuseau: tz_de(dtstart),
                regle: c.propriete("RRULE").map(|p| p.valeur.trim().to_string()),
                exclues: c.toutes("EXDATE").flat_map(&moments).collect(),
                ajoutees: c.toutes("RDATE").flat_map(&moments).collect(),
                recurrence: c.propriete("RECURRENCE-ID").and_then(&moment),
                annule: c.propriete("STATUS").is_some_and(|s| s.valeur.trim().eq_ignore_ascii_case("CANCELLED")),
            })
        })
        .collect()
}

/// Une occurrence à afficher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub uid: String,
    pub resume: String,
    pub lieu: String,
    pub description: String,
    /// Début et fin en temps universel.
    pub debut: DateTime<Utc>,
    pub fin: DateTime<Utc>,
    /// Journée entière : premier jour inclus, dernier exclu.
    pub journee: Option<(NaiveDate, NaiveDate)>,
    /// Annulée par l'organisateur (`STATUS:CANCELLED`) : montrée barrée.
    pub annule: bool,
    /// Agenda d'où elle vient, posé par l'appelant.
    pub agenda: i64,
}

fn occurrence(e: &Evenement, debut: Moment) -> Occurrence {
    let journee = match debut {
        Moment::Jour(d) => Some((d, d + Duration::days(e.duree.num_days().max(1)))),
        _ => None,
    };
    let debut = debut.utc();
    Occurrence {
        uid: e.uid.clone(),
        resume: e.resume.clone(),
        lieu: e.lieu.clone(),
        description: e.description.clone(),
        debut,
        fin: debut + e.duree,
        journee,
        annule: e.annule,
        agenda: 0,
    }
}

/// Occurrences d'un objet de l'agenda qui touchent `[de, a)`, triées.
pub fn occurrences(ical: &str, de: DateTime<Utc>, a: DateTime<Utc>) -> Vec<Occurrence> {
    let Some(racine) = analyser(ical) else { return Vec::new() };
    let evenements = evenements(&racine);
    let touche = |o: &Occurrence| o.debut < a && (o.fin > de || (o.fin == o.debut && o.debut >= de));
    let mut sortie = Vec::new();
    for maitre in evenements.iter().filter(|e| e.recurrence.is_none()) {
        // Occurrences remplacées par un composant RECURRENCE-ID du même UID.
        let remplacees: Vec<DateTime<Utc>> = evenements
            .iter()
            .filter(|e| e.uid == maitre.uid)
            .filter_map(|e| e.recurrence.map(|r| r.utc()))
            .collect();
        let debuts = match &maitre.regle {
            Some(regle) => repetitions(maitre, regle, Some((de - maitre.duree, a))).0,
            None => {
                let mut d = vec![maitre.debut];
                d.extend(maitre.ajoutees.iter().copied());
                d
            }
        };
        for debut in debuts {
            if remplacees.contains(&debut.utc()) {
                continue;
            }
            let o = occurrence(maitre, debut);
            if touche(&o) {
                sortie.push(o);
            }
        }
    }
    for remplacant in evenements.iter().filter(|e| e.recurrence.is_some()) {
        let o = occurrence(remplacant, remplacant.debut);
        if touche(&o) {
            sortie.push(o);
        }
    }
    sortie.sort_by(|x, y| (x.debut, &x.resume).cmp(&(y.debut, &y.resume)));
    sortie
}

/// Débuts des répétitions d'un événement — la règle, plus les RDATE, moins
/// les EXDATE — dans la période donnée, ou depuis le début sans période ; et
/// vrai si la liste est complète (sans borne atteinte). Une règle illisible ne
/// rend que l'occurrence d'origine.
fn repetitions(e: &Evenement, regle: &str, periode: Option<(DateTime<Utc>, DateTime<Utc>)>) -> (Vec<Moment>, bool) {
    use rrule::{RRule, Tz, Unvalidated};
    let tz = match (&e.debut, e.fuseau) {
        (Moment::Instant(_), Some(f)) => Tz::Tz(f),
        (Moment::Flottant(_), _) => Tz::LOCAL,
        _ => Tz::UTC,
    };
    let depart = match e.debut {
        Moment::Instant(i) => i.with_timezone(&tz),
        autre => autre.utc().with_timezone(&tz),
    };
    let seule = (vec![e.debut], true);
    let Ok(regle) = normaliser_until(regle, &tz).parse::<RRule<Unvalidated>>() else { return seule };
    let Ok(mut ensemble) = regle.build(depart) else { return seule };
    for x in &e.exclues {
        ensemble = ensemble.exdate(x.utc().with_timezone(&tz));
    }
    for x in &e.ajoutees {
        ensemble = ensemble.rdate(x.utc().with_timezone(&tz));
    }
    if let Some((de, a)) = periode {
        ensemble = ensemble.after(de.with_timezone(&Tz::UTC)).before(a.with_timezone(&Tz::UTC));
    }
    let resultat = ensemble.all(REPETITIONS_MAX);
    let debuts = resultat
        .dates
        .into_iter()
        .map(|d| match e.debut {
            Moment::Jour(_) => Moment::Jour(d.with_timezone(&Utc).date_naive()),
            _ => Moment::Instant(d.with_timezone(&Utc)),
        })
        .collect();
    (debuts, !resultat.limited)
}

/// `UNTIL` doit être en UTC quand le départ a un fuseau (RFC 5545) ; bien des
/// logiciels l'écrivent en date seule ou en heure locale. On le ramène en UTC
/// — fin de journée pour une date seule — plutôt que de rejeter la règle.
fn normaliser_until(regle: &str, tz: &rrule::Tz) -> String {
    regle
        .trim()
        .trim_start_matches("RRULE:")
        .split(';')
        .filter(|p| !p.is_empty())
        .map(|partie| {
            let Some(valeur) = partie.strip_prefix("UNTIL=") else { return partie.to_string() };
            if valeur.ends_with('Z') || tz.is_local() {
                return partie.to_string();
            }
            let local = if valeur.contains('T') {
                lire_date_heure(valeur)
            } else {
                lire_date(valeur).and_then(|d| d.and_hms_opt(23, 59, 59))
            };
            match local.and_then(|l| tz.from_local_datetime(&l).earliest()) {
                Some(l) => format!("UNTIL={}", l.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ")),
                None => partie.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Période qu'occupe un objet de l'agenda, en secondes Unix, pour le retrouver
/// sans relire tous les objets : du premier début à la dernière fin, la fin
/// valant `i64::MAX` pour une répétition sans terme.
pub fn etendue(ical: &str) -> Option<(i64, i64)> {
    let racine = analyser(ical)?;
    let evenements = evenements(&racine);
    let debut = evenements.iter().map(|e| e.debut.utc().timestamp()).min()?;
    let mut fin = debut;
    for e in &evenements {
        let mut dernier = e.debut.utc();
        for x in &e.ajoutees {
            dernier = dernier.max(x.utc());
        }
        if let Some(regle) = &e.regle {
            let majuscules = regle.to_ascii_uppercase();
            let (debuts, complet) = if majuscules.contains("UNTIL=") || majuscules.contains("COUNT=") {
                repetitions(e, regle, None)
            } else {
                (Vec::new(), false)
            };
            if !complet {
                return Some((debut, i64::MAX));
            }
            if let Some(d) = debuts.iter().map(Moment::utc).max() {
                dernier = dernier.max(d);
            }
        }
        fin = fin.max((dernier + e.duree).timestamp());
    }
    Some((debut, fin))
}

// ------------------------------------------------------------- disposition

const JOURS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const MOIS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre",
    "décembre",
];

/// « jeudi 1er octobre 2026 ».
pub fn date_longue(d: NaiveDate) -> String {
    let jour = if d.day() == 1 { "1er".to_string() } else { d.day().to_string() };
    format!("{} {} {} {}", JOURS[d.weekday().num_days_from_monday() as usize], jour, MOIS[d.month0() as usize], d.year())
}

/// « du vendredi 9 au samedi 10 octobre 2026 » : le mois et l'année ne se
/// répètent pas quand ils sont communs.
pub fn plage_de_jours(premier: NaiveDate, dernier: NaiveDate) -> String {
    let complet = date_longue(dernier);
    let debut = date_longue(premier);
    let mut mots: Vec<&str> = debut.split(' ').collect();
    if premier.year() == dernier.year() {
        mots.pop();
        if premier.month() == dernier.month() {
            mots.pop();
        }
    }
    format!("du {} au {complet}", mots.join(" "))
}

fn heure(d: &NaiveDateTime) -> String {
    format!("{:02}:{:02}", d.hour(), d.minute())
}

/// Ce que montre la fiche d'une occurrence.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Fiche {
    pub titre: String,
    pub lieu: String,
    pub description: String,
    pub quand: String,
    pub agenda: i64,
    pub annule: bool,
    /// Nom et couleur de l'agenda, posés par `vue`.
    #[serde(rename = "nomAgenda")]
    pub nom_agenda: String,
    pub couleur: String,
}

/// Une occurrence placée dans le bandeau des journées entières : colonnes
/// `de` à `a` (exclue), sur la ligne `ligne`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Bande {
    pub de: u32,
    pub a: u32,
    pub ligne: u32,
    pub fiche: usize,
}

/// Une occurrence placée dans la grille des heures d'un jour : minutes de
/// début et de fin dans la journée, colonne parmi celles qui se chevauchent.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Case {
    pub jour: u32,
    pub debut: u32,
    pub fin: u32,
    pub colonne: u32,
    pub colonnes: u32,
    /// « 09:30 – 11:00 », ou vide pour un morceau qui ne commence pas ce jour-là.
    pub heures: String,
    pub fiche: usize,
}

/// Vue jour ou semaine.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Grille {
    pub bandes: Vec<Bande>,
    pub lignes: u32,
    pub cases: Vec<Case>,
    pub fiches: Vec<Fiche>,
}

/// Une entrée d'une journée de la vue mois.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Entree {
    /// Heure de début, vide pour une journée entière ou une suite.
    pub heure: String,
    pub fiche: usize,
}

/// Vue mois : les journées dans l'ordre, chacune avec ses entrées.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Mois {
    pub jours: Vec<Vec<Entree>>,
    pub fiches: Vec<Fiche>,
}

/// Fin d'affichage d'une occurrence instantanée ou très courte : une demi-heure,
/// de quoi lire son titre.
const DUREE_AFFICHEE_MIN: u32 = 30;

/// Où tombe une occurrence dans le fuseau `tz`, en journées locales : la
/// première, la dernière exclue, et vrai pour une journée entière ou une
/// occurrence de 24 heures ou plus, qui vont au bandeau.
fn jours_couverts<T: TimeZone>(o: &Occurrence, tz: &T) -> (NaiveDate, NaiveDate, bool) {
    if let Some((d1, d2)) = o.journee {
        return (d1, d2, true);
    }
    let debut = o.debut.with_timezone(tz).naive_local();
    let fin = o.fin.with_timezone(tz).naive_local();
    // Une fin à minuit pile n'occupe pas le jour qui commence.
    let dernier = if fin > debut && fin.time() == chrono::NaiveTime::MIN { fin.date() } else { fin.date() + Duration::days(1) };
    (debut.date(), dernier.max(debut.date() + Duration::days(1)), o.fin - o.debut >= Duration::hours(24))
}

fn fiche<T: TimeZone>(o: &Occurrence, tz: &T) -> Fiche {
    let quand = match o.journee {
        Some((d1, d2)) if d2 - d1 <= Duration::days(1) => format!("{}, toute la journée", date_longue(d1)),
        Some((d1, d2)) => format!("{}, toute la journée", plage_de_jours(d1, d2 - Duration::days(1))),
        None => {
            let debut = o.debut.with_timezone(tz).naive_local();
            let fin = o.fin.with_timezone(tz).naive_local();
            if debut.date() == fin.date() || fin == debut {
                format!("{}, {} – {}", date_longue(debut.date()), heure(&debut), heure(&fin))
            } else {
                format!("du {} {} au {} {}", date_longue(debut.date()), heure(&debut), date_longue(fin.date()), heure(&fin))
            }
        }
    };
    Fiche {
        titre: if o.resume.is_empty() { "(sans titre)".into() } else { o.resume.clone() },
        lieu: o.lieu.clone(),
        description: o.description.clone(),
        quand,
        agenda: o.agenda,
        annule: o.annule,
        nom_agenda: String::new(),
        couleur: String::new(),
    }
}

/// Dispose des occurrences sur `jours` journées à partir de `premier`, dans
/// le fuseau `tz` (celui du poste).
pub fn grille<T: TimeZone>(occurrences: &[Occurrence], premier: NaiveDate, jours: u32, tz: &T) -> Grille {
    let dernier = premier + Duration::days(jours as i64);
    let colonne = |d: NaiveDate| (d - premier).num_days().clamp(0, jours as i64) as u32;
    let mut g = Grille::default();
    // Morceaux de chaque jour : (début, fin affichée, case).
    let mut par_jour: Vec<Vec<Case>> = vec![Vec::new(); jours as usize];
    let mut bandes: Vec<(u32, u32, usize)> = Vec::new();
    for o in occurrences {
        let (d1, d2, bandeau) = jours_couverts(o, tz);
        if d2 <= premier || d1 >= dernier {
            continue;
        }
        let indice = g.fiches.len();
        g.fiches.push(fiche(o, tz));
        if bandeau {
            bandes.push((colonne(d1), colonne(d2), indice));
            continue;
        }
        let debut = o.debut.with_timezone(tz).naive_local();
        let fin = o.fin.with_timezone(tz).naive_local();
        let mut jour = d1.max(premier);
        while jour < d2.min(dernier) {
            let minutes = |t: &NaiveDateTime| t.hour() * 60 + t.minute();
            let m1 = if jour == debut.date() { minutes(&debut) } else { 0 };
            let m2 = if jour == fin.date() { minutes(&fin) } else { 24 * 60 };
            par_jour[colonne(jour) as usize].push(Case {
                jour: colonne(jour),
                debut: m1,
                fin: m2.max(m1 + DUREE_AFFICHEE_MIN).min(24 * 60).max(m1),
                colonne: 0,
                colonnes: 1,
                heures: if jour == debut.date() { format!("{} – {}", heure(&debut), heure(&fin)) } else { String::new() },
                fiche: indice,
            });
            jour += Duration::days(1);
        }
    }
    for cases in &mut par_jour {
        disposer_colonnes(cases);
        g.cases.append(cases);
    }
    // Bandeau : les plus anciennes d'abord, les plus longues avant les autres,
    // chacune sur la première ligne où elle ne chevauche rien.
    bandes.sort_by_key(|(de, a, i)| (*de, std::cmp::Reverse(*a), *i));
    let mut lignes: Vec<Vec<(u32, u32)>> = Vec::new();
    for (de, a, fiche) in bandes {
        let ligne = match lignes.iter().position(|l| l.iter().all(|&(x, y)| a <= x || de >= y)) {
            Some(n) => n,
            None => {
                lignes.push(Vec::new());
                lignes.len() - 1
            }
        };
        lignes[ligne].push((de, a));
        g.bandes.push(Bande { de, a, ligne: ligne as u32, fiche });
    }
    g.lignes = lignes.len() as u32;
    g
}

/// Colonnes des cases d'une journée : celles qui se chevauchent, de proche en
/// proche, forment un groupe partagé en autant de colonnes qu'il en faut ;
/// chaque case prend la première colonne libre.
fn disposer_colonnes(cases: &mut [Case]) {
    cases.sort_by_key(|c| (c.debut, std::cmp::Reverse(c.fin), c.fiche));
    let mut groupe_debut = 0;
    let mut groupe_fin = 0;
    let mut fins: Vec<u32> = Vec::new();
    for i in 0..cases.len() {
        if i > 0 && cases[i].debut >= groupe_fin {
            for c in &mut cases[groupe_debut..i] {
                c.colonnes = fins.len() as u32;
            }
            groupe_debut = i;
            fins.clear();
        }
        let colonne = match fins.iter().position(|&f| f <= cases[i].debut) {
            Some(n) => n,
            None => {
                fins.push(0);
                fins.len() - 1
            }
        };
        fins[colonne] = cases[i].fin;
        cases[i].colonne = colonne as u32;
        groupe_fin = if i == groupe_debut { cases[i].fin } else { groupe_fin.max(cases[i].fin) };
    }
    let n = fins.len() as u32;
    for c in &mut cases[groupe_debut..] {
        c.colonnes = n;
    }
}

/// Dispose des occurrences sur `jours` journées à partir de `premier`, pour
/// la vue mois : chaque journée liste ce qu'elle touche, journées entières
/// d'abord, puis par heure.
pub fn mois<T: TimeZone>(occurrences: &[Occurrence], premier: NaiveDate, jours: u32, tz: &T) -> Mois {
    let dernier = premier + Duration::days(jours as i64);
    let mut m = Mois { jours: vec![Vec::new(); jours as usize], fiches: Vec::new() };
    let mut cles: Vec<Vec<(u8, NaiveDateTime)>> = vec![Vec::new(); jours as usize];
    for o in occurrences {
        let (d1, d2, bandeau) = jours_couverts(o, tz);
        if d2 <= premier || d1 >= dernier {
            continue;
        }
        let indice = m.fiches.len();
        m.fiches.push(fiche(o, tz));
        let debut = o.debut.with_timezone(tz).naive_local();
        let mut jour = d1.max(premier);
        while jour < d2.min(dernier) {
            let i = (jour - premier).num_days() as usize;
            let premier_jour = jour == debut.date();
            let journee = bandeau || o.journee.is_some() || !premier_jour;
            m.jours[i].push(Entree { heure: if journee { String::new() } else { heure(&debut) }, fiche: indice });
            cles[i].push((if journee { 0 } else { 1 }, if journee { jour.and_hms_opt(0, 0, 0).unwrap() } else { debut }));
            jour += Duration::days(1);
        }
    }
    for (entrees, cles) in m.jours.iter_mut().zip(cles) {
        let mut paires: Vec<(_, Entree)> = cles.into_iter().zip(entrees.drain(..)).collect();
        paires.sort_by(|(x, ex), (y, ey)| x.cmp(y).then(ex.fiche.cmp(&ey.fiche)));
        entrees.extend(paires.into_iter().map(|(_, e)| e));
    }
    m
}

// ------------------------------------------------------------- vues

/// Couleurs données, dans l'ordre, aux agendas que le serveur ne colore pas.
const PALETTE: [&str; 8] = ["#3A7BD5", "#D9534F", "#4E9A06", "#E39B1B", "#8E6BBF", "#2BAAB1", "#C0679A", "#7F8C8D"];

/// Couleur d'un agenda : la sienne, sinon une de la palette. SOGo rend
/// `#AAAAAA` pour un agenda dont l'utilisateur n'a pas choisi la couleur : un
/// gris qui ferait passer chaque événement pour désactivé.
pub fn couleur(agenda: &AgendaLocal) -> String {
    if agenda.couleur.is_empty() || agenda.couleur == "#AAAAAA" {
        PALETTE[(agenda.id - 1).rem_euclid(PALETTE.len() as i64) as usize].to_string()
    } else {
        agenda.couleur.clone()
    }
}

const JOURS_COURTS: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];

/// Premier jour et nombre de jours d'une vue : la journée, la semaine du lundi
/// au dimanche, ou les six semaines qui couvrent le mois.
pub fn periode(genre: &str, date: NaiveDate) -> (NaiveDate, u32) {
    let lundi = |d: NaiveDate| d - Duration::days(d.weekday().num_days_from_monday() as i64);
    match genre {
        "jour" => (date, 1),
        "mois" => (lundi(date.with_day(1).unwrap_or(date)), 42),
        _ => (lundi(date), 7),
    }
}

/// « 5 – 11 octobre 2026 », « 28 septembre – 4 octobre 2026 ».
pub fn plage_courte(premier: NaiveDate, dernier: NaiveDate) -> String {
    let fin = format!("{} {} {}", dernier.day(), MOIS[dernier.month0() as usize], dernier.year());
    let debut = if premier.year() != dernier.year() {
        format!("{} {} {}", premier.day(), MOIS[premier.month0() as usize], premier.year())
    } else if premier.month() != dernier.month() {
        format!("{} {}", premier.day(), MOIS[premier.month0() as usize])
    } else {
        premier.day().to_string()
    };
    format!("{debut} – {fin}")
}

fn majuscule(texte: &str) -> String {
    let mut lettres = texte.chars();
    match lettres.next() {
        Some(p) => p.to_uppercase().chain(lettres).collect(),
        None => String::new(),
    }
}

/// Vue de l'agenda autour de `date`, dans le fuseau du poste, en JSON pour
/// l'interface.
pub fn vue(magasin: &Magasin, genre: &str, date: NaiveDate) -> String {
    vue_dans(magasin, genre, date, chrono::Local::now().date_naive(), &chrono::Local)
}

pub fn vue_dans<T: TimeZone>(magasin: &Magasin, genre: &str, date: NaiveDate, aujourdhui: NaiveDate, tz: &T) -> String {
    let (premier, jours) = periode(genre, date);
    let minuit = |d: NaiveDate| local_vers_utc(tz, &d.and_hms_opt(0, 0, 0).unwrap()).unwrap_or_else(|| Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()));
    let (de, a) = (minuit(premier), minuit(premier + Duration::days(jours as i64)));
    let agendas = magasin.agendas().unwrap_or_default();
    let mut liste = Vec::new();
    for (agenda, ical) in magasin.evenements_periode(de.timestamp(), a.timestamp()).unwrap_or_default() {
        for mut o in occurrences(&ical, de, a) {
            o.agenda = agenda;
            liste.push(o);
        }
    }
    liste.sort_by(|x, y| (x.debut, x.fin, &x.resume, x.agenda).cmp(&(y.debut, y.fin, &y.resume, y.agenda)));
    let colorer = |fiches: &mut Vec<Fiche>| {
        for f in fiches {
            if let Some(a) = agendas.iter().find(|a| a.id == f.agenda) {
                f.nom_agenda = a.nom.clone();
                f.couleur = couleur(a);
            }
        }
    };
    let dernier = premier + Duration::days(jours as i64 - 1);
    let titre = match genre {
        "jour" => majuscule(&date_longue(date)),
        "mois" => majuscule(&format!("{} {}", MOIS[date.month0() as usize], date.year())),
        _ => plage_courte(premier, dernier),
    };
    let entetes: Vec<_> = (0..jours)
        .map(|i| {
            let d = premier + Duration::days(i as i64);
            json!({
                "date": d.format("%Y-%m-%d").to_string(),
                "jour": d.day(),
                "semaine": JOURS_COURTS[d.weekday().num_days_from_monday() as usize],
                "aujourdhui": d == aujourdhui,
                "horsMois": genre == "mois" && d.month() != date.month(),
            })
        })
        .collect();
    let mut sortie = json!({ "genre": genre, "titre": titre, "jours": entetes, "nombre": liste.len() });
    if genre == "mois" {
        let mut m = mois(&liste, premier, jours, tz);
        colorer(&mut m.fiches);
        sortie["entrees"] = json!(m.jours);
        sortie["fiches"] = json!(m.fiches);
    } else {
        let mut g = grille(&liste, premier, jours, tz);
        colorer(&mut g.fiches);
        sortie["bandes"] = json!(g.bandes);
        sortie["lignes"] = json!(g.lignes);
        sortie["cases"] = json!(g.cases);
        sortie["fiches"] = json!(g.fiches);
    }
    sortie.to_string()
}

#[cfg(test)]
mod tests;
