// SPDX-License-Identifier: GPL-3.0-or-later
//! Ce que chaque compte fait en ce moment (0.6.22).
//!
//! « Synchronisation… » ne disait ni quel compte, ni quel dossier, ni combien
//! il en restait (remarque de Guillaume LUDGER, 26/10/2026). Les fils de
//! travail notent ici leur tâche et son avancement ; l'interface en lit une
//! phrase à la seconde : compte, dossier, nombre et volume faits sur le total,
//! durée restante estimée d'après le rythme observé.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Ce qu'un compte fait.
#[derive(Debug, Clone)]
pub struct Activite {
    pub compte: i64,
    pub tache: Tache,
    /// Chemin IMAP du dossier ; vide pour la liste des dossiers.
    pub dossier: String,
    pub faits: u64,
    /// Zéro : inconnu.
    pub total: u64,
    pub octets: u64,
    /// Zéro : inconnu.
    pub octets_total: u64,
    pub debut: Instant,
    /// Numéro de la tâche : sa `Fin` n'efface qu'elle.
    pub numero: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tache {
    /// La liste des dossiers et leurs compteurs.
    Dossiers,
    /// Les en-têtes des messages d'un dossier.
    Messages,
    /// Les messages récents gardés sur le poste (décision 17).
    Contenus,
}

static EN_COURS: Mutex<Vec<Activite>> = Mutex::new(Vec::new());
static NUMERO: AtomicU64 = AtomicU64::new(1);

fn avec<T>(f: impl FnOnce(&mut Vec<Activite>) -> T) -> T {
    // Un fil qui a paniqué en tenant le verrou n'empêche pas les autres
    // d'écrire : la liste n'a pas d'invariant que la panique aurait rompu.
    let mut liste = EN_COURS.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut liste)
}

/// Le compte commence une tâche, qui remplace la précédente. Elle s'efface
/// quand le `Fin` rendu est abandonné — tâche finie, ou en échec.
#[must_use]
pub fn commencer(compte: i64, tache: Tache, dossier: &str, total: u64, octets_total: u64) -> Fin {
    let numero = NUMERO.fetch_add(1, Ordering::Relaxed);
    avec(|l| {
        l.retain(|a| a.compte != compte);
        l.push(Activite {
            compte,
            tache,
            dossier: dossier.to_string(),
            faits: 0,
            total,
            octets: 0,
            octets_total,
            debut: Instant::now(),
            numero,
        });
    });
    Fin(numero)
}

/// Le total de la tâche en cours du compte, connu une fois la tâche commencée
/// (messages à lire d'un dossier).
pub fn poser_total(compte: i64, total: u64) {
    avec(|l| {
        if let Some(a) = l.iter_mut().find(|a| a.compte == compte) {
            a.total = total;
        }
    });
}

/// Avancement de la tâche en cours du compte.
pub fn avancer(compte: i64, faits: u64, octets: u64) {
    avec(|l| {
        if let Some(a) = l.iter_mut().find(|a| a.compte == compte) {
            a.faits = faits;
            a.octets = octets;
        }
    });
}

/// Efface sa tâche à sa sortie de portée — elle seule : une tâche du même
/// compte commencée depuis reste.
pub struct Fin(u64);

impl Drop for Fin {
    fn drop(&mut self) {
        let numero = self.0;
        avec(|l| l.retain(|a| a.numero != numero));
    }
}

/// Les tâches en cours, dans l'ordre où elles ont commencé.
pub fn en_cours() -> Vec<Activite> {
    avec(|l| l.clone())
}

/// Secondes restantes au rythme observé ; `None` tant qu'il ne dit rien —
/// total inconnu, rien de fait, ou moins de deux secondes écoulées.
pub fn restant(faits: u64, total: u64, ecoule: f64) -> Option<u64> {
    if total == 0 || faits == 0 || faits >= total || ecoule < 2.0 {
        return None;
    }
    Some((ecoule * (total - faits) as f64 / faits as f64).ceil() as u64)
}

