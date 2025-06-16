param (
    [int]$transaction_size,
    [int]$n_transactions,
    [string[]]$modes
)

# Define the path to the CSV file
$csvPath = "./shared/nodes.csv"

# Check if the file exists
if (-not (Test-Path $csvPath)) {
    Write-Host "nodes.csv not found in shared/. Please generate it first."
    exit 1
}

# Validate required numeric parameters
if (-not $transaction_size -or -not $n_transactions) {
    Write-Host "Error: Both -transaction_size and -n_transactions parameters are required."
    Write-Host "Usage: .\start_nodes.ps1 -transaction_size <int> -n_transactions <int> -modes <'prob','test'>"
    exit 1
}

# Parse modes
$validModes = @("prob", "test")
$useProb = $false
$useTest = $false

foreach ($mode in $modes) {
    if ($mode -eq "prob") {
        $useProb = $true
    } elseif ($mode -eq "test") {
        $useTest = $true
    } else {
        Write-Host "Unknown mode: '$mode'. Supported modes are 'prob' and 'test'."
        exit 1
    }
}

# Skip the header and read all lines
$lines = Get-Content $csvPath | Select-Object -Skip 1

foreach ($line in $lines) {
    $parts = $line -split ","
    $id = $parts[0].Trim()
    $hostname = $parts[1].Trim()
    $port = $parts[2].Trim()

    Write-Host "Starting node ${id} on ${hostname}:${port}..."

    $argsList = @("run", "--package", "simplex", "--bin", "simplex", $id, $transaction_size, $n_transactions)

    if ($useProb) {
        $argsList += "probabilistic"
    }
    if ($useTest) {
        $argsList += "test"
    }

    $modeArgs = @()
    if ($useProb) { $modeArgs += "probabilistic" }
    if ($useTest) { $modeArgs += "test" }

    $allArgs = "$id $transaction_size $n_transactions $($modeArgs -join ' ')"
    Start-Process "powershell.exe" -ArgumentList "-NoExit", "-Command", "cargo run --package simplex --bin simplex $allArgs"
}

Write-Host "All nodes started."
