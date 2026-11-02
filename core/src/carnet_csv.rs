// SPDX-License-Identifier: GPL-3.0-or-later
//! Import d'un carnet d'adresses exporté par Outlook en CSV (0.6.35) :
//! « Fichier › Ouvrir et exporter › Importer/Exporter › Exporter des données
//! vers un fichier › Valeurs séparées par une virgule ». Outlook nomme ses
//! colonnes dans la langue de son interface (« Prénom », « Adresse de
//! messagerie », « Tél. mobile »… ou « First Name », « E-mail Address »,
//! « Mobile Phone »…) et écrit le fichier dans le jeu de caractères du poste
//! (Windows-1252 en français), parfois en UTF-8 ; le séparateur est la
//! virgule, ou le point-virgule selon les réglages régionaux. Les colonnes se
//! reconnaissent à leur nom, quelle que soit leur place ; celles que le
//! formulaire d'un contact n'a pas sont laissées.

use crate::fiche::{Adresse, Coordonnee, Fiche};

/// Texte d'un fichier : UTF-8 (avec ou sans marque d'ordre), UTF-16 marqué,
/// à défaut Windows-1252.
pub fn texte(octets: &[u8]) -> String {
    if let Some(reste) = octets.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(reste).into_owned();
    }
    if let Some(reste) = octets.strip_prefix(&[0xFF, 0xFE]) {
        let unites: Vec<u16> = reste.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
        return String::from_utf16_lossy(&unites);
    }
    if let Some(reste) = octets.strip_prefix(&[0xFE, 0xFF]) {
        let unites: Vec<u16> = reste.chunks_exact(2).map(|p| u16::from_be_bytes([p[0], p[1]])).collect();
        return String::from_utf16_lossy(&unites);
    }
    match std::str::from_utf8(octets) {
        Ok(t) => t.to_string(),
        Err(_) => octets.iter().map(|&o| windows_1252(o)).collect(),
    }
}

/// Caractère d'un octet en Windows-1252 : Latin-1, sauf de 0x80 à 0x9F.
fn windows_1252(octet: u8) -> char {
    const HAUT: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘',
        '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match octet {
        0x80..=0x9F => HAUT[(octet - 0x80) as usize],
        _ => octet as char,
    }
}

/// Séparateur des champs, lu sur la ligne des noms de colonnes : celui des
/// trois (virgule, point-virgule, tabulation) qui y revient le plus, hors
/// des guillemets.
fn separateur(texte: &str) -> char {
    let mut comptes = [(',', 0usize), (';', 0), ('\t', 0)];
    let mut guillemets = false;
    for c in texte.chars() {
        match c {
            '"' => guillemets = !guillemets,
            '\n' if !guillemets => break,
            _ if !guillemets => {
                if let Some(n) = comptes.iter_mut().find(|(s, _)| *s == c) {
                    n.1 += 1;
                }
            }
            _ => {}
        }
    }
    comptes.iter().max_by_key(|(_, n)| *n).map_or(',', |(s, _)| *s)
}

/// Lignes et champs d'un texte CSV (RFC 4180) : champs entre guillemets,
/// guillemets doublés, retours à la ligne dans un champ.
fn lignes(texte: &str, separateur: char) -> Vec<Vec<String>> {
    let mut lignes = Vec::new();
    let mut ligne = Vec::new();
    let mut champ = String::new();
    let mut guillemets = false;
    let mut caracteres = texte.chars().peekable();
    while let Some(c) = caracteres.next() {
        if guillemets {
            match c {
                '"' if caracteres.peek() == Some(&'"') => {
                    champ.push('"');
                    caracteres.next();
                }
                '"' => guillemets = false,
                '\r' => {}
                _ => champ.push(c),
            }
        } else if c == '"' {
            guillemets = true;
        } else if c == separateur {
            ligne.push(std::mem::take(&mut champ));
        } else if c == '\n' {
            ligne.push(std::mem::take(&mut champ));
            lignes.push(std::mem::take(&mut ligne));
        } else if c != '\r' {
            champ.push(c);
        }
    }
    if !champ.is_empty() || !ligne.is_empty() {
        ligne.push(champ);
        lignes.push(ligne);
    }
    lignes
}

/// Nom de colonne ramené à des minuscules sans accents, blancs réduits.
fn plier(nom: &str) -> String {
    let mut plie = String::with_capacity(nom.len());
    for c in nom.trim().to_lowercase().chars() {
        plie.push(match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            '\u{a0}' => ' ',
            _ => c,
        });
    }
    plie.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Ce que porte une colonne, d'après son nom.
