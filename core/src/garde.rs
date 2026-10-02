// SPDX-License-Identifier: GPL-3.0-or-later
//! Rédactions en cours gardées sur le poste.
//!
//! Une rédaction n'atteint le serveur qu'à l'enregistrement du brouillon. Si
//! MMail se ferme avant — mise à jour, arrêt du poste, plantage —, ce qui a
//! été tapé depuis serait perdu. La fenêtre de rédaction en écrit donc l'état
//! ici à intervalles courts, et MMail propose de la rouvrir au démarrage
//! suivant. Le dossier d'une rédaction disparaît dès qu'elle trouve une issue :
//! envoyée, enregistrée sur le serveur, abandonnée.
//!
//! Disposition : `<profil>/redactions/<id>/etat.json`, et sous `pieces/` la
//! copie des pièces jointes qui vivaient dans un dossier temporaire du profil
//! (pièces d'un message transféré, d'un brouillon repris), vidé à chaque
//! démarrage.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;

pub struct Garde {
    racine: PathBuf,
}

impl Garde {
    /// Dossier des rédactions gardées, à côté de l'index.
    pub fn du_profil(profil: &str) -> Self {
        let racine =
            Path::new(profil).parent().map(|p| p.join("redactions")).unwrap_or_else(|| PathBuf::from("redactions"));
        Garde { racine }
    }

