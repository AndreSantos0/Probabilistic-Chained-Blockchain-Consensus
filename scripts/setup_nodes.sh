#!/bin/bash

# chmod +x scripts/setup_nodes.sh
# ./scripts/setup_nodes.sh 10
# source shared/set_keys.env

set -e

NUM_NODES=${1:-4}
SHARED_DIR="shared"
KEYS_FILE="public_keys.toml"

cd "$SHARED_DIR" || { echo "Shared directory not found."; exit 1; }

echo "" > "$KEYS_FILE"
echo -n "" > set_keys.env

echo "📄 Generating nodes.csv for $NUM_NODES nodes..."
echo "id,host,port" > nodes.csv
for ((i=0; i<NUM_NODES; i++)); do
  PORT=$((8081 + i))
  echo "$i,127.0.0.1,$PORT" >> nodes.csv
done

echo "🔐 Generating PKCS#8 Ed25519 key pairs..."
for ((i=0; i<NUM_NODES; i++)); do
  TMP_DIR=$(mktemp -d)

  # Generate private key in PKCS#8 DER format
  openssl genpkey -algorithm Ed25519 -out "$TMP_DIR/private_key.pem"
  openssl pkcs8 -topk8 -nocrypt -in "$TMP_DIR/private_key.pem" -outform DER -out "$TMP_DIR/private_key.der"

  # Extract public key (base64)
  openssl pkey -in "$TMP_DIR/private_key.pem" -pubout -outform DER > "$TMP_DIR/public_key.der"
  PUB_BASE64=$(base64 -w 0 "$TMP_DIR/public_key.der")
  PRIV_BASE64=$(base64 -w 0 "$TMP_DIR/private_key.der")

  rm -rf "$TMP_DIR"

  echo "[$i]" >> "$KEYS_FILE"
  echo "public_key = \"$PUB_BASE64\"" >> "$KEYS_FILE"
  echo "" >> "$KEYS_FILE"

  export "PRIVATE_KEY_$i=$PRIV_BASE64"
  echo "export PRIVATE_KEY_$i=\"$PRIV_BASE64\"" >> set_keys.env
done

echo "✅ Keys and nodes.csv created successfully."
echo "ℹ️ Run: source set_keys.env to load private keys into your environment."