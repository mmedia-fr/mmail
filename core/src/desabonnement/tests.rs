use super::*;

/// Lettre d'information : en-têtes donnés, puis un corps HTML.
fn lettre(entetes: &str, html: &str) -> Vec<u8> {
    format!(
        "From: Lettre <lettre@liste.exemple.fr>\r\nTo: moi@exemple.fr\r\nSubject: Les nouvelles de novembre\r\n\
         Date: Sat, 31 Oct 2026 12:27:00 +0100\r\nMIME-Version: 1.0\r\n{entetes}Content-Type: text/html; charset=UTF-8\r\n\r\n{html}\r\n"
    )
    .into_bytes()
}

#[test]
fn adresses_de_l_entete() {
    assert_eq!(
        uris_de_l_entete(" <mailto:desabo@liste.exemple.fr?subject=unsubscribe>,\r\n <https://liste.exemple.fr/d?u=1>"),
        vec!["mailto:desabo@liste.exemple.fr?subject=unsubscribe", "https://liste.exemple.fr/d?u=1"]
    );
    // Blancs et repli au milieu d'une adresse : ignorés (RFC 2369).
    assert_eq!(uris_de_l_entete("<https://liste.exemple.fr/\r\n d?u=1>"), vec!["https://liste.exemple.fr/d?u=1"]);
    assert!(uris_de_l_entete("https://sans-chevrons.exemple.fr").is_empty());
    assert!(uris_de_l_entete("<https://inacheve.exemple.fr").is_empty());
}

#[test]
fn un_clic_annonce_exactement() {
    assert!(un_clic_annonce(" List-Unsubscribe=One-Click"));
    assert!(un_clic_annonce("list-unsubscribe=one-click\r\n"));
    assert!(!un_clic_annonce("List-Unsubscribe=Two-Clicks"));
    assert!(!un_clic_annonce(""));
}

#[test]
fn un_clic_par_https() {
    let brut = lettre(
        "List-Unsubscribe: <mailto:desabo@liste.exemple.fr>, <https://liste.exemple.fr/un-clic?u=42>\r\n\
         List-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n",
        "<p>Bonjour</p>",
    );
    let d = trouver(&brut).unwrap();
    assert_eq!(d.methode, "un-clic");
    assert_eq!(d.source, "entete");
    assert_eq!(d.url, "https://liste.exemple.fr/un-clic?u=42");
    assert_eq!(d.hote, "liste.exemple.fr");
}

#[test]
fn sans_un_clic_l_ordre_de_l_expediteur() {
    // L'adresse de courriel vient d'abord : c'est elle.
    let brut = lettre(
        "List-Unsubscribe: <mailto:desabo@liste.exemple.fr?subject=unsubscribe>, <https://liste.exemple.fr/d?u=1>\r\n",
        "<p>Bonjour</p>",
    );
    let d = trouver(&brut).unwrap();
    assert_eq!(d.methode, "courriel");
    assert_eq!(d.adresse, "desabo@liste.exemple.fr");
    assert_eq!(d.hote, "desabo@liste.exemple.fr");
    assert_eq!(d.objet, "unsubscribe");
    // La page d'abord : la page.
    let brut = lettre("List-Unsubscribe: <https://liste.exemple.fr/d?u=1>, <mailto:desabo@liste.exemple.fr>\r\n", "<p>x</p>");
    let d = trouver(&brut).unwrap();
    assert_eq!((d.methode, d.url.as_str()), ("lien", "https://liste.exemple.fr/d?u=1"));
    // Un clic annoncé sans adresse https : la première adresse utilisable.
    let brut = lettre(
        "List-Unsubscribe: <http://liste.exemple.fr/d>, <mailto:desabo@liste.exemple.fr>\r\n\
         List-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n",
        "<p>x</p>",
    );
    let d = trouver(&brut).unwrap();
    assert_eq!((d.methode, d.url.as_str(), d.hote.as_str()), ("lien", "http://liste.exemple.fr/d", "liste.exemple.fr"));
}

