// SPDX-License-Identifier: GPL-3.0-or-later
//! Message lu sans ses grosses pièces jointes, pour l'afficher (#179).
//!
//! Un message que le poste ne garde pas (reçu il y a plus d'un mois, ou
//! au-delà du plafond de taille) se relit sur le serveur à chaque ouverture.
//! Lu entier, il apporte ses pièces jointes, qu'Exchange Online convertit et
//! transmet à chaque fois : plusieurs secondes avant l'affichage. Lu par
//! parties, il n'apporte que ce qui s'affiche : sa structure (`BODYSTRUCTURE`),
//! ses en-têtes, ses textes et ses images intégrées. Les grosses pièces jointes
//! y gardent leurs en-têtes et un corps vide, si bien que leur liste et leurs
//! rangs restent ceux du message entier ; les ouvrir ou les enregistrer relit
//! alors le message entier (`message_entier`).
//!
//! Le message recomposé ne sert qu'à l'affichage : il n'est ni gardé sur le
//! poste, ni retenu comme dernier message lu, ni repris pour une réponse ou un
//! transfert, qui relisent le message entier.

use std::collections::HashMap;

use crate::protocole::MARQUEUR;

/// En dessous de cette taille, un message se lit entier : une commande vaut
/// mieux que deux.
pub const TAILLE_MIN: u32 = 512 * 1024;
/// Une pièce jointe plus petite se lit avec le reste.
const PARTIE_MIN: u64 = 64 * 1024;
/// Économie en deçà de laquelle le message se lit entier.
const GAIN_MIN: u64 = 256 * 1024;
/// En-tête posé sur une pièce laissée sur le serveur : sa taille d'origine,
/// décodée, pour la liste des pièces jointes (`index::pieces_jointes`).
pub const ENTETE_TAILLE: &str = "X-MMail-Taille-Origine";

/// Une valeur de réponse IMAP : `NIL`, une chaîne (atome, nombre, chaîne entre
/// guillemets ou littéral) ou une liste entre parenthèses.
#[derive(Debug, Clone, PartialEq)]
enum Valeur {
    Nil,
    Texte(Vec<u8>),
    Liste(Vec<Valeur>),
}

impl Valeur {
    fn texte(&self) -> Option<String> {
        match self {
            Valeur::Texte(t) => Some(String::from_utf8_lossy(t).into_owned()),
            _ => None,
        }
    }
}

/// Lit une valeur en tête de `texte`, littéraux remplacés par leur marqueur
/// (cf. `Client::commande`) ; rend aussi ce qui la suit.
fn lire<'a>(texte: &'a str, litteraux: &[Vec<u8>]) -> Option<(Valeur, &'a str)> {
    let texte = texte.trim_start_matches(' ');
    let premier = texte.chars().next()?;
    if premier == '(' {
        let mut reste = &texte[1..];
        let mut liste = Vec::new();
        loop {
            reste = reste.trim_start_matches(' ');
            if let Some(apres) = reste.strip_prefix(')') {
                return Some((Valeur::Liste(liste), apres));
            }
            let (valeur, apres) = lire(reste, litteraux)?;
            liste.push(valeur);
            reste = apres;
        }
    }
    if premier == '"' {
        let mut octets = Vec::new();
        let mut echappe = false;
        let mut tampon = [0u8; 4];
        for (i, c) in texte.char_indices().skip(1) {
            if !echappe && c == '\\' {
                echappe = true;
                continue;
            }
            if !echappe && c == '"' {
                return Some((Valeur::Texte(octets), &texte[i + 1..]));
            }
            echappe = false;
            octets.extend_from_slice(c.encode_utf8(&mut tampon).as_bytes());
        }
        return None;
    }
    if premier == MARQUEUR {
        let apres = &texte[MARQUEUR.len_utf8()..];
        let fin = apres.find(MARQUEUR)?;
        let indice: usize = apres[..fin].parse().ok()?;
        let octets = litteraux.get(indice)?.clone();
        return Some((Valeur::Texte(octets), &apres[fin + MARQUEUR.len_utf8()..]));
    }
    let fin = texte.find(|c: char| matches!(c, ' ' | '(' | ')' | '\r' | '\n')).unwrap_or(texte.len());
    if fin == 0 {
        return None;
    }
    let atome = &texte[..fin];
    let valeur = if atome.eq_ignore_ascii_case("NIL") { Valeur::Nil } else { Valeur::Texte(atome.as_bytes().to_vec()) };
    Some((valeur, &texte[fin..]))
}

