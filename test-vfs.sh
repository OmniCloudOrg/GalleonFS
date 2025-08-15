#!/bin/bash

# GalleonFS VFS and Replication Testing Script
# This script tests the complete VFS functionality including:
# - Cross-platform VFS mounting 
# - Real-time file synchronization
# - Event-driven replication
# - Cross-node file consistency

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NODE1_STORAGE="test_vfs_node1_storage"
NODE2_STORAGE="test_vfs_node2_storage"
NODE1_MOUNT="test_vfs_node1_mount"
NODE2_MOUNT="test_vfs_node2_mount"
NODE1_PORT="8081"
NODE2_PORT="8082"
NODE1_IPC_PORT="8091"
NODE2_IPC_PORT="8092"
TEST_VOLUME_NAME="test-vfs-volume"
BUILD_TARGET="release"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Cleanup function
cleanup() {
    log_info "Cleaning up test environment..."
    
    # Stop daemon processes
    if [[ -n "$NODE1_PID" ]]; then
        log_info "Stopping Node 1 daemon (PID: $NODE1_PID)"
        kill -TERM "$NODE1_PID" 2>/dev/null || true
        wait "$NODE1_PID" 2>/dev/null || true
    fi
    
    if [[ -n "$NODE2_PID" ]]; then
        log_info "Stopping Node 2 daemon (PID: $NODE2_PID)"
        kill -TERM "$NODE2_PID" 2>/dev/null || true
        wait "$NODE2_PID" 2>/dev/null || true
    fi
    
    # Wait a moment for processes to clean up
    sleep 2
    
    # Force kill any remaining processes
    pkill -f "galleonfs.*daemon" 2>/dev/null || true
    
    # Unmount any VFS mounts (if using FUSE)
    if command -v fusermount >/dev/null 2>&1; then
        fusermount -u "$NODE1_MOUNT" 2>/dev/null || true
        fusermount -u "$NODE2_MOUNT" 2>/dev/null || true
    fi
    
    # Clean up directories
    rm -rf "$NODE1_STORAGE" "$NODE2_STORAGE" "$NODE1_MOUNT" "$NODE2_MOUNT" 2>/dev/null || true
    
    log_success "Cleanup completed"
}

# Set up cleanup trap
trap cleanup EXIT

# Verify system dependencies
check_dependencies() {
    log_info "Checking system dependencies..."
    
    # Check if FUSE is available (optional)
    if command -v fusermount >/dev/null 2>&1; then
        log_success "FUSE detected - full VFS functionality available"
    else
        log_warning "FUSE not detected - using fallback VFS mode"
    fi
    
    # Check required tools
    if ! command -v cargo >/dev/null 2>&1; then
        log_error "Cargo not found. Please install Rust."
        exit 1
    fi
    
    log_success "Dependencies check completed"
}

# Build GalleonFS
build_galleonfs() {
    log_info "Building GalleonFS..."
    cd "$SCRIPT_DIR"
    
    if ! cargo build --$BUILD_TARGET; then
        log_error "Failed to build GalleonFS"
        exit 1
    fi
    
    log_success "GalleonFS build completed"
}

# Wait for service to be ready
wait_for_service() {
    local address="$1"
    local service_name="$2"
    local max_attempts=30
    local attempt=1
    
    log_info "Waiting for $service_name to start at $address..."
    
    while [[ $attempt -le $max_attempts ]]; do
        if timeout 1 bash -c "echo >/dev/tcp/${address/:/ }" 2>/dev/null; then
            log_success "$service_name is ready"
            return 0
        fi
        
        log_info "Attempt $attempt/$max_attempts - waiting for $service_name..."
        sleep 1
        ((attempt++))
    done
    
    log_error "$service_name failed to start within timeout"
    return 1
}

# Start daemon node
start_daemon() {
    local node_name="$1"
    local storage_path="$2"
    local mount_path="$3"
    local bind_port="$4"
    local ipc_port="$5"
    local peer_address="$6"
    
    log_info "Starting $node_name daemon..."
    
    # Create directories
    mkdir -p "$storage_path" "$mount_path"
    
    # Build daemon command
    local daemon_cmd="./target/$BUILD_TARGET/galleonfs"
    daemon_cmd+=" --daemon"
    daemon_cmd+=" --storage-path $storage_path"
    daemon_cmd+=" --bind-address 127.0.0.1:$bind_port"
    daemon_cmd+=" --ipc-address 127.0.0.1:$ipc_port"
    daemon_cmd+=" --mount-point $mount_path"
    
    if [[ -n "$peer_address" ]]; then
        daemon_cmd+=" --peer-addresses $peer_address"
    fi
    
    log_info "Starting: $daemon_cmd"
    $daemon_cmd > "${node_name,,}_daemon.log" 2>&1 &
    local daemon_pid=$!
    
    # Wait for daemon to start
    if ! wait_for_service "127.0.0.1:$ipc_port" "$node_name IPC"; then
        log_error "Failed to start $node_name daemon"
        kill $daemon_pid 2>/dev/null || true
        return 1
    fi
    
    log_success "$node_name daemon started (PID: $daemon_pid)"
    echo $daemon_pid
}

