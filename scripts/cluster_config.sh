#!/bin/bash

# Outside
# scp -oHostKeyAlgorithms=+ssh-rsa -oPubkeyAcceptedAlgorithms=+ssh-rsa "C:/Users/André Santos/.ssh/id_rsa" andresantos@quinta.navigators.di.fc.ul.pt:~/.ssh
# scp -oHostKeyAlgorithms=+ssh-rsa -oPubkeyAcceptedAlgorithms=+ssh-rsa "C:/Users/André Santos/.ssh/id_rsa.pub" andresantos@quinta.navigators.di.fc.ul.pt:~/.ssh
# Inside
# scp ~/.ssh/id_rsa.pub root@s9:~/.ssh/
# ssh -oHostKeyAlgorithms=+ssh-rsa -oPubkeyAcceptedAlgorithms=+ssh-rsa andresantos@quinta.navigators.di.fc.ul.pt
# chmod +x config.sh
# kadeploy3 -e ubuntu-20.04 -f myNodes -s config.sh -k ~/id_rsa.pub

set -e

NODES_FILE="myNodes"
KEYS_FILE="keys"
REMOTE_SCRIPT="/tmp/remote_setup.sh"
SSH_USER="root"

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
  git clone "$REPO_URL"
else
  echo "Repository exists. Pulling latest changes..."
  cd "$REPO_DIR"
  git pull origin main || git pull origin master
  cd ..
fi

# Check cargo
if ! command -v cargo &> /dev/null; then
  echo "Installing Rust/Cargo..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
fi

# Install gcc
echo "🔧 Installing build-essential (GCC)..."
apt update
DEBIAN_FRONTEND=noninteractive apt install -y build-essential

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

node_id=0
PORT=8081
while read -r node; do
  ip=$(getent hosts "$node" | awk '{ print $1 }')
  if [ -z "$ip" ]; then
    echo "❌ Could not resolve IP for node: $node"
    exit 1
  fi

  echo "echo \"$node_id,$ip,$PORT\" >> \"\$REPO_DIR/shared/nodes.csv\"" >> remote_script.sh

  ((node_id++))
  ((PORT++))
done < "$NODES_FILE"

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
  echo "Deploying to node: $node"
  scp ~/.ssh/id_rsa.pub $SSH_USER@"$node":~/.ssh/
  scp remote_script.sh $SSH_USER@"$node":$REMOTE_SCRIPT
  ssh -n $SSH_USER@"$node" "bash $REMOTE_SCRIPT"
done < "$NODES_FILE"

echo "✅ All nodes configured and built."
