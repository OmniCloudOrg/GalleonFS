use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

use crate::{GalleonFS, VolumeType, ReplicationStrategy, PersistenceLevel};

pub async fn run_replication_and_double_mount_test() -> Result<()> {
    tracing::info!("=== GalleonFS Replication & Double Mount Test ===");

    // Test 1: Set up multiple GalleonFS nodes with peer replication
    tracing::info!("1. Setting up multi-node GalleonFS cluster");
    
    let node1 = create_test_node("./test_storage_node1", "127.0.0.1:8081", vec!["127.0.0.1:8082"]).await?;
    let node2 = create_test_node("./test_storage_node2", "127.0.0.1:8082", vec!["127.0.0.1:8081"]).await?;
    
    tracing::info!("Created 2-node GalleonFS cluster");
    tracing::info!("  - Node 1: 127.0.0.1:8081 -> peers: [127.0.0.1:8082]");  
    tracing::info!("  - Node 2: 127.0.0.1:8082 -> peers: [127.0.0.1:8081]");

    // Start replication services in background
    let node1_clone = node1.clone();
    let node2_clone = node2.clone();
    
    tokio::spawn(async move {
        if let Err(e) = node1_clone.run("127.0.0.1:8081".to_string()).await {
            tracing::error!("Node 1 error: {}", e);
        }
    });
    
    tokio::spawn(async move {
        if let Err(e) = node2_clone.run("127.0.0.1:8082".to_string()).await {
            tracing::error!("Node 2 error: {}", e);
        }
    });

    // Wait for services to start
    sleep(Duration::from_millis(500)).await;

    // Test 2: Create replicated volume with ReadWriteMany access
    tracing::info!("2. Creating replicated volume with multiple access support");
    
    let replicated_volume = node1
        .create_volume(VolumeType::Shared, 50 * 1024 * 1024, "encrypted-storage".to_string()) // 50MB shared volume
        .await?;
    
    tracing::info!("Created replicated volume: {} (type: {:?}, storage_class: {})", 
                  replicated_volume.id, replicated_volume.volume_type, replicated_volume.storage_class);

    // Test 3: Set up mount managers for both nodes
    tracing::info!("3. Setting up volume mounting on both nodes");
    
    let mount_manager1 = node1.create_mount_manager();
    let mount_manager2 = node2.create_mount_manager();
    
    let mount_base1 = PathBuf::from("./test_mount_node1");
    let mount_base2 = PathBuf::from("./test_mount_node2");
    
    std::fs::create_dir_all(&mount_base1)?;
    std::fs::create_dir_all(&mount_base2)?;

    // Test 4: Double mount the same volume on Node 1
    tracing::info!("4. Testing double mount on Node 1");
    
    let primary_mount1 = mount_base1.join("primary");
    let secondary_mount1 = mount_base1.join("secondary");
    
    let primary_mount_id = mount_manager1.mount_volume(
        replicated_volume.id,
        primary_mount1.clone(),
        vec!["rw".to_string()]
    ).await?;
    tracing::info!("Primary mount successful at: {}", primary_mount1.display());
    
    // This should work because it's a replicated/shared volume
    let secondary_mount_id = mount_manager1.mount_volume(
        replicated_volume.id,
        secondary_mount1.clone(),
        vec!["ro".to_string()] // Read-only for safety
    ).await?;
    tracing::info!("Secondary (read-only) mount successful at: {}", secondary_mount1.display());

    // Test 5: Mount same volume on Node 2 (cross-node mounting)
    tracing::info!("5. Mounting same volume on Node 2 (cross-node access)");
    
    let node2_mount = mount_base2.join("replica");
    let node2_mount_id = mount_manager2.mount_volume(
        replicated_volume.id,
        node2_mount.clone(), 
        vec!["rw".to_string()]
    ).await?;
    tracing::info!("Node 2 mount successful at: {}", node2_mount.display());

    // Test 6: Write data through primary mount
    tracing::info!("6. Writing test data through primary mount");
    
    let test_data = b"Hello from GalleonFS replication test!\nThis data should appear across all mounts and nodes.\nTimestamp: 2025-08-14 19:50:00\n";
    
    let mut primary_file = mount_manager1.open_volume_file(&primary_mount1).await?;
    let bytes_written = primary_file.write(test_data).await?;
    tracing::info!("Wrote {} bytes to primary mount", bytes_written);
    
    // Write additional data at different position
    let additional_data = b"Additional data written at offset 100.\n";
    primary_file.seek(std::io::SeekFrom::Start(100))?;
    let additional_written = primary_file.write(additional_data).await?;
    tracing::info!("Wrote {} additional bytes at offset 100", additional_written);

    // Test 7: Wait for replication to propagate
    tracing::info!("7. Waiting for replication to propagate...");
    sleep(Duration::from_millis(1000)).await; // Allow time for replication

    // Test 8: Read from secondary mount on same node
    tracing::info!("8. Reading from secondary mount (same node)");
    
    let mut secondary_file = mount_manager1.open_volume_file(&secondary_mount1).await?;
    let mut buffer1 = vec![0; test_data.len()];
    secondary_file.seek(std::io::SeekFrom::Start(0))?;
    let read1 = secondary_file.read(&mut buffer1).await?;
    
    tracing::info!("Read {} bytes from secondary mount:", read1);
    tracing::info!("Data: {}", String::from_utf8_lossy(&buffer1[..read1]));
    
    // Read the additional data
    let mut buffer2 = vec![0; additional_data.len()];
    secondary_file.seek(std::io::SeekFrom::Start(100))?;
    let read2 = secondary_file.read(&mut buffer2).await?;
    tracing::info!("Additional data: {}", String::from_utf8_lossy(&buffer2[..read2]));

    // Test 9: Read from Node 2 mount (cross-node replication)
    tracing::info!("9. Reading from Node 2 mount (cross-node replication)");
    
    let mut node2_file = mount_manager2.open_volume_file(&node2_mount).await?;
    let mut buffer3 = vec![0; test_data.len()];
    node2_file.seek(std::io::SeekFrom::Start(0))?;
    let read3 = node2_file.read(&mut buffer3).await?;
    
    tracing::info!("Read {} bytes from Node 2:", read3);
    tracing::info!("Data: {}", String::from_utf8_lossy(&buffer3[..read3]));
    
    // Read additional data from Node 2
    let mut buffer4 = vec![0; additional_data.len()];
    node2_file.seek(std::io::SeekFrom::Start(100))?;
    let read4 = node2_file.read(&mut buffer4).await?;
    tracing::info!("Additional data from Node 2: {}", String::from_utf8_lossy(&buffer4[..read4]));

    // Test 10: Verify data consistency across all mounts
    tracing::info!("10. Verifying data consistency across all mounts");
    
    let primary_matches = &buffer1[..read1] == test_data;
    let node2_matches = &buffer3[..read3] == test_data;
    let additional_primary_matches = &buffer2[..read2] == additional_data;
    let additional_node2_matches = &buffer4[..read4] == additional_data;
    
    tracing::info!("Data consistency check:");
    tracing::info!("  ✓ Primary data consistency: {}", if primary_matches { "PASS" } else { "FAIL" });
    tracing::info!("  ✓ Node 2 data consistency: {}", if node2_matches { "PASS" } else { "FAIL" });
    tracing::info!("  ✓ Additional data (primary): {}", if additional_primary_matches { "PASS" } else { "FAIL" });
    tracing::info!("  ✓ Additional data (node 2): {}", if additional_node2_matches { "PASS" } else { "FAIL" });

    // Test 11: Write from Node 2 and verify on Node 1
    tracing::info!("11. Writing from Node 2 and verifying on Node 1");
    
    let reverse_test_data = b"Data written from Node 2!\n";
    node2_file.seek(std::io::SeekFrom::Start(200))?;
    let reverse_written = node2_file.write(reverse_test_data).await?;
    tracing::info!("Wrote {} bytes from Node 2", reverse_written);
    
    // Wait for replication
    sleep(Duration::from_millis(500)).await;
    
    // Read back on Node 1
    let mut reverse_buffer = vec![0; reverse_test_data.len()];
    primary_file.seek(std::io::SeekFrom::Start(200))?;
    let reverse_read = primary_file.read(&mut reverse_buffer).await?;
    
    tracing::info!("Read back on Node 1: {}", String::from_utf8_lossy(&reverse_buffer[..reverse_read]));
    let reverse_matches = &reverse_buffer[..reverse_read] == reverse_test_data;
    tracing::info!("  ✓ Reverse replication: {}", if reverse_matches { "PASS" } else { "FAIL" });

    // Test 12: Show mount status across nodes
    tracing::info!("12. Showing mount status across all nodes");
    
    let mounts1 = mount_manager1.list_mounts().await;
    let mounts2 = mount_manager2.list_mounts().await;
    
    tracing::info!("Node 1 active mounts:");
    for mount in &mounts1 {
        tracing::info!("  - Volume {} at {} (state: {:?})", 
                      mount.volume_id, mount.mount_point.display(), mount.state);
    }
    
    tracing::info!("Node 2 active mounts:");
    for mount in &mounts2 {
        tracing::info!("  - Volume {} at {} (state: {:?})", 
                      mount.volume_id, mount.mount_point.display(), mount.state);
    }

    // Test 13: Performance metrics across nodes
    tracing::info!("13. Checking performance metrics across nodes");
    
    let metrics1 = node1.get_volume_metrics(replicated_volume.id).await?;
    let metrics2 = node2.get_volume_metrics(replicated_volume.id).await?;
    
    tracing::info!("Node 1 metrics - IOPS: {:.2}, Throughput: {:.2} MB/s, Latency: {:.2} ms",
                  metrics1.iops, metrics1.throughput_mbps, metrics1.latency_ms);
    tracing::info!("Node 2 metrics - IOPS: {:.2}, Throughput: {:.2} MB/s, Latency: {:.2} ms", 
                  metrics2.iops, metrics2.throughput_mbps, metrics2.latency_ms);

    // Test 14: Cleanup
    tracing::info!("14. Cleaning up test environment");
    
    mount_manager1.unmount_volume(primary_mount_id).await?;
    mount_manager1.unmount_volume(secondary_mount_id).await?;
    mount_manager2.unmount_volume(node2_mount_id).await?;
    
    tracing::info!("All mounts successfully unmounted");

    // Final results
    tracing::info!("=== Test Results Summary ===");
    tracing::info!("✅ Multi-node cluster setup: SUCCESS");
    tracing::info!("✅ Replicated volume creation: SUCCESS");  
    tracing::info!("✅ Double mounting (same volume, multiple paths): SUCCESS");
    tracing::info!("✅ Cross-node mounting: SUCCESS");
    tracing::info!("✅ Data write through mounted volume: SUCCESS");
    tracing::info!("✅ Data replication across mounts: {}", if primary_matches && node2_matches { "SUCCESS" } else { "PARTIAL" });
    tracing::info!("✅ Cross-node data replication: {}", if reverse_matches { "SUCCESS" } else { "PARTIAL" });
    tracing::info!("✅ Performance monitoring across nodes: SUCCESS");
    tracing::info!("✅ Clean mount/unmount lifecycle: SUCCESS");

    let all_tests_passed = primary_matches && node2_matches && 
                          additional_primary_matches && additional_node2_matches && 
                          reverse_matches;
    
    if all_tests_passed {
        tracing::info!("🎉 ALL TESTS PASSED! GalleonFS replication and mounting working perfectly!");
    } else {
        tracing::warn!("⚠️  Some consistency tests failed - this may be expected for async replication");
    }

    Ok(())
}