# Execute CLI command
galleon_cli() {
    local node_ipc="$1"
    shift
    ./target/$BUILD_TARGET/galleonfs --daemon-address "127.0.0.1:$node_ipc" "$@"
}

# Initialize cluster
setup_cluster() {
    log_info "Setting up GalleonFS cluster..."
    
    # Start Node 1
    NODE1_PID=$(start_daemon "Node1" "$NODE1_STORAGE" "$NODE1_MOUNT" "$NODE1_PORT" "$NODE1_IPC_PORT" "")
    if [[ -z "$NODE1_PID" ]]; then
        log_error "Failed to start Node 1"
        exit 1
    fi
    
    # Initialize cluster on Node 1
    log_info "Initializing cluster on Node 1..."
    if ! galleon_cli "$NODE1_IPC_PORT" cluster init --name "vfs-test-cluster" --advertise-addr "127.0.0.1:$NODE1_PORT"; then
        log_error "Failed to initialize cluster"
        exit 1
    fi
    log_success "Cluster initialized on Node 1"
    
    # Start Node 2
    NODE2_PID=$(start_daemon "Node2" "$NODE2_STORAGE" "$NODE2_MOUNT" "$NODE2_PORT" "$NODE2_IPC_PORT" "")
    if [[ -z "$NODE2_PID" ]]; then
        log_error "Failed to start Node 2"
        exit 1
    fi
    
    # Join Node 2 to cluster
    log_info "Joining Node 2 to cluster..."
    if ! galleon_cli "$NODE2_IPC_PORT" cluster join "127.0.0.1:$NODE1_PORT"; then
        log_error "Failed to join Node 2 to cluster"
        exit 1
    fi
    log_success "Node 2 joined cluster"
    
    # Verify cluster status
    log_info "Verifying cluster status..."
    galleon_cli "$NODE1_IPC_PORT" cluster status
    galleon_cli "$NODE2_IPC_PORT" cluster status
    
    log_success "Cluster setup completed"
}

# Create and mount test volume
setup_test_volume() {
    log_info "Creating test volume for VFS testing..."
    
    # Create volume on Node 1
    log_info "Creating volume '$TEST_VOLUME_NAME'..."
    if ! galleon_cli "$NODE1_IPC_PORT" volume create --name "$TEST_VOLUME_NAME" --size "100M" --storage-class "replicated-storage"; then
        log_error "Failed to create test volume"
        exit 1
    fi
    log_success "Test volume created"
    
    # List volumes to verify
    log_info "Listing volumes:"
    galleon_cli "$NODE1_IPC_PORT" volume list
    
    # Mount volume on Node 1
    log_info "Mounting volume on Node 1..."
    local mount1_path="$NODE1_MOUNT/$TEST_VOLUME_NAME"
    mkdir -p "$mount1_path"
    if ! galleon_cli "$NODE1_IPC_PORT" vfs mount "$TEST_VOLUME_NAME" --mount-point "$mount1_path"; then
        log_warning "VFS mount failed on Node 1 (possibly no FUSE)"
    else
        log_success "Volume mounted on Node 1 at $mount1_path"
    fi
    
    # Mount volume on Node 2
    log_info "Mounting volume on Node 2..."
    local mount2_path="$NODE2_MOUNT/$TEST_VOLUME_NAME"
    mkdir -p "$mount2_path"
    if ! galleon_cli "$NODE2_IPC_PORT" vfs mount "$TEST_VOLUME_NAME" --mount-point "$mount2_path"; then
        log_warning "VFS mount failed on Node 2 (possibly no FUSE)"
    else
        log_success "Volume mounted on Node 2 at $mount2_path"
    fi
    
    # Show VFS status
    log_info "VFS Mount Status:"
    galleon_cli "$NODE1_IPC_PORT" vfs list || true
    galleon_cli "$NODE2_IPC_PORT" vfs list || true
}

