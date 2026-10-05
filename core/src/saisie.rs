// SPDX-License-Identifier: GPL-3.0-or-later
//! Saisie d'un événement : création, modification de la série ou d'une seule
//! occurrence, retrait d'une occurrence ; et le formulaire prérempli d'un
//! événement existant.
//!
//! Une modification part de l'objet tel que l'index le garde et ne touche
//! que ce que le formulaire montre : participants, propriétés propres à un
//! autre logiciel (`X-…`), définitions de fuseaux et occurrences déplacées
//! restent tels quels. Les heures s'écrivent dans le fuseau du poste, nommé à
//! la manière IANA (« Europe/Paris »), avec sa définition `VTIMEZONE`.

use std::hash::{BuildHasher, Hasher};

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, Offset, TimeZone, Utc, Weekday};
use serde::{Deserialize, Serialize};

use crate::agenda::{self, analyser, echapper, Alarme, Composant, Moment, Propriete};

/// Le formulaire d'un événement, tel que l'interface l'échange avec le noyau.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Saisie {
    pub agenda: i64,
    /// Objet de l'index ; 0 pour un nouvel événement.
    pub objet: i64,
    /// Occurrence visée — son début d'origine, secondes Unix, en texte — ;
    /// vide pour un événement simple ou toute la série.
    pub occurrence: String,
    pub titre: String,
    pub lieu: String,
    pub description: String,
    pub journee: bool,
    /// « 2026-10-07T09:30 » ; une journée entière, « 2026-10-07 ».
    pub debut: String,
    /// Fin ; d'une journée entière, le dernier jour, inclus.
    pub fin: String,
    /// "", "DAILY", "WEEKDAYS", "WEEKLY", "MONTHLY", "YEARLY" ; "AUTRE" pour
    /// une règle que le formulaire ne sait pas décrire, conservée telle quelle.
    pub repetition: String,
    /// Dernier jour de la répétition, « 2026-12-31 », ou vide.
    #[serde(rename = "jusquAu")]
    pub jusqu_au: String,
    /// Minutes avant le début ; -1 : pas de rappel.
    pub rappel: i64,
}

/// Fuseau du poste, nommé à la manière IANA : la variable `TZ` si elle en
/// nomme un — comme le fait chrono —, sinon celui du système ; `None` s'il ne
/// se laisse pas nommer. Saisie, vues et rappels passent tous par lui : une
/// heure saisie s'affiche telle quelle.
pub fn nom_du_fuseau() -> Option<chrono_tz::Tz> {
    std::env::var("TZ")
        .ok()
        .and_then(|t| t.trim_start_matches(':').parse().ok())
        .or_else(|| iana_time_zone::get_timezone().ok().and_then(|n| n.parse().ok()))
}

/// Fuseau dans lequel s'écrivent les heures saisies ; UTC s'il ne se laisse
/// pas nommer.
pub fn fuseau_du_poste() -> chrono_tz::Tz {
    nom_du_fuseau().unwrap_or(chrono_tz::UTC)
}

/// Identifiant d'un nouvel objet, au format d'un UUID : aléatoire, SOGo
/// écrasant sans prévenir un objet de même nom.
pub fn nouvel_uid() -> String {
    let horloge = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let mut bits = [0u64; 2];
    for (i, b) in bits.iter_mut().enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(horloge);
        h.write_usize(i);
        h.write_u32(std::process::id());
        *b = h.finish();
    }
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        bits[0] >> 32,
        (bits[0] >> 16) & 0xffff,
        bits[0] & 0xfff,
        0x8000 | ((bits[1] >> 48) & 0x3fff),
        bits[1] & 0xffff_ffff_ffff
    )
}

// ------------------------------------------------------------- dates

fn lire_jour(texte: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(texte.trim().get(..10).unwrap_or(""), "%Y-%m-%d").map_err(|_| format!("date invalide : {texte}"))
}

fn lire_local(texte: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(texte.trim(), "%Y-%m-%dT%H:%M").map_err(|_| format!("date et heure invalides : {texte}"))
}

const JOURS_ICAL: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

