# SPDX-License-Identifier: GPL-3.0-or-later
"""Cales de Windows Server 2016 (0.6.39).

Qt 6.8 vise Windows 10 1809 et au-delà. Sur Windows Server 2016 (Windows 10
1607, build 14393), MMail ne démarrait pas : « point d'entrée introuvable »
(0xC0000139). Relevé du 02/11/2026 sur un Server 2016, toutes les DLL du paquet
lues : il ne manque que deux fonctions.

- Qt6Core.dll importe SetThreadDescription de KERNEL32 ; en 1607, elle n'est
  exportée que par KERNELBASE (la redirection de KERNEL32 vient avec 1703).
- platforms/qwindows.dll importe UiaRaiseNotificationEvent de
  UIAutomationCore (Windows 10 1709).

Plutôt qu'une version à part (Qt 5), deux petites DLL de relais, posées à côté
de MMail.exe :

- MMKRNL32.dll ne contient que des renvois : chaque fonction de KERNEL32
  qu'importe Qt6Core.dll y renvoie à KERNEL32, et SetThreadDescription à
  KERNELBASE, qui l'a partout depuis 1607 ;
- MMUIACORE.dll renvoie de même à UIAutomationCore, et porte
  UiaRaiseNotificationEvent : la vraie si le système l'a, rien sinon (une
  annonce d'accessibilité en moins, sur Server 2016 seulement).

Dans Qt6Core.dll et qwindows.dll, seul change le nom de la DLL importée
(« KERNEL32.dll » → « MMKRNL32.dll », « UIAutomationCore.DLL » →
« MMUIACORE.DLL »), écrit en place : même longueur ou plus court, rien ne
bouge dans le fichier. Le comportement est le même sur tous les Windows ; un
seul paquet.

Usage (CI Windows, après windeployqt, environnement MSVC chargé) :
    python build/cales-server2016.py dist/MMail
"""
import os
import struct
import subprocess
import sys
import tempfile

CALES = [
    # (fichier, DLL importée, nom de la cale, renvois particuliers, fonctions écrites ici)
    ("Qt6Core.dll", "KERNEL32.dll", "MMKRNL32", {"SetThreadDescription": "KERNELBASE.SetThreadDescription"}, []),
    (os.path.join("platforms", "qwindows.dll"), "UIAutomationCore.dll", "MMUIACORE", {}, ["UiaRaiseNotificationEvent"]),
]

SOURCE_UIA = r"""
/* Cale de MMail (0.6.39) : UiaRaiseNotificationEvent, absente de Windows 10
 * avant 1709 (Server 2016). La vraie si le systeme l'a, sinon rien. */
#include <windows.h>
typedef HRESULT(WINAPI *Notification)(void *, int, int, BSTR, BSTR);
static Notification vraie;
static LONG lue;
HRESULT WINAPI mmail_UiaRaiseNotificationEvent(void *fournisseur, int genre, int traitement, BSTR texte, BSTR activite)
{
    if (!InterlockedCompareExchange(&lue, 1, 0)) {
        HMODULE m = LoadLibraryW(L"UIAutomationCore.dll");
        vraie = m ? (Notification)GetProcAddress(m, "UiaRaiseNotificationEvent") : NULL;
    }
    return vraie ? vraie(fournisseur, genre, traitement, texte, activite) : S_OK;
}
"""


