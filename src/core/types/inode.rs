use std::time::SystemTime;
use std::collections::HashMap;
use crate::{NodeType, Permissions, FILE_TYPE_MASK};

pub struct Inode {
    data: u64,
    block_ptrs: [u64; 12],
    file_size: u64,
    created_at: SystemTime,
    modified_at: SystemTime,
    accessed_at: SystemTime,
    owner_uid: u32,
    owner_gid: u32,
    link_count: u32,
    extended_attributes: HashMap<String, Vec<u8>>,
    flags: u32, // e.g., immutable, append-only
    generation: u64,
    device_id: Option<u64>,     // for special files
    checksum: Option<[u8; 32]>, // e.g., SHA-256
    compression: Option<String>,
    encryption: Option<String>,
    project_id: Option<u32>,
    quota: Option<u64>,
    snapshot_id: Option<u64>,
    clone_id: Option<u64>,
}

impl Inode {
    pub fn new(kind: NodeType, permissions: Permissions) -> Self {
        let mut data = 0;
        data |= kind as u64 & FILE_TYPE_MASK;
        data |= (permissions.bits() as u64) << 4;
        Self {
            data,
            block_ptrs: [0; 12],
            file_size: 0,
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
            accessed_at: SystemTime::now(),
            owner_uid: 0,
            owner_gid: 0,
            link_count: 1,
            extended_attributes: HashMap::new(),
            flags: 0,
            generation: 0,
            device_id: None,
            checksum: None,
            compression: None,
            encryption: None,
            project_id: None,
            quota: None,
            snapshot_id: None,
            clone_id: None,
        }
    }
}

impl std::fmt::Display for Inode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let file_type = self.data & FILE_TYPE_MASK;
        let file_type: NodeType = file_type.try_into().unwrap();

        let permission_bits = (self.data >> 4) as u8;
        let permissions = Permissions::from_bits(permission_bits);
        write!(
            f,
            "Inode type: {:?}, permissions: {:?}",
            file_type, permissions
        )?;
        // print raw bytes
        write!(f, ", raw: 0x{:b}", self.data)?;
        Ok(())
    }
}