fn jour_ical(j: Weekday) -> &'static str {
    JOURS_ICAL[j.num_days_from_monday() as usize]
}

fn ecrire_decalage(secondes: i32) -> String {
    let signe = if secondes < 0 { '-' } else { '+' };
    let m = secondes.abs() / 60;
    format!("{signe}{:02}{:02}", m / 60, m % 60)
}

/// Définition `VTIMEZONE` d'un fuseau, par ses changements d'heure de
/// l'année donnée, mis en règle annuelle (« dernier dimanche de mars »).
pub fn vtimezone(tz: chrono_tz::Tz, annee: i32) -> Composant {
    let decalage = |i: DateTime<Utc>| tz.offset_from_utc_datetime(&i.naive_utc()).fix().local_minus_utc();
    let mut racine = Composant { nom: "VTIMEZONE".into(), ..Default::default() };
    racine.proprietes.push(Propriete::nouvelle("TZID", &[], tz.name()));
    let debut = Utc.with_ymd_and_hms(annee, 1, 1, 0, 0, 0).unwrap();
    let mut transitions = Vec::new();
    let mut avant = decalage(debut);
    let mut instant = debut;
    while instant.year() == annee {
        let suivant = instant + Duration::hours(1);
        let apres = decalage(suivant);
        if apres != avant {
            transitions.push((suivant, avant, apres));
            avant = apres;
        }
        instant = suivant;
    }
    if transitions.is_empty() {
        let mut standard = Composant { nom: "STANDARD".into(), ..Default::default() };
        standard.proprietes.push(Propriete::nouvelle("DTSTART", &[], "19700101T000000"));
        standard.proprietes.push(Propriete::nouvelle("TZOFFSETFROM", &[], &ecrire_decalage(avant)));
        standard.proprietes.push(Propriete::nouvelle("TZOFFSETTO", &[], &ecrire_decalage(avant)));
        racine.enfants.push(standard);
        return racine;
    }
    for (instant, de, vers) in transitions {
        // Heure locale du changement, dans le décalage d'avant.
        let local = (instant + Duration::seconds(de as i64)).naive_utc();
        let jour = local.day();
        let dans_le_mois = NaiveDate::from_ymd_opt(local.year(), local.month(), 1)
            .and_then(|d| d.checked_add_months(chrono::Months::new(1)))
            .map(|d| (d - Duration::days(1)).day())
            .unwrap_or(31);
        let rang: i32 = if jour + 7 > dans_le_mois { -1 } else { ((jour - 1) / 7 + 1) as i32 };
        let mut c = Composant { nom: if vers > de { "DAYLIGHT".into() } else { "STANDARD".into() }, ..Default::default() };
        // Premier changement selon cette règle en 1970, comme l'écrivent les
        // logiciels courants : la définition vaut pour toutes les années.
        let premier = nieme_jour(1970, local.month(), local.weekday(), rang).unwrap_or(local.date());
        let dtstart = premier.and_time(local.time());
        c.proprietes.push(Propriete::nouvelle("DTSTART", &[], &dtstart.format("%Y%m%dT%H%M%S").to_string()));
        c.proprietes.push(Propriete::nouvelle(
            "RRULE",
            &[],
            &format!("FREQ=YEARLY;BYMONTH={};BYDAY={rang}{}", local.month(), jour_ical(local.weekday())),
        ));
        c.proprietes.push(Propriete::nouvelle("TZOFFSETFROM", &[], &ecrire_decalage(de)));
        c.proprietes.push(Propriete::nouvelle("TZOFFSETTO", &[], &ecrire_decalage(vers)));
        racine.enfants.push(c);
    }
    racine
}

/// Le `rang`-ième jour `jour` du mois (-1 : le dernier).
fn nieme_jour(annee: i32, mois: u32, jour: Weekday, rang: i32) -> Option<NaiveDate> {
    if rang > 0 {
        NaiveDate::from_weekday_of_month_opt(annee, mois, jour, rang as u8)
    } else {
        let mut d = NaiveDate::from_ymd_opt(annee, mois, 1)?.checked_add_months(chrono::Months::new(1))? - Duration::days(1);
        while d.weekday() != jour {
            d -= Duration::days(1);
        }
        Some(d)
    }
}

