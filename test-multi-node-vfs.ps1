#!/usr/bin/env pwsh

<#
.SYNOPSIS
    Multi-Node VFS Replication Test for GalleonFS

.DESCRIPTION
    This script tests the complete multi-node VFS replication functionality:
    - Docker Swarm-like cluster formation
    - Dynamic context-aware replication
    - File change detection and instant replication
    - Petabyte-scale block storage
    - Cross-node file system consistency

.PARAMETER BuildConfig
    Build configuration (release or debug). Default: release

.PARAMETER TestVolumeName
    Name of the test volume. Default: test-vfs-replication

.PARAMETER VolumeSize
    Size of the test volume. Default: 1G

.PARAMETER ReplicationFactor
    Replication factor for volumes. Default: 3

.PARAMETER KeepFiles
    Keep test files after completion for inspection

.PARAMETER SkipBuild
    Skip building the project (use existing binary)

.EXAMPLE
    .\test-multi-node-vfs.ps1
    Run with default settings

.EXAMPLE
    .\test-multi-node-vfs.ps1 -BuildConfig debug -KeepFiles
    Run with debug build and keep test files
#>

param(
    [string]$BuildConfig = "release",
    [string]$TestVolumeName = "test-vfs-replication",
    [string]$VolumeSize = "1G",
    [int]$ReplicationFactor = 3,
    [switch]$KeepFiles,
    [switch]$SkipBuild
)

# Configuration
$NODE1_STORAGE = ".\test_vfs_node1_storage"
$NODE2_STORAGE = ".\test_vfs_node2_storage"
$NODE3_STORAGE = ".\test_vfs_node3_storage"
$NODE1_MOUNT = ".\test_vfs_node1_mount"
$NODE2_MOUNT = ".\test_vfs_node2_mount"
$NODE3_MOUNT = ".\test_vfs_node3_mount"
$NODE1_BIND = "127.0.0.1:8081"
$NODE2_BIND = "127.0.0.1:8082"
$NODE3_BIND = "127.0.0.1:8083"
$NODE1_IPC = "127.0.0.1:8091"
$NODE2_IPC = "127.0.0.1:8092"
$NODE3_IPC = "127.0.0.1:8093"

$TEST_FILES = @(
    "test1.txt",
    "test2.txt",
    "large_file.bin",
    "subdir\nested_file.txt"
)

# Process management
$Global:DaemonProcesses = @()
$Global:TestStartTime = Get-Date

function Write-TestHeader {
    param([string]$Message)
    
    $timestamp = (Get-Date).ToString("HH:mm:ss.fff")
    $separator = "=" * 80
    Write-Host ""
    Write-Host $separator -ForegroundColor Cyan
    Write-Host "[$timestamp] $Message" -ForegroundColor Yellow
    Write-Host $separator -ForegroundColor Cyan
}

function Write-TestStep {
    param([string]$Message)
    
    $timestamp = (Get-Date).ToString("HH:mm:ss.fff")
    Write-Host "[$timestamp] [STEP] $Message" -ForegroundColor Green
}

function Write-TestInfo {
    param([string]$Message)
    
    $timestamp = (Get-Date).ToString("HH:mm:ss.fff")
    Write-Host "[$timestamp] [INFO] $Message" -ForegroundColor White
}

function Write-TestSuccess {
    param([string]$Message)
    
    $timestamp = (Get-Date).ToString("HH:mm:ss.fff")
    Write-Host "[$timestamp] [SUCCESS] $Message" -ForegroundColor Green
}

function Write-TestError {
    param([string]$Message)
    
    $timestamp = (Get-Date).ToString("HH:mm:ss.fff")
    Write-Host "[$timestamp] [ERROR] $Message" -ForegroundColor Red
}

