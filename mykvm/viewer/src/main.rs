use minifb::{Key, Window, WindowOptions};
use std::fs;
use std::thread;
use std::time::Duration;
use std::io::{self, Write};

const WIDTH: usize = 640;
const HEIGHT: usize = 480;
const FILE: &str = "../framebuffer_dump.ppm";

fn load_ppm(path: &str) -> Result<Vec<u32>, String> {
    let data = fs::read(path).map_err(|e| format!("Read error: {}", e))?;
    let header_end = data.windows(3).position(|w| w == b"\n255").map(|i| i + 4).ok_or("Header end not found")?;
    let header = String::from_utf8_lossy(&data[..header_end]);
    let mut lines = header.lines();
    if lines.next() != Some("P6") { return Err("Not P6".into()); }
    let dims = lines.next().ok_or("No dims")?;
    let mut dims = dims.split_whitespace();
    let w: usize = dims.next().ok_or("No width")?.parse().map_err(|_| "Bad width")?;
    let h: usize = dims.next().ok_or("No height")?.parse().map_err(|_| "Bad height")?;
    let maxval: usize = lines.next().ok_or("No maxval")?.parse().map_err(|_| "Bad maxval")?;
    if w != WIDTH || h != HEIGHT || maxval != 255 { return Err(format!("Bad dims: {}x{}x{}", w, h, maxval)); }
    let pixel_data = &data[header_end..];
    if pixel_data.len() < WIDTH * HEIGHT * 3 { return Err("Not enough pixel data".into()); }
    let mut buf = Vec::with_capacity(WIDTH * HEIGHT);
    for i in 0..(WIDTH * HEIGHT) {
        let r = pixel_data[i * 3] as u32;
        let g = pixel_data[i * 3 + 1] as u32;
        let b = pixel_data[i * 3 + 2] as u32;
        buf.push((r << 16) | (g << 8) | b);
    }
    Ok(buf)
}

fn main() {
    let mut window = Window::new(
        "Framebuffer Viewer",
        WIDTH,
        HEIGHT,
        WindowOptions::default(),
    ).unwrap_or_else(|e| {
        eprintln!("Window error: {}", e);
        std::process::exit(1);
    });
    let mut error_count = 0;
    let max_errors = 10;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        match load_ppm(FILE) {
            Ok(buf) => {
                window.update_with_buffer(&buf, WIDTH, HEIGHT).unwrap();
                error_count = 0; // сброс при успехе
            }
            Err(e) => {
                eprintln!("PPM error: {}", e);
                window.set_title(&format!("Framebuffer Viewer [error: {}]", e));
                error_count += 1;
                if error_count >= max_errors {
                    eprintln!("Слишком много ошибок чтения кадра, окно закрывается.");
                    break;
                }
                thread::sleep(Duration::from_secs(1));
                continue;
            }
        }
        thread::sleep(Duration::from_millis(30));
    }
}
