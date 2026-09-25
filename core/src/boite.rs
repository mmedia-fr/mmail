// SPDX-License-Identifier: GPL-3.0-or-later
//! L'objet que l'interface pilote : les comptes du profil, leur arborescence
//! commune, le dossier ouvert et ses messages, le tri entre dossiers et entre
//! boîtes.
//!
//! **Le réseau ne passe jamais par le fil de l'interface.** Chaque compte
//! connecté a son propre fil de travail, qui possède sa connexion IMAP et sa
//! propre connexion à l'index local. L'interface lui adresse des commandes et
//! reçoit les résultats par signaux, remis sur le fil Qt par
//! `CxxQtThread::queue`. Un serveur lent, un dossier volumineux ou une coupure
//! ne figent donc pas la fenêtre, et un compte injoignable ne retient pas les
//! autres.
//!
//! Trois règles tiennent l'interface à jour sans lui faire subir le passé :
//!
//! - **une commande devenue caduque avant d'être traitée est abandonnée** : si
//!   l'on clique trois dossiers pendant qu'un premier se charge, seul le dernier
//!   est lu ; un message demandé puis délaissé n'est pas téléchargé. Un
//!   déplacement ou un marquage, eux, ne sont jamais abandonnés ;
//! - **chaque session porte un numéro de génération** : les résultats d'une
//!   session fermée ou remplacée peuvent encore arriver, ils sont ignorés ;
//! - **un fil sans commande veille** : toutes les deux minutes, il relit les
//!   compteurs de ses dossiers et resynchronise le dossier ouvert.
//!
//! Les lectures de l'index (`arborescence`, `messages`) restent synchrones :
//! elles sont locales, et l'index est en WAL, qui laisse lire pendant que les
//! fils de travail écrivent.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::deplacement::{self, Rapport};
use crate::imap::{Client, Erreur};
use crate::magasin::{DossierLocal, Magasin, MessageLocal};
use crate::synchro::{self, assurer_selection, Echec};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        /// Vrai tant qu'une opération demandée par l'utilisateur est en cours.
        #[qproperty(bool, occupe)]
        /// Dernier message d'erreur, vide s'il n'y en a pas eu.
        #[qproperty(QString, erreur)]
        /// Augmente à chaque changement de l'arborescence ou des compteurs :
        /// l'interface relit alors `arborescence()`.
        #[qproperty(i32, revision)]
        /// Compte et chemin du dossier ouvert (0 et vide s'il n'y en a pas).
        #[qproperty(i32, compte_courant, cxx_name = "compteCourant")]
        #[qproperty(QString, dossier_courant, cxx_name = "dossierCourant")]
        /// Déplacements entre boîtes inscrits et pas encore soldés.
        #[qproperty(i32, en_attente, cxx_name = "enAttente")]
        type Boite = super::BoiteRust;

        /// Ouvre l'index local du profil. Local et immédiat ; à appeler avant
        /// tout le reste.
        #[qinvokable]
        #[cxx_name = "ouvrirProfil"]
        fn ouvrir_profil(self: Pin<&mut Boite>, chemin: &QString) -> bool;

        /// Comptes du profil, en JSON : `[{id, adresse, hote, etat}]`.
        #[qinvokable]
        #[cxx_name = "comptes"]
        fn comptes(&self) -> QString;

        /// Connecte un compte — nouveau ou déjà connu — en arrière-plan.
        /// Issue : `connecte`, ou `echec` avec l'étape « connexion » ou
        /// « identifiants ». Un compte nouveau dont la première connexion
        /// échoue n'est pas conservé.
        #[qinvokable]
        #[cxx_name = "connecter"]
        fn connecter(
            self: Pin<&mut Boite>,
            hote: &QString,
            adresse: &QString,
            mot_de_passe: &QString,
        ) -> bool;

        /// Ferme la session d'un compte ; son index reste consultable.
        #[qinvokable]
        #[cxx_name = "deconnecter"]
        fn deconnecter(self: Pin<&mut Boite>, compte: i32);

        /// Retire un compte du profil, avec son index local. Rien n'est touché
        /// sur le serveur.
        #[qinvokable]
        #[cxx_name = "retirerCompte"]
        fn retirer_compte(self: Pin<&mut Boite>, compte: i32);

        /// Arborescence commune, en JSON : la rubrique Favoris, puis chaque
        /// compte suivi de ses dossiers. Les dossiers masqués n'y figurent que
        /// si `masques` est vrai.
        #[qinvokable]
        #[cxx_name = "arborescence"]
        fn arborescence(&self, masques: bool) -> QString;

        /// Dossiers pouvant recevoir un message, tous comptes connectés
        /// confondus, en JSON : la liste du dialogue « Déplacer vers… ».
        #[qinvokable]
        #[cxx_name = "dossiersCibles"]
        fn dossiers_cibles(&self) -> QString;

        #[qinvokable]
        #[cxx_name = "replierCompte"]
        fn replier_compte(self: Pin<&mut Boite>, compte: i32, replie: bool);

        /// Masque un dossier dans l'arborescence, ou le réaffiche (décision 4).
        #[qinvokable]
        #[cxx_name = "masquerDossier"]
        fn masquer_dossier(self: Pin<&mut Boite>, compte: i32, chemin: &QString, masque: bool);

        /// Épingle un dossier dans la rubrique Favoris, ou l'en retire
        /// (décision 3).
        #[qinvokable]
        #[cxx_name = "epinglerDossier"]
        fn epingler_dossier(self: Pin<&mut Boite>, compte: i32, chemin: &QString, favori: bool);

        /// Place un dossier dans la rubrique Favoris, avant le favori désigné
        /// par `compte_avant` et `chemin_avant`, ou à la fin si `chemin_avant`
        /// est vide. C'est ce que fait le glisser-déposer d'un dossier.
        #[qinvokable]
        #[cxx_name = "placerFavori"]
        fn placer_favori(
            self: Pin<&mut Boite>,
            compte: i32,
            chemin: &QString,
            compte_avant: i32,
            chemin_avant: &QString,
        );

        /// Ouvre un dossier : l'index s'affiche tout de suite, la
        /// synchronisation suit. Issue : `dossierOuvert`.
        #[qinvokable]
        #[cxx_name = "ouvrirDossier"]
        fn ouvrir_dossier(self: Pin<&mut Boite>, compte: i32, chemin: &QString) -> bool;

        /// Messages du dossier ouvert, en JSON, du plus récent au plus ancien.
        #[qinvokable]
        #[cxx_name = "messages"]
        fn messages(&self) -> QString;

        /// Demande le corps affichable d'un message du dossier ouvert, et le
        /// marque comme lu. Issue : `corpsRecu`.
        #[qinvokable]
        #[cxx_name = "demanderCorps"]
        fn demander_corps(self: Pin<&mut Boite>, uid: i32) -> bool;

        /// Demande le message brut, tel que le serveur le conserve (décision 6).
        /// Issue : `corpsRecu` avec `brut` à vrai.
        #[qinvokable]
        #[cxx_name = "demanderSource"]
        fn demander_source(self: Pin<&mut Boite>, uid: i32) -> bool;

        /// Ouvre une pièce jointe d'un message du dossier ouvert avec le
        /// logiciel du système : elle est d'abord écrite dans un dossier de
        /// travail du profil. Refusé pour un type exécutable. Issue :
        /// `pieceEcrite` avec `ouvrir` à vrai.
        #[qinvokable]
        #[cxx_name = "ouvrirPiece"]
        fn ouvrir_piece(self: Pin<&mut Boite>, uid: i32, indice: i32) -> bool;

        /// Enregistre une pièce jointe à l'emplacement choisi (URL rendue par
        /// le dialogue d'enregistrement). Issue : `pieceEcrite`.
        #[qinvokable]
        #[cxx_name = "enregistrerPiece"]
        fn enregistrer_piece(self: Pin<&mut Boite>, uid: i32, indice: i32, url: &QString) -> bool;

        /// Marque des messages du dossier ouvert — UID séparés par des
        /// virgules — comme lus ou non lus.
        #[qinvokable]
        #[cxx_name = "marquerLu"]
        fn marquer_lu(self: Pin<&mut Boite>, uids: &QString, lu: bool) -> bool;

        /// Déplace des messages du dossier ouvert vers un dossier de n'importe
        /// quel compte connecté. Issue : `deplacementTermine`.
        #[qinvokable]
        #[cxx_name = "deplacer"]
        fn deplacer(
            self: Pin<&mut Boite>,
            uids: &QString,
            compte_cible: i32,
            chemin_cible: &QString,
        ) -> bool;

        /// Envoie des messages du dossier ouvert à la corbeille de leur compte.
        #[qinvokable]
        #[cxx_name = "supprimer"]
        fn supprimer(self: Pin<&mut Boite>, uids: &QString) -> bool;

        /// Relit compteurs et dossier ouvert de chaque compte ; reconnecte les
        /// comptes dont la session est tombée.
        #[qinvokable]
        #[cxx_name = "actualiser"]
        fn actualiser(self: Pin<&mut Boite>);
    }

    impl cxx_qt::Threading for Boite {}

    unsafe extern "RustQt" {
        /// La session d'un compte est ouverte et son arborescence est dans
        /// l'index.
        #[qsignal]
        #[cxx_name = "connecte"]
        fn connecte(self: Pin<&mut Boite>, compte: i32, adresse: &QString);

        /// Un dossier est synchronisé ; `veille` est vrai si c'est la veille
        /// périodique qui l'a relu, et non l'utilisateur qui l'a ouvert.
        #[qsignal]
        #[cxx_name = "dossierOuvert"]
        fn dossier_ouvert(self: Pin<&mut Boite>, compte: i32, chemin: &QString, veille: bool);

        /// Corps d'un message : affichable, ou brut si `brut` est vrai.
        /// `pieces` : ses pièces jointes, en JSON (`[{indice, nom, type,
        /// taille, risquee}]`).
        #[qsignal]
        #[cxx_name = "corpsRecu"]
        fn corps_recu(self: Pin<&mut Boite>, uid: i32, texte: &QString, brut: bool, pieces: &QString);

        /// Une pièce jointe est écrite sur le disque : `url` la désigne pour
        /// l'ouvrir, `chemin` pour l'afficher.
        #[qsignal]
        #[cxx_name = "pieceEcrite"]
        fn piece_ecrite(self: Pin<&mut Boite>, url: &QString, chemin: &QString, ouvrir: bool);

        /// Des drapeaux ont changé dans le dossier ouvert.
        #[qsignal]
        #[cxx_name = "drapeauxModifies"]
        fn drapeaux_modifies(self: Pin<&mut Boite>);

        /// Un déplacement est terminé : `nombre` messages déplacés, `erreurs`
        /// vide si tout s'est bien passé.
        #[qsignal]
        #[cxx_name = "deplacementTermine"]
        fn deplacement_termine(self: Pin<&mut Boite>, nombre: i32, erreurs: &QString);

        /// Une opération a échoué. `etape` : « connexion », « identifiants »,
        /// « dossier », « message », « tri » ou « reseau » (la session du
        /// compte est alors perdue).
        #[qsignal]
        #[cxx_name = "echec"]
        fn echec(self: Pin<&mut Boite>, compte: i32, etape: &QString, message: &QString);
    }
}

