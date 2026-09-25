// SPDX-License-Identifier: GPL-3.0-or-later
//! Objet témoin de la liaison Rust <-> Qt, et version du noyau.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, noyau)]
        #[qproperty(QString, version)]
        type Socle = super::SocleRust;
    }
}

use cxx_qt_lib::QString;

pub struct SocleRust {
    noyau: QString,
    version: QString,
}

impl Default for SocleRust {
    fn default() -> Self {
        Self {
            noyau: QString::from(&format!("mmail_core {}", env!("CARGO_PKG_VERSION"))),
            version: QString::from(env!("CARGO_PKG_VERSION")),
        }
    }
}
