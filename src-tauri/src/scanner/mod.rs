pub mod walk;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub logical_size: u64,
    pub files: u64,
    pub subdirs: u64,
    pub is_dir: bool,
    #[serde(skip)]
    pub children: Vec<TreeNode>,
}

/// Serializable child entry returned to the frontend (matches old DirEntry shape).
#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub files: u64,
    pub subdirs: u64,
    pub logical_size: u64,
    pub is_dir: bool,
}

impl TreeNode {
    pub fn to_dir_entry(&self) -> DirEntry {
        DirEntry {
            name: self.name.clone(),
            path: self.path.clone(),
            files: self.files,
            subdirs: self.subdirs,
            logical_size: self.logical_size,
            is_dir: self.is_dir,
        }
    }
}
