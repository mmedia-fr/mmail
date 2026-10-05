// SPDX-License-Identifier: GPL-3.0-or-later
//! Faux serveur IMAP pour les tests : il rejoue un dialogue écrit d'avance et
//! consigne les commandes reçues.
//!
//! Les épreuves contre un vrai serveur (`essais_serveur`) exigent un compte ;
//! celles-ci tournent partout, intégration continue comprise. Chaque échange
//! attendu donne le début de la commande (sans son étiquette), les réponses
//! non étiquetées, et la fin (« OK … », « NO … ») que le faux serveur fait
//! précéder de l'étiquette de la commande. Une commande qui ne commence pas
//! comme prévu fait échouer le test.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};

use crate::imap::Client;

/// Une commande attendue et ce que le serveur y répond.
pub struct Echange {
    pub attendu: String,
    pub reponses: String,
    pub fin: String,
}

/// `echange("UID STORE 7", "", "OK fait")`.
pub fn echange(attendu: &str, reponses: &str, fin: &str) -> Echange {
    Echange { attendu: attendu.into(), reponses: reponses.into(), fin: fin.into() }
}

/// Commandes reçues, sans étiquette, littéraux remplacés par « {n octets} ».
pub type Journal = Arc<Mutex<Vec<String>>>;

struct FluxSimule {
    attendus: VecDeque<Echange>,
    journal: Journal,
    /// Ce que le client va lire.
    sortie: VecDeque<u8>,
    /// Ce que le client a écrit et qui n'est pas encore une commande complète.
    entree: Vec<u8>,
    /// Commande en cours de lecture (une ligne coupée par un littéral).
    commande: String,
    /// Octets de littéral restant à lire ; la ligne de la commande reprend
    /// ensuite jusqu'à son CRLF.
    litteral: Option<usize>,
}

impl FluxSimule {
    fn traiter(&mut self) {
        loop {
            if let Some(taille) = self.litteral {
                if self.entree.len() < taille {
                    return;
                }
                self.entree.drain(..taille);
                self.litteral = None;
                continue;
            }
            let Some(fin) = self.entree.windows(2).position(|f| f == b"\r\n") else {
                return;
            };
            let ligne: Vec<u8> = self.entree.drain(..fin + 2).collect();
            let ligne = String::from_utf8_lossy(&ligne[..fin]).into_owned();
            // « … {12} » : littéral synchronisé, le client attend l'invite.
            if let Some(taille) = taille_litteral(&ligne) {
                self.commande.push_str(&ligne[..ligne.rfind('{').unwrap_or(ligne.len())]);
                self.commande.push_str(&format!("{{{taille} octets}}"));
                self.sortie.extend(b"+ pret\r\n");
                self.litteral = Some(taille);
                continue;
            }
            self.commande.push_str(&ligne);
            let complete = std::mem::take(&mut self.commande);
            self.repondre(&complete);
        }
    }

    fn repondre(&mut self, complete: &str) {
        let (etiquette, texte) = complete.split_once(' ').unwrap_or((complete, ""));
        self.journal.lock().unwrap().push(texte.to_string());
        let echange = self
            .attendus
            .pop_front()
            .unwrap_or_else(|| panic!("commande imprévue : {texte}"));
        assert!(
            texte.starts_with(&echange.attendu),
            "commande reçue « {texte} », attendue « {}… »",
            echange.attendu
        );
        self.sortie.extend(echange.reponses.as_bytes());
        self.sortie.extend(format!("{etiquette} {}\r\n", echange.fin).as_bytes());
    }
}

fn taille_litteral(ligne: &str) -> Option<usize> {
    let debut = ligne.rfind('{')?;
    ligne[debut + 1..].strip_suffix('}')?.parse().ok()
}

impl Read for FluxSimule {
    fn read(&mut self, tampon: &mut [u8]) -> io::Result<usize> {
        // Rien à lire : le serveur « ferme », ce que le client prend pour une
        // perte de connexion — un dialogue incomplet échoue donc tout de suite.
        let n = tampon.len().min(self.sortie.len());
        for (i, octet) in self.sortie.drain(..n).enumerate() {
            tampon[i] = octet;
        }
        Ok(n)
    }
}

impl Write for FluxSimule {
    fn write(&mut self, octets: &[u8]) -> io::Result<usize> {
        self.entree.extend_from_slice(octets);
        self.traiter();
        Ok(octets.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Session ouverte sur le faux serveur, qui annonce `capacites` après
/// connexion, puis attend les `echanges` dans l'ordre.
pub fn session(capacites: &str, echanges: Vec<Echange>) -> (Client, Journal) {
    let mut attendus = VecDeque::from(vec![echange(
        "LOGIN",
        "",
        "OK connecté",
    ), echange("CAPABILITY", &format!("* CAPABILITY IMAP4rev1 {capacites}\r\n"), "OK")]);
    if capacites.split_whitespace().any(|c| c == "QRESYNC") {
        attendus.push_back(echange("ENABLE QRESYNC", "* ENABLED QRESYNC\r\n", "OK"));
    }
    attendus.extend(echanges);
    let journal = Journal::default();
    let flux = FluxSimule {
        attendus,
        journal: journal.clone(),
        sortie: VecDeque::from(b"* OK serveur simule\r\n".to_vec()),
        entree: Vec::new(),
        commande: String::new(),
        litteral: None,
    };
    let mut client = Client::sur(Box::new(flux)).expect("salutation");
    client.ouvrir_session("essai@exemple.fr", "secret").expect("session");
    journal.lock().unwrap().clear();
    (client, journal)
}

/// Capacités d'un Dovecot récent, celles que vise MMail.
pub const DOVECOT: &str = "QRESYNC CONDSTORE UIDPLUS MOVE SPECIAL-USE LIST-STATUS IDLE";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depot_avec_litteral() {
        let (mut client, journal) = session(
            DOVECOT,
            vec![echange("APPEND \"Sent\" (\\Seen)", "", "OK [APPENDUID 5 42] fait")],
        );
        let uid = client.deposer("Sent", &["\\Seen".into()], "", b"Subject: x\r\n\r\ncorps").unwrap();
        assert_eq!(uid, Some(42));
        assert_eq!(journal.lock().unwrap()[0], "APPEND \"Sent\" (\\Seen) {19 octets}");
    }
}
