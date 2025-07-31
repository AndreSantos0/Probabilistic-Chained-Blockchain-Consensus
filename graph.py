import pandas as pd
import matplotlib.pyplot as plt

# Load datasets
simplex = pd.read_csv('simplex_2.csv', sep=';', comment='#')
pro_simplex = pd.read_csv('pro_simplex_2.csv', sep=';', comment='#')
hotstuff = pd.read_csv('hotstuff_2.csv', sep=';', comment='#')

# Label datasets
simplex['dataset'] = 'Simplex'
pro_simplex['dataset'] = 'Pro Simplex'
hotstuff['dataset'] = 'Jolteon'

# Combine all into a single DataFrame
df = pd.concat([simplex, pro_simplex, hotstuff])

tx_sizes = sorted(df['tx_size_bytes'].unique())

for tx_size in tx_sizes:
    # ---------- LATENCY PLOT ----------
    plt.figure(figsize=(8, 6))
    ax_lat = plt.gca()
    all_nodes_lat = set()

    for dataset in df['dataset'].unique():
        for txs in sorted(df['txs_per_block'].unique()):
            subset = df[(df['dataset'] == dataset) &
                        (df['txs_per_block'] == txs) &
                        (df['tx_size_bytes'] == tx_size)].sort_values('num_nodes')
            if subset.empty:
                continue
            all_nodes_lat.update(subset['num_nodes'].unique())
            ax_lat.plot(
                subset['num_nodes'].to_numpy(),
                subset['latency'].to_numpy(),
                marker='o',
                label=f'{dataset}, txs/block={txs}'
            )

    ax_lat.set_xlabel('Number of Nodes')
    ax_lat.set_ylabel('Latency (sec)')
    ax_lat.set_title(f'Latency vs Number of Nodes\n(tx_size_bytes={tx_size})')
    ax_lat.set_xticks(sorted(all_nodes_lat))
    ax_lat.legend(fontsize='small')
    ax_lat.grid(True)
    plt.tight_layout()
    plt.show()

    # ---------- THROUGHPUT PLOT ----------
    plt.figure(figsize=(8, 6))
    ax_txs = plt.gca()
    all_nodes_txs = set()

    for dataset in df['dataset'].unique():
        for txs in sorted(df['txs_per_block'].unique()):
            subset = df[(df['dataset'] == dataset) &
                        (df['txs_per_block'] == txs) &
                        (df['tx_size_bytes'] == tx_size)].sort_values('num_nodes')
            if subset.empty:
                continue
            all_nodes_txs.update(subset['num_nodes'].unique())
            ax_txs.plot(
                subset['num_nodes'].to_numpy(),
                subset['txs_sec'].to_numpy(),
                marker='o',
                label=f'{dataset}, txs/block={txs}'
            )

    ax_txs.set_xlabel('Number of Nodes')
    ax_txs.set_ylabel('Transactions per Second')
    ax_txs.set_title(f'Transactions per Second vs Number of Nodes\n(tx_size_bytes={tx_size})')
    ax_txs.set_xticks(sorted(all_nodes_txs))
    ax_txs.legend(fontsize='small')
    ax_txs.grid(True)
    plt.tight_layout()
    plt.show()
