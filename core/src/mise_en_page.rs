// SPDX-License-Identifier: GPL-3.0-or-later
//! Mise en page d'un message HTML pour le moteur de texte riche de Qt.
//!
//! Les lettres d'information et les avis automatiques (factures, forges
//! logicielles) se composent en tableaux imbriqués sur quatre ou cinq niveaux,
//! une seule colonne à chaque niveau, avec des largeurs fixes de 600 pixels
//! pour les logiciels qui ignorent `max-width`. Le moteur de Qt calcule mal la
//! largeur de tels empilements : le texte finit dans une colonne de quelques
//! mots, quelle que soit la largeur de la fenêtre.
//!
//! Un tableau dont aucune ligne n'a plus d'une cellule ne range rien en
//! colonnes : il ne sert qu'à la mise en page. Il devient ici une suite de
//! blocs, qui prennent la largeur disponible ; son fond, son alignement et
//! ses classes demeurent, ses largeurs et hauteurs tombent. Un tableau à
//! plusieurs colonnes reste un tableau, et une largeur fixe d'au moins
//! `LARGEUR_PLEINE` pixels y devient la pleine largeur.
//!
//! Le moteur de Qt peint le fond d'un bloc sur la hauteur naturelle de ses
//! lignes : l'interligne qu'impose `line-height` reste hors du fond, et chaque
//! bloc d'un encadré sombre finissait souligné d'une bande blanche (avis de
//! Steam du 27/10/2026). Sous un fond autre que le blanc de la page,
//! `line-height` est donc retiré. Un tableau gardé est un cadre à part, que le
//! fond des blocs qui l'entourent ne couvre pas : il le reçoit en `bgcolor`.
//! Une cellule vide de quelques pixels (filet, espaceur) disparaît, faute de
//! pouvoir être plus basse qu'une ligne de texte.
//!
//! Dans un tableau gardé, une cellule fusionnée sur plusieurs colonnes perd sa
//! largeur : le moteur de Qt la reporte sur les colonnes qu'elle couvre, et les
//! largeurs des lignes suivantes n'y comptent plus — le titre d'une lettre
//! d'information du 29/10/2026 tenait dans une colonne de quelques lettres.
//! Une cellule gardée qui peint son propre fond en est un pour son contenu :
//! `line-height` y tombe aussi, le libellé blanc d'un bouton noir débordant
//! sinon de la cellule. Le moteur de Qt ignore la police nulle : un texte qui
//! n'est qu'en police nulle, invisible ailleurs, est retiré.
//!
//! Un tableau gardé à pleine largeur, sous un fond autre que le blanc, dont
//! toutes les cellules d'une ligne ont une largeur fixe voit ces largeurs
//! devenir des parts de la ligne : le moteur de Qt s'en tenait à leur somme, et
//! laissait de part et d'autre du tableau deux pans que le fond de l'encadré ne
//! couvre pas. Sur le blanc de la page, ces pans ne se voient pas, et le
//! tableau garde ses largeurs (des colonnes de données, ramenées à leurs parts,
//! passaient à la ligne). L'astuce `dir="rtl"` des
//! colonnes inversées sur téléphone est retirée : Qt la portait sur le texte.
//!
//! Le HTML traité est celui qui sort d'`ammonia` : bien formé, chaque élément
//! non vide fermé, attributs entre guillemets, aucun « < » ni « > » dans un
//! attribut. Faute de pouvoir le relire, il est rendu tel quel.

/// Largeur fixe, en pixels, à partir de laquelle un tableau est réputé
/// remplir la fenêtre de lecture de l'expéditeur.
const LARGEUR_PLEINE: u32 = 400;

/// Éléments sans contenu ni balise fermante.
const VIDES: &[&str] = &["br", "hr", "img", "col", "wbr"];

/// Imbrication au-delà de laquelle le HTML est rendu tel quel. `transformer`,
/// `ecrire` et la libération de l'arbre sont récursifs : un message hostile ou
/// mal généré (des milliers de `<div>` imbriqués, qu'`ammonia` laisse passer)
/// dépasserait la pile et arrêterait tout le programme.
const PROFONDEUR_MAX: usize = 256;

/// Attributs repris par le bloc qui remplace une cellule ou un tableau.
const ATTRIBUTS_REPRIS: &[&str] = &["class", "dir", "lang"];

/// Hauteur, en pixels, en deçà de laquelle une cellule vide est un filet ou un
/// espaceur.
const ESPACEUR_MAX: u32 = 4;

#[derive(Debug, Clone, PartialEq)]
enum Noeud {
    Texte(String),
    Element { nom: String, attributs: Vec<(String, String)>, enfants: Vec<Noeud> },
}

/// Le HTML, tableaux de mise en page aplatis en blocs.
pub fn aplatir(html: &str) -> String {
    let Some(noeuds) = analyser(html) else {
        return html.to_string();
    };
    let mut sortie = String::with_capacity(html.len());
    for noeud in noeuds.into_iter().flat_map(|noeud| transformer(noeud, None)) {
        ecrire(&noeud, &mut sortie);
    }
    sortie
}

