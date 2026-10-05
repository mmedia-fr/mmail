use super::*;

const PARIS: &str = "BEGIN:VTIMEZONE\r\nTZID:Europe/Paris\r\nBEGIN:DAYLIGHT\r\nTZOFFSETFROM:+0100\r\n\
TZOFFSETTO:+0200\r\nTZNAME:CEST\r\nDTSTART:19700329T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\n\
END:DAYLIGHT\r\nBEGIN:STANDARD\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nTZNAME:CET\r\n\
DTSTART:19701025T030000\r\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n";

fn objet(corps: &str) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Essai//FR\r\n{PARIS}{corps}END:VCALENDAR\r\n")
}

fn utc(texte: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(texte).unwrap().with_timezone(&Utc)
}

fn debuts(occ: &[Occurrence]) -> Vec<String> {
    occ.iter().map(|o| o.debut.format("%Y-%m-%dT%H:%MZ").to_string()).collect()
}

const SIMPLE: &str = "BEGIN:VEVENT\r\nUID:simple\r\nDTSTAMP:20261005T140000Z\r\n\
DTSTART;TZID=Europe/Paris:20261007T093000\r\nDTEND;TZID=Europe/Paris:20261007T110000\r\n\
SUMMARY:Réunion d'équipe\r\nLOCATION:Bureau\\, 1er étage\r\nDESCRIPTION:Ordre du jour :\\n- revue\r\n\
END:VEVENT\r\n";

const HEBDO: &str = "BEGIN:VEVENT\r\nUID:hebdo\r\nDTSTAMP:20261005T140000Z\r\n\
DTSTART;TZID=Europe/Paris:20260921T080000\r\nDTEND;TZID=Europe/Paris:20260921T083000\r\n\
RRULE:FREQ=WEEKLY;UNTIL=20261231T225959Z;BYDAY=MO,TH\r\nEXDATE;TZID=Europe/Paris:20261012T080000\r\n\
SUMMARY:Point hebdomadaire\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:hebdo\r\nDTSTAMP:20261005T140000Z\r\nRECURRENCE-ID;TZID=Europe/Paris:20261008T080000\r\n\
DTSTART;TZID=Europe/Paris:20261008T140000\r\nDTEND;TZID=Europe/Paris:20261008T150000\r\n\
SUMMARY:Point hebdomadaire (déplacé)\r\nEND:VEVENT\r\n";

#[test]
fn lignes_pliees_parametres_entre_guillemets_et_echappements() {
    let texte = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:Une très lon\r\n gue ligne\r\n\
DTSTART;TZID=\"(UTC+01:00) Bruxelles; Paris\":20261007T093000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let racine = analyser(texte).unwrap();
    let e = &racine.enfants[0];
    assert_eq!(e.texte("SUMMARY"), "Une très longue ligne");
    let d = e.propriete("DTSTART").unwrap();
    assert_eq!(d.param("TZID"), Some("(UTC+01:00) Bruxelles; Paris"));
    assert_eq!(d.valeur, "20261007T093000");
    assert_eq!(desechapper("a\\, b\\; c\\nd\\\\e"), "a, b; c\nd\\e");
}

#[test]
fn objet_tronque_ou_trop_profond_refuse() {
    assert!(analyser("BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\n").is_none());
    assert!(analyser("BEGIN:VCARD\r\nEND:VCARD\r\n").is_none());
    let profond = "BEGIN:X\r\n".repeat(40) + &"END:X\r\n".repeat(40);
    assert!(analyser(&format!("BEGIN:VCALENDAR\r\n{profond}END:VCALENDAR\r\n")).is_none());
}

#[test]
fn fuseaux_iana_windows_prefixes_libelles_et_definitions() {
    let paris: chrono_tz::Tz = "Europe/Paris".parse().unwrap();
    assert_eq!(fuseau("Europe/Paris", &[]), Some(paris));
    assert_eq!(fuseau("Romance Standard Time", &[]), Some(paris));
    assert_eq!(fuseau("/mozilla.org/20050126_1/Europe/Paris", &[]), Some(paris));
    assert_eq!(fuseau("(UTC+01:00) Bruxelles, Copenhague, Madrid, Paris", &[]), Some(paris));
    assert_eq!(fuseau("W. Europe Standard Time", &[]).map(|t| t.name()), Some("Europe/Berlin"));
    assert_eq!(fuseau("Inconnu", &[]), None);
    let perso = analyser(
        "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:Customized Time Zone\r\nBEGIN:STANDARD\r\n\
TZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nEND:STANDARD\r\nBEGIN:DAYLIGHT\r\nTZOFFSETFROM:+0100\r\n\
TZOFFSETTO:+0200\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\nBEGIN:VTIMEZONE\r\nTZID:Fixe\r\nBEGIN:STANDARD\r\n\
TZOFFSETTO:+0400\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\nEND:VCALENDAR\r\n",
    )
    .unwrap();
    assert_eq!(fuseau("Customized Time Zone", &perso.enfants), Some(paris));
    assert_eq!(fuseau("Fixe", &perso.enfants).map(|t| t.name()), Some("Etc/GMT-4"));
}

