mod cli;
mod core;

use core::types::{
    inode::Inode,
    node::NodeType,
};
use bitflags::bitflags;

const FILE_TYPE_MASK: u64 = 0xF;

bitflags! {
    #[derive(Debug,Clone, Copy,PartialEq,Eq,Hash)]
    pub struct Permissions: u8 {
        const EXECUTE = 0b001;
        const READ = 0b010;
        const WRITE = 0b100;

        const RWX = Self::EXECUTE.bits() | Self::READ.bits() | Self::WRITE.bits();
    }
}

fn main() {
    let inode = Inode::new(NodeType::Symlink, Permissions::RWX);

    println!("{}", inode);
}