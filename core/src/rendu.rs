// SPDX-License-Identifier: GPL-3.0-or-later
//! Rendu HTML d'un message : le HTML de l'expéditeur, assaini pour le moteur
//! de texte riche de Qt.
//!
//! Le HTML d'un courriel est une donnée non fiable. Le moteur de Qt n'exécute
//! aucun script, mais il irait chercher de lui-même ce qu'une adresse désigne :
//! une image distante (qui dit à l'expéditeur que le message a été ouvert), une
//! feuille de style importée, un fichier du poste. Rien ne part donc vers lui
//! tel quel :
//!
//! - l'analyse et le tri des balises sont confiés à `ammonia` (analyseur HTML5
//!   de Servo) : ce qui en sort est bien formé, sans script, sans formulaire,
//!   sans commentaire ;
//! - toute adresse d'image est réécrite en un repère (`mmail-image:N`,
//!   `mmail-distante:N`) que l'appelant remplace par un fichier local — image
//!   intégrée au message, ou image distante téléchargée à la demande ;
//! - les styles perdent leurs `url()`, `@import` et règles `@media` (celles-ci
//!   visent les téléphones, et Qt les appliquerait sans condition) ;
//! - les liens ne gardent que `http`, `https` et `mailto`.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use mail_parser::{MessageParser, MimeHeaders};

/// Ce qu'il faut pour afficher un message en HTML.
#[derive(Debug, Default, PartialEq)]
pub struct CorpsHtml {
    /// HTML assaini ; les images y sont désignées par `mmail-image:N` (rang
    /// dans `images`) ou `mmail-distante:N` (rang dans `distantes`).
    pub html: String,
    /// Images portées par le message (parties `cid:`, adresses `data:`) :
    /// type MIME et octets.
    pub images: Vec<(String, Vec<u8>)>,
    /// Adresses des images distantes, sans doublon, dans l'ordre du message.
    pub distantes: Vec<String>,
}

/// Repère d'une image intégrée.
pub const REPERE_IMAGE: &str = "mmail-image:";
/// Repère d'une image distante.
pub const REPERE_DISTANTE: &str = "mmail-distante:";
/// Image écartée : l'élément est retiré du HTML.
const REPERE_VIDE: &str = "mmail-vide";
/// Style posé sur un élément masqué par l'expéditeur : l'élément est retiré.
const REPERE_MASQUE: &str = "mmail-masque";

/// Une image au-delà de cette taille n'est pas affichée.
const TAILLE_IMAGE_MAX: usize = 10 * 1024 * 1024;
/// Images retenues au plus, intégrées comme distantes.
const IMAGES_MAX: usize = 200;

/// Balises gardées : celles que le moteur de Qt sait mettre en forme. Les
/// autres disparaissent, leur contenu restant.
const BALISES: &[&str] = &[
    "a", "abbr", "address", "b", "big", "blockquote", "br", "caption", "center", "cite", "code",
    "col", "colgroup", "dd", "del", "dfn", "div", "dl", "dt", "em", "font", "h1", "h2", "h3", "h4",
    "h5", "h6", "hr", "i", "img", "ins", "kbd", "li", "nobr", "ol", "p", "pre", "q", "s", "samp",
    "small", "span", "strike", "strong", "sub", "sup", "table", "tbody", "td", "tfoot", "th",
    "thead", "tr", "tt", "u", "ul", "var",
];

/// Balises écartées avec leur contenu.
const BALISES_EFFACEES: &[&str] =
    &["script", "style", "title", "template", "head", "object", "embed", "iframe", "frame",
      "frameset", "applet", "noembed", "noframes", "svg", "math", "form", "select", "textarea",
      "button", "audio", "video", "canvas"];

/// Attributs gardés, sur toute balise gardée.
const ATTRIBUTS: &[&str] = &[
    "style", "class", "align", "valign", "dir", "lang", "width", "height", "bgcolor", "color",
    "face", "size", "border", "cellpadding", "cellspacing", "colspan", "rowspan", "nowrap", "start",
    "type", "hidden",
];

