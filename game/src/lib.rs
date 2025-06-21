#![no_std]

const WIDTH: usize = 640;
const HEIGHT: usize = 480;
const FRAMEBUFFER_ADDR: usize = 0x2000_0000;

static mut ANGLE: f32 = 0.0;

fn to_fixed(x: f32) -> i32 {
    (x * 1024.0) as i32
}

fn from_fixed(x: i32) -> f32 {
    (x as f32) / 1024.0
}

fn sin(x: f32) -> f32 {
    // Быстрая аппроксимация синуса (Тейлор, только для demo)
    let x = x % (2.0 * 3.14159265);
    let x3 = x * x * x;
    let x5 = x3 * x * x;
    x - x3 / 6.0 + x5 / 120.0
}

fn cos(x: f32) -> f32 {
    sin(x + 1.57079632)
}

fn put_pixel(x: i32, y: i32, color: u32) {
    if x < 0 || y < 0 || x >= WIDTH as i32 || y >= HEIGHT as i32 {
        return;
    }
    let offset = (y as usize * WIDTH + x as usize) as isize;
    unsafe {
        let fb = FRAMEBUFFER_ADDR as *mut u32;
        fb.offset(offset).write_volatile(color);
    }
}

fn draw_line(mut x0: i32, mut y0: i32, mut x1: i32, mut y1: i32, color: u32) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        put_pixel(x0, y0, color);
        if x0 == x1 && y0 == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x0 += sx; }
        if e2 <= dx { err += dx; y0 += sy; }
    }
}

// 8 вершин куба
const CUBE_VERTS: [(f32, f32, f32); 8] = [
    (-1.0, -1.0, -1.0),
    ( 1.0, -1.0, -1.0),
    ( 1.0,  1.0, -1.0),
    (-1.0,  1.0, -1.0),
    (-1.0, -1.0,  1.0),
    ( 1.0, -1.0,  1.0),
    ( 1.0,  1.0,  1.0),
    (-1.0,  1.0,  1.0),
];
// 12 рёбер (индексы вершин)
const CUBE_EDGES: [(usize, usize); 12] = [
    (0,1),(1,2),(2,3),(3,0), // нижняя грань
    (4,5),(5,6),(6,7),(7,4), // верхняя грань
    (0,4),(1,5),(2,6),(3,7), // боковые рёбра
];

/// Вызывается из ядра при старте
#[no_mangle]
pub extern "C" fn init() {
    unsafe { ANGLE = 0.0; }
}

/// Вызывается каждый кадр с дельтой времени в секундах
#[no_mangle]
pub extern "C" fn update(_delta: f32) {
    unsafe { ANGLE += 0.03; }
}

/// Вызывается каждый кадр для рисования
#[no_mangle]
pub extern "C" fn render() {
    // Очистить экран (чёрный)
    for i in 0..(WIDTH*HEIGHT) {
        unsafe { (FRAMEBUFFER_ADDR as *mut u32).add(i).write_volatile(0xFF000000); }
    }
    // Матрица поворота вокруг Y
    let angle = unsafe { ANGLE };
    let mut proj = [(0i32,0i32); 8];
    for (i, &(x, y, z)) in CUBE_VERTS.iter().enumerate() {
        let rx = x * cos(angle) + z * sin(angle);
        let rz = -x * sin(angle) + z * cos(angle);
        let scale = 120.0;
        let px = (rx * scale + (WIDTH/2) as f32) as i32;
        let py = (y * scale + (HEIGHT/2) as f32) as i32;
        proj[i] = (px, py);
    }
    // Нарисовать рёбра
    for &(a, b) in CUBE_EDGES.iter() {
        let (x0, y0) = proj[a];
        let (x1, y1) = proj[b];
        draw_line(x0, y0, x1, y1, 0xFFFFFFFF);
    }
}
