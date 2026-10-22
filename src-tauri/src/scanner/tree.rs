//! 紧凑扫描树（arena 结构）。
//!
//! 旧结构每个节点带 name/path 两个 String + 嵌套 Vec，约 250 字节/节点，
//! 整盘 200 万节点就要 500MB。这里改为：
//! - 所有节点平铺在一个 `Vec<Node>` 里，用 u32 下标互相引用，同一目录的子项连续存放；
//! - 文件名全部拼进一个字符串池，节点只存偏移与长度；
//! - 不存路径，需要时顺着父链拼出。
//!
//! 每节点 40 字节 + 名字字节数，且整棵树只有几次大块分配，释放后能真正还给系统。
//!
//! 不变量：父节点下标恒小于子节点下标（构建、嫁接、整理都是先父后子追加），
//! 解码时据此校验，保证损坏的存档不会构造出环。

mod codec;

use std::collections::VecDeque;

use super::{is_strict_ancestor, join_path, path_file_name, DirEntry, SEP};

pub type NodeId = u32;
pub const ROOT: NodeId = 0;
const NO_PARENT: NodeId = u32::MAX;

const FLAG_DIR: u8 = 1;
const FLAG_REMOVED: u8 = 2;

/// 刷新/删除会留下不可达的旧节点；数量超过该值且占比 ≥ 1/4 时整理一次。
const COMPACT_MIN_GARBAGE: usize = 50_000;

#[derive(Debug, Clone, Copy)]
pub struct Node {
    size: u64,
    files: u32,
    subdirs: u32,
    parent: NodeId,
    first_child: NodeId,
    child_count: u32,
    name_off: u32,
    name_len: u16,
    flags: u8,
}

impl Node {
    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn files(&self) -> u64 {
        u64::from(self.files)
    }

    pub fn subdirs(&self) -> u64 {
        u64::from(self.subdirs)
    }

    pub fn is_dir(&self) -> bool {
        self.flags & FLAG_DIR != 0
    }

    fn is_removed(&self) -> bool {
        self.flags & FLAG_REMOVED != 0
    }
}

/// 待追加的子项（扫描、旧存档转换时使用）。
pub struct NewChild<'a> {
    pub name: &'a str,
    pub size: u64,
    pub is_dir: bool,
}

/// `ScanTree::walk` 回调的返回值。
pub enum Visit {
    /// 继续深入该节点（仅对目录生效）
    Descend,
    /// 不深入该节点
    Skip,
    /// 立即结束整个遍历
    Stop,
}

#[derive(Debug)]
pub struct ScanTree {
    root_path: String,
    nodes: Vec<Node>,
    names: String,
    /// 已不可达（被替换或摘除）但尚未整理掉的节点数
    garbage: usize,
}

impl ScanTree {
    /// 只含根节点的空树。
    pub fn new(root_path: &str) -> Self {
        Self::with_capacity(root_path, 1, 0)
    }

    fn with_capacity(root_path: &str, nodes: usize, names: usize) -> Self {
        let mut tree = ScanTree {
            root_path: root_path.to_string(),
            nodes: Vec::with_capacity(nodes.max(1)),
            names: String::with_capacity(names),
            garbage: 0,
        };
        let (name_off, name_len) = tree.push_name(&path_file_name(root_path));
        tree.nodes.push(Node {
            size: 0,
            files: 0,
            subdirs: 0,
            parent: NO_PARENT,
            first_child: 0,
            child_count: 0,
            name_off,
            name_len,
            flags: FLAG_DIR,
        });
        tree
    }

    // -----------------------------------------------------------------------
    // 构建
    // -----------------------------------------------------------------------

    /// 把一批子项连续追加到 `parent` 下（覆盖其原有子项区间），返回第一个子项的下标。
    /// 子目录的聚合值先置 0，扫描完成后用 `set_totals` 回填。
    pub(crate) fn push_children<'a, I>(&mut self, parent: NodeId, children: I) -> NodeId
    where
        I: IntoIterator<Item = NewChild<'a>>,
    {
        let first = self.next_id();
        for child in children {
            let (name_off, name_len) = self.push_name(child.name);
            self.nodes.push(Node {
                size: child.size,
                files: 0,
                subdirs: 0,
                parent,
                first_child: 0,
                child_count: 0,
                name_off,
                name_len,
                flags: if child.is_dir { FLAG_DIR } else { 0 },
            });
        }
        let count = self.next_id() - first;
        let p = &mut self.nodes[parent as usize];
        p.first_child = if count > 0 { first } else { 0 };
        p.child_count = count;
        first
    }