/// Corps HTML d'un message entier, ou `None` s'il n'a pas de partie HTML (il
/// s'affiche alors en texte).
pub fn corps_html(brut: &[u8]) -> Option<CorpsHtml> {
    let message = MessageParser::default().parse(brut)?;
    let parties: Vec<&mail_parser::MessagePart<'_>> = message.html_bodies().collect();
    if !parties.iter().any(|p| p.is_text_html()) {
        return None;
    }
    // Plusieurs parties (Apple Mail coupe le HTML autour des pièces jointes) :
    // mises bout à bout, une partie texte égarée parmi elles passant en
    // paragraphe.
    let mut source = String::new();
    for partie in &parties {
        let Some(texte) = partie.text_contents() else { continue };
        if partie.is_text_html() {
            source.push_str(texte);
        } else {
            source.push_str("<div style=\"white-space:pre-wrap\">");
            source.push_str(&echapper(texte));
            source.push_str("</div>");
        }
    }

    // Images portées par le message, désignées par leur Content-ID.
    let integrees: HashMap<String, Image> = message
        .parts
        .iter()
        .filter_map(|partie| {
            let cid = partie.content_id()?;
            let ct = partie.content_type()?;
            let type_mime = format!("{}/{}", ct.ctype(), ct.subtype().unwrap_or("")).to_ascii_lowercase();
            type_mime
                .starts_with("image/")
                .then(|| (normaliser_cid(cid), (type_mime, partie.contents().to_vec())))
        })
        .collect();

    let collecte = Arc::new(Mutex::new(Collecte::default()));
    let html = {
        let partagee = Arc::clone(&collecte);
        let mut regles = ammonia::Builder::empty();
        regles
            .tags(BALISES.iter().copied().collect())
            .clean_content_tags(BALISES_EFFACEES.iter().copied().collect())
            .generic_attributes(ATTRIBUTS.iter().copied().collect())
            .tag_attributes(HashMap::from([
                ("a", HashSet::from(["href"])),
                ("img", HashSet::from(["src"])),
            ]))
            .url_schemes(HashSet::from(["http", "https", "mailto", "cid", "data"]))
            .url_relative(ammonia::UrlRelative::Deny)
            .link_rel(None)
            .strip_comments(true)
            .attribute_filter(move |balise: &str, attribut: &str, valeur: &str| {
                filtrer(balise, attribut, valeur, &partagee, &integrees).map(Cow::Owned)
            });
        regles.clean(&source).to_string()
    };
    let collecte = std::mem::take(&mut *collecte.lock().unwrap_or_else(|e| e.into_inner()));
    let html = retirer_elements(&html).trim().to_string();

    // Les feuilles de style de l'expéditeur, assainies, en tête.
    let styles = feuilles_de_style(&source);
    let html = if styles.trim().is_empty() {
        html
    } else {
        format!("<style>{styles}</style>{html}")
    };
    Some(CorpsHtml { html, images: collecte.images, distantes: collecte.distantes })
}

#[derive(Default)]
struct Collecte {
    images: Vec<(String, Vec<u8>)>,
    /// Rang d'une image intégrée déjà retenue, par sa désignation d'origine.
    deja: HashMap<String, usize>,
    distantes: Vec<String>,
}

type Image = (String, Vec<u8>);

/// Valeur gardée d'un attribut, réécrite au besoin ; `None` l'écarte.
fn filtrer(
    balise: &str,
    attribut: &str,
    valeur: &str,
    collecte: &Mutex<Collecte>,
    integrees: &HashMap<String, Image>,
) -> Option<String> {
    // Le repérage des éléments retirés suppose qu'aucun « < » ni « > » ne
    // traîne dans un attribut : le moteur de Qt n'a besoin d'aucun.
    if valeur.contains(['<', '>']) {
        return None;
    }
    match (balise, attribut) {
        (_, "hidden") => Some("hidden".to_string()),
        (_, "style") if masque(valeur) => Some(REPERE_MASQUE.to_string()),
        (_, "style") => Some(css_sur(valeur)),
        ("a", "href") => {
            let minuscule = valeur.trim().to_ascii_lowercase();
            ["http:", "https:", "mailto:"]
                .iter()
                .any(|s| minuscule.starts_with(s))
                .then(|| valeur.trim().to_string())
        }
        ("img", "src") => Some(source_image(valeur.trim(), collecte, integrees)),
        (_, "src") | (_, "href") => None,
        _ => Some(valeur.to_string()),
    }
}

