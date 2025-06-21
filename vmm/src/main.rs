mod syscall;
mod kvmproxy;

use crate::kvmproxy::KvmProxy;
use crate::syscall::{sys_ioctl, sys_mmap, sys_open, KVM_CREATE_VM, KVM_CREATE_VCPU, MAP_SHARED, O_RDWR, PROT_READ, PROT_WRITE};
use core::ptr;
use std::io::Write;
use std::os::unix::net::UnixStream;

pub fn open_kvm() -> Result<KvmProxy, String> {
    KvmProxy::connect()
}

pub fn create_vm_fd(proxy: &mut KvmProxy) -> Result<u64, String> {
    // KVM_CREATE_VM: только req, без arg, ответ — 8 байт (u64 id)
    let resp = proxy.ioctl(0xAE01, None, 8)?;
    Ok(u64::from_le_bytes(resp.try_into().unwrap()))
}

pub fn create_vcpu_fd(proxy: &mut KvmProxy) -> Result<u64, String> {
    // KVM_CREATE_VCPU: только req, без arg, ответ — 8 байт (u64 id)
    let resp = proxy.ioctl(0xAE41, None, 8)?;
    Ok(u64::from_le_bytes(resp.try_into().unwrap()))
}

const MAP_ANONYMOUS: i32 = 0x20;
const KVM_SET_USER_MEMORY_REGION: usize = 0x4020ae46;

#[repr(C)]
struct kvm_userspace_memory_region {
    slot: u32,
    flags: u32,
    guest_phys_addr: u64,
    memory_size: u64,
    userspace_addr: u64,
}

pub fn map_guest_memory(vm_fd: i32, size: usize) -> Result<*mut u8, String> {
    // 1. mmap анонимную память
    let ptr = unsafe {
        sys_mmap(
            core::ptr::null_mut(),
            size,
            PROT_READ | PROT_WRITE,
            MAP_SHARED | MAP_ANONYMOUS,
            -1, // fd = -1
            0,
        )
    };
    if ptr.is_null() || (ptr as isize) < 0 {
        eprintln!("[vmm] sys_mmap вернул ошибку: ptr = {:p} (as isize: {})", ptr, ptr as isize);
        return Err(format!("sys_mmap вернул ошибку: ptr = {:p} (as isize: {})", ptr, ptr as isize));
    }
    // 2. Зарегистрировать память в KVM
    // Изменён guest_phys_addr на 0x100000 (1 МБ)
    let region = kvm_userspace_memory_region {
        slot: 0,
        flags: 0,
        guest_phys_addr: 0x100000, // стандартный адрес загрузки ядра x86_64
        memory_size: size as u64,
        userspace_addr: ptr as u64,
    };
    let ret = unsafe { sys_ioctl(vm_fd, KVM_SET_USER_MEMORY_REGION, &region as *const _ as usize) };
    if ret < 0 {
        eprintln!("[vmm] KVM_SET_USER_MEMORY_REGION failed: {}", ret);
        return Err(format!("KVM_SET_USER_MEMORY_REGION failed: {}", ret));
    }
    Ok(ptr)
}

const KVM_GET_VCPU_MMAP_SIZE: usize = 0xAE04;

/// Возвращает размер области, которую нужно mmap для VCPU.
pub fn get_run_size(kvm_fd: i32) -> Result<usize, String> {
    let size = unsafe { sys_ioctl(kvm_fd, KVM_GET_VCPU_MMAP_SIZE, 0) };
    if size < 0 {
        Err(format!("KVM_GET_VCPU_MMAP_SIZE failed: {}", size))
    } else {
        Ok(size as usize)
    }
}

/// Мапит область управления VCPU размером `size`.
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
        Err("mmap run area failed".into())
    } else {
        Ok(ptr)
    }
}

pub struct Vmm {
    pub proxy: KvmProxy,
    pub vm_id: u64,
    pub vcpu_id: u64,
    pub guest_mem: Vec<u8>,
}

const KVM_CREATE_IRQCHIP: usize = 0xae60;

pub fn create_vm(memory_size: usize) -> Result<Vmm, String> {
    let mut proxy = open_kvm()?;
    let vm_id = create_vm_fd(&mut proxy)?;
    let vcpu_id = create_vcpu_fd(&mut proxy)?;
    let guest_mem = vec![0u8; memory_size];
    Ok(Vmm { proxy, vm_id, vcpu_id, guest_mem })
}

pub fn run_vcpu(proxy: &mut KvmProxy, vcpu_id: u64) -> Result<(), String> {
    // KVM_RUN: req, vcpu_id (8 байт), ответ — 4 байта exit_reason
    let arg = vcpu_id.to_le_bytes();
    let resp = proxy.ioctl(0xAE44, Some(&arg), 4)?;
    let exit_reason = u32::from_le_bytes(resp.try_into().unwrap());
    if exit_reason == 5 { Ok(()) } else { Err(format!("KVM_RUN exit_reason: {}", exit_reason)) }
}

const KVM_GET_SREGS: usize = 0x8138AE80;
const KVM_SET_SREGS: usize = 0x4138AE81;
const KVM_GET_REGS: usize = 0x8090AE81;

