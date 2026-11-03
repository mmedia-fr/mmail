// SPDX-License-Identifier: GPL-3.0-or-later
//! Relève en arrière-plan sous Android (0.7.0).
//!
//! Sous Android, MMail quitté — ou simplement passé en arrière-plan, Android
//! gelant alors l'application — ne relève plus rien. Une tâche du système
//! (JobScheduler : toutes les quinze minutes au mieux, réseau présent, cf.
//! `packaging/android/src/fr/mmedia/mmail/Releve.java`) regarde la boîte de
//! réception de chaque compte et montre en notification les messages arrivés.
//!
//! Lecture seule : `EXAMINE`, une recherche et des en-têtes, ni l'index ni le serveur ne
//! changent — MMail rouvert se synchronise comme d'habitude. Ce qui a déjà
//! été signalé est tenu par la tâche (`Etat`), rendu à chaque appel.
//!
//! Seuil d'un compte : ce que MMail a montré — le plus grand UID de la boîte
//! de réception que l'index connaissait quand on a quitté MMail (`vu`, noté
//! par Releve.java) —, ou le dernier signalé s'il est plus grand. L'index
//! d'après ne compte pas : MMail quitté finit en arrière-plan la
//! synchronisation en cours, et range des messages que personne n'a vus.
//! Sans l'un ni l'autre (compte jamais relevé, `UIDVALIDITY` changé), la
//! relève pose le seuil sans rien signaler : pas de notification pour tout un
//! historique. Seuls les messages non lus sont signalés.
//!
//! La notification d'un compte reprend tous les messages non lus arrivés
//! depuis ce que MMail a montré, et pas seulement ceux de la dernière
//! relève ; elle n'est reposée (et ne sonne) que si quelque chose est arrivé
//! depuis la précédente.
//!
//! Les filtres ne passent pas ici : un message qu'un filtre rangera à la
//! prochaine ouverture de MMail est signalé quand même.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::imap::{Client, Erreur};
use crate::magasin::{CompteLocal, Magasin};
use crate::oauth::Fournisseur;

/// Messages détaillés au plus, par compte et par relève ; les autres ne sont
/// que comptés.
pub const PLAFOND: usize = 20;

/// Dernier message signalé d'un compte : UID dans sa boîte de réception, et
/// l'`UIDVALIDITY` qui lui donne un sens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repere {
    pub uidvalidity: u32,
    pub uid: u32,
}

/// Repères de la tâche, par clé de compte (celle du coffre).
pub type Etat = HashMap<String, Repere>;

/// Un compte à relever : sa clé au coffre, `adresse@serveur` (cf.
/// `cle(adresse, hote)` de Main.qml), et son adresse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompteReleve {
    pub cle: String,
    pub adresse: String,
}

/// Un message arrivé, à signaler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Nouveau {
    pub uid: u32,
    pub de: String,
    pub objet: String,
}

/// Ce qu'une relève rend pour un compte.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Arrivees {
    pub cle: String,
    pub adresse: String,
    /// Nombre de messages arrivés ; `messages` n'en détaille que `PLAFOND`,
    /// les plus récents d'abord.
    pub nombre: usize,
    pub messages: Vec<Nouveau>,
    /// Motif de l'échec de la relève de ce compte ; vide sinon.
    pub echec: String,
}

/// Bilan d'une relève : les comptes qui ont du nouveau ou ont échoué, et les
/// repères à rendre à la prochaine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Bilan {
    pub comptes: Vec<Arrivees>,
    pub etat: Etat,
}

/// Clé d'un compte au coffre.
pub fn cle(compte: &CompteLocal) -> String {
    format!("{}@{}", compte.adresse, compte.hote)
}

/// Ce que MMail a montré, noté quand on le quitte : par compte, l'`UIDVALIDITY`
/// et le plus grand UID que l'index connaît de sa boîte de réception. Un
/// compte dont la boîte n'a pas encore été relevée n'y figure pas.
pub fn vu(index: &str) -> Result<Etat, String> {
    let mut vu = Etat::new();
    let Some(magasin) = ouvrir(index)? else {
        return Ok(vu);
    };
    for compte in magasin.comptes().map_err(|e| e.to_string())? {
        if let Ok(Some((uidvalidity, uid))) = magasin.reception(compte.id) {
            vu.insert(cle(&compte), Repere { uidvalidity, uid });
        }
    }
    Ok(vu)
}