#[derive(Debug, Clone, PartialEq)]
enum Colonne {
    Prenom,
    Nom,
    NomAffiche,
    Organisation,
    Fonction,
    Note,
    Courriel,
    /// Genre du numéro : « travail », « domicile », « portable », « fax »
    /// ou « autre ».
    Telephone(&'static str),
    /// Genre de l'adresse, puis la rubrique : « rue », « codePostal »,
    /// « ville » ou « pays ».
    Adresse(&'static str, &'static str),
    Ignoree,
}

fn genre_lieu(nom: &str) -> Option<&'static str> {
    if ["bureau", "business", "work", "professionnel"].iter().any(|m| nom.contains(m)) {
        Some("travail")
    } else if ["domicile", "home", "personnel"].iter().any(|m| nom.contains(m)) {
        Some("domicile")
    } else if ["autre", "other"].iter().any(|m| nom.contains(m)) {
        Some("autre")
    } else {
        None
    }
}

fn colonne(nom: &str) -> Colonne {
    let nom = plier(nom);
    let est = |noms: &[&str]| noms.contains(&nom.as_str());
    if nom.contains("type") {
        return Colonne::Ignoree;
    }
    if est(&["prenom", "first name", "given name"]) {
        return Colonne::Prenom;
    }
    if est(&["nom", "last name", "family name", "surname", "nom de famille"]) {
        return Colonne::Nom;
    }
    if est(&["name", "nom complet", "full name", "display name", "nom affiche"]) {
        return Colonne::NomAffiche;
    }
    if est(&["societe", "company", "organisation", "organization", "entreprise", "organization 1 - name"]) {
        return Colonne::Organisation;
    }
    if est(&["fonction", "job title", "titre du poste", "organization 1 - title"]) {
        return Colonne::Fonction;
    }
    if est(&["notes", "note"]) {
        return Colonne::Note;
    }
    // « Nom complet de l'adresse de messagerie », « E-mail Display Name » :
    // le nom, pas l'adresse.
    let courriel = nom.starts_with("adresse de messagerie")
        || nom.contains("e-mail")
        || nom.contains("email")
        || nom.contains("courriel");
    if courriel && !nom.contains("nom") && !nom.contains("name") {
        return Colonne::Courriel;
    }
    let telephone = ["telephone", "tel.", "tel ", "phone", "mobile", "portable", "telecopie", "fax"]
        .iter()
        .any(|m| nom.contains(m))
        || nom == "tel";
    if telephone {
        let genre = if ["mobile", "portable", "cell"].iter().any(|m| nom.contains(m)) {
            "portable"
        } else if ["telecopie", "fax"].iter().any(|m| nom.contains(m)) {
            "fax"
        } else {
            genre_lieu(&nom).unwrap_or("autre")
        };
        return Colonne::Telephone(genre);
    }
    let rubrique = if nom.starts_with("rue") || nom.contains("street") {
        Some("rue")
    } else if nom.starts_with("code postal") || nom.contains("postal code") || nom.contains("zip") {
        Some("codePostal")
    } else if nom.starts_with("ville") || nom.contains("city") {
        Some("ville")
    } else if nom.starts_with("pays") || nom.contains("country") {
        Some("pays")
    } else {
        None
    };
    match (rubrique, genre_lieu(&nom)) {
        (Some(r), Some(g)) => Colonne::Adresse(g, r),
        _ => Colonne::Ignoree,
    }
}

/// Bilan d'une lecture : les fiches retenues, et les lignes écartées faute
/// de nom, d'adresse de courriel ou de numéro.
#[derive(Debug, Default)]
pub struct Lecture {
    pub fiches: Vec<Fiche>,
    pub vides: usize,
}