/// Arbre du HTML, ou `None` s'il n'est pas bien formé.
fn analyser(html: &str) -> Option<Vec<Noeud>> {
    let mut pile: Vec<(String, Vec<(String, String)>, Vec<Noeud>)> = vec![(String::new(), Vec::new(), Vec::new())];
    let mut reste = html;
    while let Some(debut) = reste.find('<') {
        if debut > 0 {
            pile.last_mut()?.2.push(Noeud::Texte(reste[..debut].to_string()));
        }
        let apres = &reste[debut + 1..];
        let fin = apres.find('>')?;
        let balise = &apres[..fin];
        reste = &apres[fin + 1..];
        if let Some(nom) = balise.strip_prefix('/') {
            let (ouvert, attributs, enfants) = pile.pop()?;
            if ouvert != nom.trim() || pile.is_empty() {
                return None;
            }
            pile.last_mut()?.2.push(Noeud::Element { nom: ouvert, attributs, enfants });
        } else {
            let (nom, attributs) = lire_balise(balise)?;
            if VIDES.contains(&nom.as_str()) {
                pile.last_mut()?.2.push(Noeud::Element { nom, attributs, enfants: Vec::new() });
            } else if pile.len() > PROFONDEUR_MAX {
                return None;
            } else {
                pile.push((nom, attributs, Vec::new()));
            }
        }
    }
    if !reste.is_empty() {
        pile.last_mut()?.2.push(Noeud::Texte(reste.to_string()));
    }
    (pile.len() == 1).then(|| pile.pop().map(|(_, _, enfants)| enfants)).flatten()
}

/// Nom et attributs d'une balise ouvrante (sans ses chevrons). Les valeurs
/// restent échappées, telles qu'`ammonia` les a écrites.
fn lire_balise(balise: &str) -> Option<(String, Vec<(String, String)>)> {
    let balise = balise.trim_end_matches('/').trim();
    let fin_nom = balise.find(char::is_whitespace).unwrap_or(balise.len());
    let nom = &balise[..fin_nom];
    if nom.is_empty() || !nom.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let mut attributs = Vec::new();
    let mut reste = balise[fin_nom..].trim_start();
    while !reste.is_empty() {
        let fin = reste.find(|c: char| c == '=' || c.is_whitespace()).unwrap_or(reste.len());
        let attribut = reste[..fin].to_string();
        reste = reste[fin..].trim_start();
        let valeur = if let Some(suite) = reste.strip_prefix('=') {
            let suite = suite.trim_start().strip_prefix('"')?;
            let fin = suite.find('"')?;
            reste = suite[fin + 1..].trim_start();
            suite[..fin].to_string()
        } else {
            String::new()
        };
        attributs.push((attribut, valeur));
    }
    Some((nom.to_ascii_lowercase(), attributs))
}

fn ecrire(noeud: &Noeud, sortie: &mut String) {
    match noeud {
        Noeud::Texte(texte) => sortie.push_str(texte),
        Noeud::Element { nom, attributs, enfants } => {
            sortie.push('<');
            sortie.push_str(nom);
            for (attribut, valeur) in attributs {
                sortie.push_str(&format!(" {attribut}=\"{valeur}\""));
            }
            sortie.push('>');
            if VIDES.contains(&nom.as_str()) {
                return;
            }
            for enfant in enfants {
                ecrire(enfant, sortie);
            }
            sortie.push_str(&format!("</{nom}>"));
        }
    }
}

/// Un nœud après transformation : les tableaux intérieurs d'abord, puis le
/// nœud lui-même. `fond` : couleur de fond du bloc qui l'entoure, quand elle
/// n'est pas le blanc de la page.
fn transformer(noeud: Noeud, fond: Option<&str>) -> Vec<Noeud> {
    let Noeud::Element { nom, attributs, enfants } = noeud else {
        return vec![noeud];
    };
    let propre = fond_de(&attributs);
    let fond = match propre.as_deref() {
        Some(couleur) => (!blanc(couleur)).then_some(couleur),
        None => fond,
    };
    if nom == "table" && lignes(&enfants).iter().any(|ligne| cellules(ligne).len() > 1) {
        let mut attributs = pleine_largeur(attributs);
        if colonnes_inversees(&attributs, &enfants) {
            attributs = sans_direction(attributs);
        }
        if propre.is_none() {
            if let Some(couleur) = fond {
                attributs.push(("bgcolor".to_string(), couleur.to_string()));
            }
        }
        // Sur le blanc de la page, la place que Qt laisse de part et d'autre ne
        // se voit pas, et des colonnes de données, ramenées à leurs parts, y
        // passaient à la ligne : seul un tableau sous un fond est réparti.
        let a_repartir = fond.is_some() && toute_la_largeur(&attributs);
        let enfants = enfants
            .into_iter()
            .flat_map(garder)
            .map(|enfant| if a_repartir { repartir(enfant) } else { enfant })
            .collect();
        return vec![Noeud::Element { nom, attributs, enfants }];
    }
    let attributs = sans_interligne(attributs, fond.is_some());
    let enfants: Vec<Noeud> = if muet(&attributs, &enfants) {
        Vec::new()
    } else {
        enfants.into_iter().flat_map(|enfant| transformer(enfant, fond)).collect()
    };
    if nom != "table" {
        return vec![Noeud::Element { nom, attributs, enfants }];
    }
    let blocs: Vec<Noeud> = enfants.into_iter().flat_map(en_blocs).collect();
    // Plus rien à montrer : le tableau ne portait qu'un filet ou un espaceur.
    if blocs.is_empty() {
        return Vec::new();
    }
    // L'alignement d'un tableau le centre dans la page, il n'aligne pas son
    // texte : un bloc occupant toute la largeur, il est sans objet.
    let attributs: Vec<(String, String)> = attributs.into_iter().filter(|(a, _)| a != "align").collect();
    vec![bloc(&attributs, blocs)]
}

