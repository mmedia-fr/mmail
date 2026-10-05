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
    for noeud in noeuds.into_iter().flat_map(transformer) {
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
/// nœud lui-même.
fn transformer(noeud: Noeud) -> Vec<Noeud> {
    let Noeud::Element { nom, attributs, enfants } = noeud else {
        return vec![noeud];
    };
    let enfants: Vec<Noeud> = enfants.into_iter().flat_map(transformer).collect();
    if nom != "table" {
        return vec![Noeud::Element { nom, attributs, enfants }];
    }
    let lignes = lignes(&enfants);
    if lignes.iter().any(|ligne| cellules(ligne).len() > 1) {
        return vec![Noeud::Element { nom, attributs: pleine_largeur(attributs), enfants }];
    }
    let blocs: Vec<Noeud> = enfants.into_iter().flat_map(en_blocs).collect();
    // L'alignement d'un tableau le centre dans la page, il n'aligne pas son
    // texte : un bloc occupant toute la largeur, il est sans objet.
    let attributs: Vec<(String, String)> = attributs.into_iter().filter(|(a, _)| a != "align").collect();
    vec![bloc(&attributs, blocs)]
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
            // Une ligne ne porte un bloc que si elle a un style à transmettre.
            if attributs.iter().any(|(a, _)| a == "style" || a == "bgcolor") {
                vec![bloc(&attributs, contenu)]
            } else {
                contenu
            }
        }
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
    style
        .split(';')
        .filter(|declaration| {
            let propriete = declaration.split(':').next().unwrap_or("").trim().to_ascii_lowercase();
            !declaration.trim().is_empty()
                && !matches!(
                    propriete.as_str(),
                    "width" | "min-width" | "max-width" | "height" | "min-height" | "max-height"
                )
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
