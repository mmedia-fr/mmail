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
    /// Largeur, en pixels, pour laquelle l'expéditeur a composé le message
    /// (cf. `mise_en_page::largeur_de_conception`) : l'affichage s'y tient.
    pub largeur: Option<u32>,
}

/// Repère posé en tête du HTML affiché, `<!--mmail-largeur:775-->` : la
/// largeur de conception, que l'interface lit pour centrer le message. Le
/// HTML d'une réponse, qui cite le message, ne le porte pas.
pub const REPERE_LARGEUR: &str = "<!--mmail-largeur:";

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
    let mut corps = assainir(&source, move |adresse: &str| {
        let cid = adresse.get(..4).filter(|p| p.eq_ignore_ascii_case("cid:")).map(|_| &adresse[4..])?;
        integrees.get(&normaliser_cid(cid)).cloned()
    });
    corps.largeur = crate::mise_en_page::largeur_de_conception(&corps.html);
    corps.html = lier_html(&crate::mise_en_page::aplatir(&corps.html));
    Some(corps)
}

/// Assainit un HTML quelconque — corps de message, signature importée. Les
/// images `data:` sont décodées ici, les adresses `http(s)` deviennent des
/// images distantes ; toute autre adresse d'image (`cid:`, chemin relatif)
/// est confiée à `resoudre`, qui rend l'image ou rien.
pub fn assainir(source: &str, resoudre: impl Fn(&str) -> Option<Image> + Send + Sync + 'static) -> CorpsHtml {
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
            .url_schemes(HashSet::from(["http", "https", "mailto", "tel", "cid", "data"]))
            // Les adresses relatives passent jusqu'au filtre, qui les confie à
            // `resoudre` (une image) ou les écarte (un lien).
            .url_relative(ammonia::UrlRelative::PassThrough)
            .link_rel(None)
            .strip_comments(true)
            .attribute_filter(move |balise: &str, attribut: &str, valeur: &str| {
                filtrer(balise, attribut, valeur, &partagee, &resoudre).map(Cow::Owned)
            });
        regles.clean(source).to_string()
    };
    let collecte = std::mem::take(&mut *collecte.lock().unwrap_or_else(|e| e.into_inner()));
    let html = retirer_elements(&html).trim().to_string();
    // Une image d'un élément masqué (logo du thème sombre d'un avis de
    // remboursement, 29/10/2026) ne s'affiche pas : elle ne se télécharge pas non plus, et
    // son échec ne compte pas au bandeau.
    let (html, distantes) = distantes_affichees(html, collecte.distantes);

    // Les feuilles de style de l'expéditeur, assainies, en tête.
    let styles = feuilles_de_style(source);
    let html = if styles.trim().is_empty() {
        html
    } else {
        format!("<style>{styles}</style>{html}")
    };
    CorpsHtml { html, images: collecte.images, distantes, largeur: None }
}

/// Les images distantes que le HTML désigne encore après le retrait des
/// éléments masqués, renumérotées dans l'ordre de leur rang d'origine. Un rang
/// ne fait que descendre, et les rangs plus petits sont réécrits d'abord :
/// aucun repère n'en recouvre un autre.
fn distantes_affichees(mut html: String, distantes: Vec<String>) -> (String, Vec<String>) {
    let repere = |rang: usize| format!("\"{REPERE_DISTANTE}{rang}\"");
    let gardees: Vec<usize> = (0..distantes.len()).filter(|&rang| html.contains(&repere(rang))).collect();
    if gardees.len() == distantes.len() {
        return (html, distantes);
    }
    for (nouveau, &ancien) in gardees.iter().enumerate() {
        if nouveau != ancien {
            html = html.replace(&repere(ancien), &repere(nouveau));
        }
    }
    let distantes = gardees.into_iter().map(|rang| distantes[rang].clone()).collect();
    (html, distantes)
}

#[derive(Default)]
struct Collecte {
    images: Vec<(String, Vec<u8>)>,
    /// Rang d'une image intégrée déjà retenue, par sa désignation d'origine.
    deja: HashMap<String, usize>,
    distantes: Vec<String>,
}

/// Une image : type MIME et octets.
pub type Image = (String, Vec<u8>);

/// Valeur gardée d'un attribut, réécrite au besoin ; `None` l'écarte.
fn filtrer(
    balise: &str,
    attribut: &str,
    valeur: &str,
    collecte: &Mutex<Collecte>,
    resoudre: &dyn Fn(&str) -> Option<Image>,
) -> Option<String> {
    // Le repérage des éléments retirés suppose qu'aucun « < » ni « > » ne
    // traîne dans un attribut : le moteur de Qt n'a besoin d'aucun.
    if valeur.contains(['<', '>']) {
        return None;
    }
    match (balise, attribut) {
        (_, "hidden") => Some("hidden".to_string()),
        (_, "style") if masque(valeur) => Some(REPERE_MASQUE.to_string()),
        (_, "style") => Some(repli_generique(&marges_sans_auto(&css_sur(valeur)))),
        ("a", "href") => {
            let minuscule = valeur.trim().to_ascii_lowercase();
            if minuscule.starts_with("www.") {
                // Lien écrit sans protocole : les navigateurs le lisent comme
                // une adresse web, `ammonia` comme un chemin relatif (0.6.21).
                return Some(format!("http://{}", valeur.trim()));
            }
            if let Some(numero) = minuscule.strip_prefix("tel:") {
                // Lien d'appel d'une signature (0.6.32) : le numéro seul,
                // chiffres et « + », séparateurs retirés ; tout autre contenu
                // l'écarte.
                let numero = numero.replace("%20", " ");
                let admis = numero.chars().all(|c| c.is_ascii_digit() || matches!(c, '+' | ' ' | '.' | '-' | '(' | ')'));
                let chiffres: String = numero.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect();
                return (admis && chiffres.chars().filter(char::is_ascii_digit).count() >= 3)
                    .then(|| format!("tel:{chiffres}"));
            }
            ["http:", "https:", "mailto:"]
                .iter()
                .any(|s| minuscule.starts_with(s))
                .then(|| valeur.trim().to_string())
        }
        ("img", "src") => Some(source_image(valeur.trim(), collecte, resoudre)),
        (_, "src") | (_, "href") => None,
        (_, "bgcolor") | (_, "color") => couleur(valeur),
        _ => Some(valeur.to_string()),
    }
}

