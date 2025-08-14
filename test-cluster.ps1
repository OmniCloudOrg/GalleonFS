# GalleonFS Two-Node Cluster Test Script (PowerShell)
# This script sets up a two-node GalleonFS cluster and creates a replicated volume

param(
    [switch]$SkipBuild,
    [switch]$KeepFiles
)

# Configuration
$NODE1_STORAGE = ".\test_node1_storage"
$NODE2_STORAGE = ".\test_node2_storage"
$NODE1_BIND = "127.0.0.1:8081"
$NODE2_BIND = "127.0.0.1:8082"
$NODE1_IPC = "127.0.0.1:8091"
$NODE2_IPC = "127.0.0.1:8092"
$VOLUME_NAME = "test-replicated-volume"
$VOLUME_SIZE = "1G"
$REPLICAS = 2

# Global variables for process tracking
$Global:Node1Process = $null
$Global:Node2Process = $null

# Logging functions
function Write-Info {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Blue
}

function Write-Success {
    param([string]$Message)
    Write-Host "[SUCCESS] $Message" -ForegroundColor Green
}

function Write-Warning {
    param([string]$Message)
    Write-Host "[WARNING] $Message" -ForegroundColor Yellow
}

function Write-Error {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

# Cleanup function
function Cleanup {
    Write-Info "Cleaning up..."
    
    # Stop daemon processes
    if ($Global:Node1Process -and !$Global:Node1Process.HasExited) {
        Write-Info "Stopping Node 1 (PID: $($Global:Node1Process.Id))"
        try {
            $Global:Node1Process.Kill()
            $Global:Node1Process.WaitForExit(5000)
        }
        catch {
            Write-Warning "Failed to stop Node 1 gracefully: $_"
        }
    }
    
    if ($Global:Node2Process -and !$Global:Node2Process.HasExited) {
        Write-Info "Stopping Node 2 (PID: $($Global:Node2Process.Id))"
        try {
            $Global:Node2Process.Kill()
            $Global:Node2Process.WaitForExit(5000)
        }
        catch {
            Write-Warning "Failed to stop Node 2 gracefully: $_"
        }
    }
    
    # Wait a moment for processes to cleanup
    Start-Sleep -Seconds 2
    
    # Remove storage directories unless keeping files
    if (-not $KeepFiles) {
        if (Test-Path $NODE1_STORAGE) {
            Write-Info "Removing Node 1 storage directory"
            Remove-Item -Path $NODE1_STORAGE -Recurse -Force -ErrorAction SilentlyContinue
        }
        
        if (Test-Path $NODE2_STORAGE) {
            Write-Info "Removing Node 2 storage directory"
            Remove-Item -Path $NODE2_STORAGE -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
    
    Write-Success "Cleanup completed"
}

# Function to test if a TCP port is open
function Test-Port {
    param(
        [string]$Address,
        [int]$Port,
        [int]$TimeoutMs = 1000
    )
    
    try {
        $tcpClient = New-Object System.Net.Sockets.TcpClient
        $asyncResult = $tcpClient.BeginConnect($Address, $Port, $null, $null)
        $wait = $asyncResult.AsyncWaitHandle.WaitOne($TimeoutMs, $false)
        
        if ($wait) {
            $tcpClient.EndConnect($asyncResult)
            $tcpClient.Close()
            return $true
        }
        else {
            $tcpClient.Close()
            return $false
        }
    }
    catch {
        return $false
    }
}

# Function to wait for service to be ready
function Wait-ForService {
    param(
        [string]$Address,
        [string]$Name,
        [int]$MaxAttempts = 30
    )
    
    $addressParts = $Address -split ":"
    $my_host = $addressParts[0]
    $port = [int]$addressParts[1]
    
    Write-Info "Waiting for $Name to be ready at $Address..."
    
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        if (Test-Port -Address $my_host -Port $port) {
            Write-Success "$Name is ready!"
            return $true
        }
        
        Write-Host "." -NoNewline
        Start-Sleep -Seconds 1
    }
    
    Write-Host ""
    Write-Error "$Name failed to start within $MaxAttempts seconds"
    return $false
}

# Check if cargo build was successful
function Test-Binary {
    $binaryPath = ".\target\release\galleonfs.exe"
    if (-not (Test-Path $binaryPath)) {
        Write-Error "GalleonFS binary not found at $binaryPath"
        
        if (-not $SkipBuild) {
            Write-Info "Building GalleonFS..."
            $buildResult = Start-Process -FilePath "cargo" -ArgumentList "build", "--release", "--no-default-features" -Wait -PassThru -NoNewWindow
            
            if ($buildResult.ExitCode -ne 0) {
                Write-Error "Failed to build GalleonFS"
                exit 1
            }
            
            if (-not (Test-Path $binaryPath)) {
                Write-Error "Binary still not found after build"
                exit 1
            }
        }
        else {
            Write-Error "Use -SkipBuild to skip automatic building"
            exit 1
        }
    }
    
    return $binaryPath
}

# Execute a GalleonFS CLI command
function Invoke-GalleonCommand {
    param(
        [string]$BinaryPath,
        [string[]]$Arguments,
        [string]$Description
    )
    
    Write-Info $Description
    Write-Host "Command: $BinaryPath $($Arguments -join ' ')" -ForegroundColor Gray
    
    $process = Start-Process -FilePath $BinaryPath -ArgumentList $Arguments -Wait -PassThru -NoNewWindow
    
    if ($process.ExitCode -ne 0) {
        Write-Warning "Command failed with exit code: $($process.ExitCode)"
        return $false
    }
    
    return $true
}

# Main execution
function Main {
    # Set up cleanup on exit
    try {
        Write-Info "Starting GalleonFS Two-Node Cluster Test"
        Write-Info "========================================"
        
        # Check if binary exists
        $binaryPath = Test-Binary
        
        # Initial cleanup in case of previous failed runs
        Cleanup
        
        Write-Info "Configuration:"
        Write-Info "  Node 1: Storage=$NODE1_STORAGE, Bind=$NODE1_BIND, IPC=$NODE1_IPC"
        Write-Info "  Node 2: Storage=$NODE2_STORAGE, Bind=$NODE2_BIND, IPC=$NODE2_IPC"
        Write-Info "  Volume: Name=$VOLUME_NAME, Size=$VOLUME_SIZE, Replicas=$REPLICAS"
        Write-Host ""
        
        # Step 1: Start Node 1 (creates cluster)
        Write-Info "Step 1: Starting Node 1 (creates cluster)"
        
        $mountPath1 = "$NODE1_STORAGE\mount1"
        $node1Args = @(
            "-d",
            "--storage-path", $NODE1_STORAGE,
            "--bind-address", $NODE1_BIND,
            "--ipc-address", $NODE1_IPC,
            "--mount-point", $mountPath1
        )
        
        $Global:Node1Process = Start-Process -FilePath $binaryPath -ArgumentList $node1Args -PassThru -NoNewWindow -RedirectStandardOutput "node1.log" -RedirectStandardError "node1_error.log"
        Write-Info "Node 1 started with PID: $($Global:Node1Process.Id)"
        
        # Wait for Node 1 to be ready
        if (-not (Wait-ForService -Address $NODE1_IPC -Name "Node 1 IPC service")) {
            throw "Node 1 failed to start"
        }
        Start-Sleep -Seconds 2  # Give it a moment to fully initialize
        
        # Step 2: Start Node 2 (joins cluster)
        Write-Info "Step 2: Starting Node 2 (joins cluster)"
        
        $mountPath2 = "$NODE2_STORAGE\mount2"
        $node2Args = @(
            "-d",
            "--storage-path", $NODE2_STORAGE,
            "--bind-address", $NODE2_BIND,
            "--ipc-address", $NODE2_IPC,
            "--peer-addresses", $NODE1_BIND,
            "--mount-point", $mountPath2
        )
        
        $Global:Node2Process = Start-Process -FilePath $binaryPath -ArgumentList $node2Args -PassThru -NoNewWindow -RedirectStandardOutput "node2.log" -RedirectStandardError "node2_error.log"
        Write-Info "Node 2 started with PID: $($Global:Node2Process.Id)"
        
        # Wait for Node 2 to be ready
        if (-not (Wait-ForService -Address $NODE2_IPC -Name "Node 2 IPC service")) {
            throw "Node 2 failed to start"
        }
        Start-Sleep -Seconds 3  # Give cluster time to form
        
        # Step 3: Check cluster status
        Write-Info "Step 3: Checking cluster status"
        Write-Host "=========================="
        Invoke-GalleonCommand -BinaryPath $binaryPath -Arguments @("--daemon-address", $NODE1_IPC, "cluster", "status") -Description "Getting cluster status from Node 1"
        Write-Host ""
        
        # Step 4: Create a replicated volume
        Write-Info "Step 4: Creating replicated volume"
        Write-Host "================================="
        $createArgs = @(
            "--daemon-address", $NODE1_IPC,
            "volume", "create",
            "--name", $VOLUME_NAME,
            "--size", $VOLUME_SIZE,
            "--replicas", $REPLICAS.ToString()
        )
        Invoke-GalleonCommand -BinaryPath $binaryPath -Arguments $createArgs -Description "Creating replicated volume"
        Write-Host ""
        

        # Step 5: List volumes and parse volume ID
        Write-Info "Step 5: Listing volumes"
        Write-Host "====================="
        $listOutput = & $binaryPath --daemon-address $NODE1_IPC volume list
        Write-Host $listOutput
    $volumeId = ($listOutput | Select-String -Pattern '^[0-9a-fA-F-]{36} ' | ForEach-Object { $_.Line }) -replace '\s+', ' ' | ForEach-Object { ($_ -split ' ')[0] }
        if (-not $volumeId) {
            Write-Error "Failed to parse volume ID from volume list output"
            throw "Volume ID not found"
        }
        Write-Info "Parsed volume ID: $volumeId"

        # Step 6: Mount volume on Node 1
        Write-Info "Step 6: Mounting volume on Node 1"
        $mountPath1 = "$NODE1_STORAGE\mount1"
        New-Item -ItemType Directory -Force -Path $mountPath1 | Out-Null
    Invoke-GalleonCommand -BinaryPath $binaryPath -Arguments @("--daemon-address", $NODE1_IPC, "volume", "mount", $volumeId, $mountPath1, "--options", "rw") -Description "Mounting volume on Node 1"

        # Step 7: Mount volume on Node 2
        Write-Info "Step 7: Mounting volume on Node 2"
        $mountPath2 = "$NODE2_STORAGE\mount2"
        New-Item -ItemType Directory -Force -Path $mountPath2 | Out-Null
    Invoke-GalleonCommand -BinaryPath $binaryPath -Arguments @("--daemon-address", $NODE2_IPC, "volume", "mount", $volumeId, $mountPath2, "--options", "rw") -Description "Mounting volume on Node 2"
        Write-Host ""
        
        # Step 7: Check cluster status from Node 2
        Write-Info "Step 7: Checking cluster status from Node 2"
        Write-Host "==========================================="
        Invoke-GalleonCommand -BinaryPath $binaryPath -Arguments @("--daemon-address", $NODE2_IPC, "cluster", "status") -Description "Getting cluster status from Node 2"
        Write-Host ""
        
        # Step 8: List volumes from Node 2
        Write-Info "Step 8: Listing volumes from Node 2"
        Write-Host "=================================="
        Invoke-GalleonCommand -BinaryPath $binaryPath -Arguments @("--daemon-address", $NODE2_IPC, "volume", "list") -Description "Listing volumes from Node 2"
        Write-Host ""
        
        # Step 9: Show daemon status
        Write-Info "Step 9: Checking daemon status"
        Write-Host "============================="
        Write-Info "Node 1 daemon status:"
        Invoke-GalleonCommand -BinaryPath $binaryPath -Arguments @("--daemon-address", $NODE1_IPC, "daemon", "status") -Description "Getting daemon status from Node 1"
        Write-Host ""
        Write-Info "Node 2 daemon status:"
        Invoke-GalleonCommand -BinaryPath $binaryPath -Arguments @("--daemon-address", $NODE2_IPC, "daemon", "status") -Description "Getting daemon status from Node 2"
        Write-Host ""
        
        Write-Success "Two-node cluster test completed successfully!"
        Write-Info "Check node1.log and node2.log for daemon output"
        
        # Wait a moment before cleanup
        Write-Info "Test completed. Cleaning up in 5 seconds..."
        Start-Sleep -Seconds 5
    }
    catch {
        Write-Error "Test failed: $_"
        Write-Info "Check log files for details:"
        Write-Info "  node1.log, node1_error.log"
        Write-Info "  node2.log, node2_error.log"
    }
    finally {
        Cleanup
    }
}

# Run main function
Main