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

fn main() {
    let state = Arc::new(State::new());

    let socket_path = "/tmp/mykvm.sock";
    let _ = fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path).expect("bind socket");
    println!("[mykvm] Listening on {}", socket_path);
    for stream in listener.incoming() {
        let state = state.clone();
        match stream {
            Ok(mut stream) => {
                thread::spawn(move || {
                    let mut buf = [0u8; 1024];
                    loop {
                        let n = match stream.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => n,
                            Err(_) => break,
                        };
                        let req = u64::from_le_bytes(buf[..8].try_into().unwrap()) as c_ulong;
                        handle_ioctl(req, &mut stream, &state);

                        // Логируем все входящие данные
                        println!("[mykvm] received {} bytes: {:02x?}", n, &buf[..n.min(32)]);
                        if &buf[..10] == b"FRAMEBUFFER" {
                            println!("[mykvm] FRAMEBUFFER command received");
                            // Получаем framebuffer и обновляем память VM
                            let mut fb = vec![0u8; 640*480*4];
                            if stream.read_exact(&mut fb).is_ok() {
                                println!("[mykvm] FRAMEBUFFER data received: {} bytes", fb.len());
                                let vms = state.vms.lock().unwrap();
                                if let Some(vm) = vms.values().next() {
                                    let mut vm = vm.lock().unwrap();
                                    let fb_offset = 0x2000_0000 - 0x100000;
                                    if vm.memory.len() > fb_offset + fb.len() {
                                        vm.memory[fb_offset..fb_offset+fb.len()].copy_from_slice(&fb);
                                    }
                                }
                            } else {
                                println!("[mykvm] FRAMEBUFFER data read failed");
                            }
                            continue;
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
            let _ = stream.write_all(&id.to_le_bytes());
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
        }
        KVM_GET_SREGS => {
            let vms = state.vms.lock().unwrap();
            if let Some(vm) = vms.values().next() {
                let vm = vm.lock().unwrap();
                if let Some(vcpu) = vm.vcpus.values().next() {
                    let _ = stream.write_all(&vcpu.sregs);
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
                    let _ = file.write_all(format!("P6\n640 480\n255\n").as_bytes());
                    for px in fb.chunks(4) {
                        let _ = file.write_all(&px[0..3]);
                    }
                    println!("[mykvm] framebuffer_dump.ppm saved");
                }
            }
            let exit_reason = 5u32; // KVM_EXIT_HLT
            let mut resp = [0u8; 4];
            resp[..4].copy_from_slice(&exit_reason.to_le_bytes());
            let _ = stream.write_all(&resp);
        }
        _ => {
            // Для остальных — просто echo
        }
    }
}