#[test]
fn durees() {
    assert_eq!(lire_duree("PT1H30M"), Some(Duration::minutes(90)));
    assert_eq!(lire_duree("P1D"), Some(Duration::days(1)));
    assert_eq!(lire_duree("P1W"), Some(Duration::weeks(1)));
    assert_eq!(lire_duree("-PT15M"), Some(Duration::minutes(-15)));
    assert_eq!(lire_duree("P1DT2H"), Some(Duration::hours(26)));
    assert_eq!(lire_duree("PT1D"), None);
    assert_eq!(lire_duree("1H"), None);
}

#[test]
fn evenement_simple_en_heure_de_paris() {
    let occ = occurrences(&objet(SIMPLE), utc("2026-10-05T00:00:00Z"), utc("2026-10-12T00:00:00Z"));
    assert_eq!(occ.len(), 1);
    let o = &occ[0];
    assert_eq!(o.debut, utc("2026-10-07T07:30:00Z"));
    assert_eq!(o.fin, utc("2026-10-07T09:00:00Z"));
    assert_eq!(o.resume, "Réunion d'équipe");
    assert_eq!(o.lieu, "Bureau, 1er étage");
    assert_eq!(o.description, "Ordre du jour :\n- revue");
    assert!(o.journee.is_none());
    // Hors de la période : rien.
    assert!(occurrences(&objet(SIMPLE), utc("2026-10-08T00:00:00Z"), utc("2026-10-09T00:00:00Z")).is_empty());
}

#[test]
fn journees_entieres() {
    let corps = "BEGIN:VEVENT\r\nUID:salon\r\nDTSTART;VALUE=DATE:20261009\r\nDTEND;VALUE=DATE:20261011\r\n\
SUMMARY:Salon\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:seul\r\nDTSTART;VALUE=DATE:20261014\r\nSUMMARY:Seul\r\n\
END:VEVENT\r\n";
    let occ = occurrences(&objet(corps), utc("2026-10-05T00:00:00Z"), utc("2026-10-19T00:00:00Z"));
    let journees: Vec<_> = occ.iter().map(|o| o.journee.unwrap()).collect();
    let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    assert_eq!(journees, vec![(d("2026-10-09"), d("2026-10-11")), (d("2026-10-14"), d("2026-10-15"))]);
}

#[test]
fn repetition_hebdomadaire_exception_et_occurrence_deplacee() {
    let occ = occurrences(&objet(HEBDO), utc("2026-10-05T00:00:00Z"), utc("2026-10-19T00:00:00Z"));
    assert_eq!(
        debuts(&occ),
        // Lun. 5 à 8 h, jeu. 8 déplacé à 14 h, lun. 12 exclu, jeu. 15 à 8 h.
        vec!["2026-10-05T06:00Z", "2026-10-08T12:00Z", "2026-10-15T06:00Z"]
    );
    assert_eq!(occ[1].resume, "Point hebdomadaire (déplacé)");
    assert_eq!(occ[1].fin - occ[1].debut, Duration::hours(1));
}

#[test]
fn repetition_a_travers_le_changement_d_heure() {
    // Retour à l'heure d'hiver le dimanche 25 octobre 2026 : 8 h à Paris vaut
    // 6 h UTC avant, 7 h après.
    let occ = occurrences(&objet(HEBDO), utc("2026-10-22T00:00:00Z"), utc("2026-10-27T00:00:00Z"));
    assert_eq!(debuts(&occ), vec!["2026-10-22T06:00Z", "2026-10-26T07:00Z"]);
}

