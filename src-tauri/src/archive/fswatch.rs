//! 盯住一整块盘的变化，每个变了的路径回调一次。
//!
//! Windows 直接用 ReadDirectoryChangesW：notify 6 的缓冲只有 16KB，盘上一忙就溢出，而且溢出时
//! 悄悄丢掉事件不报；这里缓冲开到 1MB，溢出时回调 `Overflow`，由调用方去核对。
//! 其他平台用 notify（macOS 是 FSEvents，不占文件句柄）。

pub enum FsEvent {
    Changed(String),
    /// 漏掉了一些变化，这块盘需要核对
    Overflow,
    /// 监听停了（盘拔掉了、出错了）
    Stopped,
}

#[cfg(windows)]
mod imp {
    use super::FsEvent;
    use crate::scanner::join_path;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, ReadDirectoryChangesW, BusType1394, BusTypeMmc, BusTypeSd, BusTypeUsb, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_SIZE,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceProperty, IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    const BUF_BYTES: usize = 1 << 20;
    // 只关心大小会变的：新建、删除、改名、文件变大变小（不管改时间、改属性）
    const FILTER: u32 = FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME | FILE_NOTIFY_CHANGE_SIZE;
    const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn open(path: &str, access: u32, flags: u32) -> Option<HANDLE> {
        let w = wide(path);
        let h = unsafe { CreateFileW(w.as_ptr(), access, SHARE_ALL, std::ptr::null(), OPEN_EXISTING, flags, std::ptr::null_mut()) };
        (h != INVALID_HANDLE_VALUE).then_some(h)
    }

