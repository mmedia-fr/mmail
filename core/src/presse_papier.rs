// SPDX-License-Identifier: GPL-3.0-or-later
//! Règle de la copie automatique de la sélection.
//!
//! Reprise de MMdedit, à l'identique. MMail place toute sélection faite dans un
//! message dans le presse-papier, à la façon de la sélection primaire de X11 —
//! absente de Windows. La difficulté n'est pas la
//! copie : c'est de ne pas écraser ce qu'un tiers vient d'y déposer. D'où une
//! protection de soixante secondes après tout dépôt étranger, et la
//! reconnaissance de notre propre écho.
//!
//! Le temps est fourni par l'appelant, en secondes : cette règle n'a pas
//! d'horloge, ce qui la rend vérifiable par des tests ordinaires.

/// Durée de protection d'un contenu déposé par un tiers, en secondes.
pub const PROTECTION_S: f64 = 60.0;

/// Fenêtre pendant laquelle un dépôt identique à ce que nous venons d'écrire
/// est reconnu comme notre propre écho.
///
/// Windows émet souvent plusieurs notifications pour une seule écriture
/// (notification système, plus les ouvertures du presse-papier par les
/// gestionnaires tiers) : un simple drapeau consommé par le premier signal
/// laissait le second passer pour un dépôt étranger, et le programme se protégeait
/// alors contre sa propre écriture pendant toute la durée de protection.
pub const ECHO_S: f64 = 1.0;

/// D'où vient le contenu courant du presse-papier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origine {
    Inconnue,
    Externe,
    /// Le message tel qu'il s'affiche.
    Message,
    /// Le message brut (bouton « Source »).
    Source,
}

impl Origine {
    pub fn cle(self) -> &'static str {
        match self {
            Origine::Inconnue => "inconnue",
            Origine::Externe => "externe",
            Origine::Message => "message",
            Origine::Source => "source",
        }
    }

    /// Libellé affiché en barre d'état.
    pub fn libelle(self) -> &'static str {
        match self {
            Origine::Inconnue => "\u{2014}",
            Origine::Externe => "extérieur",
            Origine::Message => "message",
            Origine::Source => "source",
        }
    }

    pub fn depuis_cle(cle: &str) -> Origine {
        match cle {
            "externe" => Origine::Externe,
            "message" => Origine::Message,
            "source" => Origine::Source,
            _ => Origine::Inconnue,
        }
    }
}

/// État de la règle : ce qui a été écrit, quand, et par qui.
#[derive(Debug, Clone)]
pub struct Regle {
    origine: Origine,
    /// Instant du dernier dépôt étranger : c'est lui que protège la règle.
    depot_protege: Option<f64>,
    /// Dernier texte écrit par nous, et l'instant de cette écriture.
    texte_interne: String,
    ecriture_interne: Option<f64>,
    /// Origine annoncée juste avant un Ctrl+C explicite, et son instant : la
    /// notification de dépôt arrive après coup, et de façon asynchrone.
    origine_attendue: Option<(Origine, f64)>,
}

impl Default for Regle {
    fn default() -> Self {
        Self {
            origine: Origine::Inconnue,
            depot_protege: None,
            texte_interne: String::new(),
            ecriture_interne: None,
            origine_attendue: None,
        }
    }
}

impl Regle {
    pub fn origine(&self) -> Origine {
        self.origine
    }

    /// La copie automatique peut-elle écrire, ou un contenu tiers est-il encore frais ?
    pub fn peut_ecraser(&self, maintenant: f64) -> bool {
        match self.depot_protege {
            None => true,
            Some(t) => maintenant - t >= PROTECTION_S,
        }
    }

    /// Secondes de protection restantes, 0 s'il n'y en a plus.
    pub fn restant(&self, maintenant: f64) -> i32 {
        match self.depot_protege {
            None => 0,
            Some(t) => {
                let reste = PROTECTION_S - (maintenant - t);
                if reste <= 0.0 {
                    0
                } else {
                    reste.ceil() as i32
                }
            }
        }
    }