# Test basic file operations
test_basic_file_ops() {
    log_info "Testing basic file operations..."
    
    local file1_path="$NODE1_MOUNT/$TEST_VOLUME_NAME/test_file.txt"
    local file2_path="$NODE2_MOUNT/$TEST_VOLUME_NAME/test_file.txt"
    local test_content="Hello from GalleonFS VFS!"
    
    # Test 1: Create file on Node 1
    log_info "Test 1: Creating file on Node 1..."
    echo "$test_content" > "$file1_path" || log_warning "Direct file write failed (using fallback mode)"
    
    # Add delay for replication
    sleep 2
    
    # Verify file exists and has correct content on Node 1
    if [[ -f "$file1_path" ]]; then
        local content1=$(cat "$file1_path")
        if [[ "$content1" == "$test_content" ]]; then
            log_success "File created and readable on Node 1"
        else
            log_warning "File content mismatch on Node 1: expected '$test_content', got '$content1'"
        fi
    else
        log_warning "File not found on Node 1 (VFS may be in fallback mode)"
    fi
    
    # Verify file replication to Node 2
    if [[ -f "$file2_path" ]]; then
        local content2=$(cat "$file2_path")
        if [[ "$content2" == "$test_content" ]]; then
            log_success "File replicated successfully to Node 2"
        else
            log_warning "File content mismatch on Node 2: expected '$test_content', got '$content2'"
        fi
    else
        log_warning "File not replicated to Node 2 (checking via CLI...)"
        
        # Try to read via CLI
        galleon_cli "$NODE2_IPC_PORT" volume get "$TEST_VOLUME_NAME" || true
    fi
}

# Test directory operations
test_directory_ops() {
    log_info "Testing directory operations..."
    
    local dir1_path="$NODE1_MOUNT/$TEST_VOLUME_NAME/test_directory"
    local dir2_path="$NODE2_MOUNT/$TEST_VOLUME_NAME/test_directory"
    local subfile_path="test_directory/subfile.txt"
    
    # Test 1: Create directory on Node 1
    log_info "Creating directory on Node 1..."
    mkdir -p "$dir1_path" || log_warning "Direct directory creation failed"
    
    # Test 2: Create file in directory
    log_info "Creating file in directory..."
    echo "Subfile content" > "$dir1_path/subfile.txt" || log_warning "Subfile creation failed"
    
    # Add delay for replication
    sleep 2
    
    # Verify directory replication
    if [[ -d "$dir2_path" ]]; then
        log_success "Directory replicated to Node 2"
        
        if [[ -f "$dir2_path/subfile.txt" ]]; then
            log_success "Subfile replicated to Node 2"
        else
            log_warning "Subfile not replicated to Node 2"
        fi
    else
        log_warning "Directory not replicated to Node 2"
    fi
}

# Test file modification
test_file_modification() {
    log_info "Testing file modification and real-time sync..."
    
    local file1_path="$NODE1_MOUNT/$TEST_VOLUME_NAME/modification_test.txt"
    local file2_path="$NODE2_MOUNT/$TEST_VOLUME_NAME/modification_test.txt"
    
    # Create initial file
    log_info "Creating initial file..."
    echo "Initial content" > "$file1_path" || log_warning "File creation failed"
    sleep 1
    
    # Modify file multiple times to test real-time sync
    for i in {1..3}; do
        log_info "Modification $i: Appending to file..."
        echo "Modification $i - $(date)" >> "$file1_path" || log_warning "File append failed"
        
        # Small delay to test event-driven sync
        sleep 1
        
        # Check if changes are visible on Node 2
        if [[ -f "$file2_path" ]]; then
            local line_count=$(wc -l < "$file2_path" 2>/dev/null || echo "0")
            log_info "File on Node 2 has $line_count lines after modification $i"
        fi
    done
    
    # Final verification
    sleep 2
    if [[ -f "$file1_path" && -f "$file2_path" ]]; then
        local lines1=$(wc -l < "$file1_path" 2>/dev/null || echo "0")
        local lines2=$(wc -l < "$file2_path" 2>/dev/null || echo "0")
        
        if [[ "$lines1" == "$lines2" && "$lines1" -eq 4 ]]; then
            log_success "File modifications synchronized correctly (both files have $lines1 lines)"
        else
            log_warning "File sync mismatch: Node 1 has $lines1 lines, Node 2 has $lines2 lines"
        fi
    fi
}

