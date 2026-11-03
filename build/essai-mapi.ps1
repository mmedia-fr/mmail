# Essai du fournisseur MAPI de MMail (cf. mapi/mmail_mapi.c), comme le ferait un
# logiciel : appel de MAPISendMail ou MAPISendMailW dans mapi32.dll, le stub de
# Windows, qui transmet a la messagerie par defaut. L'architecture est celle du
# PowerShell qui execute ce script : pwsh ou powershell.exe de System32 pour
# 64 bits, C:\Windows\SysWOW64\WindowsPowerShell\v1.0\powershell.exe pour 32 bits.
# Code de sortie : le code MAPI (0 = succes). Sert a la CI (socle.yml) et au banc.
param(
  [ValidateSet('A', 'W')] [string] $Jeu = 'W',
  [string] $Objet = 'Essai MAPI',
  [string] $Texte = '',
  [string] $A = '',
  [string] $Nom = '',
  [string] $Piece = '',
  [int] $Options = 0
)
$code = @"
using System;
using System.Runtime.InteropServices;
public static class EssaiMapi {
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Ansi)]
  public struct RecipA { public uint reserve; public uint classe; public string nom; public string adresse; public uint taille; public IntPtr id; }
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Ansi)]
  public struct FichierA { public uint reserve; public uint options; public uint position; public string chemin; public string nom; public IntPtr type; }
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Ansi)]
  public struct MessageA { public uint reserve; public string objet; public string texte; public string type; public string date; public string conversation; public uint options; public IntPtr origine; public uint nbDest; public IntPtr dest; public uint nbFichiers; public IntPtr fichiers; }
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  public struct RecipW { public uint reserve; public uint classe; public string nom; public string adresse; public uint taille; public IntPtr id; }
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  public struct FichierW { public uint reserve; public uint options; public uint position; public string chemin; public string nom; public IntPtr type; }
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  public struct MessageW { public uint reserve; public string objet; public string texte; public string type; public string date; public string conversation; public uint options; public IntPtr origine; public uint nbDest; public IntPtr dest; public uint nbFichiers; public IntPtr fichiers; }

  [DllImport("mapi32.dll", EntryPoint = "MAPISendMail", CharSet = CharSet.Ansi)]
  static extern uint EnvoyerA(IntPtr session, IntPtr fenetre, ref MessageA m, uint options, uint reserve);
  [DllImport("mapi32.dll", EntryPoint = "MAPISendMailW", CharSet = CharSet.Unicode)]
  static extern uint EnvoyerW(IntPtr session, IntPtr fenetre, ref MessageW m, uint options, uint reserve);

  static IntPtr Bloc<T>(T s) {
    IntPtr p = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(T)));
    Marshal.StructureToPtr(s, p, false);
    return p;
  }

  public static uint Envoyer(bool unicode, string objet, string texte, string a, string nom, string piece, uint options) {
    string affiche = nom.Length > 0 ? nom : a;
    string fichier = System.IO.Path.GetFileName(piece);
    if (unicode) {
      MessageW m = new MessageW { objet = objet, texte = texte };
      if (a.Length > 0) { m.nbDest = 1; m.dest = Bloc(new RecipW { classe = 1, nom = affiche, adresse = "SMTP:" + a }); }
      if (piece.Length > 0) { m.nbFichiers = 1; m.fichiers = Bloc(new FichierW { position = 0xFFFFFFFF, chemin = piece, nom = fichier }); }
      return EnvoyerW(IntPtr.Zero, IntPtr.Zero, ref m, options, 0);
    }
    MessageA n = new MessageA { objet = objet, texte = texte };
    if (a.Length > 0) { n.nbDest = 1; n.dest = Bloc(new RecipA { classe = 1, nom = affiche, adresse = "SMTP:" + a }); }
    if (piece.Length > 0) { n.nbFichiers = 1; n.fichiers = Bloc(new FichierA { position = 0xFFFFFFFF, chemin = piece, nom = fichier }); }
    return EnvoyerA(IntPtr.Zero, IntPtr.Zero, ref n, options, 0);
  }
}
"@
Add-Type -TypeDefinition $code
$r = [EssaiMapi]::Envoyer($Jeu -eq 'W', $Objet, $Texte, $A, $Nom, $Piece, [uint32]$Options)
$fonction = if ($Jeu -eq 'W') { 'MAPISendMailW' } else { 'MAPISendMail' }
Write-Host "$fonction ($([IntPtr]::Size * 8) bits) : $r"
exit [int]$r
