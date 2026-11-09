// SPDX-License-Identifier: GPL-3.0-or-later
//! Signatures HTML avec images (décision 8 du dossier de projet).
//!
//! Une signature vit dans le profil : son HTML dans les réglages, ses images
//! dans un dossier `signatures/` à côté de l'index. C'est le seul dossier — avec
//! celui des brouillons repris — d'où une image part intégrée à un message :
//! un HTML piégé qui désignerait un fichier du poste ne le fera jamais partir.
//!
//! Elle s'écrit dans MMail, ou s'importe d'Outlook : le fichier `.htm` de
//! `%APPDATA%\Microsoft\Signatures`, avec le dossier d'images qui l'accompagne.

use std::path::{Path, PathBuf};

use crate::rendu::{assainir, extension_image, type_image, url_fichier, Image, REPERE_DISTANTE, REPERE_IMAGE};

/// Une image de signature au-delà de cette taille est refusée.
const TAILLE_MAX: u64 = 5 * 1024 * 1024;

/// Nom du dossier des images de signature d'une adresse : sans caractère
/// qu'un système de fichiers refuserait.
pub fn nom_de_dossier(adresse: &str) -> String {
    let nom: String = adresse
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c.to_ascii_lowercase() } else { '_' })
        .collect();
    if nom.trim_matches(['_', '.']).is_empty() { "signature".to_string() } else { nom }
}

/// Importe une signature HTML (celle d'Outlook, typiquement) : le HTML est
/// assaini, ses images locales — celles du dossier du fichier, et d'aucun
/// autre — sont copiées dans `dossier`, ses images distantes y sont
/// téléchargées. Une image distante restait distante : ni affichée ni animée
/// dans MMail, et partie en lien — le GIF animé d'un générateur de
/// signatures n'apparaissait pas. Celle qui ne vient pas reste distante. Rend
/// le HTML, ses images désignées par leur nouveau fichier.
pub fn importer(fichier: &Path, dossier: &Path, prefixe: &str) -> Result<String, String> {
    importer_avec(fichier, dossier, prefixe, &telecharger_image)
}

/// Une image distante téléchargée, ses octets ; `None` si elle ne vient pas.
type Telecharger<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

/// `importer`, les images distantes demandées à `telecharger`.
fn importer_avec(fichier: &Path, dossier: &Path, prefixe: &str, telecharger: Telecharger<'_>) -> Result<String, String> {
    let octets = std::fs::read(fichier).map_err(|e| format!("lecture de {} : {e}", fichier.display()))?;
    let source = decoder_html(&octets);
    let base = fichier
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .ok_or_else(|| "dossier de la signature illisible".to_string())?;
    let corps = assainir(&source, move |adresse: &str| image_relative(&base, adresse));
    std::fs::create_dir_all(dossier).map_err(|e| format!("dossier des signatures : {e}"))?;
    let mut html = corps.html;
    for (rang, (type_mime, contenu)) in corps.images.iter().enumerate() {
        let cible = dossier.join(format!("{prefixe}-{rang}.{}", extension_image(type_mime)));
        std::fs::write(&cible, contenu).map_err(|e| format!("image de signature : {e}"))?;
        html = html.replace(&format!("\"{REPERE_IMAGE}{rang}\""), &format!("\"{}\"", url_fichier(&cible)));
    }
    for (rang, adresse) in corps.distantes.iter().enumerate() {
        let remplacement = match rapatrier_image(adresse, dossier, &format!("{prefixe}-d{rang}"), telecharger) {
            Some(cible) => url_fichier(&cible),
            None => adresse.replace('"', "%22"),
        };
        html = html.replace(&format!("\"{REPERE_DISTANTE}{rang}\""), &format!("\"{remplacement}\""));
    }
    Ok(html)
}

/// Images distantes au plus téléchargées d'une signature.
const DISTANTES_MAX: usize = 20;

/// Télécharge dans `dossier` les images distantes (`https:`) d'une signature
/// éditée — collée depuis un message ou une page —, et rend son HTML, ces
/// images désignées par leur fichier. Une image qui ne vient pas reste
/// distante. Le HTML est celui de l'éditeur : il n'est pas réassaini.
pub fn rapatrier_distantes(html: &str, dossier: &Path, prefixe: &str) -> String {
    rapatrier_avec(html, dossier, prefixe, &telecharger_image)
}

