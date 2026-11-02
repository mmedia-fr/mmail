// SPDX-License-Identifier: GPL-3.0-or-later
//! Correcteur d'orthographe de la rédaction (0.6.36). Les mots tapés sont
//! comparés au dictionnaire français de Grammalecte (version 7.7, MPL 2.0 —
//! celui de LibreOffice et de Firefox, `dictionnaires/`), embarqué dans MMail
//! et lu par `spellbook` (format de Hunspell) : rien à installer sur le poste,
//! rien ne le quitte, et le même résultat sous Windows, Linux et Android.
//!
//! Le soulignement lui-même est l'affaire de l'interface (cf.
//! `cpp/correcteur.h`) : elle donne le texte d'un paragraphe et reçoit les
//! mots fautifs, en positions UTF-16, celles de Qt.

use std::collections::HashSet;
use std::sync::OnceLock;

use spellbook::Dictionary;

const AFF: &str = include_str!("../dictionnaires/fr.aff");
const DIC: &str = include_str!("../dictionnaires/fr.dic");

static DICTIONNAIRE: OnceLock<Option<Dictionary>> = OnceLock::new();

/// Le dictionnaire, chargé au premier appel ; `None` s'il est illisible.
pub fn dictionnaire() -> Option<&'static Dictionary> {
    DICTIONNAIRE.get_or_init(|| Dictionary::new(AFF, DIC).ok()).as_ref()
}

/// Charge le dictionnaire dans un fil à part : la première frappe n'attend
/// pas sa lecture.
pub fn precharger() {
    let _ = std::thread::Builder::new().name("mmail-orthographe".into()).spawn(|| {
        dictionnaire();
    });
}

/// Mots élidés du français : « l'école », « qu'il » — l'apostrophe sépare deux
/// mots, vérifiés chacun.
const ELISIONS: &[&str] = &["l", "d", "j", "m", "n", "s", "t", "c", "ç", "qu", "jusqu", "lorsqu", "puisqu", "quoiqu"];

/// Apostrophe droite : celle du dictionnaire.
fn droite(mot: &str) -> String {
    mot.replace(['’', 'ʼ'], "'")
}

/// Vrai si le mot s'écrit ainsi, d'après le dictionnaire et les mots ajoutés
/// par l'utilisateur (en minuscules).
pub fn correct(dict: &Dictionary, mot: &str, ajoutes: &HashSet<String>) -> bool {
    let mot = droite(mot);
    if dict.check(&mot) || ajoutes.contains(&mot.to_lowercase()) {
        return true;
    }
    if let Some((tete, reste)) = mot.split_once('\'') {
        if ELISIONS.contains(&tete.to_lowercase().as_str()) && !reste.is_empty() {
            return correct(dict, reste, ajoutes);
        }
    }
    // « dit-il », « celui-ci » : chaque partie, si le mot entier n'y est pas.
    if mot.contains('-') {
        return mot.split('-').all(|partie| partie.is_empty() || correct(dict, partie, ajoutes));
    }
    false
}

/// Un mot à vérifier : ni sigle (« SARL », « RDS »), ni chiffre, ni mot d'une
/// lettre hors du dictionnaire courant.
fn a_verifier(mot: &str) -> bool {
    let lettres = mot.chars().filter(|c| c.is_alphabetic()).count();
    if lettres == 0 || mot.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let majuscules = mot.chars().filter(|c| c.is_uppercase()).count();
    !(lettres >= 2 && majuscules == lettres)
}

/// Caractère d'un mot : lettre, chiffre, apostrophe ou trait d'union (pris
/// entre deux lettres seulement, cf. `mots`).
fn car_mot(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '’' | 'ʼ' | '-')
}

/// Mots d'un texte, avec leur début et leur longueur en unités UTF-16. Un
/// fragment d'adresse (courriel, lien, chemin) ne compte pas : le mot qui
/// touche « @ », « / », « \\ », « : // », « . » suivi d'une lettre, ou « _ »,
/// est écarté.
pub fn mots(texte: &str) -> Vec<(usize, usize, String)> {
    let caracteres: Vec<char> = texte.chars().collect();
    let mut sortie = Vec::new();
    let mut i = 0;
    let mut position = 0; // en UTF-16
    while i < caracteres.len() {
        if !car_mot(caracteres[i]) {
            position += caracteres[i].len_utf16();
            i += 1;
            continue;
        }
        let debut = i;
        let debut_position = position;
        while i < caracteres.len() && car_mot(caracteres[i]) {
            position += caracteres[i].len_utf16();
            i += 1;
        }
        // Apostrophes et traits d'union de bord : hors du mot.
        let mut a = debut;
        let mut b = i;
        while a < b && !caracteres[a].is_alphanumeric() {
            a += 1;
        }
        while b > a && !caracteres[b - 1].is_alphanumeric() {
            b -= 1;
        }
        if a == b {
            continue;
        }
        let avant = debut.checked_sub(1).map(|k| caracteres[k]);
        let apres = caracteres.get(i).copied();
        let lettre_apres_point = apres == Some('.') && caracteres.get(i + 1).is_some_and(|c| c.is_alphanumeric());
        let adresse = matches!(avant, Some('@' | '/' | '\\' | '_' | '.'))
            || matches!(apres, Some('@' | '/' | '\\' | '_'))
            || lettre_apres_point
            || (apres == Some(':') && caracteres.get(i + 1) == Some(&'/'));
        if adresse {
            continue;
        }
        let mot: String = caracteres[a..b].iter().collect();
        let decalage: usize = caracteres[debut..a].iter().map(|c| c.len_utf16()).sum();
        let longueur: usize = caracteres[a..b].iter().map(|c| c.len_utf16()).sum();
        sortie.push((debut_position + decalage, longueur, mot));
    }
    sortie
}

