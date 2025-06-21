use std::os::unix::net::UnixStream;
use std::io::{Read, Write};

pub struct KvmProxy {
    sock: UnixStream,
}

impl KvmProxy {
    pub fn connect() -> Result<Self, String> {
        let sock = UnixStream::connect("/tmp/mykvm.sock").map_err(|e| format!("Failed to connect to mykvm.sock: {}", e))?;
        Ok(KvmProxy { sock })
    }

    pub fn ioctl(&mut self, req: u64, arg: Option<&[u8]>, resp_len: usize) -> Result<Vec<u8>, String> {
        println!("[vmm][kvmproxy] ioctl send req: 0x{:X}, arg.len: {}", req, arg.map(|a| a.len()).unwrap_or(0));
        // Протокол: [8 байт req][N байт arg]
        let mut buf = req.to_le_bytes().to_vec();
        if let Some(arg) = arg {
            buf.extend_from_slice(arg);
        }
        println!("[vmm][kvmproxy] ioctl send bytes: {:?}", buf);
        self.sock.write_all(&buf).map_err(|e| format!("ioctl send: {}", e))?;
        let mut resp = vec![0u8; resp_len];
        self.sock.read_exact(&mut resp).map_err(|e| format!("ioctl recv: {}", e))?;
        println!("[vmm][kvmproxy] ioctl recv bytes: {:?}", resp);
        Ok(resp)
    }
}