use core::pin::Pin;
use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::{QString, QUrl};

/// Délai sans commande au bout duquel un fil de travail relit ses compteurs.
const VEILLE: Duration = Duration::from_secs(120);

/// Port IMAPS : le seul que vise le client (TLS implicite).
const PORT_IMAPS: u16 = 993;

/// De quoi ouvrir une session sur un compte. Ne vit qu'en mémoire : le mot de
/// passe est au coffre du système, jamais dans l'index.
#[derive(Clone)]
struct Identifiants {
    hote: String,
    utilisateur: String,
    mot_de_passe: String,
}

/// Dossier cible d'un déplacement.
#[derive(Clone)]
struct Cible {
    compte: i64,
    chemin: String,
    /// Identifiants de la boîte cible quand elle n'est pas la boîte source.
    identifiants: Option<Identifiants>,
}

/// Ce que l'interface demande au fil de travail d'un compte.
#[derive(Clone)]
enum Commande {
    /// Relire l'arborescence et les compteurs.
    Arborescence,
    OuvrirDossier(String),
    Corps { chemin: String, uid: u32, brut: bool },
    /// Écrire une pièce jointe : dans `destination` si elle est donnée, sinon
    /// dans le dossier de travail, pour l'ouvrir.
    Piece { chemin: String, uid: u32, indice: usize, destination: Option<PathBuf> },
    MarquerLu { chemin: String, uids: Vec<u32>, lu: bool },
    Deplacer { source: String, uids: Vec<u32>, cible: Cible },
    /// Reprendre les déplacements interrompus dont ce compte est la source.
    Reprendre(HashMap<i64, Identifiants>),
    /// Veille périodique, émise par le fil lui-même.
    Veille,
}

impl Commande {
    /// Vrai si `self`, arrivée après `anterieure`, la rend caduque.
    fn remplace(&self, anterieure: &Commande) -> bool {
        use Commande::*;
        matches!(
            (self, anterieure),
            (OuvrirDossier(_), OuvrirDossier(_))
                | (OuvrirDossier(_), Corps { .. })
                | (Corps { .. }, Corps { .. })
                | (Arborescence, Arborescence)
                | (_, Veille)
        )
    }

    /// Vrai si la commande vient de l'interface et compte dans `occupe`.
    fn comptee(&self) -> bool {
        !matches!(self, Commande::Veille)
    }
}

/// Retire de la file les commandes qu'une commande postérieure rend caduques,
/// en gardant l'ordre des autres.
fn regrouper(file: Vec<Commande>) -> (Vec<Commande>, usize) {
    let garder: Vec<bool> = (0..file.len())
        .map(|i| !file[i + 1..].iter().any(|suivante| suivante.remplace(&file[i])))
        .collect();
    let mut abandonnees = 0;
    let gardees = file
        .into_iter()
        .zip(garder)
        .filter_map(|(commande, garde)| {
            if !garde && commande.comptee() {
                abandonnees += 1;
            }
            garde.then_some(commande)
        })
        .collect();
    (gardees, abandonnees)
}

/// Ce que le fil de travail rend à l'interface.
enum Issue {
    Connecte,
    Arborescence,
    Dossier { chemin: String, veille: bool },
    Corps { uid: u32, texte: String, brut: bool, marque: bool, pieces: String },
    Piece { fichier: PathBuf, ouvrir: bool },
    Marque,
    Deplace { cible: i64, chemin_cible: String, rapport: Rapport },
    /// Des déplacements interrompus ont été repris : ni demandés à l'instant,
    /// ni attendus par l'interface, qui n'a qu'à relire ses compteurs.
    Repris { rapport: Rapport },
    Echec { etape: &'static str, message: String, session_perdue: bool },
}

/// Session d'un compte, vue de l'interface.
struct Session {
    envoi: Sender<Commande>,
    /// Commandes de l'interface en cours ou en attente, connexion comprise.
    attente: Arc<AtomicUsize>,
    generation: u64,
}

/// État de connexion d'un compte, tel que l'arborescence l'affiche.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Etat {
    HorsLigne,
    Connexion,
    Pret,
    Erreur,
}

impl Etat {
    fn code(self) -> &'static str {
        match self {
            Etat::HorsLigne => "horsligne",
            Etat::Connexion => "connexion",
            Etat::Pret => "pret",
            Etat::Erreur => "erreur",
        }
    }
}

