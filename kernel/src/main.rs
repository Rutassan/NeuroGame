#![no_std]
#![no_main]

extern crate game;

use core::panic::PanicInfo;
use core::arch::asm;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    game::init();
    loop {
        game::update(0.016);
        game::render();
        unsafe { asm!("hlt"); }
    }
}