/// « environ 40 s », « environ 3 min », « environ 1 h 20 ».
pub fn duree_lisible(secondes: u64) -> String {
    match secondes {
        0..=59 => format!("environ {} s", secondes.div_ceil(5).max(1) * 5),
        60..=3599 => format!("environ {} min", secondes.div_ceil(60)),
        _ => {
            let minutes = secondes.div_ceil(60);
            format!("environ {} h {:02}", minutes / 60, minutes % 60)
        }
    }
}

/// « 850 ko », « 12 Mo », « 1,4 Go ».
pub fn volume_lisible(octets: u64) -> String {
    const KO: u64 = 1024;
    const MO: u64 = 1024 * KO;
    const GO: u64 = 1024 * MO;
    if octets < MO {
        format!("{} ko", octets.div_ceil(KO))
    } else if octets < 10 * GO {
        if octets < GO {
            format!("{} Mo", (octets + MO / 2) / MO)
        } else {
            format!("{},{} Go", octets / GO, (octets % GO) * 10 / GO)
        }
    } else {
        format!("{} Go", (octets + GO / 2) / GO)
    }
}

/// « 1 300 » : les milliers séparés comme en français.
fn nombre(n: u64) -> String {
    let chiffres = n.to_string();
    let mut sortie = String::new();
    for (i, c) in chiffres.chars().enumerate() {
        if i > 0 && (chiffres.len() - i) % 3 == 0 {
            sortie.push('\u{202f}');
        }
        sortie.push(c);
    }
    sortie
}

/// Nom d'un dossier à montrer : le dernier élément de son chemin, la boîte
/// de réception nommée en français.
pub fn nom_dossier(chemin: &str) -> String {
    if chemin.eq_ignore_ascii_case("INBOX") {
        return "Boîte de réception".into();
    }
    let decode = crate::protocole::decoder_utf7(chemin);
    decode.rsplit(['/', '.']).next().unwrap_or(&decode).to_string()
}