// ------------------------------------------------------------- formulaire

/// Règle reconnue par le formulaire : sa répétition et son dernier jour.
fn repetition_de(regle: &str, debut: NaiveDate, tz: chrono_tz::Tz) -> (String, String) {
    let mut freq = "";
    let mut jours = "";
    let mut jusqu_au = String::new();
    let mut autre = false;
    for partie in regle.trim().trim_start_matches("RRULE:").split(';').filter(|p| !p.is_empty()) {
        let (cle, valeur) = partie.split_once('=').unwrap_or((partie, ""));
        match cle.to_ascii_uppercase().as_str() {
            "FREQ" => freq = valeur,
            "BYDAY" => jours = valeur,
            "WKST" => {}
            "INTERVAL" if valeur == "1" => {}
            "BYMONTHDAY" if valeur.parse::<u32>().ok() == Some(debut.day()) => {}
            "BYMONTH" if valeur.parse::<u32>().ok() == Some(debut.month()) => {}
            "UNTIL" => {
                jusqu_au = match agenda::lire_moment(valeur, Some(chrono_tz::UTC), !valeur.contains('T')) {
                    Some(Moment::Jour(d)) => d.format("%Y-%m-%d").to_string(),
                    Some(Moment::Instant(i)) => i.with_timezone(&tz).date_naive().format("%Y-%m-%d").to_string(),
                    _ => String::new(),
                }
            }
            _ => autre = true,
        }
    }
    let repetition = match (freq.to_ascii_uppercase().as_str(), jours.to_ascii_uppercase()) {
        _ if autre => "AUTRE",
        ("DAILY", j) if j.is_empty() => "DAILY",
        ("WEEKLY", j) if j == "MO,TU,WE,TH,FR" => "WEEKDAYS",
        ("WEEKLY", j) if j.is_empty() || j == jour_ical(debut.weekday()) => "WEEKLY",
        ("MONTHLY", j) if j.is_empty() => "MONTHLY",
        ("YEARLY", j) if j.is_empty() => "YEARLY",
        _ => "AUTRE",
    };
    (repetition.into(), if repetition == "AUTRE" { String::new() } else { jusqu_au })
}

/// Minutes du premier rappel avant le début, -1 sans rappel. Un rappel d'une
/// autre forme (à la fin, à heure fixe) compte comme « pas de rappel » pour le
/// formulaire, et reste en place tant que le rappel n'est pas changé.
fn rappel_de(c: &Composant) -> i64 {
    agenda::alarmes(c)
        .into_iter()
        .find_map(|a| match a {
            Alarme::Relative { decalage, fin: false } if decalage <= Duration::zero() => Some(-decalage.num_minutes()),
            _ => None,
        })
        .unwrap_or(-1)
}

fn maitre(racine: &Composant) -> Option<usize> {
    racine.enfants.iter().position(|c| c.nom == "VEVENT" && c.propriete("RECURRENCE-ID").is_none())
}

/// Moment d'une propriété de date, fuseau résolu par les définitions de
/// l'objet.
fn moment_de(racine: &Composant, p: &Propriete) -> Option<Moment> {
    let definitions: Vec<Composant> = racine.enfants.iter().filter(|c| c.nom == "VTIMEZONE").cloned().collect();
    let tz = p.param("TZID").and_then(|t| agenda::fuseau(t, &definitions));
    agenda::lire_moment(&p.valeur, tz, p.date_seule())
}

fn bornes(racine: &Composant, c: &Composant) -> Option<(Moment, Duration)> {
    let debut = moment_de(racine, c.propriete("DTSTART")?)?;
    let duree = match (c.propriete("DTEND").and_then(|p| moment_de(racine, p)), c.propriete("DURATION")) {
        (Some(fin), _) => fin.utc() - debut.utc(),
        (None, Some(d)) => agenda::lire_duree(&d.valeur).unwrap_or_else(Duration::zero),
        (None, None) if matches!(debut, Moment::Jour(_)) => Duration::days(1),
        (None, None) => Duration::zero(),
    };
    Some((debut, duree.max(Duration::zero())))
}