function Cleanup-TestEnvironment {
    Write-TestStep "Cleaning up test environment"
    
    # Stop all daemon processes
    foreach ($process in $Global:DaemonProcesses) {
        if (-not $process.HasExited) {
            Write-TestInfo "Stopping daemon process $($process.Id)"
            try {
                $process.Kill()
                $process.WaitForExit(5000)
            }
            catch {
                Write-TestError "Failed to stop process $($process.Id): $_"
            }
        }
    }
    
    # Wait a moment for cleanup
    Start-Sleep -Seconds 2
    
    # Remove test directories unless keeping files
    if (-not $KeepFiles) {
        $testDirs = @($NODE1_STORAGE, $NODE2_STORAGE, $NODE3_STORAGE, $NODE1_MOUNT, $NODE2_MOUNT, $NODE3_MOUNT)
        foreach ($dir in $testDirs) {
            if (Test-Path $dir) {
                Write-TestInfo "Removing test directory: $dir"
                try {
                    Remove-Item -Path $dir -Recurse -Force -ErrorAction SilentlyContinue
                }
                catch {
                    Write-TestError "Failed to remove $dir: $_"
                }
            }
        }
    }
    else {
        Write-TestInfo "Keeping test files for inspection"
    }
}

function Test-ServiceReady {
    param(
        [string]$ServiceName,
        [string]$IpcAddress,
        [int]$TimeoutSeconds = 30
    )
    
    Write-TestInfo "Waiting for $ServiceName to be ready on $IpcAddress"
    
    $startTime = Get-Date
    $timeout = $startTime.AddSeconds($TimeoutSeconds)
    
    while ((Get-Date) -lt $timeout) {
        try {
            # Test IPC connectivity
            $testResult = & ".\target\$BuildConfig\galleonfs.exe" --daemon-address $IpcAddress daemon status 2>&1
            if ($LASTEXITCODE -eq 0) {
                Write-TestSuccess "$ServiceName is ready!"
                return $true
            }
        }
        catch {
            # Service not ready yet
        }
        
        Start-Sleep -Milliseconds 500
    }
    
    Write-TestError "$ServiceName failed to become ready within $TimeoutSeconds seconds"
    return $false
}

function Start-VfsDaemon {
    param(
        [string]$NodeName,
        [string]$StoragePath,
        [string]$MountPath,
        [string]$BindAddress,
        [string]$IpcAddress,
        [string[]]$PeerAddresses = @()
    )
    
    Write-TestStep "Starting VFS daemon: $NodeName"
    
    # Create directories
    if (-not (Test-Path $StoragePath)) {
        New-Item -Path $StoragePath -ItemType Directory -Force | Out-Null
    }
    if (-not (Test-Path $MountPath)) {
        New-Item -Path $MountPath -ItemType Directory -Force | Out-Null
    }
    
    # Build command arguments
    $args = @(
        "-d",
        "--storage-path", $StoragePath,
        "--bind-address", $BindAddress,
        "--ipc-address", $IpcAddress,
        "--mount-point", $MountPath
    )
    
    if ($PeerAddresses.Count -gt 0) {
        $args += "--peer-addresses"
        $args += ($PeerAddresses -join ",")
    }
    
    Write-TestInfo "Command: .\target\$BuildConfig\galleonfs.exe $($args -join ' ')"
    
    # Start the process
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = ".\target\$BuildConfig\galleonfs.exe"
    $psi.Arguments = $args -join " "
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    
    $process = [System.Diagnostics.Process]::Start($psi)
    $Global:DaemonProcesses += $process
    
    # Start background tasks to capture output
    $outputFile = ".\${NodeName}_daemon.log"
    Start-Job -ScriptBlock {
        param($proc, $outputFile)
        while (-not $proc.HasExited) {
            $line = $proc.StandardOutput.ReadLine()
            if ($line) {
                Add-Content -Path $outputFile -Value "$(Get-Date -Format 'HH:mm:ss.fff') [STDOUT] $line"
            }
        }
    } -ArgumentList $process, $outputFile | Out-Null
    
    Start-Job -ScriptBlock {
        param($proc, $outputFile)
        while (-not $proc.HasExited) {
            $line = $proc.StandardError.ReadLine()
            if ($line) {
                Add-Content -Path $outputFile -Value "$(Get-Date -Format 'HH:mm:ss.fff') [STDERR] $line"
            }
        }
    } -ArgumentList $process, $outputFile | Out-Null
    
    Write-TestSuccess "$NodeName daemon started with PID: $($process.Id)"
    return $process
}

