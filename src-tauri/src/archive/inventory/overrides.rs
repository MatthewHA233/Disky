//! 用户（或 AI 查过之后）指定的归属：在内置规则认完之后、找缓存之前套上去。
//!
//! 只按确切的位置改：指给某个软件就成为它的一块（外层是别人的照常从外层扣掉）；
//! 「不属于任何软件」就把规则在这个位置上认的那一块拿掉。指向的软件这次没认出来（卸载了、挪了位置）
//! 就整条跳过，规则原来的判断照旧。指定过的位置之后找缓存时也不再按名字改认。
//!
//! 指的正好是另一个软件条目自己的文件夹（那个条目就是从这个文件夹认出来的）时，说明它本来就是
//! 目标软件的一部分（改名重装后留下的旧目录）：整个条目并过去，按名字找到的数据也跟着走。

use super::*;

impl Builder<'_> {
    pub(super) fn apply_overrides(&mut self, overrides: &[Override]) {
        for o in overrides {
            let target = match o.app_key.as_deref() {
                Some(key) => match self.app_by_key(key) {
                    Some(i) => Some(i),
                    None => continue,
                },
                None => None,
            };
            let l = lower(&o.path);
            if let Some(t) = target {
                if let Some(s) = self.item_from(&l).filter(|&s| s != t) {
                    self.merge_into(s, t);
                }
            }
            self.pinned.insert(l.clone());
            if let Some(prev) = self.claimed.remove(&l) {
                self.apps[prev].parts.retain(|p| lower(&p.path) != l);
            }
            let Some(i) = target else { continue };
            if self.claim_inside(i, &o.path, o.kind) {
                if let Some(p) = self.apps[i].parts.last_mut() {
                    p.origin = Some(o.origin.clone());
                    p.reason = (!o.reason.is_empty()).then(|| o.reason.clone());
                }
            }
        }
    }

    /// 按 key 找软件；并进别的软件的条目找它并进去的那个
    fn app_by_key(&self, key: &str) -> Option<usize> {
        self.merged.get(key).copied().or_else(|| self.apps.iter().position(|a| a.key == key))
    }

    /// 从这个位置（小写）认出来的软件条目：安装位置、程序目录、绿色软件的 key 都是 `app:<位置>`
    fn item_from(&self, lower_path: &str) -> Option<usize> {
        let key = format!("app:{lower_path}");
        self.apps.iter().position(|a| a.key == key && !a.parts.is_empty())
    }

    /// 条目 s 的各块都挪给 t，s 留空（收尾时去掉）；之后指向 s 的记录改指 t
    fn merge_into(&mut self, s: usize, t: usize) {
        let parts = std::mem::take(&mut self.apps[s].parts);
        self.apps[t].parts.extend(parts);
        for owner in self.claimed.values_mut().chain(self.merged.values_mut()) {
            if *owner == s {
                *owner = t;
            }
        }
        self.merged.insert(self.apps[s].key.clone(), t);
    }
}
