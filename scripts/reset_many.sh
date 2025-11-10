#!/bin/bash

set -e

NODES_FILE="myNodes"
KEYS_FILE="keys"
REMOTE_SCRIPT="/tmp/remote_setup.sh"
SSH_USER="root"

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <n_processes>"
  exit 1
fi

TOTAL_PROCESSES=$1

echo "📄 Generating remote script..."

cat << 'EOF' > remote_script.sh
#!/bin/bash
set -e

cat ~/.ssh/id_rsa.pub >> ~/.ssh/authorized_keys
chmod 600 ~/.ssh/authorized_keys
chmod 700 ~/.ssh

REPO_URL="https://github.com/AndreSantos0/Probabilistic-Chained-Blockchain-Consensus.git"
REPO_DIR="Probabilistic-Chained-Blockchain-Consensus"
SHARED_DIR="$REPO_DIR/shared"
PUBLIC_KEYS_FILE="$SHARED_DIR/public_keys.toml"
SET_KEYS_FILE="$SHARED_DIR/set_keys.env"

# Clone or pull repo
if [ ! -d "$REPO_DIR" ]; then
  echo "Cloning repository..."
  git clone -b main "$REPO_URL"
else
  echo "Repository exists. Pulling latest changes..."
  cd "$REPO_DIR"
  git checkout main
  git pull origin main
  cd ..
fi

# Check cargo
if ! command -v cargo &> /dev/null; then
  echo "Installing Rust/Cargo..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
fi

# Clean old files
echo "🧹 Cleaning old shared files..."
echo "" > "$PUBLIC_KEYS_FILE"
echo "" > "$SET_KEYS_FILE"
EOF

echo "🔐 Embedding key setup into remote script..."

node_id=0
while IFS= read -r line; do
  PRIVATE_KEY=$(echo "$line" | cut -d ':' -f1)
  PUBLIC_KEY=$(echo "$line" | cut -d ':' -f2)

  {
    echo "echo \"[$node_id]\" >> \"\$PUBLIC_KEYS_FILE\""
    echo "echo \"public_key = \\\"$PUBLIC_KEY\\\"\" >> \"\$PUBLIC_KEYS_FILE\""
    echo "echo \"\" >> \"\$PUBLIC_KEYS_FILE\""
    echo "echo \"export PRIVATE_KEY_$node_id=\\\"$PRIVATE_KEY\\\"\" >> \"\$SET_KEYS_FILE\""
  } >> remote_script.sh

  ((node_id++))
done < "$KEYS_FILE"

# Add node info and build command
echo "echo 'id,host,port' > \"\$REPO_DIR/shared/nodes.csv\"" >> remote_script.sh

# Read nodes into an array
nodes=()
while IFS= read -r line || [[ -n "$line" ]]; do
  nodes+=("$line")
done < "$NODES_FILE"

NUM_NODES=${#nodes[@]}

if [ $NUM_NODES -eq 0 ]; then
  echo "❌ No nodes found in $NODES_FILE"
  exit 1
fi

BASE=$(( TOTAL_PROCESSES / NUM_NODES ))
REMAINDER=$(( TOTAL_PROCESSES % NUM_NODES ))

node_id=0

for index in "${!nodes[@]}"; do
  node="${nodes[$index]}"
  ip=$(getent hosts "$node" | awk '{ print $1 }')
  if [ -z "$ip" ]; then
    echo "❌ Could not resolve IP for node: $node"
    exit 1
  fi

  COUNT=$BASE
  if [ $index -lt $REMAINDER ]; then
    COUNT=$(( COUNT + 1 ))
  fi

  PORT=8081
  for ((i=0; i<COUNT; i++)); do
    echo "echo \"$node_id,$ip,$PORT\" >> \"\$REPO_DIR/shared/nodes.csv\"" >> remote_script.sh
    ((node_id++))
    ((PORT++))
  done
done

# Add build command
cat << 'EOF' >> remote_script.sh

# Build the Rust project
cd "$REPO_DIR"
echo "⚙️ Building project..."
cargo build --release
EOF

chmod +x remote_script.sh

echo "🚀 Deploying script to nodes..."

while read -r node; do
  if [ -z "$node" ]; then
    continue
  fi

  echo "Deploying to node: $node"
  scp ~/.ssh/id_rsa.pub $SSH_USER@"$node":~/.ssh/
  scp remote_script.sh $SSH_USER@"$node":$REMOTE_SCRIPT
  ssh -n $SSH_USER@"$node" "bash $REMOTE_SCRIPT"
done < "$NODES_FILE"

echo "✅ All nodes configured and built."