    /// 外接盘（USB、SD 卡等）：一直开着监听句柄会让「安全弹出」报设备正在使用，这种盘不盯。
    /// 移动硬盘常被系统报成「本地磁盘」，所以按总线类型判断
    pub fn is_external(root: &str) -> bool {
        let Some(h) = open(&format!("\\\\.\\{root}"), 0, 0) else { return false };
        let query = STORAGE_PROPERTY_QUERY { PropertyId: StorageDeviceProperty, QueryType: PropertyStandardQuery, AdditionalParameters: [0] };
        let mut buf = [0u64; 128];
        let mut got = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                h,
                IOCTL_STORAGE_QUERY_PROPERTY,
                (&query as *const STORAGE_PROPERTY_QUERY).cast(),
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                buf.as_mut_ptr().cast(),
                std::mem::size_of_val(&buf) as u32,
                &mut got,
                std::ptr::null_mut(),
            )
        };
        unsafe { CloseHandle(h) };
        if ok == 0 || (got as usize) < std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
            return false;
        }
        let desc = unsafe { &*(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };
        desc.RemovableMedia != 0 || [BusTypeUsb, BusType1394, BusTypeSd, BusTypeMmc].contains(&desc.BusType)
    }

    /// 在后台线程里一直盯着 `root`（形如 `C:`），直到句柄失效（盘拔掉了）
    pub fn watch(root: &str, mut on: impl FnMut(FsEvent) + Send + 'static) -> Result<(), String> {
        let dir = format!("{}\\", root.trim_end_matches('\\'));
        let h = open(&dir, FILE_LIST_DIRECTORY, FILE_FLAG_BACKUP_SEMANTICS).ok_or_else(|| format!("打不开 {dir}"))?;
        // HANDLE 是裸指针，不能直接送进线程
        let raw = h as usize;
        let root = root.trim_end_matches('\\').to_string();
        std::thread::Builder::new()
            .name(format!("watch {root}"))
            .spawn(move || {
                let h = raw as HANDLE;
                // 按 DWORD 对齐
                let mut buf = vec![0u32; BUF_BYTES / 4];
                loop {
                    let mut got = 0u32;
                    let ok = unsafe {
                        ReadDirectoryChangesW(h, buf.as_mut_ptr().cast(), BUF_BYTES as u32, 1, FILTER, &mut got, std::ptr::null_mut(), None)
                    };
                    if ok == 0 {
                        break;
                    }
                    if got == 0 {
                        on(FsEvent::Overflow);
                        continue;
                    }
                    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), got as usize) };
                    for rel in parse(bytes) {
                        on(FsEvent::Changed(long_path(&join_path(&root, &rel))));
                    }
                }
                unsafe { CloseHandle(h) };
                on(FsEvent::Stopped);
            })
            .map_err(|e| {
                unsafe { CloseHandle(raw as HANDLE) };
                e.to_string()
            })?;
        Ok(())
    }

    /// 系统有时报 8.3 短名（ADMINI~1）：换成长名，不然会把同一个目录当成另一个、大小算两遍。
    /// 已经删掉的位置换不了，就换它还在的那段上级再接上
    pub(super) fn long_path(path: &str) -> String {
        use windows_sys::Win32::Storage::FileSystem::GetLongPathNameW;
        if !path.contains('~') {
            return path.to_string();
        }
        let w = wide(&format!(r"\\?\{path}"));
        let mut buf = vec![0u16; 1024];
        let n = unsafe { GetLongPathNameW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if n > 0 && n < buf.len() {
            let long = String::from_utf16_lossy(&buf[..n]);
            return long.strip_prefix(r"\\?\").map_or(long.clone(), str::to_string);
        }
        match path.rsplit_once('\\') {
            Some((head, name)) if head.contains('~') && head.len() > 2 => join_path(&long_path(head), name),
            _ => path.to_string(),
        }
    }

    /// 拆开 FILE_NOTIFY_INFORMATION 链表，得到相对盘根的路径
    pub(super) fn parse(bytes: &[u8]) -> Vec<String> {
        let u32_at = |off: usize| bytes.get(off..off + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
        let mut out = Vec::new();
        let mut off = 0usize;
        while let (Some(next), Some(len)) = (u32_at(off), u32_at(off + 8)) {
            let Some(name) = bytes.get(off + 12..off + 12 + len) else { break };
            let units: Vec<u16> = name.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            out.push(String::from_utf16_lossy(&units));
            if next == 0 {
                break;
            }
            off += next;
        }
        out
    }
}

#[cfg(not(windows))]
mod imp {
    use super::FsEvent;
    use notify::{RecursiveMode, Watcher};

    /// FSEvents 不占文件句柄，外接盘照样能推出，都盯
    pub fn is_external(_root: &str) -> bool {
        false
    }

    /// macOS 的数据卷经固件链接挂在 / 下：FSEvents 可能按 /System/Volumes/Data/Users/... 报，换回 /Users/...
    fn canonical(path: String) -> String {
        if cfg!(target_os = "macos") {
            if let Some(rest) = path.strip_prefix("/System/Volumes/Data/") {
                return format!("/{rest}");
            }
        }
        path
    }

    pub fn watch(root: &str, mut on: impl FnMut(FsEvent) + Send + 'static) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(tx).map_err(|e| e.to_string())?;
        watcher.watch(std::path::Path::new(root), RecursiveMode::Recursive).map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name(format!("watch {root}"))
            .spawn(move || {
                // 监听器跟着线程活着，丢掉就停了
                let _watcher = watcher;
                for res in rx {
                    match res {
                        Ok(ev) if ev.need_rescan() => on(FsEvent::Overflow),
                        Ok(ev) => ev.paths.iter().for_each(|p| on(FsEvent::Changed(canonical(p.to_string_lossy().into_owned())))),
                        Err(_) => on(FsEvent::Overflow),
                    }
                }
                on(FsEvent::Stopped);
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub use imp::{is_external, watch};

#[cfg(all(test, windows))]
mod tests {
    use super::imp::{long_path, parse};

    #[test]
    fn short_names_become_long_names() {
        // 系统盘一般开着短名生成，临时目录可能被挪到了别的盘上
        let base = std::env::var("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
        let dir = base.join(format!("disky long name {}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let long = crate::scanner::normalize_path(&dir.to_string_lossy());
        let w: Vec<u16> = long.encode_utf16().chain(Some(0)).collect();
        let mut buf = vec![0u16; 1024];
        let n = unsafe { windows_sys::Win32::Storage::FileSystem::GetShortPathNameW(w.as_ptr(), buf.as_mut_ptr(), 1024) } as usize;
        let short = String::from_utf16_lossy(&buf[..n]);
        // 这块盘关了短名生成就没有可比的
        if n > 0 && short.contains('~') && !short.eq_ignore_ascii_case(&long) {
            assert!(long_path(&short).eq_ignore_ascii_case(&long));
            let gone = format!(r"{short}\deleted.txt");
            assert!(long_path(&gone).eq_ignore_ascii_case(&format!(r"{long}\deleted.txt")), "删掉的换上级");
        }
        assert_eq!(long_path(r"C:\Users\a"), r"C:\Users\a");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn record(next: u32, name: &str) -> Vec<u8> {
        let units: Vec<u16> = name.encode_utf16().collect();
        let mut b = Vec::new();
        b.extend_from_slice(&next.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&((units.len() * 2) as u32).to_le_bytes());
        units.iter().for_each(|u| b.extend_from_slice(&u.to_le_bytes()));
        b
    }

    #[test]
    fn parses_change_records() {
        let mut first = record(0, "Users\\me\\下载\\a.zip");
        first.resize(first.len().next_multiple_of(4), 0);
        let next = first.len() as u32;
        let mut bytes = record(next, "Users\\me\\下载\\a.zip");
        bytes.resize(next as usize, 0);
        bytes.extend(record(0, "pagefile.sys"));
        assert_eq!(parse(&bytes), vec!["Users\\me\\下载\\a.zip".to_string(), "pagefile.sys".to_string()]);
        assert!(parse(&[1, 2, 3]).is_empty(), "残缺的数据不崩");
    }
}