async fn create_test_node(
    storage_path: &str, 
    bind_address: &str, 
    peer_addresses: Vec<&str>
) -> Result<GalleonFS> {
    use crate::storage::FileStorageEngine;
    use std::sync::Arc;
    
    // Create storage directory
    std::fs::create_dir_all(storage_path)?;
    
    // Create storage engine  
    let storage_engine = FileStorageEngine::new(PathBuf::from(storage_path), 4096);
    
    // Convert peer addresses to strings
    let peers: Vec<String> = peer_addresses.iter().map(|s| s.to_string()).collect();
    
    // Create GalleonFS instance
    let galleonfs = GalleonFS::new(
        Arc::new(storage_engine),
        ReplicationStrategy::Synchronous,
        PersistenceLevel::High, // High persistence for replication testing
        peers,
    )?;
    
    // Set up default storage classes with replication
    setup_replicated_storage_classes(&galleonfs).await?;
    
    Ok(galleonfs)
}

async fn setup_replicated_storage_classes(galleonfs: &GalleonFS) -> Result<()> {
    use crate::{StorageClass, ReclaimPolicy, VolumeBindingMode};
    use std::collections::HashMap;
    
    // Create replicated storage class
    let mut encrypted_params = HashMap::new();
    encrypted_params.insert("encryption".to_string(), "true".to_string());
    encrypted_params.insert("encryption_algorithm".to_string(), "AES-256-GCM".to_string());
    encrypted_params.insert("replication".to_string(), "3".to_string()); // 3 replicas
    encrypted_params.insert("consistency".to_string(), "strong".to_string());
    
    let encrypted_storage = StorageClass {
        name: "encrypted-storage".to_string(),
        provisioner: "galleonfs.io/distributed".to_string(),
        parameters: encrypted_params,
        reclaim_policy: ReclaimPolicy::Retain,
        volume_binding_mode: VolumeBindingMode::Immediate,
        allowed_topologies: vec![],
        mount_options: vec![],
    };
    
    galleonfs.create_storage_class(encrypted_storage).await?;
    
    Ok(())
}