fn rapatrier_avec(html: &str, dossier: &Path, prefixe: &str, telecharger: Telecharger<'_>) -> String {
    let mut sortie = String::with_capacity(html.len());
    let mut reste = html;
    let mut rang = 0;
    while let Some(i) = reste.find("<img") {
        let Some(fin) = reste[i..].find('>') else {
            break;
        };
        let balise = &reste[i..i + fin + 1];
        sortie.push_str(&reste[..i]);
        reste = &reste[i + fin + 1..];
        let adresse = source_distante(balise);
        let rapatriee = adresse.filter(|_| rang < DISTANTES_MAX).and_then(|a| {
            rang += 1;
            rapatrier_image(&a, dossier, &format!("{prefixe}-c{rang}"), telecharger).map(|cible| (a, cible))
        });
        match rapatriee {
            Some((adresse, cible)) => sortie.push_str(&balise.replacen(&adresse, &url_fichier(&cible), 1)),
            None => sortie.push_str(balise),
        }
    }
    sortie.push_str(reste);
    sortie
}

/// Adresse `https:` de l'attribut `src` d'une balise `<img …>`, telle
/// qu'écrite.
fn source_distante(balise: &str) -> Option<String> {
    let bas = balise.to_ascii_lowercase();
    let debut = bas.find("src=")? + 4;
    let guillemet = balise[debut..].chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let valeur = &balise[debut + 1..];
    let valeur = &valeur[..valeur.find(guillemet)?];
    valeur.to_ascii_lowercase().starts_with("https://").then(|| valeur.to_string())
}

/// Type d'une image reconnu à son contenu : un serveur peut l'annoncer de
/// travers, et l'adresse ne dit pas toujours l'extension.
pub fn type_par_contenu(octets: &[u8]) -> Option<&'static str> {
    if octets.starts_with(b"GIF87a") || octets.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if octets.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if octets.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if octets.len() > 12 && &octets[..4] == b"RIFF" && &octets[8..12] == b"WEBP" {
        Some("image/webp")
    } else if octets.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        None
    }
}

/// Écrit une image distante dans `dossier`, sous `nom` et l'extension de son
/// type, reconnu au contenu : un GIF reste un GIF, animation comprise.
/// `None` si elle ne vient pas, ou n'est pas une image.
fn rapatrier_image(adresse: &str, dossier: &Path, nom: &str, telecharger: Telecharger<'_>) -> Option<PathBuf> {
    let adresse = adresse.replace("&amp;", "&");
    if !adresse.to_ascii_lowercase().starts_with("https://") {
        return None;
    }
    let octets = telecharger(&adresse)?;
    let type_mime = type_par_contenu(&octets)?;
    std::fs::create_dir_all(dossier).ok()?;
    let cible = dossier.join(format!("{nom}.{}", extension_image(type_mime)));
    std::fs::write(&cible, &octets).ok()?;
    Some(cible)
}

/// Une image distante : HTTPS seul, 5 Mo au plus, une seconde tentative après
/// une erreur de réseau.
fn telecharger_image(adresse: &str) -> Option<Vec<u8>> {
    let essai = || crate::http::requete_bornee("GET", adresse, "image/*", TAILLE_MAX as usize);
    let reponse = match essai() {
        Err(crate::imap::Erreur::Reseau(_)) => essai(),
        autre => autre,
    }
    .ok()?;
    (reponse.statut == 200 && !reponse.corps.is_empty()).then_some(reponse.corps)
}

/// Image désignée par un chemin relatif au fichier importé — « Signature_
/// fichiers/image001.png » — et qui reste dans son dossier.
fn image_relative(base: &Path, adresse: &str) -> Option<Image> {
    if adresse.contains(':') {
        return None;
    }
    let relatif = crate::rendu::decoder_pourcent(adresse)?.replace('\\', "/");
    let chemin = base.join(relatif.trim_start_matches('/')).canonicalize().ok()?;
    if !chemin.starts_with(base) {
        return None;
    }
    let type_mime = type_image(&chemin.to_string_lossy())?;
    (std::fs::metadata(&chemin).ok()?.len() <= TAILLE_MAX)
        .then(|| std::fs::read(&chemin).ok())
        .flatten()
        .map(|octets| (type_mime.to_string(), octets))
}

/// Copie une image choisie par l'utilisateur dans le dossier des signatures,
/// et rend le fichier copié.
pub fn copier_image(source: &Path, dossier: &Path, prefixe: &str) -> Result<PathBuf, String> {
    let nom = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let type_mime = type_image(&nom).ok_or_else(|| format!("« {nom} » n'est pas une image (PNG, JPEG, GIF, BMP, WebP)"))?;
    let taille = std::fs::metadata(source).map_err(|e| format!("{nom} : {e}"))?.len();
    if taille > TAILLE_MAX {
        return Err(format!("« {nom} » dépasse 5 Mo : trop lourd pour une signature"));
    }
    std::fs::create_dir_all(dossier).map_err(|e| format!("dossier des signatures : {e}"))?;
    let cible = dossier.join(format!("{prefixe}.{}", extension_image(type_mime)));
    std::fs::copy(source, &cible).map_err(|e| format!("copie de {nom} : {e}"))?;
    Ok(cible)
}