/// Une partie du message, d'après sa structure.
#[derive(Debug, Clone, PartialEq)]
pub struct Partie {
    /// Désignation IMAP : « 1 », « 2.1 »… ; vide pour le message lui-même.
    pub section: String,
    /// Type MIME en minuscules : « multipart/mixed », « application/pdf »…
    pub genre: String,
    /// Taille encodée, telle que le serveur l'annonce.
    pub taille: u64,
    /// Encodage de transfert, en minuscules.
    pub encodage: String,
    /// La partie porte un `Content-ID` : le corps HTML peut l'afficher.
    pub avec_id: bool,
    /// Parties d'un `multipart`.
    pub enfants: Vec<Partie>,
}

impl Partie {
    fn multiple(&self) -> bool {
        self.genre.starts_with("multipart/")
    }

    /// Pièce laissée sur le serveur : ni texte, ni message joint, ni image que
    /// le corps peut afficher, et assez grosse pour que cela compte.
    fn laissee(&self) -> bool {
        !self.multiple()
            && !self.genre.starts_with("text/")
            && !self.genre.starts_with("message/")
            && !self.avec_id
            && self.taille >= PARTIE_MIN
    }

    /// Octets que la lecture par parties ne transfère pas.
    fn economie(&self) -> u64 {
        if self.multiple() {
            self.enfants.iter().map(Partie::economie).sum()
        } else if self.laissee() {
            self.taille
        } else {
            0
        }
    }

    /// Taille décodée, à peu près : le base64 range 57 octets en 78 signes,
    /// 76 et le saut de ligne.
    fn taille_decodee(&self) -> u64 {
        if self.encodage == "base64" {
            self.taille * 57 / 78
        } else {
            self.taille
        }
    }
}

fn sous_section(parent: &str, rang: usize) -> String {
    if parent.is_empty() {
        rang.to_string()
    } else {
        format!("{parent}.{rang}")
    }
}

fn partie(valeur: &Valeur, section: String) -> Option<Partie> {
    let Valeur::Liste(elements) = valeur else {
        return None;
    };
    if matches!(elements.first(), Some(Valeur::Liste(_))) {
        let mut enfants = Vec::new();
        for element in elements {
            if !matches!(element, Valeur::Liste(_)) {
                break;
            }
            let rang = enfants.len() + 1;
            enfants.push(partie(element, sous_section(&section, rang))?);
        }
        let sous_type = elements.get(enfants.len())?.texte()?.to_ascii_lowercase();
        return Some(Partie {
            section,
            genre: format!("multipart/{sous_type}"),
            taille: 0,
            encodage: String::new(),
            avec_id: false,
            enfants,
        });
    }
    let genre = format!("{}/{}", elements.first()?.texte()?, elements.get(1)?.texte()?).to_ascii_lowercase();
    let avec_id = elements.get(3).and_then(Valeur::texte).is_some_and(|id| !id.trim().is_empty());
    let encodage = elements.get(5).and_then(Valeur::texte).unwrap_or_default().to_ascii_lowercase();
    let taille = elements.get(6)?.texte()?.trim().parse().ok()?;
    Some(Partie { section, genre, taille, encodage, avec_id, enfants: Vec::new() })
}

/// Structure du message d'après une réponse `FETCH … BODYSTRUCTURE`.
pub fn analyser(reponse: &str, litteraux: &[Vec<u8>]) -> Option<Partie> {
    let debut = reponse.find("BODYSTRUCTURE ")? + "BODYSTRUCTURE ".len();
    let (valeur, _) = lire(&reponse[debut..], litteraux)?;
    partie(&valeur, String::new())
}

