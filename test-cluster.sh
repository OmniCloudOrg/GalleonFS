#!/bin/bash

# GalleonFS Two-Node Cluster Test Script
# This script sets up a two-node GalleonFS cluster and creates a replicated volume

set -e  # Exit on any error

# Configuration
NODE1_STORAGE="./test_node1_storage"
NODE2_STORAGE="./test_node2_storage"
NODE1_BIND="127.0.0.1:8081"
NODE2_BIND="127.0.0.1:8082"
NODE1_IPC="127.0.0.1:8091"
NODE2_IPC="127.0.0.1:8092"
VOLUME_NAME="test-replicated-volume"
VOLUME_SIZE="1G"
REPLICAS=2

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
    log_info "Cleaning up..."
    
    # Kill daemon processes
    if [[ -n "$NODE1_PID" ]]; then
        log_info "Stopping Node 1 (PID: $NODE1_PID)"
        kill $NODE1_PID 2>/dev/null || true
    fi
    
    if [[ -n "$NODE2_PID" ]]; then
        log_info "Stopping Node 2 (PID: $NODE2_PID)"
        kill $NODE2_PID 2>/dev/null || true
    fi
    
    # Wait a moment for processes to cleanup
    sleep 2
    
    # Remove storage directories
    if [[ -d "$NODE1_STORAGE" ]]; then
        log_info "Removing Node 1 storage directory"
        rm -rf "$NODE1_STORAGE"
    fi
    
    if [[ -d "$NODE2_STORAGE" ]]; then
        log_info "Removing Node 2 storage directory"
        rm -rf "$NODE2_STORAGE"
    fi
    
    log_success "Cleanup completed"
}

# Set trap for cleanup on script exit
trap cleanup EXIT

# Function to wait for service to be ready
wait_for_service() {
    local address=$1
    local name=$2
    local max_attempts=30
    local attempt=1
    
    log_info "Waiting for $name to be ready at $address..."
    
    while [[ $attempt -le $max_attempts ]]; do
        if nc -z ${address/:/ } 2>/dev/null; then
            log_success "$name is ready!"
            return 0
        fi
        
        echo -n "."
        sleep 1
        ((attempt++))
    done
    
    log_error "$name failed to start within $max_attempts seconds"
    return 1
}

# Check if cargo build was successful
check_binary() {
    if [[ ! -f "./target/release/galleonfs" ]]; then
        log_error "GalleonFS binary not found. Building..."
        cargo build --release --no-default-features
        if [[ $? -ne 0 ]]; then
            log_error "Failed to build GalleonFS"
            exit 1
        fi
    fi
}

# Main execution
main() {
    log_info "Starting GalleonFS Two-Node Cluster Test"
    log_info "========================================"
    
    # Check if binary exists
    check_binary
    
    # Initial cleanup in case of previous failed runs
    cleanup
    
    log_info "Configuration:"
    log_info "  Node 1: Storage=$NODE1_STORAGE, Bind=$NODE1_BIND, IPC=$NODE1_IPC"
    log_info "  Node 2: Storage=$NODE2_STORAGE, Bind=$NODE2_BIND, IPC=$NODE2_IPC"
    log_info "  Volume: Name=$VOLUME_NAME, Size=$VOLUME_SIZE, Replicas=$REPLICAS"
    echo
    
    # Step 1: Start Node 1 (creates cluster)
    log_info "Step 1: Starting Node 1 (creates cluster)"
    ./target/release/galleonfs -d \
        --storage-path "$NODE1_STORAGE" \
        --bind-address "$NODE1_BIND" \
        --ipc-address "$NODE1_IPC" \
        > node1.log 2>&1 &
    NODE1_PID=$!
    log_info "Node 1 started with PID: $NODE1_PID"
    
    # Wait for Node 1 to be ready
    wait_for_service "$NODE1_IPC" "Node 1 IPC service"
    sleep 2  # Give it a moment to fully initialize
    
    # Step 2: Start Node 2 (joins cluster)
    log_info "Step 2: Starting Node 2 (joins cluster)"
    ./target/release/galleonfs -d \
        --storage-path "$NODE2_STORAGE" \
        --bind-address "$NODE2_BIND" \
        --ipc-address "$NODE2_IPC" \
        --peer-addresses "$NODE1_BIND" \
        > node2.log 2>&1 &
    NODE2_PID=$!
    log_info "Node 2 started with PID: $NODE2_PID"
    
    # Wait for Node 2 to be ready
    wait_for_service "$NODE2_IPC" "Node 2 IPC service"
    sleep 3  # Give cluster time to form
    
    # Step 3: Check cluster status
    log_info "Step 3: Checking cluster status"
    echo "=========================="
    ./target/release/galleonfs --daemon-address "$NODE1_IPC" cluster status || log_warning "Failed to get cluster status from Node 1"
    echo
    
    # Step 4: Create a replicated volume
    log_info "Step 4: Creating replicated volume"
    echo "================================="
    ./target/release/galleonfs --daemon-address "$NODE1_IPC" volume create \
        --name "$VOLUME_NAME" \
        --size "$VOLUME_SIZE" \
        --replicas $REPLICAS
    echo
    
    # Step 5: List volumes
    log_info "Step 5: Listing volumes"
    echo "====================="
    ./target/release/galleonfs --daemon-address "$NODE1_IPC" volume list
    echo
    
    # Step 6: Get volume details
    log_info "Step 6: Getting volume details"
    echo "============================="
    ./target/release/galleonfs --daemon-address "$NODE1_IPC" volume get "$VOLUME_NAME"
    echo
    
    # Step 7: Check cluster status from Node 2
    log_info "Step 7: Checking cluster status from Node 2"
    echo "==========================================="
    ./target/release/galleonfs --daemon-address "$NODE2_IPC" cluster status || log_warning "Failed to get cluster status from Node 2"
    echo
    
    # Step 8: List volumes from Node 2
    log_info "Step 8: Listing volumes from Node 2"
    echo "=================================="
    ./target/release/galleonfs --daemon-address "$NODE2_IPC" volume list
    echo
    
    # Step 9: Show daemon status
    log_info "Step 9: Checking daemon status"
    echo "============================="
    log_info "Node 1 daemon status:"
    ./target/release/galleonfs --daemon-address "$NODE1_IPC" daemon status || log_warning "Failed to get daemon status from Node 1"
    echo
    log_info "Node 2 daemon status:"
    ./target/release/galleonfs --daemon-address "$NODE2_IPC" daemon status || log_warning "Failed to get daemon status from Node 2"
    echo
    
    log_success "Two-node cluster test completed successfully!"
    log_info "Check node1.log and node2.log for daemon output"
    
    # Wait a moment before cleanup
    log_info "Test completed. Cleaning up in 5 seconds..."
    sleep 5
}

# Check for netcat availability
if ! command -v nc &> /dev/null; then
    log_warning "netcat (nc) not found. Service readiness checks may not work properly."
fi

# Run main function
main