class Pe:
    """Lecture minimale d'un PE32+ : sections et descripteurs d'imports."""

    def __init__(self, chemin):
        self.chemin = chemin
        with open(chemin, "rb") as f:
            self.b = bytearray(f.read())
        pe = self.u32(0x3C)
        if self.b[pe:pe + 4] != b"PE\0\0":
            raise ValueError(f"{chemin} : pas un PE")
        self.nb_sections = self.u16(pe + 6)
        taille_opt = self.u16(pe + 20)
        self.opt = pe + 24
        if self.u16(self.opt) != 0x20B:
            raise ValueError(f"{chemin} : pas un PE32+")
        self.repertoires = self.opt + 112
        self.sections = self.opt + taille_opt

    def u16(self, o):
        return struct.unpack_from("<H", self.b, o)[0]

    def u32(self, o):
        return struct.unpack_from("<I", self.b, o)[0]

    def u64(self, o):
        return struct.unpack_from("<Q", self.b, o)[0]

    def fichier(self, rva):
        for i in range(self.nb_sections):
            s = self.sections + i * 40
            va, taille, brut = self.u32(s + 12), max(self.u32(s + 8), self.u32(s + 16)), self.u32(s + 20)
            if va <= rva < va + taille:
                return rva - va + brut
        raise ValueError(f"RVA {rva:#x} hors des sections")

    def chaine(self, o):
        fin = self.b.index(0, o)
        return self.b[o:fin].decode("ascii")

    def imports(self):
        """[(nom de la DLL, position du nom dans le fichier, [fonctions])]"""
        rva = self.u32(self.repertoires + 8)
        if rva == 0:
            return []
        res = []
        d = self.fichier(rva)
        while self.u32(d + 12) != 0:
            pos_nom = self.fichier(self.u32(d + 12))
            thunks = self.u32(d) or self.u32(d + 16)
            fonctions = []
            t = self.fichier(thunks)
            while True:
                v = self.u64(t)
                if v == 0:
                    break
                if v & (1 << 63):
                    # Import par numéro (ws2_32, oleaut32…) : sans nom à renvoyer.
                    fonctions.append(f"#{v & 0xFFFF}")
                else:
                    fonctions.append(self.chaine(self.fichier(v & 0xFFFFFFFF) + 2))
                t += 8
            res.append((self.chaine(pos_nom), pos_nom, fonctions))
            d += 20
        return res

    def renommer_import(self, ancien, nouveau):
        trouve = [(n, p, f) for (n, p, f) in self.imports() if n.lower() == ancien.lower()]
        if len(trouve) != 1:
            raise ValueError(f"{self.chemin} : {len(trouve)} imports de {ancien}")
        nom, pos, fonctions = trouve[0]
        if any(f.startswith("#") for f in fonctions):
            raise ValueError(f"{self.chemin} : import par numéro depuis {ancien}, non pris en charge")
        neuf = nouveau.encode("ascii")
        if len(neuf) > len(nom):
            raise ValueError(f"{nouveau} plus long que {nom}")
        self.b[pos:pos + len(nom)] = neuf + b"\0" * (len(nom) - len(neuf))
        # Imports liés (bound imports) : leurs dates ne valent plus, le chargeur
        # les relit sans eux.
        self.b[self.repertoires + 11 * 8:self.repertoires + 12 * 8] = b"\0" * 8
        return fonctions

    def ecrire(self):
        with open(self.chemin, "wb") as f:
            f.write(self.b)


def outil(nom):
    """Outil de MSVC par son chemin : sous Git Bash, « link » est celui de
    coreutils, qui passe avant celui de MSVC dans le PATH."""
    racine = os.environ.get("VCToolsInstallDir")
    if racine:
        chemin = os.path.join(racine, "bin", "Hostx64", "x64", nom + ".exe")
        if os.path.isfile(chemin):
            return chemin
    return nom


def construire(dossier, nom_cale, dll, fonctions, renvois, ecrites, travail):
    module = os.path.splitext(dll)[0]
    # Exportations en options de l'éditeur de liens (/EXPORT), dans un fichier
    # de réponse, et la bibliothèque d'import du module visé : sans elle,
    # l'éditeur de liens de MSVC tient « nom=module.fonction » pour un symbole
    # à résoudre (LNK2001, vu pour UIAutomationCore).
    options = []
    for fn in sorted(set(fonctions)):
        cible = f"mmail_{fn}" if fn in ecrites else renvois.get(fn, module + "." + fn)
        options.append(f"/EXPORT:{fn}={cible}")
    reponse = os.path.join(travail, nom_cale + ".rsp")
    with open(reponse, "w", encoding="ascii") as f:
        f.write("\n".join(options) + "\n")
    source = os.path.join(travail, nom_cale + ".c")
    with open(source, "w", encoding="ascii") as f:
        f.write(SOURCE_UIA if ecrites else "int mmail_cale_vide;\n")
    objet = os.path.join(travail, nom_cale + ".obj")
    subprocess.run([outil("cl"), "/nologo", "/c", "/O2", "/GS-", "/Fo" + objet, source], check=True)
    sortie = os.path.join(dossier, nom_cale + ".dll")
    subprocess.run([outil("link"), "/nologo", "/DLL", "/NOENTRY", "/NODEFAULTLIB", "/MACHINE:X64",
                    "/OUT:" + sortie, "/IMPLIB:" + os.path.join(travail, nom_cale + ".lib"),
                    "@" + reponse, objet, "kernel32.lib", module.lower() + ".lib"], check=True)
    return sortie


def main():
    dossier = sys.argv[1]
    with tempfile.TemporaryDirectory() as travail:
        for fichier, dll, nom_cale, renvois, ecrites in CALES:
            pe = Pe(os.path.join(dossier, fichier))
            fonctions = pe.renommer_import(dll, nom_cale + ".DLL")
            for fn in list(renvois) + ecrites:
                if fn not in fonctions:
                    raise SystemExit(f"{fichier} n'importe plus {fn} de {dll} : cale à revoir")
            construire(dossier, nom_cale, dll, fonctions, renvois, ecrites, travail)
            pe.ecrire()
            print(f"{fichier} : {dll} → {nom_cale}.DLL ({len(fonctions)} fonctions)")
    # Contrôle : plus aucun import des deux fonctions depuis leur DLL d'origine.
    for fichier, dll, nom_cale, renvois, ecrites in CALES:
        for nom, _, fonctions in Pe(os.path.join(dossier, fichier)).imports():
            if nom.lower() == dll.lower():
                raise SystemExit(f"{fichier} importe encore {dll}")
        print(f"{fichier} : contrôlé")


if __name__ == "__main__":
    main()
