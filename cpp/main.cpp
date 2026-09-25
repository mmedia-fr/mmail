// SPDX-License-Identifier: GPL-3.0-or-later
#include <QtCore/QCoreApplication>
#include <QtCore/QDir>
#include <QtCore/QFileInfo>
#include <QtCore/QStandardPaths>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QTimer>
#include <QtCore/QUrl>
#include <QtCore/QVariant>
#include <QtCore/QVariantMap>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtQml/QQmlApplicationEngine>
#include <QtQml/QQmlContext>
#include <QtQuickControls2/QQuickStyle>
#include <QtQuick/QQuickWindow>
#include <QtQml/qqml.h>

#include "coffre.h"
#include "presse_papier.h"

#ifdef Q_OS_WIN
#  include <windows.h>  // GetCommandLineW / CommandLineToArgvW
#  include <shellapi.h>
#endif

#include <cstdio>

/// Arguments du programme, en Unicode et sans perte.
///
/// Sous Windows, `argv` est encodé dans la page de code ANSI : la seule source
/// fiable est la ligne de commande native. Leçon prise sur MMdedit.
static QStringList argumentsUnicode()
{
#ifdef Q_OS_WIN
  int nombre = 0;
  LPWSTR* natifs = CommandLineToArgvW(GetCommandLineW(), &nombre);
  if (!natifs)
    return QCoreApplication::arguments();
  QStringList liste;
  liste.reserve(nombre);
  for (int i = 0; i < nombre; ++i)
    liste.append(QString::fromWCharArray(natifs[i]));
  LocalFree(natifs);
  return liste;
#else
  return QCoreApplication::arguments();
#endif
}

/// Dossier du profil, résolu par le système et jamais écrit en dur.
///
/// Décision 18 du dossier de projet : le profil vit sur un disque local, SQLite
/// en WAL se comportant mal sur un partage réseau.
static QString dossierProfil()
{
  const QString base = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
  QDir().mkpath(base);
  return base;
}

