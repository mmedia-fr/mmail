// SPDX-License-Identifier: GPL-3.0-or-later
//! Index local : une base SQLite par profil (décision 14 du dossier de projet).
//!
//! Ce que la base porte : les comptes, l'arborescence des dossiers avec leur
//! état de synchronisation et leur état purement local (masquage, favoris),
//! l'index des messages (en-têtes décodés), et la file des déplacements entre
//! boîtes en cours (`pending_ops`, décision 16). Les corps ne sont pas
//! conservés — ils sont lus au besoin sur le serveur ; la fenêtre glissante de
//! la décision 17 viendra plus tard.
//!
//! Le schéma reprend les noms décidés au dossier pour que la suite s'y pose
//! sans renommer : `accounts`, `folders`, `messages`, `pending_ops`.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::protocole::{Dossier, EtatDossier, Statut};

pub type Resultat<T> = Result<T, Erreur>;

/// Lignes écrites par transaction : le verrou d'écriture de SQLite est unique,
/// et le fil de l'interface, qui écrit aussi (marquages), attend qu'il se
/// libère. Une transaction sur tout un dossier le tenait plusieurs secondes.
const LOT_ECRITURE: usize = 500;

/// Version du schéma, portée par `PRAGMA user_version`.
const VERSION_SCHEMA: i32 = 6;

#[derive(Debug)]
pub struct Erreur(pub String);

impl std::fmt::Display for Erreur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<rusqlite::Error> for Erreur {
    fn from(e: rusqlite::Error) -> Self {
        Erreur(e.to_string())
    }
}

/// Un compte tel qu'il est enregistré dans le profil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompteLocal {
    pub id: i64,
    pub adresse: String,
    pub hote: String,
    pub port: u16,
    pub utilisateur: String,
    /// Replié dans l'arborescence : ses dossiers ne s'affichent pas.
    pub replie: bool,
}

/// Un dossier tel qu'il est rangé dans l'index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DossierLocal {
    pub id: i64,
    pub compte_id: i64,
    pub chemin: String,
    pub nom: String,
    pub separateur: String,
    pub profondeur: u32,
    pub role: String,
    pub selectionnable: bool,
    /// Compteurs : ceux du serveur (`STATUS`), puis ceux de l'index une fois
    /// le dossier synchronisé.
    pub messages: u32,
    pub non_lus: u32,
    pub uid_validity: u32,
    pub uid_next: u32,
    pub highest_mod_seq: u64,
    /// Masqué localement (décision 4) : il existe toujours sur le serveur.
    pub masque: bool,
    /// Rang dans la rubrique Favoris (décision 3), s'il y figure.
    pub favori: Option<i64>,
    /// Sous-dossiers repliés dans l'arborescence : réglage local, comme le
    /// repli d'un compte.
    pub replie: bool,
}

/// Un message tel qu'il est rangé dans l'index : de quoi remplir une liste.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct MessageLocal {
    pub uid: u32,
    pub message_id: String,
    pub expediteur: String,
    pub adresse: String,
    pub sujet: String,
    pub date: String,
    /// Date interne du serveur en secondes Unix : la clef de tri.
    pub horodatage: i64,
    pub taille: u32,
    pub lu: bool,
    pub repondu: bool,
    /// Pièces jointes : supposées d'après les en-têtes, puis constatées à la
    /// lecture du message.
    pub pieces: bool,
    /// Drapeau de suivi (`\Flagged`).
    pub suivi: bool,
    /// Importance annoncée : 1 haute, 0 normale, -1 basse.
    pub importance: i8,
}

/// Étape d'un déplacement entre boîtes (décision 16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Etape {
    /// Le message est lu et conservé en `.eml` ; il reste à le déposer dans la
    /// cible.
    Deposer,
    /// Le message est dans la cible ; il reste à le retirer de la source.
    Purger,
}

impl Etape {
    fn code(self) -> &'static str {
        match self {
            Etape::Deposer => "deposer",
            Etape::Purger => "purger",
        }
    }

    fn lire(code: &str) -> Etape {
        match code {
            "purger" => Etape::Purger,
            _ => Etape::Deposer,
        }
    }
}

/// Un déplacement entre boîtes en cours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub id: i64,
    pub compte_source: i64,
    pub chemin_source: String,
    pub validite_source: u32,
    pub uid_source: u32,
    pub compte_cible: i64,
    pub chemin_cible: String,
    pub message_id: String,
    /// Drapeaux et date interne d'origine, à reposer dans la cible.
    pub drapeaux: String,
    pub date_interne: String,
    /// Copie du message, le temps du déplacement : c'est elle qui garantit
    /// qu'aucune coupure ne le perd.
    pub fichier: String,
    pub etape: Etape,
    pub erreur: String,
}

pub struct Magasin {
    base: Connection,
}

impl Magasin {
    /// Ouvre — ou crée — l'index d'un profil.
    pub fn ouvrir(chemin: &str) -> Resultat<Magasin> {
        let base = Connection::open(chemin)?;
        Magasin::preparer(base)
    }

    /// Index en mémoire : sert aux tests, et à rien d'autre.
    pub fn en_memoire() -> Resultat<Magasin> {
        Magasin::preparer(Connection::open_in_memory()?)
    }

    fn preparer(base: Connection) -> Resultat<Magasin> {
        // WAL : plusieurs lectures pendant une écriture. Le profil est
        // obligatoirement sur un disque local (décision 18) — SQLite en WAL se
        // comporte mal sur un partage réseau.
        base.pragma_update(None, "journal_mode", "WAL")?;
        // L'interface lit pendant que les fils de travail écrivent, chacun par
        // sa propre connexion : une lecture qui tombe sur un verrou attend
        // plutôt que d'échouer.
        base.busy_timeout(std::time::Duration::from_secs(5))?;
        base.pragma_update(None, "foreign_keys", "ON")?;
        base.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                id         INTEGER PRIMARY KEY,
                adresse    TEXT NOT NULL UNIQUE,
                hote       TEXT NOT NULL,
                port       INTEGER NOT NULL DEFAULT 993,
                utilisateur TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS folders (
                id              INTEGER PRIMARY KEY,
                account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                chemin          TEXT NOT NULL,
                nom             TEXT NOT NULL,
                profondeur      INTEGER NOT NULL DEFAULT 0,
                role            TEXT NOT NULL DEFAULT '',
                uidvalidity     INTEGER NOT NULL DEFAULT 0,
                uidnext         INTEGER NOT NULL DEFAULT 0,
                highestmodseq   INTEGER NOT NULL DEFAULT 0,
                hidden_local    INTEGER NOT NULL DEFAULT 0,
                favorite_rank   INTEGER,
                UNIQUE (account_id, chemin)
            );
            CREATE TABLE IF NOT EXISTS messages (
                id          INTEGER PRIMARY KEY,
                folder_id   INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                uid         INTEGER NOT NULL,
                message_id  TEXT NOT NULL DEFAULT '',
                expediteur  TEXT NOT NULL DEFAULT '',
                adresse     TEXT NOT NULL DEFAULT '',
                sujet       TEXT NOT NULL DEFAULT '',
                date        TEXT NOT NULL DEFAULT '',
                taille      INTEGER NOT NULL DEFAULT 0,
                lu          INTEGER NOT NULL DEFAULT 0,
                repondu     INTEGER NOT NULL DEFAULT 0,
                body_state  TEXT NOT NULL DEFAULT 'headers',
                UNIQUE (folder_id, uid)
            );
            -- Adresses auxquelles on a écrit : elles complètent les expéditeurs
            -- de l'index dans les propositions de saisie.
            CREATE TABLE IF NOT EXISTS correspondants (
                adresse     TEXT PRIMARY KEY COLLATE NOCASE,
                nom         TEXT NOT NULL DEFAULT '',
                envois      INTEGER NOT NULL DEFAULT 0,
                dernier     INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS pending_ops (
                id              INTEGER PRIMARY KEY,
                nature          TEXT NOT NULL DEFAULT 'deplacer',
                compte_source   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                chemin_source   TEXT NOT NULL,
                validite_source INTEGER NOT NULL,
                uid_source      INTEGER NOT NULL,
                compte_cible    INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                chemin_cible    TEXT NOT NULL,
                message_id      TEXT NOT NULL DEFAULT '',
                drapeaux        TEXT NOT NULL DEFAULT '',
                date_interne    TEXT NOT NULL DEFAULT '',
                fichier         TEXT NOT NULL DEFAULT '',
                etape           TEXT NOT NULL DEFAULT 'deposer',
                erreur          TEXT NOT NULL DEFAULT '',
                cree            INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );
            "#,
        )?;
        let magasin = Magasin { base };
        magasin.migrer()?;
        Ok(magasin)
    }

