//! Messages et certificats d'essai fabriqués par OpenSSL
//! (`essais/fabriquer.sh`) : ce que MMail en lit doit être ce qu'OpenSSL y a
//! mis.

use super::*;

const RACINE: &[u8] = include_bytes!("essais/racine.pem");
const INTERMEDIAIRE: &[u8] = include_bytes!("essais/intermediaire.pem");
const ALICE_P12: &[u8] = include_bytes!("essais/alice.p12");
const BOB_P12: &[u8] = include_bytes!("essais/bob.p12");
const BOB_ANCIEN_P12: &[u8] = include_bytes!("essais/bob-ancien.p12");
const BOB_PEM: &[u8] = include_bytes!("essais/bob.pem");

/// Le 01/01/2027 : les certificats d'essai, faits le 03/11/2026 pour vingt
/// ans, sont valides.
const MAINTENANT: i64 = 1_798_761_600;
/// Le 01/01/2050 : ils ont expiré.
const PLUS_TARD: i64 = 2_524_608_000;

fn der(pem: &[u8]) -> Vec<u8> {
    let texte = std::str::from_utf8(pem).unwrap();
    let corps: String = texte.lines().filter(|l| !l.starts_with("-----")).collect();
    crate::rendu::base64(&corps).unwrap()
}

fn autorites() -> Autorites {
    Autorites::de(&[der(RACINE)])
}

fn alice() -> Cle {
    Cle::depuis_p12(ALICE_P12, "essai-alice").unwrap()
}

fn bob() -> Cle {
    Cle::depuis_p12(BOB_P12, "essai-bob").unwrap()
}

