// SPDX-License-Identifier: GPL-3.0-or-later
//! Synchronisation incrémentale d'un dossier (décision 15 du dossier de projet).
//!
//! Le principe : ne relire du serveur que ce qui a changé depuis la dernière
//! visite. L'index local porte, par dossier, l'`UIDVALIDITY`, l'`UIDNEXT` et le
//! `HIGHESTMODSEQ` vus la dernière fois ; le serveur, interrogé en QRESYNC
//! (RFC 7162), rend alors les UID retirés (`VANISHED`) et les drapeaux changés
//! depuis ce `HIGHESTMODSEQ`, et il ne reste qu'à lire les en-têtes des
//! messages arrivés au-delà de l'ancien `UIDNEXT`.
//!
//! Trois garde-fous :
//!
//! - **un changement d'`UIDVALIDITY` vide le dossier local** et le relit en
//!   entier : les UID d'avant ne désignent plus rien ;
//! - **l'état du dossier n'est noté qu'une fois les messages rangés** : une
//!   coupure au milieu laisse l'ancien état, et la visite suivante reprend
//!   au même point au lieu de sauter des messages ;
//! - **le nombre de messages de l'index est comparé à celui du serveur** : un
//!   écart, quelle qu'en soit la cause, déclenche une réconciliation complète
//!   par la liste des UID. C'est aussi le chemin des serveurs sans QRESYNC.

use std::collections::HashSet;

use crate::imap::{self, Client};
use crate::index::ligne_index;
use crate::magasin::{self, Magasin, MessageLocal};
use crate::protocole::{ensemble_uid, EtatDossier};

/// Échec d'une opération qui touche à la fois le serveur et l'index.
#[derive(Debug)]
pub enum Echec {
    Imap(imap::Erreur),
    Index(magasin::Erreur),
}

impl std::fmt::Display for Echec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Echec::Imap(e) => write!(f, "{e}"),
            Echec::Index(e) => write!(f, "index local : {e}"),
        }
    }
}

impl From<imap::Erreur> for Echec {
    fn from(e: imap::Erreur) -> Self {
        Echec::Imap(e)
    }
}

impl From<magasin::Erreur> for Echec {
    fn from(e: magasin::Erreur) -> Self {
        Echec::Index(e)
    }
}

impl Echec {
    /// Vrai si la connexion est perdue : la session est alors à rouvrir.
    pub fn reseau(&self) -> bool {
        matches!(self, Echec::Imap(imap::Erreur::Reseau(_)))
    }
}

/// Ce qu'une synchronisation a fait.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Bilan {
    pub dossier_id: i64,
    pub nouveaux: usize,
    pub retires: usize,
    /// Vrai si le dossier a été relu en entier (première visite, rupture
    /// d'`UIDVALIDITY` ou réconciliation).
    pub complet: bool,
}

/// Relit l'arborescence d'un compte et les compteurs de tous ses dossiers.
pub fn arborescence(client: &mut Client, magasin: &Magasin, compte: i64) -> Result<(), Echec> {
    let (dossiers, statuts) = client.dossiers_et_compteurs()?;
    magasin.poser_dossiers(compte, &dossiers)?;
    magasin.poser_compteurs(compte, &statuts)?;
    Ok(())
}

/// Sélectionne un dossier et met son index à jour. Le dossier reste
/// sélectionné : c'est sur lui que portent ensuite lectures et marquages.
pub fn synchroniser(
    client: &mut Client,
    magasin: &Magasin,
    compte: i64,
    chemin: &str,
) -> Result<Bilan, Echec> {
    let id = magasin.dossier_id(compte, chemin)?;
    let local = magasin
        .dossier(id)?
        .ok_or_else(|| magasin::Erreur(format!("dossier inconnu : {chemin}")))?;
    let reprise = (local.uid_validity != 0).then_some((local.uid_validity, local.highest_mod_seq));
    let selection = client.selectionner(chemin, reprise)?;
    let etat = selection.etat.clone();
    let mut bilan = Bilan { dossier_id: id, ..Default::default() };

    let rupture = local.uid_validity != 0 && local.uid_validity != etat.uid_validity;
    if rupture {
        // L'index du dossier est caduc : on le vide tout de suite, l'état
        // complet n'étant noté qu'à la fin.
        magasin.poser_etat(
            id,
            &EtatDossier { uid_validity: etat.uid_validity, ..Default::default() },
        )?;
    }

    if local.uid_validity == 0 || rupture {
        bilan.complet = true;
        bilan.nouveaux = relire_tout(client, magasin, id, &etat)?;
    } else if selection.reprise {
        magasin.retirer_intervalles(id, &selection.disparus)?;
        bilan.retires = selection.disparus.iter().map(|(a, b)| (b - a + 1) as usize).sum();
        magasin.poser_drapeaux(id, &selection.drapeaux)?;
        if etat.uid_next > local.uid_next {
            bilan.nouveaux = lire_depuis(client, magasin, id, local.uid_next.max(1))?;
        }
    } else {
        let (nouveaux, retires) = reconcilier(client, magasin, id, &etat)?;
        bilan.complet = true;
        bilan.nouveaux = nouveaux;
        bilan.retires = retires;
    }

    // Contrôle de cohérence : l'index doit compter autant de messages que le
    // serveur. Un écart se répare par la liste des UID, sans tout relire.
    if !bilan.complet && magasin.uids(id)?.len() as u32 != etat.messages {
        let (nouveaux, retires) = reconcilier(client, magasin, id, &etat)?;
        bilan.complet = true;
        bilan.nouveaux += nouveaux;
        bilan.retires += retires;
    }

    magasin.poser_etat(id, &etat)?;
    magasin.recompter(id)?;
    Ok(bilan)
}

