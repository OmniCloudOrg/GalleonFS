

#[derive(Debug,Clone, Copy,PartialEq, PartialOrd)]
#[repr(u64)]
pub enum NodeType {
    File = 0,
    Directory = 1,
    Symlink = 2,
}

#[derive(Debug)]
pub enum NodeTypeError {
    InvalidNodeType
}

impl TryFrom<u64> for NodeType {
    type Error = NodeTypeError;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
        0 => Ok(NodeType::File),
        1 => Ok(NodeType::Directory),
        2 => Ok(NodeType::Symlink),
        _ => Err(NodeTypeError::InvalidNodeType)
        }
    }
}