/// Ce que devient un enfant d'un tableau gardé : ses lignes et ses cellules
/// demeurent ; le contenu d'une cellule repart sous le fond de la cellule, ou
/// sans fond hérité si elle n'en a pas, la cellule peignant celui du tableau
/// sur toute sa hauteur. Une cellule muette garde sa place, vidée.
fn garder(noeud: Noeud) -> Vec<Noeud> {
    match noeud {
        Noeud::Element { nom, attributs, enfants }
            if matches!(nom.as_str(), "tbody" | "thead" | "tfoot" | "tr") =>
        {
            vec![Noeud::Element { nom, attributs, enfants: enfants.into_iter().flat_map(garder).collect() }]
        }
        Noeud::Element { nom, attributs, enfants } if matches!(nom.as_str(), "td" | "th" | "caption") => {
            let fond = fond_de(&attributs).filter(|couleur| !blanc(couleur));
            let mut attributs = sans_interligne(attributs, fond.is_some());
            if fusionnee(&attributs) {
                attributs = sans_largeur(attributs);
            }
            let enfants = if muet(&attributs, &enfants) {
                Vec::new()
            } else {
                enfants.into_iter().flat_map(|enfant| transformer(enfant, fond.as_deref())).collect()
            };
            vec![Noeud::Element { nom, attributs, enfants }]
        }
        autre => transformer(autre, None),
    }
}

/// Tableau qui prend toute la largeur : `100%` en attribut ou en style.
fn toute_la_largeur(attributs: &[(String, String)]) -> bool {
    let valeur = |nom: &str| attributs.iter().find(|(a, _)| a == nom).map(|(_, v)| v.trim());
    valeur("width") == Some("100%")
        || declarations(valeur("style").unwrap_or("")).any(|(propriete, v)| propriete == "width" && v == "100%")
}

/// Largeur fixe d'une cellule, en pixels : celle de son style, sinon celle de
/// son attribut.
fn largeur_fixe(attributs: &[(String, String)]) -> Option<u32> {
    let valeur = |nom: &str| attributs.iter().find(|(a, _)| a == nom).map(|(_, v)| v.as_str());
    match declarations(valeur("style").unwrap_or("")).filter(|(propriete, _)| propriete == "width").last() {
        Some((_, v)) => pixels(v),
        None => valeur("width").and_then(pixels),
    }
}

/// Ce que devient une ligne d'un tableau gardé à pleine largeur dont toutes les
/// cellules ont une largeur fixe : ces largeurs deviennent leurs parts de la
/// ligne. Un navigateur répartit ainsi la place qui reste ; le moteur de Qt
/// s'en tenait à leur somme, et le tableau, plus étroit que son encadré, y
/// laissait deux pans blancs (ligne du prix d'une lettre d'information du 29/10/2026).
fn repartir(noeud: Noeud) -> Noeud {
    let Noeud::Element { nom, attributs, enfants } = noeud else {
        return noeud;
    };
    fn cellule(noeud: &Noeud) -> bool {
        matches!(noeud, Noeud::Element { nom, .. } if nom == "td" || nom == "th")
    }
    match nom.as_str() {
        "tbody" | "thead" | "tfoot" => {
            Noeud::Element { nom, attributs, enfants: enfants.into_iter().map(repartir).collect() }
        }
        "tr" => {
            let largeurs: Option<Vec<u32>> = enfants
                .iter()
                .filter(|enfant| cellule(enfant))
                .map(|enfant| match enfant {
                    Noeud::Element { attributs, .. } => largeur_fixe(attributs).filter(|&l| l > 0),
                    Noeud::Texte(_) => None,
                })
                .collect();
            let Some(largeurs) = largeurs.filter(|l| l.len() > 1) else {
                return Noeud::Element { nom, attributs, enfants };
            };
            let mut parts = parts(&largeurs).into_iter();
            let enfants = enfants
                .into_iter()
                .map(|enfant| match enfant {
                    Noeud::Element { nom, attributs, enfants } if nom == "td" || nom == "th" => {
                        let part = parts.next().unwrap_or(0);
                        Noeud::Element { nom, attributs: en_part(attributs, part), enfants }
                    }
                    autre => autre,
                })
                .collect();
            Noeud::Element { nom, attributs, enfants }
        }
        _ => Noeud::Element { nom, attributs, enfants },
    }
}

/// Parts entières, en pour cent, de largeurs fixes : 1 au moins chacune, le
/// reste de l'arrondi à la plus large.
fn parts(largeurs: &[u32]) -> Vec<u32> {
    let somme: u64 = largeurs.iter().map(|&l| u64::from(l)).sum();
    let mut parts: Vec<u32> = largeurs.iter().map(|&l| ((u64::from(l) * 100 / somme) as u32).max(1)).collect();
    let total: u32 = parts.iter().sum();
    if let Some(plus_large) = (0..largeurs.len()).max_by_key(|&i| largeurs[i]) {
        parts[plus_large] = (parts[plus_large] + 100).saturating_sub(total).max(1);
    }
    parts
}