/// Comptes du profil, pour que la tâche lise leurs secrets au coffre. Un
/// index absent : aucun (rien n'est créé).
pub fn comptes(index: &str) -> Result<Vec<CompteReleve>, String> {
    let Some(magasin) = ouvrir(index)? else {
        return Ok(Vec::new());
    };
    let comptes = magasin.comptes().map_err(|e| e.to_string())?;
    Ok(comptes.iter().map(|c| CompteReleve { cle: cle(c), adresse: c.adresse.clone() }).collect())
}

fn ouvrir(index: &str) -> Result<Option<Magasin>, String> {
    if index.is_empty() || !std::path::Path::new(index).is_file() {
        return Ok(None);
    }
    Magasin::ouvrir(index).map(Some).map_err(|e| e.to_string())
}

/// Relève les comptes du profil dont la tâche a le secret (`secrets`, par
/// clé : mot de passe, ou ce que le coffre garde des jetons OAuth2). `vu` :
/// ce que MMail a montré (cf. `vu`) ; un compte qui n'y figure pas prend
/// l'index tel qu'il est.
pub fn relever(index: &str, secrets: &HashMap<String, String>, etat: &Etat, vu: &Etat) -> Result<Bilan, String> {
    let mut bilan = Bilan::default();
    let Some(magasin) = ouvrir(index)? else {
        return Ok(bilan);
    };
    let comptes = magasin.comptes().map_err(|e| e.to_string())?;
    for compte in &comptes {
        let cle = cle(compte);
        let repere = etat.get(&cle).copied();
        // Sans secret (coffre illisible, compte en cours d'ajout), le repère
        // est gardé tel quel.
        let Some(secret) = secrets.get(&cle) else {
            if let Some(r) = repere {
                bilan.etat.insert(cle, r);
            }
            continue;
        };
        let reception = match vu.get(&cle) {
            Some(r) => Some((r.uidvalidity, r.uid)),
            None => magasin.reception(compte.id).unwrap_or(None),
        };
        let issue = Client::connecter(&compte.hote, compte.port)
            .and_then(|mut client| {
                authentifier(&mut client, compte, secret)?;
                let r = relever_boite(&mut client, reception, repere);
                client.fermer();
                r
            });
        match issue {
            Ok((nombre, messages, neuf)) => {
                bilan.etat.insert(cle.clone(), neuf);
                if nombre > 0 {
                    bilan.comptes.push(Arrivees { cle, adresse: compte.adresse.clone(), nombre, messages, echec: String::new() });
                }
            }
            Err(e) => {
                if let Some(r) = repere {
                    bilan.etat.insert(cle.clone(), r);
                }
                bilan.comptes.push(Arrivees { cle, adresse: compte.adresse.clone(), echec: e.to_string(), ..Default::default() });
            }
        }
    }
    Ok(bilan)
}

/// Ouvre la session : mot de passe, ou jeton d'accès tiré du jeton de
/// rafraîchissement (Microsoft, Google). Un jeton de rafraîchissement
/// renouvelé par le service n'est pas conservé : l'ancien reste valable, et
/// MMail rouvert fait le renouvellement qui va au coffre.
fn authentifier(client: &mut Client, compte: &CompteLocal, secret: &str) -> Result<(), Erreur> {
    let Some(service) = Fournisseur::de_auth(&compte.auth) else {
        return client.ouvrir_session(&compte.utilisateur, secret);
    };
    let jetons = crate::oauth::partager_du_coffre(service, secret);
    let jeton = crate::oauth::jeton_acces(&jetons, service.portee_imap()).map_err(|e| e.vers_erreur(service.nom()))?;
    client.ouvrir_session_jeton(&compte.utilisateur, &jeton)
}

/// Seuil au-delà duquel un message est nouveau, d'après ce que MMail a montré
/// (`reception` : `UIDVALIDITY` et plus grand UID de la boîte de réception
/// connus de l'index) et le dernier repère ; `None` quand ni l'un ni l'autre ne vaut pour
/// l'`UIDVALIDITY` du serveur.
pub fn seuil(uidvalidity: u32, reception: Option<(u32, u32)>, repere: Option<Repere>) -> Option<u32> {
    let index = reception.filter(|&(v, uid)| v == uidvalidity && uid > 0).map(|(_, uid)| uid);
    let signale = repere.filter(|r| r.uidvalidity == uidvalidity).map(|r| r.uid);
    index.into_iter().chain(signale).max()
}

