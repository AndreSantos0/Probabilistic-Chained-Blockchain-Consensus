#!/bin/bash

# chmod +x scripts/setup_nodes.sh
# ./scripts/setup_nodes.sh 10
# source scripts/set_keys.env

set -e

NUM_NODES=${1:-4}
SHARED_DIR="shared"
KEYS_FILE="public_keys.toml"

cd "$SHARED_DIR" || { echo "Shared directory not found."; exit 1; }

# Step 0: Clear previous public keys file
echo "" > "$KEYS_FILE"

# Step 1: Generate nodes.csv
echo "📄 Generating nodes.csv for $NUM_NODES nodes..."
echo "id,host,port" > nodes.csv
for ((i=0; i<NUM_NODES; i++)); do
  PORT=$((8081 + i))
  echo "$i,127.0.0.1,$PORT" >> nodes.csv
done

# Step 2: Generate Ed25519 keypairs in PKCS#8 DER format and export
echo "🔐 Generating Ed25519 key pairs in PKCS#8 format..."
echo -n "" > ../set_keys.env

for ((i=0; i<NUM_NODES; i++)); do
  TMP_DIR=$(mktemp -d)

  # Generate private key in PKCS#8 DER format
  openssl genpkey -algorithm Ed25519 -outform DER -out "$TMP_DIR/private.der"

  # Extract the public key in PEM format and convert to raw base64
  openssl pkey -in "$TMP_DIR/private.der" -inform DER -pubout -outform DER > "$TMP_DIR/public.der"
  PUB_BASE64=$(base64 -w 0 "$TMP_DIR/public.der")

  # Base64 encode the private key
  PRIV_BASE64=$(base64 -w 0 "$TMP_DIR/private.der")

  # Clean up
  rm -rf "$TMP_DIR"

  # Append to TOML
  echo "[$i]" >> "$KEYS_FILE"
  echo "public_key = \"$PUB_BASE64\"" >> "$KEYS_FILE"
  echo "" >> "$KEYS_FILE"

  # Export private key as env variable
  export "PRIVATE_KEY_$i=$PRIV_BASE64"
  echo "export PRIVATE_KEY_$i=\"$PRIV_BASE64\"" >> ../set_keys.env
done

echo "✅ Finished: nodes.csv and PKCS#8 public_keys.toml generated."
echo "ℹ️ To load private keys into your session, run: source set_keys.env"