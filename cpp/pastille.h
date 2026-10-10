// SPDX-License-Identifier: GPL-3.0-or-later
//
// Pastille sur l'icône de MMail dans la barre des tâches : le nombre de
// messages non lus des boîtes de réception, tous comptes confondus (forme
// choisie par Manu le 10/10/2026 ; effacée à zéro).
//
// Windows : icône superposée au bouton de la barre des tâches
// (ITaskbarList3::SetOverlayIcon), que Qt 6 n'expose plus. Linux : compteur du
// lanceur (com.canonical.Unity.LauncherEntry, que suivent KDE Plasma et les
// docks à la Ubuntu), par D-Bus. Ailleurs, rien.
#pragma once

#include <QtCore/QObject>
#include <QtCore/QRectF>
#include <QtCore/QString>
#include <QtGui/QColor>
#include <QtGui/QFont>
#include <QtGui/QImage>
#include <QtGui/QPainter>
#include <QtGui/QWindow>

#ifdef Q_OS_WIN
#include <windows.h>
#include <shobjidl.h>

#include <string>
#endif

#if defined(Q_OS_LINUX) && !defined(Q_OS_ANDROID)
#include <QtCore/QVariantMap>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusMessage>
#endif

class Pastille : public QObject
{
  Q_OBJECT
public:
  explicit Pastille(QObject* parent = nullptr)
    : QObject(parent)
  {
  }

  ~Pastille() override
  {
#ifdef Q_OS_WIN
    if (m_barre)
      m_barre->Release();
    if (m_icone)
      DestroyIcon(m_icone);
    if (m_com)
      CoUninitialize();
#endif
  }

  /// Pose `nombre` sur l'icône de la fenêtre `fenetre` ; 0 l'efface.
  Q_INVOKABLE void poser(QObject* fenetre, int nombre)
  {
    QWindow* f = qobject_cast<QWindow*>(fenetre);
    if (!f || nombre < 0 || nombre == m_nombre)
      return;
    m_nombre = nombre;
#ifdef Q_OS_WIN
    poserWindows(reinterpret_cast<HWND>(f->winId()), nombre);
#elif defined(Q_OS_LINUX) && !defined(Q_OS_ANDROID)
    poserLanceur(nombre);
#endif
  }

  /// La pastille seule, en `cote` pixels : un disque rouge, le nombre en blanc
  /// (« 99+ » au-delà).
  static QImage image(int nombre, int cote)
  {
    QImage img(cote, cote, QImage::Format_ARGB32_Premultiplied);
    img.fill(Qt::transparent);
    QPainter p(&img);
    p.setRenderHint(QPainter::Antialiasing);
    p.setPen(Qt::NoPen);
    p.setBrush(QColor(0xc4, 0x2b, 0x1c));
    p.drawEllipse(QRectF(0.5, 0.5, cote - 1.0, cote - 1.0));
    const QString texte = nombre > 99 ? QStringLiteral("99+") : QString::number(nombre);
    QFont police;
    police.setBold(true);
    const double part = texte.size() >= 3 ? 0.40 : texte.size() == 2 ? 0.54 : 0.66;
    police.setPixelSize(qMax(6, int(cote * part)));
    p.setFont(police);
    p.setPen(Qt::white);
    p.drawText(QRectF(0, 0, cote, cote), Qt::AlignCenter, texte);
    return img;
  }

  /// L'image de la pastille dans un fichier : pour le scénario d'essai, faute
  /// de barre des tâches sans écran.
  Q_INVOKABLE bool enregistrer(int nombre, const QString& chemin) const { return image(nombre, 64).save(chemin); }

private:
  int m_nombre = -1;

#ifdef Q_OS_WIN
  ITaskbarList3* m_barre = nullptr;
  HICON m_icone = nullptr;
  bool m_com = false;

  void poserWindows(HWND fenetre, int nombre)
  {
    if (!m_barre) {
      m_com = SUCCEEDED(CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED));
      if (FAILED(CoCreateInstance(CLSID_TaskbarList, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&m_barre)))) {
        m_barre = nullptr;
        return;
      }
      if (FAILED(m_barre->HrInit())) {
        m_barre->Release();
        m_barre = nullptr;
        return;
      }
    }
    HICON ancienne = m_icone;
    m_icone = nombre > 0 ? image(nombre, 32).toHICON() : nullptr;
    const std::wstring description =
      std::to_wstring(nombre) + (nombre > 1 ? L" messages non lus" : L" message non lu");
    m_barre->SetOverlayIcon(fenetre, m_icone, nombre > 0 ? description.c_str() : nullptr);
    if (ancienne)
      DestroyIcon(ancienne);
  }
#endif

#if defined(Q_OS_LINUX) && !defined(Q_OS_ANDROID)
  static void poserLanceur(int nombre)
  {
    QDBusMessage signal = QDBusMessage::createSignal(QStringLiteral("/com/canonical/unity/launcherentry/mmail"),
                                                     QStringLiteral("com.canonical.Unity.LauncherEntry"),
                                                     QStringLiteral("Update"));
    QVariantMap proprietes;
    proprietes.insert(QStringLiteral("count"), qint64(nombre));
    proprietes.insert(QStringLiteral("count-visible"), nombre > 0);
    signal << QStringLiteral("application://mmail.desktop") << proprietes;
    QDBusConnection::sessionBus().send(signal);
  }
#endif
};
