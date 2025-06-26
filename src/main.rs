#![no_std]
#![no_main]
#![feature(offset_of)]

use core::mem::offset_of;
use core::arch::asm;
use core::cmp::min;
use core::mem::size_of;
use core::ptr::null_mut;
use core::panic::PanicInfo;
use core::writeln;
use core::fmt;
use core::fmt::Write;

type EfiVoid = u8;
type EfiHandle = u64;
type Result<T> = core::result::Result<T, &'static str>;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct EfiGuid {
    pub data0: u32,
    pub data1: u16,
    pub data2: u16,
    pub data3: [u8; 8]
}

const EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data0: 0x9042a9de,
    data1: 0x23dc,
    data2: 0x4a38,
    data3: [0x96, 0xfb, 0x7a, 0xde, 0xd0, 0x80, 0x51, 0x6a]
};

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
#[must_use]
#[repr(u64)]
enum EfiStatus {
    Success = 0,
}

#[repr(C)]
struct EfiBootServicesTable
{
    _reserved0: [u64; 7],
    get_memory_map: extern "win64" fn (
        memory_map_size: *mut usize,
        memory_map: *mut u8,
        map_key: *mut usize,
        descriptor_size: *mut usize,
        descriptor_version: *mut u32,
    ) -> EfiStatus,
    _reserved1: [u64; 32],
    locate_protocol: extern "win64" fn(
        protocol: *const EfiGuid,
        registration: *const EfiVoid,
        interface: *mut *mut EfiVoid
    ) -> EfiStatus,
}

impl EfiBootServicesTable {
    fn get_memory_map(&self,w: &mut VramTextWriter, map: &mut MemoryMapHolder) -> EfiStatus
    {
        //writeln!(w,"{} {} {} {}", map.memory_map_size, map.map_key, map.descriptor_size ,map.descriptor_version).unwrap();
        //writeln!(w,"{:?}", map.memory_map_buffer).unwrap();
        let r = (self.get_memory_map)(
            &mut map.memory_map_size,
            map.memory_map_buffer.as_mut_ptr(),
            &mut map.map_key,
            &mut map.descriptor_size,
            &mut map.descriptor_version
        );
        //writeln!(w,"{} {} {} {}", map.memory_map_size, map.map_key, map.descriptor_size ,map.descriptor_version).unwrap();
        //writeln!(w,"{:?}", map.memory_map_buffer).unwrap();
        return r;
    }
}

const _: () = assert!(offset_of!(EfiBootServicesTable, get_memory_map) == 56);
const _: () = assert!(offset_of!(EfiBootServicesTable, locate_protocol) == 320);

const MEMORY_MAP_BUFFER_SIZE:usize = 0x8000;

struct MemoryMapHolder{
    memory_map_buffer: [u8; MEMORY_MAP_BUFFER_SIZE],
    memory_map_size:usize,
    map_key:usize,
    descriptor_size:usize,
    descriptor_version:u32,
}

impl MemoryMapHolder 
{
    pub const fn new() -> MemoryMapHolder {
        MemoryMapHolder { 
            memory_map_buffer: [0; MEMORY_MAP_BUFFER_SIZE], 
            memory_map_size: MEMORY_MAP_BUFFER_SIZE , 
            map_key: 0, 
            descriptor_size: 0,
            descriptor_version: 0
        }
    }

    pub fn iter(&self) -> MemoryMapIterator
    {
        MemoryMapIterator { map: self, ofs: 0 }
    }
}

#[repr(i64)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(non_camel_case_types)]
pub enum EfiMemoryType {
    RESERVED=0,
    LOADER_CODE,
    LOADER_DATA,
    BOOT_SERVICE_CODE,
    BOOT_SERVICE_DATA,
    RUNTIME_SERVICE_CODE,
    RUNTIME_SERVICE_DATA,
    CONVENTIONAL_MEMORY,
    UNUSABLE_MEMORY,
    ACPI_RECLAIM_MEMORY,
    ACPI_MEMORY_NVS,
    MEMORY_MAPED_IO,
    MEMORY_MAPED_IO_PORT_SPACE,
    PAL_CODE,
    PERSISTENT_MEMORY,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct EfiMemoryDescriptor
{
    memory_type:EfiMemoryType,
    phisical_start: u64,
    virtual_start:u64,
    number_of_pages:u64,
    attribute:u64,
}

struct MemoryMapIterator<'a>
{
    map: &'a MemoryMapHolder,
    ofs: usize
}

impl<'a> Iterator for MemoryMapIterator<'a> {
    type Item = &'a EfiMemoryDescriptor;

