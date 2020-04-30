//  **************************************
//! **bino** - BBJ INterpreter for BBJOS *
//  **************************************

#![feature(untagged_unions, with_options, new_uninit)]

#[macro_use] extern crate static_assertions;

use minifb::{WindowOptions, Window, Key};
const_assert_eq!(mem::size_of::<Key>(), 1);

use std::path::Path;
use std::mem;

use std::ptr;

use std::fs;
use std::fs::File;

use std::io::Read;

pub const WIDTH:  usize = 640;
pub const HEIGHT: usize = 360;
pub const VIDBUF_SIZE: usize = mem::size_of::<VideoBuffer>();
const_assert_eq!(VIDBUF_SIZE, WIDTH * HEIGHT * 4);

pub const PAGE_SIZE: usize = 1024 * 1024 * 128; // 128MiB
const_assert_eq!(PAGE_SIZE, mem::size_of::<Page>());

pub const SECT_SIZE: usize = 512;
const_assert_eq!(SECT_SIZE, mem::size_of::<Section>());

pub const X_SIZE: usize = mem::size_of::<uxsize>();

#[allow(non_camel_case_types)]
pub type uxsize = u64;

#[allow(non_camel_case_types)]
pub type ixsize = i64;

pub union Section {
	pub data: [u8; SECT_SIZE],
	pub code: [uxsize; SECT_SIZE / X_SIZE]
}

#[repr(C)]
pub struct MapPage0 {
	pub disk_sect: uxsize,
	pub disk_place: uxsize,
	pub disk_dir: u8,
	pub vbuf: VideoBuffer,
	pub keys: [u8; 16]
}
const_assert!(mem::size_of::<MapPage0>() <= PAGE_SIZE);

pub union VideoBuffer {
	pub rows: [[u32; WIDTH]; HEIGHT],
	pub flat: [u32; WIDTH * HEIGHT],
	pub u8:   [u8; WIDTH * HEIGHT * 4]
}

pub union Page {
	pub map0: MapPage0,
	pub sects: [Section; PAGE_SIZE / SECT_SIZE],
	pub data: [u8; PAGE_SIZE],
	pub code: [uxsize; PAGE_SIZE / X_SIZE]
}
fn main() {
	let mut args = std::env::args();
	let disk = args.nth(1).expect("Expected disk directory.");
	if disk.starts_with("INTODISK=") {
		let file = args.next().expect("If given `INTODISK=`, a raw file is expected.");
		let folder: &Path = disk[9..].as_ref();
		let data = fs::read(&file).unwrap();
		println!("Read {} bytes of data from input file `{}`", data.len(), file);
		println!("Writing to disk folder `{}`", folder.display());
		for (i, chunk) in data.chunks(PAGE_SIZE).enumerate() {
			println!("PAGE {} (0x{:X} - 0x{:X})", i, i * PAGE_SIZE, i * PAGE_SIZE + chunk.len());
			fs::write(
				folder.join(i.to_string()),
				chunk
			).unwrap();
		}
		return;
	}
	let disk: &Path = disk.as_ref();

	let mut memory = unsafe {Box::<Page>::new_zeroed().assume_init()};
	read_page(disk, 0, memory.as_mut());

	let mut mapped = unsafe {Box::<Page>::new_zeroed().assume_init()};

	let mut window = Window::new(
		"ESOS",
		WIDTH,
		HEIGHT,
		WindowOptions::default()
	).unwrap();

	let mut ip = unsafe {memory.code[0]};
	while window.is_open() {
		unsafe {
			ptr::write_bytes(mapped.map0.keys.as_mut_ptr(), 0xFFu8, 16);
			if let Some(keys) = window.get_keys() {
				ptr::copy_nonoverlapping(
					keys.as_ptr() as *const u8,
					mapped.map0.keys.as_mut_ptr(),
					keys.len()
				);
			}
			let a = memory.code[ip as usize];
			let b = memory.code[(ip + 1) as usize];
			let c = memory.code[(ip + 2) as usize];
			if a != b {
				let src = if a >= 0xFFFFFFFF00000000 {
					mapped.data[a as usize]
				} else {
					memory.data[a as usize]
				};
				if b >= 0xFFFFFFFF00000000 {
					mapped.data[b as usize] = src;
				} else {
					memory.data[b as usize] = src;
				};
			}
			ip = c;
			window.update_with_buffer(&mapped.map0.vbuf.flat[..], WIDTH, HEIGHT).unwrap();
		}
	}
}

pub fn read_page(disk: &Path, num: u64, out: &mut Page) {
	let mut file = File::with_options()
		.create(true)
		.write(true)
		.read(true)
		.open(disk.join(num.to_string()))
		.unwrap();
	file.set_len(PAGE_SIZE as u64).unwrap();
	unsafe {
		file.read_exact(&mut out.data).unwrap();
	}
}
