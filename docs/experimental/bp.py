import pandas as pd
import matplotlib.pyplot as plt
import matplotlib

# Fix font issues for PDF output
matplotlib.rcParams['font.family'] = 'DejaVu Sans'
matplotlib.rcParams['pdf.fonttype'] = 42
matplotlib.rcParams['ps.fonttype'] = 42

# Filenames
files = ['simplex_bp.csv', 'pro_simplex_bp.csv']

# Loop over both files
for csv_file in files:
    # Load CSV with ; separator
    df = pd.read_csv(csv_file, sep=';', comment='#')

    # Create figure
    fig, ax = plt.subplots(figsize=(8, 7))

    # Plot lines for each num_nodes
    for num_nodes in sorted(df['num_nodes'].unique()):
        subset = df[df['num_nodes'] == num_nodes]
        subset = subset.sort_values(by='txs_per_block')

        # Plot line
        ax.plot(
            subset['txs_per_sec'].to_numpy(),
            subset['latency'].to_numpy(),
            marker='o',
            label=f'{num_nodes} processes'
        )

        # Draw an arrow pointing to the 10000 txs_per_block point
        row_10000 = subset[subset['txs_per_block'] == 10000]
        if not row_10000.empty:
            x = row_10000['txs_per_sec'].values[0]
            y = row_10000['latency'].values[0]

            # Decide offset direction: push text away from border
            x_offset = 0.05 * max(subset['txs_per_sec'])
            y_offset = 0.05 * max(subset['latency'])

            if x > 0.8 * max(subset['txs_per_sec']):  # Too close to right border
                x_offset = -x_offset
            if y > 0.8 * max(subset['latency']):      # Too close to top border
                y_offset = -y_offset

            ax.annotate(
                "10K txs",
                xy=(x, y),
                xytext=(x + x_offset, y + y_offset),
                arrowprops=dict(arrowstyle="->", lw=1.5, color='black'),
                fontsize=14,
                color='black',
                ha='center',
                clip_on=False
            )

    # Axis labels and title with consistent fonts and padding
    ax.set_xlabel('Transactions per Second (txs/sec)', fontsize=16, labelpad=12)
    ax.set_ylabel('Latency (sec)', fontsize=14, labelpad=12)
    name = csv_file.replace("_bp.csv", "")
    name = ''.join(part.capitalize() for part in name.split('_'))

    if name == "Simplex":
        name = "Practical Simplex"

    ax.set_title(f'Saturation Point ({name})', fontsize=16, pad=14)

    # Tick size
    ax.tick_params(axis='both', which='major', labelsize=14)
    ax.tick_params(axis='both', which='minor', labelsize=12)

    # Legend and grid
    ax.legend(fontsize=16, title_fontsize=12)
    ax.grid(True)

    plt.tight_layout()
    plt.savefig(f"sp_{name}.pdf", dpi=300, bbox_inches="tight")
    plt.close()