/// Noms de couleurs que le moteur de Qt connaît (ceux de CSS).
const NOMS_DE_COULEURS: &[&str] = &[
    "aliceblue", "antiquewhite", "aqua", "aquamarine", "azure", "beige", "bisque", "black",
    "blanchedalmond", "blue", "blueviolet", "brown", "burlywood", "cadetblue", "chartreuse",
    "chocolate", "coral", "cornflowerblue", "cornsilk", "crimson", "cyan", "darkblue", "darkcyan",
    "darkgoldenrod", "darkgray", "darkgreen", "darkgrey", "darkkhaki", "darkmagenta",
    "darkolivegreen", "darkorange", "darkorchid", "darkred", "darksalmon", "darkseagreen",
    "darkslateblue", "darkslategray", "darkslategrey", "darkturquoise", "darkviolet", "deeppink",
    "deepskyblue", "dimgray", "dimgrey", "dodgerblue", "firebrick", "floralwhite", "forestgreen",
    "fuchsia", "gainsboro", "ghostwhite", "gold", "goldenrod", "gray", "green", "greenyellow",
    "grey", "honeydew", "hotpink", "indianred", "indigo", "ivory", "khaki", "lavender",
    "lavenderblush", "lawngreen", "lemonchiffon", "lightblue", "lightcoral", "lightcyan",
    "lightgoldenrodyellow", "lightgray", "lightgreen", "lightgrey", "lightpink", "lightsalmon",
    "lightseagreen", "lightskyblue", "lightslategray", "lightslategrey", "lightsteelblue",
    "lightyellow", "lime", "limegreen", "linen", "magenta", "maroon", "mediumaquamarine",
    "mediumblue", "mediumorchid", "mediumpurple", "mediumseagreen", "mediumslateblue",
    "mediumspringgreen", "mediumturquoise", "mediumvioletred", "midnightblue", "mintcream",
    "mistyrose", "moccasin", "navajowhite", "navy", "oldlace", "olive", "olivedrab", "orange",
    "orangered", "orchid", "palegoldenrod", "palegreen", "paleturquoise", "palevioletred",
    "papayawhip", "peachpuff", "peru", "pink", "plum", "powderblue", "purple", "red", "rosybrown",
    "royalblue", "saddlebrown", "salmon", "sandybrown", "seagreen", "seashell", "sienna", "silver",
    "skyblue", "slateblue", "slategray", "slategrey", "snow", "springgreen", "steelblue", "tan",
    "teal", "thistle", "tomato", "transparent", "turquoise", "violet", "wheat", "white",
    "whitesmoke", "yellow", "yellowgreen",
];

/// Couleur d'un attribut `bgcolor` ou `color`, telle que le moteur de Qt la
/// comprend, ou `None` pour l'écarter. Les navigateurs tolèrent un code
/// hexadécimal sans « # » (`bgcolor="ffffff"`) ; Qt n'y voit pas une couleur
/// et peint le fond en noir, ce qui rendait illisibles des courriels entiers
/// (notification 3CX du 16/10). Le « # » est rétabli ; toute autre valeur
/// qu'un code ou un nom connu est écartée — mieux vaut pas de fond qu'un fond
/// noir.
pub(crate) fn couleur(valeur: &str) -> Option<String> {
    let v = valeur.trim();
    let hexa = |h: &str| matches!(h.len(), 3 | 6) && h.chars().all(|c| c.is_ascii_hexdigit());
    if let Some(code) = v.strip_prefix('#') {
        return hexa(code).then(|| v.to_string());
    }
    if hexa(v) {
        return Some(format!("#{v}"));
    }
    let nom = v.to_ascii_lowercase();
    NOMS_DE_COULEURS.contains(&nom.as_str()).then_some(nom)
}