#[test]
fn utc_et_fuseau_windows() {
    let corps = "BEGIN:VEVENT\r\nUID:utc\r\nDTSTART:20261007T150000Z\r\nDTEND:20261007T160000Z\r\n\
SUMMARY:Visio\r\nEND:VEVENT\r\n";
    let occ = occurrences(&objet(corps), utc("2026-10-05T00:00:00Z"), utc("2026-10-12T00:00:00Z"));
    assert_eq!(debuts(&occ), vec!["2026-10-07T15:00Z"]);
    let outlook = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:Romance Standard Time\r\nEND:VTIMEZONE\r\n\
BEGIN:VEVENT\r\nUID:o\r\nDTSTART;TZID=Romance Standard Time:20261008T100000\r\n\
DTEND;TZID=Romance Standard Time:20261008T113000\r\nSUMMARY:Outlook\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let occ = occurrences(outlook, utc("2026-10-05T00:00:00Z"), utc("2026-10-12T00:00:00Z"));
    assert_eq!(debuts(&occ), vec!["2026-10-08T08:00Z"]);
}

#[test]
fn duree_until_en_date_seule_et_anniversaire() {
    // UNTIL en date seule avec un départ dans un fuseau : refusé par la RFC,
    // fréquent pourtant ; la dernière occurrence est celle du 8 octobre.
    let corps = "BEGIN:VEVENT\r\nUID:quotidien\r\nDTSTART;TZID=Europe/Paris:20261005T090000\r\n\
DURATION:PT45M\r\nRRULE:FREQ=DAILY;UNTIL=20261008\r\nSUMMARY:Café\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:anniv\r\nDTSTART;VALUE=DATE:20200310\r\nRRULE:FREQ=YEARLY\r\nSUMMARY:Anniversaire\r\n\
END:VEVENT\r\n";
    let occ = occurrences(&objet(corps), utc("2026-10-01T00:00:00Z"), utc("2026-10-31T00:00:00Z"));
    assert_eq!(debuts(&occ), vec!["2026-10-05T07:00Z", "2026-10-06T07:00Z", "2026-10-07T07:00Z", "2026-10-08T07:00Z"]);
    assert_eq!(occ[0].fin - occ[0].debut, Duration::minutes(45));
    let mars = occurrences(&objet(corps), utc("2027-03-01T00:00:00Z"), utc("2027-04-01T00:00:00Z"));
    assert_eq!(mars.len(), 1);
    assert_eq!(mars[0].journee.map(|(d, _)| d.to_string()), Some("2027-03-10".into()));
}

#[test]
fn regle_illisible_rend_l_occurrence_d_origine() {
    let corps = "BEGIN:VEVENT\r\nUID:x\r\nDTSTART:20261007T150000Z\r\nDTEND:20261007T160000Z\r\n\
RRULE:FREQ=PARFOIS\r\nSUMMARY:X\r\nEND:VEVENT\r\n";
    let occ = occurrences(&objet(corps), utc("2026-10-01T00:00:00Z"), utc("2026-10-31T00:00:00Z"));
    assert_eq!(debuts(&occ), vec!["2026-10-07T15:00Z"]);
}

#[test]
fn annulation() {
    let corps = "BEGIN:VEVENT\r\nUID:x\r\nDTSTART:20261007T150000Z\r\nSTATUS:CANCELLED\r\nSUMMARY:X\r\nEND:VEVENT\r\n";
    let occ = occurrences(&objet(corps), utc("2026-10-01T00:00:00Z"), utc("2026-10-31T00:00:00Z"));
    assert!(occ[0].annule);
}

#[test]
fn etendues() {
    let e = etendue(&objet(SIMPLE)).unwrap();
    assert_eq!(e, (utc("2026-10-07T07:30:00Z").timestamp(), utc("2026-10-07T09:00:00Z").timestamp()));
    // UNTIL : la dernière occurrence est le jeudi 31 décembre, 8 h 30 à Paris.
    let e = etendue(&objet(HEBDO)).unwrap();
    assert_eq!(e, (utc("2026-09-21T06:00:00Z").timestamp(), utc("2026-12-31T07:30:00Z").timestamp()));
    let sans_fin = "BEGIN:VEVENT\r\nUID:x\r\nDTSTART:20261007T150000Z\r\nRRULE:FREQ=MONTHLY\r\nEND:VEVENT\r\n";
    assert_eq!(etendue(&objet(sans_fin)).unwrap().1, i64::MAX);
    let compte = "BEGIN:VEVENT\r\nUID:x\r\nDTSTART:20261007T150000Z\r\nDURATION:PT1H\r\nRRULE:FREQ=DAILY;COUNT=3\r\n\
END:VEVENT\r\n";
    assert_eq!(etendue(&objet(compte)).unwrap().1, utc("2026-10-09T16:00:00Z").timestamp());
    assert!(etendue("pas un agenda").is_none());
}

