//! Минимальный пользовательский эмулятор KVM (заглушка)
use std::os::unix::net::UnixListener;
use std::os::unix::io::{AsRawFd, RawFd};
use std::io::{Read, Write};
use std::fs;
use std::thread;
use std::os::unix::prelude::FromRawFd;
use std::os::raw::c_ulong;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::io::Write as _;

macro_rules! log {
    ($($arg:tt)*) => {{
        println!($($arg)*);
        std::io::stdout().flush().unwrap();
    }};
}

fn main() {
    // Перенаправляем stdout и stderr в файл mykvm.log для устойчивой фоновой работы
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;
    let log = OpenOptions::new().create(true).append(true).open("mykvm.log").unwrap();
    let fd = log.as_raw_fd();
    unsafe {
        libc::dup2(fd, libc::STDOUT_FILENO);
        libc::dup2(fd, libc::STDERR_FILENO);
    }

    let state = Arc::new(State::new());

    let socket_path = "/tmp/mykvm.sock";
    let _ = fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path).expect("bind socket");
    log!("[mykvm] Listening on {}", socket_path);
    for stream in listener.incoming() {
        let now = chrono::Local::now();
        log!("[mykvm] accept: waiting for connection... [{}]", now.format("%Y-%m-%d %H:%M:%S"));
        let state = state.clone();
        match stream {
            Ok(mut stream) => {
                let thread_id = std::thread::current().id();
                let now = chrono::Local::now();
                log!("[mykvm] New incoming connection [thread {:?}] at {}", thread_id, now.format("%Y-%m-%d %H:%M:%S"));
                thread::spawn(move || {
                    log!("[mykvm] NEW CYCLE ACTIVE [thread {:?}]", thread_id);
                    loop {
                        // Сначала читаем 8 байт
                        let mut req_buf = [0u8; 8];
                        let now = chrono::Local::now();
                        log!("[mykvm] [thread {:?}] before read_exact at {}", thread_id, now.format("%Y-%m-%d %H:%M:%S"));
                        match stream.read_exact(&mut req_buf) {
                            Ok(_) => {
                                let ascii = std::str::from_utf8(&req_buf).unwrap_or("");
                                log!("[mykvm] [thread {:?}] raw 8 bytes: {:?} ascii: {}", thread_id, req_buf, ascii);
                                // Проверяем: если это начало FRAMEBUFFER
                                if &req_buf == b"FRAMEBUF" {
                                    let mut rest = [0u8; 3];
                                    if let Err(e) = stream.read_exact(&mut rest) {
                                        log!("[mykvm] framebuffer header read error: {}", e);
                                        break;
                                    }
                                    let mut header = req_buf.to_vec();
                                    header.extend_from_slice(&rest);
                                    let header_ascii = std::str::from_utf8(&header).unwrap_or("");
                                    log!("[mykvm] framebuffer header: {:?} ascii: {}", header, header_ascii);
                                    if &header == b"FRAMEBUFFER" {
                                        log!("[mykvm] FRAMEBUFFER command received (header)");
                                        let mut fb = vec![0u8; 640*480*4];
                                        if let Err(e) = stream.read_exact(&mut fb) {
                                            log!("[mykvm] framebuffer read error: {}", e);
                                            break;
                                        }
                                        let path = "framebuffer_dump.ppm";
                                        if let Ok(mut file) = std::fs::File::create(&path) {
                                            // Корректный PPM: P6\n640 480\n255\n
                                            let _ = file.write_all(b"P6\n640 480\n255\n");
                                            for px in fb.chunks(4) {
                                                let _ = file.write_all(&px[0..3]);
                                            }
                                            file.flush().ok();
                                            log!("[mykvm] framebuffer_dump.ppm saved (on FRAMEBUFFER)");
                                            let _ = std::process::Command::new("/home/rutasan/NeuroGame/mykvm/viewer/target/debug/viewer")
                                                .spawn();
                                        } else {
                                            log!("[mykvm] failed to save framebuffer_dump.ppm");
                                        }
                                        continue;
                                    }
                                }
                                // Если не FRAMEBUFFER — трактуем как ioctl
                                let req = u64::from_le_bytes(req_buf);
                                log!("[mykvm] ioctl req: 0x{:X}", req);
                                handle_ioctl(req, &mut stream, &state);
                            }
                            Err(e) => {
                                log!("[mykvm] connection closed or error: {}", e);
                                break;
                            }
                        }
                    }
                });
            }
            Err(e) => eprintln!("[mykvm] Accept error: {}", e),
        }
    }
}

// Минимальные константы ioctls
const KVM_CREATE_VM: c_ulong = 0xAE01;
const KVM_CREATE_VCPU: c_ulong = 0xAE41;
const KVM_GET_SREGS: c_ulong = 0x8138AE80;
const KVM_GET_REGS: c_ulong = 0x8090AE81;
const KVM_SET_USER_MEMORY_REGION: c_ulong = 0xAE02;
const KVM_SET_REGS: c_ulong = 0xAE40;
const KVM_RUN: c_ulong = 0xAE44;

#[repr(C, align(8))]
struct KvmSregs {
    cs: [u8; 24],
    ds: [u8; 24],
    es: [u8; 24],
    fs: [u8; 24],
    gs: [u8; 24],
    ss: [u8; 24],
    tr: [u8; 24],
    ldt: [u8; 24],
    gdt: [u8; 16],
    idt: [u8; 16],
    cr0: u64,
    cr2: u64,
    cr3: u64,
    cr4: u64,
    cr8: u64,
    efer: u64,
    apic_base: u64,
    interrupt_bitmap: [u64; 4],
}

