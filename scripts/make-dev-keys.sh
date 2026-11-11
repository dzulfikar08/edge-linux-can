#!/bin/sh
# Create the DEVELOPMENT signing keys for RAUC bundles in keys/dev/.
#
#   scripts/make-dev-keys.sh          # creates keys/dev/ if it does not exist yet
#
# Output (keys/dev/ is in .gitignore, nothing here is committed):
#   ca.cert.pem      development CA certificate. Goes into the image as the RAUC
#                    keyring: the device accepts bundles signed by this CA only.
#   ca.key.pem       CA private key (only needed to issue more signing certs)
#   signing.cert.pem signing certificate issued by the CA
#   signing.key.pem  signing private key: the bundle recipes sign with it
#   other-ca/        a second, unrelated CA + signing key, used by the A/B test
#                    to prove that a bundle signed by someone else is refused
#
# These keys are for this QEMU learning project only. Every subject says
# "DEVELOPMENT ONLY". A real product keeps its signing key off build machines
# (HSM or a separate signing step), and devices never trust a development CA.
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
dir="$repo/keys/dev"

if [ -e "$dir/ca.cert.pem" ]; then
    echo "keys already exist in $dir (delete the directory to make new ones;"
    echo "images built with the old CA will then refuse bundles signed with the new key)"
    exit 0
fi
mkdir -p "$dir/other-ca"
cd "$dir"

ext=$(mktemp)
trap 'rm -f "$ext"' EXIT
cat > "$ext" <<'EOF'
[ca]
basicConstraints = critical, CA:true
keyUsage = critical, keyCertSign, cRLSign
subjectKeyIdentifier = hash
[signer]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid
EOF

# make_ca <dir> <name>: a self-signed CA and one signing certificate issued by it
make_ca() {
    d=$1; name=$2
    openssl req -x509 -newkey rsa:4096 -nodes -days 3650 \
        -keyout "$d/ca.key.pem" -out "$d/ca.cert.pem" \
        -subj "/O=edge-linux-can/CN=$name CA - DEVELOPMENT ONLY, NOT FOR PRODUCTION" \
        -extensions ca -config "$ext" 2>/dev/null
    openssl req -newkey rsa:4096 -nodes \
        -keyout "$d/signing.key.pem" -out "$d/signing.csr.pem" \
        -subj "/O=edge-linux-can/CN=$name bundle signing - DEVELOPMENT ONLY" 2>/dev/null
    openssl x509 -req -in "$d/signing.csr.pem" -days 3650 \
        -CA "$d/ca.cert.pem" -CAkey "$d/ca.key.pem" -CAcreateserial \
        -extfile "$ext" -extensions signer -out "$d/signing.cert.pem" 2>/dev/null
    rm -f "$d/signing.csr.pem" "$d/ca.cert.srl"
    chmod 600 "$d"/*.key.pem
}

make_ca . "edge-linux-can dev"
make_ca other-ca "untrusted other"

cat > README.txt <<'EOF'
DEVELOPMENT KEYS FOR A QEMU LEARNING PROJECT. NOT SECRET, NOT FOR PRODUCTION.
Created by scripts/make-dev-keys.sh. Not committed (see .gitignore).
EOF
openssl verify -CAfile ca.cert.pem signing.cert.pem
openssl x509 -in ca.cert.pem -noout -subject -enddate
echo "keys written to $dir"