/// Repère qui remplace l'adresse d'une image.
fn source_image(valeur: &str, collecte: &Mutex<Collecte>, integrees: &HashMap<String, Image>) -> String {
    let Ok(mut c) = collecte.lock() else {
        return REPERE_VIDE.to_string();
    };
    let minuscule = valeur.to_ascii_lowercase();
    if minuscule.starts_with("http://") || minuscule.starts_with("https://") {
        if let Some(rang) = c.distantes.iter().position(|d| d == valeur) {
            return format!("{REPERE_DISTANTE}{rang}");
        }
        if c.distantes.len() + c.images.len() >= IMAGES_MAX {
            return REPERE_VIDE.to_string();
        }
        c.distantes.push(valeur.to_string());
        return format!("{REPERE_DISTANTE}{}", c.distantes.len() - 1);
    }
    if let Some(&rang) = c.deja.get(valeur) {
        return format!("{REPERE_IMAGE}{rang}");
    }
    if c.distantes.len() + c.images.len() >= IMAGES_MAX {
        return REPERE_VIDE.to_string();
    }
    let image = if minuscule.starts_with("cid:") {
        integrees.get(&normaliser_cid(&valeur[4..])).cloned()
    } else if minuscule.starts_with("data:") {
        donnees(valeur)
    } else {
        None
    };
    match image {
        Some((type_mime, octets))
            if type_mime.starts_with("image/") && !octets.is_empty() && octets.len() <= TAILLE_IMAGE_MAX =>
        {
            c.images.push((type_mime, octets));
            let rang = c.images.len() - 1;
            c.deja.insert(valeur.to_string(), rang);
            format!("{REPERE_IMAGE}{rang}")
        }
        _ => REPERE_VIDE.to_string(),
    }
}

/// Vrai si un style masque l'élément : texte d'aperçu des lettres
/// d'information, blocs réservés à d'autres logiciels. Le moteur de Qt ne
/// connaît pas `display: none`, il l'afficherait.
fn masque(style: &str) -> bool {
    let compact: String = style.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
    compact.split(';').any(|d| {
        d.starts_with("display:none")
            || d.starts_with("visibility:hidden")
            || d.starts_with("mso-hide:all")
    })
}

/// Retire du HTML assaini les éléments masqués et les images écartées. Le HTML
/// sort d'`ammonia`, donc bien formé : chaque élément non vide a sa balise
/// fermante, aucun « < » ne traîne hors d'une balise (le texte l'échappe, le
/// filtre l'écarte des attributs).
fn retirer_elements(html: &str) -> String {
    let mut sortie = String::with_capacity(html.len());
    let mut reste = html;
    while let Some(debut) = reste.find('<') {
        sortie.push_str(&reste[..debut]);
        let apres = &reste[debut..];
        let Some(fin) = apres.find('>') else {
            sortie.push_str(apres);
            return sortie;
        };
        let balise = &apres[..=fin];
        // Image écartée, ou dont `ammonia` a déjà retiré l'adresse (schéma
        // refusé) : le moteur de Qt y dessinerait une image cassée.
        let image = balise.starts_with("<img") && balise[4..].starts_with([' ', '>', '/']);
        if image && (!balise.contains(" src=\"") || balise.contains(&format!("src=\"{REPERE_VIDE}\""))) {
            reste = &apres[fin + 1..];
            continue;
        }
        if balise.contains(&format!("style=\"{REPERE_MASQUE}\"")) || balise.contains(" hidden=") {
            let nom: String = balise[1..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            reste = &apres[fin + 1..];
            if !est_vide(&nom) {
                reste = apres_fermeture(reste, &nom);
            }
            continue;
        }
        sortie.push_str(balise);
        reste = &apres[fin + 1..];
    }
    sortie.push_str(reste);
    sortie
}

/// Ce qui suit la balise fermante de l'élément `nom` ouvert juste avant
/// `reste`, en tenant compte des éléments du même nom imbriqués.
fn apres_fermeture<'a>(mut reste: &'a str, nom: &str) -> &'a str {
    let ouvrante = format!("<{nom}");
    let fermante = format!("</{nom}>");
    let mut profondeur = 1;
    while profondeur > 0 {
        let o = reste.find(&ouvrante).filter(|&i| {
            // « <b » ne doit pas trouver « <br ».
            reste[i + ouvrante.len()..].starts_with([' ', '>', '/'])
        });
        let Some(f) = reste.find(&fermante) else { return "" };
        match o {
            Some(o) if o < f => {
                profondeur += 1;
                reste = &reste[o + ouvrante.len()..];
            }
            _ => {
                profondeur -= 1;
                reste = &reste[f + fermante.len()..];
            }
        }
    }
    reste
}

