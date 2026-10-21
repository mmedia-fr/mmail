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

# L'émulateur de GitHub, lent, se dit démarré avant que ses services le soient :
# l'installation échouait parfois sur « Can't find service: package », et le
# lancement par monkey s'arrêtait sur l'ANR d'une application du système
# (com.android.phone), sans que MMail y soit pour rien. On attend le
# gestionnaire de paquets, on réessaie l'installation, on lance par « am start »
# et on relance une fois si MMail n'a pas démarré — mais un plantage de MMail
# (FATAL EXCEPTION à son nom) échoue tout de suite.
for i in $(seq 1 30); do
  adb shell pm path android > /dev/null 2>&1 && break
  sleep 5
done
installe=0
for essai in 1 2 3; do
  if adb install -r "$apk"; then
    installe=1
    break
  fi
  echo "installation refusée (essai $essai), nouvel essai dans 20 s" >&2
  sleep 20
done
[ "$installe" = 1 ] || { echo "installation impossible" >&2; exit 1; }

composant=$(adb shell cmd package resolve-activity --brief -c android.intent.category.LAUNCHER "$PAQUET" | tr -d '\r' | tail -n 1)
echo "Activité : $composant"
adb logcat -c
pid=""
for essai in 1 2; do
  adb shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS > /dev/null 2>&1 || true
  adb shell am start -W -n "$composant" || true
  sleep "$ATTENTE"
  adb logcat -d -t 800 > logcat.txt
  if grep -qE "FATAL EXCEPTION|Process: $PAQUET" logcat.txt && grep -q "$PAQUET" logcat.txt \
     && grep -A3 "FATAL EXCEPTION" logcat.txt | grep -q "$PAQUET"; then
    echo "MMail a planté au lancement :" >&2
    grep -iE 'mmail|AndroidRuntime|FATAL' logcat.txt | tail -40 >&2
    exit 1
  fi
  pid=$(adb shell pidof "$PAQUET" || true)
  [ -n "$pid" ] && break
  echo "MMail absent $ATTENTE s après le lancement (essai $essai)" >&2
done

if [ -z "$pid" ]; then
  echo "MMail ne tourne plus $ATTENTE s apres son lancement :" >&2
  grep -iE 'mmail|AndroidRuntime|FATAL|ANR in' logcat.txt | tail -40 >&2
  exit 1
fi

echo "MMail tourne (pid $pid)"

# Veille puis réveil (0.6.10). Le 21/10, Manu retrouve MMail sur un écran blanc
# après une mise en veille du téléphone, sans autre issue que de le fermer.
# MMail doit d'abord s'afficher (sur l'émulateur, son interface met plus d'une
# minute à se charger, sur un écran blanc) ; vient alors une veille courte, puis
# une veille profonde : Doze forcé et mémoire réclamée à l'application, comme
# après une longue nuit. Au réveil, MMail disparu, une application qui ne
# répond plus (ANR), ou un écran resté presque tout blanc échouent. Captures et
# journal d'Android partent avec les artefacts du travail.
mesurer() {
  # Part des pixels presque blancs d'une capture brute (screencap sans -p :
  # largeur, hauteur, format, parfois l'espace colorimétrique, puis RGBA).
  python3 -I -c '
import struct, sys
d = open(sys.argv[1], "rb").read()
l, h = struct.unpack("<II", d[:8])
px = d[len(d) - l * h * 4:]
n = blancs = 0
for i in range(0, len(px), 4 * 97):
    n += 1
    if px[i] > 240 and px[i + 1] > 240 and px[i + 2] > 240:
        blancs += 1
print(round(100 * blancs / max(n, 1)))
' "$1"
}
# attendre <nom> <secondes> : capture jusqu'à ce que l'écran ne soit plus
# presque blanc, ou que le délai soit passé ; la dernière capture est gardée.
attendre() {
  local fin=$((SECONDS + $2)) b=100
  while :; do
    adb exec-out screencap > "$1.raw"
    b=$(mesurer "$1.raw")
    [ "$b" -lt 97 ] || [ "$SECONDS" -ge "$fin" ] && break
    sleep 3
  done
  adb exec-out screencap -p > "$1.png"
  echo "$b"
}
reveiller() {
  adb shell input keyevent KEYCODE_WAKEUP
  sleep 2
  adb shell wm dismiss-keyguard || true
}
echec=0
controler() {
  # controler <étape> <part de blanc> : MMail toujours là, sans ANR, affiché.
  local pid2
  pid2=$(adb shell pidof "$PAQUET" || true)
  adb logcat -d > "logcat-$1.txt"
  cat "logcat-$1.txt" >> logcat.txt
  adb shell dumpsys window | grep -E 'mCurrentFocus' || true
  if [ -z "$pid2" ]; then
    echo "$1 : MMail ne tourne plus" >&2
    echec=1
  elif [ "$pid2" != "$pid" ]; then
    echo "$1 : MMail a été relancé (pid $pid, puis $pid2)" >&2
  fi
  if grep -q "ANR in $PAQUET" "logcat-$1.txt"; then
    echo "$1 : MMail ne répond plus (ANR)" >&2
    echec=1
  fi
  if [ "$2" -ge 97 ]; then
    echo "$1 : écran blanc 60 s après le réveil (${2} % de blanc)" >&2
    echec=1
  fi
}

avant=$(attendre veille-0-avant 240)
echo "MMail affiché avant la veille : ${avant} % de blanc"
if [ "$avant" -ge 97 ]; then
  echo "MMail ne s'est pas affiché en 4 minutes : essai de veille impossible" >&2
  exit 1
fi

adb logcat -c
adb shell input keyevent KEYCODE_SLEEP
sleep 30
reveiller
courte=$(attendre veille-1-courte 60)
echo "Veille courte (30 s) : ${courte} % de blanc au réveil"
controler veille-courte "$courte"

adb logcat -c
adb shell input keyevent KEYCODE_SLEEP
sleep 5
# Doze n'entre pas sur secteur : l'émulateur se dit débranché le temps de l'essai.
adb shell dumpsys battery unplug || true
adb shell dumpsys deviceidle force-idle || true
adb shell am send-trim-memory "$PAQUET" COMPLETE || true
sleep 60
adb shell dumpsys deviceidle unforce || true
adb shell dumpsys battery reset || true
reveiller
profonde=$(attendre veille-2-profonde 60)
echo "Veille profonde (Doze, mémoire réclamée) : ${profonde} % de blanc au réveil"
controler veille-profonde "$profonde"

[ "$echec" = 0 ] || exit 1
echo "Réveil après veille : MMail toujours affiché"