/// Texte d'un fichier HTML : UTF-8, UTF-16 (marque d'ordre), ou le jeu de
/// caractères qu'il annonce — Outlook écrit ses signatures en windows-1252.
pub fn decoder_html(octets: &[u8]) -> String {
    if let Some(reste) = octets.strip_prefix(b"\xEF\xBB\xBF") {
        return String::from_utf8_lossy(reste).into_owned();
    }
    let utf16 = |reste: &[u8], petit: bool| {
        let unites: Vec<u16> = reste
            .chunks_exact(2)
            .map(|c| if petit { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        String::from_utf16_lossy(&unites)
    };
    if let Some(reste) = octets.strip_prefix(b"\xFF\xFE") {
        return utf16(reste, true);
    }
    if let Some(reste) = octets.strip_prefix(b"\xFE\xFF") {
        return utf16(reste, false);
    }
    // Le jeu annoncé l'emporte : un fichier windows-1252 peut être, par
    // hasard, de l'UTF-8 valide.
    let tete = String::from_utf8_lossy(&octets[..octets.len().min(2048)]).to_ascii_lowercase();
    let annonce_occidental = ["windows-1252", "iso-8859-1", "iso-8859-15", "latin1"]
        .iter()
        .any(|j| tete.contains(&format!("charset={j}")) || tete.contains(&format!("charset=\"{j}")));
    if !annonce_occidental {
        if let Ok(texte) = std::str::from_utf8(octets) {
            return texte.to_string();
        }
    }
    octets.iter().map(|&o| windows_1252(o)).collect()
}

/// Caractère d'un octet windows-1252 : ISO-8859-1, sauf les guillemets,
/// tirets, € et Œ… de la plage 0x80-0x9F.
fn windows_1252(o: u8) -> char {
    const PLAGE: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
        '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match o {
        0x80..=0x9F => PLAGE[(o - 0x80) as usize],
        _ => o as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier_essai(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mmail-signature-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn noms_de_dossier() {
        assert_eq!(nom_de_dossier("Prenom.Nom@Exemple.fr"), "prenom.nom_exemple.fr");
        assert_eq!(nom_de_dossier("../x"), "..-x".replace('-', "_"));
        assert_eq!(nom_de_dossier("@"), "signature");
        assert_eq!(nom_de_dossier(".."), "signature");
    }

    #[test]
    fn decodage_des_jeux_de_caracteres() {
        assert_eq!(decoder_html("été".as_bytes()), "été");
        assert_eq!(decoder_html(b"\xEF\xBB\xBFok"), "ok");
        assert_eq!(decoder_html(b"\xFF\xFEo\x00k\x00"), "ok");
        assert_eq!(
            decoder_html(b"<meta content=\"text/html; charset=windows-1252\">R\xe9publique \x80 \x93x\x94"),
            "<meta content=\"text/html; charset=windows-1252\">République € “x”"
        );
        // Sans annonce, un octet isolé hors UTF-8 se lit en windows-1252.
        assert_eq!(decoder_html(b"Soci\xe9t\xe9"), "Société");
    }

    #[test]
    fn import_d_une_signature_outlook() {
        let racine = dossier_essai("import");
        let source = racine.join("Outlook");
        std::fs::create_dir_all(source.join("Signature_fichiers")).unwrap();
        std::fs::write(source.join("Signature_fichiers/image001.png"), b"\x89PNG logo").unwrap();
        std::fs::write(racine.join("secret.png"), b"\x89PNG secret").unwrap();
        std::fs::write(
            source.join("Signature.htm"),
            b"<html><head><meta content=\"text/html; charset=windows-1252\"><style>p.MsoNormal{margin:0}</style></head>\
              <body><p class=MsoNormal>Pr\xe9nom NOM<o:p></o:p></p>\
              <p class=MsoNormal><img width=344 height=67 src=\"Signature_fichiers/image001.png\"></p>\
              <p><img src=\"../secret.png\"><img src=\"file:///etc/passwd\"><img src=\"https://exemple.fr/l.png\"></p>\
              <p><img src=\"https://hub.exemple.fr/sig/anime.gif?t=1&amp;v=2\"></p>\
              <p>Soci\xe9t\xe9</p><script>x()</script></body></html>",
        )
        .unwrap();
        let cible = racine.join("profil/signatures/prenom_exemple.fr");
        // Le GIF animé du générateur de signatures vient (adresse décodée) ;
        // l'autre image distante ne vient pas, elle reste distante.
        let demandes = std::cell::RefCell::new(Vec::new());
        let telecharger = |adresse: &str| {
            demandes.borrow_mut().push(adresse.to_string());
            adresse.starts_with("https://hub.exemple.fr/").then(|| b"GIF89a anime".to_vec())
        };
        let html = importer_avec(&source.join("Signature.htm"), &cible, "s1", &telecharger).unwrap();
        let copie = cible.join("s1-0.png");
        assert_eq!(std::fs::read(&copie).unwrap(), b"\x89PNG logo");
        assert!(html.contains(&format!("src=\"{}\"", url_fichier(&copie))), "{html}");
        assert!(html.contains("src=\"https://exemple.fr/l.png\""), "{html}");
        let gif = cible.join("s1-d1.gif");
        assert_eq!(std::fs::read(&gif).unwrap(), b"GIF89a anime");
        assert!(html.contains(&format!("src=\"{}\"", url_fichier(&gif))), "{html}");
        assert!(!html.contains("hub.exemple.fr"), "{html}");
        assert!(demandes.borrow().contains(&"https://hub.exemple.fr/sig/anime.gif?t=1&v=2".to_string()), "{:?}", demandes.borrow());
        assert!(html.contains("Société"), "{html}");
        assert!(html.contains("p.MsoNormal{margin:0}"), "{html}");
        assert!(!html.contains("secret") && !html.contains("passwd") && !html.contains("script"), "{html}");
        assert_eq!(std::fs::read_dir(&cible).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(&racine);
    }

    /// Signature collée depuis un message : ses images distantes sont
    /// téléchargées, type reconnu au contenu ; une page qui n'est pas une
    /// image, une adresse non chiffrée, ou une image locale ne changent pas.
    #[test]
    fn images_collees_rapatriees() {
        let racine = dossier_essai("collee");
        let cible = racine.join("signatures/a_exemple.fr");
        let telecharger = |adresse: &str| match adresse {
            "https://hub.exemple.fr/a.gif" => Some(b"GIF89a anime".to_vec()),
            "https://hub.exemple.fr/logo" => Some(b"\x89PNG\r\n\x1a\nlogo".to_vec()),
            "https://hub.exemple.fr/page" => Some(b"<html>pas une image</html>".to_vec()),
            _ => None,
        };
        let html = "<p>Nom</p><img src=\"https://hub.exemple.fr/a.gif\" width=\"120\" />\
                    <img SRC='https://hub.exemple.fr/logo'><img src=\"https://hub.exemple.fr/page\">\
                    <img src=\"http://hub.exemple.fr/a.gif\"><img src=\"file:///tmp/local.png\"><p>Fin</p>";
        let sortie = rapatrier_avec(html, &cible, "s2", &telecharger);
        let gif = cible.join("s2-c1.gif");
        let png = cible.join("s2-c2.png");
        assert_eq!(std::fs::read(&gif).unwrap(), b"GIF89a anime");
        assert!(sortie.contains(&format!("<img src=\"{}\" width=\"120\" />", url_fichier(&gif))), "{sortie}");
        assert!(sortie.contains(&format!("<img SRC='{}'>", url_fichier(&png))), "{sortie}");
        assert!(sortie.contains("src=\"https://hub.exemple.fr/page\""), "{sortie}");
        assert!(sortie.contains("src=\"http://hub.exemple.fr/a.gif\""), "{sortie}");
        assert!(sortie.contains("src=\"file:///tmp/local.png\""), "{sortie}");
        assert!(sortie.starts_with("<p>Nom</p>") && sortie.ends_with("<p>Fin</p>"), "{sortie}");
        assert_eq!(std::fs::read_dir(&cible).unwrap().count(), 2);
        // Rien de distant : le HTML revient tel quel, sans rien demander.
        let rien = |_: &str| -> Option<Vec<u8>> { panic!("rien à télécharger") };
        assert_eq!(rapatrier_avec("<p>a</p><img src=\"file:///x.png\">", &cible, "s3", &rien), "<p>a</p><img src=\"file:///x.png\">");
        assert_eq!(type_par_contenu(b"GIF87a"), Some("image/gif"));
        assert_eq!(type_par_contenu(b"\xFF\xD8\xFF\xE0"), Some("image/jpeg"));
        assert_eq!(type_par_contenu(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(type_par_contenu(b"texte"), None);
        let _ = std::fs::remove_dir_all(&racine);
    }

    #[test]
    fn copie_d_une_image() {
        let racine = dossier_essai("copie");
        std::fs::write(racine.join("logo.PNG"), b"\x89PNG").unwrap();
        std::fs::write(racine.join("note.txt"), b"x").unwrap();
        let cible = copier_image(&racine.join("logo.PNG"), &racine.join("sig"), "i7").unwrap();
        assert_eq!(cible, racine.join("sig/i7.png"));
        assert!(copier_image(&racine.join("note.txt"), &racine.join("sig"), "i8").is_err());
        let _ = std::fs::remove_dir_all(&racine);
    }
}