fn occ(debut: &str, fin: &str, titre: &str) -> Occurrence {
    Occurrence {
        uid: titre.into(),
        resume: titre.into(),
        lieu: String::new(),
        description: String::new(),
        debut: utc(debut),
        fin: utc(fin),
        journee: None,
        annule: false,
        agenda: 1,
        objet: 0,
        origine: utc(debut),
        repete: false,
        participants: false,
        rappels: Vec::new(),
    }
}

#[test]
fn grille_colonnes_des_chevauchements_et_bandeau() {
    let paris: chrono_tz::Tz = "Europe/Paris".parse().unwrap();
    let lundi = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
    let mut salon = occ("2026-10-09T00:00:00Z", "2026-10-11T00:00:00Z", "Salon");
    salon.journee = Some((NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(), NaiveDate::from_ymd_opt(2026, 10, 11).unwrap()));
    let mut ferie = occ("2026-10-10T00:00:00Z", "2026-10-11T00:00:00Z", "Férié");
    ferie.journee = Some((NaiveDate::from_ymd_opt(2026, 10, 10).unwrap(), NaiveDate::from_ymd_opt(2026, 10, 11).unwrap()));
    let liste = vec![
        // Mardi : A 9 h–11 h, B 10 h–12 h, C 11 h–12 h (A libéré : colonne 0), D 14 h seul.
        occ("2026-10-06T07:00:00Z", "2026-10-06T09:00:00Z", "A"),
        occ("2026-10-06T08:00:00Z", "2026-10-06T10:00:00Z", "B"),
        occ("2026-10-06T09:00:00Z", "2026-10-06T10:00:00Z", "C"),
        occ("2026-10-06T12:00:00Z", "2026-10-06T12:00:00Z", "D"),
        // Mercredi 23 h → jeudi 1 h : deux morceaux.
        occ("2026-10-07T21:00:00Z", "2026-10-07T23:00:00Z", "Nuit"),
        salon,
        ferie,
        // Trois jours d'affilée avec heures : au bandeau.
        occ("2026-10-05T07:00:00Z", "2026-10-08T15:00:00Z", "Congrès"),
    ];
    let g = grille(&liste, lundi, 7, &paris);
    let case = |titre: &str| -> Vec<&Case> { g.cases.iter().filter(|c| g.fiches[c.fiche].titre == titre).collect() };
    let a = case("A")[0];
    assert_eq!((a.jour, a.debut, a.fin, a.colonne, a.colonnes), (1, 540, 660, 0, 2));
    let b = case("B")[0];
    assert_eq!((b.colonne, b.colonnes), (1, 2));
    let c = case("C")[0];
    assert_eq!((c.colonne, c.colonnes), (0, 2));
    let d = case("D")[0];
    // Instantané : une demi-heure à l'affichage, une colonne pour lui seul.
    assert_eq!((d.debut, d.fin, d.colonnes), (840, 870, 1));
    let nuit = case("Nuit");
    assert_eq!(nuit.len(), 2);
    assert_eq!((nuit[0].jour, nuit[0].debut, nuit[0].fin, nuit[0].heures.as_str()), (2, 1380, 1440, "23:00 – 01:00"));
    assert_eq!((nuit[1].jour, nuit[1].debut, nuit[1].fin, nuit[1].heures.as_str()), (3, 0, 60, ""));
    // Bandeau : le congrès (lundi–jeudi) ligne 0, le salon (vendredi–samedi)
    // ligne 0 aussi, le férié (samedi) sous le salon.
    let bande = |titre: &str| g.bandes.iter().find(|b| g.fiches[b.fiche].titre == titre).unwrap();
    assert_eq!((bande("Congrès").de, bande("Congrès").a, bande("Congrès").ligne), (0, 4, 0));
    assert_eq!((bande("Salon").de, bande("Salon").a, bande("Salon").ligne), (4, 6, 0));
    assert_eq!((bande("Férié").de, bande("Férié").a, bande("Férié").ligne), (5, 6, 1));
    assert_eq!(g.lignes, 2);
    assert_eq!(g.fiches[g.cases.iter().find(|c| g.fiches[c.fiche].titre == "A").unwrap().fiche].quand, "mardi 6 octobre 2026, 09:00 – 11:00");
    assert_eq!(g.fiches[bande("Salon").fiche].quand, "du vendredi 9 au samedi 10 octobre 2026, toute la journée");
}