    pub(crate) fn set_totals(&mut self, id: NodeId, size: u64, files: u64, subdirs: u64) {
        let n = &mut self.nodes[id as usize];
        n.size = size;
        n.files = saturate_u32(files);
        n.subdirs = saturate_u32(subdirs);
    }

    pub fn shrink_to_fit(&mut self) {
        self.nodes.shrink_to_fit();
        self.names.shrink_to_fit();
    }

    fn next_id(&self) -> NodeId {
        NodeId::try_from(self.nodes.len())
            .ok()
            .filter(|&id| id != NO_PARENT)
            .expect("扫描树节点数超过 u32 上限")
    }

    fn push_name(&mut self, name: &str) -> (u32, u16) {
        let name = clip_name(name);
        let off = u32::try_from(self.names.len()).expect("扫描树名字池超过 4GB");
        self.names.push_str(name);
        // clip_name 已保证长度不超过 u16::MAX
        (off, name.len() as u16)
    }

    // -----------------------------------------------------------------------
    // 查询
    // -----------------------------------------------------------------------

    pub fn root_path(&self) -> &str {
        &self.root_path
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn name(&self, id: NodeId) -> &str {
        let n = &self.nodes[id as usize];
        let start = n.name_off as usize;
        &self.names[start..start + n.name_len as usize]
    }

    /// 存活节点数（含根）。
    pub fn node_count(&self) -> u64 {
        (self.nodes.len() - self.garbage) as u64
    }

    /// 直接子项（跳过已摘除的），按存储顺序。
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let n = &self.nodes[id as usize];
        (n.first_child..n.first_child + n.child_count)
            .filter(move |&c| !self.nodes[c as usize].is_removed())
    }

    /// 直接子项按大小降序（大小相同按下标，保证分页顺序稳定）。
    pub fn children_by_size(&self, id: NodeId) -> Vec<NodeId> {
        let mut kids: Vec<NodeId> = self.children(id).collect();
        kids.sort_unstable_by(|&a, &b| {
            self.nodes[b as usize]
                .size
                .cmp(&self.nodes[a as usize].size)
                .then(a.cmp(&b))
        });
        kids
    }

    /// 按路径查找节点（ASCII 大小写不敏感 —— NTFS 与默认 APFS 均不区分大小写）。
    pub fn find(&self, path: &str) -> Option<NodeId> {
        let root = self.root_path.as_str();
        if path.eq_ignore_ascii_case(root) {
            return Some(ROOT);
        }
        if !is_strict_ancestor(root, path) {
            return None;
        }
        let skip = if root.ends_with(SEP) { root.len() } else { root.len() + 1 };
        let comps: Vec<&str> = path.get(skip..)?.split(SEP).collect();
        if comps.iter().any(|c| c.is_empty()) {
            return None;
        }
        self.find_from(ROOT, &comps)
    }

    /// 逐级匹配路径分量：优先字节完全相同的子项，再试仅大小写不同的；
    /// 某个候选下走不通时回溯到下一个（区分大小写的卷上可能同时存在 Build/ 与 build/）。
    fn find_from(&self, cur: NodeId, comps: &[&str]) -> Option<NodeId> {
        let Some((head, rest)) = comps.split_first() else {
            return Some(cur);
        };
        let exact = self.children(cur).filter(|&c| self.name(c) == *head);
        let folded = self.children(cur).filter(|&c| {
            let name = self.name(c);
            name != *head && name.eq_ignore_ascii_case(head)
        });
        exact.chain(folded).find_map(|c| self.find_from(c, rest))
    }

    /// 顺着父链拼出节点的完整路径。
    pub fn path_of(&self, id: NodeId) -> String {
        let mut chain = Vec::new();
        let mut cur = id;
        while cur != ROOT {
            chain.push(cur);
            cur = self.nodes[cur as usize].parent;
        }
        let mut path = self.root_path.clone();
        for &c in chain.iter().rev() {
            if !path.ends_with(SEP) {
                path.push(SEP);
            }
            path.push_str(self.name(c));
        }
        path
    }

    /// 子项分页（按大小降序），返回 (子项总数, 当前页)。
    pub fn child_entries(&self, id: NodeId, offset: usize, limit: usize) -> (usize, Vec<DirEntry>) {
        let kids = self.children_by_size(id);
        let base = self.path_of(id);
        let entries = kids
            .iter()
            .skip(offset)
            .take(limit)
            .map(|&c| self.entry_of(c, &base))
            .collect();
        (kids.len(), entries)
    }

