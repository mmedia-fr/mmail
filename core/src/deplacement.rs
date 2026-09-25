// SPDX-License-Identifier: GPL-3.0-or-later
//! Déplacement de messages, dans une boîte ou d'une boîte vers une autre
//! (décisions 12 et 16 du dossier de projet).
//!
//! **Dans une même boîte**, c'est le serveur qui déplace (`UID MOVE`) : une
//! commande, atomique.
//!
//! **D'une boîte vers une autre**, aucun protocole ne le fait : le message est
//! lu en entier à la source, déposé dans la cible (`APPEND`, avec ses drapeaux
//! et sa date de réception d'origine), puis seulement retiré de la source. Entre
//! les deux, une coupure ne doit rien perdre. D'où la file `pending_ops` :
//!
//! 1. le message est lu et **copié sur disque** (`.eml`), puis l'opération est
//!    inscrite à l'étape « déposer » ;
//! 2. une fois le dépôt accepté par la cible, l'opération passe à « purger » ;
//! 3. une fois le message retiré de la source, l'opération est soldée et la
//!    copie effacée.
//!
//! Une opération interrompue reprend à son étape. À l'étape « déposer », on ne
//! sait pas si le dépôt a eu lieu : la cible est d'abord interrogée par
//! `Message-ID`, et le message n'est redéposé que s'il n'y est pas. Au pire un
//! doublon — un message sans `Message-ID` redéposé —, jamais une perte : la
//! source n'est purgée qu'après un dépôt confirmé.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::imap::{Client, MessageComplet};
use crate::magasin::{Etape, Magasin, Operation};
use crate::synchro::{assurer_selection, Echec};

/// Ce qu'un déplacement a fait.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Rapport {
    pub deplaces: usize,
    /// Messages disparus de la source avant d'avoir été lus : rien à faire.
    pub absents: usize,
    pub erreurs: Vec<String>,
}

/// Déplace des messages d'un dossier vers un autre de la même boîte.
pub fn dans_la_boite(
    client: &mut Client,
    magasin: &Magasin,
    compte: i64,
    source: &str,
    uids: &[u32],
    cible: &str,
) -> Result<Rapport, Echec> {
    if source == cible || uids.is_empty() {
        return Ok(Rapport::default());
    }
    assurer_selection(client, source)?;
    client.deplacer(uids, cible)?;
    let id = magasin.dossier_id(compte, source)?;
    magasin.retirer(id, uids)?;
    magasin.recompter(id)?;
    Ok(Rapport { deplaces: uids.len(), ..Default::default() })
}

/// Déplace des messages vers un dossier d'une autre boîte.
///
/// `client` est la session de la boîte source ; `annexe`, une session ouverte
/// sur la boîte cible. Les copies de travail vont dans `atelier`.
#[allow(clippy::too_many_arguments)]
pub fn entre_boites(
    client: &mut Client,
    annexe: &mut Client,
    magasin: &Magasin,
    compte_source: i64,
    source: &str,
    uids: &[u32],
    compte_cible: i64,
    cible: &str,
    atelier: &Path,
) -> Result<Rapport, Echec> {
    let mut rapport = Rapport::default();
    if uids.is_empty() {
        return Ok(rapport);
    }
    fs::create_dir_all(atelier)
        .map_err(|e| crate::magasin::Erreur(format!("atelier {} : {e}", atelier.display())))?;
    let selection = client.selectionner(source, None)?;
    let validite = selection.etat.uid_validity;
    let id_source = magasin.dossier_id(compte_source, source)?;

    for &uid in uids {
        // 1. Lecture et copie de sûreté.
        let Some(message) = client.message_complet(uid)? else {
            magasin.retirer(id_source, &[uid])?;
            rapport.absents += 1;
            continue;
        };
        let message_id = match magasin.message(id_source, uid)? {
            Some(m) if !m.message_id.is_empty() => m.message_id,
            _ => message_id_de(&message.octets),
        };
        let fichier = atelier.join(format!("{compte_source}-{validite}-{uid}.eml"));
        ecrire_copie(&fichier, &message.octets)
            .map_err(|e| crate::magasin::Erreur(format!("copie {} : {e}", fichier.display())))?;
        let mut operation = Operation {
            id: 0,
            compte_source,
            chemin_source: source.to_string(),
            validite_source: validite,
            uid_source: uid,
            compte_cible,
            chemin_cible: cible.to_string(),
            message_id,
            drapeaux: message.drapeaux.join(" "),
            date_interne: message.date_interne.clone(),
            fichier: fichier.to_string_lossy().into_owned(),
            etape: Etape::Deposer,
            erreur: String::new(),
        };
        operation.id = magasin.inscrire_operation(&operation)?;

        // 2. Dépôt dans la cible.
        if let Err(e) = annexe.deposer(cible, &message.drapeaux, &message.date_interne, &message.octets) {
            if matches!(e, crate::imap::Erreur::Reseau(_)) {
                // On ne sait pas si le dépôt a eu lieu : l'opération reste
                // inscrite, la reprise interrogera la cible.
                magasin.noter_echec_operation(operation.id, &e.to_string())?;
                return Err(Echec::Imap(e));
            }
            // Refus franc (quota, dossier absent…) : rien n'a été déposé, le
            // message reste où il est.
            abandonner(magasin, &operation)?;
            rapport.erreurs.push(format!("message {uid} : {e}"));
            continue;
        }
        magasin.avancer_operation(operation.id, Etape::Purger)?;

        // 3. Retrait de la source.
        purger(client, magasin, &operation, id_source)?;
        rapport.deplaces += 1;
    }
    magasin.recompter(id_source)?;
    Ok(rapport)
}

