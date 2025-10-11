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

# --- Aggregate to mean per group (no std) ---
agg_df = df.groupby(
    ['dataset', 'tx_size_bytes', 'txs_per_block', 'num_nodes']
).agg(
    latency_mean=('latency', 'mean'),
    txs_sec_mean=('txs_sec', 'mean')
).reset_index()

# Unique values
tx_sizes = sorted(agg_df['tx_size_bytes'].unique())
txs_per_block_vals = sorted(agg_df['txs_per_block'].unique())

for tx_size in tx_sizes:
    for txs in txs_per_block_vals:
        print(f"\n=== Results for {txs} txs of {tx_size} bytes ===")

        # Filter relevant subset
        subset_all = agg_df[
            (agg_df['txs_per_block'] == txs) &
            (agg_df['tx_size_bytes'] == tx_size)
            ].sort_values(['num_nodes', 'dataset'])

        # Get Pro Simplex reference
        pro_ref = subset_all[subset_all['dataset'] == 'Pro Simplex']

        for num_nodes in sorted(subset_all['num_nodes'].unique()):
            print(f"\n--- {num_nodes} processes ---")

            ref_row = pro_ref[pro_ref['num_nodes'] == num_nodes]
            if ref_row.empty:
                print("No Pro Simplex data for this config, skipping comparison.")
                continue

            pro_lat = ref_row['latency_mean'].values[0]
            pro_thr = ref_row['txs_sec_mean'].values[0]

            for dataset in subset_all['dataset'].unique():
                row = subset_all[(subset_all['dataset'] == dataset) &
                                 (subset_all['num_nodes'] == num_nodes)]
                if row.empty:
                    continue

                lat = row['latency_mean'].values[0]
                thr = row['txs_sec_mean'].values[0]

                if dataset == 'Pro Simplex':
                    print(f"{dataset}: Latency={lat:.4f} sec, Throughput={thr:.2f} tx/s")
                else:
                    lat_diff = ((lat - pro_lat) / pro_lat) * 100
                    thr_diff = ((thr - pro_thr) / pro_thr) * 100
                    print(f"{dataset}: Latency={lat:.4f} sec "
                          f"({lat_diff:+.2f}% vs Pro Simplex), "
                          f"Throughput={thr:.2f} tx/s "
                          f"({thr_diff:+.2f}% vs Pro Simplex)")

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
            ax_lat.plot(
                subset['num_nodes'].to_numpy(),
                subset['latency_mean'].to_numpy(),
                marker='o',
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

        xticks = sorted(all_nodes_lat)
        ax_lat.set_xticks(xticks)
        xtick_labels = [str(x) if x != 7 else "" for x in xticks]
        ax_lat.set_xticklabels(xtick_labels)

        ax_lat.grid(True)
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
            ax_txs.plot(
                subset['num_nodes'].to_numpy(),
                subset['txs_sec_mean'].to_numpy(),
                marker='o',
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

        xticks = sorted(all_nodes_txs)
        ax_txs.set_xticks(xticks)
        xtick_labels = [str(x) if x != 7 else "" for x in xticks]
        ax_txs.set_xticklabels(xtick_labels)

        ax_txs.grid(True)
        plt.tight_layout()
        plt.savefig(f"throughput_{txs}_{tx_size}.pdf", dpi=600, bbox_inches="tight")
        plt.close()