    /// Porte un index d'une version antérieure du schéma à la version courante.
    /// Les colonnes s'ajoutent ; rien ne se perd.
    fn migrer(&self) -> Resultat<()> {
        let version: i32 = self.base.pragma_query_value(None, "user_version", |l| l.get(0))?;
        if version >= VERSION_SCHEMA {
            return Ok(());
        }
        self.ajouter_colonne("accounts", "replie", "INTEGER NOT NULL DEFAULT 0")?;
        self.ajouter_colonne("folders", "separateur", "TEXT NOT NULL DEFAULT '/'")?;
        self.ajouter_colonne("folders", "selectionnable", "INTEGER NOT NULL DEFAULT 1")?;
        self.ajouter_colonne("folders", "nb_messages", "INTEGER NOT NULL DEFAULT 0")?;
        self.ajouter_colonne("folders", "nb_non_lus", "INTEGER NOT NULL DEFAULT 0")?;
        self.ajouter_colonne("messages", "horodatage", "INTEGER NOT NULL DEFAULT 0")?;
        self.base.execute_batch(
            "DROP INDEX IF EXISTS messages_par_date;
             CREATE INDEX IF NOT EXISTS messages_par_horodatage
                 ON messages(folder_id, horodatage DESC, uid DESC);",
        )?;
        // Version 3 : le marqueur de pièces jointes. Les en-têtes déjà indexés
        // ont été lus sans Content-Type : UIDVALIDITY remise à zéro, chaque
        // dossier est relu en entier à sa prochaine ouverture — en-têtes
        // seulement, l'état local des dossiers (favoris, masquage) restant.
        if version < 3 {
            self.ajouter_colonne("messages", "pieces", "INTEGER NOT NULL DEFAULT 0")?;
            self.ajouter_colonne("messages", "pieces_certain", "INTEGER NOT NULL DEFAULT 0")?;
            self.base.execute("UPDATE folders SET uidvalidity = 0", [])?;
        }
        // Version 4 : suivi et importance ; même relecture, pour l'importance,
        // qui vient d'en-têtes jusque-là non demandés.
        if version < 4 {
            self.ajouter_colonne("messages", "suivi", "INTEGER NOT NULL DEFAULT 0")?;
            self.ajouter_colonne("messages", "importance", "INTEGER NOT NULL DEFAULT 0")?;
            self.base.execute("UPDATE folders SET uidvalidity = 0", [])?;
        }
        // Version 5 : l'ordre des comptes dans l'arborescence, choisi par
        // l'utilisateur ; il part de l'ordre d'ajout.
        if version < 5 {
            self.ajouter_colonne("accounts", "rang", "INTEGER")?;
            self.base.execute("UPDATE accounts SET rang = id WHERE rang IS NULL", [])?;
            self.ajouter_colonne("folders", "replie_local", "INTEGER NOT NULL DEFAULT 0")?;
        }
        // Version 6 : les expéditeurs rencontrés, de quoi proposer une adresse
        // à la saisie sans parcourir tous les messages à chaque frappe ; et un
        // index des messages gardés sur le poste, que le ménage du cache
        // parcourait en entier.
        if version < 6 {
            self.base.execute_batch(
                "CREATE TABLE IF NOT EXISTS expediteurs (
                     adresse TEXT PRIMARY KEY,
                     nom     TEXT NOT NULL DEFAULT '',
                     nb      INTEGER NOT NULL DEFAULT 0,
                     dernier INTEGER NOT NULL DEFAULT 0
                 );
                 INSERT OR IGNORE INTO expediteurs (adresse, nom, nb, dernier)
                     SELECT lower(adresse), MAX(expediteur), COUNT(*), MAX(horodatage) FROM messages
                     WHERE adresse <> '' GROUP BY lower(adresse);
                 CREATE INDEX IF NOT EXISTS messages_gardes ON messages(horodatage) WHERE body_state = 'full';",
            )?;
        }
        self.base.pragma_update(None, "user_version", VERSION_SCHEMA)?;
        Ok(())
    }

    fn ajouter_colonne(&self, table: &str, colonne: &str, definition: &str) -> Resultat<()> {
        let existe: bool = self
            .base
            .prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"))?
            .exists([colonne])?;
        if !existe {
            self.base
                .execute(&format!("ALTER TABLE {table} ADD COLUMN {colonne} {definition}"), [])?;
        }
        Ok(())
    }

    // ------------------------------------------------------------- comptes

    /// Identifiant d'un compte déjà enregistré, sans rien créer.
    pub fn compte_connu(&self, adresse: &str) -> Resultat<Option<i64>> {
        Ok(self
            .base
            .query_row("SELECT id FROM accounts WHERE adresse = ?1", [adresse], |l| l.get(0))
            .optional()?)
    }

    /// Enregistre un compte, ou le retrouve s'il est déjà connu.
    pub fn compte(&self, adresse: &str, hote: &str, port: u16, utilisateur: &str) -> Resultat<i64> {
        self.base.execute(
            "INSERT INTO accounts (adresse, hote, port, utilisateur) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(adresse) DO UPDATE SET hote = ?2, port = ?3, utilisateur = ?4",
            params![adresse, hote, port, utilisateur],
        )?;
        // Un compte nouveau prend place à la fin.
        self.base.execute(
            "UPDATE accounts SET rang = (SELECT COALESCE(MAX(rang), 0) + 1 FROM accounts)
             WHERE adresse = ?1 AND rang IS NULL",
            params![adresse],
        )?;
        Ok(self.base.query_row(
            "SELECT id FROM accounts WHERE adresse = ?1",
            params![adresse],
            |l| l.get(0),
        )?)
    }

    /// Comptes du profil, dans l'ordre choisi par l'utilisateur — à défaut,
    /// celui où ils ont été ajoutés.
    pub fn comptes(&self) -> Resultat<Vec<CompteLocal>> {
        let mut requete = self.base.prepare(
            "SELECT id, adresse, hote, port, utilisateur, replie FROM accounts
             ORDER BY rang IS NULL, rang, id",
        )?;
        let comptes = requete
            .query_map([], |l| {
                Ok(CompteLocal {
                    id: l.get(0)?,
                    adresse: l.get(1)?,
                    hote: l.get(2)?,
                    port: l.get(3)?,
                    utilisateur: l.get(4)?,
                    replie: l.get::<_, i32>(5)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(comptes)
    }

    pub fn compte_par_id(&self, id: i64) -> Resultat<Option<CompteLocal>> {
        Ok(self.comptes()?.into_iter().find(|c| c.id == id))
    }

    /// Retire un compte du profil, avec son index. Rien n'est touché sur le
    /// serveur.
    pub fn retirer_compte(&self, id: i64) -> Resultat<()> {
        self.base.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Place un compte juste avant un autre, ou à la fin si `avant` est absent.
    pub fn placer_compte(&self, id: i64, avant: Option<i64>) -> Resultat<()> {
        let mut ordre: Vec<i64> = self.comptes()?.into_iter().map(|c| c.id).filter(|&c| c != id).collect();
        let place = avant.and_then(|a| ordre.iter().position(|&c| c == a)).unwrap_or(ordre.len());
        ordre.insert(place, id);
        self.ranger_comptes(&ordre)
    }

    /// Monte (`sens` négatif) ou descend un compte d'une place.
    pub fn decaler_compte(&self, id: i64, sens: i32) -> Resultat<()> {
        let mut ordre: Vec<i64> = self.comptes()?.into_iter().map(|c| c.id).collect();
        let Some(i) = ordre.iter().position(|&c| c == id) else {
            return Ok(());
        };
        let j = if sens < 0 { i.checked_sub(1) } else { Some(i + 1).filter(|&j| j < ordre.len()) };
        if let Some(j) = j {
            ordre.swap(i, j);
            self.ranger_comptes(&ordre)?;
        }
        Ok(())
    }

    fn ranger_comptes(&self, ordre: &[i64]) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for (rang, id) in ordre.iter().enumerate() {
            transaction.execute("UPDATE accounts SET rang = ?2 WHERE id = ?1", params![id, rang as i64 + 1])?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn replier_compte(&self, id: i64, replie: bool) -> Resultat<()> {
        self.base
            .execute("UPDATE accounts SET replie = ?2 WHERE id = ?1", params![id, replie as i32])?;
        Ok(())
    }

    // ------------------------------------------------------------ dossiers

    /// Remplace l'arborescence d'un compte par celle que le serveur annonce.
    ///
    /// Les dossiers disparus côté serveur s'en vont avec leurs messages ; ceux
    /// qui restent gardent leur état local (masquage, favoris) — c'est tout
    /// l'intérêt de mettre à jour plutôt que de tout réécrire.
    pub fn poser_dossiers(&self, compte: i64, dossiers: &[Dossier]) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        let chemins: Vec<&str> = dossiers.iter().map(|d| d.chemin.as_str()).collect();
        for dossier in dossiers {
            transaction.execute(
                "INSERT INTO folders (account_id, chemin, nom, profondeur, role, separateur, selectionnable)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(account_id, chemin)
                 DO UPDATE SET nom = ?3, profondeur = ?4, role = ?5, separateur = ?6,
                               selectionnable = ?7",
                params![
                    compte,
                    dossier.chemin,
                    dossier.nom(),
                    dossier.profondeur() as u32,
                    dossier.role().unwrap_or(""),
                    dossier.separateur,
                    dossier.selectionnable() as i32
                ],
            )?;
        }
        let disparus: Vec<i64> = {
            let mut connus =
                transaction.prepare("SELECT id, chemin FROM folders WHERE account_id = ?1")?;
            let lignes = connus
                .query_map(params![compte], |l| Ok((l.get::<_, i64>(0)?, l.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            lignes
                .into_iter()
                .filter(|(_, chemin)| !chemins.contains(&chemin.as_str()))
                .map(|(id, _)| id)
                .collect()
        };
        for id in disparus {
            transaction.execute("DELETE FROM folders WHERE id = ?1", params![id])?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Note les compteurs que le serveur annonce pour les dossiers d'un compte.
    pub fn poser_compteurs(&self, compte: i64, statuts: &[Statut]) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for statut in statuts {
            transaction.execute(
                "UPDATE folders SET nb_messages = ?3, nb_non_lus = ?4
                 WHERE account_id = ?1 AND chemin = ?2",
                params![compte, statut.chemin, statut.messages, statut.non_lus],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Recompte un dossier d'après son index, une fois celui-ci à jour.
    pub fn recompter(&self, dossier: i64) -> Resultat<()> {
        self.base.execute(
            "UPDATE folders SET
                nb_messages = (SELECT COUNT(*) FROM messages WHERE folder_id = ?1),
                nb_non_lus  = (SELECT COUNT(*) FROM messages WHERE folder_id = ?1 AND lu = 0)
             WHERE id = ?1",
            params![dossier],
        )?;
        Ok(())
    }

    /// Identifiant interne d'un dossier.
    pub fn dossier_id(&self, compte: i64, chemin: &str) -> Resultat<i64> {
        Ok(self.base.query_row(
            "SELECT id FROM folders WHERE account_id = ?1 AND chemin = ?2",
            params![compte, chemin],
            |l| l.get(0),
        )?)
    }

    /// Un dossier par son identifiant interne.
    pub fn dossier(&self, id: i64) -> Resultat<Option<DossierLocal>> {
        let mut requete = self.base.prepare(&format!("{SELECT_DOSSIER} WHERE f.id = ?1"))?;
        Ok(requete.query_row(params![id], lire_dossier).optional()?)
    }

    /// Oublie le `HIGHESTMODSEQ` d'un dossier : la prochaine synchronisation
    /// relira tous ses drapeaux au lieu de demander ce qui a changé.
    pub fn oublier_modseq(&self, dossier: i64) -> Resultat<()> {
        self.base.execute("UPDATE folders SET highestmodseq = 0 WHERE id = ?1", params![dossier])?;
        Ok(())
    }

    /// Note l'état de synchronisation d'un dossier après `SELECT`/`EXAMINE`.
    ///
    /// Rend vrai si l'`UIDVALIDITY` a changé : le contenu local du dossier est
    /// alors caduc et doit être relu intégralement (décision 15).
    pub fn poser_etat(&self, dossier: i64, etat: &EtatDossier) -> Resultat<bool> {
        let ancienne: u32 = self
            .base
            .query_row("SELECT uidvalidity FROM folders WHERE id = ?1", params![dossier], |l| {
                l.get(0)
            })
            .optional()?
            .unwrap_or(0);
        let rupture = ancienne != 0 && ancienne != etat.uid_validity;
        if rupture {
            self.base
                .execute("DELETE FROM messages WHERE folder_id = ?1", params![dossier])?;
        }
        self.base.execute(
            "UPDATE folders SET uidvalidity = ?2, uidnext = ?3, highestmodseq = ?4 WHERE id = ?1",
            params![dossier, etat.uid_validity, etat.uid_next, etat.highest_mod_seq],
        )?;
        Ok(rupture)
    }

    /// Dossiers d'un compte, dans l'ordre d'affichage, masqués compris.
    ///
    /// INBOX d'abord — c'est là qu'on va — puis les dossiers à rôle, puis les
    /// autres par ordre alphabétique, chacun sous son parent.
    pub fn dossiers(&self, compte: i64) -> Resultat<Vec<DossierLocal>> {
        let mut requete = self.base.prepare(&format!(
            "{SELECT_DOSSIER} WHERE f.account_id = ?1
             ORDER BY (f.chemin = 'INBOX') DESC, (f.role != '') DESC, f.chemin"
        ))?;
        let dossiers = requete
            .query_map(params![compte], lire_dossier)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(dossiers)
    }

    /// Dossiers de la rubrique Favoris, tous comptes confondus, dans leur ordre.
    pub fn favoris(&self) -> Resultat<Vec<DossierLocal>> {
        let mut requete = self.base.prepare(&format!(
            "{SELECT_DOSSIER} WHERE f.favorite_rank IS NOT NULL ORDER BY f.favorite_rank, f.id"
        ))?;
        let dossiers = requete
            .query_map([], lire_dossier)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(dossiers)
    }

    /// Replie — ou déplie — les sous-dossiers d'un dossier dans l'arborescence.
    pub fn replier_dossier(&self, dossier: i64, replie: bool) -> Resultat<()> {
        self.base.execute(
            "UPDATE folders SET replie_local = ?2 WHERE id = ?1",
            params![dossier, replie as i32],
        )?;
        Ok(())
    }

    /// Masque — ou réaffiche — un dossier dans l'arborescence (décision 4).
    pub fn masquer(&self, dossier: i64, masque: bool) -> Resultat<()> {
        self.base.execute(
            "UPDATE folders SET hidden_local = ?2 WHERE id = ?1",
            params![dossier, masque as i32],
        )?;
        Ok(())
    }

    /// Épingle un dossier en fin de rubrique Favoris, ou l'en retire.
    pub fn epingler(&self, dossier: i64, favori: bool) -> Resultat<()> {
        if favori {
            self.base.execute(
                "UPDATE folders SET favorite_rank =
                    (SELECT COALESCE(MAX(favorite_rank), 0) + 1 FROM folders)
                 WHERE id = ?1 AND favorite_rank IS NULL",
                params![dossier],
            )?;
        } else {
            self.base
                .execute("UPDATE folders SET favorite_rank = NULL WHERE id = ?1", params![dossier])?;
        }
        Ok(())
    }

    /// Place un dossier dans la rubrique Favoris, juste avant un autre favori,
    /// ou à la fin si `avant` est absent. Un dossier déjà favori change de
    /// place ; les rangs des autres se décalent pour lui faire de la place.
    pub fn placer_favori(&self, dossier: i64, avant: Option<i64>) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        transaction
            .execute("UPDATE folders SET favorite_rank = NULL WHERE id = ?1", params![dossier])?;
        let rang: Option<i64> = match avant {
            Some(autre) if autre != dossier => transaction
                .query_row(
                    "SELECT favorite_rank FROM folders WHERE id = ?1",
                    params![autre],
                    |l| l.get::<_, Option<i64>>(0),
                )
                .optional()?
                .flatten(),
            _ => None,
        };
        match rang {
            Some(rang) => {
                transaction.execute(
                    "UPDATE folders SET favorite_rank = favorite_rank + 1
                     WHERE favorite_rank IS NOT NULL AND favorite_rank >= ?1",
                    params![rang],
                )?;
                transaction.execute(
                    "UPDATE folders SET favorite_rank = ?2 WHERE id = ?1",
                    params![dossier, rang],
                )?;
            }
            None => {
                transaction.execute(
                    "UPDATE folders SET favorite_rank =
                        (SELECT COALESCE(MAX(favorite_rank), 0) + 1 FROM folders)
                     WHERE id = ?1",
                    params![dossier],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    // ------------------------------------------------------------ messages

    /// Range les messages d'un dossier. Un message déjà connu ne voit changer
    /// que ses drapeaux.
    pub fn poser_messages(&self, dossier: i64, messages: &[MessageLocal]) -> Resultat<()> {
        for lot in messages.chunks(LOT_ECRITURE) {
            let transaction = self.base.unchecked_transaction()?;
            {
                let mut insertion = transaction.prepare(
                    "INSERT OR IGNORE INTO messages
                       (folder_id, uid, message_id, expediteur, adresse, sujet, date, horodatage,
                        taille, lu, repondu, pieces, suivi, importance)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                )?;
                let mut mise_a_jour = transaction.prepare(
                    "UPDATE messages SET lu = ?3, repondu = ?4, suivi = ?6, importance = ?7,
                        pieces = CASE WHEN pieces_certain = 1 THEN pieces ELSE ?5 END
                     WHERE folder_id = ?1 AND uid = ?2",
                )?;
                // Un expéditeur compte une fois par message nouveau dans l'index.
                let mut expediteur = transaction.prepare(
                    "INSERT INTO expediteurs (adresse, nom, nb, dernier) VALUES (lower(?1), ?2, 1, ?3)
                     ON CONFLICT(adresse) DO UPDATE SET nb = nb + 1, dernier = MAX(dernier, excluded.dernier),
                        nom = CASE WHEN excluded.nom <> '' THEN excluded.nom ELSE nom END",
                )?;
                for m in lot {
                    let nouveau = insertion.execute(params![
                        dossier,
                        m.uid,
                        m.message_id,
                        m.expediteur,
                        m.adresse,
                        m.sujet,
                        m.date,
                        m.horodatage,
                        m.taille,
                        m.lu as i32,
                        m.repondu as i32,
                        m.pieces as i32,
                        m.suivi as i32,
                        m.importance as i32
                    ])? == 1;
                    if nouveau {
                        if !m.adresse.is_empty() {
                            expediteur.execute(params![m.adresse, m.expediteur, m.horodatage])?;
                        }
                    } else {
                        mise_a_jour.execute(params![
                            dossier,
                            m.uid,
                            m.lu as i32,
                            m.repondu as i32,
                            m.pieces as i32,
                            m.suivi as i32,
                            m.importance as i32
                        ])?;
                    }
                }
            }
            transaction.commit()?;
        }
        Ok(())
    }

    /// Applique des drapeaux rendus par le serveur à des messages connus.
    pub fn poser_drapeaux(&self, dossier: i64, drapeaux: &[(u32, Vec<String>)]) -> Resultat<()> {
        for lot in drapeaux.chunks(LOT_ECRITURE) {
            let transaction = self.base.unchecked_transaction()?;
            for (uid, liste) in lot {
                let lu = liste.iter().any(|d| d.eq_ignore_ascii_case("\\Seen"));
                let repondu = liste.iter().any(|d| d.eq_ignore_ascii_case("\\Answered"));
                let suivi = liste.iter().any(|d| d.eq_ignore_ascii_case("\\Flagged"));
                transaction.execute(
                    "UPDATE messages SET lu = ?3, repondu = ?4, suivi = ?5 WHERE folder_id = ?1 AND uid = ?2",
                    params![dossier, uid, lu as i32, repondu as i32, suivi as i32],
                )?;
            }
            transaction.commit()?;
        }
        Ok(())
    }

    // ------------------------------------------------------ correspondants

    /// Note les destinataires d'un message envoyé : un nom connu n'est pas
    /// effacé par une adresse saisie sans nom.
    pub fn noter_correspondants(&self, adresses: &[(String, String)], quand: i64) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for (nom, adresse) in adresses {
            transaction.execute(
                "INSERT INTO correspondants (adresse, nom, envois, dernier) VALUES (?1, ?2, 1, ?3)
                 ON CONFLICT(adresse) DO UPDATE SET
                    envois = envois + 1, dernier = ?3,
                    nom = CASE WHEN ?2 <> '' THEN ?2 ELSE nom END",
                params![adresse, nom, quand],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Adresses proposées à la saisie : celles à qui l'on a écrit, puis les
    /// expéditeurs de l'index, dont l'adresse ou le nom contient `filtre`. Les
    /// plus fréquentes d'abord.
    pub fn contacts(&self, filtre: &str, limite: usize) -> Resultat<Vec<(String, String)>> {
        let motif = format!(
            "%{}%",
            filtre.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
        );
        // Un seul agrégat par groupe : SQLite rend alors le nom et l'adresse de
        // la ligne qui le porte (un correspondant avant un expéditeur, le plus
        // récent à poids égal). Avec deux, le nom venait d'une ligne quelconque.
        let mut requete = self.base.prepare(
            "SELECT nom, adresse, MAX(poids * 10000000000 + dernier) AS cle FROM (
                SELECT nom, adresse, 1000000 + envois AS poids, dernier FROM correspondants
                 WHERE adresse LIKE ?1 ESCAPE '\\' OR nom LIKE ?1 ESCAPE '\\'
                UNION ALL
                SELECT nom, adresse, nb, dernier FROM expediteurs
                 WHERE adresse LIKE ?1 ESCAPE '\\' OR nom LIKE ?1 ESCAPE '\\'
             )
             GROUP BY lower(adresse)
             ORDER BY cle DESC
             LIMIT ?2",
        )?;
        let lignes = requete
            .query_map(params![motif, limite as i64], |l| Ok((l.get(0)?, l.get(1)?)))?
            .collect::<Result<Vec<(String, String)>, _>>()?;
        Ok(lignes)
    }

    /// Pièces jointes constatées à la lecture d'un message : elles priment
    /// désormais sur ce que les en-têtes laissaient supposer.
    pub fn poser_pieces(&self, dossier: i64, uid: u32, pieces: bool) -> Resultat<()> {
        self.base.execute(
            "UPDATE messages SET pieces = ?3, pieces_certain = 1 WHERE folder_id = ?1 AND uid = ?2",
            params![dossier, uid, pieces as i32],
        )?;
        Ok(())
    }

    /// Pose ou retire localement le drapeau de suivi.
    pub fn marquer_suivi(&self, dossier: i64, uids: &[u32], suivi: bool) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for uid in uids {
            transaction.execute(
                "UPDATE messages SET suivi = ?3 WHERE folder_id = ?1 AND uid = ?2",
                params![dossier, uid, suivi as i32],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Marque localement des messages comme lus ou non lus.
    pub fn marquer_lu(&self, dossier: i64, uids: &[u32], lu: bool) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for uid in uids {
            transaction.execute(
                "UPDATE messages SET lu = ?3 WHERE folder_id = ?1 AND uid = ?2",
                params![dossier, uid, lu as i32],
            )?;
        }
        transaction.commit()?;
        self.recompter(dossier)
    }

    /// Retire de l'index des intervalles d'UID (`VANISHED`).
    pub fn retirer_intervalles(&self, dossier: i64, intervalles: &[(u32, u32)]) -> Resultat<()> {
        let transaction = self.base.unchecked_transaction()?;
        for (debut, fin) in intervalles {
            transaction.execute(
                "DELETE FROM messages WHERE folder_id = ?1 AND uid BETWEEN ?2 AND ?3",
                params![dossier, debut, fin],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Retire des messages désignés de l'index.
    pub fn retirer(&self, dossier: i64, uids: &[u32]) -> Resultat<()> {
        let intervalles: Vec<(u32, u32)> = uids.iter().map(|&u| (u, u)).collect();
        self.retirer_intervalles(dossier, &intervalles)
    }

    /// UID connus d'un dossier.
    pub fn uids(&self, dossier: i64) -> Resultat<Vec<u32>> {
        let mut requete = self.base.prepare("SELECT uid FROM messages WHERE folder_id = ?1")?;
        let uids = requete
            .query_map(params![dossier], |l| l.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(uids)
    }

    /// Messages d'un dossier, du plus récent au plus ancien — par date de
    /// réception, que le déplacement entre boîtes conserve, et non par UID,
    /// qu'il renouvelle.
    pub fn messages(&self, dossier: i64) -> Resultat<Vec<MessageLocal>> {
        let mut requete = self.base.prepare(&format!(
            "{SELECT_MESSAGE} WHERE folder_id = ?1 ORDER BY horodatage DESC, uid DESC"
        ))?;
        let messages = requete
            .query_map(params![dossier], lire_message)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(messages)
    }

    /// Un message de l'index.
    pub fn message(&self, dossier: i64, uid: u32) -> Resultat<Option<MessageLocal>> {
        let mut requete = self
            .base
            .prepare(&format!("{SELECT_MESSAGE} WHERE folder_id = ?1 AND uid = ?2"))?;
        Ok(requete.query_row(params![dossier, uid], lire_message).optional()?)
    }

    // -------------------------------------------- messages gardés sur le poste

    /// Identifiant dans l'index, date de réception et état « lu » d'un message.
    pub fn identite_message(&self, dossier: i64, uid: u32) -> Resultat<Option<(i64, i64, bool)>> {
        Ok(self
            .base
            .query_row(
                "SELECT id, horodatage, lu FROM messages WHERE folder_id = ?1 AND uid = ?2",
                params![dossier, uid],
                |l| Ok((l.get(0)?, l.get(1)?, l.get::<_, i32>(2)? != 0)),
            )
            .optional()?)
    }

    /// Identifiant et état « lu » d'un message gardé sur le poste (`full`) ;
    /// `None` s'il ne l'est pas, même si un fichier porte son identifiant.
    pub fn garde_sur_le_poste(&self, dossier: i64, uid: u32) -> Resultat<Option<(i64, bool)>> {
        Ok(self
            .base
            .query_row(
                "SELECT id, lu FROM messages WHERE folder_id = ?1 AND uid = ?2 AND body_state = 'full'",
                params![dossier, uid],
                |l| Ok((l.get(0)?, l.get::<_, i32>(1)? != 0)),
            )
            .optional()?)
    }

    /// Note qu'un message est gardé sur le poste (`full`), ne l'est plus
    /// (`headers`), ou n'a pas pu être lu (`echec` : il n'est pas réessayé).
    pub fn poser_etat_corps(&self, id: i64, etat: &str) -> Resultat<()> {
        self.base.execute("UPDATE messages SET body_state = ?2 WHERE id = ?1", params![id, etat])?;
        Ok(())
    }

    /// Messages d'un dossier reçus depuis `depuis` et pas encore gardés, du
    /// plus récent au plus ancien : identifiant et UID.
    pub fn a_garder(&self, dossier: i64, depuis: i64, nombre: usize) -> Resultat<Vec<(i64, u32)>> {
        let mut requete = self.base.prepare(
            "SELECT id, uid FROM messages
             WHERE folder_id = ?1 AND body_state = 'headers' AND horodatage >= ?2
             ORDER BY horodatage DESC, uid DESC LIMIT ?3",
        )?;
        let liste = requete
            .query_map(params![dossier, depuis, nombre as i64], |l| Ok((l.get(0)?, l.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(liste)
    }

    /// Messages gardés sur le poste mais reçus avant `avant` : sortis de la
    /// fenêtre.
    pub fn gardes_perimes(&self, avant: i64) -> Resultat<Vec<i64>> {
        let mut requete =
            self.base.prepare("SELECT id FROM messages WHERE body_state = 'full' AND horodatage < ?1")?;
        let liste = requete.query_map(params![avant], |l| l.get(0))?.collect::<Result<Vec<_>, _>>()?;
        Ok(liste)
    }

    /// Identifiants de tous les messages gardés sur le poste.
    pub fn gardes(&self) -> Resultat<HashSet<i64>> {
        let mut requete = self.base.prepare("SELECT id FROM messages WHERE body_state = 'full'")?;
        let liste = requete.query_map([], |l| l.get(0))?.collect::<Result<HashSet<i64>, _>>()?;
        Ok(liste)
    }

    // ---------------------------------------------- déplacements en cours

    /// Inscrit un déplacement entre boîtes, une fois le message lu et copié.
    pub fn inscrire_operation(&self, op: &Operation) -> Resultat<i64> {
        self.base.execute(
            "INSERT INTO pending_ops
               (compte_source, chemin_source, validite_source, uid_source, compte_cible,
                chemin_cible, message_id, drapeaux, date_interne, fichier, etape)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                op.compte_source,
                op.chemin_source,
                op.validite_source,
                op.uid_source,
                op.compte_cible,
                op.chemin_cible,
                op.message_id,
                op.drapeaux,
                op.date_interne,
                op.fichier,
                op.etape.code()
            ],
        )?;
        Ok(self.base.last_insert_rowid())
    }

    pub fn avancer_operation(&self, id: i64, etape: Etape) -> Resultat<()> {
        self.base.execute(
            "UPDATE pending_ops SET etape = ?2, erreur = '' WHERE id = ?1",
            params![id, etape.code()],
        )?;
        Ok(())
    }

    pub fn noter_echec_operation(&self, id: i64, erreur: &str) -> Resultat<()> {
        self.base
            .execute("UPDATE pending_ops SET erreur = ?2 WHERE id = ?1", params![id, erreur])?;
        Ok(())
    }

    /// Solde un déplacement terminé.
    pub fn clore_operation(&self, id: i64) -> Resultat<()> {
        self.base.execute("DELETE FROM pending_ops WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Déplacements en cours dont un compte est la source.
    pub fn operations(&self, compte_source: i64) -> Resultat<Vec<Operation>> {
        let mut requete = self.base.prepare(
            "SELECT id, compte_source, chemin_source, validite_source, uid_source, compte_cible,
                    chemin_cible, message_id, drapeaux, date_interne, fichier, etape, erreur
             FROM pending_ops WHERE compte_source = ?1 ORDER BY id",
        )?;
        let operations = requete
            .query_map(params![compte_source], |l| {
                Ok(Operation {
                    id: l.get(0)?,
                    compte_source: l.get(1)?,
                    chemin_source: l.get(2)?,
                    validite_source: l.get(3)?,
                    uid_source: l.get(4)?,
                    compte_cible: l.get(5)?,
                    chemin_cible: l.get(6)?,
                    message_id: l.get(7)?,
                    drapeaux: l.get(8)?,
                    date_interne: l.get(9)?,
                    fichier: l.get(10)?,
                    etape: Etape::lire(&l.get::<_, String>(11)?),
                    erreur: l.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(operations)
    }

    /// Nombre de déplacements en cours, tous comptes confondus.
    pub fn nombre_operations(&self) -> Resultat<u32> {
        Ok(self.base.query_row("SELECT COUNT(*) FROM pending_ops", [], |l| l.get(0))?)
    }
}

const SELECT_DOSSIER: &str = "SELECT f.id, f.account_id, f.chemin, f.nom, f.separateur, f.profondeur,
        f.role, f.selectionnable, f.nb_messages, f.nb_non_lus, f.uidvalidity, f.uidnext,
        f.highestmodseq, f.hidden_local, f.favorite_rank, f.replie_local
     FROM folders f";

fn lire_dossier(l: &rusqlite::Row<'_>) -> rusqlite::Result<DossierLocal> {
    Ok(DossierLocal {
        id: l.get(0)?,
        compte_id: l.get(1)?,
        chemin: l.get(2)?,
        nom: l.get(3)?,
        separateur: l.get(4)?,
        profondeur: l.get(5)?,
        role: l.get(6)?,
        selectionnable: l.get::<_, i32>(7)? != 0,
        messages: l.get(8)?,
        non_lus: l.get(9)?,
        uid_validity: l.get(10)?,
        uid_next: l.get(11)?,
        highest_mod_seq: l.get(12)?,
        masque: l.get::<_, i32>(13)? != 0,
        favori: l.get(14)?,
        replie: l.get::<_, i32>(15)? != 0,
    })
}

const SELECT_MESSAGE: &str = "SELECT uid, message_id, expediteur, adresse, sujet, date,
        horodatage, taille, lu, repondu, pieces, suivi, importance
     FROM messages";

fn lire_message(l: &rusqlite::Row<'_>) -> rusqlite::Result<MessageLocal> {
    Ok(MessageLocal {
        uid: l.get(0)?,
        message_id: l.get(1)?,
        expediteur: l.get(2)?,
        adresse: l.get(3)?,
        sujet: l.get(4)?,
        date: l.get(5)?,
        horodatage: l.get(6)?,
        taille: l.get(7)?,
        lu: l.get::<_, i32>(8)? != 0,
        repondu: l.get::<_, i32>(9)? != 0,
        pieces: l.get::<_, i32>(10)? != 0,
        suivi: l.get::<_, i32>(11)? != 0,
        importance: l.get::<_, i32>(12)?.clamp(-1, 1) as i8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(chemin: &str, attributs: &[&str]) -> Dossier {
        Dossier {
            chemin: chemin.to_string(),
            separateur: "/".to_string(),
            attributs: attributs.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn message(uid: u32, horodatage: i64, lu: bool) -> MessageLocal {
        MessageLocal { uid, horodatage, lu, sujet: format!("m{uid}"), ..Default::default() }
    }

    fn boite() -> (Magasin, i64, i64) {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        (m, compte, id)
    }

    #[test]
    fn messages_gardes_sur_le_poste() {
        let m = Magasin::en_memoire().unwrap();
        let c = m.compte("a@exemple.fr", "imap.exemple.fr", 993, "a").unwrap();
        m.poser_dossiers(c, &[Dossier { chemin: "INBOX".into(), separateur: "/".into(), attributs: vec![] }])
            .unwrap();
        let d = m.dossier_id(c, "INBOX").unwrap();
        let msg = |uid: u32, horodatage: i64| MessageLocal { uid, horodatage, ..Default::default() };
        m.poser_messages(d, &[msg(1, 100), msg(2, 5_000), msg(3, 9_000)]).unwrap();
        let ids: Vec<i64> = m.a_garder(d, 1_000, 10).unwrap().into_iter().map(|(id, _)| id).collect();
        let uids: Vec<u32> = m.a_garder(d, 1_000, 10).unwrap().into_iter().map(|(_, u)| u).collect();
        // Les plus récents d'abord, rien d'avant la fenêtre.
        assert_eq!(uids, vec![3, 2]);
        m.poser_etat_corps(ids[0], "full").unwrap();
        m.poser_etat_corps(ids[1], "echec").unwrap();
        assert!(m.a_garder(d, 1_000, 10).unwrap().is_empty());
        assert_eq!(m.gardes().unwrap(), HashSet::from([ids[0]]));
        // Une synchronisation ne remet pas l'état à zéro.
        m.poser_messages(d, &[msg(3, 9_000)]).unwrap();
        assert_eq!(m.gardes().unwrap(), HashSet::from([ids[0]]));
        assert_eq!(m.identite_message(d, 3).unwrap().map(|(id, h, _)| (id, h)), Some((ids[0], 9_000)));
        assert!(m.gardes_perimes(9_000).unwrap().is_empty());
        assert_eq!(m.gardes_perimes(9_001).unwrap(), vec![ids[0]]);
    }

    #[test]
    fn ordre_des_comptes() {
        let m = Magasin::en_memoire().unwrap();
        let a = m.compte("a@exemple.fr", "imap.exemple.fr", 993, "a").unwrap();
        let b = m.compte("b@exemple.fr", "imap.exemple.fr", 993, "b").unwrap();
        let c = m.compte("c@exemple.fr", "imap.exemple.fr", 993, "c").unwrap();
        let ordre = |m: &Magasin| m.comptes().unwrap().into_iter().map(|c| c.id).collect::<Vec<_>>();
        assert_eq!(ordre(&m), vec![a, b, c]);
        m.placer_compte(c, Some(a)).unwrap();
        assert_eq!(ordre(&m), vec![c, a, b]);
        m.placer_compte(c, None).unwrap();
        assert_eq!(ordre(&m), vec![a, b, c]);
        m.decaler_compte(b, -1).unwrap();
        assert_eq!(ordre(&m), vec![b, a, c]);
        m.decaler_compte(b, -1).unwrap();
        assert_eq!(ordre(&m), vec![b, a, c]);
        m.decaler_compte(c, 1).unwrap();
        assert_eq!(ordre(&m), vec![b, a, c]);
        // Un compte réenregistré garde sa place ; un nouveau va à la fin.
        m.compte("b@exemple.fr", "autre.exemple.fr", 993, "b").unwrap();
        let d = m.compte("d@exemple.fr", "imap.exemple.fr", 993, "d").unwrap();
        assert_eq!(ordre(&m), vec![b, a, c, d]);
    }

    #[test]
    fn comptes_et_arborescence() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("essai@exemple.fr", "imap.exemple.fr", 993, "essai@exemple.fr").unwrap();
        assert!(compte > 0);
        // Deux fois le même compte ne fait pas deux lignes.
        let encore = m.compte("essai@exemple.fr", "imap.exemple.fr", 993, "essai@exemple.fr").unwrap();
        assert_eq!(compte, encore);
        assert_eq!(m.comptes().unwrap().len(), 1);

        m.poser_dossiers(
            compte,
            &[
                dossier("INBOX", &[]),
                dossier("Sent", &["\\Sent"]),
                dossier("Essais/Factures", &[]),
                dossier("Parent", &["\\Noselect"]),
            ],
        )
        .unwrap();
        let dossiers = m.dossiers(compte).unwrap();
        assert_eq!(dossiers.len(), 4);
        // INBOX en tête, puis le dossier à rôle.
        assert_eq!(dossiers[0].chemin, "INBOX");
        assert_eq!(dossiers[1].role, "Sent");
        assert_eq!(dossiers[2].nom, "Factures");
        assert_eq!(dossiers[2].profondeur, 1);
        assert!(!dossiers[3].selectionnable);
    }

    #[test]
    fn dossier_disparu_du_serveur_est_retire() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[]), dossier("Vieux", &[])]).unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let dossiers = m.dossiers(compte).unwrap();
        assert_eq!(dossiers.len(), 1);
        assert_eq!(dossiers[0].chemin, "INBOX");
    }

    #[test]
    fn etat_local_survit_a_la_relecture_de_l_arborescence() {
        let (m, compte, id) = boite();
        m.masquer(id, true).unwrap();
        m.epingler(id, true).unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[]), dossier("Autre", &[])]).unwrap();
        let inbox = m.dossier(id).unwrap().unwrap();
        assert!(inbox.masque);
        assert_eq!(inbox.favori, Some(1));
        assert_eq!(m.favoris().unwrap().len(), 1);
        m.epingler(id, false).unwrap();
        assert!(m.favoris().unwrap().is_empty());
    }

    #[test]
    fn favoris_places_et_deplaces() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("A", &[]), dossier("B", &[]), dossier("C", &[])]).unwrap();
        let id = |c: &str| m.dossier_id(compte, c).unwrap();
        let ordre = || m.favoris().unwrap().iter().map(|d| d.chemin.clone()).collect::<Vec<_>>();

        m.placer_favori(id("A"), None).unwrap();
        m.placer_favori(id("B"), None).unwrap();
        assert_eq!(ordre(), vec!["A", "B"]);
        // Déposé sur « A » : se glisse avant lui.
        m.placer_favori(id("C"), Some(id("A"))).unwrap();
        assert_eq!(ordre(), vec!["C", "A", "B"]);
        // Un favori déplacé ne se dédouble pas.
        m.placer_favori(id("B"), Some(id("C"))).unwrap();
        assert_eq!(ordre(), vec!["B", "C", "A"]);
        // Déposé sur lui-même ou sur la rubrique : à la fin.
        m.placer_favori(id("B"), Some(id("B"))).unwrap();
        assert_eq!(ordre(), vec!["C", "A", "B"]);
        // « avant » qui n'est pas un favori : à la fin aussi.
        m.epingler(id("A"), false).unwrap();
        m.placer_favori(id("C"), Some(id("A"))).unwrap();
        assert_eq!(ordre(), vec!["B", "C"]);
    }

    #[test]
    fn messages_tries_par_date_de_reception() {
        let (m, _, id) = boite();
        // Un message déplacé depuis une autre boîte reçoit un grand UID mais
        // garde sa date : il doit rester à sa place dans la liste.
        m.poser_messages(id, &[message(1, 100, true), message(2, 300, false), message(50, 200, false)])
            .unwrap();
        let uids: Vec<u32> = m.messages(id).unwrap().iter().map(|m| m.uid).collect();
        assert_eq!(uids, vec![2, 50, 1]);
    }

    #[test]
    fn compteurs_du_serveur_puis_de_l_index() {
        let (m, compte, id) = boite();
        m.poser_compteurs(compte, &[Statut { chemin: "INBOX".into(), messages: 10, non_lus: 4 }])
            .unwrap();
        let d = m.dossier(id).unwrap().unwrap();
        assert_eq!((d.messages, d.non_lus), (10, 4));

        m.poser_messages(id, &[message(1, 1, true), message(2, 2, false)]).unwrap();
        m.recompter(id).unwrap();
        let d = m.dossier(id).unwrap().unwrap();
        assert_eq!((d.messages, d.non_lus), (2, 1));

        m.marquer_lu(id, &[2], true).unwrap();
        assert_eq!(m.dossier(id).unwrap().unwrap().non_lus, 0);
    }

    #[test]
    fn changement_d_uidvalidity_vide_le_dossier() {
        let (m, _, id) = boite();
        let etat = EtatDossier { messages: 1, uid_validity: 100, uid_next: 2, highest_mod_seq: 5 };
        assert!(!m.poser_etat(id, &etat).unwrap(), "premier état : pas de rupture");
        m.poser_messages(id, &[MessageLocal { uid: 1, ..Default::default() }]).unwrap();

        // Même UIDVALIDITY : l'index reste.
        assert!(!m.poser_etat(id, &etat).unwrap());
        assert_eq!(m.messages(id).unwrap().len(), 1);

        // UIDVALIDITY changée : les UID d'avant ne désignent plus rien.
        let apres = EtatDossier { uid_validity: 101, ..etat.clone() };
        assert!(m.poser_etat(id, &apres).unwrap(), "rupture attendue");
        assert!(m.messages(id).unwrap().is_empty());
    }

    #[test]
    fn relire_un_message_met_a_jour_ses_drapeaux() {
        let (m, _, id) = boite();
        m.poser_messages(id, &[MessageLocal { uid: 1, sujet: "Objet".into(), lu: false, ..Default::default() }]).unwrap();
        m.poser_messages(id, &[MessageLocal { uid: 1, sujet: "Objet".into(), lu: true, ..Default::default() }]).unwrap();
        let messages = m.messages(id).unwrap();
        assert_eq!(messages.len(), 1, "le même UID ne doit pas se dédoubler");
        assert!(messages[0].lu);
    }

    #[test]
    fn vanished_et_drapeaux_appliques() {
        let (m, _, id) = boite();
        m.poser_messages(id, &(1..=6).map(|u| message(u, u as i64, false)).collect::<Vec<_>>())
            .unwrap();
        m.retirer_intervalles(id, &[(2, 3), (6, 100)]).unwrap();
        let mut uids = m.uids(id).unwrap();
        uids.sort();
        assert_eq!(uids, vec![1, 4, 5]);
        m.poser_drapeaux(id, &[(4, vec!["\\Seen".into(), "\\Answered".into()])]).unwrap();
        let m4 = m.message(id, 4).unwrap().unwrap();
        assert!(m4.lu && m4.repondu);
        m.retirer(id, &[1]).unwrap();
        assert!(m.message(id, 1).unwrap().is_none());
    }

    #[test]
    fn file_des_deplacements() {
        let m = Magasin::en_memoire().unwrap();
        let a = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        let b = m.compte("c@d.fr", "h", 993, "c@d.fr").unwrap();
        let op = Operation {
            id: 0,
            compte_source: a,
            chemin_source: "INBOX".into(),
            validite_source: 7,
            uid_source: 3,
            compte_cible: b,
            chemin_cible: "Archives".into(),
            message_id: "<x@y>".into(),
            drapeaux: "\\Seen".into(),
            date_interne: "16-Sep-2026 18:00:00 +0200".into(),
            fichier: "/tmp/x.eml".into(),
            etape: Etape::Deposer,
            erreur: String::new(),
        };
        let id = m.inscrire_operation(&op).unwrap();
        assert_eq!(m.nombre_operations().unwrap(), 1);
        m.noter_echec_operation(id, "coupure").unwrap();
        m.avancer_operation(id, Etape::Purger).unwrap();
        let ops = m.operations(a).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].etape, Etape::Purger);
        assert!(ops[0].erreur.is_empty(), "une étape franchie efface l'échec précédent");
        assert!(m.operations(b).unwrap().is_empty());
        m.clore_operation(id).unwrap();
        assert_eq!(m.nombre_operations().unwrap(), 0);
    }

    #[test]
    fn retirer_un_compte_emporte_son_index() {
        let (m, compte, id) = boite();
        m.poser_messages(id, &[message(1, 1, true)]).unwrap();
        m.retirer_compte(compte).unwrap();
        assert!(m.comptes().unwrap().is_empty());
        assert!(m.dossier(id).unwrap().is_none());
    }

    #[test]
    fn migration_d_un_index_de_version_1() {
        // Un index de première génération : sans les colonnes de la version 2.
        let base = Connection::open_in_memory().unwrap();
        base.execute_batch(
            "CREATE TABLE accounts (id INTEGER PRIMARY KEY, adresse TEXT NOT NULL UNIQUE,
                hote TEXT NOT NULL, port INTEGER NOT NULL DEFAULT 993, utilisateur TEXT NOT NULL);
             CREATE TABLE folders (id INTEGER PRIMARY KEY, account_id INTEGER NOT NULL,
                chemin TEXT NOT NULL, nom TEXT NOT NULL, profondeur INTEGER NOT NULL DEFAULT 0,
                role TEXT NOT NULL DEFAULT '', uidvalidity INTEGER NOT NULL DEFAULT 0,
                uidnext INTEGER NOT NULL DEFAULT 0, highestmodseq INTEGER NOT NULL DEFAULT 0,
                hidden_local INTEGER NOT NULL DEFAULT 0, favorite_rank INTEGER,
                UNIQUE (account_id, chemin));
             CREATE TABLE messages (id INTEGER PRIMARY KEY, folder_id INTEGER NOT NULL,
                uid INTEGER NOT NULL, message_id TEXT NOT NULL DEFAULT '',
                expediteur TEXT NOT NULL DEFAULT '', adresse TEXT NOT NULL DEFAULT '',
                sujet TEXT NOT NULL DEFAULT '', date TEXT NOT NULL DEFAULT '',
                taille INTEGER NOT NULL DEFAULT 0, lu INTEGER NOT NULL DEFAULT 0,
                repondu INTEGER NOT NULL DEFAULT 0, body_state TEXT NOT NULL DEFAULT 'headers',
                UNIQUE (folder_id, uid));
             INSERT INTO accounts (adresse, hote, utilisateur) VALUES ('a@b.fr', 'h', 'a@b.fr');
             INSERT INTO folders (account_id, chemin, nom) VALUES (1, 'INBOX', 'INBOX');
             INSERT INTO messages (folder_id, uid, sujet) VALUES (1, 1, 'ancien');",
        )
        .unwrap();
        let m = Magasin::preparer(base).unwrap();
        let messages = m.messages(1).unwrap();
        assert_eq!(messages.len(), 1, "l'index existant est conservé");
        assert_eq!(messages[0].sujet, "ancien");
        assert!(!messages[0].pieces);
        assert!(m.dossier(1).unwrap().unwrap().selectionnable);
    }

    #[test]
    fn migration_version_3_fait_relire_les_en_tetes() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        m.poser_etat(id, &EtatDossier { uid_validity: 42, ..Default::default() }).unwrap();
        // Retour à un index de version 2, sans les colonnes de pièces jointes.
        m.base
            .execute_batch(
                "ALTER TABLE messages DROP COLUMN pieces;
                 ALTER TABLE messages DROP COLUMN pieces_certain;
                 ALTER TABLE messages DROP COLUMN suivi;
                 ALTER TABLE messages DROP COLUMN importance;
                 PRAGMA user_version = 2;",
            )
            .unwrap();
        m.migrer().unwrap();
        assert_eq!(m.dossier(id).unwrap().unwrap().uid_validity, 0, "dossier à relire");
        m.poser_messages(id, &[MessageLocal { uid: 1, pieces: true, ..Default::default() }]).unwrap();
        assert!(m.messages(id).unwrap()[0].pieces);
    }

    #[test]
    fn suivi_et_importance_indexes() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        m.poser_messages(id, &[MessageLocal { uid: 1, importance: 1, ..Default::default() }]).unwrap();
        assert_eq!(m.messages(id).unwrap()[0].importance, 1);
        m.marquer_suivi(id, &[1], true).unwrap();
        assert!(m.messages(id).unwrap()[0].suivi);
        // Les drapeaux relus du serveur font foi.
        m.poser_drapeaux(id, &[(1, vec!["\\Seen".into()])]).unwrap();
        assert!(!m.messages(id).unwrap()[0].suivi);
        m.poser_drapeaux(id, &[(1, vec!["\\Flagged".into()])]).unwrap();
        assert!(m.messages(id).unwrap()[0].suivi);
    }

    #[test]
    fn contacts_proposes() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        let recu = |uid, nom: &str, adresse: &str| MessageLocal {
            uid,
            expediteur: nom.into(),
            adresse: adresse.into(),
            ..Default::default()
        };
        m.poser_messages(
            id,
            &[
                recu(1, "Hélène Martin", "helene@exemple.fr"),
                recu(2, "Hélène Martin", "HELENE@exemple.fr"),
                recu(3, "Noël Durand", "noel@exemple.fr"),
                recu(4, "Promo", "promo_50%@exemple.fr"),
            ],
        )
        .unwrap();
        m.noter_correspondants(&[("".into(), "ecrit@ailleurs.fr".into())], 10).unwrap();
        m.noter_correspondants(&[("Écrit Ailleurs".into(), "ecrit@ailleurs.fr".into())], 20).unwrap();
        m.noter_correspondants(&[("".into(), "ecrit@ailleurs.fr".into())], 30).unwrap();

        let tous = m.contacts("", 10).unwrap();
        assert_eq!(tous[0], ("Écrit Ailleurs".into(), "ecrit@ailleurs.fr".into()), "à qui l'on écrit d'abord, nom gardé");
        assert_eq!(tous.len(), 4, "une adresse, une ligne, quelle que soit la casse");
        assert_eq!(m.contacts("hél", 10).unwrap()[0].1.to_lowercase(), "helene@exemple.fr");
        assert_eq!(m.contacts("DURAND", 10).unwrap(), vec![("Noël Durand".into(), "noel@exemple.fr".into())]);
        assert_eq!(m.contacts("50%", 10).unwrap().len(), 1, "% cherché tel quel");
        assert!(m.contacts("zzz", 10).unwrap().is_empty());
    }

    #[test]
    fn expediteurs_comptes_une_fois_par_message() {
        let (m, _, id) = boite();
        let recu = |uid, nom: &str, adresse: &str| MessageLocal {
            uid,
            expediteur: nom.into(),
            adresse: adresse.into(),
            horodatage: uid as i64,
            ..Default::default()
        };
        let lot = [recu(1, "Rare", "rare@exemple.fr"), recu(2, "Fréquent", "f@exemple.fr"), recu(3, "Fréquent", "F@exemple.fr")];
        m.poser_messages(id, &lot).unwrap();
        // Relu (resynchronisation) : rien ne se compte deux fois.
        m.poser_messages(id, &lot).unwrap();
        m.poser_messages(id, &[recu(4, "Fréquent", "f@exemple.fr")]).unwrap();
        let nb: i64 = m.base.query_row("SELECT nb FROM expediteurs WHERE adresse = 'f@exemple.fr'", [], |l| l.get(0)).unwrap();
        assert_eq!(nb, 3);
        assert_eq!(m.contacts("exemple", 10).unwrap()[0], ("Fréquent".into(), "f@exemple.fr".into()));
    }

    #[test]
    fn ecriture_par_lots() {
        // Plus d'un lot : tout est écrit, et les mises à jour suivent.
        let (m, _, id) = boite();
        let lignes: Vec<MessageLocal> = (1..=1203).map(|uid| message(uid, uid as i64, false)).collect();
        m.poser_messages(id, &lignes).unwrap();
        assert_eq!(m.uids(id).unwrap().len(), 1203);
        let drapeaux: Vec<(u32, Vec<String>)> = (1..=1203).map(|uid| (uid, vec!["\\Seen".to_string()])).collect();
        m.poser_drapeaux(id, &drapeaux).unwrap();
        assert!(m.messages(id).unwrap().iter().all(|l| l.lu));
    }

    #[test]
    fn pieces_constatees_priment_sur_les_en_tetes() {
        let m = Magasin::en_memoire().unwrap();
        let compte = m.compte("a@b.fr", "h", 993, "a@b.fr").unwrap();
        m.poser_dossiers(compte, &[dossier("INBOX", &[])]).unwrap();
        let id = m.dossier_id(compte, "INBOX").unwrap();
        // Supposé d'après un multipart/mixed, démenti à la lecture.
        m.poser_messages(id, &[MessageLocal { uid: 1, pieces: true, ..Default::default() }]).unwrap();
        m.poser_pieces(id, 1, false).unwrap();
        // Une relecture des en-têtes ne rétablit pas la supposition.
        m.poser_messages(id, &[MessageLocal { uid: 1, pieces: true, ..Default::default() }]).unwrap();
        assert!(!m.messages(id).unwrap()[0].pieces);
    }
}
