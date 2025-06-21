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

use crate::syscall::*;

fn main() {
    match create_vm(0x400000) {
        Ok(vm) => {
            println!("VM успешно создана: vcpu_fd = {}", vm.vcpu_fd);
            match run_vcpu(&vm) {
                Ok(()) => println!("VCPU запущен"),
                Err(e) => eprintln!("Ошибка запуска VCPU: {}", e),
            }
        }
        Err(err) => {
            eprintln!("Ошибка при создании VM: {}", err);
        }
    }
}