/// Repère qui remplace l'adresse d'une image.
fn source_image(valeur: &str, collecte: &Mutex<Collecte>, resoudre: &dyn Fn(&str) -> Option<Image>) -> String {
    let Ok(mut c) = collecte.lock() else {
        return REPERE_VIDE.to_string();
    };
    let minuscule = valeur.to_ascii_lowercase();
    if minuscule.starts_with("http://") || minuscule.starts_with("https://") {
        // Une image en `http://` se demande en HTTPS, seul protocole du
        // téléchargement, comme les navigateurs le font des images d'une page
        // sûre : bien des expéditeurs les servent aux deux, et celles d'un
        // avis du 29/10/2026 ne venaient jamais, expéditeur de
        // confiance ou non.
        let adresse = if minuscule.starts_with("http://") {
            format!("https://{}", &valeur["http://".len()..])
        } else {
            valeur.to_string()
        };
        if let Some(rang) = c.distantes.iter().position(|d| *d == adresse) {
            return format!("{REPERE_DISTANTE}{rang}");
        }
        if c.distantes.len() + c.images.len() >= IMAGES_MAX {
            return REPERE_VIDE.to_string();
        }
        c.distantes.push(adresse);
        return format!("{REPERE_DISTANTE}{}", c.distantes.len() - 1);
    }
    if let Some(&rang) = c.deja.get(valeur) {
        return format!("{REPERE_IMAGE}{rang}");
    }
    if c.distantes.len() + c.images.len() >= IMAGES_MAX {
        return REPERE_VIDE.to_string();
    }
    let image = if minuscule.starts_with("data:") { donnees(valeur) } else { resoudre(valeur) };
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
    let css = repli_generique(&marges_sans_auto(&css_sur(&sans_regles_at(&css.replace("<!--", "").replace("-->", "")))));
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

/// Un style ou une feuille de style dont les marges `auto` sont nulles. Dans
/// un navigateur, `margin: 0 auto` centre un bloc de largeur fixe ; aplati en
/// bloc pleine largeur, il n'a plus rien à centrer — et le moteur de Qt en
/// faisait un retrait d'environ 185 pixels à gauche, qui étranglait le texte
/// des avis au gabarit Foundation (30/10/2026).
fn marges_sans_auto(css: &str) -> String {
    let mut sortie = String::with_capacity(css.len());
    let mut reste = css;
    while !reste.is_empty() {
        let fin = reste.find([';', '{', '}']).map_or(reste.len(), |i| i + 1);
        let (declaration, suite) = reste.split_at(fin);
        match declaration.split_once(':') {
            Some((propriete, valeur))
                if matches!(propriete.trim().to_ascii_lowercase().as_str(), "margin" | "margin-left" | "margin-right") =>
            {
                sortie.push_str(propriete);
                sortie.push(':');
                let mut mot = String::new();
                for c in valeur.chars().chain(std::iter::once('\0')) {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                        mot.push(c);
                        continue;
                    }
                    sortie.push_str(if mot.eq_ignore_ascii_case("auto") { "0" } else { &mot });
                    mot.clear();
                    if c != '\0' {
                        sortie.push(c);
                    }
                }
            }
            _ => sortie.push_str(declaration),
        }
        reste = suite;
    }
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

/// Familles génériques de CSS, et mots-clés qui ne nomment aucune police.
const GENERIQUES: &[&str] =
    &["serif", "sans-serif", "monospace", "cursive", "fantasy", "system-ui", "inherit", "initial", "unset"];

/// Termine par `sans-serif` toute liste `font-family` sans famille générique.
/// Quand aucune police nommée n'est installée (« Montserrat », « Ubuntu »), le
/// moteur de Qt prend sinon la police de repli du système, qui peut être à
/// chasse fixe ; l'application désigne, pour chaque famille générique, des
/// polices présentes sur le poste.
fn repli_generique(css: &str) -> String {
    const PROPRIETE: &str = "font-family";
    let minuscule = css.to_ascii_lowercase();
    let mut sortie = String::with_capacity(css.len() + 16);
    let mut copie = 0;
    let mut cherche = 0;
    while let Some(i) = minuscule[cherche..].find(PROPRIETE) {
        let apres_nom = cherche + i + PROPRIETE.len();
        cherche = apres_nom;
        let espaces = minuscule[apres_nom..].len() - minuscule[apres_nom..].trim_start().len();
        if !minuscule[apres_nom + espaces..].starts_with(':') {
            continue;
        }
        let debut = apres_nom + espaces + 1;
        let fin = minuscule[debut..].find([';', '}']).map_or(css.len(), |k| debut + k);
        let valeur = &css[debut..fin];
        let (familles, important) = match minuscule[debut..fin].find("!important") {
            Some(k) => (&valeur[..k], &valeur[k..]),
            None => (valeur, ""),
        };
        let generique = familles.split(',').any(|famille| {
            let famille = famille.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
            GENERIQUES.contains(&famille.as_str())
        });
        if !generique && !familles.trim().is_empty() {
            sortie.push_str(&css[copie..debut]);
            sortie.push_str(familles.trim_end());
            sortie.push_str(", sans-serif");
            if !important.is_empty() {
                sortie.push(' ');
                sortie.push_str(important);
            }
            copie = fin;
        }
        cherche = fin;
    }
    sortie.push_str(&css[copie..]);
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

pub(crate) fn base64(texte: &str) -> Option<Vec<u8>> {
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

pub(crate) fn echapper(texte: &str) -> String {
    texte.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Un message en texte brut plus long n'est pas mis en HTML pour ses liens :
/// le moteur de texte riche le dessinerait trop lentement.
const TEXTE_LIE_MAX: usize = 512 * 1024;

/// Texte brut d'un message en HTML, ses adresses web et de courriel rendues
/// cliquables (0.6.21), ou `None` s'il n'en porte aucune — il s'affiche
/// alors en texte simple, comme avant. Les espaces sont gardés tels quels
/// (`white-space:pre-wrap`) ; un retour à la ligne devient `<br>` : laissé
/// tel quel, le moteur de Qt en ferait un paragraphe, avec ses marges.
pub fn texte_en_html(texte: &str) -> Option<String> {
    if texte.len() > TEXTE_LIE_MAX {
        return None;
    }
    let echappe = echapper(&texte.replace("\r\n", "\n"));
    let lie = lier_texte(&echappe);
    (lie != echappe).then(|| format!("<div style=\"white-space:pre-wrap\">{}</div>", lie.replace('\n', "<br>")))
}

/// Rend cliquables les adresses écrites sans lien dans un HTML assaini
/// (0.6.21) : seulement dans le texte, hors des liens existants et des
/// feuilles de style. Le HTML vient d'`assainir` : bien formé, sans « < » ni
/// « > » dans un attribut.
pub(crate) fn lier_html(html: &str) -> String {
    let mut sortie = String::with_capacity(html.len());
    let mut reste = html;
    let mut dans_lien = 0usize;
    while !reste.is_empty() {
        let Some(debut) = reste.find('<') else {
            sortie.push_str(&if dans_lien == 0 { lier_texte(reste) } else { reste.to_string() });
            break;
        };
        let (texte, suite) = reste.split_at(debut);
        let Some(fin) = suite.find('>') else {
            sortie.push_str(&if dans_lien == 0 { lier_texte(texte) } else { texte.to_string() });
            sortie.push_str(suite);
            break;
        };
        let balise = &suite[..=fin];
        let nom = balise[1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '/')
            .collect::<String>()
            .to_ascii_lowercase();
        // Adresse postale coupée par un retour à la ligne (0.6.32) : la voie
        // avant le `<br>`, le code postal et la commune après ; deux liens
        // vers la même carte.
        if dans_lien == 0 && nom == "br" {
            let apres = &suite[fin + 1..];
            let ligne = &apres[..apres.find('<').unwrap_or(apres.len())];
            if let Some((debut_rue, fin_rue, ville, cible)) = adresse_sur_deux_lignes(texte, ligne) {
                let blancs = ligne.len() - ligne.trim_start().len();
                sortie.push_str(&lier_texte(&texte[..debut_rue]));
                sortie.push_str(&format!("<a href=\"{cible}\">{}</a>", &texte[debut_rue..fin_rue]));
                sortie.push_str(&texte[fin_rue..]);
                sortie.push_str(balise);
                sortie.push_str(&ligne[..blancs]);
                sortie.push_str(&format!("<a href=\"{cible}\">{}</a>", &ligne[blancs..blancs + ville]));
                sortie.push_str(&lier_texte(&ligne[blancs + ville..]));
                reste = &apres[ligne.len()..];
                continue;
            }
        }
        if dans_lien == 0 {
            sortie.push_str(&lier_texte(texte));
        } else {
            sortie.push_str(texte);
        }
        match nom.as_str() {
            "a" if !balise.ends_with("/>") => dans_lien += 1,
            "/a" => dans_lien = dans_lien.saturating_sub(1),
            "style" => {
                // Une feuille de style se recopie telle quelle, jusqu'à sa fin.
                let minuscule = suite.to_ascii_lowercase();
                let fin_style = minuscule.find("</style>").map_or(suite.len(), |i| i + "</style>".len());
                sortie.push_str(&suite[..fin_style]);
                reste = &suite[fin_style..];
                continue;
            }
            _ => {}
        }
        sortie.push_str(balise);
        reste = &suite[fin + 1..];
    }
    sortie
}

/// Caractère d'une partie locale d'adresse de courriel.
fn car_local(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
}

/// Entités qui terminent une adresse dans un texte échappé : « < », « > »,
/// guillemet, espace insécable, toute entité numérique.
const FINS_D_ADRESSE: &[&str] = &["&lt;", "&gt;", "&quot;", "&nbsp;", "&#"];

/// Liens posés sur un texte déjà échappé pour HTML (`&amp;`, `&lt;`…) :
/// adresses `http(s)://`, adresses commençant par `www.`, adresses de
/// courriel ; numéros de téléphone (`tel:`) et adresses postales (carte
/// d'OpenStreetMap), 0.6.32. Le texte affiché ne change pas ; seuls `http`,
/// `https`, `mailto` et `tel` sont produits.
fn lier_texte(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    let mut precedent: Option<char> = None;
    let mut i = 0;
    while i < texte.len() {
        let reste = &texte[i..];
        let frontiere = precedent
            .is_none_or(|c| !c.is_alphanumeric() && !matches!(c, '/' | ':' | '@' | '.' | '-' | '_' | '%' | '+'));
        // Un numéro suit volontiers « Tél. : » ou « Tél: », pas un chiffre,
        // une lettre ni « + », « - », « / » (référence, plage, date).
        let frontiere_numero = precedent.is_none_or(|c| {
            !c.is_alphanumeric() && !matches!(c, '/' | '@' | '-' | '_' | '%' | '+' | '#' | '&')
        });
        let trouve = if frontiere {
            adresse_web(reste).or_else(|| adresse_courriel(reste)).or_else(|| adresse_postale(reste))
        } else {
            None
        };
        let trouve = trouve.or_else(|| if frontiere_numero { numero_telephone(reste) } else { None });
        {
            if let Some((longueur, cible)) = trouve {
                let vue = &reste[..longueur];
                sortie.push_str(&format!("<a href=\"{cible}\">{vue}</a>"));
                precedent = vue.chars().last();
                i += longueur;
                continue;
            }
        }
        let c = reste.chars().next().unwrap_or(' ');
        sortie.push(c);
        precedent = Some(c);
        i += c.len_utf8();
    }
    sortie
}

/// Longueur et cible d'une adresse web en tête de `texte` (échappé), ou
/// `None`. La ponctuation qui suit une adresse dans une phrase n'en fait pas
/// partie.
fn adresse_web(texte: &str) -> Option<(usize, String)> {
    let debut = texte.get(..8).unwrap_or(texte).to_ascii_lowercase();
    let www = debut.starts_with("www.");
    if !(debut.starts_with("http://") || debut.starts_with("https://") || www) {
        return None;
    }
    let mut fin = 0;
    for (k, c) in texte.char_indices() {
        if c.is_whitespace()
            || matches!(c, '<' | '>' | '"' | '\'' | '`')
            || FINS_D_ADRESSE.iter().any(|e| texte[k..].starts_with(e))
        {
            break;
        }
        fin = k + c.len_utf8();
    }
    let mut adresse = &texte[..fin];
    loop {
        if let Some(sans) = adresse.strip_suffix("&amp;") {
            adresse = sans;
            continue;
        }
        let Some(dernier) = adresse.chars().last() else { break };
        let fermante_seule = (dernier == ')' && adresse.matches('(').count() < adresse.matches(')').count())
            || (dernier == ']' && adresse.matches('[').count() < adresse.matches(']').count());
        if matches!(dernier, '.' | ',' | ';' | ':' | '!' | '?' | '}') || fermante_seule {
            adresse = &adresse[..adresse.len() - 1];
        } else {
            break;
        }
    }
    if www {
        // « www.exemple.fr » au moins : un point après « www. ».
        let hote = adresse.get(4..)?.split(['/', '?', '#']).next().unwrap_or("");
        if !hote.contains('.') || hote.starts_with('.') || hote.ends_with('.') {
            return None;
        }
        Some((adresse.len(), format!("http://{adresse}")))
    } else {
        let apres = &adresse[adresse.find("://")? + 3..];
        if !apres.chars().next()?.is_alphanumeric() {
            return None;
        }
        Some((adresse.len(), adresse.to_string()))
    }
}

/// Longueur et cible (`mailto:`) d'une adresse de courriel en tête de
/// `texte`, ou `None`.
fn adresse_courriel(texte: &str) -> Option<(usize, String)> {
    let local = texte.find(|c: char| !car_local(c)).unwrap_or(texte.len());
    if local == 0 || !texte[local..].starts_with('@') || texte.starts_with('.') {
        return None;
    }
    let domaine = &texte[local + 1..];
    let mut longueur = domaine
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'))
        .unwrap_or(domaine.len());
    while longueur > 0 && matches!(domaine.as_bytes()[longueur - 1], b'.' | b'-') {
        longueur -= 1;
    }
    let domaine = &domaine[..longueur];
    let tld = domaine.rsplit('.').next().unwrap_or("");
    if !domaine.contains('.')
        || domaine.starts_with(['.', '-'])
        || domaine.contains("..")
        || tld.len() < 2
        || !tld.chars().all(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    let total = local + 1 + longueur;
    Some((total, format!("mailto:{}", &texte[..total])))
}

/// Séparateur entre deux groupes de chiffres d'un numéro de téléphone :
/// espace, point, tiret, espace insécable (caractère ou entité). Rend sa
/// longueur.
fn separateur_numero(texte: &str) -> Option<usize> {
    [" ", ".", "-", "\u{a0}", "\u{202f}", "&nbsp;", "&#160;"]
        .iter()
        .find(|s| texte.starts_with(**s))
        .map(|s| s.len())
}

/// Longueur et cible (`tel:`) d'un numéro de téléphone en tête de `texte`
/// (échappé), ou `None` (0.6.32). Numéro français à dix chiffres, d'un seul
/// tenant ou par paires (« 05 57 00 00 00 », « 05.57.00.00.00 ») ; numéro
/// international (« +33 5 57 00 00 00 », « +33 (0)5 57 00 00 00 »,
/// « 0033… », « +41 22 000 00 00 »), de 8 à 15 chiffres par groupes de 4 au
/// plus. Un seul séparateur entre deux groupes ; rien de collé à la fin.
fn numero_telephone(texte: &str) -> Option<(usize, String)> {
    let b = texte.as_bytes();
    let non_nul = |c: &u8| c.is_ascii_digit() && *c != b'0';
    let (mut i, international) = match b {
        [b'+', c, ..] if non_nul(c) => (1, true),
        [b'0', b'0', c, ..] if non_nul(c) => (2, true),
        [b'0', c, ..] if non_nul(c) => (0, false),
        _ => return None,
    };
    let mut groupes: Vec<&str> = Vec::new();
    let mut fin = 0;
    let mut zero_entre_parentheses = false;
    loop {
        let debut = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == debut {
            break;
        }
        groupes.push(&texte[debut..i]);
        fin = i;
        let mut j = i + separateur_numero(&texte[i..]).unwrap_or(0);
        // « +33 (0)5… » : le zéro national entre parenthèses.
        if international && groupes.len() == 1 && texte[j..].starts_with("(0)") {
            zero_entre_parentheses = true;
            j += 3;
            j += separateur_numero(&texte[j..]).unwrap_or(0);
        }
        if j == i || !b.get(j).is_some_and(u8::is_ascii_digit) {
            break;
        }
        i = j;
    }
    if texte[fin..].chars().next().is_some_and(|c| c.is_alphanumeric() || matches!(c, '@' | '/' | '_')) {
        return None;
    }
    let chiffres = groupes.concat();
    let cible = if !international {
        let forme = groupes.len() == 1 || (groupes.len() == 5 && groupes.iter().all(|g| g.len() == 2));
        (chiffres.len() == 10 && forme).then(|| format!("tel:{chiffres}"))?
    } else if let Some(national) = chiffres.strip_prefix("33") {
        // Le zéro national écrit après l'indicatif (« +33 05… ») tombe.
        let national = if !zero_entre_parentheses && national.len() == 10 {
            national.strip_prefix('0')?
        } else {
            national
        };
        (national.len() == 9 && !national.starts_with('0')).then(|| format!("tel:+33{national}"))?
    } else {
        let forme = groupes.len() == 1 || groupes.iter().all(|g| g.len() <= 4);
        ((8..=15).contains(&chiffres.len()) && forme).then(|| format!("tel:+{chiffres}"))?
    };
    Some((fin, cible))
}

/// Types de voie d'une adresse postale, en minuscules.
const VOIES: &[&str] = &[
    "rue", "avenue", "av", "av.", "boulevard", "bd", "bld", "bvd", "place", "pl.", "allée", "allee",
    "allées", "chemin", "ch.", "impasse", "imp.", "route", "rte", "quai", "cours", "square", "passage",
    "lieu-dit", "lieudit", "lotissement", "lot.", "résidence", "residence", "zone", "za", "zi", "zac", "zae",
    "parc", "rond-point", "esplanade", "faubourg", "fg", "voie", "sentier", "montée", "côte", "hameau",
    "parvis", "promenade", "cité", "cite", "villa", "domaine", "traverse", "rampe", "ruelle", "venelle",
    "galerie", "chaussée", "quartier",
];

/// Mots qui ne prolongent pas le nom d'une commune : ce qui suit souvent une
/// adresse dans une signature.
const APRES_COMMUNE: &[&str] = &[
    "tél", "tél.", "tel", "tel.", "téléphone", "telephone", "fax", "mobile", "mob", "portable", "port",
    "email", "e-mail", "mail", "courriel", "site", "web", "siret", "siren", "rcs", "tva",
];

/// Blanc entre les éléments d'une adresse postale : espace, tabulation,
/// espaces insécables (caractère ou entité). Rend sa longueur.
fn blanc_adresse(texte: &str) -> Option<usize> {
    [" ", "\t", "\u{a0}", "\u{202f}", "&nbsp;", "&#160;"]
        .iter()
        .find(|s| texte.starts_with(**s))
        .map(|s| s.len())
}

/// Longueur des blancs en tête de `texte`.
fn blancs_adresse(texte: &str) -> usize {
    let mut n = 0;
    while let Some(l) = blanc_adresse(&texte[n..]) {
        n += l;
    }
    n
}

/// Début d'une adresse postale en tête de `texte` : numéro (« 12 »,
/// « 12 bis », « 12B »), virgule éventuelle, puis type de voie (« rue »,
/// « avenue »…) ; sans numéro, seulement un type de voie à initiale
/// capitale (« ZA des… », « Lieu-dit… »). Rend la longueur jusqu'au nom
/// de la voie.
fn debut_de_voie(texte: &str) -> Option<usize> {
    let b = texte.as_bytes();
    let mut i = 0;
    while i < b.len() && i < 4 && b[i].is_ascii_digit() {
        i += 1;
    }
    let numero = i > 0;
    // Sans numéro, un type de voie à initiale capitale : les autres mots
    // s'écartent sans rien calculer.
    if !numero && !texte.chars().next().is_some_and(char::is_uppercase) {
        return None;
    }
    if numero {
        if b.get(i).is_some_and(u8::is_ascii_digit) {
            return None;
        }
        if b.get(i).is_some_and(|&c| matches!(c, b'A'..=b'D' | b'a'..=b'd'))
            && !b.get(i + 1).is_some_and(u8::is_ascii_alphanumeric)
        {
            i += 1;
        }
        let n = blancs_adresse(&texte[i..]);
        let apres = &texte[i + n..];
        for indice in ["bis", "ter", "quater"] {
            let suivant = apres.get(indice.len()..).and_then(|r| r.chars().next());
            if n > 0
                && apres.get(..indice.len()).is_some_and(|m| m.eq_ignore_ascii_case(indice))
                && !suivant.is_some_and(char::is_alphanumeric)
            {
                i += n + indice.len();
                break;
            }
        }
        if texte[i..].starts_with(',') {
            i += 1;
        }
        let n = blancs_adresse(&texte[i..]);
        if n == 0 {
            return None;
        }
        i += n;
    }
    let fin_mot = texte[i..]
        .find(|c: char| !(c.is_alphabetic() || c == '-' || c == '.'))
        .map_or(texte.len(), |k| i + k);
    let mot = &texte[i..fin_mot];
    if !VOIES.contains(&mot.to_lowercase().as_str()) {
        return None;
    }
    if !numero && !mot.chars().next().is_some_and(char::is_uppercase) {
        return None;
    }
    let n = blancs_adresse(&texte[fin_mot..]);
    (n > 0).then_some(fin_mot + n)
}

/// Caractère admis dans le nom d'une voie (« du 8 Mai 1945 », « de l'Église »,
/// « Bât. B, »).
fn car_voie(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, ' ' | '-' | '\'' | '’' | '.' | ',' | '(' | ')' | '\u{a0}' | '\u{202f}')
}

/// Entité admise dans le nom d'une voie : espace insécable, apostrophe,
/// esperluette. Rend sa longueur.
fn entite_voie(texte: &str) -> Option<usize> {
    ["&nbsp;", "&#160;", "&#39;", "&#x27;", "&amp;"]
        .iter()
        .find(|s| texte.starts_with(**s))
        .map(|s| s.len())
}

/// Code postal (5 chiffres, département 01 à 98) et commune en tête de
/// `texte` : longueur jusqu'à la fin de la commune. La commune commence par
/// une capitale ; ses mots se lient par un espace, un tiret ou une
/// apostrophe, les petits mots (« sur », « de », « lès »…) entre deux ;
/// « CEDEX » et son numéro en font partie.
fn code_postal_et_commune(texte: &str) -> Option<usize> {
    let b = texte.as_bytes();
    if b.len() < 7 || !b[..5].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let departement = (b[0] - b'0') * 10 + (b[1] - b'0');
    if departement == 0 || departement > 98 {
        return None;
    }
    let n = blancs_adresse(&texte[5..]);
    if n == 0 {
        return None;
    }
    let debut = 5 + n;
    let mut i = debut;
    let mut fin = 0;
    let mut mots = 0;
    loop {
        let t = &texte[i..];
        let longueur = t
            .find(|c: char| !(c.is_alphabetic() || matches!(c, '-' | '\'' | '’')))
            .unwrap_or(t.len());
        if longueur == 0 {
            break;
        }
        let mot = &t[..longueur];
        let minuscule = mot.to_lowercase();
        if APRES_COMMUNE.contains(&minuscule.trim_end_matches('.')) {
            break;
        }
        let capitale = mot.chars().next().is_some_and(char::is_uppercase);
        let petit_mot = ["sur", "sous", "en", "de", "du", "des", "la", "le", "les", "lès", "lez", "et", "aux", "au"]
            .contains(&minuscule.as_str());
        if !capitale && !(mots > 0 && petit_mot) {
            break;
        }
        i += longueur;
        mots += 1;
        if capitale {
            fin = i;
        }
        if minuscule == "cedex" {
            let n = blancs_adresse(&texte[i..]);
            let chiffres = texte[i + n..].find(|c: char| !c.is_ascii_digit()).unwrap_or(texte.len() - i - n);
            if n > 0 && (1..=2).contains(&chiffres) {
                fin = i + n + chiffres;
            }
            break;
        }
        let n = blanc_adresse(&texte[i..]).unwrap_or(0);
        if n == 0 || mots >= 6 || i - debut > 50 {
            break;
        }
        i += n;
    }
    (fin > 0).then_some(fin)
}

/// Fin d'une adresse postale en tête de `texte`, juste après le nom de la
/// voie : virgule, blancs, tiret, un seul retour à la ligne, boîte postale
/// éventuelle (« BP 12 », « CS 30012 »), puis code postal et commune. Rend
/// la longueur jusqu'à la fin de la commune.
fn suite_postale(texte: &str) -> Option<usize> {
    let mut i = 0;
    let mut lignes = 0;
    let mut vu = false;
    loop {
        if let Some(n) = blanc_adresse(&texte[i..]) {
            i += n;
            vu = true;
            continue;
        }
        match texte[i..].chars().next() {
            Some(',') => i += 1,
            Some('\n') if lignes == 0 => {
                i += 1;
                lignes += 1;
            }
            Some('-') if vu => i += 1,
            _ => break,
        }
        vu = true;
    }
    if !vu {
        return None;
    }
    for boite in ["BP", "B.P.", "CS"] {
        if let Some(apres) = texte[i..].strip_prefix(boite) {
            let n = blancs_adresse(apres);
            let chiffres = apres[n..].find(|c: char| !c.is_ascii_digit()).unwrap_or(apres.len() - n);
            if n > 0 && (1..=5).contains(&chiffres) {
                i += boite.len() + n + chiffres;
                let apres = &texte[i..];
                let virgule = usize::from(apres.starts_with(','));
                let n = blancs_adresse(&apres[virgule..]);
                if n == 0 {
                    return None;
                }
                i += virgule + n;
            }
            break;
        }
    }
    Some(i + code_postal_et_commune(&texte[i..])?)
}

/// Longueur et cible (carte d'OpenStreetMap) d'une adresse postale en tête
/// de `texte` (échappé), ou `None` (0.6.32) : voie, puis code postal et
/// commune sur la même ligne ou la suivante.
fn adresse_postale(texte: &str) -> Option<(usize, String)> {
    let nom = debut_de_voie(texte)?;
    let mut k = nom;
    let mut lettres = 0;
    while k < texte.len() && k - nom <= 70 {
        let t = &texte[k..];
        let c = t.chars().next()?;
        if lettres >= 2 && matches!(c, ' ' | ',' | '\n' | '-' | '&' | '\t' | '\u{a0}' | '\u{202f}') {
            if let Some(longueur) = suite_postale(t) {
                let fin = k + longueur;
                return Some((fin, carte(&texte[..fin])));
            }
        }
        if let Some(n) = entite_voie(t) {
            k += n;
            continue;
        }
        if !car_voie(c) {
            return None;
        }
        if c.is_alphabetic() {
            lettres += 1;
        }
        k += c.len_utf8();
    }
    None
}

/// Adresse postale coupée par un `<br>` (0.6.32) : la voie termine `avant`,
/// le code postal et la commune ouvrent `ligne`. Rend le début et la fin de
/// la voie dans `avant`, la longueur du code postal et de la commune dans
/// `ligne` (après ses blancs), et la carte.
fn adresse_sur_deux_lignes(avant: &str, ligne: &str) -> Option<(usize, usize, usize, String)> {
    let blancs = ligne.len() - ligne.trim_start().len();
    let ville = code_postal_et_commune(&ligne[blancs..])?;
    let fin_rue = avant.trim_end().trim_end_matches(',').trim_end().len();
    let rue = &avant[..fin_rue];
    let mut precedent: Option<char> = None;
    for (k, c) in rue.char_indices() {
        if precedent.is_none_or(|p| !p.is_alphanumeric() && !matches!(p, '-' | '\'' | '’' | '&' | '#')) {
            if let Some(nom) = debut_de_voie(&rue[k..]) {
                let reste = &rue[k + nom..];
                let lettres = reste.chars().filter(|c| c.is_alphabetic()).count();
                let admis = reste.chars().all(|c| car_voie(c) || matches!(c, '&' | '#' | ';'));
                if lettres >= 2 && admis && reste.len() <= 70 {
                    let cible = carte(&format!("{}, {}", &rue[k..], &ligne[blancs..blancs + ville]));
                    return Some((k, fin_rue, ville, cible));
                }
            }
        }
        precedent = Some(c);
    }
    None
}

/// Carte d'OpenStreetMap cherchant une adresse postale écrite dans un texte
/// échappé : entités rendues, blancs ramenés à un espace, le reste encodé.
fn carte(adresse: &str) -> String {
    let clair = adresse
        .replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let clair = clair.split(char::is_whitespace).filter(|m| !m.is_empty()).collect::<Vec<_>>().join(" ");
    let mut url = String::from("https://www.openstreetmap.org/search?query=");
    for octet in clair.bytes() {
        match octet {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => url.push(octet as char),
            _ => url.push_str(&format!("%{octet:02X}")),
        }
    }
    url
}

/// Adresse `file:` d'un fichier local, caractères réservés encodés : le
/// profil peut vivre sous « C:\\Users\\Jean Dupont\\… ».
pub fn url_fichier(chemin: &std::path::Path) -> String {
    let texte = chemin.to_string_lossy().replace('\\', "/");
    let mut url = String::from(if texte.starts_with('/') { "file://" } else { "file:///" });
    for octet in texte.bytes() {
        match octet {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                url.push(octet as char)
            }
            _ => url.push_str(&format!("%{octet:02X}")),
        }
    }
    url
}

/// Chemin désigné par une adresse `file:`, ou `None` si ce n'en est pas une.
/// « file:///C:/x » → « C:/x » ; « file:///home/x » → « /home/x ».
pub fn chemin_de_url(url: &str) -> Option<std::path::PathBuf> {
    let reste = url.get(..5).filter(|p| p.eq_ignore_ascii_case("file:")).map(|_| &url[5..])?;
    let reste = reste.strip_prefix("//").unwrap_or(reste);
    // « //hote/… » : un partage réseau, jamais un fichier du profil.
    let reste = reste.strip_prefix("localhost").unwrap_or(reste);
    let texte = decoder_pourcent(reste)?;
    // « /C:/… » sous Windows : la lettre de lecteur suit la barre.
    let b = texte.as_bytes();
    let texte = if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        texte[1..].to_string()
    } else {
        texte
    };
    let c = texte.as_bytes();
    let absolu = texte.starts_with('/')
        || (c.len() >= 3 && c[0].is_ascii_alphabetic() && c[1] == b':' && (c[2] == b'/' || c[2] == b'\\'));
    absolu.then(|| std::path::PathBuf::from(texte))
}

/// Décode les « %XX » d'une adresse ; `None` si le résultat n'est pas de l'UTF-8.
pub fn decoder_pourcent(texte: &str) -> Option<String> {
    let octets = texte.as_bytes();
    let mut decode = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        if octets[i] == b'%' && i + 2 < octets.len() {
            if let Some(v) = texte.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                decode.push(v);
                i += 3;
                continue;
            }
        }
        decode.push(octets[i]);
        i += 1;
    }
    String::from_utf8(decode).ok()
}

