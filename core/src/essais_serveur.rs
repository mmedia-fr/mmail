// SPDX-License-Identifier: GPL-3.0-or-later
//! Épreuve du client contre un vrai Dovecot.
//!
//! Ignoré par défaut : il exige un serveur et deux comptes, donnés par
//! l'environnement. Aucune identification n'est écrite ici.
//!
//! ```text
//! MMAIL_HOTE=imap.exemple.fr MMAIL_UTILISATEUR=… MMAIL_MOTDEPASSE=… \
//! MMAIL_UTILISATEUR2=… MMAIL_MOTDEPASSE2=… \
//!   cargo test --manifest-path core/Cargo.toml essais_serveur -- --ignored --test-threads=1
//! ```
//!
//! Chaque épreuve dépose ses propres messages, marqués d'un `Message-ID`
//! unique, et les retire en fin de parcours : le contenu d'essai des boîtes
//! n'est pas touché. `--test-threads=1` : deux épreuves qui écrivent dans la
//! même boîte en même temps fausseraient leurs comptes.


#![cfg(test)]

use std::time::{SystemTime, UNIX_EPOCH};

use crate::deplacement;
use crate::imap::Client;
use crate::magasin::{Etape, Magasin, Operation};
use crate::protocole::secondes_date_interne;
use crate::synchro;

fn variable(nom: &str) -> String {
    std::env::var(nom).unwrap_or_else(|_| panic!("{nom} absente de l'environnement"))
}

fn session() -> Client {
    session_de("MMAIL_UTILISATEUR", "MMAIL_MOTDEPASSE")
}

fn session2() -> Client {
    session_de("MMAIL_UTILISATEUR2", "MMAIL_MOTDEPASSE2")
}

fn session_de(utilisateur: &str, mot_de_passe: &str) -> Client {
    let mut client = Client::connecter(&variable("MMAIL_HOTE"), 993).expect("connexion");
    client
        .ouvrir_session(&variable(utilisateur), &variable(mot_de_passe))
        .expect("session");
    client
}

/// Index en mémoire portant les deux comptes d'essai, arborescences relues.
fn index(a: &mut Client, b: Option<&mut Client>) -> (Magasin, i64, i64) {
    let magasin = Magasin::en_memoire().unwrap();
    let hote = variable("MMAIL_HOTE");
    let ua = variable("MMAIL_UTILISATEUR");
    let ca = magasin.compte(&ua, &hote, 993, &ua).unwrap();
    synchro::arborescence(a, &magasin, ca).expect("arborescence A");
    let mut cb = 0;
    if let Some(b) = b {
        let ub = variable("MMAIL_UTILISATEUR2");
        cb = magasin.compte(&ub, &hote, 993, &ub).unwrap();
        synchro::arborescence(b, &magasin, cb).expect("arborescence B");
    }
    (magasin, ca, cb)
}

/// Message d'essai au `Message-ID` unique.
fn message_essai(objet: &str) -> (String, Vec<u8>) {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let id = format!("mmail-essai-{unique}@exemple.fr");
    let brut = format!(
        "From: Essai MMail <mmail@exemple.fr>\r\n\
         To: test@exemple.fr\r\n\
         Subject: {objet} \u{e9}preuve\r\n\
         Date: Tue, 22 Sep 2026 10:00:00 +0200\r\n\
         Message-ID: <{id}>\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: 8bit\r\n\
         \r\n\
         Corps d'essai d\u{e9}pos\u{e9} par les \u{e9}preuves de MMail.\r\n"
    );
    (id, brut.into_bytes())
}

const DATE_ESSAI: &str = "22-Sep-2026 10:00:00 +0200";

/// Retire d'un dossier tout message portant un `Message-ID` donné.
fn nettoyer(client: &mut Client, chemin: &str, message_id: &str) {
    client.selectionner(chemin, None).expect("SELECT pour nettoyage");
    let uids = client.chercher_message_id(message_id).expect("SEARCH pour nettoyage");
    client.supprimer(&uids).expect("purge de nettoyage");
}

