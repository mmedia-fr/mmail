//! S/MIME (0.8.0) : lecture des messages signés et chiffrés.
//!
//! `ouvrir` rend, pour un message signé ou chiffré, un message MIME ordinaire
//! — les en-têtes extérieurs, suivis du contenu déballé ou déchiffré — et ce
//! qu'on sait de sa sécurité. Tout l'affichage (corps, pièces jointes,
//! réponse, transfert) s'applique ensuite à ce message sans rien savoir de
//! S/MIME.
//!
//! Les briques sont éprouvées, la logique qui les assemble est ici :
//! - structures CMS et X.509 : `cms`, `x509-cert` et `der` (lecture BER :
//!   Outlook et Thunderbird écrivent leurs enveloppes en longueurs
//!   indéfinies) ;
//! - signature, chiffrement, empreintes : AWS-LC (`aws-lc-rs`), sauf le 3DES
//!   des anciens messages (`des`), qu'AWS-LC n'expose pas sans liaisons
//!   complètes ;
//! - chaîne de certificats : `rustls-webpki`, avec l'usage `emailProtection`
//!   exigé s'il est déclaré ;
//! - fichiers `.p12` : `p12-keystore`.
//!
//! La révocation n'est pas interrogée (CRL, OCSP) : la requête dirait à
//! l'autorité que le message est lu, comme une image distante.

use std::time::Duration;

use aws_lc_rs::cipher::{DecryptionContext, PaddedBlockDecryptingKey, UnboundCipherKey, AES_128, AES_192, AES_256};
use aws_lc_rs::digest;
use aws_lc_rs::rsa::{
    OaepPrivateDecryptingKey, Pkcs1PrivateDecryptingKey, PrivateDecryptingKey, OAEP_SHA1_MGF1SHA1,
    OAEP_SHA256_MGF1SHA256, OAEP_SHA384_MGF1SHA384, OAEP_SHA512_MGF1SHA512,
};
use aws_lc_rs::signature::{self as sig, UnparsedPublicKey, VerificationAlgorithm};
use cms::cert::CertificateChoices;
use cms::content_info::ContentInfo;
use cms::enveloped_data::{EnvelopedData, RecipientIdentifier, RecipientInfo};
use cms::signed_data::{SignedData, SignerIdentifier};
use der::asn1::OctetString;
use der::oid::ObjectIdentifier;
use der::{Decode, Encode, EncodingRules};
use mail_parser::{MessageParser, MimeHeaders};
use rustls::pki_types::{CertificateDer, TrustAnchor, UnixTime};
use serde::Serialize;
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::{SubjectAltName, SubjectKeyIdentifier};
use x509_cert::Certificate;

const fn oid(texte: &str) -> ObjectIdentifier {
    ObjectIdentifier::new_unwrap(texte)
}

const ID_SIGNED_DATA: ObjectIdentifier = oid("1.2.840.113549.1.7.2");
const ID_ENVELOPED_DATA: ObjectIdentifier = oid("1.2.840.113549.1.7.3");
const ID_AUTH_ENVELOPED_DATA: ObjectIdentifier = oid("1.2.840.113549.1.9.16.1.23");
const ATTR_MESSAGE_DIGEST: ObjectIdentifier = oid("1.2.840.113549.1.9.4");
const ATTR_SIGNING_TIME: ObjectIdentifier = oid("1.2.840.113549.1.9.5");

const SHA1: ObjectIdentifier = oid("1.3.14.3.2.26");
const SHA256: ObjectIdentifier = oid("2.16.840.1.101.3.4.2.1");
const SHA384: ObjectIdentifier = oid("2.16.840.1.101.3.4.2.2");
const SHA512: ObjectIdentifier = oid("2.16.840.1.101.3.4.2.3");

const RSA: ObjectIdentifier = oid("1.2.840.113549.1.1.1");
const RSA_OAEP: ObjectIdentifier = oid("1.2.840.113549.1.1.7");
const RSA_PSS: ObjectIdentifier = oid("1.2.840.113549.1.1.10");
const SHA1_RSA: ObjectIdentifier = oid("1.2.840.113549.1.1.5");
const SHA256_RSA: ObjectIdentifier = oid("1.2.840.113549.1.1.11");
const SHA384_RSA: ObjectIdentifier = oid("1.2.840.113549.1.1.12");
const SHA512_RSA: ObjectIdentifier = oid("1.2.840.113549.1.1.13");
const EC_PUBLIC_KEY: ObjectIdentifier = oid("1.2.840.10045.2.1");
const ECDSA_SHA256: ObjectIdentifier = oid("1.2.840.10045.4.3.2");
const ECDSA_SHA384: ObjectIdentifier = oid("1.2.840.10045.4.3.3");
const ECDSA_SHA512: ObjectIdentifier = oid("1.2.840.10045.4.3.4");
const P256: ObjectIdentifier = oid("1.2.840.10045.3.1.7");
const P384: ObjectIdentifier = oid("1.3.132.0.34");

const AES128_CBC: ObjectIdentifier = oid("2.16.840.1.101.3.4.1.2");
const AES192_CBC: ObjectIdentifier = oid("2.16.840.1.101.3.4.1.22");
const AES256_CBC: ObjectIdentifier = oid("2.16.840.1.101.3.4.1.42");
const DES_EDE3_CBC: ObjectIdentifier = oid("1.2.840.113549.3.7");

/// Usage `emailProtection` (1.3.6.1.5.5.7.3.4), sous la forme qu'attend
/// webpki : le contenu de l'OID, sans étiquette ni longueur.
const EKU_COURRIER: &[u8] = &[0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x04];

/// Emboîtements lus au plus : signé dans chiffré dans signé (triple
/// enveloppe), au-delà le message reste tel quel.
const EMBOITEMENTS: usize = 4;

// ------------------------------------------------------------------ clés

/// Une clé S/MIME d'un compte : son certificat, les intermédiaires livrés
/// avec, et la clé privée (PKCS#8), qui ne quitte pas la mémoire.
#[derive(Clone)]
pub struct Cle {
    pub certificat: Vec<u8>,
    pub intermediaires: Vec<Vec<u8>>,
    privee: Vec<u8>,
}

impl std::fmt::Debug for Cle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cle").field("empreinte", &empreinte(&self.certificat)).finish_non_exhaustive()
    }
}

