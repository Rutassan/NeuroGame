mod syscall;

use crate::syscall::{sys_ioctl, sys_mmap, sys_open, KVM_CREATE_VM, KVM_CREATE_VCPU, MAP_SHARED, O_RDWR, PROT_READ, PROT_WRITE};
use core::ptr;

pub fn open_kvm() -> Result<i32, String> {
    let path = b"/dev/kvm\0";
    let fd = unsafe { sys_open(path.as_ptr(), O_RDWR) };
    if fd < 0 {
        Err("Failed to open /dev/kvm".to_string())
    } else {
        Ok(fd)
    }
}

pub fn create_vm_fd(kvm_fd: i32) -> Result<i32, String> {
    let vm_fd = unsafe { sys_ioctl(kvm_fd, KVM_CREATE_VM, 0) };
    if vm_fd < 0 {
        Err("Failed to create VM fd".to_string())
    } else {
        Ok(vm_fd)
    }
}

pub fn create_vcpu_fd(vm_fd: i32) -> Result<i32, String> {
    let vcpu_fd = unsafe { sys_ioctl(vm_fd, KVM_CREATE_VCPU, 0) };
    if vcpu_fd < 0 {
        Err("Failed to create VCPU fd".to_string())
    } else {
        Ok(vcpu_fd)
    }
}

pub fn map_guest_memory(vm_fd: i32, size: usize) -> Result<*mut u8, String> {
    let ptr = unsafe {
        sys_mmap(
            ptr::null_mut(),
            size,
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            vm_fd,
            0,
        )
    };
    if ptr.is_null() {
        Err("Failed to mmap guest memory".to_string())
    } else {
        Ok(ptr)
    }
}

const KVM_GET_VCPU_MMAP_SIZE: usize = 0xAE04;

pub fn get_run_size(kvm_fd: i32) -> Result<usize, String> {
    let size = unsafe { sys_ioctl(kvm_fd, KVM_GET_VCPU_MMAP_SIZE, 0) };
    if size < 0 {
        Err("Не удалось получить размер KVM_RUN mmap".to_string())
    } else {
        Ok(size as usize)
    }
}

pub fn map_run_area(vcpu_fd: i32, size: usize) -> Result<*mut u8, String> {
    let ptr = unsafe {
        sys_mmap(
            core::ptr::null_mut(),
            size,
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            vcpu_fd,
            0,
        )
    };
    if ptr.is_null() {
        Err("Не удалось mmap KVM_RUN область".to_string())
    } else {
        Ok(ptr)
    }
}

pub struct Vmm {
    pub kvm_fd: i32,
    pub vm_fd: i32,
    pub vcpu_fd: i32,
    pub guest_mem: *mut u8,
}

pub fn create_vm(memory_size: usize) -> Result<Vmm, String> {
    let kvm_fd = open_kvm()?;
    let vm_fd = create_vm_fd(kvm_fd)?;
    let vcpu_fd = create_vcpu_fd(vm_fd)?;
    let guest_mem = map_guest_memory(vm_fd, memory_size)?;
    Ok(Vmm { kvm_fd, vm_fd, vcpu_fd, guest_mem })
}

pub fn run_vcpu(vmm: &Vmm) -> Result<(), String> {
    const KVM_RUN: usize = 0xAE80; // _IO(KVMIO, 0x80)
    let ret = unsafe { sys_ioctl(vmm.vcpu_fd, KVM_RUN, 0) };
    if ret < 0 {
        Err("Не удалось запустить VCPU".to_string())
    } else {
        Ok(())
    }
}

#[repr(C)]
struct kvm_sregs {
    // ...минимальный набор полей для sregs, только cs
    _pad1: [u8; 0x18],
    cs: kvm_segment,
    _pad2: [u8; 0x2d0],
}

#[repr(C)]
struct kvm_segment {
    base: u64,
    limit: u32,
    selector: u16,
    type_: u8,
    present: u8,
    dpl: u8,
    db: u8,
    s: u8,
    l: u8,
    g: u8,
    avl: u8,
    unusable: u8,
    padding: u8,
}

#[repr(C)]
struct kvm_regs {
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rsp: u64,
    rbp: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rip: u64,
    rflags: u64,
}

const KVM_GET_SREGS: usize = 0x8138ae83;
const KVM_SET_SREGS: usize = 0x4138ae84;
const KVM_SET_REGS: usize  = 0x4090ae82;

pub fn setup_sregs(vcpu_fd: i32) -> Result<(), String> {
    let mut sregs = kvm_sregs {
        _pad1: [0; 0x18],
        cs: kvm_segment {
            base: 0,
            limit: 0xffff,
            selector: 0,
            type_: 0xb,
            present: 1,
            dpl: 0,
            db: 0,
            s: 1,
            l: 1,
            g: 1,
            avl: 0,
            unusable: 0,
            padding: 0,
        },
        _pad2: [0; 0x2d0],
    };
    let ret = unsafe { sys_ioctl(vcpu_fd, KVM_GET_SREGS, &mut sregs as *mut _ as usize) };
    if ret < 0 {
        return Err("KVM_GET_SREGS failed".to_string());
    }
    sregs.cs.base = 0;
    let ret = unsafe { sys_ioctl(vcpu_fd, KVM_SET_SREGS, &sregs as *const _ as usize) };
    if ret < 0 {
        Err("KVM_SET_SREGS failed".to_string())
    } else {
        Ok(())
    }
}

pub fn setup_regs(vcpu_fd: i32, entry: u64, stack: u64) -> Result<(), String> {
    let regs = kvm_regs {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rsp: stack,
        rbp: 0,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rip: entry,
        rflags: 2,
    };
    let ret = unsafe { sys_ioctl(vcpu_fd, KVM_SET_REGS, &regs as *const _ as usize) };
    if ret < 0 {
        Err("KVM_SET_REGS failed".to_string())
    } else {
        Ok(())
    }
}

use crate::syscall::*;

fn main() {
    match create_vm(0x400000) {
        Ok(vm) => {
            let run_size = match get_run_size(vm.kvm_fd) {
                Ok(sz) => sz,
                Err(e) => { eprintln!("Ошибка get_run_size: {}", e); return; }
            };
            let run_ptr = match map_run_area(vm.vcpu_fd, run_size) {
                Ok(ptr) => ptr,
                Err(e) => { eprintln!("Ошибка map_run_area: {}", e); return; }
            };
            if let Err(e) = setup_sregs(vm.vcpu_fd) {
                eprintln!("Ошибка setup_sregs: {}", e);
                return;
            }
            if let Err(e) = setup_regs(vm.vcpu_fd, 0x100000, 0x200000) {
                eprintln!("Ошибка setup_regs: {}", e);
                return;
            }
            println!("Starting VCPU...");
            loop {
                if let Err(e) = run_vcpu(&vm) {
                    eprintln!("Ошибка run_vcpu: {}", e);
                    break;
                }
                let exit_reason = unsafe { *(run_ptr as *const u32).offset(0) };
                println!("Exit reason: {:?}", exit_reason);
            }
        }
        Err(err) => {
            eprintln!("Ошибка при создании VM: {}", err);
        }
    }
}