/// Attributs d'une cellule dont la largeur devient `part` pour cent de la
/// ligne : attribut `width` remplacé à sa place, largeurs du style retirées.
fn en_part(attributs: Vec<(String, String)>, part: u32) -> Vec<(String, String)> {
    let largeur = format!("{part}%");
    let mut posee = false;
    let mut attributs: Vec<(String, String)> = attributs
        .into_iter()
        .filter_map(|(attribut, valeur)| match attribut.as_str() {
            "width" => {
                posee = true;
                Some((attribut, largeur.clone()))
            }
            "style" => {
                let style = sans_proprietes(&valeur, &["width", "min-width", "max-width"]);
                (!style.is_empty()).then_some((attribut, style))
            }
            _ => Some((attribut, valeur)),
        })
        .collect();
    if !posee {
        attributs.push(("width".to_string(), largeur));
    }
    attributs
}

/// Tableau en `dir="rtl"` dont les cellules se déclarent `direction:ltr` :
/// l'astuce des lettres d'information qui inverse l'ordre des colonnes sur un
/// téléphone. Le moteur de Qt applique la direction au texte des cellules
/// (« -67% » devenait « 67%- ») sans inverser les colonnes.
fn colonnes_inversees(attributs: &[(String, String)], enfants: &[Noeud]) -> bool {
    fn direction(attributs: &[(String, String)], sens: &str) -> bool {
        attributs.iter().any(|(a, v)| a == "dir" && v.trim().eq_ignore_ascii_case(sens))
            || attributs.iter().filter(|(a, _)| a == "style").any(|(_, style)| {
                declarations(style).any(|(propriete, v)| propriete == "direction" && v.eq_ignore_ascii_case(sens))
            })
    }
    direction(attributs, "rtl")
        && lignes(enfants).iter().flat_map(|ligne| cellules(ligne)).any(|cellule| match cellule {
            Noeud::Element { attributs, .. } => direction(attributs, "ltr"),
            Noeud::Texte(_) => false,
        })
}

/// Attributs sans direction : ni `dir`, ni `direction` dans le style.
fn sans_direction(attributs: Vec<(String, String)>) -> Vec<(String, String)> {
    attributs
        .into_iter()
        .filter(|(attribut, _)| attribut != "dir")
        .filter_map(|(attribut, valeur)| {
            if attribut != "style" {
                return Some((attribut, valeur));
            }
            let style = sans_proprietes(&valeur, &["direction"]);
            (!style.is_empty()).then_some((attribut, style))
        })
        .collect()
}

/// Cellule fusionnée sur plusieurs colonnes.
fn fusionnee(attributs: &[(String, String)]) -> bool {
    attributs.iter().any(|(a, v)| a == "colspan" && v.trim().parse::<u32>().is_ok_and(|n| n > 1))
}

/// Attributs sans largeur : ni attribut `width`, ni `width` ou ses bornes dans
/// le style.
fn sans_largeur(attributs: Vec<(String, String)>) -> Vec<(String, String)> {
    attributs
        .into_iter()
        .filter(|(attribut, _)| attribut != "width")
        .filter_map(|(attribut, valeur)| {
            if attribut != "style" {
                return Some((attribut, valeur));
            }
            let style = sans_proprietes(&valeur, &["width", "min-width", "max-width"]);
            (!style.is_empty()).then_some((attribut, style))
        })
        .collect()
}

/// Couleur de fond d'un élément : son attribut `bgcolor` (déjà validé par
/// `ammonia`), sinon la dernière déclaration `background-color` ou
/// `background` de son style qui en nomme une. `transparent` n'en est pas une.
fn fond_de(attributs: &[(String, String)]) -> Option<String> {
    let valeur = |nom: &str| attributs.iter().find(|(a, _)| a == nom).map(|(_, v)| v.as_str());
    let couleur = valeur("bgcolor").map(str::to_string).or_else(|| {
        declarations(valeur("style").unwrap_or(""))
            .filter(|(propriete, _)| propriete == "background-color" || propriete == "background")
            .filter_map(|(_, v)| v.split_whitespace().find_map(crate::rendu::couleur))
            .last()
    })?;
    (!couleur.eq_ignore_ascii_case("transparent")).then_some(couleur)
}

/// Vrai pour le blanc : sur la page blanche du volet de lecture, un fond blanc
/// ne laisse voir aucun interligne.
fn blanc(couleur: &str) -> bool {
    matches!(couleur.to_ascii_lowercase().as_str(), "#fff" | "#ffffff" | "white")
}

/// Attributs dont le style perd ses `line-height` : tous sous un fond, les nuls
/// partout (`line-height:0`, qui ne sert qu'aux espaceurs pour Outlook). Un
/// style dont rien n'est retiré reste tel qu'écrit.
fn sans_interligne(attributs: Vec<(String, String)>, sous_fond: bool) -> Vec<(String, String)> {
    attributs
        .into_iter()
        .filter_map(|(attribut, valeur)| {
            if attribut != "style" {
                return Some((attribut, valeur));
            }
            let retire = |declaration: &str| {
                let (propriete, v) = declaration.split_once(':').unwrap_or((declaration, ""));
                propriete.trim().eq_ignore_ascii_case("line-height") && (sous_fond || nul(v))
            };
            if !valeur.split(';').any(retire) {
                return Some((attribut, valeur));
            }
            let style = valeur
                .split(';')
                .filter(|declaration| !declaration.trim().is_empty() && !retire(declaration))
                .map(str::trim)
                .collect::<Vec<_>>()
                .join(";");
            (!style.is_empty()).then_some((attribut, style))
        })
        .collect()
}