/// Relit tout le dossier : en-têtes de chaque message.
fn relire_tout(
    client: &mut Client,
    magasin: &Magasin,
    id: i64,
    etat: &EtatDossier,
) -> Result<usize, Echec> {
    if etat.messages == 0 {
        // « 1:* » sur un dossier vide fait répondre une erreur à certains
        // serveurs ; et ce qui traînait dans l'index n'existe plus.
        let connus = magasin.uids(id)?;
        magasin.retirer(id, &connus)?;
        return Ok(0);
    }
    let entetes = client.entetes("1:*")?;
    let presents: HashSet<u32> = entetes.iter().map(|e| e.uid).collect();
    let perimes: Vec<u32> =
        magasin.uids(id)?.into_iter().filter(|u| !presents.contains(u)).collect();
    magasin.retirer(id, &perimes)?;
    let lignes: Vec<MessageLocal> = entetes.iter().map(ligne_index).collect();
    magasin.poser_messages(id, &lignes)?;
    Ok(lignes.len())
}

/// En-têtes des messages arrivés à partir d'un UID.
fn lire_depuis(client: &mut Client, magasin: &Magasin, id: i64, depuis: u32) -> Result<usize, Echec> {
    // « n:* » rend au moins le dernier message, même si son UID est inférieur
    // à n (RFC 3501 § 6.4.8) : on filtre.
    let lignes: Vec<MessageLocal> = client
        .entetes(&format!("{depuis}:*"))?
        .iter()
        .filter(|e| e.uid >= depuis)
        .map(ligne_index)
        .collect();
    magasin.poser_messages(id, &lignes)?;
    Ok(lignes.len())
}

/// Réconciliation complète par la liste des UID et de leurs drapeaux : ne
/// relit les en-têtes que des messages absents de l'index.
fn reconcilier(
    client: &mut Client,
    magasin: &Magasin,
    id: i64,
    etat: &EtatDossier,
) -> Result<(usize, usize), Echec> {
    if etat.messages == 0 {
        let connus = magasin.uids(id)?;
        magasin.retirer(id, &connus)?;
        return Ok((0, connus.len()));
    }
    let serveur = client.tous_les_drapeaux()?;
    let presents: HashSet<u32> = serveur.iter().map(|(u, _)| *u).collect();
    let connus: HashSet<u32> = magasin.uids(id)?.into_iter().collect();

    let perimes: Vec<u32> = connus.difference(&presents).copied().collect();
    magasin.retirer(id, &perimes)?;
    magasin.poser_drapeaux(id, &serveur)?;

    let manquants: Vec<u32> = presents.difference(&connus).copied().collect();
    let mut nouveaux = 0;
    // Par paquets : une commande par millier d'UID reste de taille raisonnable
    // même quand les UID sont épars.
    for paquet in manquants.chunks(1000) {
        let lignes: Vec<MessageLocal> =
            client.entetes(&ensemble_uid(paquet))?.iter().map(ligne_index).collect();
        nouveaux += lignes.len();
        magasin.poser_messages(id, &lignes)?;
    }
    Ok((nouveaux, perimes.len()))
}

/// Marque des messages comme lus ou non lus, sur le serveur puis dans l'index.
pub fn marquer_lu(
    client: &mut Client,
    magasin: &Magasin,
    compte: i64,
    chemin: &str,
    uids: &[u32],
    lu: bool,
) -> Result<(), Echec> {
    assurer_selection(client, chemin)?;
    client.marquer(uids, "\\Seen", lu)?;
    let id = magasin.dossier_id(compte, chemin)?;
    magasin.marquer_lu(id, uids, lu)?;
    Ok(())
}

/// Sélectionne un dossier s'il ne l'est pas déjà, sans resynchroniser.
pub fn assurer_selection(client: &mut Client, chemin: &str) -> Result<(), Echec> {
    if client.selection() != Some(chemin) {
        client.selectionner(chemin, None)?;
    }
    Ok(())
}
