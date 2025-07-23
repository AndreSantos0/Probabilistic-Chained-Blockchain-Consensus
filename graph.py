import pandas as pd
import matplotlib.pyplot as plt

simplex = pd.read_csv('simplex_2.csv', sep=';', comment='#')
pro_simplex = pd.read_csv('pro_simplex_2.csv', sep=';', comment='#')

simplex['dataset'] = 'Simplex'
pro_simplex['dataset'] = 'Pro Simplex'

df = pd.concat([simplex, pro_simplex])

tx_sizes = sorted(df['tx_size_bytes'].unique())

fig, axes = plt.subplots(2, 2, figsize=(16, 12))

for col, tx_size in enumerate(tx_sizes):
    ax_lat = axes[0, col]
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


    ax_txs = axes[1, col]
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