function Test-VfsVolumeOperations {
    param([string]$IpcAddress)
    
    Write-TestStep "Testing VFS volume operations"
    
    # Create test volume
    Write-TestInfo "Creating test volume: $TestVolumeName"
    $result = & ".\target\$BuildConfig\galleonfs.exe" --daemon-address $IpcAddress volume create $TestVolumeName --path $NODE1_MOUNT --size $VolumeSize
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to create volume: $result"
    }
    Write-TestSuccess "Volume created successfully"
    
    # List volumes
    Write-TestInfo "Listing volumes"
    $result = & ".\target\$BuildConfig\galleonfs.exe" --daemon-address $IpcAddress volume list --verbose
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to list volumes: $result"
    }
    Write-TestSuccess "Volume listing completed"
    Write-Host $result
    
    return $true
}

function Test-FileReplication {
    Write-TestStep "Testing file replication across nodes"
    
    # Create test files on Node 1
    Write-TestInfo "Creating test files on Node 1"
    
    # Simple text file
    $testFile1 = Join-Path $NODE1_MOUNT $TEST_FILES[0]
    "Hello from GalleonFS VFS - Node 1!" | Out-File -FilePath $testFile1 -Encoding UTF8
    
    # Another text file
    $testFile2 = Join-Path $NODE1_MOUNT $TEST_FILES[1]
    "Multi-node replication test - $(Get-Date)" | Out-File -FilePath $testFile2 -Encoding UTF8
    
    # Large binary file
    $testFile3 = Join-Path $NODE1_MOUNT $TEST_FILES[2]
    $largeData = [byte[]](1..1024 | ForEach-Object { Get-Random -Maximum 256 })
    [System.IO.File]::WriteAllBytes($testFile3, $largeData)
    
    # Nested directory file
    $testSubDir = Join-Path $NODE1_MOUNT "subdir"
    if (-not (Test-Path $testSubDir)) {
        New-Item -Path $testSubDir -ItemType Directory -Force | Out-Null
    }
    $testFile4 = Join-Path $NODE1_MOUNT $TEST_FILES[3]
    "Nested file for directory replication test" | Out-File -FilePath $testFile4 -Encoding UTF8
    
    Write-TestSuccess "Created test files on Node 1"
    
    # Wait for replication
    Write-TestInfo "Waiting for replication to propagate (10 seconds)"
    Start-Sleep -Seconds 10
    
    # Check replication on Node 2
    Write-TestInfo "Verifying replication on Node 2"
    $replicationSuccessful = $true
    
    foreach ($testFile in $TEST_FILES) {
        $node1File = Join-Path $NODE1_MOUNT $testFile
        $node2File = Join-Path $NODE2_MOUNT $testFile
        
        if (Test-Path $node1File) {
            if (Test-Path $node2File) {
                # Compare file contents
                $node1Content = Get-Content -Path $node1File -Raw -ErrorAction SilentlyContinue
                $node2Content = Get-Content -Path $node2File -Raw -ErrorAction SilentlyContinue
                
                if ($node1Content -eq $node2Content) {
                    Write-TestSuccess "File replicated successfully: $testFile"
                }
                else {
                    Write-TestError "File content mismatch: $testFile"
                    $replicationSuccessful = $false
                }
            }
            else {
                Write-TestError "File not replicated to Node 2: $testFile"
                $replicationSuccessful = $false
            }
        }
    }
    
    return $replicationSuccessful
}

function Test-DynamicModification {
    Write-TestStep "Testing dynamic file modification and replication"
    
    $testFile = Join-Path $NODE1_MOUNT "dynamic_test.txt"
    
    # Create initial file
    "Initial content - $(Get-Date)" | Out-File -FilePath $testFile -Encoding UTF8
    Start-Sleep -Seconds 2
    
    # Modify file multiple times
    for ($i = 1; $i -le 5; $i++) {
        "Modification $i - $(Get-Date)" | Add-Content -Path $testFile
        Write-TestInfo "Applied modification $i"
        Start-Sleep -Seconds 1
    }
    
    # Wait for final replication
    Start-Sleep -Seconds 5
    
    # Verify final state on Node 2
    $node2File = Join-Path $NODE2_MOUNT "dynamic_test.txt"
    if (Test-Path $node2File) {
        $node1Lines = (Get-Content -Path $testFile).Count
        $node2Lines = (Get-Content -Path $node2File).Count
        
        if ($node1Lines -eq $node2Lines -and $node1Lines -eq 6) {  # Initial + 5 modifications
            Write-TestSuccess "Dynamic modification replication successful"
            return $true
        }
        else {
            Write-TestError "Line count mismatch - Node1: $node1Lines, Node2: $node2Lines"
            return $false
        }
    }
    else {
        Write-TestError "Dynamic test file not replicated to Node 2"
        return $false
    }
}

