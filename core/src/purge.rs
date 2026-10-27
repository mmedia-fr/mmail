// SPDX-License-Identifier: GPL-3.0-or-later
//! Effacement définitif dans la corbeille et les indésirables (0.6.24,
//! demande de Manu du 27/10/2026) : un message qui s'y trouve n'a plus de
//! corbeille où aller, « Supprimer » l'efface pour de bon ; et ces dossiers se
//! vident en partie, selon une date de réception et les conditions des
//! filtres (cf. `filtres`).
//!
//! La date se cherche sur le serveur (`UID SEARCH BEFORE`, date de réception,
//! celle du tri de la liste) : la purge atteint aussi les messages que l'index
//! du poste n'a pas encore. Les conditions se vérifient ici, sur l'expéditeur
//! et l'objet relus au serveur : `FROM` et `SUBJECT` ne se comportent pas
//! partout de même — par mots chez Gmail ou sous un index plein texte —, et la
//! purge doit effacer ce que les filtres trouveraient, rien d'autre.
//!
//! Rien ne part sans le décompte montré à l'utilisateur : `chercher` rend les
//! UID et la validité du dossier, `effacer` ne touche qu'à ces UID-là, et
//! refuse si le dossier a été recréé entre-temps (`UIDVALIDITY` changée, les
//! mêmes numéros désigneraient d'autres messages).

use std::collections::HashSet;

use chrono::{Datelike, NaiveDate};
use serde::Deserialize;

use crate::filtres::{self, Condition, Recu};
use crate::imap::{Client, Erreur};
use crate::index::ligne_index;
use crate::protocole::ensemble_uid;
use crate::synchro::Echec;

/// Critères d'une purge partielle, en JSON depuis l'interface.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Criteres {
    /// Reçus avant ce jour, exclu : « JJ/MM/AAAA » ou « AAAA-MM-JJ » ; vide :
    /// à toute date.
    #[serde(default)]
    pub avant: String,
    /// Vrai : toutes les conditions ; faux : une au moins.
    #[serde(default = "vrai")]
    pub tout: bool,
    #[serde(default)]
    pub conditions: Vec<Condition>,
}

fn vrai() -> bool {
    true
}

/// Lit et contrôle des critères. Rend un message lisible s'ils ne vont pas :
/// sans date ni condition, la purge serait un vidage complet, qui a son propre
/// geste (« Vider ce dossier… »).
pub fn lire(json: &str) -> Result<Criteres, String> {
    let criteres: Criteres = serde_json::from_str(json).map_err(|e| format!("critères illisibles : {e}"))?;
    if criteres.avant.trim().is_empty() && criteres.conditions.is_empty() {
        return Err("indiquez une date ou une condition".into());
    }
    if !criteres.avant.trim().is_empty() {
        jour_imap(&criteres.avant)?;
    }
    for c in &criteres.conditions {
        if !filtres::CHAMPS.contains(&c.champ.as_str()) {
            return Err(format!("champ inconnu : {}", c.champ));
        }
        if !filtres::OPERATEURS.contains(&c.operateur.as_str()) {
            return Err(format!("comparaison inconnue : {}", c.operateur));
        }
        if c.valeur.trim().is_empty() {
            return Err("une condition est sans texte".into());
        }
    }
    Ok(criteres)
}

/// Jour au format de `SEARCH BEFORE` (RFC 3501) : « 1-Jan-2026 », le mois en
/// anglais quelle que soit la langue du poste.
fn jour_imap(texte: &str) -> Result<String, String> {
    const MOIS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let texte = texte.trim();
    let nombres: Vec<&str> = texte.split(['/', '-', '.']).collect();
    let lu = match nombres.as_slice() {
        [a, m, j] if texte.contains('-') && a.len() == 4 => (a.parse(), m.parse(), j.parse()),
        [j, m, a] if a.len() == 4 => (a.parse(), m.parse(), j.parse()),
        _ => return Err(format!("date illisible : {texte} (attendu : JJ/MM/AAAA)")),
    };
    let date = match lu {
        (Ok(a), Ok(m), Ok(j)) => NaiveDate::from_ymd_opt(a, m, j),
        _ => None,
    };
    let date = date.ok_or_else(|| format!("date illisible : {texte} (attendu : JJ/MM/AAAA)"))?;
    Ok(format!("{}-{}-{}", date.day(), MOIS[date.month0() as usize], date.year()))
}

