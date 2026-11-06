// SPDX-License-Identifier: GPL-3.0-or-later
//! Contacts Microsoft 365 (0.9.0, lot 3) : lecture par Graph.
//!
//! Exchange Online n'offre pas CardDAV. Graph donne le dossier « Contacts »
//! de la boîte (`/me/contacts`) et ses sous-dossiers (`/me/contactFolders`) :
//! chacun devient un carnet de MMail, chaque contact une fiche vCard que
//! `carddav::lire_vcard` lit comme les autres — l'identifiant Graph tient lieu
//! d'adresse, `changeKey` d'ETag. Les fiches viennent alors en tête des
//! adresses proposées à la saisie, et dans le carnet d'adresses.
//!
//! Pas d'équivalent de `getctag` : à chaque synchronisation, la liste légère
//! de chaque dossier (identifiant et `changeKey`) est relue, puis seules les
//! fiches nouvelles ou modifiées sont demandées en entier.
//!
//! Lecture seule pour l'instant : `Boite::motif_carnet` tient déjà en lecture
//! seule les carnets d'un compte à jetons.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::{appeler_avec, reponse_json, Transport, GRAPH};
use crate::http::{self, Demande};
use crate::magasin::Magasin;
use crate::oauth::{encoder, ErreurOAuth};

/// Préfixe de l'adresse d'un carnet Microsoft 365 dans l'index, devant
/// l'identifiant Graph de son dossier ; `graph:contacts` : le dossier
/// « Contacts » de la boîte.
pub const PREFIXE: &str = "graph:";
const PRINCIPAL: &str = "graph:contacts";

pub fn est_graph(adresse: &str) -> bool {
    adresse.starts_with(PREFIXE)
}

/// Fiches par page, et pages au plus par dossier.
const PAGE: usize = 500;
const PAGES_MAX: usize = 100;
/// Au-delà, le dossier est relu en entier plutôt que fiche à fiche.
const UNE_A_UNE: usize = 20;
const TAILLE_PAGE: usize = 16 * 1024 * 1024;

/// Ce qui est demandé d'un contact.
const CHAMPS: &str = "id,changeKey,displayName,givenName,surname,middleName,title,companyName,department,jobTitle,\
                      emailAddresses,businessPhones,homePhones,mobilePhone,personalNotes";

/// Ce qu'a fait une synchronisation ; un dossier illisible n'arrête pas les
/// autres.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Bilan {
    pub change: bool,
    pub carnets: usize,
    pub lus: usize,
    pub retires: usize,
    pub erreurs: Vec<String>,
}

/// Synchronise les contacts Microsoft 365 du compte `compte` avec l'index.
/// `jeton` : un jeton de `PORTEE_CONTACTS`.
pub fn synchroniser(magasin: &Magasin, compte: i64, jeton: &str) -> Result<Bilan, String> {
    synchroniser_par(&mut |d: &Demande| http::envoyer(d), magasin, compte, jeton)
}

fn synchroniser_par(t: &mut Transport, magasin: &Magasin, compte: i64, jeton: &str) -> Result<Bilan, String> {
    // Le dossier « Contacts », puis ses sous-dossiers.
    let mut dossiers: Vec<(String, String)> = vec![(PRINCIPAL.to_string(), "Contacts".to_string())];
    let sous = lister(t, jeton, format!("{GRAPH}/contactFolders?$select=id,displayName&$top=100"))
        .map_err(|e| e.to_string())?;
    for d in &sous {
        let Some(id) = d.get("id").and_then(Value::as_str).filter(|v| !v.is_empty()) else { continue };
        let nom = d.get("displayName").and_then(Value::as_str).unwrap_or_default().trim().to_string();
        dossiers.push((format!("{PREFIXE}{id}"), nom));
    }
    // Lecture seule : l'écriture par Graph n'est pas encore là.
    let descriptions: Vec<(String, String, bool)> = dossiers.iter().map(|(a, n)| (a.clone(), n.clone(), false)).collect();
    let mut bilan = Bilan { carnets: dossiers.len(), ..Bilan::default() };
    bilan.change |= magasin.poser_carnets(compte, &descriptions).map_err(|e| e.to_string())?;
    let locaux = magasin.carnets(compte).map_err(|e| e.to_string())?;
    for (adresse, nom) in &dossiers {
        let Some((id, _, _)) = locaux.iter().find(|(_, url, _)| url == adresse) else { continue };
        match synchroniser_dossier(t, magasin, *id, adresse, jeton) {
            Ok((lus, retires)) => {
                bilan.lus += lus;
                bilan.retires += retires;
                bilan.change |= lus + retires > 0;
            }
            Err(e) => bilan.erreurs.push(format!("contacts « {nom} » : {e}")),
        }
    }
    Ok(bilan)
}