#[repr(C, align(8))]
pub struct kvm_segment {
    pub base: u64,
    pub limit: u32,
    pub selector: u16,
    pub type_: u8,
    pub present: u8,
    pub dpl: u8,
    pub db: u8,
    pub s: u8,
    pub l: u8,
    pub g: u8,
    pub avl: u8,
    pub unusable: u8,
    pub padding: u8,
}

#[repr(C, align(8))]
pub struct kvm_dtable {
    pub base: u64,
    pub limit: u16,
    pub padding: [u16; 3],
}

#[repr(C, align(8))]
pub struct kvm_sregs {
    pub cs: kvm_segment,
    pub ds: kvm_segment,
    pub es: kvm_segment,
    pub fs: kvm_segment,
    pub gs: kvm_segment,
    pub ss: kvm_segment,
    pub tr: kvm_segment,
    pub ldt: kvm_segment,
    pub gdt: kvm_dtable,
    pub idt: kvm_dtable,
    pub cr0: u64,
    pub cr2: u64,
    pub cr3: u64,
    pub cr4: u64,
    pub cr8: u64,
    pub efer: u64,
    pub apic_base: u64,
    pub interrupt_bitmap: [u64; 4],
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

pub fn setup_sregs(proxy: &mut KvmProxy, vcpu_id: u64, sregs: &[u8]) -> Result<(), String> {
    // KVM_SET_SREGS: req, vcpu_id (8 байт) + sregs (312 байт), ответ — 8 байт ack
    let mut arg = vcpu_id.to_le_bytes().to_vec();
    arg.extend_from_slice(sregs);
    let resp = proxy.ioctl(0xAE82, Some(&arg), 8)?;
    if resp[0] == 1 { Ok(()) } else { Err("KVM_SET_SREGS failed".to_string()) }
}

pub fn setup_regs(proxy: &mut KvmProxy, vcpu_id: u64, regs: &[u8]) -> Result<(), String> {
    // KVM_SET_REGS: req, vcpu_id (8 байт) + regs (184 байт), ответ — 8 байт ack
    let mut arg = vcpu_id.to_le_bytes().to_vec();
    arg.extend_from_slice(regs);
    let resp = proxy.ioctl(0xAE80, Some(&arg), 8)?;
    if resp[0] == 1 { Ok(()) } else { Err("KVM_SET_REGS failed".to_string()) }
}

pub fn get_sregs(proxy: &mut KvmProxy, vcpu_id: u64) -> Result<Vec<u8>, String> {
    // KVM_GET_SREGS: req, vcpu_id (8 байт), ответ — 312 байт sregs
    let arg = vcpu_id.to_le_bytes();
    let resp = proxy.ioctl(0xAE80, Some(&arg), 312)?;
    Ok(resp)
}

pub fn get_regs(proxy: &mut KvmProxy, vcpu_id: u64) -> Result<Vec<u8>, String> {
    // KVM_GET_REGS: req, vcpu_id (8 байт), ответ — 184 байт regs
    let arg = vcpu_id.to_le_bytes();
    let resp = proxy.ioctl(0xAE81, Some(&arg), 184)?;
    Ok(resp)
}

// Удалены дублирующиеся определения:
// pub fn setup_sregs(proxy: &mut KvmProxy, vcpu_id: u64) -> Result<(), String> { ... }
// pub fn setup_regs(proxy: &mut KvmProxy, vcpu_id: u64, entry: u64, stack: u64) -> Result<(), String> { ... }
// Оставлены только новые версии с сигнатурами:
// pub fn setup_sregs(proxy: &mut KvmProxy, vcpu_id: u64, sregs: &[u8]) -> Result<(), String>
// pub fn setup_regs(proxy: &mut KvmProxy, vcpu_id: u64, regs: &[u8]) -> Result<(), String>

/// Загружает файл по пути `path` в память гостя.
pub fn load_guest_kernel(vm: &mut Vmm, path: &str, guest_mem_size: usize) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| format!("Ошибка чтения ядра: {}", e))?;
    let len = data.len().min(guest_mem_size);
    vm.guest_mem[..len].copy_from_slice(&data[..len]);
    Ok(())
}

use crate::syscall::*;

const KVM_EXIT_IO: u32  = 2;
const KVM_EXIT_HLT: u32 = 5;

#[repr(C)]
struct kvm_run_io {
    direction: u8,
    size: u8,
    port: u16,
    count: u32,
    data_offset: u64,
    _pad: [u8; 40],
}

const FRAMEBUFFER_ADDR: usize = 0x2000_0000;
const WIDTH: usize = 640;
const HEIGHT: usize = 480;