    /// Annonce l'origine d'un Ctrl+C explicite, avant que le dépôt ne soit vu.
    pub fn annoncer(&mut self, origine: Origine, maintenant: f64) {
        self.origine_attendue = Some((origine, maintenant));
    }

    /// Enregistre ce que nous venons d'écrire nous-mêmes.
    pub fn ecriture_interne(&mut self, texte: &str, origine: Origine, maintenant: f64) {
        self.texte_interne = texte.to_string();
        self.ecriture_interne = Some(maintenant);
        self.origine = origine;
    }

    /// Prend acte d'un changement du presse-papier et rend l'origine retenue.
    ///
    /// Trois cas : c'est l'écho de notre propre écriture (rien ne change), c'est
    /// le dépôt d'un Ctrl+C que nous venons d'annoncer (origine annoncée, pas de
    /// protection : le contenu vient de nous), ou c'est un dépôt étranger — et
    /// il est alors protégé.
    pub fn depot(&mut self, texte: &str, maintenant: f64) -> Origine {
        if let Some(t) = self.ecriture_interne {
            if texte == self.texte_interne && maintenant - t < ECHO_S {
                return self.origine;
            }
        }
        if let Some((origine, t)) = self.origine_attendue {
            if maintenant - t < ECHO_S {
                self.origine_attendue = None;
                self.texte_interne = texte.to_string();
                self.ecriture_interne = Some(maintenant);
                self.origine = origine;
                return self.origine;
            }
        }
        self.origine = Origine::Externe;
        self.depot_protege = Some(maintenant);
        self.origine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depot_etranger_protege_une_minute() {
        let mut regle = Regle::default();
        assert!(regle.peut_ecraser(0.0));
        assert_eq!(regle.depot("venu d'ailleurs", 10.0), Origine::Externe);
        assert!(!regle.peut_ecraser(10.0));
        assert!(!regle.peut_ecraser(69.0));
        assert!(regle.peut_ecraser(70.0));
        assert_eq!(regle.restant(40.0), 30);
        assert_eq!(regle.restant(100.0), 0);
    }

    #[test]
    fn notre_propre_echo_ne_declenche_pas_la_protection() {
        let mut regle = Regle::default();
        regle.ecriture_interne("selection", Origine::Message, 5.0);
        // Windows peut notifier plusieurs fois la même écriture.
        assert_eq!(regle.depot("selection", 5.1), Origine::Message);
        assert_eq!(regle.depot("selection", 5.6), Origine::Message);
        assert!(regle.peut_ecraser(5.6));
    }

    #[test]
    fn un_echo_trop_tardif_est_un_depot_etranger() {
        // Au-delà de la fenêtre d'écho, le même texte revenu dans le presse-papier
        // vient forcément d'ailleurs : il est protégé.
        let mut regle = Regle::default();
        regle.ecriture_interne("selection", Origine::Message, 5.0);
        assert_eq!(regle.depot("selection", 7.0), Origine::Externe);
        assert!(!regle.peut_ecraser(7.0));
    }

    #[test]
    fn ctrl_c_annonce_garde_son_origine() {
        let mut regle = Regle::default();
        regle.annoncer(Origine::Source, 3.0);
        assert_eq!(regle.depot("phrase mise en forme", 3.2), Origine::Source);
        // Une copie venue de nous ne se protège pas contre nous-mêmes.
        assert!(regle.peut_ecraser(3.2));
        // L'annonce est consommée : le dépôt suivant est bien étranger.
        assert_eq!(regle.depot("autre chose", 4.0), Origine::Externe);
    }

    #[test]
    fn libelles() {
        assert_eq!(Origine::Externe.libelle(), "extérieur");
        assert_eq!(Origine::depuis_cle("source"), Origine::Source);
        assert_eq!(Origine::depuis_cle("n'importe quoi"), Origine::Inconnue);
    }
}