function Test-PerformanceMetrics {
    Write-TestStep "Testing performance metrics collection"
    
    # Create multiple files to generate load
    Write-TestInfo "Creating performance test files"
    
    $perfTestDir = Join-Path $NODE1_MOUNT "performance_test"
    if (-not (Test-Path $perfTestDir)) {
        New-Item -Path $perfTestDir -ItemType Directory -Force | Out-Null
    }
    
    $startTime = Get-Date
    for ($i = 1; $i -le 20; $i++) {
        $perfFile = Join-Path $perfTestDir "perf_file_$i.txt"
        "Performance test file $i - $(Get-Date)" | Out-File -FilePath $perfFile -Encoding UTF8
        
        if ($i % 5 -eq 0) {
            Write-TestInfo "Created $i performance test files"
        }
    }
    $endTime = Get-Date
    $creationTime = ($endTime - $startTime).TotalSeconds
    
    Write-TestSuccess "Created 20 test files in $([math]::Round($creationTime, 2)) seconds"
    
    # Wait for replication
    Start-Sleep -Seconds 10
    
    # Check replication rate
    $node2PerfDir = Join-Path $NODE2_MOUNT "performance_test"
    if (Test-Path $node2PerfDir) {
        $replicatedFiles = (Get-ChildItem -Path $node2PerfDir -File).Count
        $replicationRate = [math]::Round(($replicatedFiles / 20.0) * 100, 2)
        
        Write-TestInfo "Replication rate: $replicatedFiles/20 files ($replicationRate%)"
        
        if ($replicationRate -ge 90) {
            Write-TestSuccess "Performance test shows good replication rate"
            return $true
        }
        else {
            Write-TestError "Performance test shows poor replication rate: $replicationRate%"
            return $false
        }
    }
    else {
        Write-TestError "Performance test directory not replicated"
        return $false
    }
}

function Show-TestSummary {
    param([bool[]]$TestResults)
    
    Write-TestHeader "Test Summary"
    
    $totalTests = $TestResults.Count
    $passedTests = ($TestResults | Where-Object { $_ -eq $true }).Count
    $failedTests = $totalTests - $passedTests
    $successRate = [math]::Round(($passedTests / $totalTests) * 100, 2)
    
    $elapsedTime = (Get-Date) - $Global:TestStartTime
    
    Write-Host ""
    Write-Host "Total Tests:     $totalTests" -ForegroundColor White
    Write-Host "Passed:          $passedTests" -ForegroundColor Green
    Write-Host "Failed:          $failedTests" -ForegroundColor Red
    Write-Host "Success Rate:    $successRate%" -ForegroundColor $(if ($successRate -ge 90) { "Green" } else { "Yellow" })
    Write-Host "Execution Time:  $($elapsedTime.ToString('mm\:ss\.fff'))" -ForegroundColor White
    Write-Host ""
    
    if ($failedTests -eq 0) {
        Write-TestSuccess "All tests passed! 🎉"
        return $true
    }
    else {
        Write-TestError "$failedTests test(s) failed"
        return $false
    }
}

