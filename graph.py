import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns

sns.set_theme(style="whitegrid")

df = pd.read_csv('pro_simplex.csv', sep=';', decimal=',')

df['txs_sec'] = df['txs_sec'].astype(float)

grouped = df.groupby(['tx_size_bytes', 'txs_per_block'])

for (tx_size, txs_per_block), group in grouped:

    stats = group.groupby('num_nodes')['txs_sec'].agg(['mean', 'std']).reset_index()
    plt.figure(figsize=(8, 6))
    sns.barplot(
        data=stats,
        x='num_nodes',
        y='mean',
        hue='num_nodes',
        palette='viridis',
        legend=False,
        yerr=stats['std']
    )
    plt.title(f'{txs_per_block} txs of {tx_size} bytes')
    plt.xlabel('Number of Nodes')
    plt.ylabel('Throughput (txs/s)')
    plt.tight_layout()
    plt.show()