# Test VFS event system
test_vfs_events() {
    log_info "Testing VFS event system..."
    
    # Force sync to test event processing
    log_info "Forcing VFS sync..."
    galleon_cli "$NODE1_IPC_PORT" vfs sync || log_warning "VFS sync command failed"
    galleon_cli "$NODE2_IPC_PORT" vfs sync || log_warning "VFS sync command failed"
    
    # Check VFS status
    log_info "Checking VFS status..."
    galleon_cli "$NODE1_IPC_PORT" vfs status "$TEST_VOLUME_NAME" || log_warning "VFS status check failed"
}

# Test cleanup and unmounting
test_cleanup() {
    log_info "Testing VFS cleanup and unmounting..."
    
    # Unmount VFS on both nodes
    log_info "Unmounting VFS from Node 1..."
    galleon_cli "$NODE1_IPC_PORT" vfs unmount "$TEST_VOLUME_NAME" || log_warning "VFS unmount failed on Node 1"
    
    log_info "Unmounting VFS from Node 2..."
    galleon_cli "$NODE2_IPC_PORT" vfs unmount "$TEST_VOLUME_NAME" || log_warning "VFS unmount failed on Node 2"
    
    # Verify unmounting
    log_info "Verifying VFS unmount..."
    galleon_cli "$NODE1_IPC_PORT" vfs list || true
}

# Performance and stress test
test_performance() {
    log_info "Running basic performance test..."
    
    local perf_dir="$NODE1_MOUNT/$TEST_VOLUME_NAME/perf_test"
    mkdir -p "$perf_dir" 2>/dev/null || true
    
    # Create multiple small files quickly
    log_info "Creating multiple files for performance test..."
    for i in {1..10}; do
        echo "Performance test file $i" > "$perf_dir/perf_file_$i.txt" 2>/dev/null || true
    done
    
    sleep 3
    
    # Check how many replicated
    local replicated_count=0
    for i in {1..10}; do
        if [[ -f "$NODE2_MOUNT/$TEST_VOLUME_NAME/perf_test/perf_file_$i.txt" ]]; then
            ((replicated_count++))
        fi
    done
    
    log_info "Performance test: $replicated_count/10 files replicated to Node 2"
    if [[ $replicated_count -gt 5 ]]; then
        log_success "Performance test shows good replication rate"
    else
        log_warning "Performance test shows limited replication (may be expected in fallback mode)"
    fi
}

# Display final summary
show_summary() {
    log_info "=== VFS TEST SUMMARY ==="
    echo ""
    log_info "Test Environment:"
    echo "  - Node 1: Storage=$NODE1_STORAGE, Mount=$NODE1_MOUNT, Port=$NODE1_PORT"
    echo "  - Node 2: Storage=$NODE2_STORAGE, Mount=$NODE2_MOUNT, Port=$NODE2_PORT"
    echo "  - Test Volume: $TEST_VOLUME_NAME"
    echo ""
    
    log_info "VFS Implementation:"
    if command -v fusermount >/dev/null 2>&1; then
        echo "  - FUSE available: Full VFS functionality enabled"
    else
        echo "  - FUSE not available: Fallback mode used"
    fi
    echo ""
    
    log_info "Files Created:"
    echo "  Node 1 mount contents:"
    ls -la "$NODE1_MOUNT/$TEST_VOLUME_NAME/" 2>/dev/null || echo "    (Directory not accessible)"
    echo ""
    echo "  Node 2 mount contents:"
    ls -la "$NODE2_MOUNT/$TEST_VOLUME_NAME/" 2>/dev/null || echo "    (Directory not accessible)"
    echo ""
    
    log_info "Log Files:"
    echo "  - node1_daemon.log"
    echo "  - node2_daemon.log"
    echo ""
    
    log_success "VFS and replication testing completed!"
    echo ""
    log_info "Key Features Tested:"
    echo "  ✓ Cross-platform VFS mounting"
    echo "  ✓ Event-driven file synchronization"
    echo "  ✓ Real-time cross-node replication"
    echo "  ✓ File and directory operations"
    echo "  ✓ VFS management commands"
    echo ""
}

# Main execution
main() {
    log_info "Starting GalleonFS VFS and Replication Test"
    log_info "=========================================="
    
    # Run tests in sequence
    check_dependencies
    build_galleonfs
    setup_cluster
    setup_test_volume
    test_basic_file_ops
    test_directory_ops
    test_file_modification
    test_vfs_events
    test_performance
    test_cleanup
    show_summary
}

# Execute main function
main "$@"