fn est_vide(nom: &str) -> bool {
    matches!(nom, "br" | "hr" | "img" | "col" | "wbr" | "area" | "base" | "input" | "link" | "meta")
}

/// Contenu des éléments `<style>` du HTML d'origine, assaini.
fn feuilles_de_style(html: &str) -> String {
    let minuscule = html.to_ascii_lowercase();
    let mut css = String::new();
    let mut depart = 0;
    while let Some(i) = minuscule[depart..].find("<style") {
        let ouverture = depart + i;
        let Some(f) = minuscule[ouverture..].find('>') else { break };
        let contenu = ouverture + f + 1;
        let Some(g) = minuscule[contenu..].find("</style") else { break };
        css.push_str(&html[contenu..contenu + g]);
        css.push('\n');
        depart = contenu + g;
    }
    let css = css_sur(&sans_regles_at(&css.replace("<!--", "").replace("-->", "")));
    // Une feuille ne doit pas pouvoir refermer l'élément qui la porte.
    css.replace('<', "")
}

/// Retire les règles `@…` d'une feuille de style : `@media` (visent les
/// téléphones), `@font-face` et `@import` (iraient chercher un fichier),
/// `@supports`, `@keyframes`, `@page`.
fn sans_regles_at(css: &str) -> String {
    let mut sortie = String::with_capacity(css.len());
    let mut reste = css;
    while let Some(i) = reste.find('@') {
        sortie.push_str(&reste[..i]);
        let apres = &reste[i..];
        let accolade = apres.find('{');
        let point_virgule = apres.find(';');
        match (accolade, point_virgule) {
            // « @import url(…); » : jusqu'au point-virgule.
            (Some(a), Some(p)) if p < a => reste = &apres[p + 1..],
            (None, Some(p)) => reste = &apres[p + 1..],
            // « @media … { … { … } … } » : jusqu'à l'accolade qui ferme.
            (Some(a), _) => {
                let mut profondeur = 0;
                let mut fin = apres.len();
                for (j, c) in apres[a..].char_indices() {
                    match c {
                        '{' => profondeur += 1,
                        '}' => {
                            profondeur -= 1;
                            if profondeur == 0 {
                                fin = a + j + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                reste = &apres[fin..];
            }
            (None, None) => reste = "",
        }
    }
    sortie.push_str(reste);
    sortie
}

/// Un style sans rien qui aille chercher un fichier : les `url(…)` deviennent
/// `none`, `expression(` et `@import` disparaissent.
pub fn css_sur(css: &str) -> String {
    let mut sortie = String::with_capacity(css.len());
    let minuscule = css.to_ascii_lowercase();
    let mut i = 0;
    while i < css.len() {
        if minuscule[i..].starts_with("url(") {
            // Jusqu'à la parenthèse fermante, guillemets compris.
            let mut j = i + 4;
            let mut guillemet: Option<char> = None;
            for (k, c) in css[j..].char_indices() {
                match (guillemet, c) {
                    (None, '"') | (None, '\'') => guillemet = Some(c),
                    (Some(g), c) if c == g => guillemet = None,
                    (None, ')') => {
                        j += k + 1;
                        guillemet = Some('\0');
                        break;
                    }
                    _ => {}
                }
            }
            if guillemet != Some('\0') {
                j = css.len();
            }
            sortie.push_str("none");
            i = j;
            continue;
        }
        if minuscule[i..].starts_with("expression(") || minuscule[i..].starts_with("@import") {
            sortie.push_str("none");
            i += if minuscule[i..].starts_with("@import") { 7 } else { 11 };
            continue;
        }
        let c = css[i..].chars().next().unwrap_or(' ');
        sortie.push(c);
        i += c.len_utf8();
    }
    sortie
}

/// `<image001.png@01DA…>` et `cid:image001.png%4001DA…` désignent la même
/// partie.
pub(crate) fn normaliser_cid(cid: &str) -> String {
    let cid = cid.trim().trim_start_matches('<').trim_end_matches('>');
    let octets = cid.as_bytes();
    let mut i = 0;
    let mut decode = Vec::with_capacity(octets.len());
    while i < octets.len() {
        if octets[i] == b'%' && i + 2 < octets.len() {
            if let Some(v) = cid.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                decode.push(v);
                i += 3;
                continue;
            }
        }
        decode.push(octets[i]);
        i += 1;
    }
    String::from_utf8_lossy(&decode).to_lowercase()
}

/// Octets d'une adresse `data:image/…;base64,…`.
fn donnees(url: &str) -> Option<Image> {
    let (entete, contenu) = url[5..].split_once(',')?;
    let mut morceaux = entete.split(';');
    let type_mime = morceaux.next().unwrap_or("").trim().to_ascii_lowercase();
    if !morceaux.any(|m| m.trim().eq_ignore_ascii_case("base64")) {
        return None;
    }
    Some((type_mime, base64(contenu)?))
}

fn base64(texte: &str) -> Option<Vec<u8>> {
    let mut sortie = Vec::with_capacity(texte.len() * 3 / 4);
    let mut tampon = 0u32;
    let mut bits = 0;
    for c in texte.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        tampon = (tampon << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            sortie.push((tampon >> bits) as u8);
            tampon &= (1 << bits) - 1;
        }
    }
    Some(sortie)
}

fn echapper(texte: &str) -> String {
    texte.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Extension de fichier d'une image d'après son type : le moteur de Qt la
/// reconnaît à son contenu, l'extension ne sert qu'à qui ouvre le dossier.
pub fn extension_image(type_mime: &str) -> &'static str {
    match type_mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" | "image/pjpeg" => "jpg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "img",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(html: &str) -> Vec<u8> {
        format!(
            "From: a@exemple.fr\r\nTo: b@exemple.fr\r\nSubject: essai\r\nMIME-Version: 1.0\r\n\
             Content-Type: text/html; charset=utf-8\r\n\r\n{html}\r\n"
        )
        .into_bytes()
    }

    #[test]
    fn texte_seul_pas_de_html() {
        let brut = b"From: a@exemple.fr\r\nSubject: x\r\nContent-Type: text/plain\r\n\r\nBonjour\r\n";
        assert_eq!(corps_html(brut), None);
    }

    #[test]
    fn scripts_formulaires_et_commentaires_ecartes() {
        let c = corps_html(&message(
            "<p onclick=\"x()\">Bonjour</p><script>alert(1)</script><!-- note -->\
             <form action=\"https://x\"><input value=\"y\"></form><iframe src=\"https://z\"></iframe>",
        ))
        .unwrap();
        assert_eq!(c.html, "<p>Bonjour</p>");
    }

    #[test]
    fn liens_limites_a_http_et_mailto() {
        let c = corps_html(&message(
            "<a href=\"https://exemple.fr/a\">a</a><a href=\"javascript:alert(1)\">b</a>\
             <a href=\"file:///etc/passwd\">c</a><a href=\"mailto:x@y.fr\">d</a><a href=\"/relatif\">e</a>",
        ))
        .unwrap();
        assert_eq!(
            c.html,
            "<a href=\"https://exemple.fr/a\">a</a><a>b</a><a>c</a><a href=\"mailto:x@y.fr\">d</a><a>e</a>"
        );
    }

    #[test]
    fn images_distantes_reperees_sans_doublon() {
        let c = corps_html(&message(
            "<img src=\"https://exemple.fr/logo.png\" width=\"10\"><img src=\"https://exemple.fr/logo.png\">\
             <img src=\"http://exemple.fr/b.gif\"><img src=\"file:///c:/x.png\"><img src=\"ftp://x/y\">",
        ))
        .unwrap();
        assert_eq!(c.distantes, vec!["https://exemple.fr/logo.png", "http://exemple.fr/b.gif"]);
        assert_eq!(
            c.html,
            "<img src=\"mmail-distante:0\" width=\"10\"><img src=\"mmail-distante:0\"><img src=\"mmail-distante:1\">"
        );
    }

    #[test]
    fn image_integree_par_cid() {
        let brut = "From: a@exemple.fr\r\nSubject: x\r\nMIME-Version: 1.0\r\n\
            Content-Type: multipart/related; boundary=\"B\"\r\n\r\n\
            --B\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
            <p>Logo :</p><img src=\"cid:logo%40exemple\"><img src=\"cid:absent\">\r\n\
            --B\r\nContent-Type: image/png\r\nContent-ID: <logo@exemple>\r\nContent-Transfer-Encoding: base64\r\n\r\n\
            iVBORw0KGgo=\r\n--B--\r\n";
        let c = corps_html(brut.as_bytes()).unwrap();
        assert_eq!(c.images.len(), 1);
        assert_eq!(c.images[0].0, "image/png");
        assert_eq!(&c.images[0].1[..4], b"\x89PNG");
        assert_eq!(c.html, "<p>Logo :</p><img src=\"mmail-image:0\">");
    }

    #[test]
    fn image_en_donnees() {
        let c = corps_html(&message("<img src=\"data:image/gif;base64,R0lGODlh\">")).unwrap();
        assert_eq!(c.images, vec![("image/gif".to_string(), b"GIF89a".to_vec())]);
        assert_eq!(c.html, "<img src=\"mmail-image:0\">");
    }

    #[test]
    fn elements_masques_retires() {
        let c = corps_html(&message(
            "<div style=\"display: none; max-height:0\">Aperçu <div>imbriqué</div> caché</div>\
             <p>Visible</p><span hidden>non</span><table><tr><td style=\"mso-hide:all\">x</td><td>y</td></tr></table>",
        ))
        .unwrap();
        assert_eq!(c.html, "<p>Visible</p><table><tbody><tr><td>y</td></tr></tbody></table>");
    }

    #[test]
    fn styles_sans_adresse() {
        let c = corps_html(&message(
            "<html><head><style>@import url(https://x/a.css); <!-- p.MsoNormal{margin:0;background:url('https://t/p.gif')}\
             @media only screen and (max-width:600px){ .m{display:none} } --></style></head>\
             <body><table><tr><td style=\"background-image:url(https://x/fond.png); color:red\">a</td></tr></table></body></html>",
        ))
        .unwrap();
        assert!(c.html.starts_with("<style>"), "{}", c.html);
        assert!(!c.html.contains("https://"), "{}", c.html);
        assert!(!c.html.contains("@media"), "{}", c.html);
        assert!(c.html.contains("p.MsoNormal{margin:0;background:none}"), "{}", c.html);
        assert!(c.html.contains("background-image:none; color:red"), "{}", c.html);
    }

    #[test]
    fn css_sur_neutralise_les_adresses() {
        assert_eq!(css_sur("a:url(\"x)y\");b:1"), "a:none;b:1");
        assert_eq!(css_sur("width:expression(alert(1))"), "width:nonealert(1))");
        assert_eq!(css_sur("color:URL(x"), "color:none");
    }

    #[test]
    fn attribut_porteur_de_chevron_ecarte() {
        let c = corps_html(&message("<p title=\"a\" class=\"x<div>\">t</p><b>g</b><br>")).unwrap();
        assert_eq!(c.html, "<p>t</p><b>g</b><br>");
    }

    #[test]
    fn imbrication_de_meme_nom() {
        assert_eq!(apres_fermeture("<b>x</b>y</b>z", "b"), "z");
        assert_eq!(apres_fermeture("<br>x</b>z", "b"), "z");
    }

    #[test]
    fn decodage_base64() {
        assert_eq!(base64("TU1haWw=").unwrap(), b"MMail");
        assert_eq!(base64("TU1h\r\naWw").unwrap(), b"MMail");
        assert!(base64("T*").is_none());
    }
}