/// Type MIME d'une image d'après l'extension de son fichier.
pub fn type_image(nom: &str) -> Option<&'static str> {
    let extension = nom.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        _ => return None,
    })
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
        // L'image en `http://` est demandée en HTTPS.
        assert_eq!(c.distantes, vec!["https://exemple.fr/logo.png", "https://exemple.fr/b.gif"]);
        assert_eq!(
            c.html,
            "<img src=\"mmail-distante:0\" width=\"10\"><img src=\"mmail-distante:0\"><img src=\"mmail-distante:1\">"
        );
        // La même, une fois en `http://` et une fois en `https://` : une seule.
        let c = corps_html(&message("<img src=\"HTTP://exemple.fr/a.png\"><img src=\"https://exemple.fr/a.png\">"))
            .unwrap();
        assert_eq!(c.distantes, vec!["https://exemple.fr/a.png"]);
    }

    #[test]
    fn images_distantes_masquees_ni_reperees_ni_comptees() {
        // Avis de remboursement du 29/10/2026 : les logos du thème sombre, masqués,
        // se téléchargeaient pour rien et leur échec comptait au bandeau.
        let c = corps_html(&message(
            "<img src=\"https://exemple.fr/clair.png\"><img style=\"display:none\" src=\"https://exemple.fr/sombre.png\">\
             <div style=\"display: none\"><img src=\"https://exemple.fr/cache.png\"></div>\
             <img src=\"https://exemple.fr/article.jpg\"><img src=\"https://exemple.fr/clair.png\">",
        ))
        .unwrap();
        assert_eq!(c.distantes, vec!["https://exemple.fr/clair.png", "https://exemple.fr/article.jpg"]);
        assert_eq!(
            c.html,
            "<img src=\"mmail-distante:0\"><img src=\"mmail-distante:1\"><img src=\"mmail-distante:0\">"
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
        // Le tableau, réduit à une cellule, s'aplatit en blocs.
        assert_eq!(c.html, "<p>Visible</p><div><div>y</div></div>");
    }

    #[test]
    fn styles_sans_adresse() {
        let c = corps_html(&message(
            "<html><head><style>@import url(https://x/a.css); <!-- p.MsoNormal{margin:0;background:url('https://t/p.gif')}\
             @media only screen and (max-width:600px){ .m{display:none} } --></style></head>\
             <body><table><tr><td style=\"background-image:url(https://x/fond.png); color:red\">a</td><td>b</td></tr></table></body></html>",
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
    fn marges_auto_nulles() {
        // Gabarit Foundation (avis du 30/10/2026) : style d'un élément, puis
        // feuille de style ; les autres propriétés et marges restent.
        assert_eq!(
            marges_sans_auto("text-align:left;Margin: 0 auto;width:616px"),
            "text-align:left;Margin: 0 0;width:616px"
        );
        assert_eq!(
            marges_sans_auto(".float-center{margin:0 auto !important;float:none}a:hover{margin-left:auto}"),
            ".float-center{margin:0 0 !important;float:none}a:hover{margin-left:0}"
        );
        assert_eq!(marges_sans_auto("margin:10px 20px;padding:auto"), "margin:10px 20px;padding:auto");
    }

    #[test]
    fn couleurs_sans_diese_retablies() {
        assert_eq!(couleur("ffffff").as_deref(), Some("#ffffff"));
        assert_eq!(couleur(" FFF ").as_deref(), Some("#FFF"));
        assert_eq!(couleur("#0596D4").as_deref(), Some("#0596D4"));
        assert_eq!(couleur("White").as_deref(), Some("white"));
        // Ce que Qt peindrait en noir est écarté.
        assert_eq!(couleur("#ff00"), None);
        assert_eq!(couleur("fffff"), None);
        assert_eq!(couleur("rgb(1,2,3)"), None);
        assert_eq!(couleur("blanc"), None);
        assert_eq!(couleur(""), None);
        // La lettre 3CX du 16/10 : cadre blanc déclaré sans « # ».
        let c = corps_html(&message(
            "<table bgcolor=\"ffffff\" width=\"600\"><tr><td><font color=\"373737\">Bonjour</font></td></tr></table>\
             <p><font color=\"nimportequoi\">x</font></p>",
        ))
        .unwrap();
        assert!(!c.html.contains("\"ffffff\"") && !c.html.contains(":ffffff"), "{}", c.html);
        assert!(c.html.contains("#ffffff"), "{}", c.html);
        assert!(c.html.contains("#373737"), "{}", c.html);
        assert!(!c.html.contains("nimportequoi"), "{}", c.html);
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

    #[test]
    fn repli_sur_une_famille_generique() {
        assert_eq!(repli_generique("font-family:Montserrat;font-size:16px"),
                   "font-family:Montserrat, sans-serif;font-size:16px");
        assert_eq!(repli_generique("p { font-family: 'Open Sans', Ubuntu !important }"),
                   "p { font-family: 'Open Sans', Ubuntu, sans-serif !important }");
        for gardee in ["font-family: Arial, sans-serif", "font-family:Georgia,serif", "font-family: inherit",
                       "font-size:12px", "x-font-family-y: z"] {
            assert_eq!(repli_generique(gardee), gardee);
        }
        // Dans un message entier : style d'élément et feuille de style.
        let corps = corps_html(&message("<style>body{font-family:Ubuntu}</style><p style=\"font-family:Montserrat\">x</p>"))
            .unwrap();
        assert!(corps.html.contains("font-family:Ubuntu, sans-serif"), "{}", corps.html);
        assert!(corps.html.contains("font-family:Montserrat, sans-serif"), "{}", corps.html);
    }

    #[test]
    fn lettre_en_tableaux_imbriques_aplatie() {
        let html = "<div style=\"max-width:600px\"><table align=\"center\" style=\"width:100%\"><tbody><tr>\
                    <td style=\"font-size:0px;padding:20px 0\"><table width=\"600\"><tr><td align=\"left\">\
                    <p>Votre facture est disponible.</p></td></tr></table></td></tr></tbody></table></div>";
        let corps = corps_html(&message(html)).unwrap();
        assert!(!corps.html.contains("<table"), "{}", corps.html);
        assert!(corps.html.contains("<div style=\"text-align:left\"><p>Votre facture est disponible.</p></div>"),
                "{}", corps.html);
    }

    #[test]
    fn liens_poses_sur_le_texte_brut() {
        let html = texte_en_html(
            "Voir https://exemple.fr/a?b=1&c=2, ou www.exemple.fr/doc.\n\
             Écrire à jean.dupont@exemple.fr (ou <marie@exemple.fr>).",
        )
        .unwrap();
        assert_eq!(
            html,
            "<div style=\"white-space:pre-wrap\">Voir <a href=\"https://exemple.fr/a?b=1&amp;c=2\">\
             https://exemple.fr/a?b=1&amp;c=2</a>, ou <a href=\"http://www.exemple.fr/doc\">www.exemple.fr/doc</a>.<br>\
             Écrire à <a href=\"mailto:jean.dupont@exemple.fr\">jean.dupont@exemple.fr</a> (ou \
             &lt;<a href=\"mailto:marie@exemple.fr\">marie@exemple.fr</a>&gt;).</div>"
        );
    }

    #[test]
    fn texte_sans_adresse_reste_du_texte() {
        assert_eq!(texte_en_html("Bonjour,\nà demain à 10 h. 3.5 kg, n@, a@b, www.\n"), None);
        assert_eq!(texte_en_html("ftp://exemple.fr javascript:alert(1) file:///etc/passwd"), None);
    }

    #[test]
    fn adresses_entre_parentheses_et_ponctuation() {
        assert_eq!(
            lier_texte("(https://fr.wikipedia.org/wiki/Agen_(Lot-et-Garonne)) et https://x.fr/;"),
            "(<a href=\"https://fr.wikipedia.org/wiki/Agen_(Lot-et-Garonne)\">\
             https://fr.wikipedia.org/wiki/Agen_(Lot-et-Garonne)</a>) et <a href=\"https://x.fr/\">https://x.fr/</a>;"
        );
        // Une adresse collée à un mot n'en est pas une ; ni un chemin.
        assert_eq!(lier_texte("abcwww.exemple.fr /chemin/www.exemple.fr"), "abcwww.exemple.fr /chemin/www.exemple.fr");
    }

    #[test]
    fn liens_poses_dans_le_html_hors_des_liens() {
        let c = corps_html(&message(
            "<p>Site : www.exemple.fr, contact@exemple.fr</p>\
             <p><a href=\"https://exemple.fr\">https://exemple.fr</a></p>\
             <p><a href=\"www.autre.fr\">autre</a></p>",
        ))
        .unwrap();
        assert_eq!(
            c.html,
            "<p>Site : <a href=\"http://www.exemple.fr\">www.exemple.fr</a>, \
             <a href=\"mailto:contact@exemple.fr\">contact@exemple.fr</a></p>\
             <p><a href=\"https://exemple.fr\">https://exemple.fr</a></p>\
             <p><a href=\"http://www.autre.fr\">autre</a></p>"
        );
    }

    #[test]
    fn feuille_de_style_et_entites_respectees() {
        assert_eq!(
            lier_html("<style>a{x:url(https://x.fr)}</style><p>&lt;https://x.fr&gt; a&nbsp;www.y.fr</p>"),
            "<style>a{x:url(https://x.fr)}</style><p>&lt;<a href=\"https://x.fr\">https://x.fr</a>&gt; \
             a&nbsp;<a href=\"http://www.y.fr\">www.y.fr</a></p>"
        );
    }

    const CARTE: &str = "https://www.openstreetmap.org/search?query=";

    #[test]
    fn numeros_de_telephone_lies() {
        let html = texte_en_html(
            "Tél. : 05 57 00 00 00, portable 06.12.34.56.78\n\
             Suisse : +41 22 000 00 00 ; standard +33 (0)5 53 00 00 00.",
        )
        .unwrap();
        assert_eq!(
            html,
            "<div style=\"white-space:pre-wrap\">Tél. : <a href=\"tel:0557000000\">05 57 00 00 00</a>, \
             portable <a href=\"tel:0612345678\">06.12.34.56.78</a><br>\
             Suisse : <a href=\"tel:+41220000000\">+41 22 000 00 00</a> ; \
             standard <a href=\"tel:+33553000000\">+33 (0)5 53 00 00 00</a>.</div>"
        );
        // Le zéro national après l'indicatif tombe ; « 00 » vaut « + ».
        assert_eq!(
            lier_texte("+33 05 53 00 00 00 et 0033 5 53 00 00 00"),
            "<a href=\"tel:+33553000000\">+33 05 53 00 00 00</a> et \
             <a href=\"tel:+33553000000\">0033 5 53 00 00 00</a>"
        );
    }

    #[test]
    fn dates_montants_et_references_ne_sont_pas_des_numeros() {
        let texte = "Facture 2026-0114 du 02.11.2026 : 1 234,56 €, SIRET 123 456 789 00012, \
                     réf. 0612345678X, +3 points, 05 57 00 00 00 12";
        assert_eq!(lier_texte(texte), texte);
    }

    #[test]
    fn adresses_postales_liees_a_une_carte() {
        let html = texte_en_html(
            "Siège : 12 rue de la Paix, 75002 Paris.\nDépôt : ZA des Essais\n47000 Agen\n\
             Au 3 rue Victor Hugo à Agen.",
        )
        .unwrap();
        assert_eq!(
            html,
            format!(
                "<div style=\"white-space:pre-wrap\">Siège : <a href=\"{CARTE}12%20rue%20de%20la%20Paix%2C%2075002%20Paris\">\
                 12 rue de la Paix, 75002 Paris</a>.<br>Dépôt : <a href=\"{CARTE}ZA%20des%20Essais%2047000%20Agen\">\
                 ZA des Essais<br>47000 Agen</a><br>Au 3 rue Victor Hugo à Agen.</div>"
            )
        );
        assert_eq!(
            lier_texte("5 avenue des Essais, CS 30012, 47916 Agen Cedex 9 - France"),
            format!(
                "<a href=\"{CARTE}5%20avenue%20des%20Essais%2C%20CS%2030012%2C%2047916%20Agen%20Cedex%209\">\
                 5 avenue des Essais, CS 30012, 47916 Agen Cedex 9</a> - France"
            )
        );
        // Sans numéro, une voie en minuscules n'est pas une adresse.
        assert_eq!(lier_texte("la rue des Essais, 47000 Agen"), "la rue des Essais, 47000 Agen");
    }

    #[test]
    fn adresse_sur_deux_lignes_du_html() {
        let carte = format!("{CARTE}12%20rue%20des%20Essais%2C%2047000%20Agen");
        assert_eq!(
            lier_html("<p>MMEDIA<br>12 rue des Essais<br>47000 Agen<br>Tél : 05 53 00 00 00</p>"),
            format!(
                "<p>MMEDIA<br><a href=\"{carte}\">12 rue des Essais</a><br><a href=\"{carte}\">47000 Agen</a><br>\
                 Tél : <a href=\"tel:0553000000\">05 53 00 00 00</a></p>"
            )
        );
    }

    #[test]
    fn liens_d_appel_d_origine_gardes() {
        let c = corps_html(&message("<a href=\"tel:+33-5-53-00-00-00\">Appeler</a><a href=\"tel:javascript:x\">b</a>"))
            .unwrap();
        assert_eq!(c.html, "<a href=\"tel:+33553000000\">Appeler</a><a>b</a>");
    }
}