#[test]
fn schemas_inconnus_ignores() {
    let brut = lettre("List-Unsubscribe: <ftp://liste.exemple.fr/d>, <javascript:alert(1)>\r\n", "<p>Bonjour</p>");
    assert_eq!(trouver(&brut), None);
}

#[test]
fn adresse_mailto() {
    assert_eq!(
        analyser_mailto("mailto:desabo%2Bliste@exemple.fr?subject=Se%20d%C3%A9sabonner&body=retirer+moi").unwrap(),
        ("desabo+liste@exemple.fr".to_string(), "Se désabonner".to_string(), "retirer+moi".to_string())
    );
    // Plusieurs destinataires : le premier ; « to= » à défaut d'adresse.
    assert_eq!(analyser_mailto("MAILTO:a@exemple.fr,b@exemple.fr").unwrap().0, "a@exemple.fr");
    assert_eq!(analyser_mailto("mailto:?to=c@exemple.fr&subject=x").unwrap().0, "c@exemple.fr");
    // Un saut de ligne dans l'objet ne passe pas dans l'en-tête.
    assert_eq!(analyser_mailto("mailto:a@exemple.fr?subject=a%0D%0ABcc:%20b@exemple.fr").unwrap().1, "aBcc: b@exemple.fr");
    assert_eq!(analyser_mailto("mailto:pas-une-adresse"), None);
    assert_eq!(analyser_mailto("mailto:a%20b@exemple.fr"), None);
    assert_eq!(analyser_mailto("https://exemple.fr"), None);
}

#[test]
fn echec_dmarc_rien_n_est_propose() {
    let brut = lettre(
        "Authentication-Results: mx.exemple.fr; dmarc=fail header.from=liste.exemple.fr\r\n\
         List-Unsubscribe: <https://liste.exemple.fr/un-clic>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n",
        "<a href=\"https://liste.exemple.fr/d\">Se désabonner</a>",
    );
    assert_eq!(trouver(&brut), None);
}

#[test]
fn lien_du_corps_par_son_texte() {
    // À la manière d'une lettre de grande enseigne : « Voir en ligne », un
    // lien de refus du pistage dont l'adresse contient « unsubscribe », puis
    // le vrai lien, au texte encodé en entités.
    let html = "<p><a href=\"https://vue.exemple.fr/?qs=1\">Voir cet e-mail en ligne</a></p>\
                <p><a href=\"https://www.exemple.fr/consent/unsubscribe/pixel/?p=2\">M&#8217;opposer au pistage</a> |\
                <a href=\"https://ww4.exemple.fr/fr/unsubscribe/?ei=3&amp;j=4\"><span style=\"color:#484848\">D&eacute;sinscription</span></a></p>";
    let d = trouver(&lettre("", html)).unwrap();
    assert_eq!(d.methode, "lien");
    assert_eq!(d.source, "corps");
    assert_eq!(d.url, "https://ww4.exemple.fr/fr/unsubscribe/?ei=3&j=4");
    assert_eq!(d.hote, "ww4.exemple.fr");
}

#[test]
fn lien_du_corps_par_son_adresse() {
    let html = "<p>Pour ne plus rien recevoir, cliquez <a href='https://news.exemple.fr/unsubscribe?u=9'>ici</a>.</p>";
    let d = trouver(&lettre("", html)).unwrap();
    assert_eq!((d.methode, d.url.as_str()), ("lien", "https://news.exemple.fr/unsubscribe?u=9"));
    // Lien de courriel dans le corps : un message à préparer.
    let html = "<a href=\"mailto:desabo@exemple.fr?subject=STOP\">Se désinscrire</a>";
    let d = trouver(&lettre("", html)).unwrap();
    assert_eq!((d.methode, d.source, d.adresse.as_str(), d.objet.as_str()), ("courriel", "corps", "desabo@exemple.fr", "STOP"));
}

#[test]
fn bloc_entier_cliquable_ignore() {
    let long = "Découvrez nos offres de la semaine, et si vous ne souhaitez plus les recevoir, vous pourrez vous désinscrire à tout moment depuis votre compte";
    let html = format!("<a href=\"https://boutique.exemple.fr/offres\">{long}</a>");
    assert_eq!(trouver(&lettre("", &html)), None);
}

