// SPDX-License-Identifier: GPL-3.0-or-later
//! Passage des en-têtes bruts du serveur à une ligne d'index affichable.
//!
//! C'est ici que l'on décode ce que le protocole transporte encodé : un sujet
//! en MIME (« =?UTF-8?Q?R=C3=A9union?= »), une adresse avec nom d'affichage,
//! une date au format courrier. L'analyse est confiée à `mail-parser` : c'est
//! la surface d'attaque du client, et elle n'a pas à être écrite à la main.

use mail_parser::{Address, MessageParser, MimeHeaders};

use crate::magasin::MessageLocal;
use crate::protocole::Entete;

/// Convertit les en-têtes bruts d'un message en ligne d'index.
pub fn ligne_index(entete: &Entete) -> MessageLocal {
    let mut ligne = MessageLocal {
        uid: entete.uid,
        taille: entete.taille,
        lu: entete.lu(),
        repondu: entete.repondu(),
        ..Default::default()
    };

    let Some(message) = MessageParser::default().parse(&entete.brut) else {
        // En-têtes illisibles : la ligne reste, avec ce qu'on sait d'elle. Un
        // message incompréhensible ne doit pas disparaître de la liste.
        ligne.sujet = "(message illisible)".to_string();
        ligne.horodatage = horodatage(entete, None);
        return ligne;
    };

    ligne.sujet = message.subject().unwrap_or("(sans objet)").to_string();
    ligne.message_id = message.message_id().unwrap_or_default().to_string();
    if let Some(from) = message.from() {
        let (nom, adresse) = premier_contact(from);
        ligne.adresse = adresse;
        // À défaut de nom d'affichage, l'adresse fait l'affaire.
        ligne.expediteur = if nom.is_empty() { ligne.adresse.clone() } else { nom };
    }
    // La date interne du serveur sert de repli : un message peut arriver sans
    // en-tête Date, ou avec une date fantaisiste.
    ligne.date = match message.date() {
        Some(date) => date.to_rfc3339(),
        None => entete.date_interne.clone(),
    };
    ligne.horodatage = horodatage(entete, message.date().map(|d| d.to_timestamp()));
    ligne
}

/// Clef de tri : la date de réception du serveur, à défaut celle de l'en-tête.
fn horodatage(entete: &Entete, date_entete: Option<i64>) -> i64 {
    crate::protocole::secondes_date_interne(&entete.date_interne)
        .or(date_entete)
        .unwrap_or(0)
}

/// Corps lisible d'un message entier : le texte, ou le HTML réduit au texte.
///
/// Un message sans partie texte rend une ligne qui le dit, plutôt que rien :
/// l'utilisateur doit pouvoir distinguer « vide » de « pas encore chargé ».
/// L'affichage du message brut (décision 6) reste accessible à côté.
pub fn corps_affichable(brut: &[u8]) -> String {
    let Some(message) = MessageParser::default().parse(brut) else {
        return "(message illisible)".to_string();
    };
    if let Some(texte) = message.body_text(0) {
        return texte.into_owned();
    }
    if let Some(html) = message.body_html(0) {
        return html_vers_texte(&html);
    }
    "(ce message n'a pas de partie texte)".to_string()
}

/// Une pièce jointe, telle que l'interface la présente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PieceJointe {
    /// Rang parmi les pièces jointes du message : c'est ce qui la désigne
    /// quand on la demande ensuite.
    pub indice: usize,
    /// Nom de fichier sûr : sans chemin ni caractère interdit.
    pub nom: String,
    pub type_mime: String,
    pub taille: usize,
    /// Vrai si l'ouvrir directement lancerait un programme (décision de
    /// prudence, comme Outlook) : l'interface ne propose alors que
    /// l'enregistrement.
    pub risquee: bool,
}