fn origine_de(racine: &Composant, c: &Composant) -> Option<i64> {
    c.propriete("RECURRENCE-ID").and_then(|p| moment_de(racine, p)).map(|m| m.utc().timestamp())
}

/// Formulaire prérempli d'un événement : la série (ou l'événement simple)
/// sans `occurrence`, sinon l'occurrence dont c'est le début d'origine.
pub fn saisie_de(ical: &str, occurrence: Option<i64>, tz: chrono_tz::Tz) -> Option<Saisie> {
    let racine = analyser(ical)?;
    let m = &racine.enfants[maitre(&racine)?];
    let remplacante = occurrence.and_then(|o| {
        racine.enfants.iter().find(|c| c.nom == "VEVENT" && origine_de(&racine, c) == Some(o))
    });
    let source = remplacante.unwrap_or(m);
    let (mut debut, duree) = bornes(&racine, source)?;
    if let (None, Some(o)) = (remplacante, occurrence) {
        let instant = Utc.timestamp_opt(o, 0).single()?;
        debut = match debut {
            Moment::Jour(_) => Moment::Jour(instant.date_naive()),
            _ => Moment::Instant(instant),
        };
    }
    let (debut_texte, fin_texte, journee, jour_debut) = match debut {
        Moment::Jour(d) => {
            let dernier = d + Duration::days(duree.num_days().max(1) - 1);
            (d.format("%Y-%m-%d").to_string(), dernier.format("%Y-%m-%d").to_string(), true, d)
        }
        autre => {
            let d = autre.utc().with_timezone(&tz).naive_local();
            let f = (autre.utc() + duree).with_timezone(&tz).naive_local();
            (d.format("%Y-%m-%dT%H:%M").to_string(), f.format("%Y-%m-%dT%H:%M").to_string(), false, d.date())
        }
    };
    let (repetition, jusqu_au) = match (occurrence, m.propriete("RRULE")) {
        (None, Some(r)) => repetition_de(&r.valeur, jour_debut, tz),
        _ => (String::new(), String::new()),
    };
    Some(Saisie {
        agenda: 0,
        objet: 0,
        occurrence: occurrence.map(|o| o.to_string()).unwrap_or_default(),
        titre: source.texte("SUMMARY"),
        lieu: source.texte("LOCATION"),
        description: source.texte("DESCRIPTION"),
        journee,
        debut: debut_texte,
        fin: fin_texte,
        repetition,
        jusqu_au,
        rappel: rappel_de(source),
    })
}

// ------------------------------------------------------------- écriture

/// Début et fin vérifiés d'une saisie.
enum Bornes {
    Jours(NaiveDate, NaiveDate),
    Heures(NaiveDateTime, NaiveDateTime),
}

fn bornes_de(s: &Saisie) -> Result<Bornes, String> {
    if s.journee {
        let (d, f) = (lire_jour(&s.debut)?, lire_jour(&s.fin)?);
        if f < d {
            return Err("le dernier jour précède le premier".into());
        }
        Ok(Bornes::Jours(d, f + Duration::days(1)))
    } else {
        let (d, f) = (lire_local(&s.debut)?, lire_local(&s.fin)?);
        if f < d {
            return Err("la fin précède le début".into());
        }
        Ok(Bornes::Heures(d, f))
    }
}

fn horodatage(i: DateTime<Utc>) -> String {
    i.format("%Y%m%dT%H%M%SZ").to_string()
}