/// Mots fautifs d'un texte : début et longueur en unités UTF-16.
pub fn fautes(texte: &str, ajoutes: &HashSet<String>) -> Vec<(usize, usize)> {
    let Some(dict) = dictionnaire() else {
        return Vec::new();
    };
    mots(texte)
        .into_iter()
        .filter(|(_, _, mot)| a_verifier(mot) && !correct(dict, mot, ajoutes))
        .map(|(debut, longueur, _)| (debut, longueur))
        .collect()
}

/// Suggestions pour un mot fautif, six au plus, avec l'apostrophe du mot
/// tapé.
pub fn suggestions(mot: &str) -> Vec<String> {
    let Some(dict) = dictionnaire() else {
        return Vec::new();
    };
    let courbe = mot.contains('’');
    let mot = droite(mot);
    let mut proposees = Vec::new();
    // « l'ecole » : les suggestions du mot élidé, derrière l'élision tapée.
    match mot.split_once('\'') {
        Some((tete, reste)) if ELISIONS.contains(&tete.to_lowercase().as_str()) && !reste.is_empty() => {
            dict.suggest(reste, &mut proposees);
            proposees = proposees.into_iter().map(|s| format!("{tete}'{s}")).collect();
        }
        _ => dict.suggest(&mot, &mut proposees),
    }
    proposees
        .into_iter()
        .map(|s| if courbe { s.replace('\'', "’") } else { s })
        .take(6)
        .collect()
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type Orthographe = super::OrthographeRust;

        /// Mots fautifs d'un paragraphe : « début:longueur » séparés par des
        /// espaces, en unités UTF-16.
        #[qinvokable]
        fn fautes(&self, texte: &QString) -> QString;

        /// Suggestions pour un mot, en JSON (liste de chaînes).
        #[qinvokable]
        fn suggestions(&self, mot: &QString) -> QString;

        /// Mots ajoutés au dictionnaire par l'utilisateur, un par ligne.
        #[qinvokable]
        #[cxx_name = "poserAjoutes"]
        fn poser_ajoutes(self: Pin<&mut Orthographe>, mots: &QString);

        /// Lit le dictionnaire dans un fil à part.
        #[qinvokable]
        fn precharger(&self);
    }
}

use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct OrthographeRust {
    ajoutes: HashSet<String>,
}

impl qobject::Orthographe {
    pub fn fautes(&self, texte: &QString) -> QString {
        let liste: Vec<String> =
            fautes(&texte.to_string(), &self.ajoutes).into_iter().map(|(d, l)| format!("{d}:{l}")).collect();
        QString::from(&liste.join(" "))
    }

    pub fn suggestions(&self, mot: &QString) -> QString {
        QString::from(&serde_json::to_string(&suggestions(&mot.to_string())).unwrap_or_else(|_| "[]".into()))
    }

    pub fn poser_ajoutes(mut self: Pin<&mut Self>, mots: &QString) {
        let ajoutes: HashSet<String> =
            mots.to_string().lines().map(|m| droite(m.trim()).to_lowercase()).filter(|m| !m.is_empty()).collect();
        self.as_mut().rust_mut().ajoutes = ajoutes;
    }

    pub fn precharger(&self) {
        precharger();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rien() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn dictionnaire_francais() {
        let dict = dictionnaire().expect("dictionnaire lisible");
        for mot in [
            "école", "maisons", "mangeaient", "été", "à", "aujourd'hui", "aujourd’hui", "l'école", "l’école", "qu'il",
            "jusqu'à", "c'est", "peut-être", "rendez-vous", "dit-il", "Cordialement", "Bonjour", "Paris", "facturé",
        ] {
            assert!(correct(dict, mot, &rien()), "« {mot} » devrait être juste");
        }
        for mot in ["ecolle", "maisonn", "rendez-vou", "facturationn", "mangaient"] {
            assert!(!correct(dict, mot, &rien()), "« {mot} » devrait être faux");
        }
        let mut ajoutes = HashSet::new();
        ajoutes.insert("mmedia".to_string());
        assert!(correct(dict, "Mmedia", &ajoutes));
    }

    #[test]
    fn fautes_en_positions_utf16() {
        // « é » compte pour une unité, « 😀 » pour deux.
        let texte = "Bonjour 😀, je vous envoie la factrue de l’école — SARL MMEDIA, dossier 2026-01, voir www.exemple.fr ou jean@exemple.fr.";
        let fautes = fautes(texte, &rien());
        let utf16: Vec<u16> = texte.encode_utf16().collect();
        let vus: Vec<String> = fautes.iter().map(|(d, l)| String::from_utf16_lossy(&utf16[*d..d + l])).collect();
        assert_eq!(vus, vec!["factrue"]);
    }

    #[test]
    fn mots_et_adresses() {
        let mots: Vec<String> = mots("l'école, « dit-il » ; '1er' https://x.fr/chemin a_b c@d.fr fin.")
            .into_iter()
            .map(|(_, _, m)| m)
            .collect();
        // « https » touche « :// », les autres morceaux d'adresse « / », « _ »,
        // « @ » ou un point suivi d'une lettre.
        assert_eq!(mots, vec!["l'école", "dit-il", "1er", "fin"]);
    }

    #[test]
    fn suggestions_pour_un_mot_fautif() {
        let s = suggestions("factrue");
        assert!(s.iter().any(|m| m == "facture"), "{s:?}");
        let s = suggestions("l’ecole");
        assert!(s.iter().any(|m| m == "l’école"), "{s:?}");
    }
}