/// Pièces jointes d'un message entier, dans l'ordre du message.
pub fn pieces_jointes(brut: &[u8]) -> Vec<PieceJointe> {
    let Some(message) = MessageParser::default().parse(brut) else {
        return Vec::new();
    };
    message
        .attachments()
        .enumerate()
        .map(|(indice, partie)| {
            let type_mime = type_de(partie);
            let nom = nom_sur(partie.attachment_name().unwrap_or(""), indice, &type_mime);
            PieceJointe {
                indice,
                risquee: ouverture_risquee(&nom),
                nom,
                type_mime,
                taille: partie.contents().len(),
            }
        })
        .collect()
}

/// Nom sûr et contenu d'une pièce jointe désignée par son rang.
pub fn extraire_piece(brut: &[u8], indice: usize) -> Option<(String, Vec<u8>)> {
    let message = MessageParser::default().parse(brut)?;
    let partie = message.attachments().nth(indice)?;
    let nom = nom_sur(partie.attachment_name().unwrap_or(""), indice, &type_de(partie));
    Some((nom, partie.contents().to_vec()))
}

fn type_de(partie: &mail_parser::MessagePart<'_>) -> String {
    match partie.content_type() {
        Some(ct) => match ct.subtype() {
            Some(sous) => format!("{}/{}", ct.ctype(), sous).to_ascii_lowercase(),
            None => ct.ctype().to_ascii_lowercase(),
        },
        None if partie.is_message() => "message/rfc822".to_string(),
        None => "application/octet-stream".to_string(),
    }
}

/// Nom de fichier sûr pour une pièce jointe : le nom annoncé par l'expéditeur
/// est une donnée non fiable. On n'en garde que le dernier segment — jamais un
/// chemin, qui pourrait faire écrire hors du dossier choisi —, sans caractère
/// de contrôle ni caractère interdit sous Windows, et borné en longueur. Un
/// nom absent ou vide devient « piece-jointe-N » avec l'extension du type.
pub fn nom_sur(nom: &str, indice: usize, type_mime: &str) -> String {
    let dernier = nom.rsplit(['/', '\\']).next().unwrap_or("");
    let propre: String = dernier
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    let propre = propre.trim().trim_matches('.').trim().to_string();
    // Noms réservés de Windows : « CON », « NUL.txt »… y désignent un
    // périphérique, pas un fichier.
    let racine = propre.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserve = matches!(racine.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (racine.len() == 4
            && (racine.starts_with("COM") || racine.starts_with("LPT"))
            && racine.as_bytes()[3].is_ascii_digit());
    let propre = if propre.is_empty() || reserve {
        format!("piece-jointe-{}{}", indice + 1, extension_de(type_mime))
    } else {
        propre
    };
    // 150 caractères au plus, en gardant l'extension.
    if propre.chars().count() <= 150 {
        return propre;
    }
    let extension = propre.rfind('.').map(|i| propre[i..].to_string()).unwrap_or_default();
    let extension: String = extension.chars().take(16).collect();
    let tronque: String = propre.chars().take(150 - extension.chars().count()).collect();
    format!("{tronque}{extension}")
}

fn extension_de(type_mime: &str) -> &'static str {
    match type_mime {
        "application/pdf" => ".pdf",
        "image/png" => ".png",
        "image/jpeg" => ".jpg",
        "image/gif" => ".gif",
        "text/plain" => ".txt",
        "text/html" => ".html",
        "text/calendar" => ".ics",
        "message/rfc822" => ".eml",
        _ => ".bin",
    }
}

/// Vrai si ouvrir ce fichier d'un double clic lancerait un programme ou un
/// script. Ces pièces jointes ne s'ouvrent pas depuis MMail : on les
/// enregistre, et l'on décide ensuite en connaissance de cause.
pub fn ouverture_risquee(nom: &str) -> bool {
    const EXECUTABLES: &[&str] = &[
        "exe", "com", "bat", "cmd", "scr", "pif", "msi", "msp", "mst", "jar", "js", "jse",
        "vbs", "vbe", "wsf", "wsh", "wsc", "ps1", "psm1", "psd1", "hta", "cpl", "lnk", "url",
        "reg", "inf", "dll", "sys", "msc", "application", "appref-ms", "gadget", "appx",
        "appxbundle", "msix", "msixbundle", "iso", "img", "vhd", "vhdx", "scf", "sct",
        "settingcontent-ms", "sh", "run", "desktop", "appimage", "apk", "py", "pl",
        "command", "app", "pkg", "dmg",
    ];
    let Some((_, extension)) = nom.rsplit_once('.') else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    EXECUTABLES.contains(&extension.as_str())
}