/// Relève la boîte de réception d'une session ouverte : nombre de messages
/// non lus arrivés depuis ce que MMail a montré, les `PLAFOND` plus récents,
/// et le repère suivant. Nombre nul quand rien n'est arrivé depuis la relève
/// précédente (ou seulement des messages déjà lus ailleurs) : la
/// notification en place reste telle quelle.
fn relever_boite(client: &mut Client, reception: Option<(u32, u32)>, repere: Option<Repere>) -> Result<(usize, Vec<Nouveau>, Repere), Erreur> {
    let etat = client.examiner("INBOX")?;
    let haut = etat.uid_next.saturating_sub(1);
    let Some(seuil) = seuil(etat.uid_validity, reception, repere) else {
        // Première relève : le seuil est posé, rien n'est signalé.
        return Ok((0, Vec::new(), Repere { uidvalidity: etat.uid_validity, uid: haut }));
    };
    let mut neuf = Repere { uidvalidity: etat.uid_validity, uid: seuil };
    // UIDNEXT est presque toujours annoncé ; sans lui, on demande.
    if etat.messages == 0 || (etat.uid_next != 0 && haut <= seuil) {
        return Ok((0, Vec::new(), neuf));
    }
    // Depuis ce que MMail a montré, ou à défaut le dernier signalé.
    let montre = reception.filter(|&(v, uid)| v == etat.uid_validity && uid > 0).map(|(_, uid)| uid);
    let depuis = montre.unwrap_or(seuil).min(seuil);
    let mut non_lus: Vec<u32> = client
        .chercher_non_lus(depuis.saturating_add(1))?
        .into_iter()
        .filter(|&uid| uid > depuis)
        .collect();
    neuf.uid = if etat.uid_next != 0 { haut.max(seuil) } else { non_lus.iter().copied().fold(seuil, u32::max) };
    if !non_lus.iter().any(|&uid| uid > seuil) {
        return Ok((0, Vec::new(), neuf));
    }
    non_lus.sort_unstable_by(|a, b| b.cmp(a));
    let detail: Vec<String> = non_lus.iter().take(PLAFOND).map(u32::to_string).collect();
    let mut entetes = client.entetes(&detail.join(","))?;
    entetes.retain(|e| non_lus.contains(&e.uid));
    entetes.sort_by(|a, b| b.uid.cmp(&a.uid));
    let messages = entetes
        .iter()
        .map(|e| {
            let ligne = crate::index::ligne_index(e);
            Nouveau { uid: e.uid, de: ligne.expediteur, objet: ligne.sujet }
        })
        .collect();
    Ok((non_lus.len(), messages, neuf))
}

/// Interface C de la tâche Android (cf. `cpp/releve_android.cpp`, qui en fait
/// les méthodes natives de `Releve.java`). Entrées et sorties en UTF-8, JSON
/// pour ce qui est structuré ; chaque chaîne rendue se libère par
/// `mmail_releve_liberer`.
#[cfg(target_os = "android")]
mod interface {
    use std::collections::HashMap;
    use std::ffi::{c_char, CStr, CString};