/// Déclarations d'un style : propriété en minuscules, valeur sans
/// `!important`.
fn declarations(style: &str) -> impl Iterator<Item = (String, &str)> {
    style.split(';').filter_map(|declaration| {
        let (propriete, valeur) = declaration.split_once(':')?;
        let valeur = valeur.trim();
        Some((propriete.trim().to_ascii_lowercase(), valeur.strip_suffix("!important").unwrap_or(valeur).trim()))
    })
}

/// Longueur nulle : « 0 », « 0pt », « 0px », « 0% ».
fn nul(valeur: &str) -> bool {
    let valeur = valeur.trim();
    let valeur = valeur.strip_suffix("!important").unwrap_or(valeur).trim();
    valeur
        .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%')
        .parse::<f32>()
        .is_ok_and(|v| v == 0.0)
}

/// Cellule sans autre contenu que des blancs, d'au plus `ESPACEUR_MAX` pixels
/// de haut ou en police nulle : un filet, un espaceur. Le moteur de Qt ne fait
/// pas de bloc plus bas qu'une ligne de texte — le filet de 1 pixel d'un avis
/// de Steam devenait une bande grise de toute la largeur.
fn espaceur(attributs: &[(String, String)], enfants: &[Noeud]) -> bool {
    let vide = enfants.iter().all(|enfant| match enfant {
        Noeud::Texte(texte) => texte.replace("&nbsp;", " ").replace("&#160;", " ").trim().is_empty(),
        Noeud::Element { .. } => false,
    });
    let valeur = |nom: &str| attributs.iter().find(|(a, _)| a == nom).map(|(_, v)| v.as_str());
    let basse = valeur("height")
        .into_iter()
        .chain(
            declarations(valeur("style").unwrap_or(""))
                .filter(|(propriete, _)| propriete == "height")
                .map(|(_, v)| v),
        )
        .any(|hauteur| pixels(hauteur).is_some_and(|h| h <= ESPACEUR_MAX));
    vide && (basse || police_nulle(attributs))
}

/// Police nulle : `font-size` à 0 dans le style.
fn police_nulle(attributs: &[(String, String)]) -> bool {
    let style = attributs.iter().find(|(a, _)| a == "style").map_or("", |(_, v)| v.as_str());
    declarations(style).any(|(propriete, v)| propriete == "font-size" && nul(v))
}

/// Élément en police nulle sans autre contenu que du texte : rien ne s'en voit
/// ailleurs, mais le moteur de Qt ignore la police nulle — le point qui garde
/// ouverte, pour Outlook, la cellule d'un espaceur s'y affichait.
fn muet(attributs: &[(String, String)], enfants: &[Noeud]) -> bool {
    police_nulle(attributs) && enfants.iter().all(|enfant| matches!(enfant, Noeud::Texte(_)))
}

/// Lignes d'un tableau : directement sous lui, ou sous `tbody`, `thead`,
/// `tfoot`.
fn lignes(enfants: &[Noeud]) -> Vec<&Noeud> {
    let mut lignes = Vec::new();
    for enfant in enfants {
        if let Noeud::Element { nom, enfants: sous, .. } = enfant {
            match nom.as_str() {
                "tr" => lignes.push(enfant),
                "tbody" | "thead" | "tfoot" => {
                    lignes.extend(sous.iter().filter(|n| matches!(n, Noeud::Element { nom, .. } if nom == "tr")))
                }
                _ => {}
            }
        }
    }
    lignes
}

fn cellules(ligne: &Noeud) -> Vec<&Noeud> {
    match ligne {
        Noeud::Element { enfants, .. } => enfants
            .iter()
            .filter(|n| matches!(n, Noeud::Element { nom, .. } if nom == "td" || nom == "th"))
            .collect(),
        Noeud::Texte(_) => Vec::new(),
    }
}

/// Ce que devient un enfant d'un tableau aplati.
fn en_blocs(noeud: Noeud) -> Vec<Noeud> {
    let Noeud::Element { nom, attributs, enfants } = noeud else {
        // Blancs entre les lignes : le texte d'un tableau a déjà été sorti
        // devant lui par l'analyseur HTML5.
        return Vec::new();
    };
    match nom.as_str() {
        "tbody" | "thead" | "tfoot" => enfants.into_iter().flat_map(en_blocs).collect(),
        "tr" => {
            let contenu: Vec<Noeud> = enfants.into_iter().flat_map(en_blocs).collect();
            // Une ligne ne porte un bloc que si elle a un style à transmettre,
            // et un contenu : vide, son fond ferait une ligne de texte.
            if !contenu.is_empty() && attributs.iter().any(|(a, _)| a == "style" || a == "bgcolor") {
                vec![bloc(&attributs, contenu)]
            } else {
                contenu
            }
        }
        "td" | "th" | "caption" if espaceur(&attributs, &enfants) => Vec::new(),
        "td" | "th" | "caption" => vec![bloc(&attributs, enfants)],
        // `colgroup`, `col` : des largeurs de colonnes, sans objet.
        _ => Vec::new(),
    }
}

