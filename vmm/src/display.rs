#![allow(dead_code)]

use crate::syscall::{sys_open, sys_ioctl, O_RDWR};

// DRM ioctl constants
const DRM_IOCTL_MODE_GETRESOURCES: u64 = 0xC01064A0; // _IOWR('d', 0xA0, drm_mode_card_res)

/// Открыть DRM-устройство
pub fn open_drm() -> Result<i32, String> {
    let path = b"/dev/dri/card0\0";
    let fd = unsafe { sys_open(path.as_ptr(), O_RDWR) };
    if fd < 0 {
        Err("Failed to open /dev/dri/card0".to_string())
    } else {
        Ok(fd)
    }
}

#[repr(C)]
pub struct drm_mode_card_res {
    pub fb_id_ptr: u64,
    pub crtc_id_ptr: u64,
    pub connector_id_ptr: u64,
    pub encoder_id_ptr: u64,
    pub count_fbs: u32,
    pub count_crtcs: u32,
    pub count_connectors: u32,
    pub count_encoders: u32,
    // ...остальные поля не нужны...
}

/// Получить ресурсы DRM
pub fn get_resources(fd: i32) -> Result<drm_mode_card_res, String> {
    let mut res = drm_mode_card_res {
        fb_id_ptr: 0,
        crtc_id_ptr: 0,
        connector_id_ptr: 0,
        encoder_id_ptr: 0,
        count_fbs: 0,
        count_crtcs: 0,
        count_connectors: 0,
        count_encoders: 0,
    };
    let ret = unsafe { sys_ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES as usize, &mut res as *mut _ as usize) };
    if ret < 0 {
        Err(format!("GETRESOURCES failed: {}", ret))
    } else {
        Ok(res)
    }
}
