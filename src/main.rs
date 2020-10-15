//  **************************************
//! **bino** - BBJ INterpreter for BBJOS *
//  **************************************

#![feature(with_options, alloc_layout_extra)]

#[macro_use] extern crate static_assertions;

use minifb::{WindowOptions, Window, Key};
const_assert_eq!(mem::size_of::<Key>(), 1);

use std::mem;
use std::ptr;
use std::slice;
use std::fs;
use std::alloc;
use std::alloc::Layout;

pub const MAP_START: uxsize = 0xFFFFFFFF00000000;
pub const KMAP_SIZE: usize = 16;
pub const SECTOR_SIZE: usize = 512;

#[repr(transparent)]
pub struct Memory {
	pub code: [uxsize],
}

impl Memory {
	pub fn get_code(&self, addr: usize) -> uxsize {
		if addr + 8 > self.data().len() {
			panic!("index {}-{} out of bounds (len = `{}`)", addr, addr + 7, self.data().len());
		}
		unsafe {
			(self.data().as_ptr().offset(addr as isize) as *const uxsize).read_unaligned()
		}
	}

	pub fn data(&self) -> &[u8] {
		unsafe {
			slice::from_raw_parts(self.code.as_ptr() as *const u8, self.code.len() * 8)
		}
	}

	pub fn data_mut(&mut self) -> &mut [u8] {
		unsafe {
			slice::from_raw_parts_mut(self.code.as_mut_ptr() as *mut u8, self.code.len() * 8)
		}
	}
}

#[repr(transparent)]
pub struct Mapped {
	pub code: [uxsize]
}

impl Mapped {
	pub fn read(&mut self, memory: &mut Memory, addr: usize) -> u8 {
		unsafe {
			match addr {
				0 => {
					// r/w disk
					let disk = self.disk_map();
					if disk.write == 0 {
						match fs::read(format!("disk/{}", disk.sector)) {
							Ok(x) => {
								if disk.addr + x.len() as uxsize > memory.data().len() as uxsize {
									255
								} else {
									std::ptr::copy_nonoverlapping(x.as_ptr(), memory.data_mut().as_mut_ptr().offset(disk.addr as isize), x.len());
									0
								}
							},
							Err(e) => {
								e.raw_os_error().unwrap_or(255) as u8
							}
						}
					} else {
						if disk.addr + SECTOR_SIZE as uxsize > memory.data().len() as uxsize {
							return 255;
						}
						match fs::write(format!("disk/{}", disk.sector), slice::from_raw_parts(memory.data().as_ptr().offset(disk.addr as isize), SECTOR_SIZE)) {
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
					self.data()[addr]
				}
			}
		}
	}

	pub fn write(&mut self, _memory: &mut Memory, addr: usize, val: u8) {
		match addr {
			_ => {
				self.data_mut()[addr] = val;
			}
		}
	}

	pub fn data(&self) -> &[u8] {
		unsafe {
			slice::from_raw_parts(self.code.as_ptr() as *const u8, self.code.len() * 8)
		}
	}

	pub fn data_mut(&mut self) -> &mut [u8] {
		unsafe {
			slice::from_raw_parts_mut(self.code.as_mut_ptr() as *mut u8, self.code.len() * 8)
		}
	}

	pub fn disk_map(&self) -> &DiskMap {
		unsafe {
			&*(self as *const Mapped as *const DiskMap)
		}
	}

	pub fn disk_map_mut(&mut self) -> &mut DiskMap {
		unsafe {
			&mut *(self as *mut Mapped as *mut DiskMap)
		}
	}

	pub fn video_map(&self, vpixels: usize) -> &VideoMap {
		let d = self as *const Mapped as *const DiskMap;
		unsafe {
			&*(slice::from_raw_parts(d.offset(1) as *const u32, vpixels) as *const [u32] as *const VideoMap)
		}
	}

	pub fn video_map_mut(&mut self, vpixels: usize) -> &mut VideoMap {
		let d = self as *mut Mapped as *mut DiskMap;
		unsafe {
			&mut *(slice::from_raw_parts_mut(d.offset(1) as *mut u32, vpixels) as *mut [u32] as *mut VideoMap)
		}
	}

	pub fn key_map(&self, vpixels: usize) -> &KeyMap {
		let d = self as *const Mapped as *const DiskMap;
		unsafe {
			let v = d.offset(1) as *const u32;
			let k = v.offset(vpixels as isize) as *const KeyMap;
			&*k
		}
	}

	pub fn key_map_mut(&mut self, vpixels: usize) -> &mut KeyMap {
		let d = self as *mut Mapped as *mut DiskMap;
		unsafe {
			let v = d.offset(1) as *mut u32;
			let k = v.offset(vpixels as isize) as *mut KeyMap;
			&mut *k
		}
	}
}

#[repr(C)]
pub struct DiskMap {
	pub sector: uxsize,
	pub addr: uxsize,
	pub write: u8
}

#[repr(transparent)]
pub struct VideoMap {
	pub pixels: [u32]
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

	let file = args.nth(1).expect("Usage: bino <file> <memwords> [w=600] [h=400]");
	let memory_words = args.next().expect("Usage: bino <file> <memwords> [w=600] [h=400]").parse::<usize>().unwrap();
	let width = args.next().map(|x| x.parse::<usize>().unwrap()).unwrap_or(600);
	let height = args.next().map(|x| x.parse::<usize>().unwrap()).unwrap_or(400);

	let data = fs::read(file).unwrap();
	let mut memory = unsafe {
		let ptr = alloc::alloc(Layout::array::<uxsize>(memory_words).unwrap());
		let mem = slice::from_raw_parts_mut(ptr as *mut uxsize, memory_words);
		Box::from_raw(mem as *mut [uxsize] as *mut Memory)
	};
	unsafe {
		assert!(data.len() <= memory.data().len());
		ptr::copy_nonoverlapping(
			data.as_ptr(),
			memory.data_mut().as_mut_ptr(),
			data.len()
		);
	}

	let mut map = unsafe {
		let map_bytes = mem::size_of::<DiskMap>() + width * height * 4 + KMAP_SIZE;
		let map_words = map_bytes / mem::size_of::<uxsize>() + 1;
		let ptr = alloc::alloc(Layout::array::<uxsize>(map_words).unwrap());
		let mem = slice::from_raw_parts_mut(ptr as *mut uxsize, map_words);
		Box::from_raw(mem as *mut [uxsize] as *mut Mapped)
	};

	let mut window = Window::new(
		"bino",
		width,
		height,
		WindowOptions::default()
	).unwrap();

	let vpixels = width * height;

	let mut ip = memory.code[0];
	while window.is_open() {
		map.key_map_mut(vpixels).update(&mut window);
		let a = memory.get_code(ip as usize);
		let b = memory.get_code((ip + 8) as usize);
		let c = memory.get_code((ip + 16) as usize);
		//eprintln!("DEBUG: ip = {:#X}", ip);
		//eprintln!("DEBUG: a = {:#X}", a);
		//eprintln!("DEBUG: b = {:#X}", b);
		//eprintln!("DEBUG: c = {:#X}", c);
		if a == 0xFFFFFFFFFFFFFFFF {
			let bytes = &memory.data()[b as usize..][..(c as usize)];
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
			memory.data()[a as usize]
		};
		if b >= MAP_START {
			map.write(&mut memory, (b - MAP_START) as usize, src)
		} else {
			memory.data_mut()[b as usize] = src;
		};
		ip = c;
		window.update_with_buffer(&map.video_map(vpixels).pixels, width, height).unwrap();
	}
}
