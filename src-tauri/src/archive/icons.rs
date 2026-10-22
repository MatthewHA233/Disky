//! 软件图标：从 exe / ico 取出图标，转成 PNG 缓存在 app 数据目录 icons/ 下。
//! 其他平台暂不支持，前端用首字母图标代替。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use base64::Engine;

/// 提取的图标边长（像素）。界面上按 40px 显示，留出高分屏余量。
const ICON_SIZE: u32 = 64;

fn cache_file(dir: &Path, source: &str) -> Option<PathBuf> {
    let meta = std::fs::metadata(source).ok()?;
    let mut h = DefaultHasher::new();
    source.to_ascii_lowercase().hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified().ok().hash(&mut h);
    Some(dir.join(format!("{:016x}.png", h.finish())))
}

/// 取一个文件的图标，返回 `data:image/png;base64,...`；取不到返回 None
pub fn icon_data_url(cache_dir: &Path, source: &str) -> Option<String> {
    let cached = cache_file(cache_dir, source)?;
    let png = match std::fs::read(&cached) {
        Ok(bytes) => bytes,
        Err(_) => {
            let bytes = extract_png(source, ICON_SIZE)?;
            let _ = std::fs::create_dir_all(cache_dir);
            let _ = std::fs::write(&cached, &bytes);
            bytes
        }
    };
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

#[cfg(not(windows))]
fn extract_png(_source: &str, _size: u32) -> Option<Vec<u8>> {
    None
}

#[cfg(windows)]
fn extract_png(source: &str, size: u32) -> Option<Vec<u8>> {
    let (w, h, rgba) = win::extract_rgba(source, size)?;
    encode_png(w, h, &rgba)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HBITMAP,
    };
    use windows_sys::Win32::UI::Shell::SHDefExtractIconW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    /// 读一张位图的 32 位像素（BGRA，自上而下）
    unsafe fn bitmap_bits(bmp: HBITMAP, w: i32, h: i32) -> Option<Vec<u8>> {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {
            return None;
        }
        let mut info: BITMAPINFO = zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(dc, bmp, 0, h as u32, buf.as_mut_ptr() as *mut c_void, &mut info, DIB_RGB_COLORS);
        DeleteDC(dc);
        (lines == h).then_some(buf)
    }

    pub fn extract_rgba(source: &str, size: u32) -> Option<(u32, u32, Vec<u8>)> {
        let path: Vec<u16> = source.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let mut icon: HICON = std::ptr::null_mut();
            // 低 16 位：大图标尺寸；高 16 位：小图标尺寸（不需要）
            let hr = SHDefExtractIconW(path.as_ptr(), 0, 0, &mut icon, std::ptr::null_mut(), size | (16 << 16));
            if hr != 0 || icon.is_null() {
                return None;
            }
            let result = icon_rgba(icon);
            DestroyIcon(icon);
            result
        }
    }

    unsafe fn icon_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
        let mut ii: ICONINFO = zeroed();
        if GetIconInfo(icon, &mut ii) == 0 {
            return None;
        }
        let color = ii.hbmColor;
        let mask = ii.hbmMask;
        let result = (|| {
            if color.is_null() {
                return None; // 单色图标不处理
            }
            let mut bm: BITMAP = zeroed();
            if GetObjectW(color, size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut c_void) == 0 {
                return None;
            }
            let (w, h) = (bm.bmWidth, bm.bmHeight);
            if w <= 0 || h <= 0 || w > 512 || h > 512 {
                return None;
            }
            let mut px = bitmap_bits(color, w, h)?;
            // 老式图标没有 alpha 通道，透明区域在掩码里（掩码白 = 透明）
            if px.chunks_exact(4).all(|p| p[3] == 0) {
                let m = if mask.is_null() { None } else { bitmap_bits(mask, w, h) };
                for (i, p) in px.chunks_exact_mut(4).enumerate() {
                    let transparent = m.as_ref().is_some_and(|m| m[i * 4] != 0);
                    p[3] = if transparent { 0 } else { 255 };
                }
            }
            // BGRA → RGBA
            for p in px.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
            Some((w as u32, h as u32, px))
        })();
        if !color.is_null() {
            DeleteObject(color);
        }
        if !mask.is_null() {
            DeleteObject(mask);
        }
        result
    }
}
