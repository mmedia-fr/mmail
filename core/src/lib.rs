// SPDX-License-Identifier: GPL-3.0-or-later
//! Noyau de MMail : protocole IMAP, index local, et les objets exposés à QML.
pub mod boite;
pub mod deplacement;
pub mod imap;
pub mod index;
pub mod magasin;
pub mod pont_presse_papier;
pub mod presse_papier;
pub mod socle;
pub mod protocole;
pub mod synchro;

// Épreuves contre un vrai serveur : ignorées par défaut, elles exigent un
// compte fourni par l'environnement (cf. le module).
#[cfg(test)]
mod essais_serveur;