impl Cle {
    /// Lit un fichier `.p12` (PKCS#12) : formats récents (PBES2, AES) et
    /// anciens (3DES, RC2-40).
    pub fn depuis_p12(octets: &[u8], mot_de_passe: &str) -> Result<Cle, String> {
        use p12_keystore::{KeyStore, Pkcs12ImportPolicy};
        let magasin = KeyStore::from_pkcs12(octets, mot_de_passe, Pkcs12ImportPolicy::Strict)
            .map_err(|e| format!("fichier .p12 illisible ou mot de passe incorrect ({e})"))?;
        let (_, chaine) = magasin.private_key_chain().ok_or("le fichier .p12 ne contient pas de clé privée")?;
        let (premier, reste) =
            chaine.certs().split_first().ok_or("le fichier .p12 ne contient pas le certificat de sa clé")?;
        let cle = Cle {
            certificat: premier.as_der().to_vec(),
            // La racine, si le fichier la porte, ne sert à rien dans un message :
            // le destinataire doit déjà la connaître.
            intermediaires: reste.iter().map(|c| c.as_der().to_vec()).filter(|c| !auto_signe(c)).collect(),
            privee: chaine.key().as_der().to_vec(),
        };
        PrivateDecryptingKey::from_pkcs8(&cle.privee)
            .map_err(|_| "clé privée d'un type non pris en charge (RSA seulement)".to_string())?;
        Ok(cle)
    }

    pub fn empreinte(&self) -> String {
        empreinte(&self.certificat)
    }

    pub fn titulaire(&self) -> Option<Titulaire> {
        titulaire(&self.certificat)
    }
}

fn auto_signe(der: &[u8]) -> bool {
    Certificate::from_der(der).map(|c| c.tbs_certificate().issuer() == c.tbs_certificate().subject()).unwrap_or(false)
}

/// Empreinte SHA-256 d'un certificat, en hexadécimal : son identifiant dans
/// le profil et au coffre.
pub fn empreinte(der: &[u8]) -> String {
    digest::digest(&digest::SHA256, der).as_ref().iter().map(|o| format!("{o:02x}")).collect()
}

/// Ce qu'un certificat dit de son titulaire, pour l'affichage.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Titulaire {
    pub nom: String,
    pub adresses: Vec<String>,
    pub emetteur: String,
    pub debut: i64,
    pub fin: i64,
    pub empreinte: String,
}

pub fn titulaire(der: &[u8]) -> Option<Titulaire> {
    let c = Certificate::from_der(der).ok()?;
    let tbs = c.tbs_certificate();
    let nom_de = |n: &x509_cert::name::Name| match n.common_name() {
        Ok(Some(cn)) => cn.value().into_owned(),
        _ => n.to_string(),
    };
    let mut adresses: Vec<String> = Vec::new();
    if let Ok(Some((_, san))) = tbs.get_extension::<SubjectAltName>() {
        for nom in san.0.iter() {
            if let GeneralName::Rfc822Name(a) = nom {
                adresses.push(a.as_str().to_ascii_lowercase());
            }
        }
    }
    if adresses.is_empty() {
        if let Ok(Some(a)) = tbs.subject().email_address() {
            adresses.push(a.as_str().to_ascii_lowercase());
        }
    }
    let validite = tbs.validity();
    Some(Titulaire {
        nom: nom_de(tbs.subject()),
        adresses,
        emetteur: nom_de(tbs.issuer()),
        debut: validite.not_before.to_unix_duration().as_secs() as i64,
        fin: validite.not_after.to_unix_duration().as_secs() as i64,
        empreinte: empreinte(der),
    })
}

// ------------------------------------------------------------- autorités

/// Les autorités de certification de confiance : celles du système (de
/// Mozilla sous Android), et celles ajoutées dans MMail.
pub struct Autorites {
    ancres: Vec<TrustAnchor<'static>>,
}

impl Autorites {
    /// Seulement les autorités données (certificats DER) : les essais, et
    /// le complément du système.
    pub fn de(certificats: &[Vec<u8>]) -> Autorites {
        let ancres = certificats
            .iter()
            .filter_map(|c| {
                let der = CertificateDer::from(c.as_slice());
                webpki::anchor_from_trusted_cert(&der).ok().map(|a| a.to_owned())
            })
            .collect();
        Autorites { ancres }
    }

    /// Celles du système, et les `ajoutees`.
    pub fn du_systeme(ajoutees: &[Vec<u8>]) -> Autorites {
        let mut autorites = Autorites::de(ajoutees);
        #[cfg(not(target_os = "android"))]
        {
            let systeme = rustls_native_certs::load_native_certs();
            for c in systeme.certs {
                if let Ok(a) = webpki::anchor_from_trusted_cert(&c) {
                    autorites.ancres.push(a.to_owned());
                }
            }
        }
        #[cfg(target_os = "android")]
        autorites.ancres.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        autorites
    }

    pub fn nombre(&self) -> usize {
        self.ancres.len()
    }
}

// ------------------------------------------------------------- sécurité

/// Ce qu'on sait de la sécurité d'un message ouvert.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Securite {
    /// Le message était chiffré.
    pub chiffre: bool,
    /// Pourquoi son contenu n'a pas pu être déchiffré (vide : déchiffré).
    pub illisible: String,
    /// Chiffré par un algorithme ancien (3DES).
    pub ancien: bool,
    pub signature: Option<Signature>,
}

/// La signature d'un message, contrôlée.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    /// `verifiee`, `expiree` (signée avant l'échéance du certificat),
    /// `adresse` (certificat d'une autre adresse que l'expéditeur),
    /// `inconnue` (autorité inconnue), `usage` (certificat non prévu pour le
    /// courrier), `faible` (SHA-1), `modifiee` (contenu ou signature
    /// altérés), `invalide` (autre défaut).
    pub etat: String,
    /// La phrase à afficher.
    pub texte: String,
    pub titulaire: Option<Titulaire>,
    /// Heure de signature déclarée par le signataire (0 : absente).
    pub signe_le: i64,
    /// Le certificat du signataire, à recueillir si la signature est bonne.
    #[serde(skip)]
    pub certificat: Vec<u8>,
    /// Les autres certificats du message (intermédiaires), recueillis avec
    /// lui : sans eux, sa chaîne ne remonte pas jusqu'à l'autorité.
    #[serde(skip)]
    pub intermediaires: Vec<Vec<u8>>,
}

impl Signature {
    fn defaut(etat: &str, texte: String) -> Signature {
        Signature { etat: etat.into(), texte, titulaire: None, signe_le: 0, certificat: Vec::new(), intermediaires: Vec::new() }
    }

    /// Bonne au point d'en recueillir le certificat, pour chiffrer vers son
    /// titulaire : vérifiée, adresse comprise.
    pub fn recueillable(&self) -> bool {
        self.etat == "verifiee" && !self.certificat.is_empty()
    }
}

