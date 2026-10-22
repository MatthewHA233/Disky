//! 扫描树存档格式（v2）：直接按小端顺序写出节点数组与名字池，
//! 无需像旧版那样先整树克隆成嵌套结构再 bincode，保存时内存峰值只多一份 blob。
//!
//! 布局：
//! ```text
//! "DSKY" | version u32 | root_len u32 | root_path | node_count u64 | names_len u64
//! | node_count × NODE_BYTES | names（UTF-8）
//! ```

use super::{Node, ScanTree, FLAG_DIR, NO_PARENT};

const MAGIC: &[u8; 4] = b"DSKY";
const VERSION: u32 = 2;
/// size u64 + files/subdirs/parent/first_child/child_count/name_off 6×u32 + name_len u16 + flags u8
pub(super) const NODE_BYTES: usize = 8 + 6 * 4 + 2 + 1;

impl ScanTree {
    /// 是否为新格式存档（旧版是嵌套结构的 bincode，开头不可能是该魔数）。
    pub fn is_encoded(bytes: &[u8]) -> bool {
        bytes.starts_with(MAGIC)
    }

    /// 序列化；有垃圾节点时先整理，只写出存活节点。
    pub fn encode(&self) -> Vec<u8> {
        if self.garbage > 0 {
            return self.compacted().encode();
        }
        let mut out = Vec::with_capacity(
            28 + self.root_path.len() + self.nodes.len() * NODE_BYTES + self.names.len(),
        );
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        // 根路径来自本机目录，长度远小于 4GB
        out.extend_from_slice(&(self.root_path.len() as u32).to_le_bytes());
        out.extend_from_slice(self.root_path.as_bytes());
        out.extend_from_slice(&(self.nodes.len() as u64).to_le_bytes());
        out.extend_from_slice(&(self.names.len() as u64).to_le_bytes());
        for n in &self.nodes {
            out.extend_from_slice(&n.size.to_le_bytes());
            for v in [n.files, n.subdirs, n.parent, n.first_child, n.child_count, n.name_off] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&n.name_len.to_le_bytes());
            out.push(n.flags);
        }
        out.extend_from_slice(self.names.as_bytes());
        out
    }

    /// 反序列化并做完整结构校验（存档文件属于外部输入，可能损坏）。
    pub fn decode(bytes: &[u8]) -> Result<ScanTree, String> {
        let mut r = Reader { buf: bytes, pos: 0 };
        if r.take(4)? != MAGIC {
            return Err("不是 Disky 扫描树存档".into());
        }
        let version = r.u32()?;
        if version != VERSION {
            return Err(format!("不支持的扫描树存档版本: {version}"));
        }
        let root_len = r.u32()? as usize;
        let root_path = std::str::from_utf8(r.take(root_len)?)
            .map_err(|_| "存档根路径不是合法 UTF-8".to_string())?
            .to_string();
        let node_count = usize::try_from(r.u64()?).map_err(|_| corrupt("节点数过大"))?;
        let names_len = usize::try_from(r.u64()?).map_err(|_| corrupt("名字池过大"))?;

        // 先整体核对长度，避免按损坏的计数做超大分配
        let expected = node_count
            .checked_mul(NODE_BYTES)
            .and_then(|n| n.checked_add(names_len))
            .ok_or_else(|| corrupt("长度溢出"))?;
        if r.remaining() != expected {
            return Err(corrupt("文件长度与头部不符"));
        }
        if node_count == 0 || node_count >= NO_PARENT as usize {
            return Err(corrupt("节点数非法"));
        }

        let mut nodes = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            nodes.push(Node {
                size: r.u64()?,
                files: r.u32()?,
                subdirs: r.u32()?,
                parent: r.u32()?,
                first_child: r.u32()?,
                child_count: r.u32()?,
                name_off: r.u32()?,
                name_len: r.u16()?,
                flags: r.u8()?,
            });
        }
        let names = String::from_utf8(r.take(names_len)?.to_vec())
            .map_err(|_| corrupt("名字池不是合法 UTF-8"))?;

        let tree = ScanTree {
            root_path,
            nodes,
            names,
            garbage: 0,
        };
        tree.validate()?;
        Ok(tree)
    }

    /// 校验：名字区间合法；根无父；其余节点父下标 < 自身；
    /// 子项区间在自身之后、不越界、且区间内每个节点的父指针都指回自己；
    /// 子项总数 = 节点数 - 1（每个非根节点恰好属于一个父区间，无孤儿）。
    fn validate(&self) -> Result<(), String> {
        let len = self.nodes.len();
        let mut covered: usize = 0;
        for (i, n) in self.nodes.iter().enumerate() {
            let start = n.name_off as usize;
            let end = start + n.name_len as usize;
            if end > self.names.len()
                || !self.names.is_char_boundary(start)
                || !self.names.is_char_boundary(end)
            {
                return Err(corrupt("名字区间越界"));
            }
            if n.flags & !FLAG_DIR != 0 {
                return Err(corrupt("节点标志非法"));
            }
            if i == 0 {
                if n.parent != NO_PARENT || !n.is_dir() {
                    return Err(corrupt("根节点非法"));
                }
            } else if n.parent as usize >= i {
                return Err(corrupt("父节点下标非法"));
            }
            if n.child_count > 0 {
                let first = n.first_child as usize;
                let count = n.child_count as usize;
                if !n.is_dir() || first <= i || first + count > len {
                    return Err(corrupt("子项区间非法"));
                }
                if self.nodes[first..first + count]
                    .iter()
                    .any(|c| c.parent as usize != i)
                {
                    return Err(corrupt("子项父指针不一致"));
                }
                covered += count;
            }
        }
        if covered != len - 1 {
            return Err(corrupt("存在不可达节点"));
        }
        Ok(())
    }
}

fn corrupt(reason: &str) -> String {
    format!("扫描树存档已损坏（{reason}）")
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or_else(|| corrupt("数据被截断"))?;
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        self.array().map(u16::from_le_bytes)
    }

    fn u32(&mut self) -> Result<u32, String> {
        self.array().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Result<u64, String> {
        self.array().map(u64::from_le_bytes)
    }
}