#[test]
fn vue_mois_journees_entieres_d_abord() {
    let paris: chrono_tz::Tz = "Europe/Paris".parse().unwrap();
    let premier = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
    let mut ferie = occ("2026-10-06T00:00:00Z", "2026-10-07T00:00:00Z", "Férié");
    ferie.journee = Some((NaiveDate::from_ymd_opt(2026, 10, 6).unwrap(), NaiveDate::from_ymd_opt(2026, 10, 7).unwrap()));
    let liste = vec![
        occ("2026-10-06T07:00:00Z", "2026-10-06T09:00:00Z", "Matin"),
        ferie,
        occ("2026-10-06T21:00:00Z", "2026-10-07T07:00:00Z", "Nuit"),
    ];
    let m = mois(&liste, premier, 42, &paris);
    let titres = |i: usize| -> Vec<(String, String)> {
        m.jours[i].iter().map(|e| (e.heure.clone(), m.fiches[e.fiche].titre.clone())).collect()
    };
    // Mardi 6 octobre : 8e case.
    assert_eq!(titres(8), vec![("".into(), "Férié".into()), ("09:00".into(), "Matin".into()), ("23:00".into(), "Nuit".into())]);
    // Mercredi 7 : la suite de la nuit, sans heure.
    assert_eq!(titres(9), vec![("".into(), "Nuit".into())]);
    assert_eq!(date_longue(NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()), "jeudi 1er octobre 2026");
    let j = |a, m, d| NaiveDate::from_ymd_opt(a, m, d).unwrap();
    assert_eq!(plage_de_jours(j(2026, 9, 30), j(2026, 10, 2)), "du mercredi 30 septembre au vendredi 2 octobre 2026");
    assert_eq!(plage_de_jours(j(2026, 12, 31), j(2027, 1, 1)), "du jeudi 31 décembre 2026 au vendredi 1er janvier 2027");
}

#[test]
fn vue_semaine_et_mois_depuis_l_index() {
    use crate::magasin::ObjetAgenda;
    let paris: chrono_tz::Tz = "Europe/Paris".parse().unwrap();
    let magasin = Magasin::en_memoire().unwrap();
    let compte = magasin.compte("a@exemple.fr", "h", 993, "a@exemple.fr").unwrap();
    magasin
        .poser_agendas(
            compte,
            &[("https://h/p/".into(), "Personnel".into(), "".into(), true), ("https://h/t/".into(), "Travail".into(), "#112233".into(), false)],
        )
        .unwrap();
    let agendas = magasin.agendas().unwrap();
    let (perso, travail) = (agendas[0].id, agendas[1].id);
    let objet = |href: &str, ical: String| {
        let (debut, fin) = etendue(&ical).unwrap();
        ObjetAgenda { href: href.into(), etag: "1".into(), ical, debut, fin }
    };
    magasin.poser_evenements(perso, &[objet("s", objet_texte(SIMPLE)), objet("h", objet_texte(HEBDO))]).unwrap();
    magasin
        .poser_evenements(travail, &[objet("j", objet_texte("BEGIN:VEVENT\r\nUID:j\r\nDTSTART;VALUE=DATE:20261009\r\nDTEND;VALUE=DATE:20261011\r\nSUMMARY:Salon\r\nEND:VEVENT\r\n"))])
        .unwrap();
    let mercredi = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
    let mardi = NaiveDate::from_ymd_opt(2026, 10, 6).unwrap();

    let v: serde_json::Value = serde_json::from_str(&vue_dans(&magasin, "semaine", mercredi, mardi, &paris)).unwrap();
    assert_eq!(v["titre"], "5 – 11 octobre 2026");
    assert_eq!(v["jours"][0]["date"], "2026-10-05");
    assert_eq!(v["jours"][1]["aujourdhui"], true);
    // Réunion, point du lundi, point déplacé du jeudi ; le salon au bandeau.
    assert_eq!(v["cases"].as_array().unwrap().len(), 3);
    assert_eq!(v["bandes"].as_array().unwrap().len(), 1);
    let salon = &v["fiches"][v["bandes"][0]["fiche"].as_u64().unwrap() as usize];
    assert_eq!((salon["titre"].as_str(), salon["couleur"].as_str(), salon["nomAgenda"].as_str()), (Some("Salon"), Some("#112233"), Some("Travail")));
    // L'agenda sans couleur en reçoit une de la palette.
    assert!(v["fiches"].as_array().unwrap().iter().all(|f| f["couleur"].as_str().is_some_and(|c| c.starts_with('#'))));

    // Agenda masqué : ses événements disparaissent.
    magasin.afficher_agenda(travail, false).unwrap();
    let v: serde_json::Value = serde_json::from_str(&vue_dans(&magasin, "semaine", mercredi, mardi, &paris)).unwrap();
    assert_eq!(v["bandes"].as_array().unwrap().len(), 0);

    let m: serde_json::Value = serde_json::from_str(&vue_dans(&magasin, "mois", mercredi, mardi, &paris)).unwrap();
    assert_eq!(m["titre"], "Octobre 2026");
    assert_eq!(m["jours"].as_array().unwrap().len(), 42);
    assert_eq!(m["jours"][0]["date"], "2026-09-28");
    assert_eq!(m["jours"][0]["horsMois"], true);
    // Les lundis et jeudis d'octobre jusqu'au 31 décembre : le point
    // hebdomadaire paraît partout, sauf le 12 (exclu).
    let lundi_12 = &m["entrees"][14];
    assert_eq!(lundi_12.as_array().unwrap().len(), 0);
    let j: serde_json::Value = serde_json::from_str(&vue_dans(&magasin, "jour", mercredi, mardi, &paris)).unwrap();
    assert_eq!(j["titre"], "Mercredi 7 octobre 2026");
    assert_eq!(j["cases"][0]["heures"], "09:30 – 11:00");
}