/// Les contacts d'un dossier, champs `champs`.
fn liste(adresse: &str, champs: &str) -> String {
    if adresse == PRINCIPAL {
        format!("{GRAPH}/contacts?$select={champs}&$top={PAGE}")
    } else {
        let id = adresse.strip_prefix(PREFIXE).unwrap_or(adresse);
        format!("{GRAPH}/contactFolders/{}/contacts?$select={champs}&$top={PAGE}", encoder(id))
    }
}

/// Toutes les pages d'une liste de Graph ; une liste qui ne finit pas est une
/// erreur, pour ne rien retirer à tort.
fn lister(t: &mut Transport, jeton: &str, depart: String) -> Result<Vec<Value>, ErreurOAuth> {
    let mut sortie = Vec::new();
    let mut url = Some(depart);
    for _ in 0..PAGES_MAX {
        let Some(u) = url.take() else { break };
        let page = reponse_json(&appeler_avec(t, jeton, "GET", &u, "", b"", TAILLE_PAGE, &[])?)?;
        if let Some(l) = page.get("value").and_then(Value::as_array) {
            sortie.extend(l.iter().cloned());
        }
        url = page.get("@odata.nextLink").and_then(Value::as_str).map(str::to_string);
    }
    if url.is_some() {
        return Err(ErreurOAuth::refus("TropDeContacts", "trop de contacts dans ce dossier"));
    }
    Ok(sortie)
}

fn synchroniser_dossier(t: &mut Transport, magasin: &Magasin, carnet: i64, adresse: &str, jeton: &str) -> Result<(usize, usize), String> {
    let legere = lister(t, jeton, liste(adresse, "id,changeKey")).map_err(|e| e.to_string())?;
    let distants: HashMap<String, String> = legere
        .iter()
        .filter_map(|c| {
            let id = c.get("id").and_then(Value::as_str).filter(|v| !v.is_empty())?;
            Some((id.to_string(), c.get("changeKey").and_then(Value::as_str).unwrap_or_default().to_string()))
        })
        .collect();
    let locales = magasin.etags_contacts(carnet).map_err(|e| e.to_string())?;
    let a_retirer: Vec<String> = locales.keys().filter(|h| !distants.contains_key(*h)).cloned().collect();
    let a_lire: HashSet<&str> =
        distants.iter().filter(|(id, cle)| locales.get(*id) != Some(*cle)).map(|(id, _)| id.as_str()).collect();
    magasin.retirer_contacts(carnet, &a_retirer).map_err(|e| e.to_string())?;
    if a_lire.is_empty() {
        return Ok((0, a_retirer.len()));
    }
    let completes: Vec<Value> = if a_lire.len() <= UNE_A_UNE {
        let mut l = Vec::new();
        for id in &a_lire {
            let url = format!("{GRAPH}/contacts/{}?$select={CHAMPS}", encoder(id));
            let r = appeler_avec(t, jeton, "GET", &url, "", b"", TAILLE_PAGE, &[]).map_err(|e| e.to_string())?;
            if r.statut == 404 {
                continue;
            }
            l.push(reponse_json(&r).map_err(|e| e.to_string())?);
        }
        l
    } else {
        lister(t, jeton, liste(adresse, CHAMPS))
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|c| c.get("id").and_then(Value::as_str).is_some_and(|i| a_lire.contains(i)))
            .collect()
    };
    let fiches: Vec<(String, String, crate::carddav::Contact, String)> = completes
        .iter()
        .filter_map(|c| {
            let href = c.get("id").and_then(Value::as_str).filter(|v| !v.is_empty())?.to_string();
            let etag = c.get("changeKey").and_then(Value::as_str).unwrap_or_default().to_string();
            let vcard = vcard_de(c);
            Some((href, etag, crate::carddav::lire_vcard(&vcard), vcard))
        })
        .collect();
    magasin.poser_contacts(carnet, &fiches).map_err(|e| e.to_string())?;
    Ok((fiches.len(), a_retirer.len()))
}

