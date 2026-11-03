// SPDX-License-Identifier: GPL-3.0-or-later
//! Relève en arrière-plan sous Android (0.7.0).
//!
//! Sous Android, MMail quitté — ou simplement passé en arrière-plan, Android
//! gelant alors l'application — ne relève plus rien. Une tâche du système
//! (JobScheduler : toutes les quinze minutes au mieux, réseau présent, cf.
//! `packaging/android/src/fr/mmedia/mmail/Releve.java`) regarde la boîte de
//! réception de chaque compte et montre en notification les messages arrivés.
//!
//! Lecture seule : `EXAMINE` et des en-têtes, ni l'index ni le serveur ne
//! changent — MMail rouvert se synchronise comme d'habitude. Ce qui a déjà
//! été signalé est tenu par la tâche (`Etat`), rendu à chaque appel.
//!
//! Seuil d'un compte : le plus grand UID de la boîte de réception que l'index
//! connaît (ce que MMail a déjà montré), ou le dernier signalé s'il est plus
//! grand. Sans l'un ni l'autre (compte jamais relevé, `UIDVALIDITY` changé),
//! la relève pose le seuil sans rien signaler : pas de notification pour tout
//! un historique. Seuls les messages non lus sont signalés. Limite : un
//! message que MMail range lui-même dans l'index après qu'on l'a quitté (une
//! synchronisation encore en cours, celle d'un compte qu'on vient d'ajouter)
//! passe pour déjà montré, et n'est pas signalé.
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
/// clé : mot de passe, ou ce que le coffre garde des jetons OAuth2).
pub fn relever(index: &str, secrets: &HashMap<String, String>, etat: &Etat) -> Result<Bilan, String> {
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
        let reception = magasin.reception(compte.id).unwrap_or(None);
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

/// Seuil au-delà duquel un message est nouveau, d'après l'index (`reception` :
/// `UIDVALIDITY` et plus grand UID connus de la boîte de réception) et le
/// dernier repère ; `None` quand ni l'un ni l'autre ne vaut pour
/// l'`UIDVALIDITY` du serveur.
pub fn seuil(uidvalidity: u32, reception: Option<(u32, u32)>, repere: Option<Repere>) -> Option<u32> {
    let index = reception.filter(|&(v, uid)| v == uidvalidity && uid > 0).map(|(_, uid)| uid);
    let signale = repere.filter(|r| r.uidvalidity == uidvalidity).map(|r| r.uid);
    index.into_iter().chain(signale).max()
}

/// Relève la boîte de réception d'une session ouverte : nombre de messages
/// arrivés non lus, les `PLAFOND` plus récents, et le repère suivant.
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
    // « n:* » rend au moins le dernier message, même d'UID inférieur à n.
    let mut arrives: Vec<_> = client
        .entetes(&format!("{}:*", seuil.saturating_add(1)))?
        .into_iter()
        .filter(|e| e.uid > seuil)
        .collect();
    neuf.uid = arrives.iter().map(|e| e.uid).max().unwrap_or(seuil).max(seuil);
    arrives.retain(|e| !e.lu());
    arrives.sort_by(|a, b| b.uid.cmp(&a.uid));
    let nombre = arrives.len();
    let messages = arrives
        .iter()
        .take(PLAFOND)
        .map(|e| {
            let ligne = crate::index::ligne_index(e);
            Nouveau { uid: e.uid, de: ligne.expediteur, objet: ligne.sujet }
        })
        .collect();
    Ok((nombre, messages, neuf))
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

    /// Relève : `secrets` `{clé: secret}`, `etat` le dernier rendu (`{}` la
    /// première fois). Rend le bilan, ou `{"erreur": …}`.
    #[no_mangle]
    pub extern "C" fn mmail_releve(index: *const c_char, secrets: *const c_char, etat: *const c_char) -> *mut c_char {
        let (index, secrets, etat) = (texte(index), texte(secrets), texte(etat));
        let r = std::panic::catch_unwind(|| {
            let secrets: HashMap<String, String> = serde_json::from_str(&secrets).unwrap_or_default();
            let etat: super::Etat = serde_json::from_str(&etat).unwrap_or_default();
            super::relever(&index, &secrets, &etat)
        });
        rendre(match r {
            Ok(Ok(b)) => serde_json::to_string(&b).unwrap_or_else(|_| "{}".into()),
            Ok(Err(e)) => serde_json::json!({ "erreur": e }).to_string(),
            Err(_) => serde_json::json!({ "erreur": "erreur interne" }).to_string(),
        })
    }

    /// Libère une chaîne rendue par l'une des deux fonctions ci-dessus.
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
        let (mut client, _) = session(
            "IMAP4rev1",
            vec![
                echange("EXAMINE \"INBOX\"", "* 2 EXISTS\r\n* OK [UIDVALIDITY 7] x\r\n* OK [UIDNEXT 44] x\r\n", "OK fait"),
                echange("UID FETCH 42:*", &[fetch(1, 42, "", "Jean Dupont <jean@exemple.fr>", "Devis"), fetch(2, 43, "\\Seen", "Marie <marie@exemple.fr>", "Lu ailleurs")].concat(), "OK fait"),
            ],
        );
        let (nombre, messages, neuf) = relever_boite(&mut client, Some((7, 41)), None).unwrap();
        // 43 est lu (sur un autre appareil) : seul 42 est signalé.
        assert_eq!(nombre, 1);
        assert_eq!(messages, vec![Nouveau { uid: 42, de: "Jean Dupont".into(), objet: "Devis".into() }]);
        // Le repère passe au plus grand UID vu, lu ou non.
        assert_eq!(neuf, Repere { uidvalidity: 7, uid: 43 });
    }
}
