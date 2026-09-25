// SPDX-License-Identifier: GPL-3.0-or-later
//! Passerelle QML vers la règle de copie automatique (`crate::presse_papier`).
//!
//! L'interface fournit l'heure (`Date.now() / 1000`) et rapporte ce qui arrive
//! au presse-papier ; la règle, elle, ne décide qu'à partir de ce qu'on lui dit.

use crate::presse_papier::{Origine, Regle};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        /// Libellé de l'origine du contenu courant : « extérieur », « message »,
        /// « source », ou un tiret.
        #[qproperty(QString, libelle)]
        /// Secondes de protection restantes, 0 s'il n'y en a plus.
        #[qproperty(i32, restant)]
        type ReglePressePapier = super::ReglePressePapierRust;

        /// La copie automatique peut-elle écrire, ou faut-il laisser en place ?
        #[qinvokable]
        #[cxx_name = "peutEcraser"]
        fn peut_ecraser(&self, maintenant: f64) -> bool;

        /// Le presse-papier a changé : rend l'origine retenue.
        #[qinvokable]
        #[cxx_name = "depot"]
        fn depot(self: Pin<&mut ReglePressePapier>, texte: &QString, maintenant: f64) -> QString;

        /// Nous venons d'y écrire nous-mêmes.
        #[qinvokable]
        #[cxx_name = "ecriture"]
        fn ecriture(
            self: Pin<&mut ReglePressePapier>,
            texte: &QString,
            origine: &QString,
            maintenant: f64,
        );

        /// Un Ctrl+C explicite va déposer : dire d'où, avant que Qt ne le signale.
        #[qinvokable]
        #[cxx_name = "annoncer"]
        fn annoncer(self: Pin<&mut ReglePressePapier>, origine: &QString, maintenant: f64);

        /// Rafraîchit le compte à rebours affiché en barre d'état.
        #[qinvokable]
        #[cxx_name = "rafraichir"]
        fn rafraichir(self: Pin<&mut ReglePressePapier>, maintenant: f64);
    }
}

use core::pin::Pin;
// rust_mut() vient de ce trait : sans lui, les champs du struct Rust ne sont pas
// accessibles en écriture depuis les méthodes du QObject.
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

pub struct ReglePressePapierRust {
    libelle: QString,
    restant: i32,
    regle: Regle,
}

impl Default for ReglePressePapierRust {
    fn default() -> Self {
        Self {
            libelle: QString::from(Origine::Inconnue.libelle()),
            restant: 0,
            regle: Regle::default(),
        }
    }
}

impl qobject::ReglePressePapier {
    pub fn peut_ecraser(&self, maintenant: f64) -> bool {
        self.regle.peut_ecraser(maintenant)
    }

    pub fn depot(mut self: Pin<&mut Self>, texte: &QString, maintenant: f64) -> QString {
        let origine = self
            .as_mut()
            .rust_mut()
            .regle
            .depot(&texte.to_string(), maintenant);
        self.as_mut().publier(maintenant);
        QString::from(origine.cle())
    }

    pub fn ecriture(
        mut self: Pin<&mut Self>,
        texte: &QString,
        origine: &QString,
        maintenant: f64,
    ) {
        let origine = Origine::depuis_cle(&origine.to_string());
        self.as_mut().rust_mut().regle.ecriture_interne(
            &texte.to_string(),
            origine,
            maintenant,
        );
        self.as_mut().publier(maintenant);
    }

    pub fn annoncer(mut self: Pin<&mut Self>, origine: &QString, maintenant: f64) {
        let origine = Origine::depuis_cle(&origine.to_string());
        self.as_mut().rust_mut().regle.annoncer(origine, maintenant);
    }

    pub fn rafraichir(mut self: Pin<&mut Self>, maintenant: f64) {
        self.as_mut().publier(maintenant);
    }

    /// Recopie l'état de la règle dans les propriétés que lit l'interface.
    fn publier(mut self: Pin<&mut Self>, maintenant: f64) {
        let libelle = self.regle.origine().libelle();
        let restant = self.regle.restant(maintenant);
        self.as_mut().set_libelle(QString::from(libelle));
        self.as_mut().set_restant(restant);
    }
}