/// Fiches de contact d'un fichier CSV d'Outlook (octets tels que lus). Rend
/// un motif si le fichier n'a ni nom, ni adresse de courriel, ni numéro
/// reconnus en tête.
pub fn lire(octets: &[u8]) -> Result<Lecture, String> {
    let texte = texte(octets);
    let separateur = separateur(&texte);
    let mut lignes = lignes(&texte, separateur).into_iter();
    let colonnes: Vec<Colonne> = lignes.next().unwrap_or_default().iter().map(|n| colonne(n)).collect();
    let utiles = colonnes.iter().any(|c| {
        matches!(c, Colonne::Prenom | Colonne::Nom | Colonne::NomAffiche | Colonne::Courriel | Colonne::Telephone(_))
    });
    if !utiles {
        return Err("ce fichier n'a pas les colonnes d'un carnet d'adresses (nom, adresse de messagerie, téléphone)".into());
    }
    let mut lecture = Lecture::default();
    for champs in lignes {
        if champs.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let mut fiche = Fiche::default();
        // Adresses par genre, dans l'ordre où leurs colonnes viennent.
        let mut adresses: Vec<Adresse> = Vec::new();
        for (c, valeur) in colonnes.iter().zip(champs.iter()) {
            let valeur = valeur.trim();
            if valeur.is_empty() {
                continue;
            }
            match c {
                Colonne::Prenom => fiche.prenom = valeur.to_string(),
                Colonne::Nom => fiche.nom = valeur.to_string(),
                Colonne::NomAffiche => fiche.nom_affiche = valeur.to_string(),
                Colonne::Organisation => fiche.organisation = valeur.to_string(),
                Colonne::Fonction => fiche.fonction = valeur.to_string(),
                Colonne::Note => fiche.note = valeur.to_string(),
                Colonne::Courriel => {
                    let deja = fiche.courriels.iter().any(|c| c.valeur.eq_ignore_ascii_case(valeur));
                    if valeur.contains('@') && !valeur.contains(char::is_whitespace) && !deja {
                        let genre = if fiche.courriels.is_empty() { "travail" } else { "autre" };
                        fiche.courriels.push(Coordonnee { genre: genre.into(), valeur: valeur.to_string(), cle: None });
                    }
                }
                Colonne::Telephone(genre) => {
                    if !fiche.telephones.iter().any(|t| t.valeur == valeur) {
                        fiche.telephones.push(Coordonnee { genre: genre.to_string(), valeur: valeur.to_string(), cle: None });
                    }
                }
                Colonne::Adresse(genre, rubrique) => {
                    let a = match adresses.iter().position(|a| a.genre == *genre) {
                        Some(i) => &mut adresses[i],
                        None => {
                            adresses.push(Adresse { genre: genre.to_string(), ..Default::default() });
                            adresses.last_mut().expect("ajoutée")
                        }
                    };
                    let champ = match *rubrique {
                        "rue" => &mut a.rue,
                        "codePostal" => &mut a.code_postal,
                        "ville" => &mut a.ville,
                        _ => &mut a.pays,
                    };
                    // « Rue (bureau) 2 », « 3 » : à la ligne.
                    if !champ.is_empty() {
                        champ.push('\n');
                    }
                    champ.push_str(valeur);
                }
                Colonne::Ignoree => {}
            }
        }
        // L'adresse du bureau d'abord, comme Outlook la montre.
        for genre in ["travail", "domicile", "autre"] {
            if let Some(a) = adresses.iter().find(|a| a.genre == genre) {
                fiche.adresse = a.clone();
                break;
            }
        }
        if fiche.nom_affiche.is_empty() {
            let complet = format!("{} {}", fiche.prenom, fiche.nom).trim().to_string();
            fiche.nom_affiche = if !complet.is_empty() {
                complet
            } else if !fiche.organisation.is_empty() {
                fiche.organisation.clone()
            } else {
                fiche.courriels.first().map(|c| c.valeur.clone()).unwrap_or_default()
            };
        }
        if fiche.nom_affiche.is_empty() && fiche.telephones.is_empty() {
            lecture.vides += 1;
            continue;
        }
        lecture.fiches.push(fiche);
    }
    Ok(lecture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_d_outlook_en_francais_windows_1252() {
        // En-tête d'Outlook en français (extrait), points-virgules, accents
        // en Windows-1252 : « é » = 0xE9.
        let mut octets: Vec<u8> = Vec::new();
        octets.extend_from_slice(
            b"\"Titre\";\"Pr\xe9nom\";\"Nom\";\"Soci\xe9t\xe9\";\"Fonction\";\"Rue (bureau)\";\"Rue (bureau) 2\";\
              \"Ville (bureau)\";\"Code postal (bureau)\";\"Pays/R\xe9gion (bureau)\";\"T\xe9l\xe9copie (bureau)\";\
              \"T\xe9l\xe9phone (bureau)\";\"T\xe9l. mobile\";\"Adresse de messagerie\";\"Type de messagerie\";\
              \"Nom complet de l'adresse de messagerie\";\"Adresse de messagerie 2\";\"Notes\";\"R\xe9cepteur de radiomessagerie\"\r\n",
        );
        octets.extend_from_slice(
            b"\"M.\";\"Ren\xe9\";\"Dupont\";\"Exemple SARL\";\"G\xe9rant\";\"12 rue des Essais\";\"B\xe2timent B\";\
              \"Agen\";\"47000\";\"France\";\"05 53 00 00 01\";\"05 53 00 00 00\";\"06 00 00 00 00\";\
              \"rene.dupont@exemple.fr\";\"SMTP\";\"Ren\xe9 Dupont (rene.dupont@exemple.fr)\";\"rene@perso.exemple\";\
              \"Client depuis 2020;\r\nappeler le matin\";\"\"\r\n",
        );
        octets.extend_from_slice(b";;;;;;;;;;;;;;;;;;\r\n");
        octets.extend_from_slice(b"\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\";\"\"\r\n");
        let lecture = lire(&octets).unwrap();
        assert_eq!(lecture.fiches.len(), 1);
        let f = &lecture.fiches[0];
        assert_eq!((f.prenom.as_str(), f.nom.as_str(), f.nom_affiche.as_str()), ("René", "Dupont", "René Dupont"));
        assert_eq!((f.organisation.as_str(), f.fonction.as_str()), ("Exemple SARL", "Gérant"));
        let courriels: Vec<(&str, &str)> = f.courriels.iter().map(|c| (c.genre.as_str(), c.valeur.as_str())).collect();
        assert_eq!(courriels, vec![("travail", "rene.dupont@exemple.fr"), ("autre", "rene@perso.exemple")]);
        let telephones: Vec<(&str, &str)> = f.telephones.iter().map(|c| (c.genre.as_str(), c.valeur.as_str())).collect();
        assert_eq!(telephones, vec![("fax", "05 53 00 00 01"), ("travail", "05 53 00 00 00"), ("portable", "06 00 00 00 00")]);
        assert_eq!(f.adresse.genre, "travail");
        assert_eq!(f.adresse.rue, "12 rue des Essais\nBâtiment B");
        assert_eq!((f.adresse.code_postal.as_str(), f.adresse.ville.as_str(), f.adresse.pays.as_str()), ("47000", "Agen", "France"));
        assert_eq!(f.note, "Client depuis 2020;\nappeler le matin");
    }

    #[test]
    fn export_en_anglais_utf8_et_lignes_sans_rien() {
        let csv = "\u{feff}First Name,Last Name,Company,E-mail Address,E-mail Display Name,Mobile Phone,Home Street,Home City,Home Postal Code\n\
                   Marie,,,marie@exemple.fr,Marie (marie@exemple.fr),,5 avenue Exemple,Bordeaux,33000\n\
                   ,,Boulangerie Exemple,,,05 56 00 00 00,,,\n\
                   ,,,,,,,,\n\
                   ,,,pas une adresse,,,,,\n";
        let lecture = lire(csv.as_bytes()).unwrap();
        assert_eq!(lecture.fiches.len(), 2);
        assert_eq!(lecture.vides, 1, "la ligne sans nom ni adresse valable ni numéro");
        let marie = &lecture.fiches[0];
        assert_eq!(marie.nom_affiche, "Marie");
        assert_eq!(marie.courriels.len(), 1);
        assert_eq!((marie.adresse.genre.as_str(), marie.adresse.ville.as_str()), ("domicile", "Bordeaux"));
        let boulangerie = &lecture.fiches[1];
        assert_eq!(boulangerie.nom_affiche, "Boulangerie Exemple");
        assert_eq!(boulangerie.telephones[0].genre, "portable");
    }

    #[test]
    fn fichier_qui_n_est_pas_un_carnet() {
        assert!(lire(b"Date,Montant,Libelle\n2026-11-02,12,Essai\n").is_err());
        assert!(lire(b"").is_err());
    }

    #[test]
    fn separateurs_et_guillemets() {
        assert_eq!(separateur("a;b;c\nd,e"), ';');
        assert_eq!(separateur("\"a;b\",c,d\n"), ',');
        assert_eq!(
            lignes("\"un, deux\",\"il dit \"\"oui\"\"\"\r\ntrois,\n", ','),
            vec![vec!["un, deux".to_string(), "il dit \"oui\"".to_string()], vec!["trois".to_string(), String::new()]]
        );
        assert_eq!(texte(&[0x80, 0x9C, 0xE9]), "€œé");
    }
}