/// Reprend un déplacement interrompu, à son étape.
pub fn reprendre(
    client: &mut Client,
    annexe: &mut Client,
    magasin: &Magasin,
    operation: &Operation,
) -> Result<(), Echec> {
    if operation.etape == Etape::Deposer {
        let deja_la = if operation.message_id.is_empty() {
            false
        } else {
            annexe.examiner(&operation.chemin_cible)?;
            !annexe.chercher_message_id(&operation.message_id)?.is_empty()
        };
        if !deja_la {
            let octets = match fs::read(&operation.fichier) {
                Ok(o) => o,
                Err(e) => {
                    // Sans la copie, rien à déposer : le message est toujours
                    // à la source, puisqu'elle n'est purgée qu'après dépôt.
                    magasin.noter_echec_operation(operation.id, &format!("copie illisible : {e}"))?;
                    magasin.clore_operation(operation.id)?;
                    return Ok(());
                }
            };
            let message = MessageComplet {
                drapeaux: operation.drapeaux.split_whitespace().map(str::to_string).collect(),
                date_interne: operation.date_interne.clone(),
                octets,
            };
            annexe.deposer(
                &operation.chemin_cible,
                &message.drapeaux,
                &message.date_interne,
                &message.octets,
            )?;
        }
        magasin.avancer_operation(operation.id, Etape::Purger)?;
    }

    let selection = client.selectionner(&operation.chemin_source, None)?;
    if selection.etat.uid_validity != operation.validite_source {
        // Les UID de la source ont été renumérotés : l'UID noté ne désigne
        // plus ce message, le purger pourrait en supprimer un autre. Le
        // message est déjà dans la cible ; on s'arrête là, au pire un doublon.
        magasin.noter_echec_operation(operation.id, "UIDVALIDITY de la source changée")?;
        magasin.clore_operation(operation.id)?;
        let _ = fs::remove_file(&operation.fichier);
        return Ok(());
    }
    let id_source = magasin.dossier_id(operation.compte_source, &operation.chemin_source)?;
    purger(client, magasin, operation, id_source)?;
    magasin.recompter(id_source)?;
    Ok(())
}

fn purger(client: &mut Client, magasin: &Magasin, operation: &Operation, id_source: i64) -> Result<(), Echec> {
    if let Err(e) = client.supprimer(&[operation.uid_source]) {
        magasin.noter_echec_operation(operation.id, &e.to_string())?;
        return Err(Echec::Imap(e));
    }
    magasin.retirer(id_source, &[operation.uid_source])?;
    magasin.clore_operation(operation.id)?;
    let _ = fs::remove_file(&operation.fichier);
    Ok(())
}

fn abandonner(magasin: &Magasin, operation: &Operation) -> Result<(), Echec> {
    magasin.clore_operation(operation.id)?;
    let _ = fs::remove_file(&operation.fichier);
    Ok(())
}

/// Écrit la copie de sûreté et la force sur le disque avant de continuer.
fn ecrire_copie(chemin: &PathBuf, octets: &[u8]) -> std::io::Result<()> {
    let mut fichier = fs::File::create(chemin)?;
    fichier.write_all(octets)?;
    fichier.sync_all()
}

/// `Message-ID` d'un message brut, à défaut de celui de l'index.
fn message_id_de(octets: &[u8]) -> String {
    mail_parser::MessageParser::default()
        .parse_headers(octets)
        .and_then(|m| m.message_id().map(str::to_string))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_id_lu_dans_le_brut() {
        let brut = b"From: a@b.fr\r\nMessage-ID: <x.1@b.fr>\r\nSubject: s\r\n\r\ncorps\r\n";
        assert_eq!(message_id_de(brut), "x.1@b.fr");
        assert_eq!(message_id_de(b"From: a@b.fr\r\n\r\ncorps"), "");
    }
}
