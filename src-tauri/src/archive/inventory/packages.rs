//! 应用商店装的第三方软件（WindowsApps\OpenAI.Codex_26.930.2377.0_x64__2p2nqsd0c76g0）：
//! 每个包族一项，几个版本并在一起，Local\Packages 里它的数据也归它。
//! 微软自家的包（照片、Xbox、各版运行库）是系统的一部分，留在「应用商店应用」里。

use super::*;

/// 包目录名拆出包族名：Name_Version_Arch_ResourceId_PublisherId → Name
fn package_family(dir: &str) -> Option<&str> {
    let (name, rest) = dir.split_once('_')?;
    (!name.is_empty() && !rest.is_empty()).then_some(name)
}

fn is_microsoft_package(family: &str) -> bool {
    let l = family.to_ascii_lowercase();
    l.starts_with("microsoft") || l.starts_with("windows")
}

/// 显示名和发行商：OpenAI.Codex → (Codex, OpenAI)；发行商是一串编号（AD2F1837）时不要
fn package_display(family: &str) -> (String, String) {
    match family.rsplit_once('.') {
        Some((publisher, name)) if !name.is_empty() => {
            let publisher = if publisher.chars().any(|c| c.is_ascii_digit()) { "" } else { publisher };
            (name.to_string(), publisher.to_string())
        }
        _ => (family.to_string(), String::new()),
    }
}

pub(super) fn package_key(family: &str) -> String {
    format!("app:store:{}", family.to_ascii_lowercase())
}

impl Builder<'_> {
    pub(super) fn store_packages(&mut self) {
        if !WINDOWS {
            return;
        }
        let stores: Vec<String> = self
            .roots
            .iter()
            .flat_map(|r| [join_path(&join_path(r, "Program Files"), "WindowsApps"), join_path(r, "WindowsApps")])
            .collect();
        for store in stores {
            for e in self.idx.children(&store) {
                let dir = base(&e.path);
                let Some(family) = package_family(&dir).filter(|f| !is_microsoft_package(f)) else { continue };
                if !e.is_dir || e.size < MIN_APP {
                    continue;
                }
                let i = self.package_app(family);
                // 外层的 WindowsApps 归应用商店应用，这里嵌套认，收尾时从外层扣掉
                self.claim_inside(i, &e.path, Kind::Prog);
                if self.apps[i].icon.is_none() {
                    let keys = self.apps[i].keys.clone();
                    self.apps[i].icon = self.best_exe(&e.path, &keys);
                }
            }
        }
        let data_roots: Vec<String> = self.homes.iter().map(|h| join_path(&join_path(&join_path(h, "AppData"), "Local"), "Packages")).collect();
        for root in data_roots {
            for e in self.idx.children(&root) {
                let key = package_family(&base(&e.path)).map(package_key);
                if let Some(i) = key.and_then(|k| self.apps.iter().position(|a| a.key == k)) {
                    self.claim_inside(i, &e.path, Kind::Data);
                }
            }
        }
    }

    fn package_app(&mut self, family: &str) -> usize {
        let key = package_key(family);
        if let Some(i) = self.apps.iter().position(|a| a.key == key) {
            return i;
        }
        let (name, publisher) = package_display(family);
        let mut d = Draft::new(key, name.clone());
        d.publisher = publisher;
        // 商店装的就是装着的，不做卸载判断
        d.registered = true;
        d.add_key(&name);
        d.add_key(family);
        self.push(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package_names() {
        assert_eq!(package_family("OpenAI.Codex_26.930.2377.0_x64__2p2nqsd0c76g0"), Some("OpenAI.Codex"));
        assert_eq!(package_family("Claude_pzs8sxrjxfjjc"), Some("Claude"));
        assert_eq!(package_family("Deleted"), None);
        assert_eq!(package_display("OpenAI.Codex"), ("Codex".to_string(), "OpenAI".to_string()));
        assert_eq!(package_display("AD2F1837.HPPrinterControl"), ("HPPrinterControl".to_string(), String::new()));
        assert_eq!(package_display("Claude"), ("Claude".to_string(), String::new()));
        assert!(is_microsoft_package("MicrosoftCorporationII.WindowsSubsystemForLinux"));
        assert!(!is_microsoft_package("Claude"));
    }
}
