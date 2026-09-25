#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Installe l'APK sur l'émulateur courant, le lance, et vérifie qu'il tient debout.
#
# Ce contrôle vit dans un fichier, et non dans le workflow : l'action
# android-emulator-runner exécute son « script » **ligne par ligne, chacune dans
# un shell distinct** — une variable posée à la première ligne est perdue à la
# suivante, ce qui fait échouer en silence toute séquence écrite naïvement.
set -euo pipefail

PAQUET=fr.mmedia.mmail
ATTENTE=20

apk=$(find _build/android-build -name '*.apk' | head -n 1)
if [ -z "$apk" ]; then
  echo "aucun APK construit sous _build/android-build" >&2
  exit 1
fi
echo "APK : $apk"

# La fabrication rend un APK non signé, et Android refuse de l'installer
# (INSTALL_PARSE_FAILED_NO_CERTIFICATES). On le signe ici avec une clé créée pour
# ce seul essai : elle vit le temps du travail, ne sert qu'à l'émulateur, et n'a
# rien à voir avec la clé de test conservée pour les APK publiés.
if ! unzip -l "$apk" | grep -qE 'META-INF/.*\.(RSA|EC|DSA)'; then
  echo "APK non signé : signature avec une clé d'essai jetable"
  cle=$(mktemp -u /tmp/essai-XXXXXX.keystore)
  motdepasse=$(head -c 18 /dev/urandom | base64 | tr -d '/+=')
  keytool -genkeypair -keystore "$cle" -storepass "$motdepasse" -keypass "$motdepasse" \
    -alias essai -keyalg RSA -keysize 2048 -validity 30 \
    -dname "CN=MMail essai, O=M-Media, C=FR" > /dev/null
  outils=$(ls -d "$ANDROID_SDK_ROOT"/build-tools/* | sort -V | tail -n 1)
  aligne=$(mktemp -u /tmp/mmail-XXXXXX.apk)
  "$outils/zipalign" -f -p 4 "$apk" "$aligne"
  "$outils/apksigner" sign --ks "$cle" --ks-pass "pass:$motdepasse" \
    --key-pass "pass:$motdepasse" --ks-key-alias essai "$aligne"
  apk="$aligne"
fi

adb install -r "$apk"
adb logcat -c
adb shell monkey -p "$PAQUET" -c android.intent.category.LAUNCHER 1
sleep "$ATTENTE"

pid=$(adb shell pidof "$PAQUET" || true)
adb logcat -d -t 500 > logcat.txt

if [ -z "$pid" ]; then
  echo "MMail ne tourne plus $ATTENTE s apres son lancement :" >&2
  grep -iE 'mmail|AndroidRuntime|FATAL' logcat.txt | tail -40 >&2
  exit 1
fi

echo "MMail tourne (pid $pid)"
