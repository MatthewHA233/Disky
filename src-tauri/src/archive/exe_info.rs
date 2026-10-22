//! 读 exe / dll 的版本信息（厂商、产品名、文件说明、版本），给 AI 判断这个程序是谁家的、是什么软件。

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ExeInfo {
    pub company: String,
    pub product: String,
    pub description: String,
    pub version: String,
}

#[cfg(windows)]
pub fn read(path: &str) -> Option<ExeInfo> {
    use std::ffi::c_void;
    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};

    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let file = wide(path);
    // SAFETY: file 以 0 结尾；缓冲区按系统给的大小分配（按 u32 对齐，里面的 u16 读取都对齐），
    // 查到的指针都指向这块缓冲区内部，读取长度也截在缓冲区末尾以内
    unsafe {
        let mut handle = 0u32;
        let len = GetFileVersionInfoSizeW(file.as_ptr(), &mut handle);
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u32; (len as usize).div_ceil(4)];
        if GetFileVersionInfoW(file.as_ptr(), 0, len, buf.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let start = buf.as_ptr() as usize;
        let end = start + len as usize;
        // 查一项，返回指向缓冲区内部的 u16 切片（size 对字符串是字符数，对 Translation 是字节数）
        let query = |sub: &str, size_in_bytes: bool| -> Option<&[u16]> {
            let mut ptr: *mut c_void = std::ptr::null_mut();
            let mut size = 0u32;
            let ok = VerQueryValueW(buf.as_ptr().cast(), wide(sub).as_ptr(), &mut ptr, &mut size);
            let at = ptr as usize;
            if ok == 0 || ptr.is_null() || size == 0 || at < start || at >= end {
                return None;
            }
            let want = if size_in_bytes { size as usize / 2 } else { size as usize };
            Some(std::slice::from_raw_parts(ptr.cast::<u16>(), want.min((end - at) / 2)))
        };
        // 登记的每种语言 / 代码页都试，再试常见的英文和中性语言
        let mut langs: Vec<String> = query("\\VarFileInfo\\Translation", true)
            .map(|t| t.chunks_exact(2).map(|p| format!("{:04x}{:04x}", p[0], p[1])).collect())
            .unwrap_or_default();
        langs.extend(["040904b0", "040904e4", "000004b0"].map(String::from));
        let text = |lang: &str, key: &str| -> String {
            query(&format!("\\StringFileInfo\\{lang}\\{key}"), false)
                .map(|chars| String::from_utf16_lossy(chars).trim_end_matches('\0').trim().to_string())
                .unwrap_or_default()
        };
        langs.iter().find_map(|lang| {
            let info = ExeInfo {
                company: text(lang, "CompanyName"),
                product: text(lang, "ProductName"),
                description: text(lang, "FileDescription"),
                version: text(lang, "ProductVersion"),
            };
            (info != ExeInfo::default()).then_some(info)
        })
    }
}

#[cfg(not(windows))]
pub fn read(_path: &str) -> Option<ExeInfo> {
    None
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn reads_system_exe() {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let info = super::read(&format!("{root}\\explorer.exe")).expect("explorer.exe 有版本信息");
        assert!(info.company.contains("Microsoft"), "{info:?}");
        assert!(super::read("C:\\definitely\\not\\here.exe").is_none());
    }
}