/// Un message signé ou chiffré, rendu lisible.
#[derive(Debug, Clone, PartialEq)]
pub struct Ouvert {
    /// Le message MIME ordinaire à afficher.
    pub message: Vec<u8>,
    pub securite: Securite,
}

/// Vrai si le message est signé ou chiffré en S/MIME (lecture des seuls
/// en-têtes de premier niveau).
pub fn est_smime(brut: &[u8]) -> bool {
    !matches!(forme(brut), Forme::Autre)
}

/// Ouvre un message signé ou chiffré ; `None` s'il n'est ni l'un ni l'autre.
/// `maintenant` : l'heure du contrôle des certificats, en secondes Unix.
pub fn ouvrir(brut: &[u8], cles: &[Cle], autorites: &Autorites, maintenant: i64) -> Option<Ouvert> {
    if matches!(forme(brut), Forme::Autre) {
        return None;
    }
    let entetes = &brut[..fin_entetes(brut).unwrap_or(brut.len())];
    let expediteur = MessageParser::default()
        .parse_headers(brut)
        .and_then(|m| m.from().and_then(|f| f.first()).and_then(|a| a.address()).map(|a| a.to_ascii_lowercase()));
    let mut securite = Securite::default();
    let mut entite: Vec<u8> = brut.to_vec();
    for _ in 0..EMBOITEMENTS {
        match forme(&entite) {
            Forme::Autre => break,
            Forme::Detachee { contenu, signature } => {
                let (s, _) = verifier(&signature, Some(&contenu), expediteur.as_deref(), autorites, maintenant);
                securite.signature.get_or_insert(s);
                entite = contenu;
            }
            Forme::Pkcs7(der) => match ContentInfo::from_ber(&der) {
                Ok(ci) if ci.content_type == ID_ENVELOPED_DATA => {
                    securite.chiffre = true;
                    match dechiffrer(&ci, cles) {
                        Ok((contenu, ancien)) => {
                            securite.ancien |= ancien;
                            entite = contenu;
                        }
                        Err(motif) => {
                            securite.illisible = motif;
                            entite = texte_seul(&format!(
                                "Ce message est chiffré, et MMail ne peut pas le lire : {}.",
                                securite.illisible
                            ));
                            break;
                        }
                    }
                }
                Ok(ci) if ci.content_type == ID_AUTH_ENVELOPED_DATA => {
                    securite.chiffre = true;
                    securite.illisible = "chiffrement AES-GCM (S/MIME 4.0), pas encore pris en charge".into();
                    entite = texte_seul(&format!(
                        "Ce message est chiffré, et MMail ne peut pas le lire : {}.",
                        securite.illisible
                    ));
                    break;
                }
                Ok(ci) if ci.content_type == ID_SIGNED_DATA => {
                    let (s, contenu) = verifier(&der, None, expediteur.as_deref(), autorites, maintenant);
                    securite.signature.get_or_insert(s);
                    match contenu {
                        Some(contenu) => entite = contenu,
                        None => break,
                    }
                }
                _ => break,
            },
        }
    }
    Some(Ouvert { message: recomposer(entetes, &entite), securite })
}

/// Un corps de texte seul, à la place d'un contenu illisible.
fn texte_seul(texte: &str) -> Vec<u8> {
    format!("Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{texte}\r\n").into_bytes()
}

// --------------------------------------------------------------- formes

enum Forme {
    Autre,
    /// `multipart/signed` : l'entité signée, telle que transmise, et la
    /// signature (CMS, décodée de sa base64).
    Detachee { contenu: Vec<u8>, signature: Vec<u8> },
    /// `application/pkcs7-mime` : la structure CMS, décodée.
    Pkcs7(Vec<u8>),
}

fn forme(entite: &[u8]) -> Forme {
    let Some(m) = MessageParser::default().parse(entite) else {
        return Forme::Autre;
    };
    let racine = m.root_part();
    let Some(ct) = racine.content_type() else {
        return Forme::Autre;
    };
    let type_ = ct.ctype().to_ascii_lowercase();
    let sous = ct.subtype().unwrap_or("").to_ascii_lowercase();
    match (type_.as_str(), sous.as_str()) {
        ("multipart", "signed") => {
            let protocole = ct.attribute("protocol").unwrap_or("").to_ascii_lowercase();
            if !protocole.ends_with("pkcs7-signature") {
                return Forme::Autre;
            }
            let Some(frontiere) = ct.attribute("boundary") else {
                return Forme::Autre;
            };
            let Some((contenu, partie)) = decouper_signe(entite, frontiere) else {
                return Forme::Autre;
            };
            let signature = MessageParser::default()
                .parse(&partie)
                .map(|p| p.root_part().contents().to_vec())
                .unwrap_or_default();
            if signature.is_empty() {
                return Forme::Autre;
            }
            Forme::Detachee { contenu, signature }
        }
        ("application", "pkcs7-mime" | "x-pkcs7-mime") => Forme::Pkcs7(racine.contents().to_vec()),
        // Quelques logiciels envoient l'enveloppe en type générique, nommée
        // `smime.p7m`.
        ("application", "octet-stream")
            if racine
                .attachment_name()
                .map(|n| n.to_ascii_lowercase().ends_with(".p7m"))
                .unwrap_or(false) =>
        {
            Forme::Pkcs7(racine.contents().to_vec())
        }
        _ => Forme::Autre,
    }
}