/// Дампит содержимое guest framebuffer в файл "frame_<num>.ppm"
fn dump_frame(vm: &Vmm, frame_num: usize) -> Result<(), String> {
    use std::fs::File;
    use std::io::Write;
    let fb_offset = 0x2000_0000 - 0x100000;
    let fb_size = 640 * 480 * 4;
    if vm.guest_mem.len() < fb_offset + fb_size {
        return Err("Framebuffer выходит за пределы выделенной памяти VM".to_string());
    }
    let fb_slice = &vm.guest_mem[fb_offset..fb_offset+fb_size];
    let path = format!("frames/frame_{}.ppm", frame_num);
    let mut file = File::create(&path).map_err(|e| e.to_string())?;
    file.write_all(format!("P6\n{} {}\n255\n", 640, 480).as_bytes())
        .map_err(|e| e.to_string())?;
    for px in fb_slice.chunks(4) {
        file.write_all(&px[0..3]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// После KVM_RUN отправляем framebuffer в эмулятор
pub fn send_framebuffer(vm: &Vmm) {
    println!("[vmm] send_framebuffer: start");
    let fb_offset = 0x2000_0000 - 0x100000;
    let fb_size = 640 * 480 * 4;
    let fb_slice = &vm.guest_mem[fb_offset..fb_offset+fb_size];
    // Диагностика: дамп первых 64 байт framebuffer
    let dump = fb_slice.iter().take(64).map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
    println!("[vmm] framebuffer head (64): {} ({} bytes)", dump, fb_slice.len());
    match std::os::unix::net::UnixStream::connect("/tmp/mykvm.sock") {
        Ok(mut sock) => {
            println!("[vmm] send_framebuffer: connected to mykvm.sock");
            if let Err(e) = sock.write_all(b"FRAMEBUFFER") {
                println!("[vmm] send_framebuffer: failed to send header: {}", e);
                return;
            }
            println!("[vmm] send_framebuffer: header sent");
            if let Err(e) = sock.write_all(fb_slice) {
                println!("[vmm] send_framebuffer: failed to send data: {}", e);
                return;
            }
            println!("[vmm] send_framebuffer: sent {} bytes", fb_slice.len());
        }
        Err(e) => {
            println!("[vmm] send_framebuffer: failed to connect: {}", e);
        }
    }
    println!("[vmm] send_framebuffer: finished");
}

fn main() {
    use core::mem::{size_of, align_of};
    println!("[vmm] kvm_sregs size: {} align: {}", size_of::<kvm_sregs>(), align_of::<kvm_sregs>());
    println!("[vmm] kvm_segment size: {} align: {}", size_of::<kvm_segment>(), align_of::<kvm_segment>());

    let kernel_path = "../kernel/target/x86_64-unknown-none/debug/kernel";
    let memory_size = 0x20200000;
    println!("[vmm] before create_vm");
    let mut vmm = match create_vm(memory_size) {
        Ok(vmm) => vmm,
        Err(e) => {
            eprintln!("[vmm] create_vm error: {}", e);
            return;
        }
    };
    println!("[vmm] after create_vm");
    if let Err(e) = load_guest_kernel(&mut vmm, kernel_path, memory_size) {
        eprintln!("[vmm] load_guest_kernel error: {}", e);
    }
    println!("[vmm] after load_guest_kernel");
    use std::time::Instant;
    let start_time = Instant::now();
    let timeout = std::time::Duration::from_secs(10); // 10 секунд
    let mut sregs = match get_sregs(&mut vmm.proxy, vmm.vcpu_id) {
        Ok(s) => {
            println!("[vmm] get_sregs: успешно получено {} байт", s.len());
            s
        },
        Err(e) => {
            eprintln!("[vmm] get_sregs error: {}", e);
            vec![0u8; 312]
        }
    };
    let mut regs = match get_regs(&mut vmm.proxy, vmm.vcpu_id) {
        Ok(r) => {
            println!("[vmm] get_regs: успешно получено {} байт", r.len());
            r
        },
        Err(e) => {
            eprintln!("[vmm] get_regs error: {}", e);
            vec![0u8; 184]
        }
    };

    for frame in 0..300 {
        if start_time.elapsed() > timeout {
            println!("[vmm] Таймаут: выполнение завершено через 10 секунд");
            break;
        }
        println!("[vmm] === FRAME {} ===", frame);
        println!("[vmm] before setup_sregs");
        let sregs_result = setup_sregs(&mut vmm.proxy, vmm.vcpu_id, &sregs);
        if let Err(e) = &sregs_result {
            eprintln!("[vmm] setup_sregs error: {}", e);
        } else {
            println!("[vmm] setup_sregs: ok");
        }
        println!("[vmm] after setup_sregs");
        println!("[vmm] before setup_regs");
        let regs_result = setup_regs(&mut vmm.proxy, vmm.vcpu_id, &regs);
        if let Err(e) = &regs_result {
            eprintln!("[vmm] setup_regs error: {}", e);
        } else {
            println!("[vmm] setup_regs: ok");
        }
        println!("[vmm] after setup_regs");
        println!("[vmm] before run_vcpu");
        let run_result = run_vcpu(&mut vmm.proxy, vmm.vcpu_id);
        if let Err(e) = &run_result {
            eprintln!("[vmm] run_vcpu error: {}", e);
        } else {
            println!("[vmm] run_vcpu: ok");
        }
        println!("[vmm] after run_vcpu");
        println!("[vmm] before send_framebuffer");
        send_framebuffer(&vmm);
        println!("[vmm] after send_framebuffer");
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
}