/// DTSTART et DTEND dans le fuseau du poste (UTC si le poste n'en nomme pas).
fn poser_bornes(c: &mut Composant, bornes: &Bornes, tz: chrono_tz::Tz) {
    c.retirer("DURATION");
    match bornes {
        Bornes::Jours(d, f) => {
            c.poser(Propriete::nouvelle("DTSTART", &[("VALUE", "DATE")], &d.format("%Y%m%d").to_string()));
            c.poser(Propriete::nouvelle("DTEND", &[("VALUE", "DATE")], &f.format("%Y%m%d").to_string()));
        }
        Bornes::Heures(d, f) if tz == chrono_tz::UTC => {
            c.poser(Propriete::nouvelle("DTSTART", &[], &d.format("%Y%m%dT%H%M%SZ").to_string()));
            c.poser(Propriete::nouvelle("DTEND", &[], &f.format("%Y%m%dT%H%M%SZ").to_string()));
        }
        Bornes::Heures(d, f) => {
            let tzid = [("TZID", tz.name())];
            c.poser(Propriete::nouvelle("DTSTART", &tzid, &d.format("%Y%m%dT%H%M%S").to_string()));
            c.poser(Propriete::nouvelle("DTEND", &tzid, &f.format("%Y%m%dT%H%M%S").to_string()));
        }
    }
}

/// Règle de répétition d'une saisie, ou `None` sans répétition.
fn regle_de(s: &Saisie, bornes: &Bornes, tz: chrono_tz::Tz) -> Result<Option<String>, String> {
    let debut = match bornes {
        Bornes::Jours(d, _) => *d,
        Bornes::Heures(d, _) => d.date(),
    };
    let mut regle = match s.repetition.as_str() {
        "" => return Ok(None),
        "DAILY" => "FREQ=DAILY".to_string(),
        "WEEKDAYS" => "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR".to_string(),
        "WEEKLY" => format!("FREQ=WEEKLY;BYDAY={}", jour_ical(debut.weekday())),
        "MONTHLY" => format!("FREQ=MONTHLY;BYMONTHDAY={}", debut.day()),
        "YEARLY" => "FREQ=YEARLY".to_string(),
        autre => return Err(format!("répétition inconnue : {autre}")),
    };
    if !s.jusqu_au.trim().is_empty() {
        let dernier = lire_jour(&s.jusqu_au)?;
        if dernier < debut {
            return Err("la répétition s'arrête avant le premier jour".into());
        }
        match bornes {
            Bornes::Jours(..) => regle.push_str(&format!(";UNTIL={}", dernier.format("%Y%m%d"))),
            Bornes::Heures(..) => {
                let fin = agenda::local_vers_utc(&tz, &dernier.and_hms_opt(23, 59, 59).unwrap())
                    .ok_or("dernier jour de la répétition invalide")?;
                regle.push_str(&format!(";UNTIL={}", horodatage(fin)));
            }
        }
    }
    Ok(Some(regle))
}

fn rappel_valarm(titre: &str, minutes: i64) -> Composant {
    let mut a = Composant { nom: "VALARM".into(), ..Default::default() };
    a.proprietes.push(Propriete::nouvelle("ACTION", &[], "DISPLAY"));
    a.proprietes.push(Propriete::nouvelle("DESCRIPTION", &[], &echapper(if titre.is_empty() { "Rappel" } else { titre })));
    a.proprietes.push(Propriete::nouvelle("TRIGGER", &[], &if minutes == 0 { "PT0S".to_string() } else { format!("-PT{minutes}M") }));
    a
}

/// Titre, lieu, description, horaires et rappel de la saisie sur un VEVENT.
/// Le rappel n'est remplacé que s'il a changé : un rappel d'une forme que le
/// formulaire ignore reste en place.
fn poser_champs(c: &mut Composant, s: &Saisie, bornes: &Bornes, tz: chrono_tz::Tz) {
    c.poser(Propriete::nouvelle("SUMMARY", &[], &echapper(s.titre.trim())));
    for (nom, valeur) in [("LOCATION", s.lieu.trim()), ("DESCRIPTION", s.description.trim())] {
        if valeur.is_empty() {
            c.retirer(nom);
        } else {
            c.poser(Propriete::nouvelle(nom, &[], &echapper(valeur)));
        }
    }
    poser_bornes(c, bornes, tz);
    if s.rappel != rappel_de(c) {
        c.enfants.retain(|e| e.nom != "VALARM");
        if s.rappel >= 0 {
            c.enfants.push(rappel_valarm(s.titre.trim(), s.rappel));
        }
    }
}