/// Position du premier octet du corps d'une entité MIME (après la ligne
/// vide qui clôt ses en-têtes).
fn fin_entetes(entite: &[u8]) -> Option<usize> {
    if entite.starts_with(b"\r\n") {
        return Some(2);
    }
    if entite.starts_with(b"\n") {
        return Some(1);
    }
    let crlf = entite.windows(4).position(|f| f == b"\r\n\r\n").map(|p| p + 4);
    let lf = entite.windows(2).position(|f| f == b"\n\n").map(|p| p + 2);
    match (crlf, lf) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Découpe un `multipart/signed` sur sa frontière : l'entité signée, octet
/// pour octet (le saut de ligne qui précède une frontière lui appartient,
/// RFC 2046), et la partie de la signature.
fn decouper_signe(entite: &[u8], frontiere: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    let debut = fin_entetes(entite)?;
    let corps = &entite[debut..];
    let marque = format!("--{frontiere}");
    let marque = marque.as_bytes();
    // Débuts des lignes qui commencent par la frontière.
    let mut lignes = Vec::new();
    let mut i = 0;
    while i < corps.len() {
        if corps[i..].starts_with(marque) {
            lignes.push(i);
        }
        match corps[i..].iter().position(|&o| o == b'\n') {
            Some(p) => i += p + 1,
            None => break,
        }
    }
    let apres_ligne = |p: usize| corps[p..].iter().position(|&o| o == b'\n').map(|q| p + q + 1);
    let avant_ligne = |p: usize| {
        if p >= 2 && &corps[p - 2..p] == b"\r\n" {
            p - 2
        } else if p >= 1 && corps[p - 1] == b'\n' {
            p - 1
        } else {
            p
        }
    };
    let (&a, &b, &c) = (lignes.first()?, lignes.get(1)?, lignes.get(2)?);
    let contenu = corps[apres_ligne(a)?..avant_ligne(b)].to_vec();
    let partie = corps[apres_ligne(b)?..avant_ligne(c)].to_vec();
    Some((contenu, partie))
}

/// Les en-têtes extérieurs, moins ceux du contenu, suivis de l'entité
/// intérieure (ses propres en-têtes `Content-*`, puis son corps).
fn recomposer(entetes: &[u8], interieur: &[u8]) -> Vec<u8> {
    let texte = String::from_utf8_lossy(entetes);
    let mut sortie = Vec::with_capacity(entetes.len() + interieur.len());
    let mut garder = true;
    for ligne in texte.split_inclusive('\n') {
        if ligne.trim_end_matches(['\r', '\n']).is_empty() {
            break;
        }
        if !ligne.starts_with([' ', '\t']) {
            let nom = ligne.split(':').next().unwrap_or("").trim().to_ascii_lowercase();
            garder = !nom.starts_with("content-");
        }
        if garder {
            sortie.extend_from_slice(ligne.as_bytes());
            if !ligne.ends_with('\n') {
                sortie.extend_from_slice(b"\r\n");
            }
        }
    }
    // Une entité sans en-têtes commence par la ligne vide : elle sépare
    // alors les en-têtes extérieurs du corps.
    sortie.extend_from_slice(interieur);
    sortie
}

/// Le même contenu en fins de ligne CRLF : la forme canonique, que le
/// signataire a signée, quand un relais a réécrit les fins de ligne.
fn en_crlf(octets: &[u8]) -> Vec<u8> {
    let mut sortie = Vec::with_capacity(octets.len() + octets.len() / 32);
    let mut precedent = 0u8;
    for &o in octets {
        if o == b'\n' && precedent != b'\r' {
            sortie.push(b'\r');
        }
        sortie.push(o);
        precedent = o;
    }
    sortie
}

// ---------------------------------------------------------- vérification

/// Contrôle une signature CMS. `detache` : le contenu signé d'un
/// `multipart/signed` ; sinon le contenu est dans la structure et revient
/// avec le résultat.
fn verifier(
    der: &[u8],
    detache: Option<&[u8]>,
    expediteur: Option<&str>,
    autorites: &Autorites,
    maintenant: i64,
) -> (Signature, Option<Vec<u8>>) {
    let illisible = |motif: &str| (Signature::defaut("invalide", format!("Signature illisible : {motif}.")), None);
    let Ok(ci) = ContentInfo::from_ber(der) else {
        return illisible("structure CMS invalide");
    };
    if ci.content_type != ID_SIGNED_DATA {
        return illisible("ce n'est pas une signature");
    }
    let Ok(sd) = ci.content.decode_as_encoding::<SignedData>(EncodingRules::Ber) else {
        return illisible("structure de signature invalide");
    };
    let encapsule = sd
        .encap_content_info
        .econtent
        .as_ref()
        .and_then(|a| a.decode_as_encoding::<OctetString>(EncodingRules::Ber).ok())
        .map(|o| o.as_bytes().to_vec());
    let contenu: Vec<u8> = match (detache, &encapsule) {
        (Some(d), _) => d.to_vec(),
        (None, Some(e)) => e.clone(),
        (None, None) => return illisible("contenu signé absent"),
    };
    let rendu = if detache.is_some() { None } else { Some(contenu.clone()) };

    let certificats: Vec<(Vec<u8>, Certificate)> = sd
        .certificates
        .as_ref()
        .map(|cs| {
            cs.0.iter()
                .filter_map(|c| match c {
                    CertificateChoices::Certificate(c) => c.to_der().ok().map(|d| (d, c.clone())),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let Some(signataire) = sd.signer_infos.0.iter().next() else {
        return (Signature::defaut("invalide", "Message signé sans signataire.".into()), rendu);
    };
    let trouve = certificats.iter().find(|(_, c)| {
        let tbs = c.tbs_certificate();
        match &signataire.sid {
            SignerIdentifier::IssuerAndSerialNumber(ias) => {
                ias.issuer == *tbs.issuer() && ias.serial_number == *tbs.serial_number()
            }
            SignerIdentifier::SubjectKeyIdentifier(ski) => cle_de(c).as_ref() == Some(ski),
        }
    });
    let Some((cert_der, cert)) = trouve else {
        return (
            Signature::defaut("invalide", "Le certificat du signataire n'accompagne pas le message.".into()),
            rendu,
        );
    };
    let qui = titulaire(cert_der);
    let mut s = Signature {
        etat: String::new(),
        texte: String::new(),
        titulaire: qui.clone(),
        signe_le: 0,
        certificat: cert_der.clone(),
        intermediaires: certificats.iter().map(|(d, _)| d.clone()).filter(|d| d != cert_der).collect(),
    };
    let nom = qui
        .as_ref()
        .map(|t| match t.adresses.first() {
            Some(a) => format!("{} <{a}>", t.nom),
            None => t.nom.clone(),
        })
        .unwrap_or_else(|| "un signataire".into());

    // Empreinte du contenu.
    let hachage = match signataire.digest_alg.oid {
        SHA256 => &digest::SHA256,
        SHA384 => &digest::SHA384,
        SHA512 => &digest::SHA512,
        SHA1 => {
            s.etat = "faible".into();
            s.texte = format!("Signé par {nom} en SHA-1, algorithme trop faible : signature non vérifiée.");
            return (s, rendu);
        }
        _ => {
            s.etat = "invalide".into();
            s.texte = format!("Signé par {nom} : algorithme d'empreinte non pris en charge.");
            return (s, rendu);
        }
    };
    // Message à vérifier : les attributs signés (forme DER, étiquette SET),
    // qui portent l'empreinte du contenu ; à défaut, le contenu lui-même.
    let (signe, contenu_ok): (Vec<u8>, bool) = match &signataire.signed_attrs {
        Some(attributs) => {
            let mut annonce: Option<Vec<u8>> = None;
            for a in attributs.iter() {
                if a.oid == ATTR_MESSAGE_DIGEST {
                    annonce = a
                        .values
                        .iter()
                        .next()
                        .and_then(|v| v.decode_as::<OctetString>().ok())
                        .map(|o| o.as_bytes().to_vec());
                } else if a.oid == ATTR_SIGNING_TIME {
                    // Choix UTCTime / GeneralizedTime : décodé depuis sa forme complète.
                    let heure = a.values.iter().next().and_then(|v| v.to_der().ok());
                    if let Some(t) = heure.and_then(|d| x509_cert::time::Time::from_der(&d).ok()) {
                        s.signe_le = t.to_unix_duration().as_secs() as i64;
                    }
                }
            }
            let concorde = |octets: &[u8]| annonce.as_deref() == Some(digest::digest(hachage, octets).as_ref());
            let ok = concorde(&contenu) || (detache.is_some() && concorde(&en_crlf(&contenu)));
            (attributs.to_der().unwrap_or_default(), ok)
        }
        None => {
            // Sans attributs, la signature porte sur le contenu ; on essaie aussi
            // sa forme canonique.
            (contenu.clone(), true)
        }
    };
    let verifie = |message: &[u8]| verifier_signature(cert, signataire.signature_algorithm.oid, hachage, message, signataire.signature.as_bytes());
    let signature_ok = match &signataire.signed_attrs {
        Some(_) => contenu_ok && verifie(&signe),
        None => verifie(&signe) || (detache.is_some() && verifie(&en_crlf(&contenu))),
    };
    if !signature_ok {
        s.etat = "modifiee".into();
        s.texte = format!("Signé par {nom}, mais le message a été modifié depuis sa signature : ne pas s'y fier.");
        return (s, rendu);
    }

    // Chaîne jusqu'à une autorité de confiance, usage compris.
    let intermediaires: Vec<CertificateDer<'_>> = certificats
        .iter()
        .filter(|(d, _)| d != cert_der)
        .map(|(d, _)| CertificateDer::from(d.as_slice()))
        .collect();
    let chaine = |a_l_heure: i64| -> Result<(), webpki::Error> {
        let fin = CertificateDer::from(cert_der.as_slice());
        let ee = webpki::EndEntityCert::try_from(&fin)?;
        ee.verify_for_usage(
            ALGORITHMES,
            &autorites.ancres,
            &intermediaires,
            UnixTime::since_unix_epoch(Duration::from_secs(a_l_heure.max(0) as u64)),
            webpki::KeyUsage::required_if_present(EKU_COURRIER),
            None,
            None,
        )
        .map(|_| ())
    };
    let echeance = qui.as_ref().map(|t| date(t.fin)).unwrap_or_default();
    let mut expiree = false;
    if let Err(e) = chaine(maintenant) {
        let e = match e {
            webpki::Error::CertExpired { .. } if s.signe_le > 0 => match chaine(s.signe_le) {
                Ok(()) => {
                    expiree = true;
                    None
                }
                Err(e2) => Some(e2),
            },
            e => Some(e),
        };
        if let Some(e) = e {
            (s.etat, s.texte) = match e {
                webpki::Error::UnknownIssuer => (
                    "inconnue".into(),
                    format!("Signé par {nom}, certificat émis par une autorité inconnue : l'identité du signataire n'est pas garantie."),
                ),
                #[allow(deprecated)]
                webpki::Error::RequiredEkuNotFound | webpki::Error::RequiredEkuNotFoundContext(_) => (
                    "usage".into(),
                    format!("Signé par {nom}, avec un certificat qui n'est pas prévu pour le courrier électronique."),
                ),
                webpki::Error::CertExpired { .. } => (
                    "invalide".into(),
                    format!("Signé par {nom}, avec un certificat expiré le {echeance}."),
                ),
                webpki::Error::CertNotValidYet { .. } => (
                    "invalide".into(),
                    format!("Signé par {nom}, avec un certificat pas encore valide."),
                ),
                e => ("invalide".into(), format!("Signé par {nom} : certificat refusé ({e:?}).")),
            };
            return (s, rendu);
        }
    }

    // Concordance de l'adresse.
    let adresses = qui.as_ref().map(|t| t.adresses.clone()).unwrap_or_default();
    if let Some(de) = expediteur {
        if !adresses.iter().any(|a| a.eq_ignore_ascii_case(de)) {
            s.etat = "adresse".into();
            s.texte = format!(
                "Signé par {nom}, qui n'est pas l'adresse de l'expéditeur ({de}) : la signature ne garantit pas l'expéditeur."
            );
            return (s, rendu);
        }
    }
    if expiree {
        s.etat = "expiree".into();
        s.texte = format!("Signé par {nom} : signature vérifiée, faite avant l'expiration du certificat le {echeance}.");
    } else {
        s.etat = "verifiee".into();
        s.texte = format!("Signé par {nom} : signature vérifiée.");
    }
    (s, rendu)
}

/// Algorithmes admis dans les chaînes de certificats ; ni SHA-1 ni RSA de
/// moins de 2048 bits.
static ALGORITHMES: &[&dyn rustls::pki_types::SignatureVerificationAlgorithm] = &[
    webpki::aws_lc_rs::RSA_PKCS1_2048_8192_SHA256,
    webpki::aws_lc_rs::RSA_PKCS1_2048_8192_SHA384,
    webpki::aws_lc_rs::RSA_PKCS1_2048_8192_SHA512,
    webpki::aws_lc_rs::RSA_PKCS1_2048_8192_SHA256_ABSENT_PARAMS,
    webpki::aws_lc_rs::RSA_PKCS1_2048_8192_SHA384_ABSENT_PARAMS,
    webpki::aws_lc_rs::RSA_PKCS1_2048_8192_SHA512_ABSENT_PARAMS,
    webpki::aws_lc_rs::RSA_PSS_2048_8192_SHA256_LEGACY_KEY,
    webpki::aws_lc_rs::RSA_PSS_2048_8192_SHA384_LEGACY_KEY,
    webpki::aws_lc_rs::RSA_PSS_2048_8192_SHA512_LEGACY_KEY,
    webpki::aws_lc_rs::ECDSA_P256_SHA256,
    webpki::aws_lc_rs::ECDSA_P256_SHA384,
    webpki::aws_lc_rs::ECDSA_P384_SHA256,
    webpki::aws_lc_rs::ECDSA_P384_SHA384,
    webpki::aws_lc_rs::ED25519,
];

/// Identifiant de clé (extension `subjectKeyIdentifier`) d'un certificat.
fn cle_de(c: &Certificate) -> Option<SubjectKeyIdentifier> {
    c.tbs_certificate().get_extension::<SubjectKeyIdentifier>().ok().flatten().map(|(_, ski)| ski)
}

/// Vérifie `signature` sur `message` par la clé publique du certificat.
fn verifier_signature(
    cert: &Certificate,
    algorithme: ObjectIdentifier,
    hachage: &'static digest::Algorithm,
    message: &[u8],
    signature: &[u8],
) -> bool {
    let spki = cert.tbs_certificate().subject_public_key_info();
    let cle = spki.subject_public_key.raw_bytes();
    let par_hachage = |h256: &'static dyn VerificationAlgorithm,
                       h384: &'static dyn VerificationAlgorithm,
                       h512: &'static dyn VerificationAlgorithm| {
        if hachage == &digest::SHA256 {
            h256
        } else if hachage == &digest::SHA384 {
            h384
        } else {
            h512
        }
    };
    let alg: &'static dyn VerificationAlgorithm = match algorithme {
        RSA | SHA256_RSA | SHA384_RSA | SHA512_RSA if spki.algorithm.oid == RSA => par_hachage(
            &sig::RSA_PKCS1_2048_8192_SHA256,
            &sig::RSA_PKCS1_2048_8192_SHA384,
            &sig::RSA_PKCS1_2048_8192_SHA512,
        ),
        RSA_PSS if spki.algorithm.oid == RSA || spki.algorithm.oid == RSA_PSS => par_hachage(
            &sig::RSA_PSS_2048_8192_SHA256,
            &sig::RSA_PSS_2048_8192_SHA384,
            &sig::RSA_PSS_2048_8192_SHA512,
        ),
        EC_PUBLIC_KEY | ECDSA_SHA256 | ECDSA_SHA384 | ECDSA_SHA512 if spki.algorithm.oid == EC_PUBLIC_KEY => {
            let courbe = spki.algorithm.parameters.as_ref().and_then(|p| p.decode_as::<ObjectIdentifier>().ok());
            match courbe {
                Some(P256) if hachage == &digest::SHA384 => &sig::ECDSA_P256_SHA384_ASN1,
                Some(P256) => &sig::ECDSA_P256_SHA256_ASN1,
                Some(P384) if hachage == &digest::SHA256 => &sig::ECDSA_P384_SHA256_ASN1,
                Some(P384) => &sig::ECDSA_P384_SHA384_ASN1,
                _ => return false,
            }
        }
        SHA1_RSA => return false,
        _ => return false,
    };
    UnparsedPublicKey::new(alg, cle).verify(message, signature).is_ok()
}

fn date(secondes: i64) -> String {
    chrono::DateTime::from_timestamp(secondes, 0).map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default()
}

// ---------------------------------------------------------- déchiffrement

/// Déchiffre une enveloppe CMS par l'une des clés : le contenu, et s'il
/// était chiffré par un algorithme ancien.
fn dechiffrer(ci: &ContentInfo, cles: &[Cle]) -> Result<(Vec<u8>, bool), String> {
    let ed = ci
        .content
        .decode_as_encoding::<EnvelopedData>(EncodingRules::Ber)
        .map_err(|_| "enveloppe illisible".to_string())?;
    if cles.is_empty() {
        return Err("aucun certificat S/MIME n'est installé pour ce compte".into());
    }
    let mut elliptique = false;
    let mut cle_contenu: Option<Vec<u8>> = None;
    'destinataires: for ri in ed.recip_infos.0.iter() {
        let ktri = match ri {
            RecipientInfo::Ktri(k) => k,
            RecipientInfo::Kari(_) => {
                elliptique = true;
                continue;
            }
            _ => continue,
        };
        for cle in cles {
            let Ok(cert) = Certificate::from_der(&cle.certificat) else {
                continue;
            };
            let tbs = cert.tbs_certificate();
            let pour_elle = match &ktri.rid {
                RecipientIdentifier::IssuerAndSerialNumber(ias) => {
                    ias.issuer == *tbs.issuer() && ias.serial_number == *tbs.serial_number()
                }
                RecipientIdentifier::SubjectKeyIdentifier(ski) => cle_de(&cert).as_ref() == Some(ski),
            };
            if !pour_elle {
                continue;
            }
            if let Some(k) = dechiffrer_cle(cle, &ktri.key_enc_alg, ktri.enc_key.as_bytes()) {
                cle_contenu = Some(k);
                break 'destinataires;
            }
        }
    }
    let Some(cle_contenu) = cle_contenu else {
        return Err(if elliptique {
            "chiffré pour une clé elliptique, pas encore prise en charge".into()
        } else {
            "aucun certificat de ce compte ne fait partie de ses destinataires".into()
        });
    };
    let eci = &ed.encrypted_content;
    let chiffre = eci.encrypted_content.as_ref().ok_or("contenu chiffré absent")?.as_bytes();
    let iv = eci
        .content_enc_alg
        .parameters
        .as_ref()
        .and_then(|p| p.decode_as::<OctetString>().ok())
        .map(|o| o.as_bytes().to_vec())
        .unwrap_or_default();
    let aes = match eci.content_enc_alg.oid {
        AES128_CBC => Some(&AES_128),
        AES192_CBC => Some(&AES_192),
        AES256_CBC => Some(&AES_256),
        _ => None,
    };
    if let Some(alg) = aes {
        let iv: [u8; 16] = iv.as_slice().try_into().map_err(|_| "vecteur d'initialisation invalide".to_string())?;
        let cle = UnboundCipherKey::new(alg, &cle_contenu).map_err(|_| "clé de contenu invalide".to_string())?;
        let cle = PaddedBlockDecryptingKey::cbc_pkcs7(cle).map_err(|_| "clé de contenu invalide".to_string())?;
        let mut tampon = chiffre.to_vec();
        let clair =
            cle.decrypt(&mut tampon, DecryptionContext::Iv128(iv.into())).map_err(|_| "déchiffrement impossible".to_string())?;
        return Ok((clair.to_vec(), false));
    }
    if eci.content_enc_alg.oid == DES_EDE3_CBC {
        use cbc::cipher::{block_padding::Pkcs7, BlockModeDecrypt, KeyIvInit};
        let d = cbc::Decryptor::<des::TdesEde3>::new_from_slices(&cle_contenu, &iv)
            .map_err(|_| "clé de contenu invalide".to_string())?;
        let clair = d.decrypt_padded_vec::<Pkcs7>(chiffre).map_err(|_| "déchiffrement impossible".to_string())?;
        return Ok((clair, true));
    }
    Err(format!("algorithme de chiffrement non pris en charge ({})", eci.content_enc_alg.oid))
}

/// Déchiffre la clé de contenu destinée à `cle` (RSA PKCS#1 v1.5 ou OAEP).
fn dechiffrer_cle(cle: &Cle, algorithme: &spki::AlgorithmIdentifierOwned, chiffree: &[u8]) -> Option<Vec<u8>> {
    let privee = PrivateDecryptingKey::from_pkcs8(&cle.privee).ok()?;
    let mut sortie = vec![0u8; privee.key_size_bytes()];
    if algorithme.oid == RSA {
        let k = Pkcs1PrivateDecryptingKey::new(privee).ok()?;
        return k.decrypt(chiffree, &mut sortie).ok().map(|c| c.to_vec());
    }
    if algorithme.oid == RSA_OAEP {
        // Paramètres OAEP : l'empreinte par défaut est SHA-1 ; les autres
        // se reconnaissent à leur OID dans les paramètres.
        let parametres = algorithme.parameters.as_ref().and_then(|p| p.to_der().ok()).unwrap_or_default();
        let contient = |o: ObjectIdentifier| {
            let motif = o.as_bytes();
            !motif.is_empty() && parametres.windows(motif.len()).any(|f| f == motif)
        };
        let oaep = if contient(SHA512) {
            &OAEP_SHA512_MGF1SHA512
        } else if contient(SHA384) {
            &OAEP_SHA384_MGF1SHA384
        } else if contient(SHA256) {
            &OAEP_SHA256_MGF1SHA256
        } else {
            &OAEP_SHA1_MGF1SHA1
        };
        let k = OaepPrivateDecryptingKey::new(privee).ok()?;
        return k.decrypt(oaep, chiffree, &mut sortie, None).ok().map(|c| c.to_vec());
    }
    None
}

// ---------------------------------------------------------------- profil

/// Rangement dans le profil : `smime/cles/<adresse>/<empreinte>.p12`, chaque
/// fichier toujours protégé par son mot de passe (au coffre du système, sous
/// `smime:<empreinte>`) ; `smime/autorites/<empreinte>.cer`, les autorités
/// ajoutées dans MMail.
/// `profil` est le chemin de l'index (`…/index.sqlite`), comme pour les
/// autres dossiers du profil : `smime/` est à côté de lui.
fn racine(profil: &str) -> std::path::PathBuf {
    std::path::Path::new(profil)
        .parent()
        .map(|p| p.join("smime"))
        .unwrap_or_else(|| std::path::PathBuf::from("smime"))
}

fn dossier_cles(profil: &str) -> std::path::PathBuf {
    racine(profil).join("cles")
}

fn dossier_autorites(profil: &str) -> std::path::PathBuf {
    racine(profil).join("autorites")
}

/// Écrit un fichier entier ou pas du tout (fichier provisoire, puis
/// renommage), comme le cache.
fn ecrire_entier(chemin: &std::path::Path, octets: &[u8]) -> std::io::Result<()> {
    let provisoire = chemin.with_extension("partiel");
    std::fs::write(&provisoire, octets)?;
    std::fs::rename(&provisoire, chemin)
}

/// Une clé rangée dans le profil.
#[derive(Debug, Clone, PartialEq)]
pub struct Rangee {
    pub adresse: String,
    pub empreinte: String,
    pub chemin: std::path::PathBuf,
}

/// Range le `.p12` d'une clé lue, pour le compte `adresse`. Les anciennes
/// clés restent : elles déchiffrent les anciens messages.
pub fn ranger_cle(profil: &str, adresse: &str, p12: &[u8], cle: &Cle) -> std::io::Result<Rangee> {
    let adresse = adresse.trim().to_ascii_lowercase();
    if adresse.is_empty() || adresse.contains(['/', '\\']) || adresse.starts_with('.') {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "adresse invalide"));
    }
    let dossier = dossier_cles(profil).join(&adresse);
    std::fs::create_dir_all(&dossier)?;
    let empreinte = cle.empreinte();
    let chemin = dossier.join(format!("{empreinte}.p12"));
    ecrire_entier(&chemin, p12)?;
    // Le certificat à côté, en clair (il est public) : de quoi présenter la
    // clé avant de l'avoir ouverte.
    ecrire_entier(&chemin.with_extension("cer"), &cle.certificat)?;
    Ok(Rangee { adresse, empreinte, chemin })
}