/// Sections à lire pour recomposer le message sans ses grosses pièces
/// jointes ; `None` : il se lit entier — il n'est pas en plusieurs parties, ou
/// il y a trop peu à y gagner.
pub fn sections(structure: &Partie) -> Option<Vec<String>> {
    fn parcourir(p: &Partie, sections: &mut Vec<String>) {
        for enfant in &p.enfants {
            sections.push(format!("{}.MIME", enfant.section));
            if enfant.multiple() {
                parcourir(enfant, sections);
            } else if !enfant.laissee() {
                sections.push(enfant.section.clone());
            }
        }
    }
    if !structure.multiple() || structure.economie() < GAIN_MIN {
        return None;
    }
    let mut sections = vec!["HEADER".to_string()];
    parcourir(structure, &mut sections);
    Some(sections)
}

/// Contenu de chaque section d'une réponse `FETCH … BODY[…]`, par section en
/// capitales (« HEADER », « 1.MIME », « 2 »).
pub fn contenus(reponse: &str, litteraux: &[Vec<u8>]) -> HashMap<String, Vec<u8>> {
    let mut contenus = HashMap::new();
    let mut reste = reponse;
    while let Some(i) = reste.find("BODY[") {
        let apres = &reste[i + "BODY[".len()..];
        let Some(fin) = apres.find(']') else {
            break;
        };
        let section = apres[..fin].to_ascii_uppercase();
        let mut suite = &apres[fin + 1..];
        // « BODY[1]<0> » : origine d'une lecture partielle, sans objet ici.
        if let Some(origine) = suite.strip_prefix('<') {
            suite = origine.split_once('>').map_or(origine, |(_, apres)| apres);
        }
        match lire(suite, litteraux) {
            Some((valeur, apres_valeur)) => {
                match valeur {
                    Valeur::Texte(octets) => {
                        contenus.insert(section, octets);
                    }
                    Valeur::Nil => {
                        contenus.insert(section, Vec::new());
                    }
                    Valeur::Liste(_) => {}
                }
                reste = apres_valeur;
            }
            None => reste = suite,
        }
    }
    contenus
}

/// Le message recomposé : en-têtes, textes et images intégrées lus, pièces
/// laissées sur le serveur réduites à leurs en-têtes. `None` s'il manque une
/// section ou la frontière d'une partie multiple : le message se lit alors
/// entier.
pub fn recomposer(structure: &Partie, contenus: &HashMap<String, Vec<u8>>) -> Option<Vec<u8>> {
    let mut sortie = Vec::new();
    ecrire(structure, contenus.get("HEADER")?, contenus, &mut sortie)?;
    Some(sortie)
}

fn ecrire(p: &Partie, entete: &[u8], contenus: &HashMap<String, Vec<u8>>, sortie: &mut Vec<u8>) -> Option<()> {
    let entete = sans_ligne_vide(entete);
    if p.multiple() {
        let frontiere = frontiere(&entete)?;
        sortie.extend_from_slice(&entete);
        sortie.extend_from_slice(b"\r\n");
        for enfant in &p.enfants {
            sortie.extend_from_slice(format!("--{frontiere}\r\n").as_bytes());
            ecrire(enfant, contenus.get(&format!("{}.MIME", enfant.section))?, contenus, sortie)?;
            sortie.extend_from_slice(b"\r\n");
        }
        sortie.extend_from_slice(format!("--{frontiere}--\r\n").as_bytes());
    } else if p.laissee() {
        sortie.extend_from_slice(&entete);
        sortie.extend_from_slice(format!("{ENTETE_TAILLE}: {}\r\n\r\n", p.taille_decodee()).as_bytes());
    } else {
        sortie.extend_from_slice(&entete);
        sortie.extend_from_slice(b"\r\n");
        sortie.extend_from_slice(contenus.get(&p.section)?);
    }
    Some(())
}

/// Les lignes d'en-tête, chacune close par CRLF, sans la ligne vide qui les
/// termine.
fn sans_ligne_vide(entete: &[u8]) -> Vec<u8> {
    let mut fin = entete.len();
    while fin > 0 && matches!(entete[fin - 1], b'\r' | b'\n') {
        fin -= 1;
    }
    let mut lignes = entete[..fin].to_vec();
    if !lignes.is_empty() {
        lignes.extend_from_slice(b"\r\n");
    }
    lignes
}