/// Date de modification et numéro de version d'un VEVENT.
fn estampiller(c: &mut Composant, maintenant: DateTime<Utc>) {
    let sequence = c.propriete("SEQUENCE").and_then(|p| p.valeur.trim().parse::<i64>().ok()).map_or(0, |n| n + 1);
    c.poser(Propriete::nouvelle("DTSTAMP", &[], &horodatage(maintenant)));
    c.poser(Propriete::nouvelle("LAST-MODIFIED", &[], &horodatage(maintenant)));
    c.poser(Propriete::nouvelle("SEQUENCE", &[], &sequence.to_string()));
}

/// Ajoute la définition du fuseau s'il en faut une et qu'elle manque.
fn assurer_vtimezone(racine: &mut Composant, bornes: &Bornes, tz: chrono_tz::Tz) {
    let (Bornes::Heures(d, _), false) = (bornes, tz == chrono_tz::UTC) else { return };
    let presente = racine
        .enfants
        .iter()
        .any(|c| c.nom == "VTIMEZONE" && c.propriete("TZID").is_some_and(|p| p.valeur == tz.name()));
    if !presente {
        racine.enfants.insert(0, vtimezone(tz, d.year()));
    }
}

/// Nouvel objet iCalendar pour une saisie : son UID et son texte.
pub fn creer(s: &Saisie, tz: chrono_tz::Tz, maintenant: DateTime<Utc>) -> Result<(String, String), String> {
    let bornes = bornes_de(s)?;
    let uid = nouvel_uid();
    let mut racine = Composant { nom: "VCALENDAR".into(), ..Default::default() };
    racine.proprietes.push(Propriete::nouvelle("VERSION", &[], "2.0"));
    racine
        .proprietes
        .push(Propriete::nouvelle("PRODID", &[], &format!("-//M-Media//MMail {}//FR", env!("CARGO_PKG_VERSION"))));
    racine.proprietes.push(Propriete::nouvelle("CALSCALE", &[], "GREGORIAN"));
    let mut e = Composant { nom: "VEVENT".into(), ..Default::default() };
    e.proprietes.push(Propriete::nouvelle("UID", &[], &uid));
    e.proprietes.push(Propriete::nouvelle("CREATED", &[], &horodatage(maintenant)));
    e.proprietes.push(Propriete::nouvelle("TRANSP", &[], if s.journee { "TRANSPARENT" } else { "OPAQUE" }));
    // Sans rappel encore, `poser_champs` pose celui de la saisie s'il y en a un.
    poser_champs(&mut e, s, &bornes, tz);
    if let Some(regle) = regle_de(s, &bornes, tz)? {
        e.poser(Propriete::nouvelle("RRULE", &[], &regle));
    }
    estampiller(&mut e, maintenant);
    e.poser(Propriete::nouvelle("SEQUENCE", &[], "0"));
    assurer_vtimezone(&mut racine, &bornes, tz);
    racine.enfants.push(e);
    Ok((uid, racine.ecrire()))
}

/// Désignation d'une occurrence dans la forme du DTSTART de la série :
/// date seule, UTC, ou heure locale du même fuseau.
fn designation(racine: &Composant, maitre: &Composant, nom: &str, origine: DateTime<Utc>) -> Result<Propriete, String> {
    let dtstart = maitre.propriete("DTSTART").ok_or("série sans début")?;
    if dtstart.date_seule() {
        return Ok(Propriete::nouvelle(nom, &[("VALUE", "DATE")], &origine.date_naive().format("%Y%m%d").to_string()));
    }
    if dtstart.valeur.trim().ends_with(['Z', 'z']) {
        return Ok(Propriete::nouvelle(nom, &[], &horodatage(origine)));
    }
    let definitions: Vec<Composant> = racine.enfants.iter().filter(|c| c.nom == "VTIMEZONE").cloned().collect();
    match dtstart.param("TZID") {
        Some(tzid) => {
            let tz = agenda::fuseau(tzid, &definitions).ok_or_else(|| format!("fuseau inconnu : {tzid}"))?;
            let local = origine.with_timezone(&tz).naive_local();
            Ok(Propriete::nouvelle(nom, &[("TZID", tzid)], &local.format("%Y%m%dT%H%M%S").to_string()))
        }
        None => {
            let local = origine.with_timezone(&chrono::Local).naive_local();
            Ok(Propriete::nouvelle(nom, &[], &local.format("%Y%m%dT%H%M%S").to_string()))
        }
    }
}