pub struct BoiteRust {
    occupe: bool,
    erreur: QString,
    revision: i32,
    compte_courant: i32,
    dossier_courant: QString,
    en_attente: i32,
    profil: String,
    magasin: Option<Magasin>,
    sessions: HashMap<i64, Session>,
    identites: HashMap<i64, Identifiants>,
    etats: HashMap<i64, Etat>,
    /// Comptes créés par une connexion pas encore aboutie.
    nouveaux: HashSet<i64>,
    generation: u64,
    /// Dossier ouvert : compte, chemin, identifiant dans l'index.
    courant: Option<(i64, String, i64)>,
}

impl Default for BoiteRust {
    fn default() -> Self {
        Self {
            occupe: false,
            erreur: QString::from(""),
            revision: 0,
            compte_courant: 0,
            dossier_courant: QString::from(""),
            en_attente: 0,
            profil: String::new(),
            magasin: None,
            sessions: HashMap::new(),
            identites: HashMap::new(),
            etats: HashMap::new(),
            nouveaux: HashSet::new(),
            generation: 0,
            courant: None,
        }
    }
}

impl qobject::Boite {
    pub fn ouvrir_profil(mut self: Pin<&mut Self>, chemin: &QString) -> bool {
        let chemin = chemin.to_string();
        if self.magasin.is_some() && self.profil == chemin {
            return true;
        }
        match Magasin::ouvrir(&chemin) {
            Ok(magasin) => {
                // Les pièces jointes ouvertes lors d'une session précédente
                // n'ont plus à traîner sur le disque.
                let _ = std::fs::remove_dir_all(dossier_pieces(&chemin));
                let attente = magasin.nombre_operations().unwrap_or(0) as i32;
                {
                    let mut noyau = self.as_mut().rust_mut();
                    noyau.magasin = Some(magasin);
                    noyau.profil = chemin;
                }
                self.as_mut().set_en_attente(attente);
                self.as_mut().set_erreur(QString::from(""));
                self.as_mut().reviser();
                true
            }
            Err(e) => {
                self.as_mut().set_erreur(QString::from(&format!("index local : {e}")));
                false
            }
        }
    }