/// Bloc qui remplace un tableau, une ligne ou une cellule : son style moins
/// les dimensions, plus le fond et l'alignement que portaient ses attributs.
fn bloc(attributs: &[(String, String)], enfants: Vec<Noeud>) -> Noeud {
    let valeur = |nom: &str| attributs.iter().find(|(a, _)| a == nom).map(|(_, v)| v.as_str());
    let mut style = sans_dimensions(valeur("style").unwrap_or(""));
    if let Some(fond) = valeur("bgcolor") {
        ajouter(&mut style, &format!("background-color:{fond}"));
    }
    if let Some(alignement) = valeur("align") {
        ajouter(&mut style, &format!("text-align:{alignement}"));
    }
    let mut repris: Vec<(String, String)> =
        attributs.iter().filter(|(a, _)| ATTRIBUTS_REPRIS.contains(&a.as_str())).cloned().collect();
    if !style.is_empty() {
        repris.insert(0, ("style".to_string(), style));
    }
    Noeud::Element { nom: "div".to_string(), attributs: repris, enfants }
}

fn ajouter(style: &mut String, declaration: &str) {
    if !style.is_empty() && !style.ends_with(';') {
        style.push(';');
    }
    style.push_str(declaration);
}

/// Un style sans `width`, `height` ni leurs bornes.
fn sans_dimensions(style: &str) -> String {
    sans_proprietes(style, &["width", "min-width", "max-width", "height", "min-height", "max-height"])
}

/// Un style sans les propriétés nommées.
fn sans_proprietes(style: &str, proprietes: &[&str]) -> String {
    style
        .split(';')
        .filter(|declaration| {
            let propriete = declaration.split(':').next().unwrap_or("").trim().to_ascii_lowercase();
            !declaration.trim().is_empty() && !proprietes.contains(&propriete.as_str())
        })
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(";")
}