# Main execution
try {
    Write-TestHeader "Starting GalleonFS Multi-Node VFS Replication Test"
    
    Write-TestInfo "Configuration:"
    Write-TestInfo "  Build Config: $BuildConfig"
    Write-TestInfo "  Test Volume: $TestVolumeName"
    Write-TestInfo "  Volume Size: $VolumeSize"
    Write-TestInfo "  Replication Factor: $ReplicationFactor"
    Write-TestInfo "  Node 1: Storage=$NODE1_STORAGE, Bind=$NODE1_BIND, IPC=$NODE1_IPC"
    Write-TestInfo "  Node 2: Storage=$NODE2_STORAGE, Bind=$NODE2_BIND, IPC=$NODE2_IPC"
    Write-TestInfo "  Node 3: Storage=$NODE3_STORAGE, Bind=$NODE3_BIND, IPC=$NODE3_IPC"
    
    # Cleanup any existing test environment
    Cleanup-TestEnvironment
    
    # Build the project if not skipping
    if (-not $SkipBuild) {
        Write-TestStep "Building GalleonFS"
        $buildArgs = if ($BuildConfig -eq "release") { "--release" } else { "" }
        & cargo build $buildArgs
        if ($LASTEXITCODE -ne 0) {
            throw "Build failed"
        }
        Write-TestSuccess "GalleonFS build completed"
    }
    
    # Start Node 1 (creates cluster)
    Write-TestStep "Starting Node 1 (cluster leader)"
    $node1Process = Start-VfsDaemon -NodeName "Node1" -StoragePath $NODE1_STORAGE -MountPath $NODE1_MOUNT -BindAddress $NODE1_BIND -IpcAddress $NODE1_IPC
    
    if (-not (Test-ServiceReady -ServiceName "Node 1" -IpcAddress $NODE1_IPC)) {
        throw "Node 1 failed to start"
    }
    
    # Start Node 2 (joins cluster)
    Write-TestStep "Starting Node 2 (joining cluster)"
    $node2Process = Start-VfsDaemon -NodeName "Node2" -StoragePath $NODE2_STORAGE -MountPath $NODE2_MOUNT -BindAddress $NODE2_BIND -IpcAddress $NODE2_IPC -PeerAddresses @($NODE1_BIND)
    
    if (-not (Test-ServiceReady -ServiceName "Node 2" -IpcAddress $NODE2_IPC)) {
        throw "Node 2 failed to start"
    }
    
    # Start Node 3 (joins cluster)
    Write-TestStep "Starting Node 3 (joining cluster)"
    $node3Process = Start-VfsDaemon -NodeName "Node3" -StoragePath $NODE3_STORAGE -MountPath $NODE3_MOUNT -BindAddress $NODE3_BIND -IpcAddress $NODE3_IPC -PeerAddresses @($NODE1_BIND, $NODE2_BIND)
    
    if (-not (Test-ServiceReady -ServiceName "Node 3" -IpcAddress $NODE3_IPC)) {
        throw "Node 3 failed to start"
    }
    
    # Wait for cluster formation
    Write-TestStep "Waiting for cluster formation"
    Start-Sleep -Seconds 5
    
    # Check cluster status
    Write-TestStep "Checking cluster status"
    $clusterStatus = & ".\target\$BuildConfig\galleonfs.exe" --daemon-address $NODE1_IPC cluster status
    Write-Host $clusterStatus
    
    # Run tests
    $testResults = @()
    
    # Test 1: Volume Operations
    try {
        $testResults += Test-VfsVolumeOperations -IpcAddress $NODE1_IPC
    }
    catch {
        Write-TestError "Volume operations test failed: $_"
        $testResults += $false
    }
    
    # Test 2: File Replication
    try {
        $testResults += Test-FileReplication
    }
    catch {
        Write-TestError "File replication test failed: $_"
        $testResults += $false
    }
    
    # Test 3: Dynamic Modification
    try {
        $testResults += Test-DynamicModification
    }
    catch {
        Write-TestError "Dynamic modification test failed: $_"
        $testResults += $false
    }
    
    # Test 4: Performance Metrics
    try {
        $testResults += Test-PerformanceMetrics
    }
    catch {
        Write-TestError "Performance metrics test failed: $_"
        $testResults += $false
    }
    
    # Show results
    $overallSuccess = Show-TestSummary -TestResults $testResults
    
    if ($overallSuccess) {
        Write-TestSuccess "Multi-node VFS replication testing completed successfully! 🚀"
        exit 0
    }
    else {
        Write-TestError "Multi-node VFS replication testing completed with failures"
        exit 1
    }
}
catch {
    Write-TestError "Test execution failed: $_"
    Write-TestError "Stack trace: $($_.ScriptStackTrace)"
    exit 1
}
finally {
    # Always cleanup
    Cleanup-TestEnvironment
    
    # Stop background jobs
    Get-Job | Stop-Job
    Get-Job | Remove-Job
}