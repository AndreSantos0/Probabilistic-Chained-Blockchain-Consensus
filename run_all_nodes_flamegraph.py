#!/usr/bin/env python3
"""
Launch all simplex nodes using the configuration from shared/nodes.csv.

Same as the original script, but runs node 0 under `cargo flamegraph`
for performance profiling.
"""

from __future__ import annotations

import argparse
import csv
import pathlib
import subprocess
import sys
from typing import List

MODES_TO_ARGS = {
    "prob": "probabilistic",
    "test": "test",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Start all simplex nodes defined in shared/nodes.csv."
    )
    parser.add_argument(
        "--transaction-size",
        required=True,
        type=int,
        help="Transaction size to use for each node (positive integer).",
    )
    parser.add_argument(
        "--n-transactions",
        required=True,
        type=int,
        help="Number of transactions to generate for each node (positive integer).",
    )
    parser.add_argument(
        "--epoch-time",
        required=True,
        type=float,
        help="Epoch duration in seconds for the profiled streamlet node (positive float).",
    )
    parser.add_argument(
        "--modes",
        nargs="*",
        default=[],
        choices=MODES_TO_ARGS.keys(),
        help="Optional modes to enable. Supported values: %(choices)s.",
    )
    parser.add_argument(
        "--no-store-results",
        action="store_true",
        help="Skip persisting finalized blocks to disk.",
    )
    parser.add_argument(
        "--processes",
        type=int,
        help="Number of node processes to launch (default: all listed nodes).",
    )
    parser.add_argument(
        "--profile-node",
        type=int,
        default=0,
        help="ID of the node to profile using cargo flamegraph (default: 0).",
    )
    return parser.parse_args()


def ensure_positive(name: str, value: int) -> None:
    if value <= 0:
        raise ValueError(f"{name} must be a positive integer (got {value}).")


def read_nodes(csv_path: pathlib.Path) -> List[dict]:
    with csv_path.open(newline="", encoding="utf-8") as fh:
        reader = csv.DictReader(fh)
        return list(reader)


def build_command(
    node_id: str,
    transaction_size: int,
    n_transactions: int,
    epoch_time: float,
    mode_args: List[str],
    skip_persistence: bool,
    process_limit: int | None,
    use_flamegraph: bool,
) -> List[str]:
    """Builds the command to start one node."""
    if use_flamegraph:
        # Profile this node with cargo flamegraph
        command = [
            "cargo",
            "flamegraph",
            "--bin",
            "streamlet",
            "--",
            str(node_id),
            str(transaction_size),
            str(n_transactions),
            f"{epoch_time}",
        ]
    else:
        # Normal execution
        command = [
            "cargo",
            "run",
            "--release",
            "--package",
            "simplex",
            "--bin",
            "simplex",
            str(node_id),
            str(transaction_size),
            str(n_transactions),
        ]

    command.extend(mode_args)
    if skip_persistence:
        command.append("no-store")
    if process_limit is not None:
        command.append(f"node-count={process_limit}")
    return command


def main() -> int:
    args = parse_args()
    ensure_positive("transaction_size", args.transaction_size)
    ensure_positive("n_transactions", args.n_transactions)
    if args.processes is not None:
        ensure_positive("processes", args.processes)
    if args.epoch_time <= 0.0:
        print("epoch_time must be a positive number.", file=sys.stderr)
        return 1

    csv_path = pathlib.Path("shared") / "nodes.csv"
    if not csv_path.exists():
        print("nodes.csv not found in shared/. Please generate it first.", file=sys.stderr)
        return 1

    try:
        nodes = read_nodes(csv_path)
    except Exception as exc:
        print(f"Failed to read {csv_path}: {exc}", file=sys.stderr)
        return 1

    if not nodes:
        print(f"No nodes found in {csv_path}.", file=sys.stderr)
        return 1

    mode_args = [MODES_TO_ARGS[mode] for mode in args.modes]

    selected_nodes = nodes
    if args.processes is not None:
        if args.processes > len(nodes):
            print(
                f"Requested {args.processes} processes but only {len(nodes)} entries exist in {csv_path}.",
                file=sys.stderr,
            )
            return 1
        selected_nodes = nodes[:args.processes]

    for node in selected_nodes:
        node_id = int(node.get("id"))
        hostname = node.get("host") or node.get("hostname")
        port = node.get("port")

        print(f"Starting node {node_id} on {hostname}:{port}...")

        use_flamegraph = node_id == args.profile_node
        command = build_command(
            str(node_id),
            args.transaction_size,
            args.n_transactions,
            args.epoch_time,
            mode_args,
            args.no_store_results,
            args.processes,
            use_flamegraph,
        )

        try:
            subprocess.Popen(command)
        except FileNotFoundError:
            print("Failed to launch cargo. Is Cargo installed and on PATH?", file=sys.stderr)
            return 1

    print("All nodes started.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