/// Réduction sommaire d'un HTML à son texte : retire les balises et rend les
/// entités les plus courantes. Ce n'est pas un rendu — l'affichage HTML viendra
/// avec la rédaction (décision 7) — mais de quoi lire un message.
fn html_vers_texte(html: &str) -> String {
    let mut sortie = String::with_capacity(html.len());
    let mut dans_balise = false;
    for c in html.chars() {
        match c {
            '<' => dans_balise = true,
            '>' => dans_balise = false,
            c if !dans_balise => sortie.push(c),
            _ => {}
        }
    }
    sortie
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Nom d'affichage et adresse du premier contact d'un champ d'adresses.
fn premier_contact(adresses: &Address<'_>) -> (String, String) {
    adresses
        .iter()
        .next()
        .map(|a| {
            (
                a.name().unwrap_or_default().to_string(),
                a.address().unwrap_or_default().to_string(),
            )
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entete(brut: &str) -> Entete {
        Entete {
            uid: 7,
            taille: 512,
            drapeaux: vec!["\\Seen".to_string()],
            date_interne: "16-Sep-2026 18:00:00 +0200".to_string(),
            brut: brut.as_bytes().to_vec(),
        }
    }

    #[test]
    fn sujet_et_expediteur_decodes() {
        let ligne = ligne_index(&entete(
            "From: \"Service compta\" <compta@exemple.fr>\r\n\
             Message-ID: <abc.123@exemple.fr>\r\n\
             Subject: =?UTF-8?Q?R=C3=A9union_d'=C3=A9quipe?=\r\n\
             Date: Tue, 16 Sep 2026 09:30:00 +0200\r\n\r\n",
        ));
        assert_eq!(ligne.uid, 7);
        assert_eq!(ligne.sujet, "Réunion d'équipe");
        assert_eq!(ligne.expediteur, "Service compta");
        assert_eq!(ligne.adresse, "compta@exemple.fr");
        assert_eq!(ligne.message_id, "abc.123@exemple.fr");
        assert!(ligne.date.starts_with("2026-09-16"));
        assert!(ligne.lu);
        // Tri sur la date interne du serveur, 16/09/2026 16:00 UTC.
        assert_eq!(ligne.horodatage, 1789574400);
    }

    #[test]
    fn sans_nom_d_affichage_l_adresse_suffit() {
        let ligne = ligne_index(&entete("From: brut@exemple.fr\r\nSubject: Objet\r\n\r\n"));
        assert_eq!(ligne.expediteur, "brut@exemple.fr");
        assert_eq!(ligne.adresse, "brut@exemple.fr");
    }

    #[test]
    fn message_sans_sujet_ni_date() {
        let ligne = ligne_index(&entete("From: a@b.fr\r\n\r\n"));
        assert_eq!(ligne.sujet, "(sans objet)");
        // Repli sur la date interne du serveur.
        assert_eq!(ligne.date, "16-Sep-2026 18:00:00 +0200");
    }

    #[test]
    fn corps_texte_et_corps_html() {
        let texte = corps_affichable(
            b"From: a@b.fr\r\nSubject: O\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nBonjour \xc3\xa0 tous.\r\n",
        );
        assert!(texte.contains("Bonjour à tous."));

        let html = corps_affichable(
            b"From: a@b.fr\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Co\xc3\xbbt : 12&nbsp;\xe2\x82\xac</p>\r\n",
        );
        assert!(html.contains("Coût"), "html rendu : {html}");
        assert!(!html.contains('<'));
    }

    #[test]
    fn message_sans_partie_texte() {
        let corps = corps_affichable(b"From: a@b.fr\r\nContent-Type: image/png\r\n\r\n\x89PNG");
        assert!(corps.contains("pas de partie texte"));
    }

    #[test]
    fn en_tetes_illisibles_gardent_une_ligne() {
        let mut e = entete("");
        e.brut = vec![0xff, 0xfe, 0x00];
        let ligne = ligne_index(&e);
        assert_eq!(ligne.uid, 7);
        assert!(!ligne.sujet.is_empty());
    }

    const AVEC_PIECES: &[u8] = b"From: a@b.fr\r\n\
Subject: Pieces\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"XX\"\r\n\
\r\n\
--XX\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Voir pi\xc3\xa8ces jointes.\r\n\
--XX\r\n\
Content-Type: application/pdf; name=\"facture.pdf\"\r\n\
Content-Disposition: attachment; filename=\"facture.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
JVBERi0xLjQKJcOkw7w=\r\n\
--XX\r\n\
Content-Type: application/octet-stream\r\n\
Content-Disposition: attachment; filename=\"../../Windows/outil.exe\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
TVqQAA==\r\n\
--XX--\r\n";

    #[test]
    fn pieces_jointes_listees_et_extraites() {
        let pieces = pieces_jointes(AVEC_PIECES);
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        assert_eq!(pieces[0].nom, "facture.pdf");
        assert_eq!(pieces[0].type_mime, "application/pdf");
        assert!(!pieces[0].risquee);
        // Le chemin annoncé par l'expéditeur est écarté, l'exécutable signalé.
        assert_eq!(pieces[1].nom, "outil.exe");
        assert!(pieces[1].risquee);

        let (nom, octets) = extraire_piece(AVEC_PIECES, 0).unwrap();
        assert_eq!(nom, "facture.pdf");
        assert!(octets.starts_with(b"%PDF-1.4"));
        assert_eq!(pieces[0].taille, octets.len());
        assert!(extraire_piece(AVEC_PIECES, 5).is_none());
        // Le corps affichable ne compte pas parmi les pièces jointes.
        assert!(corps_affichable(AVEC_PIECES).contains("Voir pièces jointes."));
    }

    #[test]
    fn noms_de_fichiers_assainis() {
        assert_eq!(nom_sur("rapport.pdf", 0, "application/pdf"), "rapport.pdf");
        assert_eq!(nom_sur("..\\..\\x.txt", 0, "text/plain"), "x.txt");
        assert_eq!(nom_sur("a<b>c:d|e?.txt", 0, "text/plain"), "a_b_c_d_e_.txt");
        assert_eq!(nom_sur("", 2, "image/png"), "piece-jointe-3.png");
        assert_eq!(nom_sur("...", 0, "application/zip"), "piece-jointe-1.bin");
        assert_eq!(nom_sur("CON.txt", 0, "text/plain"), "piece-jointe-1.txt");
        assert_eq!(nom_sur("com1", 0, "text/plain"), "piece-jointe-1.txt");
        assert_eq!(nom_sur("console.txt", 0, "text/plain"), "console.txt");
        let long = format!("{}.pdf", "é".repeat(300));
        let court = nom_sur(&long, 0, "application/pdf");
        assert_eq!(court.chars().count(), 150);
        assert!(court.ends_with(".pdf"));
    }

    #[test]
    fn executables_reconnus() {
        for nom in ["x.exe", "X.JS", "a.b.vbs", "lien.lnk", "s.ps1", "outil.AppImage"] {
            assert!(ouverture_risquee(nom), "{nom}");
        }
        for nom in ["facture.pdf", "photo.jpeg", "exe", "tableau.xlsx", "notes.txt"] {
            assert!(!ouverture_risquee(nom), "{nom}");
        }
    }
}
