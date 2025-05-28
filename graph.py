import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import numpy as np

sns.set_theme(style="whitegrid")

# Load and prepare both datasets
df1 = pd.read_csv('simplex.csv', sep=';', decimal=',')
df2 = pd.read_csv('pro_simplex.csv', sep=';', decimal=',')

df1['txs_sec'] = df1['txs_sec'].astype(float)
df2['txs_sec'] = df2['txs_sec'].astype(float)

df1['Protocol'] = 'Simplex'
df2['Protocol'] = 'ProSimplex'

df = pd.concat([df1, df2], ignore_index=True)

# Group by tx_size and txs_per_block to create subplots
grouped = df.groupby(['tx_size_bytes', 'txs_per_block'])

fig, axes = plt.subplots(1, len(grouped), figsize=(18, 6), sharey=True)
if len(grouped) == 1:
    axes = [axes]

palette = {
    'Simplex': '#1f77b4',      # Blue
    'ProSimplex': '#ff7f0e'    # Orange
}

for ax, ((tx_size, txs_per_block), group) in zip(axes, grouped):
    stats = group.groupby(['num_nodes', 'Protocol'])['txs_sec'].agg(['mean', 'std']).reset_index()
    num_nodes = sorted(stats['num_nodes'].unique())
    protocols = stats['Protocol'].unique()

    x = np.arange(len(num_nodes))
    width = 0.35

    for i, protocol in enumerate(protocols):
        data = stats[stats['Protocol'] == protocol]
        ax.bar(
            x + i * width,
            data['mean'],
            width=width,
            yerr=data['std'],
            capsize=5,
            label=protocol,
            color=palette[protocol]
        )

    ax.set_title(f'{txs_per_block} txs of {tx_size} bytes')
    ax.set_xlabel('Number of Nodes')
    ax.set_xticks(x + width / 2)
    ax.set_xticklabels([str(n) for n in num_nodes])

axes[0].set_ylabel('Throughput (txs/s)')
axes[0].legend(title='Protocol', loc='upper left')
plt.tight_layout()
plt.show()
