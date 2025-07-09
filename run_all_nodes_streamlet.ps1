param (
    [int]$transaction_size,
    [int]$n_transactions,
    [double]$epoch_time,
    [string[]]$modes
)

# Path to CSV
$csvPath = "./shared/nodes.csv"

if (-not (Test-Path $csvPath)) {
    Write-Host "nodes.csv not found in shared/. Please generate it first."
    exit 1
}

# Validate numeric args
if (-not $transaction_size -or -not $n_transactions -or -not $epoch_time) {
    Write-Host "Error: Parameters -transaction_size, -n_transactions, and -epoch_time are required."
    Write-Host "Usage: .\start_nodes.ps1 -transaction_size <int> -n_transactions <int> -epoch_time <float64> -modes <'test'>"
    exit 1
}

# Modes: only "test" is valid now
$useTest = $false
foreach ($mode in $modes) {
    if ($mode -eq "test") {
        $useTest = $true
    } else {
        Write-Host "Unknown mode: '$mode'. Supported mode is 'test'."
        exit 1
    }
}

# Read nodes
$lines = Get-Content $csvPath | Select-Object -Skip 1

foreach ($line in $lines) {
    $parts = $line -split ","
    $id = $parts[0].Trim()
    $hostname = $parts[1].Trim()
    $port = $parts[2].Trim()

    Write-Host "Starting node ${id} on ${hostname}:${port}..."

    $modeArgs = @()
    if ($useTest) { $modeArgs += "test" }

    $allArgs = "$id $transaction_size $n_transactions $epoch_time $($modeArgs -join ' ')"

    Start-Process "powershell.exe" -ArgumentList "-NoExit", "-Command", "cargo run --release --package streamlet --bin streamlet $allArgs"
}

Write-Host "All nodes started."