    pub fn comptes(&self) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("[]");
        };
        let comptes = magasin.comptes().unwrap_or_default();
        let corps: Vec<String> = comptes
            .iter()
            .map(|c| {
                format!(
                    r#"{{"id":{},"adresse":{},"hote":{},"etat":{}}}"#,
                    c.id,
                    texte_json(&c.adresse),
                    texte_json(&c.hote),
                    texte_json(self.etat(c.id).code())
                )
            })
            .collect();
        QString::from(&format!("[{}]", corps.join(",")))
    }

    pub fn connecter(
        mut self: Pin<&mut Self>,
        hote: &QString,
        adresse: &QString,
        mot_de_passe: &QString,
    ) -> bool {
        let (hote, adresse) = (hote.to_string().trim().to_string(), adresse.to_string().trim().to_string());
        if hote.is_empty() || adresse.is_empty() {
            self.as_mut().set_erreur(QString::from("serveur et adresse sont obligatoires"));
            return false;
        }
        let compte = {
            let Some(magasin) = self.magasin.as_ref() else {
                self.as_mut().set_erreur(QString::from("le profil n'est pas ouvert"));
                return false;
            };
            let nouveau = matches!(magasin.compte_connu(&adresse), Ok(None));
            match magasin.compte(&adresse, &hote, PORT_IMAPS, &adresse) {
                Ok(id) => (id, nouveau),
                Err(e) => {
                    let message = QString::from(&format!("index local : {e}"));
                    self.as_mut().set_erreur(message);
                    return false;
                }
            }
        };
        let (id, nouveau) = compte;
        {
            let mut noyau = self.as_mut().rust_mut();
            if nouveau {
                noyau.nouveaux.insert(id);
            }
            noyau.identites.insert(
                id,
                Identifiants { hote, utilisateur: adresse, mot_de_passe: mot_de_passe.to_string() },
            );
        }
        self.as_mut().lancer(id, Vec::new())
    }

    pub fn deconnecter(mut self: Pin<&mut Self>, compte: i32) {
        let compte = compte as i64;
        self.as_mut().fermer_session(compte);
        self.as_mut().rust_mut().etats.insert(compte, Etat::HorsLigne);
        self.as_mut().rafraichir_occupe();
        self.as_mut().reviser();
    }

    pub fn retirer_compte(mut self: Pin<&mut Self>, compte: i32) {
        let id = compte as i64;
        self.as_mut().fermer_session(id);
        {
            let mut noyau = self.as_mut().rust_mut();
            noyau.identites.remove(&id);
            noyau.etats.remove(&id);
            noyau.nouveaux.remove(&id);
        }
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.retirer_compte(id);
        }
        if self.courant.as_ref().map(|c| c.0) == Some(id) {
            self.as_mut().poser_courant(None);
        }
        self.as_mut().rafraichir_occupe();
        self.as_mut().reviser();
    }

    pub fn arborescence(&self, masques: bool) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("[]");
        };
        let comptes = magasin.comptes().unwrap_or_default();
        let adresses: HashMap<i64, String> =
            comptes.iter().map(|c| (c.id, c.adresse.clone())).collect();
        let mut lignes: Vec<String> = Vec::new();

        // La rubrique Favoris est toujours là, même vide : c'est une cible de
        // dépôt pour les dossiers qu'on y glisse (décision 3).
        let favoris = magasin.favoris().unwrap_or_default();
        lignes.push(r#"{"genre":"rubrique","nom":"Favoris"}"#.to_string());
        if favoris.is_empty() {
            lignes.push(r#"{"genre":"favori-vide"}"#.to_string());
        }
        for d in &favoris {
            let adresse = adresses.get(&d.compte_id).cloned().unwrap_or_default();
            lignes.push(json_dossier("favori", d, &adresse, 0));
        }
        for c in &comptes {
            lignes.push(format!(
                r#"{{"genre":"compte","compte":{},"adresse":{},"hote":{},"etat":{},"replie":{}}}"#,
                c.id,
                texte_json(&c.adresse),
                texte_json(&c.hote),
                texte_json(self.etat(c.id).code()),
                c.replie
            ));
            if c.replie {
                continue;
            }
            for d in magasin.dossiers(c.id).unwrap_or_default() {
                if d.masque && !masques {
                    continue;
                }
                lignes.push(json_dossier("dossier", &d, &c.adresse, d.profondeur));
            }
        }
        QString::from(&format!("[{}]", lignes.join(",")))
    }

    pub fn dossiers_cibles(&self) -> QString {
        let Some(magasin) = self.magasin.as_ref() else {
            return QString::from("[]");
        };
        let mut lignes = Vec::new();
        for c in magasin.comptes().unwrap_or_default() {
            if self.etat(c.id) != Etat::Pret {
                continue;
            }
            for d in magasin.dossiers(c.id).unwrap_or_default() {
                if !d.selectionnable {
                    continue;
                }
                lignes.push(json_dossier("cible", &d, &c.adresse, d.profondeur));
            }
        }
        QString::from(&format!("[{}]", lignes.join(",")))
    }

    pub fn replier_compte(mut self: Pin<&mut Self>, compte: i32, replie: bool) {
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.replier_compte(compte as i64, replie);
        }
        self.as_mut().reviser();
    }

    pub fn masquer_dossier(mut self: Pin<&mut Self>, compte: i32, chemin: &QString, masque: bool) {
        if let Some(magasin) = self.magasin.as_ref() {
            if let Ok(id) = magasin.dossier_id(compte as i64, &chemin.to_string()) {
                let _ = magasin.masquer(id, masque);
            }
        }
        self.as_mut().reviser();
    }

    pub fn placer_favori(
        mut self: Pin<&mut Self>,
        compte: i32,
        chemin: &QString,
        compte_avant: i32,
        chemin_avant: &QString,
    ) {
        if let Some(magasin) = self.magasin.as_ref() {
            if let Ok(id) = magasin.dossier_id(compte as i64, &chemin.to_string()) {
                let avant = if chemin_avant.is_empty() {
                    None
                } else {
                    magasin.dossier_id(compte_avant as i64, &chemin_avant.to_string()).ok()
                };
                let _ = magasin.placer_favori(id, avant);
            }
        }
        self.as_mut().reviser();
    }

    pub fn epingler_dossier(mut self: Pin<&mut Self>, compte: i32, chemin: &QString, favori: bool) {
        if let Some(magasin) = self.magasin.as_ref() {
            if let Ok(id) = magasin.dossier_id(compte as i64, &chemin.to_string()) {
                let _ = magasin.epingler(id, favori);
            }
        }
        self.as_mut().reviser();
    }

    pub fn ouvrir_dossier(mut self: Pin<&mut Self>, compte: i32, chemin: &QString) -> bool {
        let compte = compte as i64;
        let chemin = chemin.to_string();
        let Some(id) = self.magasin.as_ref().and_then(|m| m.dossier_id(compte, &chemin).ok()) else {
            self.as_mut().set_erreur(QString::from(&format!("dossier inconnu : {chemin}")));
            return false;
        };
        self.as_mut().poser_courant(Some((compte, chemin.clone(), id)));
        self.as_mut().envoyer(compte, Commande::OuvrirDossier(chemin))
    }

    pub fn messages(&self) -> QString {
        let (Some(magasin), Some((_, _, id))) = (self.magasin.as_ref(), self.courant.as_ref()) else {
            return QString::from("[]");
        };
        match magasin.messages(*id) {
            Ok(messages) => QString::from(&json_messages(&messages)),
            Err(_) => QString::from("[]"),
        }
    }

    pub fn demander_corps(self: Pin<&mut Self>, uid: i32) -> bool {
        self.demander(uid, false)
    }

    pub fn demander_source(self: Pin<&mut Self>, uid: i32) -> bool {
        self.demander(uid, true)
    }

    pub fn ouvrir_piece(self: Pin<&mut Self>, uid: i32, indice: i32) -> bool {
        self.demander_piece(uid, indice, None)
    }

    pub fn enregistrer_piece(mut self: Pin<&mut Self>, uid: i32, indice: i32, url: &QString) -> bool {
        let chemin = QUrl::from(url).to_local_file().map(|c| c.to_string()).unwrap_or_default();
        if chemin.is_empty() {
            self.as_mut().set_erreur(QString::from("emplacement d'enregistrement invalide"));
            return false;
        }
        self.demander_piece(uid, indice, Some(PathBuf::from(chemin)))
    }

    pub fn marquer_lu(mut self: Pin<&mut Self>, uids: &QString, lu: bool) -> bool {
        let uids = lire_uids(uids);
        let Some((compte, chemin, id)) = self.courant.clone() else {
            return false;
        };
        if uids.is_empty() {
            return false;
        }
        // L'index d'abord : la liste change tout de suite, le serveur suit.
        if let Some(magasin) = self.magasin.as_ref() {
            let _ = magasin.marquer_lu(id, &uids, lu);
        }
        self.as_mut().reviser();
        self.as_mut().envoyer(compte, Commande::MarquerLu { chemin, uids, lu })
    }

    pub fn deplacer(
        mut self: Pin<&mut Self>,
        uids: &QString,
        compte_cible: i32,
        chemin_cible: &QString,
    ) -> bool {
        let uids = lire_uids(uids);
        let Some((compte, source, _)) = self.courant.clone() else {
            return false;
        };
        let cible_compte = compte_cible as i64;
        let chemin_cible = chemin_cible.to_string();
        if uids.is_empty() || (cible_compte == compte && chemin_cible == source) {
            return false;
        }
        let identifiants = if cible_compte == compte {
            None
        } else {
            match self.identites.get(&cible_compte) {
                Some(i) if self.etat(cible_compte) == Etat::Pret => Some(i.clone()),
                _ => {
                    self.as_mut().set_erreur(QString::from(
                        "la boîte cible n'est pas connectée : le déplacement est refusé",
                    ));
                    return false;
                }
            }
        };
        let cible = Cible { compte: cible_compte, chemin: chemin_cible, identifiants };
        self.as_mut().envoyer(compte, Commande::Deplacer { source, uids, cible })
    }

    pub fn supprimer(mut self: Pin<&mut Self>, uids: &QString) -> bool {
        let Some((compte, source, _)) = self.courant.clone() else {
            return false;
        };
        let corbeille = self.magasin.as_ref().and_then(|m| {
            m.dossiers(compte)
                .ok()?
                .into_iter()
                .find(|d| d.role == "Trash")
                .map(|d| d.chemin)
        });
        match corbeille {
            Some(corbeille) if corbeille != source => {
                self.deplacer(uids, compte as i32, &QString::from(&corbeille))
            }
            Some(_) => {
                self.as_mut().set_erreur(QString::from("ces messages sont déjà dans la corbeille"));
                false
            }
            None => {
                self.as_mut().set_erreur(QString::from("ce compte n'a pas de corbeille déclarée"));
                false
            }
        }
    }

    pub fn actualiser(mut self: Pin<&mut Self>) {
        let comptes: Vec<i64> = self
            .magasin
            .as_ref()
            .and_then(|m| m.comptes().ok())
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.id)
            .collect();
        let courant = self.courant.clone();
        for compte in comptes {
            let mut commandes = vec![Commande::Arborescence];
            if let Some((c, chemin, _)) = courant.as_ref() {
                if *c == compte {
                    commandes.push(Commande::OuvrirDossier(chemin.clone()));
                }
            }
            if self.sessions.contains_key(&compte) {
                for commande in commandes {
                    self.as_mut().envoyer(compte, commande);
                }
            } else if self.identites.contains_key(&compte) {
                self.as_mut().lancer(compte, commandes);
            }
        }
    }

    // ----------------------------------------------------------------- privé

    fn etat(&self, compte: i64) -> Etat {
        self.etats.get(&compte).copied().unwrap_or(Etat::HorsLigne)
    }

    fn reviser(mut self: Pin<&mut Self>) {
        let suivante = self.revision.wrapping_add(1);
        self.as_mut().set_revision(suivante);
    }

    fn poser_courant(mut self: Pin<&mut Self>, courant: Option<(i64, String, i64)>) {
        let (compte, chemin) = match &courant {
            Some((c, chemin, _)) => (*c as i32, QString::from(chemin)),
            None => (0, QString::from("")),
        };
        self.as_mut().rust_mut().courant = courant;
        self.as_mut().set_compte_courant(compte);
        self.as_mut().set_dossier_courant(chemin);
    }

    fn demander_piece(mut self: Pin<&mut Self>, uid: i32, indice: i32, destination: Option<PathBuf>) -> bool {
        if uid <= 0 || indice < 0 {
            return false;
        }
        let Some((compte, chemin, _)) = self.courant.clone() else {
            return false;
        };
        self.as_mut().envoyer(
            compte,
            Commande::Piece { chemin, uid: uid as u32, indice: indice as usize, destination },
        )
    }

    fn demander(mut self: Pin<&mut Self>, uid: i32, brut: bool) -> bool {
        if uid <= 0 {
            return false;
        }
        let Some((compte, chemin, _)) = self.courant.clone() else {
            return false;
        };
        self.as_mut().envoyer(compte, Commande::Corps { chemin, uid: uid as u32, brut })
    }

    /// Lance le fil de travail d'un compte, avec des commandes à traiter dès la
    /// connexion ouverte.
    fn lancer(mut self: Pin<&mut Self>, compte: i64, commandes: Vec<Commande>) -> bool {
        let Some(identifiants) = self.identites.get(&compte).cloned() else {
            return false;
        };
        self.as_mut().fermer_session(compte);

        let generation = self.generation + 1;
        let attente = Arc::new(AtomicUsize::new(1 + commandes.len()));
        let (envoi, reception) = mpsc::channel();
        for commande in commandes {
            let _ = envoi.send(commande);
        }
        let travail = Travail {
            fil: self.qt_thread(),
            compte,
            generation,
            attente: attente.clone(),
            atelier: atelier(&self.profil),
            profil: self.profil.clone(),
        };
        let lancement = thread::Builder::new()
            .name(format!("mmail-imap-{compte}"))
            .spawn(move || travail.executer(identifiants, reception));
        if let Err(e) = lancement {
            self.as_mut().set_erreur(QString::from(&format!("fil de travail : {e}")));
            return false;
        }
        {
            let mut noyau = self.as_mut().rust_mut();
            noyau.generation = generation;
            noyau.sessions.insert(compte, Session { envoi, attente, generation });
            noyau.etats.insert(compte, Etat::Connexion);
        }
        self.as_mut().set_erreur(QString::from(""));
        self.as_mut().rafraichir_occupe();
        self.as_mut().reviser();
        true
    }

    fn envoyer(mut self: Pin<&mut Self>, compte: i64, commande: Commande) -> bool {
        if !self.sessions.contains_key(&compte) {
            // Session tombée, identifiants connus : on la rouvre, la commande
            // attendra la connexion.
            if self.identites.contains_key(&compte) {
                return self.as_mut().lancer(compte, vec![commande]);
            }
            self.as_mut().set_erreur(QString::from("ce compte n'est pas connecté"));
            return false;
        }
        let envoye = {
            let session = &self.sessions[&compte];
            if commande.comptee() {
                session.attente.fetch_add(1, Ordering::SeqCst);
            }
            let comptee = commande.comptee();
            if session.envoi.send(commande).is_ok() {
                true
            } else {
                if comptee {
                    session.attente.fetch_sub(1, Ordering::SeqCst);
                }
                false
            }
        };
        if !envoye {
            self.as_mut().rust_mut().sessions.remove(&compte);
            self.as_mut().set_erreur(QString::from("la session du compte s'est fermée"));
        }
        self.as_mut().rafraichir_occupe();
        envoye
    }

    /// Lâche la session d'un compte sans l'attendre. Son fil s'arrête de
    /// lui-même dès qu'il constate la fermeture du canal ; ce qu'il rendrait
    /// d'ici là porte une génération périmée et sera ignoré.
    fn fermer_session(mut self: Pin<&mut Self>, compte: i64) {
        self.as_mut().rust_mut().sessions.remove(&compte);
    }

    fn rafraichir_occupe(mut self: Pin<&mut Self>) {
        let occupe = self.sessions.values().any(|s| s.attente.load(Ordering::SeqCst) > 0);
        self.as_mut().set_occupe(occupe);
    }

    fn rafraichir_attente(mut self: Pin<&mut Self>) {
        let attente = self
            .magasin
            .as_ref()
            .and_then(|m| m.nombre_operations().ok())
            .unwrap_or(0) as i32;
        self.as_mut().set_en_attente(attente);
    }

    /// Applique sur le fil Qt ce que le fil de travail d'un compte a rendu.
    fn recevoir(mut self: Pin<&mut Self>, compte: i64, issue: Issue) {
        let compte_qt = compte as i32;
        match issue {
            Issue::Connecte => {
                let adresse = self.identites.get(&compte).map(|i| i.utilisateur.clone()).unwrap_or_default();
                {
                    let mut noyau = self.as_mut().rust_mut();
                    noyau.etats.insert(compte, Etat::Pret);
                    noyau.nouveaux.remove(&compte);
                }
                // Ce compte peut être la source ou la cible de déplacements
                // interrompus : chaque session connectée reprend les siens.
                let identites = self.identites.clone();
                let connectes: Vec<i64> = self
                    .sessions
                    .keys()
                    .copied()
                    .filter(|c| *c == compte || self.etat(*c) == Etat::Pret)
                    .collect();
                for c in connectes {
                    self.as_mut().envoyer(c, Commande::Reprendre(identites.clone()));
                }
                self.as_mut().set_erreur(QString::from(""));
                self.as_mut().reviser();
                self.as_mut().connecte(compte_qt, &QString::from(&adresse));
            }
            Issue::Arborescence => {
                self.as_mut().rafraichir_attente();
                self.as_mut().reviser();
            }
            Issue::Dossier { chemin, veille } => {
                self.as_mut().reviser();
                self.as_mut().dossier_ouvert(compte_qt, &QString::from(&chemin), veille);
            }
            Issue::Corps { uid, texte, brut, marque, pieces } => {
                if marque {
                    self.as_mut().reviser();
                    self.as_mut().drapeaux_modifies();
                }
                self.as_mut().corps_recu(uid as i32, &QString::from(&texte), brut, &QString::from(&pieces));
            }
            Issue::Piece { fichier, ouvrir } => {
                let chemin = QString::from(fichier.to_string_lossy().as_ref());
                let url = QUrl::from_local_file(&chemin).to_qstring();
                self.as_mut().piece_ecrite(&url, &chemin, ouvrir);
            }
            Issue::Marque => {
                self.as_mut().reviser();
                self.as_mut().drapeaux_modifies();
            }
            Issue::Deplace { cible, chemin_cible, rapport } => {
                // La cible a changé aussi : ses compteurs, et sa liste si c'est
                // le dossier ouvert.
                if cible != compte && self.sessions.contains_key(&cible) {
                    self.as_mut().envoyer(cible, Commande::Arborescence);
                }
                let courant = self.courant.clone();
                if let Some((c, chemin, _)) = courant {
                    if c == cible && chemin == chemin_cible {
                        self.as_mut().envoyer(cible, Commande::OuvrirDossier(chemin));
                    }
                }
                self.as_mut().rafraichir_attente();
                self.as_mut().reviser();
                let erreurs = rapport.erreurs.join("\n");
                if !erreurs.is_empty() {
                    self.as_mut().set_erreur(QString::from(&erreurs));
                }
                self.as_mut()
                    .deplacement_termine(rapport.deplaces as i32, &QString::from(&erreurs));
            }
            Issue::Repris { rapport } => {
                // Les cibles ont reçu des messages : leurs compteurs aussi.
                let connectes: Vec<i64> =
                    self.sessions.keys().copied().filter(|c| *c != compte).collect();
                for c in connectes {
                    self.as_mut().envoyer(c, Commande::Arborescence);
                }
                self.as_mut().rafraichir_attente();
                self.as_mut().reviser();
                if !rapport.erreurs.is_empty() {
                    let message = format!("reprise des déplacements : {}", rapport.erreurs.join("\n"));
                    self.as_mut().set_erreur(QString::from(&message));
                }
            }
            Issue::Echec { etape, message, session_perdue } => {
                if session_perdue {
                    let nouveau = self.nouveaux.contains(&compte);
                    {
                        let mut noyau = self.as_mut().rust_mut();
                        noyau.sessions.remove(&compte);
                        noyau.etats.insert(compte, Etat::Erreur);
                    }
                    if nouveau && (etape == "connexion" || etape == "identifiants") {
                        // Un compte dont la toute première connexion échoue
                        // n'a rien à faire dans le profil.
                        {
                            let mut noyau = self.as_mut().rust_mut();
                            noyau.nouveaux.remove(&compte);
                            noyau.identites.remove(&compte);
                            noyau.etats.remove(&compte);
                        }
                        if let Some(magasin) = self.magasin.as_ref() {
                            let _ = magasin.retirer_compte(compte);
                        }
                    }
                    if etape == "identifiants" {
                        self.as_mut().rust_mut().identites.remove(&compte);
                    }
                }
                self.as_mut().rafraichir_attente();
                self.as_mut().reviser();
                let message = QString::from(&message);
                self.as_mut().set_erreur(message.clone());
                self.as_mut().echec(compte_qt, &QString::from(etape), &message);
            }
        }
        self.as_mut().rafraichir_occupe();
    }
}

