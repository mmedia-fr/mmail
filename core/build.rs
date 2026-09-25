// SPDX-License-Identifier: GPL-3.0-or-later
use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    // Le rcc de l'hôte compresse en zstd, absent du Qt officiel pour Android :
    // sans cette option, l'édition de liens de l'APK échoue sur
    // « undefined symbol: qt_resourceFeatureZstd ». Leçon du portage de MMdedit.
    std::env::set_var("CXX_QT_AUTORCC_OPTIONS", "--no-zstd");

    CxxQtBuilder::new_qml_module(QmlModule::new("fr.mmedia.mmail").qml_files(["qml/Main.qml"]))
        .qt_module("Qml")
        .files(["src/socle.rs", "src/boite.rs", "src/pont_presse_papier.rs"])
        .build();
}
