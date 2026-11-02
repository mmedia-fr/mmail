// SPDX-License-Identifier: GPL-3.0-or-later
//
// Méthodes natives de fr.mmedia.mmail.Releve (packaging/android/src/) : la
// tâche de relève en arrière-plan appelle le noyau (core/src/releve.rs) par
// son interface C. Sous Android seulement (cf. CMakeLists.txt).
//
// Les chaînes passent en tableaux d'octets UTF-8 : le JNI ne parle, pour ses
// chaînes, que l'UTF-8 « modifié » de Java, où un émoji d'objet de message
// (hors du plan de base) s'écrit autrement qu'en UTF-8 — et NewStringUTF
// arrête le programme sur une séquence qu'il refuse.
//
// Rien de Qt ici : la tâche peut tourner sans que MMail ait démarré (Android
// relance alors le seul processus, sans activité), et Qt n'est pas initialisé.

#include <jni.h>

#include <string>

extern "C" {
char* mmail_releve_comptes(const char* index);
char* mmail_releve(const char* index, const char* secrets, const char* etat);
void mmail_releve_liberer(char* texte);
}

namespace {

std::string octets(JNIEnv* env, jbyteArray tableau)
{
  if (!tableau)
    return {};
  const jsize n = env->GetArrayLength(tableau);
  std::string texte(static_cast<size_t>(n), '\0');
  if (n > 0)
    env->GetByteArrayRegion(tableau, 0, n, reinterpret_cast<jbyte*>(texte.data()));
  return texte;
}

jbyteArray rendre(JNIEnv* env, char* texte)
{
  const std::string copie = texte ? texte : "";
  mmail_releve_liberer(texte);
  jbyteArray tableau = env->NewByteArray(static_cast<jsize>(copie.size()));
  if (tableau && !copie.empty())
    env->SetByteArrayRegion(tableau, 0, static_cast<jsize>(copie.size()),
                            reinterpret_cast<const jbyte*>(copie.data()));
  return tableau;
}

} // namespace

extern "C" JNIEXPORT jbyteArray JNICALL Java_fr_mmedia_mmail_Releve_comptes(JNIEnv* env, jclass, jbyteArray index)
{
  return rendre(env, mmail_releve_comptes(octets(env, index).c_str()));
}

extern "C" JNIEXPORT jbyteArray JNICALL Java_fr_mmedia_mmail_Releve_relever(JNIEnv* env, jclass, jbyteArray index,
                                                                           jbyteArray secrets, jbyteArray etat)
{
  const std::string i = octets(env, index), s = octets(env, secrets), e = octets(env, etat);
  return rendre(env, mmail_releve(i.c_str(), s.c_str(), e.c_str()));
}