/// Dossier des copies de travail des déplacements, à côté de l'index.
fn atelier(profil: &str) -> PathBuf {
    Path::new(profil)
        .parent()
        .map(|p| p.join("operations"))
        .unwrap_or_else(|| PathBuf::from("operations"))
}

/// Dossier de travail des pièces jointes ouvertes, à côté de l'index. Vidé à
/// chaque ouverture du profil.
fn dossier_pieces(profil: &str) -> PathBuf {
    Path::new(profil)
        .parent()
        .map(|p| p.join("pieces-jointes"))
        .unwrap_or_else(|| PathBuf::from("pieces-jointes"))
}

/// « 3,5,7 » → [3, 5, 7] ; ce qui n'est pas un nombre est ignoré.
fn lire_uids(texte: &QString) -> Vec<u32> {
    texte
        .to_string()
        .split(',')
        .filter_map(|u| u.trim().parse().ok())
        .filter(|&u: &u32| u > 0)
        .collect()
}

// -------------------------------------------------------------- fil de travail

struct Travail {
    fil: CxxQtThread<qobject::Boite>,
    compte: i64,
    generation: u64,
    attente: Arc<AtomicUsize>,
    atelier: PathBuf,
    profil: String,
}

/// Ce que possède le fil de travail d'un compte.
struct Etabli {
    client: Client,
    magasin: Magasin,
    /// Sessions ouvertes sur d'autres boîtes, pour y déposer des messages.
    annexes: HashMap<i64, Client>,
    /// Identifiants des autres boîtes, reçus avec les commandes.
    identites: HashMap<i64, Identifiants>,
}

