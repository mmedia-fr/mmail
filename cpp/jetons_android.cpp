// SPDX-License-Identifier: GPL-3.0-or-later
//
// Comptes Microsoft et Google connectés par Android (0.7.0) : le pont entre
// le noyau et fr.mmedia.mmail.Jetons (packaging/android/src/). Sous Android
// seulement (cf. CMakeLists.txt).
//
//  - mmail_jeton_android : le noyau (core/src/android.rs) demande un jeton
//    d'accès, depuis le fil d'une session ou de la relève. Appel bloquant de
//    Jetons.acces, par la machine virtuelle et la classe que Jetons.inscrire
//    a données : un fil natif ne trouverait pas, par FindClass, les classes
//    de l'application. Sans QJniObject non plus : dans le processus de la
//    relève, Qt n'a ni activité ni chargeur de classes.
//  - Jetons.termine : issue d'une connexion interactive, remise à
//    l'interface (cpp/connexion_android.h).
//
// Chaînes en tableaux d'octets UTF-8, comme pour la relève
// (cpp/releve_android.cpp).

#include <jni.h>

#include <cstdlib>
#include <cstring>
#include <mutex>
#include <string>

#include "connexion_android.h"

namespace {

std::mutex verrou;
JavaVM* machine = nullptr;
jclass classeJetons = nullptr; // référence globale

char* copie(const std::string& texte)
{
  char* p = static_cast<char*>(std::malloc(texte.size() + 1));
  if (p)
    std::memcpy(p, texte.c_str(), texte.size() + 1);
  return p;
}

jbyteArray tableau(JNIEnv* env, const char* texte)
{
  const jsize n = static_cast<jsize>(std::strlen(texte));
  jbyteArray t = env->NewByteArray(n);
  if (t && n > 0)
    env->SetByteArrayRegion(t, 0, n, reinterpret_cast<const jbyte*>(texte));
  return t;
}

std::string octets(JNIEnv* env, jbyteArray t)
{
  if (!t)
    return {};
  const jsize n = env->GetArrayLength(t);
  std::string texte(static_cast<size_t>(n), '\0');
  if (n > 0)
    env->GetByteArrayRegion(t, 0, n, reinterpret_cast<jbyte*>(texte.data()));
  return texte;
}

const char* const INDISPONIBLE = R"({"erreur": "jetons d'Android indisponibles", "reseau": true})";

} // namespace

extern "C" JNIEXPORT void JNICALL Java_fr_mmedia_mmail_Jetons_inscrire(JNIEnv* env, jclass classe)
{
  std::lock_guard<std::mutex> v(verrou);
  if (classeJetons)
    return;
  env->GetJavaVM(&machine);
  classeJetons = static_cast<jclass>(env->NewGlobalRef(classe));
}

extern "C" JNIEXPORT void JNICALL Java_fr_mmedia_mmail_Jetons_termine(JNIEnv* env, jclass, jbyteArray issue)
{
  ConnexionAndroid::remettre(QString::fromUtf8(octets(env, issue)));
}

extern "C" char* mmail_jeton_android(const char* service, const char* compte, const char* portee)
{
  JavaVM* vm;
  jclass classe;
  {
    std::lock_guard<std::mutex> v(verrou);
    vm = machine;
    classe = classeJetons;
  }
  if (!vm || !classe)
    return copie(INDISPONIBLE);
  JNIEnv* env = nullptr;
  bool attache = false;
  if (vm->GetEnv(reinterpret_cast<void**>(&env), JNI_VERSION_1_6) == JNI_EDETACHED) {
    if (vm->AttachCurrentThread(&env, nullptr) != JNI_OK)
      return copie(INDISPONIBLE);
    attache = true;
  }
  std::string reponse;
  const jmethodID acces = env->GetStaticMethodID(classe, "acces", "([B[B[B)[B");
  if (acces) {
    jbyteArray s = tableau(env, service), c = tableau(env, compte), p = tableau(env, portee);
    auto r = static_cast<jbyteArray>(env->CallStaticObjectMethod(classe, acces, s, c, p));
    if (env->ExceptionCheck())
      env->ExceptionClear();
    else
      reponse = octets(env, r);
    env->DeleteLocalRef(s);
    env->DeleteLocalRef(c);
    env->DeleteLocalRef(p);
    if (r)
      env->DeleteLocalRef(r);
  } else if (env->ExceptionCheck()) {
    env->ExceptionClear();
  }
  if (attache)
    vm->DetachCurrentThread();
  return copie(reponse.empty() ? std::string(INDISPONIBLE) : reponse);
}

extern "C" void mmail_jeton_android_liberer(char* texte)
{
  std::free(texte);
}