impl Rangee {
    /// Le certificat de la clé, rangé à côté d'elle.
    pub fn certificat(&self) -> Option<Vec<u8>> {
        std::fs::read(self.chemin.with_extension("cer")).ok()
    }
}

/// Les clés rangées dans le profil, toutes adresses.
pub fn cles_rangees(profil: &str) -> Vec<Rangee> {
    let mut sortie = Vec::new();
    let Ok(adresses) = std::fs::read_dir(dossier_cles(profil)) else {
        return sortie;
    };
    for a in adresses.flatten() {
        let adresse = a.file_name().to_string_lossy().into_owned();
        let Ok(fichiers) = std::fs::read_dir(a.path()) else {
            continue;
        };
        for f in fichiers.flatten() {
            let nom = f.file_name().to_string_lossy().into_owned();
            if let Some(empreinte) = nom.strip_suffix(".p12") {
                sortie.push(Rangee { adresse: adresse.clone(), empreinte: empreinte.to_string(), chemin: f.path() });
            }
        }
    }
    sortie.sort_by(|a, b| (&a.adresse, &a.empreinte).cmp(&(&b.adresse, &b.empreinte)));
    sortie
}

/// Retire une clé du profil (sur demande, après avertissement : les messages
/// chiffrés pour elle deviennent illisibles).
pub fn retirer_cle(profil: &str, empreinte: &str) -> std::io::Result<bool> {
    let mut retiree = false;
    for r in cles_rangees(profil) {
        if r.empreinte == empreinte {
            std::fs::remove_file(&r.chemin)?;
            let _ = std::fs::remove_file(r.chemin.with_extension("cer"));
            retiree = true;
        }
    }
    oublier(empreinte);
    Ok(retiree)
}

