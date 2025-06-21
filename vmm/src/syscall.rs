#![no_std]

// System call numbers
pub const SYS_OPEN: usize        = 2;
pub const SYS_IOCTL: usize       = 16;
pub const SYS_MMAP: usize        = 9;

// Flags and constants
pub const O_RDWR: i32            = 2;
pub const PROT_READ: i32         = 1;
pub const PROT_WRITE: i32        = 2;
pub const MAP_SHARED: i32        = 1;

// KVM ioctls
pub const KVM_CREATE_VM: usize   = 0xAE01;  // _IO(KVMIO, 0x01)
pub const KVM_CREATE_VCPU: usize = 0xAE41;  // _IO(KVMIO, 0x41)

/// Прямой syscall open(path, flags)
#[inline(always)]
pub unsafe fn sys_open(path: *const u8, flags: i32) -> i32 {
    let ret: i32;
    core::arch::asm!(
        "syscall",
        in("rax") SYS_OPEN,
        in("rdi") path,
        in("rsi") flags,
        lateout("rax") ret,
        lateout("rcx") _,
        lateout("r11") _,
    );
    ret
}

/// Прямой syscall ioctl(fd, request, arg)
#[inline(always)]
pub unsafe fn sys_ioctl(fd: i32, request: usize, arg: usize) -> i32 {
    let ret: i32;
    core::arch::asm!(
        "syscall",
        in("rax") SYS_IOCTL,
        in("rdi") fd,
        in("rsi") request,
        in("rdx") arg,
        lateout("rax") ret,
        lateout("rcx") _,
        lateout("r11") _,
    );
    ret
}

/// Прямой syscall mmap(addr, len, prot, flags, fd, offset)
#[inline(always)]
pub unsafe fn sys_mmap(
    addr: *mut u8,
    len: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: usize
) -> *mut u8 {
    let ret: *mut u8;
    core::arch::asm!(
        "syscall",
        in("rax") SYS_MMAP,
        in("rdi") addr,
        in("rsi") len,
        in("rdx") prot,
        in("r10") flags,
        in("r8") fd,
        in("r9") offset,
        lateout("rax") ret,
        lateout("rcx") _,
        lateout("r11") _,
    );
    ret
}
