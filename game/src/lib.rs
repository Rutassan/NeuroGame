#![no_std]

/// Вызывается из ядра при старте
#[no_mangle]
pub extern "C" fn init() {
    // пока пусто
}

/// Вызывается каждый кадр с дельтой времени в секундах
#[no_mangle]
pub extern "C" fn update(_delta: f32) {
    // пока пусто
}

/// Вызывается каждый кадр для рисования
#[no_mangle]
pub extern "C" fn render() {
    const WIDTH: usize = 640;
    const HEIGHT: usize = 480;
    const FRAMEBUFFER_ADDR: usize = 0x2000_0000;
    let buf = FRAMEBUFFER_ADDR as *mut u32;
    let color = 0xFF00FF00u32; // зелёный
    for i in 0..(WIDTH * HEIGHT) {
        unsafe { buf.add(i).write_volatile(color) };
    }
}
