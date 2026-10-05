use super::*;
use crate::agenda::occurrences;

fn paris() -> chrono_tz::Tz {
    "Europe/Paris".parse().unwrap()
}

fn utc(texte: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(texte).unwrap().with_timezone(&Utc)
}

fn maintenant() -> DateTime<Utc> {
    utc("2026-10-05T16:00:00Z")
}

fn debuts(ical: &str, de: &str, a: &str) -> Vec<String> {
    occurrences(ical, utc(de), utc(a)).iter().map(|o| format!("{} {}", o.debut.format("%Y-%m-%dT%H:%MZ"), o.resume)).collect()
}

fn saisie(titre: &str, debut: &str, fin: &str) -> Saisie {
    Saisie { titre: titre.into(), debut: debut.into(), fin: fin.into(), rappel: -1, ..Default::default() }
}

const SERIE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Autre//FR\r\nBEGIN:VTIMEZONE\r\nTZID:Romance Standard Time\r\n\
BEGIN:STANDARD\r\nDTSTART:16011028T030000\r\nRRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=10\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\n\
END:STANDARD\r\nBEGIN:DAYLIGHT\r\nDTSTART:16010325T020000\r\nRRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=3\r\nTZOFFSETFROM:+0100\r\n\
TZOFFSETTO:+0200\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:serie\r\nDTSTAMP:20260901T000000Z\r\n\
DTSTART;TZID=Romance Standard Time:20260921T080000\r\nDTEND;TZID=Romance Standard Time:20260921T083000\r\n\
RRULE:FREQ=WEEKLY;BYDAY=MO;UNTIL=20261231T225959Z\r\nEXDATE;TZID=Romance Standard Time:20261012T080000\r\n\
SUMMARY:Point\r\nX-AUTRE-LOGICIEL:garde-moi\r\nSEQUENCE:3\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER;RELATED=END:PT0S\r\n\
END:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:serie\r\nRECURRENCE-ID;TZID=Romance Standard Time:20261005T080000\r\n\
DTSTART;TZID=Romance Standard Time:20261005T140000\r\nDTEND;TZID=Romance Standard Time:20261005T150000\r\n\
SUMMARY:Point déplacé\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

#[test]
fn definition_de_fuseau_hiver_ete_et_fixe() {
    let v = vtimezone(paris(), 2026).ecrire();
    assert!(v.contains("TZID:Europe/Paris"), "{v}");
    assert!(v.contains("BEGIN:DAYLIGHT\r\nDTSTART:19700329T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200"), "{v}");
    assert!(v.contains("BEGIN:STANDARD\r\nDTSTART:19701025T030000\r\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100"), "{v}");
    let tokyo = vtimezone("Asia/Tokyo".parse().unwrap(), 2026).ecrire();
    assert!(tokyo.contains("TZOFFSETFROM:+0900\r\nTZOFFSETTO:+0900"), "{tokyo}");
    // Hémisphère sud : l'heure d'été commence le premier dimanche d'octobre.
    let sydney = vtimezone("Australia/Sydney".parse().unwrap(), 2026).ecrire();
    assert!(sydney.contains("RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=1SU\r\nTZOFFSETFROM:+1000\r\nTZOFFSETTO:+1100"), "{sydney}");
    // Une définition sous un nom inconnu est reconnue par ses décalages.
    let mut renommee = vtimezone(paris(), 2026);
    renommee.poser(Propriete::nouvelle("TZID", &[], "Fuseau maison"));
    assert_eq!(agenda::fuseau("Fuseau maison", &[renommee]), Some(paris()));
}

#[test]
fn ecriture_pliee_relue_a_l_identique() {
    let mut e = Composant { nom: "VEVENT".into(), ..Default::default() };
    let long = "Ordre du jour, très long : revue des comptes; budget 2027, recrutements, déménagement du dépôt et questions diverses";
    e.proprietes.push(Propriete::nouvelle("DESCRIPTION", &[], &echapper(long)));
    e.proprietes.push(Propriete::nouvelle("DTSTART", &[("TZID", "(UTC+01:00) Bruxelles, Paris")], "20261007T093000"));
    let mut racine = Composant { nom: "VCALENDAR".into(), ..Default::default() };
    racine.enfants.push(e);
    let texte = racine.ecrire();
    assert!(texte.lines().all(|l| l.trim_end_matches('\r').len() <= 75), "{texte}");
    let relu = analyser(&texte).unwrap();
    assert_eq!(relu, racine);
    assert_eq!(relu.enfants[0].texte("DESCRIPTION"), long);
    assert!(texte.contains("TZID=\"(UTC+01:00) Bruxelles, Paris\""));
}