/// Lit un certificat d'autorité, PEM ou DER.
pub fn lire_certificat(octets: &[u8]) -> Result<Vec<u8>, String> {
    let der = match std::str::from_utf8(octets) {
        Ok(texte) if texte.contains("-----BEGIN CERTIFICATE-----") => {
            let debut = texte.find("-----BEGIN CERTIFICATE-----").unwrap_or(0) + "-----BEGIN CERTIFICATE-----".len();
            let fin = texte[debut..].find("-----END CERTIFICATE-----").map(|f| debut + f).unwrap_or(texte.len());
            crate::rendu::base64(&texte[debut..fin]).ok_or("certificat PEM illisible")?
        }
        _ => octets.to_vec(),
    };
    Certificate::from_der(&der).map_err(|_| "ce fichier n'est pas un certificat X.509".to_string())?;
    Ok(der)
}

/// Ajoute une autorité de confiance au profil (autorité interne d'une
/// entreprise, en particulier sous Android).
pub fn ranger_autorite(profil: &str, der: &[u8]) -> std::io::Result<std::path::PathBuf> {
    let dossier = dossier_autorites(profil);
    std::fs::create_dir_all(&dossier)?;
    let chemin = dossier.join(format!("{}.cer", empreinte(der)));
    ecrire_entier(&chemin, der)?;
    if let Ok(mut a) = AUTORITES.write() {
        *a = None;
    }
    Ok(chemin)
}