/// Messages du dossier `chemin` qui répondent aux critères : la validité des
/// UID du dossier, et les UID.
pub fn chercher(client: &mut Client, chemin: &str, criteres: &Criteres) -> Result<(u32, Vec<u32>), Echec> {
    let jour = if criteres.avant.trim().is_empty() {
        None
    } else {
        Some(jour_imap(&criteres.avant).map_err(|m| Echec::Imap(Erreur::Refuse(m)))?)
    };
    let selection = client.selectionner(chemin, None)?;
    let validite = selection.etat.uid_validity;
    if selection.etat.messages == 0 {
        return Ok((validite, Vec::new()));
    }
    let mut uids = client.chercher_avant(jour.as_deref())?;
    if !criteres.conditions.is_empty() && !uids.is_empty() {
        let retenus: HashSet<u32> = client
            .champs(&ensemble_uid(&uids), "FROM SUBJECT")?
            .iter()
            .map(ligne_index)
            .filter(|l| {
                let recu = Recu {
                    uid: l.uid,
                    expediteur: l.expediteur.clone(),
                    adresse: l.adresse.clone(),
                    sujet: l.sujet.clone(),
                    ..Default::default()
                };
                filtres::conditions_remplies(criteres.tout, &criteres.conditions, &recu)
            })
            .map(|l| l.uid)
            .collect();
        uids.retain(|u| retenus.contains(u));
    }
    Ok((validite, uids))
}