    fn entry_of(&self, id: NodeId, parent_path: &str) -> DirEntry {
        let n = &self.nodes[id as usize];
        let name = self.name(id);
        DirEntry {
            name: name.to_string(),
            path: join_path(parent_path, name),
            files: n.files(),
            subdirs: n.subdirs(),
            logical_size: n.size,
            is_dir: n.is_dir(),
        }
    }

    /// 从 `start` 开始深度优先遍历全部存活后代（不含 start 自身）。
    /// 回调参数为 (节点, 父目录完整路径)；只有返回 `Descend` 的目录才会展开，
    /// 路径也只为被展开的目录拼接，避免给海量小文件逐个分配字符串。
    pub fn walk<F>(&self, start: NodeId, mut visit: F)
    where
        F: FnMut(NodeId, &str) -> Visit,
    {
        let mut stack = vec![(start, self.path_of(start))];
        while let Some((id, path)) = stack.pop() {
            for child in self.children(id) {
                match visit(child, &path) {
                    Visit::Stop => return,
                    Visit::Skip => {}
                    Visit::Descend => {
                        if self.nodes[child as usize].is_dir() {
                            stack.push((child, join_path(&path, self.name(child))));
                        }
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // 增量修改
    // -----------------------------------------------------------------------

    /// 用 `sub`（以同一目录为根的新扫描结果）替换目录 `target` 的整棵子树，
    /// 并把大小/计数差量传播给所有祖先，保证上层统计立即一致。
    /// target 不是目录时不做任何修改并返回 false。
    /// 可能触发整理，调用后原有 NodeId 失效，需要重新 `find`。
    pub fn replace_children(&mut self, target: NodeId, sub: ScanTree) -> bool {
        let old = self.nodes[target as usize];
        if !old.is_dir() || old.is_removed() {
            return false;
        }
        if target == ROOT {
            // 刷新整棵树：直接换成新树（O(1)），旧树整体释放；根路径保留原写法
            let root_path = std::mem::take(&mut self.root_path);
            *self = sub;
            self.root_path = root_path;
            return true;
        }

        let sub = if sub.garbage > 0 { sub.compacted() } else { sub };
        let new = sub.nodes[ROOT as usize];
        self.garbage += self.count_descendants(target);
        self.graft(target, &sub);
        self.set_totals(target, new.size, new.files(), new.subdirs());
        self.propagate(
            old.parent,
            signed_delta(new.size, old.size),
            signed_delta(new.files(), old.files()),
            signed_delta(new.subdirs(), old.subdirs()),
        );
        self.maybe_compact();
        true
    }

    /// 把无垃圾的 `sub` 除根以外的全部节点整块追加到末尾，挂到 `target` 下。
    /// sub 自身满足「父下标 < 子下标、子项连续」，整体平移下标后依然成立。
    fn graft(&mut self, target: NodeId, sub: &ScanTree) {
        let added = sub.nodes.len() - 1;
        let base = self.next_id();
        NodeId::try_from(self.nodes.len() + added)
            .ok()
            .filter(|&n| n != NO_PARENT)
            .expect("扫描树节点数超过 u32 上限");
        let names_base = u32::try_from(self.names.len()).expect("扫描树名字池超过 4GB");
        u32::try_from(self.names.len() + sub.names.len()).expect("扫描树名字池超过 4GB");

        // sub 的下标 i（≥1）平移为 base + i - 1；sub 的根映射为 target
        let shift = |id: NodeId| if id == ROOT { target } else { base + id - 1 };
        if let Some(n) = headroom(self.nodes.len(), self.nodes.capacity(), added) {
            self.nodes.reserve_exact(n);
        }
        self.nodes.extend(sub.nodes[1..].iter().map(|n| Node {
            parent: shift(n.parent),
            first_child: if n.child_count > 0 { shift(n.first_child) } else { 0 },
            name_off: names_base + n.name_off,
            ..*n
        }));
        if let Some(n) = headroom(self.names.len(), self.names.capacity(), sub.names.len()) {
            self.names.reserve_exact(n);
        }
        self.names.push_str(&sub.names);

        let root = sub.nodes[ROOT as usize];
        let t = &mut self.nodes[target as usize];
        t.first_child = if root.child_count > 0 { shift(root.first_child) } else { 0 };
        t.child_count = root.child_count;
    }

    /// 摘除节点（删除/移走之后）并回减所有祖先的统计，返回是否真的摘除了。
    /// 可能触发整理，调用后原有 NodeId 失效。
    pub fn remove(&mut self, id: NodeId) -> bool {
        if id == ROOT {
            return false;
        }
        let n = self.nodes[id as usize];
        if n.is_removed() {
            return false;
        }
        let (d_files, d_dirs) = if n.is_dir() {
            (-(n.files as i64), -(n.subdirs as i64) - 1)
        } else {
            (-1, 0)
        };

        self.garbage += 1 + self.count_descendants(id);
        self.nodes[id as usize].flags |= FLAG_REMOVED;
        self.propagate(n.parent, -(n.size as i64), d_files, d_dirs);
        self.maybe_compact();
        true
    }

    /// 有垃圾节点时原地整理（峰值为旧树 + 整理副本，旧树随即释放）。
    pub fn compact(&mut self) {
        if self.garbage > 0 {
            *self = self.compacted();
        }
    }

    /// 只保留存活节点的紧凑副本。
    pub fn compacted(&self) -> ScanTree {
        let live = self.node_count() as usize;
        let mut out = ScanTree::with_capacity(&self.root_path, live, self.names.len());
        let root = self.nodes[ROOT as usize];
        out.set_totals(ROOT, root.size, root.files(), root.subdirs());
        out.copy_children(ROOT, self, ROOT);
        out.shrink_to_fit();
        out
    }

    fn maybe_compact(&mut self) {
        if self.garbage >= COMPACT_MIN_GARBAGE && self.garbage * 4 >= self.nodes.len() {
            *self = self.compacted();
        }
    }

    /// 按 BFS 把 `src` 中 `src_parent` 的全部存活后代复制到本树 `dst_parent` 之下。
    /// BFS 保证每个目录的子项连续追加，且父下标恒小于子下标。
    fn copy_children(&mut self, dst_parent: NodeId, src: &ScanTree, src_parent: NodeId) {
        let mut queue = VecDeque::from([(src_parent, dst_parent)]);
        let mut kids = Vec::new();
        while let Some((s, d)) = queue.pop_front() {
            kids.clear();
            kids.extend(src.children(s));
            let first = self.push_children(
                d,
                kids.iter().map(|&k| NewChild {
                    name: src.name(k),
                    size: src.nodes[k as usize].size,
                    is_dir: src.nodes[k as usize].is_dir(),
                }),
            );
            for (i, &k) in kids.iter().enumerate() {
                let sn = src.nodes[k as usize];
                if sn.is_dir() {
                    let id = first + i as NodeId;
                    self.set_totals(id, sn.size, sn.files(), sn.subdirs());
                    if sn.child_count > 0 {
                        queue.push_back((k, id));
                    }
                }
            }
        }
    }

    fn count_descendants(&self, id: NodeId) -> usize {
        let mut count = 0;
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            for c in self.children(cur) {
                count += 1;
                if self.nodes[c as usize].child_count > 0 {
                    stack.push(c);
                }
            }
        }
        count
    }

    /// 从 `from` 起沿父链向上（含 from）累加差量。
    fn propagate(&mut self, from: NodeId, d_size: i64, d_files: i64, d_dirs: i64) {
        let mut cur = from;
        while cur != NO_PARENT {
            let n = &mut self.nodes[cur as usize];
            n.size = add_signed(n.size, d_size);
            n.files = saturate_u32(add_signed(u64::from(n.files), d_files));
            n.subdirs = saturate_u32(add_signed(u64::from(n.subdirs), d_dirs));
            cur = n.parent;
        }
    }
}

/// 追加前需要扩容时返回 `reserve_exact` 的参数：额外留 1/16 余量。
/// 既避免 shrink_to_fit 之后每次小刷新都整块重分配，也避免默认倍增策略一下多占一倍内存。
fn headroom(len: usize, capacity: usize, additional: usize) -> Option<usize> {
    (capacity - len < additional).then(|| additional + len / 16)
}

fn saturate_u32(v: u64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

fn add_signed(v: u64, delta: i64) -> u64 {
    if delta >= 0 {
        v.saturating_add(delta as u64)
    } else {
        v.saturating_sub(delta.unsigned_abs())
    }
}

fn signed_delta(new: u64, old: u64) -> i64 {
    new as i64 - old as i64
}

/// 名字长度上限 u16::MAX 字节（NTFS/APFS 文件名远小于此，仅防御性截断到字符边界）。
fn clip_name(name: &str) -> &str {
    let max = u16::MAX as usize;
    if name.len() <= max {
        return name;
    }
    let mut end = max;
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

#[cfg(test)]
mod tests;