// --- Минимальные структуры для VM/VCPU ---
struct Vm {
    memory: Vec<u8>,
    vcpus: HashMap<u64, Vcpu>,
}
struct Vcpu {
    regs: [u8; 184], // размер kvm_regs
    sregs: [u8; 312], // размер kvm_sregs
}

struct State {
    vms: Mutex<HashMap<u64, Arc<Mutex<Vm>>>>,
    next_vm_id: Mutex<u64>,
    next_vcpu_id: Mutex<u64>,
}

impl State {
    fn new() -> Self {
        State {
            vms: Mutex::new(HashMap::new()),
            next_vm_id: Mutex::new(1),
            next_vcpu_id: Mutex::new(1),
        }
    }
}

// --- В main ---
fn handle_ioctl(req: c_ulong, stream: &mut std::os::unix::net::UnixStream, state: &Arc<State>) {
    match req {
        KVM_CREATE_VM => {
            let mut vms = state.vms.lock().unwrap();
            let mut next_id = state.next_vm_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            vms.insert(id, Arc::new(Mutex::new(Vm {
                memory: vec![0; 0x400000], // 4 МБ памяти
                vcpus: HashMap::new(),
            })));
            log!("[mykvm] KVM_CREATE_VM: sending id {}", id);
            let _ = stream.write_all(&id.to_le_bytes());
            let _ = stream.flush();
            log!("[mykvm] KVM_CREATE_VM: sent id, flushed");
        }
        KVM_CREATE_VCPU => {
            let mut vms = state.vms.lock().unwrap();
            let mut next_vcpu = state.next_vcpu_id.lock().unwrap();
            let vcpu_id = *next_vcpu;
            *next_vcpu += 1;
            // Для простоты: всегда используем первую VM
            if let Some(vm) = vms.values().next() {
                let mut vm = vm.lock().unwrap();
                vm.vcpus.insert(vcpu_id, Vcpu { regs: [0; 184], sregs: [0; 312] });
            }
            let _ = stream.write_all(&vcpu_id.to_le_bytes());
            let _ = stream.flush();
        }
        KVM_GET_SREGS => {
            let vms = state.vms.lock().unwrap();
            if let Some(vm) = vms.values().next() {
                let vm = vm.lock().unwrap();
                if let Some(vcpu) = vm.vcpus.values().next() {
                    let _ = stream.write_all(&vcpu.sregs);
                    let _ = stream.flush();
                }
            }
        }
        KVM_GET_REGS => {
            let vms = state.vms.lock().unwrap();
            if let Some(vm) = vms.values().next() {
                let vm = vm.lock().unwrap();
                if let Some(vcpu) = vm.vcpus.values().next() {
                    let _ = stream.write_all(&vcpu.regs);
                    let _ = stream.flush();
                }
            }
        }
        KVM_SET_USER_MEMORY_REGION => {
            // Получаем параметры региона памяти (простая десериализация)
            let mut region = [0u8; 40];
            if stream.read_exact(&mut region).is_ok() {
                // guest_phys_addr = u64, memory_size = u64, userspace_addr = u64
                let guest_phys_addr = u64::from_le_bytes(region[8..16].try_into().unwrap());
                let memory_size = u64::from_le_bytes(region[16..24].try_into().unwrap());
                // Для простоты: выделяем память в VM
                let vms = state.vms.lock().unwrap();
                if let Some(vm) = vms.values().next() {
                    let mut vm = vm.lock().unwrap();
                    vm.memory = vec![0; memory_size as usize];
                }
            }
            let mut ack = [1u8; 8];
            let _ = stream.write_all(&ack);
            let _ = stream.flush();
        }
        KVM_SET_REGS => {
            // Примитивная эмуляция: сохраняем regs в VCPU
            let mut buf = [0u8; 184];
            if stream.read_exact(&mut buf).is_ok() {
                let vms = state.vms.lock().unwrap();
                if let Some(vm) = vms.values().next() {
                    let mut vm = vm.lock().unwrap();
                    if let Some(vcpu) = vm.vcpus.values_mut().next() {
                        vcpu.regs.copy_from_slice(&buf);
                    }
                }
            }
            let mut ack = [1u8; 8];
            let _ = stream.write_all(&ack);
            let _ = stream.flush();
        }
        KVM_RUN => {
            // Примитивная эмуляция: копируем framebuffer из памяти гостя и сохраняем в файл
            let vms = state.vms.lock().unwrap();
            if let Some(vm) = vms.values().next() {
                let vm = vm.lock().unwrap();
                // Параметры framebuffer
                let fb_offset = 0x2000_0000 - 0x100000; // guest_phys_addr - base
                let fb_size = 640 * 480 * 4;
                if vm.memory.len() > fb_offset + fb_size {
                    let fb = &vm.memory[fb_offset..fb_offset+fb_size];
                    let path = format!("framebuffer_dump.ppm");
                    let mut file = std::fs::File::create(&path).unwrap();
                    // Корректный PPM: P6\n640 480\n255\n
                    let _ = file.write_all(b"P6\n640 480\n255\n");
                    for px in fb.chunks(4) {
                        let _ = file.write_all(&px[0..3]);
                    }
                    file.flush().ok();
                    log!("[mykvm] framebuffer_dump.ppm saved");
                }
            }
            let exit_reason = 5u32; // KVM_EXIT_HLT
            let mut resp = [0u8; 4];
            resp[..4].copy_from_slice(&exit_reason.to_le_bytes());
            let _ = stream.write_all(&resp);
            let _ = stream.flush();
        }
        _ => {
            // Для остальных — просто echo
            log!("[mykvm] ioctl unknown req: 0x{:X}, echo 8 bytes", req);
            let mut resp = [0u8; 8];
            resp[0] = 0xFF; // для наглядности, что это echo
            let _ = stream.write_all(&resp);
            let _ = stream.flush();
        }
    }
}