/// Efface pour de bon les messages `uids` du dossier `chemin`, ceux-là seuls
/// (`UID EXPUNGE`). `validite` : celle du dossier quand les UID ont été
/// relevés, 0 si on ne la connaît pas.
pub fn effacer(client: &mut Client, chemin: &str, validite: u32, uids: &[u32]) -> Result<(), Echec> {
    let selection = client.selectionner(chemin, None)?;
    if validite != 0 && selection.etat.uid_validity != validite {
        return Err(Echec::Imap(Erreur::Refuse(
            "le dossier a été recréé sur le serveur depuis le décompte : rien n'est effacé".into(),
        )));
    }
    client.supprimer(uids)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simule::{echange, session};

    fn criteres(json: &str) -> Criteres {
        lire(json).unwrap()
    }

    #[test]
    fn controle_des_criteres() {
        assert_eq!(criteres(r#"{"avant": "01/01/2026"}"#).avant, "01/01/2026");
        assert!(criteres(r#"{"avant": "01/01/2026"}"#).tout, "toutes les conditions par défaut");
        assert!(lire("{}").unwrap_err().contains("date ou une condition"));
        assert!(lire(r#"{"avant": "  "}"#).is_err());
        assert!(lire(r#"{"avant": "31/02/2026"}"#).unwrap_err().contains("date illisible"));
        assert!(lire(r#"{"avant": "demain"}"#).is_err());
        let sans_texte = r#"{"conditions": [{"champ": "objet", "operateur": "contient", "valeur": " "}]}"#;
        assert!(lire(sans_texte).unwrap_err().contains("sans texte"));
        let champ = r#"{"conditions": [{"champ": "corps", "operateur": "contient", "valeur": "x"}]}"#;
        assert!(lire(champ).unwrap_err().contains("champ inconnu"));
        let comparaison = r#"{"conditions": [{"champ": "objet", "operateur": "ressemble", "valeur": "x"}]}"#;
        assert!(lire(comparaison).unwrap_err().contains("comparaison inconnue"));
        let seule = r#"{"conditions": [{"champ": "expediteur", "operateur": "finit", "valeur": "@exemple.fr"}]}"#;
        assert_eq!(criteres(seule).conditions.len(), 1);
    }

    #[test]
    fn jour_au_format_de_la_rfc() {
        assert_eq!(jour_imap("01/01/2026").unwrap(), "1-Jan-2026");
        assert_eq!(jour_imap("1/3/2025").unwrap(), "1-Mar-2025");
        assert_eq!(jour_imap(" 15.08.2024 ").unwrap(), "15-Aug-2024");
        assert_eq!(jour_imap("2026-12-31").unwrap(), "31-Dec-2026");
        assert!(jour_imap("2026/12/31").is_err(), "année en tête : seulement avec des tirets");
        assert!(jour_imap("29/02/2025").is_err());
        assert!(jour_imap("01/01/26").is_err(), "année sur quatre chiffres");
    }

    /// Réponse FETCH d'expéditeur et d'objet, littéral compris.
    fn fetch(seq: u32, uid: u32, de: &str, objet: &str) -> String {
        let entete = format!("From: {de}\r\nSubject: {objet}\r\n\r\n");
        format!(
            "* {seq} FETCH (UID {uid} FLAGS () INTERNALDATE \"05-Oct-2025 10:00:00 +0200\" RFC822.SIZE 40 \
             BODY[HEADER.FIELDS (FROM SUBJECT)] {{{}}}\r\n{entete})\r\n",
            entete.len()
        )
    }

    const SELECT: &str = "* 4 EXISTS\r\n* OK [UIDVALIDITY 7] v\r\n";

    #[test]
    fn date_seule_sans_relire_les_en_tetes() {
        let (mut client, journal) = session(
            "UIDPLUS",
            vec![
                echange("SELECT \"Trash\"", SELECT, "OK [READ-WRITE] fait"),
                echange("UID SEARCH BEFORE 1-Jan-2026", "* SEARCH 3 5\r\n", "OK"),
            ],
        );
        let (validite, uids) = chercher(&mut client, "Trash", &criteres(r#"{"avant": "01/01/2026"}"#)).unwrap();
        assert_eq!((validite, uids), (7, vec![3, 5]));
        assert_eq!(journal.lock().unwrap().len(), 2);
    }

    #[test]
    fn conditions_verifiees_sur_les_en_tetes() {
        let reponse = [
            fetch(1, 3, "Banque <avis@banque.fr>", "Relevé de septembre"),
            fetch(2, 5, "Lettre <info@lettre.fr>", "Relevé et nouveautés"),
            fetch(3, 8, "Banque <avis@banque.fr>", "Nouvelle offre"),
        ]
        .concat();
        let (mut client, _) = session(
            "UIDPLUS",
            vec![
                echange("SELECT \"Trash\"", SELECT, "OK [READ-WRITE] fait"),
                echange("UID SEARCH ALL", "* SEARCH 3 5 8\r\n", "OK"),
                echange("UID FETCH", &reponse, "OK"),
            ],
        );
        let json = r#"{"tout": true, "conditions": [
            {"champ": "expediteur", "operateur": "finit", "valeur": "@banque.fr"},
            {"champ": "objet", "operateur": "commence", "valeur": "relevé"}]}"#;
        assert_eq!(chercher(&mut client, "Trash", &criteres(json)).unwrap().1, vec![3]);
    }

    #[test]
    fn une_condition_au_moins() {
        let reponse = [fetch(1, 3, "a@banque.fr", "Relevé"), fetch(2, 5, "b@lettre.fr", "Bonjour")].concat();
        let (mut client, _) = session(
            "UIDPLUS",
            vec![
                echange("SELECT \"Junk\"", SELECT, "OK [READ-WRITE] fait"),
                echange("UID SEARCH BEFORE 1-Jan-2026", "* SEARCH 3 5 9\r\n", "OK"),
                echange("UID FETCH", &reponse, "OK"),
            ],
        );
        let json = r#"{"avant": "2026-01-01", "tout": false, "conditions": [
            {"champ": "expediteur", "operateur": "contient", "valeur": "lettre"},
            {"champ": "objet", "operateur": "est", "valeur": "relevé"}]}"#;
        // 9 n'a pas été rendu par le FETCH (effacé entre-temps) : il n'est pas retenu.
        assert_eq!(chercher(&mut client, "Junk", &criteres(json)).unwrap().1, vec![3, 5]);
    }

    #[test]
    fn dossier_vide_sans_recherche() {
        let (mut client, journal) = session(
            "UIDPLUS",
            vec![echange("SELECT \"Trash\"", "* 0 EXISTS\r\n* OK [UIDVALIDITY 7] v\r\n", "OK [READ-WRITE] fait")],
        );
        let (_, uids) = chercher(&mut client, "Trash", &criteres(r#"{"avant": "01/01/2026"}"#)).unwrap();
        assert!(uids.is_empty());
        assert_eq!(journal.lock().unwrap().len(), 1);
    }

    #[test]
    fn effacement_de_ces_uid_seuls() {
        let (mut client, journal) = session(
            "UIDPLUS",
            vec![
                echange("SELECT \"Trash\"", SELECT, "OK [READ-WRITE] fait"),
                echange("UID STORE 3,5 +FLAGS.SILENT (\\Deleted)", "", "OK"),
                echange("UID EXPUNGE 3,5", "* 1 EXPUNGE\r\n* 1 EXPUNGE\r\n", "OK"),
            ],
        );
        effacer(&mut client, "Trash", 7, &[3, 5]).unwrap();
        assert_eq!(journal.lock().unwrap().len(), 3);
    }

    #[test]
    fn rien_d_efface_si_le_dossier_a_ete_recree() {
        let (mut client, journal) =
            session("UIDPLUS", vec![echange("SELECT \"Trash\"", SELECT, "OK [READ-WRITE] fait")]);
        let e = effacer(&mut client, "Trash", 6, &[3, 5]).unwrap_err();
        assert!(e.to_string().contains("rien n'est effacé"), "{e}");
        assert_eq!(journal.lock().unwrap().len(), 1, "ni STORE ni EXPUNGE");
    }
}