int main(int argc, char* argv[])
{
  QGuiApplication app(argc, argv);
  QCoreApplication::setApplicationName(QStringLiteral("MMail"));
  QCoreApplication::setOrganizationName(QStringLiteral("M-Media"));
  QCoreApplication::setOrganizationDomain(QStringLiteral("mmedia.fr"));
  QCoreApplication::setApplicationVersion(QStringLiteral(MMAIL_VERSION));
  QGuiApplication::setWindowIcon(QIcon(QStringLiteral(":/assets/mmail.ico")));

  // Fusion : le seul style qui honore une palette sur les trois cibles livrées.
  QQuickStyle::setStyle(QStringLiteral("Fusion"));

  bool smoke = false;
  QString capture;
  int delaiCapture = 600;
  const QStringList arguments = argumentsUnicode();
  for (int i = 1; i < arguments.size(); ++i) {
    const QString argument = arguments.at(i);
    if (argument == QStringLiteral("--smoke"))
      smoke = true;
    else if (argument == QStringLiteral("--capture") && i + 1 < arguments.size())
      capture = arguments.at(++i);
    // Délai avant la capture, en millisecondes : le temps qu'une session
    // d'essai se connecte et charge un dossier.
    else if (argument == QStringLiteral("--delai") && i + 1 < arguments.size())
      delaiCapture = arguments.at(++i).toInt();
  }

  // Coffre des mots de passe, natif : exposé à QML comme un type à part, hors du
  // module Rust.
  qmlRegisterType<Coffre>("fr.mmedia.mmail.natif", 1, 0, "Coffre");
  // Presse-papier du système, que QML ne sait pas lire : la copie automatique
  // de la sélection en a besoin pour ne pas écraser ce qu'un tiers y a déposé.
  qmlRegisterType<PressePapier>("fr.mmedia.mmail.natif", 1, 0, "PressePapier");

  QQmlApplicationEngine engine;
  // En contrôle de fabrication, l'interface ne touche ni au coffre ni au réseau
  // de sa propre initiative.
  engine.rootContext()->setContextProperty(QStringLiteral("modeControle"), smoke);
  // Commodité de développement : des variables d'environnement ouvrent une ou
  // deux sessions au démarrage, ce qui permet de saisir l'interface en image
  // sans personne devant l'écran. Rien n'est lu si elles sont absentes.
  const QByteArray hoteEssai = qgetenv("MMAIL_HOTE");
  QVariantMap essai;
  if (!hoteEssai.isEmpty()) {
    essai.insert(QStringLiteral("hote"), QString::fromUtf8(hoteEssai));
    essai.insert(QStringLiteral("utilisateur"), QString::fromUtf8(qgetenv("MMAIL_UTILISATEUR")));
    essai.insert(QStringLiteral("motDePasse"), QString::fromUtf8(qgetenv("MMAIL_MOTDEPASSE")));
    essai.insert(QStringLiteral("utilisateur2"), QString::fromUtf8(qgetenv("MMAIL_UTILISATEUR2")));
    essai.insert(QStringLiteral("motDePasse2"), QString::fromUtf8(qgetenv("MMAIL_MOTDEPASSE2")));
    essai.insert(QStringLiteral("scenario"), QString::fromUtf8(qgetenv("MMAIL_SCENARIO")));
    essai.insert(QStringLiteral("sortie"),
                 QUrl::fromLocalFile(QString::fromUtf8(qgetenv("MMAIL_SORTIE"))).toString());
  }
  engine.rootContext()->setContextProperty(QStringLiteral("identifiantsEssai"), essai);

  // Dossier proposé à l'enregistrement d'une pièce jointe : celui des
  // téléchargements de l'utilisateur, résolu par le système.
  const QString telechargements = QStandardPaths::writableLocation(QStandardPaths::DownloadLocation);
  engine.rootContext()->setContextProperty(QStringLiteral("dossierTelechargements"),
                                           QUrl::fromLocalFile(telechargements).toString());

  // MMAIL_PROFIL : un profil jetable pour les essais, qui ne touche pas celui
  // de l'utilisateur.
  const QByteArray profilEssai = qgetenv("MMAIL_PROFIL");
  const QString profil = profilEssai.isEmpty()
    ? QDir(dossierProfil()).filePath(QStringLiteral("index.sqlite"))
    : QString::fromUtf8(profilEssai);
  engine.rootContext()->setContextProperty(QStringLiteral("cheminProfil"), profil);

  const QUrl url(QStringLiteral("qrc:/qt/qml/fr/mmedia/mmail/qml/Main.qml"));
  QObject::connect(
    &engine, &QQmlApplicationEngine::objectCreated, &app,
    [url](QObject* obj, const QUrl& objUrl) {
      if (!obj && url == objUrl)
        QCoreApplication::exit(1);
    },
    Qt::QueuedConnection);
  engine.load(url);

  if (smoke) {
    // Contrôle de fabrication : la fenêtre existe, le noyau Rust répond, et
    // l'index local s'ouvre et se relit. Rien de tout cela ne touche au réseau.
    QTimer::singleShot(0, &app, [&engine, profil] {
      if (engine.rootObjects().isEmpty()) {
        std::fprintf(stderr, "smoke: aucune fenetre creee\n");
        QCoreApplication::exit(1);
        return;
      }
      QObject* racine = engine.rootObjects().first();
      const QString noyau = racine->property("noyau").toString();
      QVariant controle;
      const bool appel =
        QMetaObject::invokeMethod(racine, "controleIndex", Q_RETURN_ARG(QVariant, controle));
      std::printf("smoke: %s | profil %s | index %s\n", noyau.toUtf8().constData(),
                  QFileInfo::exists(profil) ? "ecrit" : "absent",
                  controle.toString().toUtf8().constData());
      std::fflush(stdout);
      if (!noyau.startsWith(QStringLiteral("mmail_core"))) {
        QCoreApplication::exit(2);
        return;
      }
      QCoreApplication::exit(appel && controle.toString() == QStringLiteral("ok") ? 0 : 3);
    });
  }

  if (!capture.isEmpty()) {
    QTimer::singleShot(delaiCapture, &app, [&engine, capture] {
      if (engine.rootObjects().isEmpty()) {
        QCoreApplication::exit(1);
        return;
      }
      auto* fenetre = qobject_cast<QQuickWindow*>(engine.rootObjects().first());
      if (!fenetre) {
        QCoreApplication::exit(5);
        return;
      }
      QCoreApplication::exit(fenetre->grabWindow().save(capture) ? 0 : 6);
    });
  }
  return app.exec();
}