fn objet_texte(corps: &str) -> String {
    objet(corps)
}

#[test]
fn rappels_echus_vus_et_repousses() {
    use crate::magasin::ObjetAgenda;
    let paris: chrono_tz::Tz = "Europe/Paris".parse().unwrap();
    let magasin = Magasin::en_memoire().unwrap();
    let compte = magasin.compte("a@exemple.fr", "h", 993, "a@exemple.fr").unwrap();
    magasin.poser_agendas(compte, &[("https://h/p/".into(), "Personnel".into(), "".into(), true)]).unwrap();
    let agenda = magasin.agendas().unwrap()[0].id;
    // Réunion à 9 h 30 (7 h 30 UTC), rappel un quart d'heure avant ; un
    // rappel par courriel, que MMail n'a pas à montrer.
    let ical = objet(
        "BEGIN:VEVENT\r\nUID:r\r\nDTSTART;TZID=Europe/Paris:20261007T093000\r\nDTEND;TZID=Europe/Paris:20261007T110000\r\n\
SUMMARY:Réunion\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT15M\r\nEND:VALARM\r\nBEGIN:VALARM\r\nACTION:EMAIL\r\n\
TRIGGER:-PT1H\r\nEND:VALARM\r\nEND:VEVENT\r\n",
    );
    let (debut, fin) = etendue(&ical).unwrap();
    magasin.poser_evenements(agenda, &[ObjetAgenda { href: "r".into(), etag: "1".into(), ical, debut, fin }]).unwrap();
    let echus = |t: &str| rappels_dans(&magasin, utc(t), &paris);

    assert!(echus("2026-10-07T07:14:00Z").is_empty());
    let r = echus("2026-10-07T07:16:00Z");
    assert_eq!(r.len(), 1);
    assert_eq!((r[0].titre.as_str(), r[0].quand.as_str(), r[0].date.as_str()), ("Réunion", "mercredi 7 octobre 2026, 09:30 – 11:00", "2026-10-07"));
    // Repoussé de cinq minutes : absent jusque-là, de retour ensuite.
    magasin.poser_rappel(&r[0].cle, false, utc("2026-10-07T07:21:00Z").timestamp(), utc("2026-10-07T07:16:00Z").timestamp()).unwrap();
    assert!(echus("2026-10-07T07:20:00Z").is_empty());
    assert_eq!(echus("2026-10-07T07:22:00Z").len(), 1);
    // Vu : ne revient plus.
    magasin.poser_rappel(&r[0].cle, true, 0, utc("2026-10-07T07:22:00Z").timestamp()).unwrap();
    assert!(echus("2026-10-07T07:30:00Z").is_empty());
    // Jamais vu, mais la réunion est finie depuis longtemps : plus montré.
    magasin.poser_rappel(&r[0].cle, false, 0, utc("2026-10-07T07:22:00Z").timestamp()).unwrap();
    assert_eq!(echus("2026-10-07T08:50:00Z").len(), 1);
    assert!(echus("2026-10-07T12:00:00Z").is_empty());
}