    fn next(&mut self) -> Option<&'a EfiMemoryDescriptor>{
        if self.ofs >= self.map.memory_map_size {
            None
        } else {
            let e: &EfiMemoryDescriptor = unsafe {
                        &*(self.map.memory_map_buffer 
                            .as_ptr() 
                            .add(self.ofs) as *const EfiMemoryDescriptor)
            };

            self.ofs += self.map.descriptor_size;
            Some(e)
        }
    }
}


#[repr(C)]
struct EfiSystemTable
{
    _reserved0: [u64; 12],
    pub boot_services: &'static EfiBootServicesTable,
}

const _: () = assert!(offset_of!(EfiSystemTable, boot_services) == 96);

#[repr(C)]
#[derive(Debug)]
struct
EfiGraphicsOutputProtocolPixelInfo
{
    version: u32,
    pub horizontal_resolution: u32,
    pub vertical_resolution: u32,
    _padding0: [u32;5],
    pub pixels_per_scan_line:u32,
}

const _: () = assert!(size_of::<EfiGraphicsOutputProtocolPixelInfo>() == 36);

#[repr(C)]
#[derive(Debug)]
struct 
EfiGraphicsOutputProtocolMode<'a>
{
    pub max_mode: u32,
    pub mode: u32,
    pub info: &'a EfiGraphicsOutputProtocolPixelInfo,
    pub size_of_info: u64,
    pub frame_buffer_base: usize,
    pub frame_buffer_size: usize,
}

#[repr(C)]
#[derive(Debug)]
struct EfiGraphicsOutputProtocol<'a> {
    reserved: [u64; 3],
    pub mode: &'a EfiGraphicsOutputProtocolMode<'a>
}

fn locate_graphics_protocol<'a> (efi_system_tabe: &EfiSystemTable) -> Result<&'a EfiGraphicsOutputProtocol> {
    let mut graphic_outout_protocol = null_mut::<EfiGraphicsOutputProtocol>();
    let status = (efi_system_tabe.boot_services.locate_protocol)(&EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
            null_mut::<EfiVoid>(),
            &mut graphic_outout_protocol as *mut *mut EfiGraphicsOutputProtocol as *mut *mut EfiVoid
        );
    if status != EfiStatus::Success {
        return Err("Failed to locate graphics output protocol");
    }
    Ok(unsafe {
        &*graphic_outout_protocol
    })
}

trait Bitmap{
    fn bytes_per_pixel(&self) -> i64;
    fn pixels_per_line(&self) -> i64;
    fn width(&self) -> i64;
    fn height(&self) -> i64;

    fn buf_mut(&mut self)-> *mut u8;

    unsafe fn uncheck_pixel_at_mut(&mut self, x:i64, y:i64) -> *mut u32
    {
        self.buf_mut().add(
            ((y * self.pixels_per_line() + x) * self.bytes_per_pixel()) 
            as usize
        ) as *mut u32
    }

    fn pixel_at_mut(&mut self, x:i64, y:i64) -> Option<&mut u32>
    {
        if self.is_in_x_range(x) && self.is_in_y_range(y) {
            unsafe {Some(&mut *(self.uncheck_pixel_at_mut(x, y)))}
        }
        else
        {
            None
        }
    }

    fn is_in_x_range(&self, px:i64) -> bool {
        0 <= px && px < min(self.width(), self.pixels_per_line())
    }

    fn is_in_y_range(&self, py:i64) -> bool {
        0 <= py && py < self.height()
    }
}