#[test]
fn creation_puis_formulaire_prerempli() {
    let s = Saisie { lieu: "Bureau, 1er étage".into(), description: "Ligne 1\nLigne 2".into(), rappel: 15, ..saisie("Réunion", "2026-10-07T09:30", "2026-10-07T11:00") };
    let (uid, ical) = creer(&s, paris(), maintenant()).unwrap();
    assert!(ical.contains(&format!("UID:{uid}")));
    assert!(ical.contains("DTSTART;TZID=Europe/Paris:20261007T093000"), "{ical}");
    assert!(ical.contains("BEGIN:VTIMEZONE"));
    assert!(ical.contains("TRIGGER:-PT15M"));
    assert_eq!(debuts(&ical, "2026-10-01T00:00:00Z", "2026-10-31T00:00:00Z"), ["2026-10-07T07:30Z Réunion"]);
    let o = &occurrences(&ical, utc("2026-10-01T00:00:00Z"), utc("2026-10-31T00:00:00Z"))[0];
    assert_eq!(o.rappels, [utc("2026-10-07T07:15:00Z")]);
    assert!(agenda::etendue(&ical).is_some());
    let relue = saisie_de(&ical, None, paris()).unwrap();
    assert_eq!(relue, Saisie { agenda: 0, objet: 0, ..s });
}

#[test]
fn creation_journee_entiere_et_repetition() {
    let s = Saisie { journee: true, ..saisie("Salon", "2026-10-09", "2026-10-10") };
    let (_, ical) = creer(&s, paris(), maintenant()).unwrap();
    assert!(ical.contains("DTSTART;VALUE=DATE:20261009") && ical.contains("DTEND;VALUE=DATE:20261011"), "{ical}");
    assert!(!ical.contains("VTIMEZONE"));
    assert_eq!(saisie_de(&ical, None, paris()).unwrap().fin, "2026-10-10");

    let s = Saisie { repetition: "WEEKLY".into(), jusqu_au: "2026-10-26".into(), ..saisie("Hebdo", "2026-10-05T08:00", "2026-10-05T08:30") };
    let (_, ical) = creer(&s, paris(), maintenant()).unwrap();
    assert!(ical.contains("RRULE:FREQ=WEEKLY;BYDAY=MO;UNTIL=20261026T225959Z"), "{ical}");
    // Le changement d'heure du 25 octobre est suivi : 8 h reste 8 h.
    assert_eq!(
        debuts(&ical, "2026-10-01T00:00:00Z", "2026-11-30T00:00:00Z"),
        ["2026-10-05T06:00Z Hebdo", "2026-10-12T06:00Z Hebdo", "2026-10-19T06:00Z Hebdo", "2026-10-26T07:00Z Hebdo"]
    );
    let relue = saisie_de(&ical, None, paris()).unwrap();
    assert_eq!((relue.repetition.as_str(), relue.jusqu_au.as_str()), ("WEEKLY", "2026-10-26"));

    let erreur = creer(&saisie("X", "2026-10-07T11:00", "2026-10-07T09:00"), paris(), maintenant()).unwrap_err();
    assert!(erreur.contains("précède"), "{erreur}");
    assert!(creer(&saisie("X", "07/10/2026", "2026-10-07T09:00"), paris(), maintenant()).is_err());
}

#[test]
fn modification_de_la_serie() {
    // Titre seul : exceptions, occurrence déplacée, propriété inconnue et
    // rappel d'une autre forme restent ; la version augmente.
    let mut s = saisie_de(SERIE, None, paris()).unwrap();
    assert_eq!((s.repetition.as_str(), s.rappel, s.debut.as_str()), ("WEEKLY", -1, "2026-09-21T08:00"));
    s.titre = "Point d'équipe".into();
    let ical = modifier(SERIE, &s, paris(), maintenant()).unwrap();
    assert!(ical.contains("X-AUTRE-LOGICIEL:garde-moi") && ical.contains("SEQUENCE:4") && ical.contains("RELATED=END"), "{ical}");
    assert_eq!(
        debuts(&ical, "2026-10-01T00:00:00Z", "2026-10-20T00:00:00Z"),
        ["2026-10-05T12:00Z Point déplacé", "2026-10-19T06:00Z Point d'équipe"]
    );
    // Heure changée : les exceptions, devenues sans objet, disparaissent.
    s.debut = "2026-09-21T09:00".into();
    s.fin = "2026-09-21T09:30".into();
    let ical = modifier(SERIE, &s, paris(), maintenant()).unwrap();
    assert!(ical.contains("DTSTART;TZID=Europe/Paris:20260921T090000"), "{ical}");
    assert_eq!(
        debuts(&ical, "2026-10-01T00:00:00Z", "2026-10-20T00:00:00Z"),
        ["2026-10-05T07:00Z Point d'équipe", "2026-10-12T07:00Z Point d'équipe", "2026-10-19T07:00Z Point d'équipe"]
    );
    // Plus de répétition.
    s.repetition = String::new();
    let ical = modifier(SERIE, &s, paris(), maintenant()).unwrap();
    assert!(!ical.contains("RRULE:FREQ=WEEKLY"));
    assert_eq!(debuts(&ical, "2026-09-01T00:00:00Z", "2026-12-31T00:00:00Z").len(), 1);
}

