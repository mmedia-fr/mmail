use super::*;

const PARIS: &str = "BEGIN:VTIMEZONE\r\nTZID:Europe/Paris\r\nBEGIN:DAYLIGHT\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\n\
DTSTART:19700329T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nEND:DAYLIGHT\r\nBEGIN:STANDARD\r\nTZOFFSETFROM:+0200\r\n\
TZOFFSETTO:+0100\r\nDTSTART:19701025T030000\r\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n";

fn calendrier(methode: &str, sequence: u32, partstat: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nPRODID:-//Essai//FR\r\nVERSION:2.0\r\nMETHOD:{methode}\r\n{PARIS}BEGIN:VEVENT\r\nUID:reunion-1\r\n\
DTSTAMP:20261005T220000Z\r\nDTSTART;TZID=Europe/Paris:20261020T100000\r\nDTEND;TZID=Europe/Paris:20261020T110000\r\n\
SUMMARY:Point d'avancement\r\nLOCATION:Salle 2\r\nATTENDEE;ROLE=REQ-PARTICIPANT;RSVP=TRUE;PARTSTAT={partstat};CN=Bob:mailt\r\n\x20o:\
bob@exemple.fr\r\nSEQUENCE:{sequence}\r\nORGANIZER;CN=Alice Martin:mailto:alice@exemple.fr\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

/// Courriel d'invitation à la manière de SOGo : HTML, puis la partie
/// calendrier.
fn courriel_sogo(methode: &str, partstat: &str) -> Vec<u8> {
    format!(
        "From: \"Alice Martin\" <alice@exemple.fr>\r\nTo: bob@exemple.fr\r\nSubject: Event Invitation\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"B\"\r\n\r\n--B\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Invitation</p>\r\n\
--B\r\nContent-Type: text/calendar; method={methode}; charset=UTF-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{}\r\n--B--\r\n",
        calendrier(methode, 0, partstat)
    )
    .into_bytes()
}

/// À la manière d'Outlook : la partie calendrier dans `multipart/alternative`,
/// et la même en pièce jointe `.ics` encodée en base64.
fn courriel_outlook() -> Vec<u8> {
    let ics = calendrier("REQUEST", 2, "NEEDS-ACTION");
    let b64 = crate::smtp::base64(ics.as_bytes());
    format!(
        "From: alice@exemple.fr\r\nTo: bob@exemple.fr\r\nSubject: Point\r\nMIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"M\"\r\n\r\n--M\r\nContent-Type: multipart/alternative; boundary=\"A\"\r\n\r\n\
--A\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nInvitation\r\n--A\r\nContent-Type: text/calendar; charset=\"utf-8\"; method=REQUEST\r\n\
Content-Transfer-Encoding: base64\r\n\r\n{b64}\r\n--A--\r\n--M\r\nContent-Type: application/ics; name=\"invite.ics\"\r\n\
Content-Disposition: attachment; filename=\"invite.ics\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{b64}\r\n--M--\r\n"
    )
    .into_bytes()
}

fn paris() -> chrono_tz::Tz {
    "Europe/Paris".parse().unwrap()
}

#[test]
fn invitation_de_sogo_et_d_outlook() {
    let e = extraire(&courriel_sogo("REQUEST", "NEEDS-ACTION")).unwrap();
    assert_eq!(e.methode, "REQUEST");
    let i = decrire(&e, &paris()).unwrap();
    assert_eq!((i.uid.as_str(), i.titre.as_str(), i.lieu.as_str(), i.sequence), ("reunion-1", "Point d'avancement", "Salle 2", 0));
    assert_eq!(i.quand, "mardi 20 octobre 2026, 10:00 – 11:00");
    assert_eq!(i.organisateur.as_ref().map(|o| (o.nom.as_str(), o.adresse.as_str())), Some(("Alice Martin", "alice@exemple.fr")));
    assert_eq!(i.participants.len(), 1);
    assert_eq!((i.participants[0].adresse.as_str(), i.participants[0].statut.as_str()), ("bob@exemple.fr", "NEEDS-ACTION"));

    // Outlook : la partie du corps, décodée, plutôt que la pièce en double.
    let e = extraire(&courriel_outlook()).unwrap();
    assert_eq!(e.methode, "REQUEST");
    assert_eq!(decrire(&e, &paris()).unwrap().sequence, 2);
}

#[test]
fn reponse_annulation_et_simple_fichier_ics() {
    assert_eq!(extraire(&courriel_sogo("REPLY", "ACCEPTED")).unwrap().methode, "REPLY");
    assert_eq!(extraire(&courriel_sogo("CANCEL", "NEEDS-ACTION")).unwrap().methode, "CANCEL");
    // Un agenda joint sans méthode d'invitation n'en est pas une.
    let publie = calendrier("PUBLISH", 0, "NEEDS-ACTION");
    let courriel = format!(
        "From: a@exemple.fr\r\nSubject: Agenda\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"P\"\r\n\r\n\
--P\r\nContent-Type: text/plain\r\n\r\nCi-joint.\r\n--P\r\nContent-Type: text/calendar; name=\"agenda.ics\"\r\n\
Content-Disposition: attachment; filename=\"agenda.ics\"\r\n\r\n{publie}\r\n--P--\r\n"
    );
    assert!(extraire(courriel.as_bytes()).is_none());
    assert!(extraire(b"From: a@exemple.fr\r\nSubject: rien\r\n\r\nTexte.\r\n").is_none());
}

#[test]
fn repondre_a_une_invitation() {
    let invitation = calendrier("REQUEST", 1, "NEEDS-ACTION");
    let ical = repondre(&invitation, None, "Bob@Exemple.fr", "ACCEPTED").unwrap();
    assert!(!ical.contains("METHOD:"), "{ical}");
    assert_eq!(reponse_de(&ical, "bob@exemple.fr"), "ACCEPTED");
    // L'organisateur et le reste de l'événement sont gardés.
    assert!(ical.contains("ORGANIZER;CN=Alice Martin:mailto:alice@exemple.fr"));
    assert_eq!(identite(&ical), Some(("reunion-1".into(), 1)));

    // Une copie plus récente déjà dans l'agenda l'emporte sur une vieille
    // invitation ; une plus ancienne cède.
    let recente = calendrier("REQUEST", 3, "NEEDS-ACTION").replace("Salle 2", "Salle 5").replace("METHOD:REQUEST\r\n", "");
    let ical = repondre(&invitation, Some(&recente), "bob@exemple.fr", "TENTATIVE").unwrap();
    assert!(ical.contains("Salle 5") && ical.contains("PARTSTAT=TENTATIVE"));
    let ical = repondre(&recente.replace("SEQUENCE:3", "SEQUENCE:0"), Some(&recente.replace("SEQUENCE:3", "SEQUENCE:0")), "bob@exemple.fr", "DECLINED").unwrap();
    assert_eq!(reponse_de(&ical, "bob@exemple.fr"), "DECLINED");

    // Invité par une liste : sa ligne est ajoutée.
    let ical = repondre(&invitation, None, "carole@exemple.fr", "ACCEPTED").unwrap();
    assert_eq!(reponse_de(&ical, "carole@exemple.fr"), "ACCEPTED");
    assert_eq!(reponse_de(&ical, "bob@exemple.fr"), "NEEDS-ACTION");

    assert!(repondre(&invitation, None, "bob@exemple.fr", "PEUT-ETRE").is_err());
}

#[test]
fn reponse_inscrite_chez_l_organisateur() {
    let organisateur = calendrier("REQUEST", 0, "NEEDS-ACTION").replace("METHOD:REQUEST\r\n", "");
    let reponse = calendrier("REPLY", 0, "ACCEPTED");
    let inscrite = inscrire_reponse(&organisateur, &reponse).unwrap().unwrap();
    assert_eq!(reponse_de(&inscrite, "bob@exemple.fr"), "ACCEPTED");
    // Déjà inscrite : rien à écrire.
    assert_eq!(inscrire_reponse(&inscrite, &reponse).unwrap(), None);
    // Un participant inconnu de l'événement n'y entre pas.
    let inconnu = reponse.replace("bob@exemple.fr", "intrus@exemple.fr");
    assert_eq!(inscrire_reponse(&organisateur, &inconnu).unwrap(), None);
}
