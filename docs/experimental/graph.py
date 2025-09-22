import pandas as pd
import matplotlib.pyplot as plt
import matplotlib

# Fix font issues for PDF output
matplotlib.rcParams['font.family'] = 'DejaVu Sans'
matplotlib.rcParams['pdf.fonttype'] = 42
matplotlib.rcParams['ps.fonttype'] = 42

# Load datasets
simplex = pd.read_csv('simplex.csv', sep=';', comment='#')
pro_simplex = pd.read_csv('pro_simplex.csv', sep=';', comment='#')
hotstuff = pd.read_csv('hotstuff_2.csv', sep=';', comment='#')

# Label datasets
simplex['dataset'] = 'Simplex'
pro_simplex['dataset'] = 'Pro Simplex'
hotstuff['dataset'] = 'Jolteon'

# Combine all into a single DataFrame
df = pd.concat([simplex, pro_simplex, hotstuff])

# --- Compute throughput dynamically ---
df['txs_sec'] = (df['blocks_finalized'] * df['txs_per_block']) / df['duration_sec']

# --- Aggregate to mean + std per group ---
agg_df = df.groupby(
    ['dataset', 'tx_size_bytes', 'txs_per_block', 'num_nodes']
).agg(
    latency_mean=('latency', 'mean'),
    latency_std=('latency', 'std'),
    txs_sec_mean=('txs_sec', 'mean'),
    txs_sec_std=('txs_sec', 'std'),
).reset_index()

# Replace NaN std (cases with 1 value) with 0
agg_df['latency_std'] = agg_df['latency_std'].fillna(0)
agg_df['txs_sec_std'] = agg_df['txs_sec_std'].fillna(0)

# Unique values
tx_sizes = sorted(agg_df['tx_size_bytes'].unique())
txs_per_block_vals = sorted(agg_df['txs_per_block'].unique())

for tx_size in tx_sizes:
    for txs in txs_per_block_vals:
        # ---------- LATENCY PLOT ----------
        plt.figure(figsize=(8, 6))
        ax_lat = plt.gca()
        all_nodes_lat = set()

        for dataset in agg_df['dataset'].unique():
            subset = agg_df[(agg_df['dataset'] == dataset) &
                            (agg_df['txs_per_block'] == txs) &
                            (agg_df['tx_size_bytes'] == tx_size)].sort_values('num_nodes')
            if subset.empty:
                continue
            all_nodes_lat.update(subset['num_nodes'].unique())
            ax_lat.errorbar(
                subset['num_nodes'].to_numpy(),
                subset['latency_mean'].to_numpy(),
                yerr=subset['latency_std'].to_numpy(),
                marker='o',
                capsize=4,
                label=dataset
            )

        if not all_nodes_lat:
            plt.close()
            continue

        ax_lat.set_xlabel('Total number of processes', fontsize=16, labelpad=12)
        ax_lat.set_ylabel('Latency (sec)', fontsize=14, labelpad=12)
        ax_lat.set_title(f'{txs} txs of {tx_size} bytes', fontsize=16)
        ax_lat.tick_params(axis='both', which='major', labelsize=16)
        ax_lat.tick_params(axis='both', which='minor', labelsize=16)
        ax_lat.set_xticks(sorted(all_nodes_lat))
        ax_lat.grid(True)
        ax_lat.legend()
        plt.tight_layout()
        plt.savefig(f"latency_{txs}_{tx_size}.pdf", dpi=600, bbox_inches="tight")
        plt.close()

        # ---------- THROUGHPUT PLOT ----------
        plt.figure(figsize=(8, 6))
        ax_txs = plt.gca()
        all_nodes_txs = set()

        for dataset in agg_df['dataset'].unique():
            subset = agg_df[(agg_df['dataset'] == dataset) &
                            (agg_df['txs_per_block'] == txs) &
                            (agg_df['tx_size_bytes'] == tx_size)].sort_values('num_nodes')
            if subset.empty:
                continue
            all_nodes_txs.update(subset['num_nodes'].unique())
            ax_txs.errorbar(
                subset['num_nodes'].to_numpy(),
                subset['txs_sec_mean'].to_numpy(),
                yerr=subset['txs_sec_std'].to_numpy(),
                marker='o',
                capsize=4,
                label=dataset
            )

        if not all_nodes_txs:
            plt.close()
            continue

        ax_txs.set_xlabel('Total number of processes', fontsize=16, labelpad=12)
        ax_txs.set_ylabel('Throughput (txs/sec)', fontsize=14, labelpad=12)
        ax_txs.set_title(f'{txs} txs of {tx_size} bytes', fontsize=16)
        ax_txs.tick_params(axis='both', which='major', labelsize=16)
        ax_txs.tick_params(axis='both', which='minor', labelsize=16)
        ax_txs.set_xticks(sorted(all_nodes_txs))
        ax_txs.grid(True)
        ax_txs.legend()
        plt.tight_layout()
        plt.savefig(f"throughput_{txs}_{tx_size}.pdf", dpi=600, bbox_inches="tight")
        plt.close()
