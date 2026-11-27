//! Windows: a monitor copied out of the screen with GDI, at its real pixels (the app is DPI aware, so the
//! monitor's rectangle is in real pixels too).

use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
};

use super::Frame;
use crate::desktop::Monitor;

pub fn grab(m: &Monitor) -> Result<Frame, String> {
    let (w, h) = (m.width as i32, m.height as i32);
    if w <= 0 || h <= 0 {
        return Err("the monitor has no size".to_owned());
    }
    let mut bgra = vec![0u8; w as usize * h as usize * 4];
    // SAFETY: every handle is checked, used on this thread only and released before returning; GetDIBits
    // writes at most w * h * 4 bytes into `bgra`, which is that long.
    let copied = unsafe {
        let screen = GetDC(std::ptr::null_mut());
        if screen.is_null() {
            return Err("could not read the screen".to_owned());
        }
        let memory = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(memory, bitmap);
        let blitted = BitBlt(memory, 0, 0, w, h, screen, m.x, m.y, SRCCOPY | CAPTUREBLT) != 0;
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            // negative: rows from the top down
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        SelectObject(memory, old);
        let lines =
            GetDIBits(memory, bitmap, 0, h as u32, bgra.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(std::ptr::null_mut(), screen);
        blitted && lines == h
    };
    if !copied {
        return Err("could not copy the screen".to_owned());
    }
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 255;
    }
    Ok(Frame { width: w as u32, height: h as u32, rgba: bgra })
}
