//  **************************************
//! **bino** - BBJ INterpreter for BBJOS *
//  **************************************

#![feature(untagged_unions, with_options, new_uninit)]

#[macro_use] extern crate static_assertions;

use minifb::{WindowOptions, Window, Key};
const_assert_eq!(mem::size_of::<Key>(), 1);

use std::mem;
use std::ptr;
use std::slice;
use std::fs;

pub const WIDTH:     usize = 640;
pub const HEIGHT:    usize = 360;
pub const VMEM_SIZE: usize = WIDTH * HEIGHT * 4;
pub const MEM_SIZE:  usize = 1024 * 1024 * 8; // 8MiB

pub const MAP_START: uxsize = 0xFFFFFFFF00000000;
pub const MAP_SIZE: usize = mem::size_of::<MapLayout>();
pub const KMAP_SIZE: usize = 16;
pub const SECTOR_SIZE: usize = 512;

pub union Memory {
	pub data: [u8; MEM_SIZE],
	pub code: [uxsize; MEM_SIZE / X_SIZE]
}

impl Memory {
	pub fn get_code(&self, addr: usize) -> uxsize {
		if addr + 8 > MEM_SIZE {
			panic!("index {}-{} out of bounds (len = `{}`)", addr, addr + 7, MEM_SIZE);
		}
		unsafe {
			((self.code.as_ptr() as *const u8).offset(addr as isize) as *const uxsize).read_unaligned()
		}
	}
}

#[repr(C)]
pub union Mapped {
	pub layout: MapLayout,
	pub data: [u8; MAP_SIZE],
	pub code: [uxsize; MAP_SIZE / X_SIZE]
}

impl Mapped {
	pub fn read(&mut self, memory: &mut Memory, addr: usize) -> u8 {
		unsafe {
			match addr {
				0 => {
					// r/w disk
					let disk = &self.layout.diskmap;
					if disk.write == 0 {
						match fs::read(format!("disk/{}", disk.sector)) {
							Ok(x) => {
								if disk.addr + x.len() as uxsize > memory.data.len() as uxsize {
									255
								} else {
									std::ptr::copy_nonoverlapping(x.as_ptr(), memory.data.as_mut_ptr().offset(disk.addr as isize), x.len());
									0
								}
							},
							Err(e) => {
								e.raw_os_error().unwrap_or(255) as u8
							}
						}
					} else {
						if disk.addr + SECTOR_SIZE as uxsize > memory.data.len() as uxsize {
							return 255;
						}
						match fs::write(format!("disk/{}", disk.sector), slice::from_raw_parts(memory.data.as_ptr().offset(disk.addr as isize), SECTOR_SIZE)) {
							Ok(_) => {
								0
							},
							Err(e) => {
								e.raw_os_error().unwrap_or(255) as u8
							}
						}
					}
				}
				_ => {
					self.data[addr]
				}
			}
		}
	}

	pub fn write(&mut self, _memory: &mut Memory, addr: usize, val: u8) {
		unsafe {
			match addr {
				_ => {
					self.data[addr] = val;
				}
			}
		}
	}
}

#[repr(C)]
pub struct MapLayout {
	pub diskmap: DiskMap,
	pub vmap: VideoMap,
	pub kmap: KeyMap
}

#[repr(C)]
pub struct DiskMap {
	pub sector: uxsize,
	pub addr: uxsize,
	pub write: u8
}

#[repr(C)]
pub union VideoMap {
	pub pixels: [u32; WIDTH * HEIGHT],
	pub bytes: [u8; VMEM_SIZE]
}

#[repr(transparent)]
pub struct KeyMap {
	pub keys: [u8; KMAP_SIZE]
}

impl KeyMap {
	pub fn update(&mut self, window: &mut Window) {
		if let Some(keys) = window.get_keys() {
			unsafe {
				ptr::copy_nonoverlapping(
					keys.as_ptr() as *const u8,
					self.keys.as_mut_ptr(),
					keys.len().min(KMAP_SIZE)
				);
			}
		}
	}
}

pub const X_SIZE: usize = mem::size_of::<uxsize>();

#[allow(non_camel_case_types)]
pub type uxsize = u64;

#[allow(non_camel_case_types)]
pub type ixsize = i64;

fn main() {
	let mut args = std::env::args();
	let file = args.nth(1).expect("expected binary file");

	let data = fs::read(file).unwrap();
	let mut memory = unsafe { Box::<Memory>::new_zeroed().assume_init() };
	unsafe {
		assert!(data.len() <= memory.data.len());
		ptr::copy_nonoverlapping(
			data.as_ptr(),
			memory.data.as_mut_ptr(),
			data.len()
		);
	}

	let mut map = unsafe { Box::<Mapped>::new_zeroed().assume_init() };

	let mut window = Window::new(
		"bino",
		WIDTH,
		HEIGHT,
		WindowOptions::default()
	).unwrap();

	let mut ip = unsafe {memory.code[0]};
	while window.is_open() {
		unsafe {
			map.layout.kmap.update(&mut window);
		}
		let a = memory.get_code(ip as usize);
		let b = memory.get_code((ip + 8) as usize);
		let c = memory.get_code((ip + 16) as usize);
		eprintln!("DEBUG: ip = {:#X}", ip);
		eprintln!("DEBUG: a = {:#X}", a);
		eprintln!("DEBUG: b = {:#X}", b);
		eprintln!("DEBUG: c = {:#X}", c);
		if a == 0xFFFFFFFFFFFFFFFF {
			let bytes = unsafe {
				&memory.data[b as usize..][..(c as usize)]
			};
			eprintln!("DEBUG BYTES: ({}) {:?}", c, bytes);
			match std::str::from_utf8(bytes) {
				Ok(x) => eprintln!("DEBUG STR: ({}) {:?}", c, x),
				Err(e) => eprintln!("DEBUG UTF8 ERROR: ({}) {:?}", c, e)
			}
			panic!("debug abort");
		}
		let src = if a >= MAP_START {
			map.read(&mut memory, (a - MAP_START) as usize)
		} else {
			unsafe {
				memory.data[a as usize]
			}
		};
		if b >= MAP_START {
			map.write(&mut memory, (b - MAP_START) as usize, src)
		} else {
			unsafe {
				memory.data[b as usize] = src;
			}
		};
		ip = c;
		window.update_with_buffer(unsafe {
			&map.layout.vmap.pixels[..]
		}, WIDTH, HEIGHT).unwrap();
	}
}