fn compter(client: &mut Client, chemin: &str, message_id: &str) -> Vec<u32> {
    client.examiner(chemin).expect("EXAMINE");
    client.chercher_message_id(message_id).expect("SEARCH")
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn capacites_attendues() {
    let client = session();
    for capacite in ["QRESYNC", "CONDSTORE", "UIDPLUS", "MOVE", "SPECIAL-USE", "IDLE"] {
        assert!(client.sait(capacite), "{capacite} manquante");
    }
    println!("capacités : {}", client.capacites().join(" "));
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn arborescence_et_roles() {
    let mut client = session();
    let dossiers = client.dossiers().expect("LIST");
    println!("{} dossiers", dossiers.len());
    for d in &dossiers {
        println!("  {:<24} profondeur {} rôle {:?}", d.chemin, d.profondeur(), d.role());
    }
    assert!(dossiers.iter().any(|d| d.chemin == "INBOX"));
    // Les rôles viennent de SPECIAL-USE : c'est ce qui permet de placer
    // « Envoyés » et « Corbeille » sans deviner d'après leur nom.
    assert!(dossiers.iter().any(|d| d.role() == Some("Sent")));
    assert!(dossiers.iter().any(|d| d.role() == Some("Trash")));
    // Un sous-dossier doit être reconnu comme tel.
    assert!(dossiers.iter().any(|d| d.profondeur() == 1));
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn index_et_message_accentue() {
    let mut client = session();
    let etat = client.examiner("INBOX").expect("EXAMINE");
    println!("INBOX : {etat:?}");
    assert!(etat.messages > 0, "la boîte d'essai doit contenir des messages");
    assert!(etat.uid_validity > 0);
    assert!(etat.uid_next > etat.messages);
    // HIGHESTMODSEQ conditionne la synchronisation incrémentale à venir.
    assert!(etat.highest_mod_seq > 0, "CONDSTORE annoncé mais pas de HIGHESTMODSEQ");

    let entetes = client.entetes("1:*").expect("FETCH");
    assert_eq!(entetes.len() as u32, etat.messages);
    let premier = &entetes[0];
    println!(
        "premier message : uid {} taille {} drapeaux {:?}",
        premier.uid, premier.taille, premier.drapeaux
    );
    assert!(premier.taille > 0);
    assert!(!premier.date_interne.is_empty());

    // Les en-têtes bruts doivent porter le sujet, accents compris une fois
    // décodés : c'est mail-parser qui s'en charge côté index.
    let brut = String::from_utf8_lossy(&premier.brut);
    assert!(brut.contains("Subject:"), "en-têtes sans sujet : {brut}");

    let corps = client.corps(premier.uid).expect("BODY.PEEK[]");
    assert!(corps.len() >= premier.taille as usize / 2);
    let texte = String::from_utf8_lossy(&corps);
    assert!(texte.contains("Subject:"));
    println!("corps de {} octets", corps.len());

    // BODY.PEEK ne doit pas avoir marqué le message comme lu.
    let apres = client.entetes(&premier.uid.to_string()).expect("FETCH 2");
    assert_eq!(apres[0].lu(), premier.lu(), "la lecture a changé le drapeau \\Seen");
    client.fermer();
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn synchronisation_incrementale() {
    let mut a = session();
    let mut autre = session();
    assert!(a.qresync(), "QRESYNC doit être activé sur Dovecot");
    let (magasin, compte, _) = index(&mut a, None);
    let id = magasin.dossier_id(compte, "INBOX").unwrap();

    // L'arborescence relue avec ses compteurs garde les rôles SPECIAL-USE.
    let dossiers = magasin.dossiers(compte).unwrap();
    for role in ["Sent", "Trash", "Drafts", "Junk"] {
        assert!(dossiers.iter().any(|d| d.role == role), "rôle {role} perdu : {dossiers:?}");
    }
    assert!(dossiers.iter().any(|d| d.messages > 0), "compteurs absents");

    // Première visite : relecture complète, compteurs cohérents.
    let premier = synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 1");
    assert!(premier.complet);
    let dossier = magasin.dossier(id).unwrap().unwrap();
    assert_eq!(dossier.messages as usize, magasin.uids(id).unwrap().len());

    // Seconde visite sans changement : reprise QRESYNC, rien à relire.
    let second = synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 2");
    assert!(!second.complet, "la reprise doit être incrémentale : {second:?}");
    assert_eq!((second.nouveaux, second.retires), (0, 0));

    // Un message arrive par une autre session.
    let (message_id, brut) = message_essai("Synchronisation");
    let uid = autre
        .deposer("INBOX", &["\\Seen".into()], DATE_ESSAI, &brut)
        .expect("APPEND")
        .expect("APPENDUID");
    let troisieme = synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 3");
    assert!(!troisieme.complet);
    assert_eq!(troisieme.nouveaux, 1);
    let ligne = magasin.message(id, uid).unwrap().expect("message indexé");
    assert_eq!(ligne.message_id, message_id);
    assert!(ligne.lu);
    assert!(ligne.sujet.contains("épreuve"), "sujet : {}", ligne.sujet);
    assert_eq!(Some(ligne.horodatage), secondes_date_interne(DATE_ESSAI));

    // Un drapeau change ailleurs : QRESYNC le rend.
    autre.selectionner("INBOX", None).unwrap();
    autre.marquer(&[uid], "\\Seen", false).unwrap();
    let quatrieme = synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 4");
    assert!(!quatrieme.complet);
    assert!(!magasin.message(id, uid).unwrap().unwrap().lu, "\\Seen retiré ailleurs");

    // Marquage depuis le client : serveur et index suivent.
    synchro::marquer_lu(&mut a, &magasin, compte, "INBOX", &[uid], true).unwrap();
    assert!(magasin.message(id, uid).unwrap().unwrap().lu);

    // Le message disparaît ailleurs : VANISHED le retire de l'index.
    autre.supprimer(&[uid]).expect("purge");
    let cinquieme = synchro::synchroniser(&mut a, &magasin, compte, "INBOX").expect("sync 5");
    assert!(!cinquieme.complet);
    assert!(cinquieme.retires >= 1);
    assert!(magasin.message(id, uid).unwrap().is_none());
    assert_eq!(magasin.uids(id).unwrap().len(), premier_compte(&magasin, id));
}

fn premier_compte(magasin: &Magasin, id: i64) -> usize {
    magasin.dossier(id).unwrap().unwrap().messages as usize
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn deplacement_dans_la_boite_et_retour() {
    let mut a = session();
    let (magasin, compte, _) = index(&mut a, None);
    let (message_id, brut) = message_essai("Tri interne");
    let uid = a.deposer("INBOX", &[], DATE_ESSAI, &brut).unwrap().unwrap();
    synchro::synchroniser(&mut a, &magasin, compte, "INBOX").unwrap();
    let inbox = magasin.dossier_id(compte, "INBOX").unwrap();
    assert!(magasin.message(inbox, uid).unwrap().is_some());

    let rapport =
        deplacement::dans_la_boite(&mut a, &magasin, compte, "INBOX", &[uid], "Essais/Clients").unwrap();
    assert_eq!(rapport.deplaces, 1);
    assert!(magasin.message(inbox, uid).unwrap().is_none(), "retiré de l'index source");
    assert!(compter(&mut a, "INBOX", &message_id).is_empty());
    let la = compter(&mut a, "Essais/Clients", &message_id);
    assert_eq!(la.len(), 1);

    // Retour, depuis le dossier cible synchronisé.
    synchro::synchroniser(&mut a, &magasin, compte, "Essais/Clients").unwrap();
    deplacement::dans_la_boite(&mut a, &magasin, compte, "Essais/Clients", &la, "INBOX").unwrap();
    assert!(compter(&mut a, "Essais/Clients", &message_id).is_empty());
    assert_eq!(compter(&mut a, "INBOX", &message_id).len(), 1);
    nettoyer(&mut a, "INBOX", &message_id);
}

#[test]
#[ignore = "exige un serveur IMAP et deux comptes"]
fn deplacement_entre_boites_et_retour() {
    let mut a = session();
    let mut b = session2();
    let mut annexe_b = session2();
    let mut annexe_a = session();
    let (magasin, ca, cb) = index(&mut a, Some(&mut b));
    let atelier = std::env::temp_dir().join(format!("mmail-essai-{}", std::process::id()));

    let (message_id, brut) = message_essai("Tri entre boîtes");
    let drapeaux = vec!["\\Seen".to_string(), "\\Flagged".to_string()];
    let uid = a.deposer("INBOX", &drapeaux, DATE_ESSAI, &brut).unwrap().unwrap();
    synchro::synchroniser(&mut a, &magasin, ca, "INBOX").unwrap();

    // A/INBOX → B/Archive.
    let rapport = deplacement::entre_boites(
        &mut a, &mut annexe_b, &magasin, ca, "INBOX", &[uid], cb, "Archive", &atelier,
    )
    .expect("déplacement A → B");
    assert_eq!(rapport.deplaces, 1, "{rapport:?}");
    assert!(rapport.erreurs.is_empty());
    assert_eq!(magasin.nombre_operations().unwrap(), 0, "aucune opération ne doit rester en file");
    assert!(
        std::fs::read_dir(&atelier).map(|d| d.count()).unwrap_or(0) == 0,
        "la copie de travail doit être effacée"
    );
    assert!(compter(&mut a, "INBOX", &message_id).is_empty(), "purgé de la source");
    let dans_b = compter(&mut b, "Archive", &message_id);
    assert_eq!(dans_b.len(), 1, "présent une fois dans la cible");

    // Drapeaux, date de réception et octets conservés.
    b.selectionner("Archive", None).unwrap();
    let copie = b.message_complet(dans_b[0]).unwrap().expect("message dans B");
    assert!(copie.drapeaux.iter().any(|d| d == "\\Seen"));
    assert!(copie.drapeaux.iter().any(|d| d == "\\Flagged"));
    assert_eq!(secondes_date_interne(&copie.date_interne), secondes_date_interne(DATE_ESSAI));
    assert_eq!(copie.octets, brut, "le message doit arriver octet pour octet");

    // Retour B/Archive → A/INBOX.
    synchro::synchroniser(&mut b, &magasin, cb, "Archive").unwrap();
    let rapport = deplacement::entre_boites(
        &mut b, &mut annexe_a, &magasin, cb, "Archive", &dans_b, ca, "INBOX", &atelier,
    )
    .expect("déplacement B → A");
    assert_eq!(rapport.deplaces, 1);
    assert!(compter(&mut b, "Archive", &message_id).is_empty());
    assert_eq!(compter(&mut a, "INBOX", &message_id).len(), 1);
    nettoyer(&mut a, "INBOX", &message_id);
    let _ = std::fs::remove_dir_all(&atelier);
}

#[test]
#[ignore = "exige un serveur IMAP et deux comptes"]
fn reprise_d_un_deplacement_interrompu_sans_doublon() {
    let mut a = session();
    let mut b = session2();
    let mut annexe_b = session2();
    let (magasin, ca, cb) = index(&mut a, Some(&mut b));
    let atelier = std::env::temp_dir().join(format!("mmail-reprise-{}", std::process::id()));
    std::fs::create_dir_all(&atelier).unwrap();

    // Situation d'une coupure juste après le dépôt : le message est dans la
    // source ET dans la cible, l'opération est restée à l'étape « déposer ».
    let (message_id, brut) = message_essai("Reprise");
    let uid = a.deposer("INBOX", &[], DATE_ESSAI, &brut).unwrap().unwrap();
    b.deposer("Archive", &[], DATE_ESSAI, &brut).unwrap();
    synchro::synchroniser(&mut a, &magasin, ca, "INBOX").unwrap();
    let validite = a.selectionner("INBOX", None).unwrap().etat.uid_validity;
    let fichier = atelier.join("reprise.eml");
    std::fs::write(&fichier, &brut).unwrap();
    let mut operation = Operation {
        id: 0,
        compte_source: ca,
        chemin_source: "INBOX".into(),
        validite_source: validite,
        uid_source: uid,
        compte_cible: cb,
        chemin_cible: "Archive".into(),
        message_id: message_id.clone(),
        drapeaux: String::new(),
        date_interne: DATE_ESSAI.into(),
        fichier: fichier.to_string_lossy().into_owned(),
        etape: Etape::Deposer,
        erreur: String::new(),
    };
    operation.id = magasin.inscrire_operation(&operation).unwrap();

    deplacement::reprendre(&mut a, &mut annexe_b, &magasin, &operation).expect("reprise");
    assert_eq!(compter(&mut b, "Archive", &message_id).len(), 1, "pas de doublon dans la cible");
    assert!(compter(&mut a, "INBOX", &message_id).is_empty(), "source purgée");
    assert_eq!(magasin.nombre_operations().unwrap(), 0);
    assert!(!fichier.exists(), "copie effacée une fois l'opération soldée");

    nettoyer(&mut b, "Archive", &message_id);
    let _ = std::fs::remove_dir_all(&atelier);
}

#[test]
#[ignore = "exige un serveur IMAP et un compte"]
fn piece_jointe_relue_octet_pour_octet() {
    let mut a = session();
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let message_id = format!("mmail-essai-{unique}@exemple.fr");
    // Contenu binaire couvrant toutes les valeurs d'octet : un décodage
    // base64 approximatif ne passerait pas.
    let contenu: Vec<u8> = (0..=255u8).cycle().take(3000).collect();
    let base64 = base64_simple(&contenu);
    let brut = format!(
        "From: Essai MMail <mmail@exemple.fr>\r\n\
         Subject: Pi\u{e8}ce jointe d'\u{e9}preuve\r\n\
         Message-ID: <{message_id}>\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"LIMITE\"\r\n\
         \r\n\
         --LIMITE\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         Voir la pi\u{e8}ce jointe.\r\n\
         --LIMITE\r\n\
         Content-Type: application/octet-stream; name=\"donn\u{e9}es.bin\"\r\n\
         Content-Disposition: attachment; filename=\"donnees.bin\"\r\n\
         Content-Transfer-Encoding: base64\r\n\
         \r\n\
         {base64}\r\n\
         --LIMITE--\r\n"
    );
    let uid = a.deposer("INBOX", &[], DATE_ESSAI, brut.as_bytes()).unwrap().unwrap();
    a.selectionner("INBOX", None).unwrap();
    let octets = a.corps(uid).expect("corps");
    let pieces = crate::index::pieces_jointes(&octets);
    assert_eq!(pieces.len(), 1, "{pieces:?}");
    assert_eq!(pieces[0].nom, "donnees.bin");
    assert!(!pieces[0].risquee);
    let (_, relu) = crate::index::extraire_piece(&octets, 0).expect("extraction");
    assert_eq!(relu, contenu, "la pièce jointe doit revenir octet pour octet");
    nettoyer(&mut a, "INBOX", &message_id);
}

/// Base64 standard, lignes de 76 caractères (RFC 2045).
fn base64_simple(octets: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut sortie = String::new();
    for bloc in octets.chunks(3) {
        let n = (bloc[0] as u32) << 16
            | (*bloc.get(1).unwrap_or(&0) as u32) << 8
            | *bloc.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= bloc.len() {
                sortie.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                sortie.push('=');
            }
        }
    }
    sortie
        .as_bytes()
        .chunks(76)
        .map(|l| std::str::from_utf8(l).unwrap())
        .collect::<Vec<_>>()
        .join("\r\n")
}