#[derive(Clone, Copy)]
struct VramBufferInfo{
    buf: *mut u8,
    width: i64,
    height: i64,
    pixels_per_line: i64
}

impl Bitmap for VramBufferInfo {
    fn bytes_per_pixel(&self) -> i64 {
        4
    }
    fn pixels_per_line(&self) -> i64 {
        self.pixels_per_line
    }
    fn width(&self) -> i64 {
        self.width
    }
    fn height(&self) -> i64 {
        self.height
    }
    fn buf_mut(&mut self)-> *mut u8 {
        self.buf
    }
}

fn init_vram(efi_system_table: &EfiSystemTable) -> Result<VramBufferInfo> {
    let gp = locate_graphics_protocol(efi_system_table)?;
    Ok(
        VramBufferInfo {
            buf: gp.mode.frame_buffer_base as *mut u8,
            width: gp.mode.info.horizontal_resolution as i64,
            height: gp.mode.info.vertical_resolution as i64,
            pixels_per_line: gp.mode.info.pixels_per_scan_line as i64 
        }
    )
}

#[no_mangle]
fn efi_main(_image_handle: EfiHandle, efi_system_table: &EfiSystemTable) {
    let mut vram = init_vram(efi_system_table).expect("init vram failed");
    let vw = vram.width();
    let vh = vram.height();

    let _ = fill_rect(& mut vram, 0x000000, 0, 0, vw, vh);

    let grid_size:i64 = 32;
    let rect_size:i64 = grid_size * 8;

    let _ = fill_rect(&mut vram, 0xff0000, 0, 0, 32, 32);
    let _ = fill_rect(&mut vram, 0x00ff00, 32, 32, 32, 32);
    let _ = fill_rect(&mut vram, 0x0000ff, 64, 64, 32, 32);


    for i in (0..rect_size).step_by(grid_size as usize){
        let _ = draw_line(&mut vram, 0xff0000, 0, i, rect_size, i);
        let _ = draw_line(&mut vram, 0xff0000, i, 0, i, rect_size);
    }
    let cx = rect_size / 2;
    let cy = rect_size / 2;
    for i in (0..rect_size).step_by(grid_size as usize){
        let _ = draw_line(&mut vram, 0xffff00, cx, cy, 0, i);
        let _ = draw_line(&mut vram, 0x00ffff, cx, cy, i, 0);
        let _ = draw_line(&mut vram, 0xff00ff, cx, cy, rect_size, i);
        let _ = draw_line(&mut vram, 0xffffff, cx, cy, i, rect_size);
    }

    for (i, c) in "ABCDEF".chars().enumerate() {
        draw_font_fg(&mut vram, i as i64 * 16 + 256, i as i64 * 16, 0xffffff, c);
    }
    let mut w = VramTextWriter::new(&mut vram);
    for i in 0..4 {
        writeln!(w, "hello {}", i).unwrap();
    }

    let mut memory_map = MemoryMapHolder::new();

    let status = efi_system_table
        .boot_services
        .get_memory_map(&mut w, &mut memory_map); //memory_mapに値がセットされるはず
    writeln!(w, "{status:?}").unwrap();
    for e in memory_map.iter() {
        writeln!(w, "{e:?}").unwrap();
    }
    // println!("Hello, world!");
    loop {
        unsafe{
            asm!("hlt")
        }
    }
}


unsafe fn uncheck_draw_point<T: Bitmap>(
    buf: &mut T,
    color: u32,
    x:i64,
    y:i64,
){
    *buf.uncheck_pixel_at_mut(x, y) = color;
}

fn draw_point<T: Bitmap>(
    buf: &mut T,
    color:u32,
    x: i64,
    y: i64,
) -> Result<()>
{
    *(buf.pixel_at_mut(x, y).ok_or("Out of range")?) = color;
    Ok(())
}

