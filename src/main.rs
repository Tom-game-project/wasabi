#![no_std]
#![no_main]
#![feature(offset_of)]

use core::mem::offset_of;
use core::arch::asm;
use core::cmp::min;
use core::mem::size_of;
use core::ptr::null_mut;
use core::panic::PanicInfo;
use core::slice;

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

#[repr(C)]
struct EfiBootServicesTable
{
    _reserved0: [u64; 40],
    locate_protocol: extern "win64" fn(
        protocol: *const EfiGuid,
        registration: *const EfiVoid,
        interface: *mut *mut EfiVoid
    ) -> EfiStatus,
}


#[derive(Debug, PartialEq, Eq, Copy, Clone)]
#[must_use]
#[repr(u64)]
enum EfiStatus {
    Success = 0,
}

const _: () = assert!(offset_of!(EfiBootServicesTable, locate_protocol) == 320);

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

    fill_rect(& mut vram, 0x000000, 0, 0, vw, vh);
    fill_rect(& mut vram, 0xff0000, 0, 0, 32, 32);
    fill_rect(& mut vram, 0x00ff00, 32, 32, 32, 32);
    fill_rect(& mut vram, 0x0000ff, 64, 64, 32, 32);

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


#[panic_handler]
fn panic(_info: &PanicInfo) -> !{
    loop {
        
    }
}