/// Frontière déclarée par l'en-tête `Content-Type` d'une partie multiple.
fn frontiere(entete: &[u8]) -> Option<String> {
    // Une ligne qui commence par une espace prolonge la précédente.
    let texte = String::from_utf8_lossy(entete).replace("\r\n", "\n").replace("\n ", " ").replace("\n\t", " ");
    let ligne =
        texte.lines().find(|l| l.get(..13).is_some_and(|debut| debut.eq_ignore_ascii_case("content-type:")))?;
    // Minuscules ASCII seulement : les positions restent celles de la ligne,
    // dont on garde la casse — une frontière la respecte.
    let debut = ligne.to_ascii_lowercase().find("boundary=")? + "boundary=".len();
    let valeur = &ligne[debut..];
    let valeur = match valeur.strip_prefix('"') {
        Some(citee) => &citee[..citee.find('"')?],
        None => valeur.split(|c: char| c == ';' || c.is_whitespace()).next()?,
    };
    (!valeur.is_empty()).then(|| valeur.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Message à texte et HTML, logo intégré et devis en PDF, tel que Dovecot
    /// en rend la structure.
    const STRUCTURE: &str = "* 1 FETCH (UID 7 BODYSTRUCTURE (((\"text\" \"plain\" (\"charset\" \"utf-8\") NIL NIL \
        \"quoted-printable\" 120 4 NIL NIL NIL NIL)(\"text\" \"html\" (\"charset\" \"utf-8\") NIL NIL \
        \"quoted-printable\" 480 12 NIL NIL NIL NIL) \"alternative\" (\"boundary\" \"alt\") NIL NIL NIL)\
        (\"image\" \"png\" (\"name\" \"logo.png\") \"<logo@exemple.fr>\" NIL \"base64\" 2000 NIL \
        (\"inline\" (\"filename\" \"logo.png\")) NIL NIL)(\"application\" \"pdf\" (\"name\" \"devis.pdf\") NIL NIL \
        \"base64\" 400000 NIL (\"attachment\" (\"filename\" \"devis.pdf\")) NIL NIL) \"mixed\" \
        (\"boundary\" \"ext\") NIL NIL NIL))\n";

    fn contenus_d_essai() -> HashMap<String, Vec<u8>> {
        let mut c = HashMap::new();
        let mut poser = |s: &str, v: &str| {
            c.insert(s.to_string(), v.as_bytes().to_vec());
        };
        poser(
            "HEADER",
            "From: Essai <essai@exemple.fr>\r\nSubject: Devis\r\nMIME-Version: 1.0\r\n\
             Content-Type: multipart/mixed;\r\n\tboundary=\"ext\"\r\n\r\n",
        );
        poser("1.MIME", "Content-Type: multipart/alternative; boundary=alt\r\n\r\n");
        poser("1.1.MIME", "Content-Type: text/plain; charset=utf-8\r\n\r\n");
        poser("1.1", "Bonjour, voici le devis.");
        poser("1.2.MIME", "Content-Type: text/html; charset=utf-8\r\n\r\n");
        poser("1.2", "<p>Bonjour, voici le <b>devis</b>.<img src=\"cid:logo@exemple.fr\"></p>");
        poser(
            "2.MIME",
            "Content-Type: image/png; name=logo.png\r\nContent-ID: <logo@exemple.fr>\r\n\
             Content-Transfer-Encoding: base64\r\nContent-Disposition: inline; filename=logo.png\r\n\r\n",
        );
        poser("2", "iVBORw0KGgo=");
        poser(
            "3.MIME",
            "Content-Type: application/pdf; name=devis.pdf\r\nContent-Transfer-Encoding: base64\r\n\
             Content-Disposition: attachment; filename=devis.pdf\r\n\r\n",
        );
        c
    }

    #[test]
    fn structure_lue() {
        let s = analyser(STRUCTURE, &[]).unwrap();
        assert_eq!(s.genre, "multipart/mixed");
        assert_eq!(s.enfants.len(), 3);
        let alternative = &s.enfants[0];
        assert_eq!((alternative.section.as_str(), alternative.genre.as_str()), ("1", "multipart/alternative"));
        assert_eq!(alternative.enfants[1].section, "1.2");
        assert_eq!(alternative.enfants[1].genre, "text/html");
        assert!(s.enfants[1].avec_id);
        let pdf = &s.enfants[2];
        assert_eq!((pdf.section.as_str(), pdf.genre.as_str(), pdf.taille), ("3", "application/pdf", 400000));
        assert_eq!(pdf.encodage, "base64");
    }

    /// Un nom de fichier envoyé en littéral (accents) ne gêne pas la lecture.
    #[test]
    fn structure_avec_litteral() {
        let reponse = format!(
            "* 1 FETCH (UID 7 BODYSTRUCTURE ((\"text\" \"plain\" NIL NIL NIL \"7bit\" 10 1 NIL NIL NIL NIL)\
             (\"application\" \"pdf\" (\"name\" {m}0{m}) NIL NIL \"base64\" 300000 NIL NIL NIL NIL) \"mixed\" \
             (\"boundary\" \"b\") NIL NIL NIL))\n",
            m = MARQUEUR
        );
        let s = analyser(&reponse, &["résumé.pdf".as_bytes().to_vec()]).unwrap();
        assert_eq!(s.enfants[1].genre, "application/pdf");
        assert_eq!(s.enfants[1].taille, 300000);
        assert_eq!(sections(&s).unwrap(), ["HEADER", "1.MIME", "1", "2.MIME"]);
    }

    #[test]
    fn sections_sans_la_piece_jointe() {
        let s = analyser(STRUCTURE, &[]).unwrap();
        assert_eq!(
            sections(&s).unwrap(),
            ["HEADER", "1.MIME", "1.1.MIME", "1.1", "1.2.MIME", "1.2", "2.MIME", "2", "3.MIME"]
        );
    }

    /// Trop peu à gagner, ou un message d'un seul tenant : il se lit entier.
    #[test]
    fn lu_entier_sans_gain() {
        let petit = STRUCTURE.replace(" 400000 ", " 100000 ");
        assert_eq!(sections(&analyser(&petit, &[]).unwrap()), None);
        let seul = "* 1 FETCH (UID 7 BODYSTRUCTURE (\"application\" \"pdf\" NIL NIL NIL \"base64\" 900000 NIL NIL NIL NIL))\n";
        assert_eq!(sections(&analyser(seul, &[]).unwrap()), None);
    }

    #[test]
    fn contenus_d_une_reponse() {
        let reponse = format!(
            "* 1 FETCH (UID 7 FLAGS (\\Seen) BODY[HEADER] {m}0{m} BODY[1.MIME] {m}1{m} BODY[1] \"court\" \
             BODY[2.MIME] NIL)\nm0003 OK fait\n",
            m = MARQUEUR
        );
        let litteraux = vec![b"Subject: x\r\n\r\n".to_vec(), b"Content-Type: text/plain\r\n\r\n".to_vec()];
        let c = contenus(&reponse, &litteraux);
        assert_eq!(c["HEADER"], litteraux[0]);
        assert_eq!(c["1.MIME"], litteraux[1]);
        assert_eq!(c["1"], b"court");
        assert!(c["2.MIME"].is_empty());
    }

    /// Le message recomposé s'affiche comme l'entier — texte, HTML, logo
    /// intégré — et liste le devis avec sa taille, sans l'avoir transféré.
    #[test]
    fn recompose_sans_la_piece_jointe() {
        let s = analyser(STRUCTURE, &[]).unwrap();
        let octets = recomposer(&s, &contenus_d_essai()).unwrap();
        assert!(octets.len() < 2_000, "{}", String::from_utf8_lossy(&octets));
        let pieces = crate::index::pieces_jointes(&octets);
        assert_eq!(pieces.len(), 1, "{pieces:?}");
        assert_eq!(pieces[0].nom, "devis.pdf");
        assert_eq!(pieces[0].taille, 400000 * 57 / 78);
        let corps = crate::rendu::corps_html(&octets).unwrap();
        assert!(corps.html.contains("devis"), "{}", corps.html);
        assert_eq!(corps.images.len(), 1, "le logo intégré reste affiché");
    }

    /// Une section manquante : rien n'est recomposé, le message se lit entier.
    #[test]
    fn section_manquante() {
        let s = analyser(STRUCTURE, &[]).unwrap();
        let mut c = contenus_d_essai();
        c.remove("1.2");
        assert_eq!(recomposer(&s, &c), None);
    }

    #[test]
    fn frontieres() {
        assert_eq!(frontiere(b"Content-Type: multipart/mixed;\r\n\tboundary=\"=_a b\"\r\n\r\n").as_deref(), Some("=_a b"));
        assert_eq!(frontiere(b"content-type: multipart/alternative; BOUNDARY=Xyz; charset=x\r\n").as_deref(), Some("Xyz"));
        assert_eq!(frontiere(b"Subject: multipart\r\n\r\n"), None);
    }

    /// Lecture par le client : la structure, puis les seules sections utiles,
    /// en deux commandes.
    #[test]
    fn lecture_en_deux_commandes() {
        use crate::simule::{echange, session};
        let structure = "* 1 FETCH (UID 7 BODYSTRUCTURE ((\"text\" \"plain\" (\"charset\" \"utf-8\") NIL NIL \"8bit\" 9 1 \
            NIL NIL NIL NIL)(\"application\" \"octet-stream\" (\"name\" \"gros.bin\") NIL NIL \"base64\" 300000 NIL \
            (\"attachment\" (\"filename\" \"gros.bin\")) NIL NIL) \"mixed\" (\"boundary\" \"b1\") NIL NIL NIL))\r\n";
        let litteral = |s: &str| format!("{{{}}}\r\n{s}", s.len());
        let reponse = format!(
            "* 1 FETCH (UID 7 FLAGS (\\Seen $MDNSent) BODY[HEADER] {} BODY[1.MIME] {} BODY[1] {} BODY[2.MIME] {})\r\n",
            litteral("Subject: Essai\r\nContent-Type: multipart/mixed; boundary=b1\r\n\r\n"),
            litteral("Content-Type: text/plain; charset=utf-8\r\n\r\n"),
            litteral("Bonjour !"),
            litteral(
                "Content-Type: application/octet-stream; name=gros.bin\r\nContent-Transfer-Encoding: base64\r\n\
                 Content-Disposition: attachment; filename=gros.bin\r\n\r\n"
            ),
        );
        let (mut client, journal) = session(
            "UIDPLUS",
            vec![
                echange("UID FETCH 7 (UID BODYSTRUCTURE)", structure, "OK fait"),
                echange(
                    "UID FETCH 7 (UID FLAGS BODY.PEEK[HEADER] BODY.PEEK[1.MIME] BODY.PEEK[1] BODY.PEEK[2.MIME])",
                    &reponse,
                    "OK fait",
                ),
            ],
        );
        let (octets, drapeaux) = client.message_allege(7).unwrap().expect("lu par parties");
        assert!(drapeaux.iter().any(|d| d == "$MDNSent"), "{drapeaux:?}");
        assert_eq!(crate::index::corps_affichable(&octets).trim(), "Bonjour !");
        let pieces = crate::index::pieces_jointes(&octets);
        assert_eq!((pieces.len(), pieces[0].nom.as_str()), (1, "gros.bin"), "{pieces:?}");
        assert_eq!(journal.lock().unwrap().len(), 2);
    }

    /// Un message sans gain à espérer coûte une commande de plus, rien d'autre.
    #[test]
    fn sans_gain_une_seule_commande() {
        use crate::simule::{echange, session};
        let (mut client, journal) = session(
            "UIDPLUS",
            vec![echange(
                "UID FETCH 7 (UID BODYSTRUCTURE)",
                "* 1 FETCH (UID 7 BODYSTRUCTURE (\"text\" \"plain\" NIL NIL NIL \"7bit\" 900000 10 NIL NIL NIL NIL))\r\n",
                "OK fait",
            )],
        );
        assert_eq!(client.message_allege(7).unwrap(), None);
        assert_eq!(journal.lock().unwrap().len(), 1);
    }
}