fn fill_rect<T: Bitmap>(
    buf: &mut T,
    color:u32,
    px:i64,
    py:i64,
    w:i64,
    h:i64,
) -> Result<()>
{
    if !buf.is_in_x_range(px)
        || !buf.is_in_x_range(px + w - 1)
        || !buf.is_in_y_range(py)
        || !buf.is_in_y_range(py + h - 1) {
           return Err("Out of range");
    }
    for y in py..py + h {
        for x in px..px + w {
            unsafe {
                uncheck_draw_point(buf, color, x, y);
            }
        }
    }
    Ok(())
}

// https://ja.wikipedia.org/wiki/%E3%83%96%E3%83%AC%E3%82%BC%E3%83%B3%E3%83%8F%E3%83%A0%E3%81%AE%E3%82%A2%E3%83%AB%E3%82%B4%E3%83%AA%E3%82%BA%E3%83%A0
fn draw_line<T: Bitmap>(
    buf: &mut T,
    color:u32,
    mut x0: i64,
    mut y0: i64,
    x1:i64,
    y1:i64,
) -> Result<()>
{
    if !buf.is_in_x_range(x0)
        || !buf.is_in_x_range(x1)
        || !buf.is_in_y_range(y0)
        || !buf.is_in_y_range(y1) 
    {
           return Err("Out of range");
    }
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = 2 * (if x0 < x1 {1} else {0}) -1;
    let sy = 2 * (if y0 < y1 {1} else {0}) -1;
    let mut err = dx - dy;

    loop {
        unsafe {
            uncheck_draw_point(buf, color, x0, y0);
        }
        if x0 == x1 && y0 == y1
        {
            break;
        }
        let e2 =2 * err;
        if e2 > -dy
        {
            err -= dy;
            x0 += sx;
        }
        if e2 < dx {
            err += dx;
            y0 += sy;
        }
    }
    Ok(())
}


fn lookup_font(c: char) -> Option<[[char; 8]; 16]>
{
    const FONT_SOURCE: &str = include_str!("./font.txt");
    if let Ok(c) = u8::try_from(c) {
        let mut fi = FONT_SOURCE.split('\n');
        while let Some(line) = fi.next() {

            if let Some(line) = line.strip_prefix("0x") {
                if let Ok(idx) = u8::from_str_radix(line, 16)
                {
                    if idx != c
                    {
                        continue;
                    }
                    let mut font = [['*'; 8]; 16];
                    for (y, line) in fi.clone().take(16).enumerate() 
                    {
                        for (x, c) in line.chars().enumerate()
                        {
                            if let Some(e) = font[y].get_mut(x) {
                                *e = c;
                            }
                        }
                    }
                    return Some(font);

                }
            }
        }
    }
    None

}

fn draw_font_fg<T:Bitmap>(buf: &mut T, x:i64, y:i64, color:u32, c:char)
{
    if let Some(font) = lookup_font(c){
        for (dy, row) in font.iter().enumerate() {
            for (dx, pixel) in row.iter().enumerate() {
                let color = match pixel {
                    '*' => color,
                    _ => continue,
                };
                let _ = draw_point(buf, color, x + dx as i64, y + dy as i64);
            }
        }
    }
}


fn draw_str_fg<T: Bitmap>(buf : &mut T, x:i64, y:i64, color:u32, s: &str)
{
    for (i,c) in s.chars().enumerate() {
        draw_font_fg(buf, x * i as i64 * 8, y, color, c);
    }
}

struct VramTextWriter<'a> 
{
    vram: &'a mut VramBufferInfo,
    cursor_x:i64,
    cursor_y:i64
}

impl <'a> VramTextWriter<'a>
{
    fn new(vram: &'a mut VramBufferInfo) -> Self
    {
        Self {
            vram,
            cursor_x:0,
            cursor_y:0,
        }
    }
}

impl fmt::Write for VramTextWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars(){
            if c == '\n'{
                self.cursor_x = 0;
                self.cursor_y += 16;
            }else
            {
                draw_font_fg(self.vram, self.cursor_x, self.cursor_y, 0xffffff, c);
                self.cursor_x += 8;
            }
        }
        Ok(())
    }
}


#[panic_handler]
fn panic(_info: &PanicInfo) -> !{
    loop {
        
    }
}