/// Les autorités ajoutées au profil.
pub fn autorites_ajoutees(profil: &str) -> Vec<Vec<u8>> {
    let Ok(fichiers) = std::fs::read_dir(dossier_autorites(profil)) else {
        return Vec::new();
    };
    let mut sortie: Vec<Vec<u8>> = fichiers
        .flatten()
        .filter(|f| f.file_name().to_string_lossy().ends_with(".cer"))
        .filter_map(|f| std::fs::read(f.path()).ok())
        .collect();
    sortie.sort();
    sortie
}

/// Recueille le certificat d'un signataire dont la signature est bonne :
/// `smime/correspondants/<adresse>/<empreinte>.cer`, de quoi lui écrire
/// chiffré ensuite. Rend vrai s'il est nouveau.
pub fn recueillir(profil: &str, signature: &Signature) -> bool {
    if !signature.recueillable() {
        return false;
    }
    let Some(t) = titulaire(&signature.certificat) else {
        return false;
    };
    let mut nouveau = false;
    for adresse in &t.adresses {
        if adresse.is_empty() || adresse.contains(['/', '\\']) || adresse.starts_with('.') {
            continue;
        }
        let dossier = racine(profil).join("correspondants").join(adresse);
        let chemin = dossier.join(format!("{}.cer", t.empreinte));
        if chemin.exists() {
            continue;
        }
        if std::fs::create_dir_all(&dossier).is_ok() && ecrire_entier(&chemin, &signature.certificat).is_ok() {
            nouveau = true;
        }
    }
    let dossier = racine(profil).join("intermediaires");
    for i in &signature.intermediaires {
        let chemin = dossier.join(format!("{}.cer", empreinte(i)));
        if !chemin.exists() && Certificate::from_der(i).is_ok() && std::fs::create_dir_all(&dossier).is_ok() {
            let _ = ecrire_entier(&chemin, i);
        }
    }
    nouveau
}