/// La phrase de la barre d'état : « eb@mmedia.fr › Boîte de réception :
/// en-têtes 1 000 / 13 000, environ 2 min ».
pub fn phrase(a: &Activite, adresse: &str, ecoule: f64) -> String {
    let ou = if a.dossier.is_empty() { adresse.to_string() } else { format!("{adresse} › {}", nom_dossier(&a.dossier)) };
    let mut quoi = match a.tache {
        Tache::Dossiers => "liste des dossiers".to_string(),
        Tache::Messages if a.total > 0 => format!("en-têtes {} / {}", nombre(a.faits), nombre(a.total)),
        Tache::Messages => "nouveaux messages".to_string(),
        // Le préchargement va par lots : ce qui reste parle mieux qu'un compte
        // qui repartirait de zéro à chaque lot.
        Tache::Contenus if a.total > a.faits => {
            let reste = a.total - a.faits;
            format!("mise sur le poste, {} {}", nombre(reste), if reste > 1 { "messages restants" } else { "message restant" })
        }
        Tache::Contenus => "mise sur le poste".to_string(),
    };
    if a.octets_total > a.octets {
        quoi.push_str(&format!(" ({})", volume_lisible(a.octets_total - a.octets)));
    }
    // Le volume, quand il est connu, mesure le travail mieux que le nombre :
    // un message de 20 Mo n'en vaut pas un de 20 ko.
    let reste = if a.octets_total > 0 { restant(a.octets, a.octets_total, ecoule) } else { restant(a.faits, a.total, ecoule) };
    if let Some(s) = reste {
        quoi.push_str(&format!(", {}", duree_lisible(s)));
    }
    format!("{ou} : {quoi}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duree_restante_au_rythme_observe() {
        assert_eq!(restant(250, 1000, 10.0), Some(30));
        assert_eq!(restant(1, 3, 2.5), Some(5));
        assert_eq!(restant(0, 1000, 10.0), None, "rien de fait");
        assert_eq!(restant(10, 0, 10.0), None, "total inconnu");
        assert_eq!(restant(10, 100, 1.0), None, "trop tôt");
        assert_eq!(restant(100, 100, 10.0), None, "fini");
    }

    #[test]
    fn durees_et_volumes_en_francais() {
        assert_eq!(duree_lisible(3), "environ 5 s");
        assert_eq!(duree_lisible(41), "environ 45 s");
        assert_eq!(duree_lisible(61), "environ 2 min");
        assert_eq!(duree_lisible(4800), "environ 1 h 20");
        assert_eq!(volume_lisible(1), "1 ko");
        assert_eq!(volume_lisible(870_000), "850 ko");
        assert_eq!(volume_lisible(12 * 1024 * 1024 + 100), "12 Mo");
        assert_eq!(volume_lisible(1_503_238_554), "1,4 Go");
        assert_eq!(volume_lisible(30 * 1024 * 1024 * 1024), "30 Go");
        assert_eq!(nombre(1300), "1\u{202f}300");
        assert_eq!(nombre(999), "999");
        assert_eq!(nombre(1_000_000), "1\u{202f}000\u{202f}000");
    }

    #[test]
    fn phrases_de_la_barre_d_etat() {
        let a = |tache, dossier: &str, faits, total, octets, octets_total| Activite {
            compte: 1,
            tache,
            dossier: dossier.into(),
            faits,
            total,
            octets,
            octets_total,
            debut: Instant::now(),
            numero: 0,
        };
        assert_eq!(phrase(&a(Tache::Dossiers, "", 0, 0, 0, 0), "eb@mmedia.fr", 0.5), "eb@mmedia.fr : liste des dossiers");
        assert_eq!(
            phrase(&a(Tache::Messages, "INBOX", 1000, 13000, 0, 0), "eb@mmedia.fr", 10.0),
            "eb@mmedia.fr › Boîte de réception : en-têtes 1\u{202f}000 / 13\u{202f}000, environ 2 min"
        );
        assert_eq!(
            phrase(&a(Tache::Messages, "Archives/2025", 0, 0, 0, 0), "a@b.fr", 0.2),
            "a@b.fr › 2025 : nouveaux messages"
        );
        assert_eq!(
            phrase(&a(Tache::Contenus, "Envoy&AOk-s", 3, 9, 4 * 1024 * 1024, 16 * 1024 * 1024), "a@b.fr", 4.0),
            "a@b.fr › Envoyés : mise sur le poste, 6 messages restants (12 Mo), environ 15 s"
        );
    }

    #[test]
    fn une_tache_par_compte_effacee_a_sa_fin() {
        // Comptes propres à ce test : la liste est commune au processus.
        let fin = commencer(-71, Tache::Messages, "INBOX", 0, 0);
        poser_total(-71, 40);
        avancer(-71, 10, 0);
        let vue = |c| en_cours().into_iter().find(|a| a.compte == c);
        assert_eq!(vue(-71).map(|a| (a.faits, a.total)), Some((10, 40)));
        // Une nouvelle tâche du même compte remplace la précédente.
        let seconde = commencer(-71, Tache::Contenus, "INBOX", 9, 0);
        assert_eq!(en_cours().iter().filter(|a| a.compte == -71).count(), 1);
        assert_eq!(vue(-71).map(|a| a.tache), Some(Tache::Contenus));
        drop(seconde);
        assert!(vue(-71).is_none());
        // La première, abandonnée après coup, n'efface pas celle qui l'a
        // remplacée.
        let troisieme = commencer(-71, Tache::Dossiers, "", 0, 0);
        drop(fin);
        assert_eq!(vue(-71).map(|a| a.tache), Some(Tache::Dossiers));
        drop(troisieme);
        assert!(vue(-71).is_none());
    }
}