/// Attributs d'un tableau gardé, une largeur fixe d'au moins
/// `LARGEUR_PLEINE` pixels devenue la pleine largeur.
fn pleine_largeur(attributs: Vec<(String, String)>) -> Vec<(String, String)> {
    attributs
        .into_iter()
        .map(|(attribut, valeur)| match attribut.as_str() {
            "width" if pixels(&valeur).is_some_and(|l| l >= LARGEUR_PLEINE) => (attribut, "100%".to_string()),
            "style" => {
                let style = valeur
                    .split(';')
                    .map(|declaration| match declaration.split_once(':') {
                        Some((propriete, v))
                            if propriete.trim().eq_ignore_ascii_case("width")
                                && pixels(v.trim().trim_end_matches("!important").trim())
                                    .is_some_and(|l| l >= LARGEUR_PLEINE) =>
                        {
                            "width:100%".to_string()
                        }
                        _ => declaration.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(";");
                (attribut, style)
            }
            _ => (attribut, valeur),
        })
        .collect()
}

/// Largeur en pixels : « 600 », « 600px ».
fn pixels(valeur: &str) -> Option<u32> {
    let valeur = valeur.trim();
    valeur.strip_suffix("px").unwrap_or(valeur).trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tableaux_d_une_colonne_aplatis() {
        let html = "<table align=\"center\" style=\"width:100%;background:black\"><tbody><tr>\
                    <td style=\"padding:20px;width:600px\" align=\"center\">\
                    <table width=\"100%\"><tbody><tr><td class=\"t\">Bonjour</td></tr></tbody></table>\
                    </td></tr></tbody></table>";
        assert_eq!(
            aplatir(html),
            "<div style=\"background:black\"><div style=\"padding:20px;text-align:center\">\
             <div><div class=\"t\">Bonjour</div></div></div></div>"
        );
    }

    #[test]
    fn tableau_a_plusieurs_colonnes_garde() {
        let html = "<table width=\"600\" style=\"width: 600px; border:0\"><tbody>\
                    <tr><td>Statut</td><td>Tâche</td></tr></tbody></table>";
        assert_eq!(
            aplatir(html),
            "<table width=\"100%\" style=\"width:100%; border:0\"><tbody>\
             <tr><td>Statut</td><td>Tâche</td></tr></tbody></table>"
        );
        // Une petite largeur fixe reste : une vignette, une colonne étroite.
        let petite = "<table width=\"105\"><tbody><tr><td>a</td><td>b</td></tr></tbody></table>";
        assert_eq!(aplatir(petite), petite);
    }

    #[test]
    fn tableau_imbrique_dans_un_tableau_a_colonnes() {
        // L'intérieur s'aplatit, l'extérieur reste.
        let html = "<table><tbody><tr><td><table><tbody><tr><td>x</td></tr></tbody></table></td>\
                    <td>y</td></tr></tbody></table>";
        assert_eq!(
            aplatir(html),
            "<table><tbody><tr><td><div><div>x</div></div></td><td>y</td></tr></tbody></table>"
        );
    }

    #[test]
    fn fond_de_ligne_et_attributs_repris() {
        let html = "<table><tbody><tr bgcolor=\"#eee\"><td dir=\"rtl\" lang=\"ar\" valign=\"top\" \
                    height=\"16\">x</td></tr></tbody></table>";
        assert_eq!(
            aplatir(html),
            "<div><div style=\"background-color:#eee\"><div dir=\"rtl\" lang=\"ar\">x</div></div></div>"
        );
    }

    #[test]
    fn interligne_retire_sous_un_fond() {
        // Avis de Steam du 27/10/2026 : l'interligne restait hors du fond, une
        // bande blanche sous chaque bloc.
        let html = "<table bgcolor=\"#212429\"><tbody><tr><td style=\"font-size:18px; line-height:25px; \
                    color:#f1f1f1\">x <span style=\"line-height:1.3\">y</span></td></tr></tbody></table>";
        assert_eq!(
            aplatir(html),
            "<div style=\"background-color:#212429\"><div style=\"font-size:18px;color:#f1f1f1\">\
             x <span>y</span></div></div>"
        );
        // Sur du blanc, il ne se voit pas : l'interligne demeure, son style
        // tel qu'écrit.
        let blanc = "<table bgcolor=\"#ffffff\"><tbody><tr><td><p style=\"line-height: 25px\">x</p>\
                     </td></tr></tbody></table>";
        assert_eq!(
            aplatir(blanc),
            "<div style=\"background-color:#ffffff\"><div><p style=\"line-height: 25px\">x</p></div></div>"
        );
        // Fond d'un style, et interligne nul retiré partout (le texte en police
        // nulle aussi, invisible ailleurs).
        let style = "<div style=\"background: none #333 \"><p style=\"line-height:20px\">x</p></div>\
                     <div style=\"font-size:0pt; line-height:0pt\">y</div>";
        assert_eq!(
            aplatir(style),
            "<div style=\"background: none #333 \"><p>x</p></div><div style=\"font-size:0pt\"></div>"
        );
    }

    #[test]
    fn filet_et_espaceur_ecartes() {
        // Le filet de Steam : 210 px de large, 1 px de haut, une espace insécable.
        let html = "<table width=\"210\" bgcolor=\"#727483\"><tbody><tr><td height=\"1\" \
                    style=\"font-size:0pt; line-height:0pt\">&nbsp;</td></tr></tbody></table><p>y</p>";
        assert_eq!(aplatir(html), "<p>y</p>");
        let police_nulle = "<table><tbody><tr style=\"background:#ccc\"><td style=\"font-size:0\"> </td></tr>\
                            </tbody></table>";
        assert_eq!(aplatir(police_nulle), "");
        // Un espace de 20 px, ou une cellule de 1 px qui porte un texte, reste.
        let espace = "<table><tbody><tr><td height=\"20\">&nbsp;</td></tr></tbody></table>";
        assert_eq!(aplatir(espace), "<div><div>&nbsp;</div></div>");
        let texte = "<table><tbody><tr><td height=\"1\">x</td></tr></tbody></table>";
        assert_eq!(aplatir(texte), "<div><div>x</div></div>");
        // Le point d'un espaceur en police nulle : la cellule aplatie disparaît,
        // celle d'un tableau gardé reste, vidée.
        let point = "<table><tbody><tr><td style=\"font-size:0\" height=\"20\">.</td></tr></tbody></table><p>y</p>";
        assert_eq!(aplatir(point), "<p>y</p>");
        let garde = "<table><tbody><tr><td>a</td><td style=\"font-size: 0; width: 12px\" width=\"12\">.</td>\
                     <td style=\"font-size: 0\"><div style=\"height:1px;font-size:0\"> . </div></td>\
                     </tr></tbody></table>";
        assert_eq!(
            aplatir(garde),
            "<table><tbody><tr><td>a</td><td style=\"font-size: 0; width: 12px\" width=\"12\"></td>\
             <td style=\"font-size: 0\"><div style=\"height:1px;font-size:0\"></div></td>\
             </tr></tbody></table>"
        );
    }

    #[test]
    fn cellule_fusionnee_sans_largeur() {
        // Lettre d'information du 29/10/2026 : la ligne d'espace, fusionnée sur
        // les colonnes, fixait 600 px que Qt reportait sur elles ; le titre de
        // la colonne de 310 px tenait en quelques lettres par ligne. Les
        // largeurs fixes de la ligne suivante deviennent ses parts (le tableau
        // est à pleine largeur, sur son propre fond).
        let html = "<table width=\"100%\" bgcolor=\"#fff3f0\"><tbody><tr><td width=\"600\" height=\"32\" \
                    colspan=\"3\" style=\"width: 600px; height: 32px;\"></td></tr><tr><td width=\"135\">a</td>\
                    <td width=\"35\"></td><td width=\"310\">b</td></tr></tbody></table>";
        assert_eq!(
            aplatir(html),
            "<table width=\"100%\" bgcolor=\"#fff3f0\"><tbody><tr><td height=\"32\" colspan=\"3\" \
             style=\"height: 32px\"></td></tr><tr><td width=\"28%\">a</td><td width=\"7%\"></td>\
             <td width=\"65%\">b</td></tr></tbody></table>"
        );
    }

    #[test]
    fn colonnes_fixes_en_parts_de_la_ligne() {
        // Ligne du prix d'une lettre d'information du 29/10/2026 : 220 + 25 + 365 px
        // dans un tableau à pleine largeur, que Qt dessinait à 610 px entre
        // deux pans blancs, dans un encadré sombre. Largeur du style comprise,
        // reste de l'arrondi à la plus large.
        let ligne = "<table width=\"100%\"><tbody><tr><th width=\"220\">a</th><th width=\"25\"></th>\
                     <th width=\"365\" style=\"width:365px; color:#3999ec\">b</th></tr></tbody></table>";
        let html = format!("<table bgcolor=\"#212429\"><tbody><tr><td>{ligne}</td></tr></tbody></table>");
        assert_eq!(
            aplatir(&html),
            "<div style=\"background-color:#212429\"><div><table width=\"100%\" bgcolor=\"#212429\"><tbody><tr>\
             <th width=\"36%\">a</th><th width=\"4%\"></th><th width=\"60%\" style=\"color:#3999ec\">b</th>\
             </tr></tbody></table></div></div>"
        );
        // Sur le blanc de la page, une colonne sans largeur, un tableau qui
        // n'est pas à pleine largeur : rien ne change.
        for html in [
            ligne,
            "<table width=\"100%\"><tbody><tr><td width=\"3\" bgcolor=\"#3a9aed\"></td><td>a</td></tr></tbody></table>",
            "<table width=\"210\"><tbody><tr><td width=\"100\">a</td><td width=\"110\">b</td></tr></tbody></table>",
        ] {
            assert_eq!(aplatir(html), html);
        }
        assert_eq!(parts(&[3, 600]), vec![1, 99]);
    }

    #[test]
    fn colonnes_inversees_remises_dans_le_sens_du_texte() {
        // Même avis : `dir="rtl"` inversait les colonnes sur téléphone ; Qt
        // l'appliquait au texte, « -67% » devenait « 67%- ».
        let html = "<table width=\"100%\" dir=\"rtl\" style=\"direction:rtl;\"><tbody><tr>\
                    <th style=\"direction:ltr\">-67%</th><th style=\"direction:ltr\">b</th></tr></tbody></table>";
        assert_eq!(
            aplatir(html),
            "<table width=\"100%\"><tbody><tr>\
             <th style=\"direction:ltr\">-67%</th><th style=\"direction:ltr\">b</th></tr></tbody></table>"
        );
        // Un texte qui s'écrit vraiment de droite à gauche garde sa direction.
        let arabe = "<table dir=\"rtl\"><tbody><tr><td>أ</td><td>ب</td></tr></tbody></table>";
        assert_eq!(aplatir(arabe), arabe);
    }

    #[test]
    fn interligne_des_cellules_gardees() {
        // Même lettre : des vignettes en interligne nul, dont l'image recouvrait
        // le texte suivant, et un bouton noir dont le libellé débordait.
        let html = "<table><tbody><tr><td style=\"line-height: 0; padding-top: 12px;\">a</td>\
                    <td bgcolor=\"#000000\"><table><tbody><tr><td style=\"font-size: 28px; line-height: 42px\">\
                    Voir plus</td></tr></tbody></table></td></tr></tbody></table>";
        assert_eq!(
            aplatir(html),
            "<table><tbody><tr><td style=\"padding-top: 12px\">a</td><td bgcolor=\"#000000\">\
             <div><div style=\"font-size: 28px\">Voir plus</div></div></td></tr></tbody></table>"
        );
    }

    #[test]
    fn fond_transmis_au_tableau_garde() {
        // La signature de Steam : barre bleue, espace, texte clair, dans un
        // encadré sombre aplati. Sans le fond, le texte clair restait sur blanc.
        let html = "<table bgcolor=\"#212429\"><tbody><tr><td><table width=\"100%\"><tbody><tr>\
                    <td width=\"3\" bgcolor=\"#3a9aed\"></td><td style=\"line-height:22px\">Cordialement</td>\
                    </tr></tbody></table></td></tr></tbody></table>";
        assert_eq!(
            aplatir(html),
            "<div style=\"background-color:#212429\"><div><table width=\"100%\" bgcolor=\"#212429\"><tbody><tr>\
             <td width=\"3\" bgcolor=\"#3a9aed\"></td><td style=\"line-height:22px\">Cordialement</td>\
             </tr></tbody></table></div></div>"
        );
        // Un tableau qui a son fond le garde ; sur la page, aucun n'est ajouté.
        let propre = "<div style=\"background-color:#000\"><table bgcolor=\"#fff\"><tbody><tr><td>a</td>\
                      <td>b</td></tr></tbody></table></div>";
        assert_eq!(aplatir(propre), propre);
        let page = "<table><tbody><tr><td>a</td><td>b</td></tr></tbody></table>";
        assert_eq!(aplatir(page), page);
    }

    #[test]
    fn elements_vides_et_texte_conserves() {
        let html = "<style>a > b { color: red }</style><p>Un<br>deux <img src=\"x\" width=\"32\"> &lt;trois&gt;</p>";
        assert_eq!(aplatir(html), html);
    }

    #[test]
    fn html_mal_forme_rendu_tel_quel() {
        for html in ["<div><p>x</div>", "<div>x", "x</div>", "<table><tr><td>x</td>"] {
            assert_eq!(aplatir(html), html);
        }
    }

    #[test]
    fn imbrication_hostile_rendue_telle_quelle() {
        // Cent mille niveaux, bien formés : sans borne, la récursion de
        // `transformer` arrêtait le programme.
        let html = format!("{}x{}", "<div>".repeat(100_000), "</div>".repeat(100_000));
        assert_eq!(aplatir(&html), html);
        // Une imbrication ordinaire est toujours traitée.
        let html = format!("{}<table><tr><td>x</td></tr></table>{}", "<div>".repeat(50), "</div>".repeat(50));
        assert!(!aplatir(&html).contains("<table"));
    }
}