/// Modifie un objet selon une saisie : la série, ou l'occurrence désignée.
pub fn modifier(ical: &str, s: &Saisie, tz: chrono_tz::Tz, maintenant: DateTime<Utc>) -> Result<String, String> {
    let mut racine = analyser(ical).ok_or("événement illisible")?;
    let i = maitre(&racine).ok_or("événement illisible")?;
    let nouvelles = bornes_de(s)?;
    if s.occurrence.is_empty() {
        let avant = bornes(&racine, &racine.enfants[i]).map(|(d, _)| d.utc());
        let regle_avant = racine.enfants[i].propriete("RRULE").map(|p| p.valeur.clone());
        let regle = if s.repetition == "AUTRE" { None } else { Some(regle_de(s, &nouvelles, tz)?) };
        let m = &mut racine.enfants[i];
        poser_champs(m, s, &nouvelles, tz);
        if let Some(regle) = regle {
            match regle {
                Some(regle) => m.poser(Propriete::nouvelle("RRULE", &[], &regle)),
                None => m.retirer("RRULE"),
            }
        }
        estampiller(m, maintenant);
        // Série déplacée ou règle changée : les exceptions désignaient des
        // occurrences qui n'existent plus.
        let apres = bornes(&racine, &racine.enfants[i]).map(|(d, _)| d.utc());
        let regle_apres = racine.enfants[i].propriete("RRULE").map(|p| p.valeur.clone());
        if avant != apres || regle_avant != regle_apres {
            racine.enfants[i].retirer("EXDATE");
            racine.enfants.retain(|c| !(c.nom == "VEVENT" && c.propriete("RECURRENCE-ID").is_some()));
        }
    } else {
        let origine: i64 = s.occurrence.parse().map_err(|_| "occurrence invalide")?;
        let instant = Utc.timestamp_opt(origine, 0).single().ok_or("occurrence invalide")?;
        let existante = racine.enfants.iter().position(|c| c.nom == "VEVENT" && origine_de(&racine, c) == Some(origine));
        let j = match existante {
            Some(j) => j,
            None => {
                let rid = designation(&racine, &racine.enfants[i], "RECURRENCE-ID", instant)?;
                let mut copie = racine.enfants[i].clone();
                for nom in ["RRULE", "RDATE", "EXDATE", "EXRULE"] {
                    copie.retirer(nom);
                }
                copie.poser(rid);
                racine.enfants.push(copie);
                racine.enfants.len() - 1
            }
        };
        let o = &mut racine.enfants[j];
        poser_champs(o, s, &nouvelles, tz);
        estampiller(o, maintenant);
    }
    assurer_vtimezone(&mut racine, &nouvelles, tz);
    Ok(racine.ecrire())
}

/// Retire une occurrence d'une série : une exception (EXDATE) sur la série,
/// et l'occurrence déplacée s'il y en avait une.
pub fn retirer_occurrence(ical: &str, origine: i64, maintenant: DateTime<Utc>) -> Result<String, String> {
    let mut racine = analyser(ical).ok_or("événement illisible")?;
    let i = maitre(&racine).ok_or("événement illisible")?;
    let instant = Utc.timestamp_opt(origine, 0).single().ok_or("occurrence invalide")?;
    let exdate = designation(&racine, &racine.enfants[i], "EXDATE", instant)?;
    let racine_copie = racine.clone();
    racine.enfants.retain(|c| !(c.nom == "VEVENT" && origine_de(&racine_copie, c) == Some(origine)));
    let i = maitre(&racine).ok_or("événement illisible")?;
    let m = &mut racine.enfants[i];
    m.proprietes.push(exdate);
    estampiller(m, maintenant);
    Ok(racine.ecrire())
}

#[cfg(test)]
mod tests;