    /// Écrit l'état d'une rédaction : du JSON dont `contenu.pieces` liste les
    /// chemins des fichiers joints. Ceux qui vivent sous `temporaire` sont
    /// copiés à côté, et l'état écrit désigne les copies.
    pub fn garder(&self, id: &str, etat: &str, temporaire: &Path) -> Result<(), String> {
        let dossier = self.dossier(id)?;
        let mut etat: Value = serde_json::from_str(etat).map_err(|e| format!("état illisible : {e}"))?;
        std::fs::create_dir_all(&dossier).map_err(|e| format!("{} : {e}", dossier.display()))?;
        if let Some(pieces) = etat.pointer_mut("/contenu/pieces").and_then(Value::as_array_mut) {
            for (rang, piece) in pieces.iter_mut().enumerate() {
                let Some(chemin) = piece.as_str().map(PathBuf::from) else { continue };
                if !chemin.starts_with(temporaire) {
                    continue;
                }
                let Some(nom) = chemin.file_name() else { continue };
                let copie = dossier.join("pieces").join(rang.to_string()).join(nom);
                let deja = std::fs::metadata(&copie).ok().map(|m| m.len());
                if deja.is_none() || deja != std::fs::metadata(&chemin).ok().map(|m| m.len()) {
                    if let Some(parent) = copie.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| format!("{} : {e}", parent.display()))?;
                    }
                    std::fs::copy(&chemin, &copie).map_err(|e| format!("{} : {e}", chemin.display()))?;
                }
                *piece = Value::String(copie.to_string_lossy().into_owned());
            }
        }
        // Écrit à part puis renommé : une coupure en pleine écriture laisse
        // l'état précédent intact.
        let fichier = dossier.join("etat.json");
        let partiel = dossier.join("etat.json.partiel");
        std::fs::write(&partiel, etat.to_string()).map_err(|e| format!("{} : {e}", partiel.display()))?;
        std::fs::rename(&partiel, &fichier).map_err(|e| format!("{} : {e}", fichier.display()))
    }

    /// Retire une rédaction gardée, copies de pièces comprises.
    pub fn oublier(&self, id: &str) {
        if let Ok(dossier) = self.dossier(id) {
            let _ = std::fs::remove_dir_all(dossier);
        }
    }

    /// Les rédactions gardées, de la plus ancienne à la plus récente : leur
    /// état, plus `id` et `modifie` (secondes Unix de la dernière écriture).
    pub fn lister(&self) -> Vec<Value> {
        let mut liste: Vec<(u64, Value)> = Vec::new();
        let Ok(entrees) = std::fs::read_dir(&self.racine) else {
            return Vec::new();
        };
        for entree in entrees.flatten() {
            let id = entree.file_name().to_string_lossy().into_owned();
            let fichier = entree.path().join("etat.json");
            let Some(mut etat) = std::fs::read_to_string(&fichier)
                .ok()
                .and_then(|texte| serde_json::from_str::<Value>(&texte).ok())
                .filter(Value::is_object)
            else {
                continue;
            };
            let modifie = std::fs::metadata(&fichier)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            etat["id"] = Value::String(id);
            etat["modifie"] = Value::from(modifie);
            liste.push((modifie, etat));
        }
        liste.sort_by_key(|(modifie, _)| *modifie);
        liste.into_iter().map(|(_, etat)| etat).collect()
    }

    /// Dossier d'une rédaction ; l'identifiant vient de la fenêtre de
    /// rédaction, il ne doit désigner rien d'autre qu'un sous-dossier.
    fn dossier(&self, id: &str) -> Result<PathBuf, String> {
        let valide = !id.is_empty()
            && id.len() <= 64
            && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if valide {
            Ok(self.racine.join(id))
        } else {
            Err(format!("identifiant de rédaction invalide : {id:?}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profil(nom: &str) -> (PathBuf, String) {
        let racine = std::env::temp_dir().join(format!("mmail-garde-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&racine);
        std::fs::create_dir_all(&racine).unwrap();
        let index = racine.join("index.sqlite").to_string_lossy().into_owned();
        (racine, index)
    }

    #[test]
    fn garder_lister_oublier() {
        let (racine, index) = profil("cycle");
        let garde = Garde::du_profil(&index);
        assert!(garde.lister().is_empty());
        garde.garder("r-1", r#"{"compte":2,"contenu":{"objet":"Devis","pieces":[]}}"#, &racine.join("tmp")).unwrap();
        // Une seconde écriture remplace la première.
        garde.garder("r-1", r#"{"compte":2,"contenu":{"objet":"Devis signé","pieces":[]}}"#, &racine.join("tmp")).unwrap();
        let liste = garde.lister();
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0]["id"], "r-1");
        assert_eq!(liste[0]["compte"], 2);
        assert_eq!(liste[0]["contenu"]["objet"], "Devis signé");
        assert!(liste[0]["modifie"].as_u64().unwrap() > 0);
        assert!(!racine.join("redactions/r-1/etat.json.partiel").exists());
        garde.oublier("r-1");
        assert!(garde.lister().is_empty());
        assert!(!racine.join("redactions/r-1").exists());
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn pieces_temporaires_copiees() {
        let (racine, index) = profil("pieces");
        let temporaire = racine.join("pieces-jointes");
        let transferee = temporaire.join("redaction-1-7-0/2/facture.pdf");
        std::fs::create_dir_all(transferee.parent().unwrap()).unwrap();
        std::fs::write(&transferee, b"%PDF").unwrap();
        let personnelle = racine.join("devis.odt");
        std::fs::write(&personnelle, b"odt").unwrap();
        let etat = serde_json::json!({"compte": 1, "contenu": {"pieces": [transferee, personnelle]}});
        let garde = Garde::du_profil(&index);
        garde.garder("r-2", &etat.to_string(), &temporaire).unwrap();
        // Le dossier temporaire est vidé au démarrage : la copie demeure.
        std::fs::remove_dir_all(&temporaire).unwrap();
        let liste = garde.lister();
        let pieces = liste[0]["contenu"]["pieces"].as_array().unwrap();
        let copie = PathBuf::from(pieces[0].as_str().unwrap());
        assert!(copie.starts_with(racine.join("redactions/r-2/pieces")));
        assert_eq!(std::fs::read(&copie).unwrap(), b"%PDF");
        // Un fichier hors du dossier temporaire reste désigné tel quel.
        assert_eq!(pieces[1].as_str().unwrap(), personnelle.to_string_lossy());
        let _ = std::fs::remove_dir_all(racine);
    }

    #[test]
    fn identifiant_borne_au_dossier() {
        let (racine, index) = profil("id");
        let garde = Garde::du_profil(&index);
        for id in ["", "../index", "a/b", "a\\b", ".."] {
            assert!(garde.garder(id, r#"{"contenu":{}}"#, &racine).is_err(), "{id}");
        }
        assert!(garde.garder("r1", "pas du json", &racine).is_err());
        let _ = std::fs::remove_dir_all(racine);
    }
}