    fn texte(p: *const c_char) -> String {
        if p.is_null() {
            return String::new();
        }
        // SAFETY : chaîne terminée par un zéro, tenue par l'appelant le temps
        // de l'appel.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }

    fn rendre(t: String) -> *mut c_char {
        CString::new(t.replace('\0', "")).unwrap_or_default().into_raw()
    }

    /// Comptes du profil : `[{"cle": …, "adresse": …}]`, ou `{"erreur": …}`.
    #[no_mangle]
    pub extern "C" fn mmail_releve_comptes(index: *const c_char) -> *mut c_char {
        let index = texte(index);
        let r = std::panic::catch_unwind(|| super::comptes(&index));
        rendre(match r {
            Ok(Ok(c)) => serde_json::to_string(&c).unwrap_or_else(|_| "[]".into()),
            Ok(Err(e)) => serde_json::json!({ "erreur": e }).to_string(),
            Err(_) => serde_json::json!({ "erreur": "erreur interne" }).to_string(),
        })
    }

    /// Ce que MMail a montré : `{clé: {"uidvalidity", "uid"}}`, ou
    /// `{"erreur": …}`. Appelé par MMail quand on le quitte.
    #[no_mangle]
    pub extern "C" fn mmail_releve_vu(index: *const c_char) -> *mut c_char {
        let index = texte(index);
        let r = std::panic::catch_unwind(|| super::vu(&index));
        rendre(match r {
            Ok(Ok(v)) => serde_json::to_string(&v).unwrap_or_else(|_| "{}".into()),
            Ok(Err(e)) => serde_json::json!({ "erreur": e }).to_string(),
            Err(_) => serde_json::json!({ "erreur": "erreur interne" }).to_string(),
        })
    }

    /// Relève : `secrets` `{clé: secret}`, `etat` le dernier rendu (`{}` la
    /// première fois), `vu` le dernier noté par MMail (`{}` sans). Rend le
    /// bilan, ou `{"erreur": …}`.
    #[no_mangle]
    pub extern "C" fn mmail_releve(index: *const c_char, secrets: *const c_char, etat: *const c_char, vu: *const c_char) -> *mut c_char {
        let (index, secrets, etat, vu) = (texte(index), texte(secrets), texte(etat), texte(vu));
        let r = std::panic::catch_unwind(|| {
            let secrets: HashMap<String, String> = serde_json::from_str(&secrets).unwrap_or_default();
            let etat: super::Etat = serde_json::from_str(&etat).unwrap_or_default();
            // `{"erreur": …}` ou illisible : rien de noté.
            let vu: super::Etat = serde_json::from_str(&vu).unwrap_or_default();
            super::relever(&index, &secrets, &etat, &vu)
        });
        rendre(match r {
            Ok(Ok(b)) => serde_json::to_string(&b).unwrap_or_else(|_| "{}".into()),
            Ok(Err(e)) => serde_json::json!({ "erreur": e }).to_string(),
            Err(_) => serde_json::json!({ "erreur": "erreur interne" }).to_string(),
        })
    }

    /// Libère une chaîne rendue par l'une des fonctions ci-dessus.
    #[no_mangle]
    pub extern "C" fn mmail_releve_liberer(p: *mut c_char) {
        if !p.is_null() {
            // SAFETY : `p` vient de `CString::into_raw` (cf. `rendre`).
            drop(unsafe { CString::from_raw(p) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simule::{echange, session};

    #[test]
    fn seuil_de_l_index_ou_du_repere() {
        // Ni index ni repère : rien à signaler, le seuil sera posé.
        assert_eq!(seuil(7, None, None), None);
        // Index de la même UIDVALIDITY : son plus grand UID.
        assert_eq!(seuil(7, Some((7, 40)), None), Some(40));
        // Repère plus avancé que l'index : le repère.
        assert_eq!(seuil(7, Some((7, 40)), Some(Repere { uidvalidity: 7, uid: 45 })), Some(45));
        // Index plus avancé (MMail rouvert entre deux relèves) : l'index.
        assert_eq!(seuil(7, Some((7, 50)), Some(Repere { uidvalidity: 7, uid: 45 })), Some(50));
        // UIDVALIDITY changée : ni l'un ni l'autre ne vaut.
        assert_eq!(seuil(8, Some((7, 50)), Some(Repere { uidvalidity: 7, uid: 45 })), None);
        // Index vide (dossier jamais relevé) : le repère seul.
        assert_eq!(seuil(7, Some((7, 0)), Some(Repere { uidvalidity: 7, uid: 3 })), Some(3));
    }

    fn fetch(seq: u32, uid: u32, drapeaux: &str, de: &str, objet: &str) -> String {
        let entete = format!("From: {de}\r\nSubject: {objet}\r\n\r\n");
        format!(
            "* {seq} FETCH (UID {uid} FLAGS ({drapeaux}) INTERNALDATE \"02-Nov-2026 21:00:00 +0100\" RFC822.SIZE 40 \
             BODY[HEADER.FIELDS (FROM SUBJECT)] {{{}}}\r\n{entete})\r\n",
            entete.len()
        )
    }

    #[test]
    fn premiere_releve_pose_le_seuil() {
        let (mut client, journal) = session(
            "IMAP4rev1",
            vec![echange("EXAMINE \"INBOX\"", "* 12 EXISTS\r\n* OK [UIDVALIDITY 7] x\r\n* OK [UIDNEXT 44] x\r\n", "OK [READ-ONLY] fait")],
        );
        let (nombre, messages, neuf) = relever_boite(&mut client, None, None).unwrap();
        assert_eq!((nombre, messages.len()), (0, 0));
        assert_eq!(neuf, Repere { uidvalidity: 7, uid: 43 });
        assert_eq!(journal.lock().unwrap().len(), 1, "aucun en-tête demandé");
    }

    #[test]
    fn rien_de_neuf_sans_fetch() {
        let (mut client, journal) = session(
            "IMAP4rev1",
            vec![echange("EXAMINE \"INBOX\"", "* 12 EXISTS\r\n* OK [UIDVALIDITY 7] x\r\n* OK [UIDNEXT 44] x\r\n", "OK fait")],
        );
        let (nombre, _, neuf) = relever_boite(&mut client, Some((7, 43)), None).unwrap();
        assert_eq!(nombre, 0);
        assert_eq!(neuf, Repere { uidvalidity: 7, uid: 43 });
        assert_eq!(journal.lock().unwrap().len(), 1);
    }

    #[test]
    fn arrives_non_lus_signales() {
        let (mut client, journal) = session(
            "IMAP4rev1",
            vec![
                echange("EXAMINE \"INBOX\"", "* 2 EXISTS\r\n* OK [UIDVALIDITY 7] x\r\n* OK [UIDNEXT 44] x\r\n", "OK fait"),
                // 43 est lu (sur un autre appareil) : le serveur ne rend que 42.
                echange("UID SEARCH UNSEEN UID 42:*", "* SEARCH 42\r\n", "OK fait"),
                echange("UID FETCH 42 ", &fetch(1, 42, "", "Jean Dupont <jean@exemple.fr>", "Devis"), "OK fait"),
            ],
        );
        let (nombre, messages, neuf) = relever_boite(&mut client, Some((7, 41)), None).unwrap();
        assert_eq!(nombre, 1);
        assert_eq!(messages, vec![Nouveau { uid: 42, de: "Jean Dupont".into(), objet: "Devis".into() }]);
        // Le repère passe au dernier message de la boîte, lu ou non.
        assert_eq!(neuf, Repere { uidvalidity: 7, uid: 43 });
        assert_eq!(journal.lock().unwrap().len(), 3);
    }

    #[test]
    fn notification_cumulee_depuis_ce_que_mmail_a_montre() {
        // MMail a montré jusqu'à 40 ; 41 a déjà été signalé, 42 lu ailleurs,
        // 43 et 44 arrivent : la notification reprend 44, 43 et 41.
        let (mut client, journal) = session(
            "IMAP4rev1",
            vec![
                echange("EXAMINE \"INBOX\"", "* 4 EXISTS\r\n* OK [UIDVALIDITY 7] x\r\n* OK [UIDNEXT 45] x\r\n", "OK fait"),
                echange("UID SEARCH UNSEEN UID 41:*", "* SEARCH 41 43 44\r\n", "OK fait"),
                echange(
                    "UID FETCH 44,43,41 ",
                    &[fetch(1, 41, "", "Anne <anne@exemple.fr>", "Premier"), fetch(3, 43, "", "Paul <paul@exemple.fr>", "Deuxième"), fetch(4, 44, "", "Marie <marie@exemple.fr>", "Troisième")].concat(),
                    "OK fait",
                ),
            ],
        );
        let (nombre, messages, neuf) = relever_boite(&mut client, Some((7, 40)), Some(Repere { uidvalidity: 7, uid: 42 })).unwrap();
        assert_eq!(nombre, 3);
        assert_eq!(messages.iter().map(|m| m.uid).collect::<Vec<_>>(), vec![44, 43, 41]);
        assert_eq!(neuf, Repere { uidvalidity: 7, uid: 44 });
        assert_eq!(journal.lock().unwrap().len(), 3);
    }

    #[test]
    fn arrives_deja_lus_rien_a_reposer() {
        // 43 est arrivé, mais déjà lu ailleurs ; 41, non lu, a déjà été
        // signalé : la notification en place reste, le repère avance.
        let (mut client, journal) = session(
            "IMAP4rev1",
            vec![
                echange("EXAMINE \"INBOX\"", "* 3 EXISTS\r\n* OK [UIDVALIDITY 7] x\r\n* OK [UIDNEXT 44] x\r\n", "OK fait"),
                echange("UID SEARCH UNSEEN UID 41:*", "* SEARCH 41\r\n", "OK fait"),
            ],
        );
        let (nombre, messages, neuf) = relever_boite(&mut client, Some((7, 40)), Some(Repere { uidvalidity: 7, uid: 42 })).unwrap();
        assert_eq!((nombre, messages.len()), (0, 0));
        assert_eq!(neuf, Repere { uidvalidity: 7, uid: 43 });
        assert_eq!(journal.lock().unwrap().len(), 2, "aucun en-tête demandé");
    }
}