/// Les certificats intermédiaires recueillis sur les messages signés.
pub fn intermediaires_recueillis(profil: &str) -> Vec<Vec<u8>> {
    let Ok(fichiers) = std::fs::read_dir(racine(profil).join("intermediaires")) else {
        return Vec::new();
    };
    fichiers
        .flatten()
        .filter(|f| f.file_name().to_string_lossy().ends_with(".cer"))
        .filter_map(|f| std::fs::read(f.path()).ok())
        .collect()
}

/// Les certificats recueillis pour une adresse.
pub fn certificats_de(profil: &str, adresse: &str) -> Vec<Vec<u8>> {
    let dossier =
        racine(profil).join("correspondants").join(adresse.trim().to_ascii_lowercase());
    let Ok(fichiers) = std::fs::read_dir(dossier) else {
        return Vec::new();
    };
    fichiers
        .flatten()
        .filter(|f| f.file_name().to_string_lossy().ends_with(".cer"))
        .filter_map(|f| std::fs::read(f.path()).ok())
        .collect()
}

// --------------------------------------------------------------- session

/// Les clés déverrouillées de la session, par compte (identifiant de
/// l'index) : lues sur tous les fils, posées par l'interface, qui tient les
/// mots de passe du coffre.
static TROUSSEAU: std::sync::Mutex<Vec<(i64, Cle)>> = std::sync::Mutex::new(Vec::new());

/// Autorités chargées à la première vérification (le magasin du système est
/// long à lire), et rechargées après un ajout.
static AUTORITES: std::sync::RwLock<Option<std::sync::Arc<Autorites>>> = std::sync::RwLock::new(None);

pub fn deposer(compte: i64, cle: Cle) {
    let mut t = TROUSSEAU.lock().unwrap_or_else(|e| e.into_inner());
    let empreinte = cle.empreinte();
    t.retain(|(c, k)| !(*c == compte && k.empreinte() == empreinte));
    t.push((compte, cle));
}

pub fn oublier(empreinte: &str) {
    TROUSSEAU.lock().unwrap_or_else(|e| e.into_inner()).retain(|(_, k)| k.empreinte() != empreinte);
}

/// Les clés d'un compte, la plus récente en dernier.
pub fn cles(compte: i64) -> Vec<Cle> {
    TROUSSEAU
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(c, _)| *c == compte)
        .map(|(_, k)| k.clone())
        .collect()
}

pub fn deverrouillee(empreinte: &str) -> bool {
    TROUSSEAU.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|(_, k)| k.empreinte() == empreinte)
}

/// Les autorités de confiance du profil (système et ajoutées).
pub fn autorites(profil: &str) -> std::sync::Arc<Autorites> {
    if let Some(a) = AUTORITES.read().ok().and_then(|a| a.clone()) {
        return a;
    }
    let a = std::sync::Arc::new(Autorites::du_systeme(&autorites_ajoutees(profil)));
    if let Ok(mut place) = AUTORITES.write() {
        *place = Some(a.clone());
    }
    a
}

/// Ouvre un message pour l'affichage avec les clés du compte et les
/// autorités du profil ; `None` s'il n'est ni signé ni chiffré.
pub fn ouvrir_pour(profil: &str, compte: i64, brut: &[u8]) -> Option<Ouvert> {
    if !est_smime(brut) {
        return None;
    }
    let maintenant = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    ouvrir(brut, &cles(compte), &autorites(profil), maintenant)
}

/// Le message à lire : ouvert s'il est signé ou chiffré, tel quel sinon
/// (réponse, transfert, pièces jointes).
pub fn lisible(profil: &str, compte: i64, brut: Vec<u8>) -> Vec<u8> {
    match ouvrir_pour(profil, compte, &brut) {
        Some(o) => o.message,
        None => brut,
    }
}

#[cfg(test)]
mod tests;