#[test]
fn modification_d_une_occurrence() {
    // Le lundi 19 octobre, à 10 h et avec un rappel : une occurrence déplacée
    // se crée, désignée dans le fuseau de la série.
    let origine = utc("2026-10-19T06:00:00Z").timestamp();
    let mut s = saisie_de(SERIE, Some(origine), paris()).unwrap();
    assert_eq!((s.debut.as_str(), s.fin.as_str(), s.repetition.as_str()), ("2026-10-19T08:00", "2026-10-19T08:30", ""));
    s.debut = "2026-10-19T10:00".into();
    s.fin = "2026-10-19T11:00".into();
    s.rappel = 10;
    let ical = modifier(SERIE, &s, paris(), maintenant()).unwrap();
    assert!(ical.contains("RECURRENCE-ID;TZID=Romance Standard Time:20261019T080000"), "{ical}");
    let occ = occurrences(&ical, utc("2026-10-19T00:00:00Z"), utc("2026-10-20T00:00:00Z"));
    assert_eq!(occ.len(), 1);
    assert_eq!((occ[0].debut, occ[0].origine), (utc("2026-10-19T08:00:00Z"), utc("2026-10-19T06:00:00Z")));
    assert_eq!(occ[0].rappels, [utc("2026-10-19T07:50:00Z")]);
    // L'occurrence déjà déplacée du 5 octobre se modifie sur place.
    let origine = utc("2026-10-05T06:00:00Z").timestamp();
    let mut s = saisie_de(SERIE, Some(origine), paris()).unwrap();
    assert_eq!((s.titre.as_str(), s.debut.as_str()), ("Point déplacé", "2026-10-05T14:00"));
    s.titre = "Point redéplacé".into();
    let ical = modifier(SERIE, &s, paris(), maintenant()).unwrap();
    assert_eq!(ical.matches("RECURRENCE-ID").count(), 1);
    assert_eq!(debuts(&ical, "2026-10-05T00:00:00Z", "2026-10-06T00:00:00Z"), ["2026-10-05T12:00Z Point redéplacé"]);
}

#[test]
fn retrait_d_une_occurrence() {
    let ical = retirer_occurrence(SERIE, utc("2026-10-19T06:00:00Z").timestamp(), maintenant()).unwrap();
    assert!(ical.contains("EXDATE;TZID=Romance Standard Time:20261019T080000"), "{ical}");
    assert_eq!(debuts(&ical, "2026-10-13T00:00:00Z", "2026-10-27T00:00:00Z"), ["2026-10-26T07:00Z Point"]);
    // Une occurrence déplacée retirée disparaît, avec son remplaçant.
    let ical = retirer_occurrence(SERIE, utc("2026-10-05T06:00:00Z").timestamp(), maintenant()).unwrap();
    assert!(!ical.contains("RECURRENCE-ID"));
    assert!(debuts(&ical, "2026-10-05T00:00:00Z", "2026-10-06T00:00:00Z").is_empty());
}

#[test]
fn regle_que_le_formulaire_ne_decrit_pas_conservee() {
    let ical = SERIE.replace("RRULE:FREQ=WEEKLY;BYDAY=MO;UNTIL=20261231T225959Z", "RRULE:FREQ=MONTHLY;BYDAY=MO;BYSETPOS=1");
    let mut s = saisie_de(&ical, None, paris()).unwrap();
    assert_eq!(s.repetition, "AUTRE");
    s.titre = "Premier lundi".into();
    let modifie = modifier(&ical, &s, paris(), maintenant()).unwrap();
    assert!(modifie.contains("RRULE:FREQ=MONTHLY;BYDAY=MO;BYSETPOS=1"), "{modifie}");
}

#[test]
fn identifiants_au_format_uuid_et_distincts() {
    let (a, b) = (nouvel_uid(), nouvel_uid());
    assert_ne!(a, b);
    assert_eq!(a.len(), 36);
    assert_eq!(a.matches('-').count(), 4);
    assert_eq!(a.as_bytes()[14], b'4');
}