#[test]
fn ni_reponse_ni_transfert_pour_le_corps() {
    let html = "<a href=\"https://news.exemple.fr/unsubscribe?u=9\">Désinscription</a>";
    for objet in ["TR : Offre flash", "Fwd: Offre", "RE: question", "Re[2]: suite", "AW: Frage"] {
        let brut = format!(
            "From: Collègue <collegue@exemple.fr>\r\nTo: moi@exemple.fr\r\nSubject: {objet}\r\nMIME-Version: 1.0\r\n\
             Content-Type: text/html; charset=UTF-8\r\n\r\n{html}\r\n"
        );
        assert_eq!(trouver(brut.as_bytes()), None, "{objet}");
    }
    let brut = format!(
        "From: c@exemple.fr\r\nSubject: Offre\r\nIn-Reply-To: <x@exemple.fr>\r\nContent-Type: text/html\r\n\r\n{html}\r\n"
    );
    assert_eq!(trouver(brut.as_bytes()), None);
    // Une liste de diffusion où l'on répond porte son propre en-tête : il vaut.
    let brut = "From: c@exemple.fr\r\nSubject: Re: discussion\r\nList-Unsubscribe: <https://liste.exemple.fr/d>\r\n\r\nx\r\n";
    assert_eq!(trouver(brut.as_bytes()).unwrap().methode, "lien");
    // « Remise » ou « Info: » ne sont pas des marqueurs.
    let brut = format!("From: c@exemple.fr\r\nSubject: Info: nouveautés\r\nContent-Type: text/html\r\n\r\n{html}\r\n");
    assert!(trouver(brut.as_bytes()).is_some());
}

#[test]
fn lien_d_un_message_en_texte() {
    let brut = "From: l@exemple.fr\r\nSubject: Nouvelles\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n\
                Bonjour,\r\n\r\nPour vous désinscrire :\r\nhttps://liste.exemple.fr/d?u=5.\r\n";
    let d = trouver(brut.as_bytes()).unwrap();
    assert_eq!((d.methode, d.url.as_str()), ("lien", "https://liste.exemple.fr/d?u=5"));
    let sans = "From: l@exemple.fr\r\nSubject: Bonjour\r\n\r\nVoir https://exemple.fr\r\n";
    assert_eq!(trouver(sans.as_bytes()), None);
}

#[test]
fn liens_et_attributs() {
    let liens = liens_html(
        "<abbr title=\"x\">y</abbr><A HREF=https://a.exemple.fr/1>Un</A>\
         <a data-href=\"https://faux.exemple.fr\" href = 'https://b.exemple.fr/2'><b>Deux</b>\r\n fois</a><a name=\"ancre\">sans</a>",
    );
    assert_eq!(
        liens,
        vec![
            ("https://a.exemple.fr/1".to_string(), "Un".to_string()),
            ("https://b.exemple.fr/2".to_string(), "Deux fois".to_string()),
        ]
    );
}

#[test]
fn demande_un_clic_en_https_seulement() {
    assert!(demander("http://liste.exemple.fr/d").is_err());
    assert!(demander("https://utilisateur@liste.exemple.fr/d").is_err());
}

#[test]
fn dans_l_en_tete_de_l_interface() {
    let brut = lettre(
        "List-Unsubscribe: <https://liste.exemple.fr/un-clic?u=42>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n",
        "<p>x</p>",
    );
    let e: serde_json::Value = serde_json::from_str(&crate::index::entete_affichable(&brut)).unwrap();
    assert_eq!(e["desabonnement"]["methode"], "un-clic");
    assert_eq!(e["desabonnement"]["hote"], "liste.exemple.fr");
    let sans: serde_json::Value = serde_json::from_str(&crate::index::entete_affichable(b"From: a@exemple.fr\r\n\r\nx\r\n")).unwrap();
    assert!(sans["desabonnement"].is_null());
}
