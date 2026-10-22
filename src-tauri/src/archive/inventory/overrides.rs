//! 用户（或 AI 查过之后）指定的归属：在内置规则认完之后、找缓存之前套上去。
//!
//! 只按确切的位置改：指给某个软件就成为它的一块（外层是别人的照常从外层扣掉）；
//! 「不属于任何软件」就把规则在这个位置上认的那一块拿掉。指向的软件这次没认出来（卸载了、挪了位置）
//! 就整条跳过，规则原来的判断照旧。指定过的位置之后找缓存时也不再按名字改认。

use super::*;

impl Builder<'_> {
    pub(super) fn apply_overrides(&mut self, overrides: &[Override]) {
        for o in overrides {
            let target = match o.app_key.as_deref() {
                Some(key) => match self.apps.iter().position(|a| a.key == key) {
                    Some(i) => Some(i),
                    None => continue,
                },
                None => None,
            };
            let l = lower(&o.path);
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
}