impl Travail {
    /// Remet une issue à l'interface, qui l'ignorera si la session a changé.
    fn remettre(&self, issue: Issue) {
        let generation = self.generation;
        let compte = self.compte;
        // Échoue seulement si l'objet Qt n'existe plus : rien à faire alors.
        let _ = self.fil.queue(move |boite: Pin<&mut qobject::Boite>| {
            let a_jour = boite.sessions.get(&compte).map(|s| s.generation) == Some(generation);
            if a_jour {
                boite.recevoir(compte, issue);
            }
        });
    }

    fn terminer(&self, nombre: usize) {
        if nombre > 0 {
            self.attente.fetch_sub(nombre, Ordering::SeqCst);
        }
    }

    fn executer(self, identifiants: Identifiants, reception: Receiver<Commande>) {
        let mut etabli = match self.etablir(&identifiants) {
            Ok(etabli) => etabli,
            Err(issue) => {
                // La connexion et les commandes qui l'attendaient sont perdues.
                let restantes = reception.try_iter().filter(Commande::comptee).count();
                self.terminer(1 + restantes);
                self.remettre(issue);
                return;
            }
        };
        drop(identifiants);
        self.terminer(1);
        self.remettre(Issue::Connecte);

        let mut file: Vec<Commande> = Vec::new();
        loop {
            if file.is_empty() {
                match reception.recv_timeout(VEILLE) {
                    Ok(commande) => file.push(commande),
                    Err(RecvTimeoutError::Timeout) => file.push(Commande::Veille),
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            loop {
                match reception.try_recv() {
                    Ok(commande) => file.push(commande),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        etabli.client.fermer();
                        return;
                    }
                }
            }
            let (restantes, abandonnees) = regrouper(file);
            file = restantes;
            self.terminer(abandonnees);

            let commande = file.remove(0);
            let comptee = commande.comptee();
            let issue = self.traiter(commande, &mut etabli);
            let perdue = matches!(issue, Some(Issue::Echec { session_perdue: true, .. }));
            self.terminer(comptee as usize);
            if let Some(issue) = issue {
                self.remettre(issue);
            }
            if perdue {
                let restantes = reception.try_iter().filter(Commande::comptee).count()
                    + file.iter().filter(|c| c.comptee()).count();
                self.terminer(restantes);
                return;
            }
        }
        etabli.client.fermer();
    }

    /// Connexion, authentification et relevé de l'arborescence.
    fn etablir(&self, p: &Identifiants) -> Result<Etabli, Issue> {
        let perdue = |etape, message| Issue::Echec { etape, message, session_perdue: true };
        let magasin = Magasin::ouvrir(&self.profil)
            .map_err(|e| perdue("connexion", format!("index local : {e}")))?;
        let mut client = Client::connecter(&p.hote, PORT_IMAPS)
            .map_err(|e| perdue("connexion", format!("connexion à {} : {e}", p.hote)))?;
        client.ouvrir_session(&p.utilisateur, &p.mot_de_passe).map_err(|e| match e {
            Erreur::Reseau(_) => perdue("connexion", format!("connexion à {} : {e}", p.hote)),
            _ => perdue("identifiants", format!("{} : identifiants refusés ({e})", p.utilisateur)),
        })?;
        synchro::arborescence(&mut client, &magasin, self.compte)
            .map_err(|e| perdue("connexion", format!("lecture des dossiers : {e}")))?;
        Ok(Etabli { client, magasin, annexes: HashMap::new(), identites: HashMap::new() })
    }

    fn traiter(&self, commande: Commande, e: &mut Etabli) -> Option<Issue> {
        let compte = self.compte;
        match commande {
            Commande::Arborescence => Some(match synchro::arborescence(&mut e.client, &e.magasin, compte) {
                Ok(()) => Issue::Arborescence,
                Err(err) => echec("dossier", format!("lecture des dossiers : {err}"), &err),
            }),
            Commande::OuvrirDossier(chemin) => {
                Some(match synchro::synchroniser(&mut e.client, &e.magasin, compte, &chemin) {
                    Ok(_) => Issue::Dossier { chemin, veille: false },
                    Err(err) => echec("dossier", format!("ouverture de {chemin} : {err}"), &err),
                })
            }
            Commande::Corps { chemin, uid, brut } => Some(match lire_corps(e, compte, &chemin, uid, brut) {
                Ok((texte, marque, pieces)) => Issue::Corps { uid, texte, brut, marque, pieces },
                Err(err) => echec("message", format!("lecture du message {uid} : {err}"), &err),
            }),
            Commande::Piece { chemin, uid, indice, destination } => {
                let ouvrir = destination.is_none();
                let dossier = dossier_pieces(&self.profil).join(format!("{compte}-{uid}-{indice}"));
                Some(match ecrire_piece(e, &chemin, uid, indice, destination, &dossier) {
                    Ok(fichier) => Issue::Piece { fichier, ouvrir },
                    Err(err) => echec("piece", format!("pièce jointe : {err}"), &err),
                })
            }
            Commande::MarquerLu { chemin, uids, lu } => {
                Some(match synchro::marquer_lu(&mut e.client, &e.magasin, compte, &chemin, &uids, lu) {
                    Ok(()) => Issue::Marque,
                    Err(err) => echec("tri", format!("marquage : {err}"), &err),
                })
            }
            Commande::Deplacer { source, uids, cible } => {
                if let Some(identifiants) = cible.identifiants.clone() {
                    e.identites.insert(cible.compte, identifiants);
                }
                let resultat = if cible.compte == compte {
                    deplacement::dans_la_boite(&mut e.client, &e.magasin, compte, &source, &uids, &cible.chemin)
                } else {
                    self.entre_boites(e, &source, &uids, &cible)
                };
                // Les compteurs des deux dossiers ont changé.
                let _ = synchro::arborescence(&mut e.client, &e.magasin, compte);
                Some(match resultat {
                    Ok(rapport) => Issue::Deplace { cible: cible.compte, chemin_cible: cible.chemin, rapport },
                    Err(err) => self.echec_de_tri(e, cible.compte, err),
                })
            }
            Commande::Reprendre(identites) => {
                e.identites.extend(identites);
                self.reprendre(e)
            }
            Commande::Veille => {
                if let Err(err) = synchro::arborescence(&mut e.client, &e.magasin, compte) {
                    return Some(echec("reseau", format!("veille : {err}"), &err));
                }
                // Une reprise faite pendant la veille se remet à part ; la
                // veille continue.
                if let Some(issue) = self.reprendre(e) {
                    if matches!(issue, Issue::Echec { session_perdue: true, .. }) {
                        return Some(issue);
                    }
                    self.remettre(issue);
                }
                let selection = e.client.selection().map(str::to_string);
                match selection {
                    Some(chemin) => Some(match synchro::synchroniser(&mut e.client, &e.magasin, compte, &chemin) {
                        Ok(bilan) if bilan.nouveaux > 0 || bilan.retires > 0 => {
                            Issue::Dossier { chemin, veille: true }
                        }
                        Ok(_) => Issue::Arborescence,
                        Err(err) => echec("reseau", format!("veille : {err}"), &err),
                    }),
                    None => Some(Issue::Arborescence),
                }
            }
        }
    }

    fn entre_boites(
        &self,
        e: &mut Etabli,
        source: &str,
        uids: &[u32],
        cible: &Cible,
    ) -> Result<Rapport, Echec> {
        let annexe = annexe(e, cible.compte)?;
        // L'annexe est retirée le temps de l'opération : les deux sessions
        // sont empruntées en même temps.
        let mut annexe = annexe;
        let resultat = deplacement::entre_boites(
            &mut e.client,
            &mut annexe,
            &e.magasin,
            self.compte,
            source,
            uids,
            cible.compte,
            &cible.chemin,
            &self.atelier,
        );
        e.annexes.insert(cible.compte, annexe);
        resultat
    }

    /// Reprend les déplacements interrompus dont ce compte est la source.
    fn reprendre(&self, e: &mut Etabli) -> Option<Issue> {
        let operations = e.magasin.operations(self.compte).ok()?;
        if operations.is_empty() {
            return None;
        }
        let mut erreurs = Vec::new();
        let mut soldees = 0;
        for operation in operations {
            if !e.identites.contains_key(&operation.compte_cible) {
                // Cible pas encore connectée : la reprise attendra.
                continue;
            }
            let mut annexe = match annexe(e, operation.compte_cible) {
                Ok(a) => a,
                Err(err) => {
                    erreurs.push(err.to_string());
                    continue;
                }
            };
            let resultat = deplacement::reprendre(&mut e.client, &mut annexe, &e.magasin, &operation);
            e.annexes.insert(operation.compte_cible, annexe);
            match resultat {
                Ok(()) => soldees += 1,
                Err(err) => {
                    let _ = e.magasin.noter_echec_operation(operation.id, &err.to_string());
                    if err.reseau() && e.client.noop().is_err() {
                        return Some(echec("reseau", format!("reprise : {err}"), &err));
                    }
                    e.annexes.remove(&operation.compte_cible);
                    erreurs.push(err.to_string());
                }
            }
        }
        if soldees == 0 && erreurs.is_empty() {
            return None;
        }
        let _ = synchro::arborescence(&mut e.client, &e.magasin, self.compte);
        Some(Issue::Repris { rapport: Rapport { deplaces: soldees, absents: 0, erreurs } })
    }

    /// Un déplacement a échoué : si c'est la boîte cible qui a lâché, la
    /// session de la source reste valable.
    fn echec_de_tri(&self, e: &mut Etabli, cible: i64, err: Echec) -> Issue {
        if err.reseau() {
            e.annexes.remove(&cible);
            if e.client.noop().is_ok() {
                return Issue::Echec {
                    etape: "tri",
                    message: format!("déplacement interrompu, il reprendra : {err}"),
                    session_perdue: false,
                };
            }
        }
        echec("tri", format!("déplacement : {err}"), &err)
    }
}

/// Session annexe ouverte sur une autre boîte, retirée de la réserve pour
/// l'opération en cours.
fn annexe(e: &mut Etabli, compte: i64) -> Result<Client, Echec> {
    if let Some(client) = e.annexes.remove(&compte) {
        return Ok(client);
    }
    let identifiants = e
        .identites
        .get(&compte)
        .cloned()
        .ok_or_else(|| Echec::Imap(Erreur::Refuse("boîte cible non connectée".into())))?;
    let mut client = Client::connecter(&identifiants.hote, PORT_IMAPS)?;
    client.ouvrir_session(&identifiants.utilisateur, &identifiants.mot_de_passe)?;
    Ok(client)
}

/// Lit le corps d'un message du dossier ouvert ; un message affiché (et non sa
/// source) est marqué comme lu, comme le fait Outlook. Rend le texte, vrai si
/// le marquage a eu lieu, et les pièces jointes en JSON.
fn lire_corps(
    e: &mut Etabli,
    compte: i64,
    chemin: &str,
    uid: u32,
    brut: bool,
) -> Result<(String, bool, String), Echec> {
    assurer_selection(&mut e.client, chemin)?;
    let octets = e.client.corps(uid)?;
    let texte = if brut {
        String::from_utf8_lossy(&octets).into_owned()
    } else {
        crate::index::corps_affichable(&octets)
    };
    let pieces = json_pieces(&crate::index::pieces_jointes(&octets));
    let mut marque = false;
    if !brut {
        let id = e.magasin.dossier_id(compte, chemin)?;
        if matches!(e.magasin.message(id, uid)?, Some(m) if !m.lu) {
            synchro::marquer_lu(&mut e.client, &e.magasin, compte, chemin, &[uid], true)?;
            marque = true;
        }
    }
    Ok((texte, marque, pieces))
}

/// Relit un message et écrit l'une de ses pièces jointes : à l'emplacement
/// choisi, ou dans `dossier` pour l'ouvrir. Une pièce exécutable n'est jamais
/// écrite pour être ouverte — le refus est ici, et pas seulement dans
/// l'interface.
fn ecrire_piece(
    e: &mut Etabli,
    chemin: &str,
    uid: u32,
    indice: usize,
    destination: Option<PathBuf>,
    dossier: &Path,
) -> Result<PathBuf, Echec> {
    let erreur = |m: String| Echec::Index(crate::magasin::Erreur(m));
    assurer_selection(&mut e.client, chemin)?;
    let octets = e.client.corps(uid)?;
    let (nom, contenu) = crate::index::extraire_piece(&octets, indice)
        .ok_or_else(|| erreur(format!("pièce jointe {indice} introuvable")))?;
    let fichier = match destination {
        Some(destination) => destination,
        None => {
            if crate::index::ouverture_risquee(&nom) {
                return Err(erreur(format!(
                    "{nom} est un programme ou un script : enregistrez-le plutôt que de l'ouvrir"
                )));
            }
            std::fs::create_dir_all(dossier).map_err(|x| erreur(format!("{} : {x}", dossier.display())))?;
            dossier.join(&nom)
        }
    };
    std::fs::write(&fichier, &contenu).map_err(|x| erreur(format!("{} : {x}", fichier.display())))?;
    Ok(fichier)
}

fn echec(etape: &'static str, message: String, erreur: &Echec) -> Issue {
    let session_perdue = erreur.reseau();
    Issue::Echec {
        etape: if session_perdue { "reseau" } else { etape },
        message,
        session_perdue,
    }
}

// ---------------------------------------------------------------- sérialisation

/// Sérialise un dossier pour l'interface.
fn json_dossier(genre: &str, d: &DossierLocal, adresse: &str, profondeur: u32) -> String {
    format!(
        r#"{{"genre":{},"compte":{},"adresse":{},"chemin":{},"nom":{},"separateur":{},"profondeur":{},"role":{},"selectionnable":{},"messages":{},"nonLus":{},"masque":{},"favori":{}}}"#,
        texte_json(genre),
        d.compte_id,
        texte_json(adresse),
        texte_json(&d.chemin),
        texte_json(&d.nom),
        texte_json(&d.separateur),
        profondeur,
        texte_json(&d.role),
        d.selectionnable,
        d.messages,
        d.non_lus,
        d.masque,
        d.favori.is_some()
    )
}

/// Sérialise les pièces jointes d'un message pour l'interface.
fn json_pieces(pieces: &[crate::index::PieceJointe]) -> String {
    let corps: Vec<String> = pieces
        .iter()
        .map(|p| {
            format!(
                r#"{{"indice":{},"nom":{},"type":{},"taille":{},"risquee":{}}}"#,
                p.indice,
                texte_json(&p.nom),
                texte_json(&p.type_mime),
                p.taille,
                p.risquee
            )
        })
        .collect();
    format!("[{}]", corps.join(","))
}

/// Sérialise une liste de messages pour l'interface.
fn json_messages(messages: &[MessageLocal]) -> String {
    let corps: Vec<String> = messages
        .iter()
        .map(|m| {
            format!(
                r#"{{"uid":{},"expediteur":{},"adresse":{},"sujet":{},"date":{},"taille":{},"lu":{},"repondu":{}}}"#,
                m.uid,
                texte_json(&m.expediteur),
                texte_json(&m.adresse),
                texte_json(&m.sujet),
                texte_json(&m.date),
                m.taille,
                m.lu,
                m.repondu
            )
        })
        .collect();
    format!("[{}]", corps.join(","))
}

/// Chaîne JSON échappée. Écrite ici pour ne pas dépendre d'un sérialiseur
/// entier là où quelques champs suffisent — et testée pour les cas qui mordent.
fn texte_json(valeur: &str) -> String {
    let mut sortie = String::with_capacity(valeur.len() + 2);
    sortie.push('"');
    for c in valeur.chars() {
        match c {
            '"' => sortie.push_str("\\\""),
            '\\' => sortie.push_str("\\\\"),
            '\n' => sortie.push_str("\\n"),
            '\r' => sortie.push_str("\\r"),
            '\t' => sortie.push_str("\\t"),
            c if (c as u32) < 0x20 => sortie.push_str(&format!("\\u{:04x}", c as u32)),
            // Séparateurs de ligne Unicode : valides en JSON, mais pas dans un
            // littéral JavaScript ancien ; on les échappe par prudence.
            '\u{2028}' => sortie.push_str("\\u2028"),
            '\u{2029}' => sortie.push_str("\\u2029"),
            c => sortie.push(c),
        }
    }
    sortie.push('"');
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(chemin: &str) -> Commande {
        Commande::OuvrirDossier(chemin.into())
    }
    fn corps(uid: u32) -> Commande {
        Commande::Corps { chemin: "INBOX".into(), uid, brut: false }
    }
    fn noms(file: &[Commande]) -> Vec<String> {
        file.iter()
            .map(|c| match c {
                Commande::OuvrirDossier(d) => format!("ouvrir {d}"),
                Commande::Corps { uid, .. } => format!("corps {uid}"),
                Commande::Piece { uid, .. } => format!("piece {uid}"),
                Commande::MarquerLu { .. } => "marquer".into(),
                Commande::Deplacer { .. } => "deplacer".into(),
                Commande::Arborescence => "arborescence".into(),
                Commande::Reprendre(_) => "reprendre".into(),
                Commande::Veille => "veille".into(),
            })
            .collect()
    }

    #[test]
    fn seul_le_dernier_dossier_demande_est_lu() {
        let (file, abandonnees) = regrouper(vec![dossier("INBOX"), dossier("Sent"), dossier("Archives")]);
        assert_eq!(noms(&file), vec!["ouvrir Archives"]);
        assert_eq!(abandonnees, 2);
    }

    #[test]
    fn un_message_delaisse_n_est_pas_telecharge() {
        let (file, _) = regrouper(vec![corps(1), corps(2), corps(3)]);
        assert_eq!(noms(&file), vec!["corps 3"]);
        // Changer de dossier rend caduque la lecture d'un message de l'ancien.
        let (file, _) = regrouper(vec![corps(4), dossier("Sent")]);
        assert_eq!(noms(&file), vec!["ouvrir Sent"]);
    }

    #[test]
    fn un_deplacement_n_est_jamais_abandonne() {
        let deplacer = Commande::Deplacer {
            source: "INBOX".into(),
            uids: vec![1],
            cible: Cible { compte: 1, chemin: "Archives".into(), identifiants: None },
        };
        let marquer = Commande::MarquerLu { chemin: "INBOX".into(), uids: vec![2], lu: true };
        let (file, abandonnees) =
            regrouper(vec![dossier("INBOX"), deplacer, marquer, dossier("Sent"), corps(9)]);
        assert_eq!(noms(&file), vec!["deplacer", "marquer", "ouvrir Sent", "corps 9"]);
        assert_eq!(abandonnees, 1);
    }

    #[test]
    fn la_veille_cede_la_place_et_ne_compte_pas() {
        let (file, abandonnees) = regrouper(vec![Commande::Veille, dossier("INBOX")]);
        assert_eq!(noms(&file), vec!["ouvrir INBOX"]);
        assert_eq!(abandonnees, 0, "la veille n'a pas été comptée à l'envoi");
    }

    #[test]
    fn file_vide() {
        assert!(regrouper(Vec::new()).0.is_empty());
    }

    #[test]
    fn uids_lus_depuis_l_interface() {
        assert_eq!(lire_uids(&QString::from("3, 5,x,0,7")), vec![3, 5, 7]);
        assert!(lire_uids(&QString::from("")).is_empty());
    }

    #[test]
    fn atelier_a_cote_de_l_index() {
        assert_eq!(atelier("/profil/index.sqlite"), PathBuf::from("/profil/operations"));
    }

    #[test]
    fn echappement_json() {
        assert_eq!(texte_json("simple"), "\"simple\"");
        assert_eq!(texte_json(r#"gui"llemet"#), r#""gui\"llemet""#);
        assert_eq!(texte_json("deux\nlignes"), r#""deux\nlignes""#);
        assert_eq!(texte_json("tab\t"), r#""tab\t""#);
        // Un caractère de contrôle casserait l'analyse côté QML : il doit sortir
        // sous sa forme échappée, et non tel quel.
        assert_eq!(texte_json("a\u{1}b"), "\"a\\u0001b\"");
        // Les accents restent tels quels : le JSON est de l'UTF-8.
        assert_eq!(texte_json("été"), "\"été\"");
    }

    #[test]
    fn dossier_serialise() {
        let json = json_dossier(
            "dossier",
            &DossierLocal {
                id: 4,
                compte_id: 2,
                chemin: "Essais/Factures".into(),
                nom: "Factures".into(),
                separateur: "/".into(),
                profondeur: 1,
                selectionnable: true,
                messages: 5,
                non_lus: 2,
                favori: Some(1),
                ..Default::default()
            },
            "a@b.fr",
            1,
        );
        assert!(json.starts_with('{') && json.ends_with('}'));
        assert!(json.contains(r#""nom":"Factures""#));
        assert!(json.contains(r#""nonLus":2"#));
        assert!(json.contains(r#""compte":2"#));
        assert!(json.contains(r#""favori":true"#));
    }

    #[test]
    fn pieces_serialisees() {
        let json = json_pieces(&[crate::index::PieceJointe {
            indice: 1,
            nom: "outil \"x\".exe".into(),
            type_mime: "application/octet-stream".into(),
            taille: 42,
            risquee: true,
        }]);
        assert!(json.contains(r#""indice":1"#));
        assert!(json.contains(r#""risquee":true"#));
        assert!(json.contains(r#"outil \"x\".exe"#));
    }

    #[test]
    fn dossier_des_pieces_a_cote_de_l_index() {
        assert_eq!(dossier_pieces("/profil/index.sqlite"), PathBuf::from("/profil/pieces-jointes"));
    }

    #[test]
    fn messages_serialises() {
        let json = json_messages(&[MessageLocal {
            uid: 3,
            expediteur: "Service \"compta\"".into(),
            sujet: "Réunion d'équipe".into(),
            lu: true,
            ..Default::default()
        }]);
        assert!(json.contains(r#""uid":3"#));
        assert!(json.contains(r#""lu":true"#));
        assert!(json.contains("Réunion"));
        assert!(json.contains(r#"Service \"compta\""#));
    }
}
