# Define the path to the CSV file
$csvPath = "./shared/nodes.csv"

# Check if the file exists
if (-not (Test-Path $csvPath)) {
    Write-Host "nodes.csv not found in shared/. Please generate it first."
    exit 1
}

# Parse all provided arguments to check for "prob" and/or "test"
$useProb = $args -contains "prob"
$useTest = $args -contains "test"

# Validate that no unknown arguments were passed
foreach ($arg in $args) {
    if ($arg -ne "prob" -and $arg -ne "test") {
        Write-Host "Unknown mode: '$arg'. Supported modes are 'prob', 'test', or both."
        exit 1
    }
}

# Skip the header and read all lines
$lines = Get-Content $csvPath | Select-Object -Skip 1

foreach ($line in $lines) {
    # Split each line into components
    $parts = $line -split ","
    $id = $parts[0].Trim()
    $hostname = $parts[1].Trim()
    $port = $parts[2].Trim()

    Write-Host "Starting node ${id} on ${hostname}:${port}..."

    # Build the argument list for cargo run
    $argsList = @("run", "--package", "simplex", "--bin", "simplex", $id)

    if ($useProb) {
        $argsList += "probabilistic"
    }
    if ($useTest) {
        $argsList += "test"
    }

    # Start the process
    Start-Process "cargo" -ArgumentList $argsList
}

Write-Host "All nodes started."