fn ouvrir_essai(nom: &str, cles: &[Cle], quand: i64) -> Ouvert {
    let brut = std::fs::read(format!("{}/src/smime/essais/{nom}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    ouvrir(&brut, cles, &autorites(), quand).unwrap_or_else(|| panic!("{nom} : pas reconnu comme S/MIME"))
}

fn etat(o: &Ouvert) -> &str {
    o.securite.signature.as_ref().map(|s| s.etat.as_str()).unwrap_or("")
}

/// Le contenu d'essai se lit après ouverture, comme un message ordinaire.
fn contenu_lisible(o: &Ouvert) {
    let texte = crate::index::corps_affichable(&o.message);
    assert!(texte.contains("Bonjour Bob, voici le devis signé."), "corps : {texte}");
    let pieces: Vec<String> = crate::index::pieces_jointes(&o.message).into_iter().map(|p| p.nom).collect();
    assert_eq!(pieces, vec!["devis.txt".to_string()], "pièces : {pieces:?}");
    let m = MessageParser::default().parse(&o.message).unwrap();
    assert_eq!(m.subject(), Some("Devis"));
}

#[test]
fn p12_recents_et_anciens() {
    let a = alice();
    let t = a.titulaire().unwrap();
    assert_eq!(t.nom, "Alice Essai");
    assert_eq!(t.adresses, vec!["alice@exemple.fr".to_string()]);
    assert_eq!(t.emetteur, "Intermediaire d'essai");
    assert_eq!(a.intermediaires.len(), 1);
    // 3DES et RC2-40 : les exports anciens.
    let ancien = Cle::depuis_p12(BOB_ANCIEN_P12, "essai-bob").unwrap();
    assert_eq!(ancien.certificat, der(BOB_PEM));
    assert_eq!(ancien.empreinte(), bob().empreinte());
    assert!(Cle::depuis_p12(BOB_P12, "pas-le-bon").is_err());
}

#[test]
fn message_ordinaire_non_concerne() {
    let brut = b"From: a@exemple.fr\r\nTo: b@exemple.fr\r\nSubject: s\r\n\r\ncorps\r\n";
    assert!(!est_smime(brut));
    assert!(ouvrir(brut, &[], &autorites(), MAINTENANT).is_none());
}

#[test]
fn signature_detachee_verifiee() {
    let o = ouvrir_essai("signe-detache.eml", &[], MAINTENANT);
    assert_eq!(etat(&o), "verifiee", "{:?}", o.securite);
    let s = o.securite.signature.as_ref().unwrap();
    assert!(s.texte.contains("Alice Essai <alice@exemple.fr>"), "{}", s.texte);
    assert!(s.signe_le > 0);
    assert!(s.recueillable());
    assert!(!o.securite.chiffre);
    contenu_lisible(&o);
}

#[test]
fn signature_opaque_verifiee() {
    let o = ouvrir_essai("signe-opaque.eml", &[], MAINTENANT);
    assert_eq!(etat(&o), "verifiee", "{:?}", o.securite);
    contenu_lisible(&o);
}

#[test]
fn signature_ecdsa_verifiee() {
    let o = ouvrir_essai("signe-ecdsa.eml", &[], MAINTENANT);
    assert_eq!(etat(&o), "verifiee", "{:?}", o.securite);
}

#[test]
fn contenu_modifie_repere() {
    let brut = std::fs::read(format!("{}/src/smime/essais/signe-detache.eml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let texte = String::from_utf8(brut).unwrap().replace("1 234,56", "9 234,56");
    let o = ouvrir(texte.as_bytes(), &[], &autorites(), MAINTENANT).unwrap();
    assert_eq!(etat(&o), "modifiee", "{:?}", o.securite);
    assert!(!o.securite.signature.as_ref().unwrap().recueillable());
}

#[test]
fn fins_de_ligne_reecrites_par_un_relais() {
    let brut = std::fs::read(format!("{}/src/smime/essais/signe-detache.eml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let lf = String::from_utf8(brut).unwrap().replace("\r\n", "\n");
    let o = ouvrir(lf.as_bytes(), &[], &autorites(), MAINTENANT).unwrap();
    assert_eq!(etat(&o), "verifiee", "{:?}", o.securite);
}

#[test]
fn adresse_de_l_expediteur_differente() {
    let brut = std::fs::read(format!("{}/src/smime/essais/signe-detache.eml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let texte = String::from_utf8(brut).unwrap().replace("From: alice@exemple.fr", "From: mallory@exemple.fr");
    let o = ouvrir(texte.as_bytes(), &[], &autorites(), MAINTENANT).unwrap();
    assert_eq!(etat(&o), "adresse", "{:?}", o.securite);
    assert!(o.securite.signature.as_ref().unwrap().texte.contains("mallory@exemple.fr"));
}

#[test]
fn autorite_inconnue() {
    let o = ouvrir_essai("signe-inconnu.eml", &[], MAINTENANT);
    assert_eq!(etat(&o), "inconnue", "{:?}", o.securite);
    // Sans la racine d'essai, Alice aussi est d'une autorité inconnue.
    let brut = std::fs::read(format!("{}/src/smime/essais/signe-detache.eml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let o = ouvrir(&brut, &[], &Autorites::de(&[]), MAINTENANT).unwrap();
    assert_eq!(etat(&o), "inconnue", "{:?}", o.securite);
}

#[test]
fn certificat_non_prevu_pour_le_courrier() {
    let o = ouvrir_essai("signe-serveur.eml", &[], MAINTENANT);
    assert_eq!(etat(&o), "usage", "{:?}", o.securite);
}

#[test]
fn sha1_refuse() {
    let o = ouvrir_essai("signe-sha1.eml", &[], MAINTENANT);
    assert_eq!(etat(&o), "faible", "{:?}", o.securite);
}

#[test]
fn certificat_expire_apres_la_signature() {
    let o = ouvrir_essai("signe-detache.eml", &[], PLUS_TARD);
    assert_eq!(etat(&o), "expiree", "{:?}", o.securite);
}

#[test]
fn chiffre_aes_pour_deux_destinataires() {
    for cle in [bob(), alice()] {
        let o = ouvrir_essai("chiffre-aes.eml", &[cle], MAINTENANT);
        assert!(o.securite.chiffre && o.securite.illisible.is_empty(), "{:?}", o.securite);
        assert!(!o.securite.ancien);
        assert!(o.securite.signature.is_none());
        contenu_lisible(&o);
    }
}

#[test]
fn chiffre_variantes_d_openssl() {
    // 3DES (ancien), destinataire désigné par identifiant de clé, RSA-OAEP,
    // longueurs indéfinies (BER).
    for nom in ["chiffre-3des.eml", "chiffre-ski.eml", "chiffre-oaep.eml", "chiffre-ber.eml"] {
        let o = ouvrir_essai(nom, &[bob()], MAINTENANT);
        assert!(o.securite.chiffre && o.securite.illisible.is_empty(), "{nom} : {:?}", o.securite);
        assert_eq!(o.securite.ancien, nom == "chiffre-3des.eml", "{nom}");
        contenu_lisible(&o);
    }
}

#[test]
fn chiffre_illisible_sans_la_bonne_cle() {
    let o = ouvrir_essai("chiffre-pour-alice.eml", &[bob()], MAINTENANT);
    assert!(o.securite.chiffre);
    assert!(o.securite.illisible.contains("destinataires"), "{}", o.securite.illisible);
    let texte = crate::index::corps_affichable(&o.message);
    assert!(texte.contains("chiffré"), "{texte}");
    // Sans aucune clé.
    let o = ouvrir_essai("chiffre-pour-alice.eml", &[], MAINTENANT);
    assert!(o.securite.illisible.contains("aucun certificat"), "{}", o.securite.illisible);
    // L'ancienne clé d'Alice parmi d'autres suffit.
    let o = ouvrir_essai("chiffre-pour-alice.eml", &[bob(), alice()], MAINTENANT);
    assert!(o.securite.illisible.is_empty(), "{:?}", o.securite);
}

#[test]
fn signe_puis_chiffre_comme_outlook() {
    let o = ouvrir_essai("signe-chiffre.eml", &[bob()], MAINTENANT);
    assert!(o.securite.chiffre && o.securite.illisible.is_empty(), "{:?}", o.securite);
    assert_eq!(etat(&o), "verifiee", "{:?}", o.securite);
    contenu_lisible(&o);
}

#[test]
fn intermediaires_hors_du_message() {
    // La chaîne s'établit par les intermédiaires que porte la signature :
    // l'autorité racine seule suffit côté lecteur.
    assert_eq!(Autorites::de(&[der(RACINE)]).nombre(), 1);
    assert_eq!(Autorites::de(&[der(RACINE), der(INTERMEDIAIRE)]).nombre(), 2);
}

#[test]
fn recueil_du_signataire_et_de_ses_intermediaires() {
    let profil = std::env::temp_dir().join(format!("mmail-smime-recueil-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&profil);
    // Le profil se désigne par son index, comme dans MMail.
    let p = profil.join("index.sqlite").to_string_lossy().into_owned();
    let o = ouvrir_essai("signe-detache.eml", &[], MAINTENANT);
    let s = o.securite.signature.as_ref().unwrap();
    assert!(recueillir(&p, s));
    // Déjà là : rien de nouveau.
    assert!(!recueillir(&p, s));
    assert_eq!(certificats_de(&p, "alice@exemple.fr").len(), 1);
    assert_eq!(intermediaires_recueillis(&p), vec![der(INTERMEDIAIRE)]);
    // Une signature dont l'autorité est inconnue ne se recueille pas.
    let o = ouvrir_essai("signe-inconnu.eml", &[], MAINTENANT);
    assert!(!recueillir(&p, o.securite.signature.as_ref().unwrap()));
    assert!(certificats_de(&p, "inconnu@exemple.fr").is_empty());
    assert!(profil.join("smime").join("correspondants").join("alice@exemple.fr").is_dir());
    let _ = std::fs::remove_dir_all(&profil);
}

#[test]
fn cles_et_autorites_rangees_a_cote_de_l_index() {
    let profil = std::env::temp_dir().join(format!("mmail-smime-rangement-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&profil);
    std::fs::create_dir_all(&profil).unwrap();
    let p = profil.join("index.sqlite").to_string_lossy().into_owned();
    std::fs::write(&p, b"").unwrap();
    let r = ranger_cle(&p, "Bob@Exemple.fr", BOB_P12, &bob()).unwrap();
    assert_eq!(r.adresse, "bob@exemple.fr");
    let rangees = cles_rangees(&p);
    assert_eq!(rangees, vec![r.clone()]);
    assert_eq!(rangees[0].certificat(), Some(der(BOB_PEM)));
    let a = lire_certificat(RACINE).unwrap();
    ranger_autorite(&p, &a).unwrap();
    assert_eq!(autorites_ajoutees(&p), vec![der(RACINE)]);
    assert!(retirer_cle(&p, &r.empreinte).unwrap());
    assert!(cles_rangees(&p).is_empty());
    assert!(ranger_cle(&p, "../x", BOB_P12, &bob()).is_err());
    let _ = std::fs::remove_dir_all(&profil);
}

// ------------------------------------------------------------------ envoi

/// Un message tel que le fabrique la rédaction, avec une pièce jointe.
fn message_essai() -> Vec<u8> {
    let r = crate::redaction::Redaction {
        de: "alice@exemple.fr".into(),
        nom: "Alice Essai".into(),
        a: "bob@exemple.fr".into(),
        objet: "Devis".into(),
        texte: "Bonjour Bob, voici le devis signé.".into(),
        ..Default::default()
    };
    let pj = [crate::redaction::Fichier { nom: "devis.txt".into(), contenu: b"Total : 1 234,56 euros\r\n".to_vec() }];
    crate::redaction::fabriquer(&r, MAINTENANT, &pj).unwrap().envoi
}

#[test]
fn signe_par_mmail_verifie_par_mmail() {
    let m = proteger(&message_essai(), Some(&alice()), &[], MAINTENANT).unwrap();
    let texte = String::from_utf8_lossy(&m);
    assert!(texte.contains("multipart/signed"), "{texte}");
    let o = ouvrir(&m, &[], &autorites(), MAINTENANT).unwrap();
    assert_eq!(etat(&o), "verifiee", "{:?}", o.securite);
    assert!(!o.securite.chiffre);
    contenu_lisible(&o);
}

#[test]
fn signe_et_chiffre_par_mmail() {
    let pour = vec![der(BOB_PEM), alice().certificat.clone()];
    let m = proteger(&message_essai(), Some(&alice()), &pour, MAINTENANT).unwrap();
    let texte = String::from_utf8_lossy(&m);
    // L'objet et les adresses restent en clair ; le corps, non.
    assert!(texte.contains("Subject: Devis"), "{texte}");
    assert!(!texte.contains("Bonjour Bob") && !texte.contains("devis.txt"), "{texte}");
    for cle in [bob(), alice()] {
        let o = ouvrir(&m, &[cle], &autorites(), MAINTENANT).unwrap();
        assert!(o.securite.chiffre && o.securite.illisible.is_empty(), "{:?}", o.securite);
        assert_eq!(etat(&o), "verifiee", "{:?}", o.securite);
        contenu_lisible(&o);
    }
    let o = ouvrir(&m, &[], &autorites(), MAINTENANT).unwrap();
    assert!(!o.securite.illisible.is_empty());
}

#[test]
fn chiffrement_refuse_sans_destinataire_ni_cle_elliptique() {
    assert!(chiffrer(b"Content-Type: text/plain\r\n\r\nx", &[]).is_err());
    let carole = der(include_bytes!("essais/carole.pem"));
    let e = chiffrer(b"Content-Type: text/plain\r\n\r\nx", &[carole]).unwrap_err();
    assert!(e.contains("elliptique"), "{e}");
}

#[test]
fn certificat_du_correspondant_choisi_parmi_les_recueillis() {
    let profil = std::env::temp_dir().join(format!("mmail-smime-choix-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&profil);
    let p = profil.join("index.sqlite").to_string_lossy().into_owned();
    assert!(certificat_pour(&p, "alice@exemple.fr", &autorites(), MAINTENANT).is_none());
    let o = ouvrir_essai("signe-detache.eml", &[], MAINTENANT);
    recueillir(&p, o.securite.signature.as_ref().unwrap());
    let c = certificat_pour(&p, "Alice@Exemple.fr", &autorites(), MAINTENANT).unwrap();
    assert_eq!(titulaire(&c).unwrap().nom, "Alice Essai");
    // Expiré : plus personne à qui chiffrer.
    assert!(certificat_pour(&p, "alice@exemple.fr", &autorites(), PLUS_TARD).is_none());
    // Autorité retirée : refusé.
    assert!(certificat_pour(&p, "alice@exemple.fr", &Autorites::de(&[]), MAINTENANT).is_none());
    let _ = std::fs::remove_dir_all(&profil);
}

/// OpenSSL, la référence du CMS, vérifie et déchiffre ce que MMail produit.
/// Absent du poste, l'essai se passe ; absent de la CI, il échoue.
#[test]
fn openssl_lit_ce_que_mmail_signe_et_chiffre() {
    use std::process::Command;
    if Command::new("openssl").arg("version").output().is_err() {
        assert!(std::env::var_os("CI").is_none(), "openssl absent de la CI");
        return;
    }
    let d = std::env::temp_dir().join(format!("mmail-smime-openssl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let ecrire = |nom: &str, octets: &[u8]| std::fs::write(d.join(nom), octets).unwrap();
    ecrire("racine.pem", RACINE);
    ecrire("bob.pem", BOB_PEM);
    ecrire("bob.p12", BOB_P12);
    ecrire("signe.eml", &proteger(&message_essai(), Some(&alice()), &[], MAINTENANT).unwrap());
    ecrire(
        "chiffre.eml",
        &proteger(&message_essai(), Some(&alice()), &[der(BOB_PEM), alice().certificat.clone()], MAINTENANT).unwrap(),
    );
    let openssl = |args: &[&str]| {
        let s = Command::new("openssl").args(args).current_dir(&d).output().unwrap();
        assert!(s.status.success(), "openssl {args:?} : {}", String::from_utf8_lossy(&s.stderr));
    };
    openssl(&["pkcs12", "-in", "bob.p12", "-passin", "pass:essai-bob", "-nodes", "-nocerts", "-out", "bob.key"]);
    // L'heure de contrôle est celle de la signature : les certificats d'essai
    // sont valides à cette date.
    let quand = MAINTENANT.to_string();
    openssl(&["cms", "-verify", "-in", "signe.eml", "-CAfile", "racine.pem", "-attime", &quand, "-out", "verifie.mime"]);
    openssl(&["cms", "-decrypt", "-in", "chiffre.eml", "-recip", "bob.pem", "-inkey", "bob.key", "-out", "dechiffre.mime"]);
    openssl(&["cms", "-verify", "-in", "dechiffre.mime", "-CAfile", "racine.pem", "-attime", &quand, "-out", "interieur.mime"]);
    for f in ["verifie.mime", "interieur.mime"] {
        let contenu = String::from_utf8_lossy(&std::fs::read(d.join(f)).unwrap()).into_owned();
        assert!(contenu.contains("devis.txt"), "{f} : {contenu}");
    }
    let _ = std::fs::remove_dir_all(&d);
#[test]
fn messages_malformes_sans_panique() {
    // En-têtes S/MIME, contenu absurde : rien ne doit paniquer.
    let cas: [&[u8]; 4] = [
        b"From: a@exemple.fr\r\nContent-Type: application/pkcs7-mime; smime-type=enveloped-data\r\nContent-Transfer-Encoding: base64\r\n\r\nMIAGCSqGSIb3DQEHA6CAMIA=\r\n",
        b"From: a@exemple.fr\r\nContent-Type: multipart/signed; protocol=\"application/pkcs7-signature\"; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: text/plain\r\n\r\nx\r\n--b\r\nContent-Type: application/pkcs7-signature\r\nContent-Transfer-Encoding: base64\r\n\r\nAAAA\r\n--b--\r\n",
        b"From: a@exemple.fr\r\nContent-Type: multipart/signed; protocol=\"application/pkcs7-signature\"; boundary=\"b\"\r\n\r\n--b\r\nsans fin",
        b"From: a@exemple.fr\r\nContent-Type: application/pkcs7-mime\r\n\r\n",
    ];
    for brut in cas {
        assert!(est_smime(brut));
        let _ = ouvrir(brut, &[bob()], &autorites(), MAINTENANT);
    }
    // BER profondément imbriqué : refusé sans débordement de pile.
    let mut profond = Vec::new();
    for _ in 0..5000 {
        profond.extend_from_slice(&[0x30, 0x80]);
    }
    let corps = crate::smtp::base64(&profond);
    let brut = format!("From: a@exemple.fr\r\nContent-Type: application/pkcs7-mime\r\nContent-Transfer-Encoding: base64\r\n\r\n{corps}\r\n");
    let o = ouvrir(brut.as_bytes(), &[bob()], &autorites(), MAINTENANT);
    assert!(o.map(|o| o.securite.signature.is_none()).unwrap_or(true));
}