/// Un contact de Graph en fiche vCard 3.0.
pub fn vcard_de(c: &Value) -> String {
    let texte = |cle: &str| c.get(cle).and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let echapper = crate::agenda::echapper;
    let mut lignes = vec!["BEGIN:VCARD".to_string(), "VERSION:3.0".to_string()];
    let id = texte("id");
    if !id.is_empty() {
        lignes.push(format!("UID:{}", echapper(&id)));
    }
    let (prenom, nom, autre, civilite) = (texte("givenName"), texte("surname"), texte("middleName"), texte("title"));
    let mut affiche = texte("displayName");
    if affiche.is_empty() {
        affiche = [prenom.as_str(), nom.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" ");
    }
    if !affiche.is_empty() {
        lignes.push(format!("FN:{}", echapper(&affiche)));
    }
    if !(prenom.is_empty() && nom.is_empty() && autre.is_empty()) {
        lignes.push(format!("N:{};{};{};{};", echapper(&nom), echapper(&prenom), echapper(&autre), echapper(&civilite)));
    }
    let (societe, service) = (texte("companyName"), texte("department"));
    if !societe.is_empty() || !service.is_empty() {
        lignes.push(if service.is_empty() {
            format!("ORG:{}", echapper(&societe))
        } else {
            format!("ORG:{};{}", echapper(&societe), echapper(&service))
        });
    }
    let fonction = texte("jobTitle");
    if !fonction.is_empty() {
        lignes.push(format!("TITLE:{}", echapper(&fonction)));
    }
    for e in c.get("emailAddresses").and_then(Value::as_array).into_iter().flatten() {
        let adresse = e.get("address").and_then(Value::as_str).unwrap_or_default().trim();
        if adresse.contains('@') {
            lignes.push(format!("EMAIL;TYPE=INTERNET:{}", echapper(adresse)));
        }
    }
    let mut telephones = |cle: &str, genre: &str| {
        let valeurs: Vec<String> = match c.get(cle) {
            Some(Value::Array(l)) => l.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            Some(Value::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        };
        for v in valeurs.iter().map(|v| v.trim()).filter(|v| !v.is_empty()) {
            lignes.push(format!("TEL;TYPE={genre}:{}", echapper(v)));
        }
    };
    telephones("businessPhones", "WORK");
    telephones("mobilePhone", "CELL");
    telephones("homePhones", "HOME");
    let note = texte("personalNotes");
    if !note.is_empty() {
        lignes.push(format!("NOTE:{}", echapper(&note)));
    }
    lignes.push("END:VCARD".to_string());
    lignes.join("\r\n") + "\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn contact(id: &str, cle: &str, nom: &str) -> Value {
        serde_json::json!({
            "id": id, "changeKey": cle, "displayName": nom, "givenName": nom, "surname": "Essai",
            "emailAddresses": [{"name": nom, "address": format!("{}@exemple.fr", nom.to_lowercase())}]
        })
    }

    #[test]
    fn contact_en_vcard() {
        let c = serde_json::json!({
            "id": "AAMk=1", "displayName": "Alice Martin", "givenName": "Alice", "surname": "Martin", "title": "Mme",
            "companyName": "Exemple, SARL", "department": "Achats", "jobTitle": "Acheteuse",
            "emailAddresses": [{"name": "Alice", "address": "alice@exemple.fr"}, {"address": "sans-arobase"}],
            "businessPhones": ["05 53 00 00 00"], "mobilePhone": "06 00 00 00 00", "homePhones": [],
            "personalNotes": "Ligne 1\nLigne 2"
        });
        let v = vcard_de(&c);
        assert!(v.contains("N:Martin;Alice;;Mme;\r\n") && v.contains("ORG:Exemple\\, SARL;Achats\r\n"), "{v}");
        let lu = crate::carddav::lire_vcard(&v);
        assert_eq!(lu.nom, "Alice Martin");
        assert_eq!(lu.organisation, "Exemple, SARL, Achats");
        assert_eq!(lu.courriels, vec!["alice@exemple.fr".to_string()]);
        assert_eq!(lu.telephones, vec!["05 53 00 00 00".to_string(), "06 00 00 00 00".to_string()]);
        // Sans nom affiché : prénom et nom ; sans rien, l'organisation.
        let v = vcard_de(&serde_json::json!({"id": "2", "givenName": "Bob", "surname": "Durand"}));
        assert_eq!(crate::carddav::lire_vcard(&v).nom, "Bob Durand");
        let v = vcard_de(&serde_json::json!({"id": "3", "companyName": "Société seule"}));
        assert_eq!(crate::carddav::lire_vcard(&v).nom, "Société seule");
    }

    struct Faux {
        dossiers: Vec<Value>,
        /// Par adresse de liste (sans les paramètres) : les contacts complets.
        contacts: HashMap<String, Vec<Value>>,
        journal: RefCell<Vec<String>>,
    }

    impl Faux {
        fn repondre(&self, d: &Demande) -> crate::imap::Resultat<http::Reponse> {
            self.journal.borrow_mut().push(d.url.to_string());
            let json = |statut: u16, v: Value| http::Reponse {
                statut,
                type_contenu: "application/json".into(),
                redirection: None,
                corps: v.to_string().into_bytes(),
                adresse: String::new(),
                entetes: Vec::new(),
            };
            let (chemin, requete) = d.url.split_once('?').unwrap_or((d.url, ""));
            let chemin = chemin.trim_start_matches("https://graph.microsoft.com/v1.0/me");
            if chemin == "/contactFolders" {
                return Ok(json(200, serde_json::json!({ "value": self.dossiers })));
            }
            if let Some(liste) = self.contacts.get(chemin) {
                let legere = requete.contains("$select=id,changeKey&");
                let v: Vec<Value> = if legere {
                    liste.iter().map(|c| serde_json::json!({"id": c["id"], "changeKey": c["changeKey"]})).collect()
                } else {
                    liste.clone()
                };
                return Ok(json(200, serde_json::json!({ "value": v })));
            }
            if let Some(id) = chemin.strip_prefix("/contacts/") {
                let trouve = self.contacts.values().flatten().find(|c| encoder(c["id"].as_str().unwrap()) == id).cloned();
                return Ok(match trouve {
                    Some(c) => json(200, c),
                    None => json(404, serde_json::json!({"error": {"code": "ErrorItemNotFound", "message": ""}})),
                });
            }
            Ok(json(404, serde_json::json!({"error": {"code": "Inconnu", "message": d.url}})))
        }
    }

    fn noms(magasin: &Magasin) -> Vec<String> {
        let mut n: Vec<String> = magasin.fiches_contacts("", 100).unwrap().into_iter().map(|f| f.nom).collect();
        n.sort();
        n
    }

    #[test]
    fn synchronisations_successives() {
        let magasin = Magasin::en_memoire().unwrap();
        let compte = magasin.compte("moi@exemple.fr", "outlook.office365.com", 993, "moi@exemple.fr").unwrap();
        let mut faux = Faux {
            dossiers: vec![serde_json::json!({"id": "DOS/1=", "displayName": "Fournisseurs"})],
            contacts: HashMap::from([
                ("/contacts".to_string(), vec![contact("c1", "a", "Alice"), contact("c2", "a", "Bob")]),
                ("/contactFolders/DOS%2F1%3D/contacts".to_string(), vec![contact("f1", "a", "Chloe")]),
            ]),
            journal: RefCell::new(Vec::new()),
        };
        let bilan = synchroniser_par(&mut |d: &Demande| faux.repondre(d), &magasin, compte, "J").unwrap();
        assert_eq!((bilan.carnets, bilan.lus, bilan.retires, bilan.change), (2, 3, 0, true));
        assert!(bilan.erreurs.is_empty(), "{:?}", bilan.erreurs);
        assert_eq!(noms(&magasin), vec!["Alice", "Bob", "Chloe"]);
        let carnets: Vec<(String, String, bool)> =
            magasin.carnets_locaux().unwrap().into_iter().map(|c| (c.adresse, c.nom, c.ecriture)).collect();
        assert_eq!(
            carnets,
            vec![("graph:contacts".into(), "Contacts".into(), false), ("graph:DOS/1=".into(), "Fournisseurs".into(), false)]
        );
        // Rien de changé : les listes légères seules.
        faux.journal.borrow_mut().clear();
        let bilan = synchroniser_par(&mut |d: &Demande| faux.repondre(d), &magasin, compte, "J").unwrap();
        assert_eq!((bilan.lus, bilan.retires, bilan.change), (0, 0, false));
        assert_eq!(faux.journal.borrow().len(), 3);
        // Une fiche modifiée relue seule, une disparue retirée, le dossier
        // « Fournisseurs » supprimé dans Outlook.
        faux.contacts.insert("/contacts".into(), vec![contact("c1", "b", "Alicia")]);
        faux.dossiers.clear();
        faux.journal.borrow_mut().clear();
        let bilan = synchroniser_par(&mut |d: &Demande| faux.repondre(d), &magasin, compte, "J").unwrap();
        assert_eq!((bilan.carnets, bilan.lus, bilan.retires, bilan.change), (1, 1, 1, true));
        assert_eq!(noms(&magasin), vec!["Alicia"]);
        assert!(faux.journal.borrow().iter().any(|u| u.contains("/contacts/c1?$select=id,changeKey,displayName")));
    }
}
