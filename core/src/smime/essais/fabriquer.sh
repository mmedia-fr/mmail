#!/usr/bin/env bash
# Fabrique les certificats et les messages d'essai S/MIME (0.8.0) avec
# OpenSSL, la référence du CMS : ce que MMail lit ici, Outlook et Thunderbird
# l'écrivent de la même façon. Identités fictives (exemple.fr), clés d'essai
# seulement. À relancer seulement pour changer les essais : les fichiers
# produits sont versionnés, et les essais en dépendent.
#
#   bash core/src/smime/essais/fabriquer.sh
set -euo pipefail
cd "$(dirname "$0")"
rm -f ./*.pem ./*.key ./*.p12 ./*.eml ./*.cnf ./*.srl
export LC_ALL=C ADRESSE=x

cat > ext.cnf <<'EOF'
[racine]
basicConstraints = critical, CA:true
keyUsage = critical, keyCertSign, cRLSign
subjectKeyIdentifier = hash
[intermediaire]
basicConstraints = critical, CA:true, pathlen:0
keyUsage = critical, keyCertSign, cRLSign
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid
[personne]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature, keyEncipherment
extendedKeyUsage = emailProtection
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid
subjectAltName = email:${ENV::ADRESSE}
[serveur]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid
subjectAltName = email:${ENV::ADRESSE}
EOF

jours=7300  # vingt ans : les essais ne doivent pas expirer.

autorite() {  # nom, CN
  openssl req -x509 -newkey rsa:2048 -nodes -keyout "$1.key" -out "$1.pem" -days $jours \
    -subj "/O=MMail essais/CN=$2" -extensions racine -config <(cat /etc/ssl/openssl.cnf ext.cnf) 2>/dev/null
}
signer_par() {  # sujet, émetteur, section, CN, adresse, [clé : rsa|ec]
  local cle=${6:-rsa}
  if [ "$cle" = ec ]; then
    openssl req -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -keyout "$1.key" -out "$1.csr" \
      -subj "/O=MMail essais/CN=$4" 2>/dev/null
  else
    openssl req -newkey rsa:2048 -nodes -keyout "$1.key" -out "$1.csr" -subj "/O=MMail essais/CN=$4" 2>/dev/null
  fi
  ADRESSE="$5" openssl x509 -req -in "$1.csr" -CA "$2.pem" -CAkey "$2.key" -CAcreateserial \
    -days $jours -sha256 -extfile ext.cnf -extensions "$3" -out "$1.pem" 2>/dev/null
  rm -f "$1.csr"
}

autorite racine "Autorite d'essai"
ADRESSE=x openssl req -newkey rsa:2048 -nodes -keyout intermediaire.key -out intermediaire.csr \
  -subj "/O=MMail essais/CN=Intermediaire d'essai" 2>/dev/null
openssl x509 -req -in intermediaire.csr -CA racine.pem -CAkey racine.key -CAcreateserial -days $jours \
  -sha256 -extfile ext.cnf -extensions intermediaire -out intermediaire.pem 2>/dev/null
rm -f intermediaire.csr
signer_par alice intermediaire personne "Alice Essai" alice@exemple.fr
signer_par bob intermediaire personne "Bob Essai" bob@exemple.fr
signer_par carole intermediaire personne "Carole Essai" carole@exemple.fr ec
signer_par serveur intermediaire serveur "Serveur Essai" serveur@exemple.fr
autorite autre "Autre autorite"
signer_par inconnu autre personne "Inconnu Essai" inconnu@exemple.fr

# Le .p12 d'Alice et de Bob, comme les exporte Windows récent ou OpenSSL 3
# (PBES2, AES-256), et celui de Bob au format ancien (3DES, RC2-40).
openssl pkcs12 -export -inkey alice.key -in alice.pem -certfile intermediaire.pem -name alice \
  -passout pass:essai-alice -out alice.p12
openssl pkcs12 -export -inkey bob.key -in bob.pem -certfile intermediaire.pem -name bob \
  -passout pass:essai-bob -out bob.p12
openssl pkcs12 -export -legacy -inkey bob.key -in bob.pem -certfile intermediaire.pem -name bob \
  -passout pass:essai-bob -out bob-ancien.p12 2>/dev/null \
  || openssl pkcs12 -export -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-RC2-40 -macalg sha1 \
       -inkey bob.key -in bob.pem -certfile intermediaire.pem -name bob -passout pass:essai-bob -out bob-ancien.p12

# Le contenu des messages : une entité MIME, avec une pièce jointe.
printf '%s\r\n' \
  'Content-Type: multipart/mixed; boundary="contenu"' \
  '' \
  '--contenu' \
  'Content-Type: text/plain; charset=utf-8' \
  'Content-Transfer-Encoding: 8bit' \
  '' \
  'Bonjour Bob, voici le devis signé.' \
  '--contenu' \
  'Content-Type: text/plain; name="devis.txt"' \
  'Content-Disposition: attachment; filename="devis.txt"' \
  '' \
  'Total : 1 234,56 euros' \
  '--contenu--' > contenu.mime

entetes=(-from alice@exemple.fr -to bob@exemple.fr -subject "Devis")
openssl cms -sign -in contenu.mime -binary -signer alice.pem -inkey alice.key -certfile intermediaire.pem \
  -md sha256 -crlfeol "${entetes[@]}" -out signe-detache.eml
openssl cms -sign -in contenu.mime -binary -nodetach -signer alice.pem -inkey alice.key \
  -certfile intermediaire.pem -md sha256 -crlfeol "${entetes[@]}" -out signe-opaque.eml
openssl cms -sign -in contenu.mime -binary -signer carole.pem -inkey carole.key -certfile intermediaire.pem \
  -md sha256 -crlfeol -from carole@exemple.fr -to bob@exemple.fr -subject "Devis" -out signe-ecdsa.eml
openssl cms -sign -in contenu.mime -binary -signer inconnu.pem -inkey inconnu.key -md sha256 -crlfeol \
  -from inconnu@exemple.fr -to bob@exemple.fr -subject "Devis" -out signe-inconnu.eml
openssl cms -sign -in contenu.mime -binary -signer serveur.pem -inkey serveur.key -certfile intermediaire.pem \
  -md sha256 -crlfeol -from serveur@exemple.fr -to bob@exemple.fr -subject "Devis" -out signe-serveur.eml
openssl cms -sign -in contenu.mime -binary -signer alice.pem -inkey alice.key -certfile intermediaire.pem \
  -md sha1 -crlfeol "${entetes[@]}" -out signe-sha1.eml

openssl cms -encrypt -in contenu.mime -binary -aes256 -recip bob.pem -recip alice.pem -crlfeol \
  "${entetes[@]}" -out chiffre-aes.eml
openssl cms -encrypt -in contenu.mime -binary -des3 -recip bob.pem -crlfeol "${entetes[@]}" -out chiffre-3des.eml
openssl cms -encrypt -in contenu.mime -binary -aes128 -recip bob.pem -keyid -crlfeol "${entetes[@]}" \
  -out chiffre-ski.eml
openssl cms -encrypt -in contenu.mime -binary -aes256 -recip bob.pem -keyopt rsa_padding_mode:oaep -crlfeol \
  "${entetes[@]}" -out chiffre-oaep.eml
# Longueurs indéfinies (BER), comme les écrivent Outlook et Thunderbird.
openssl cms -encrypt -in contenu.mime -binary -aes256 -recip bob.pem -stream -crlfeol "${entetes[@]}" \
  -out chiffre-ber.eml
openssl cms -encrypt -in contenu.mime -binary -aes256 -recip alice.pem -crlfeol "${entetes[@]}" \
  -out chiffre-pour-alice.eml
# S/MIME 4.0 : AES-256-GCM, enveloppe authentifiée (ajouté le 03/11/2026).
openssl cms -encrypt -in contenu.mime -binary -aes-256-gcm -recip bob.pem -crlfeol "${entetes[@]}" \
  -out chiffre-gcm.eml
# Clé elliptique P-256 (ECDH, ECDSA), autosignée, mais certificat de personne
# (CA:FALSE : `req -x509` poserait CA:TRUE, refusé pour signer). Dérivation
# SHA-1 (défaut d'OpenSSL), puis SHA-512.
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -keyout dan.key -out dan.pem -days $jours \
  -subj "/O=MMail essais/CN=Dan Essai" -addext "subjectAltName=email:dan@exemple.fr" \
  -addext "basicConstraints=critical,CA:FALSE" -addext "keyUsage=critical,digitalSignature,keyAgreement" \
  -addext "extendedKeyUsage=emailProtection" 2>/dev/null
openssl pkcs12 -export -inkey dan.key -in dan.pem -name dan -passout pass:essai-dan -out dan.p12
openssl cms -encrypt -in contenu.mime -binary -aes256 -recip dan.pem -crlfeol \
  -from alice@exemple.fr -to dan@exemple.fr -subject "Devis" -out chiffre-ecdh.eml
openssl cms -encrypt -in contenu.mime -binary -aes128 -recip dan.pem -keyopt ecdh_kdf_md:sha512 -crlfeol \
  -from alice@exemple.fr -to dan@exemple.fr -subject "Devis" -out chiffre-ecdh-sha512.eml

# Signé puis chiffré, comme Outlook : la signature détachée devient le contenu
# chiffré.
openssl cms -sign -in contenu.mime -binary -signer alice.pem -inkey alice.key -certfile intermediaire.pem \
  -md sha256 -crlfeol -out signe-interieur.mime
openssl cms -encrypt -in signe-interieur.mime -binary -aes256 -recip bob.pem -recip alice.pem -stream -crlfeol \
  "${entetes[@]}" -out signe-chiffre.eml
rm -f signe-interieur.mime ext.cnf ./*.srl
rm -f ./*.key contenu.mime
ls -l
