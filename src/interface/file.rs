// Author: Nicholas Renner
//
// File related interface
#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::env;
use std::slice;
use std::path::{PathBuf, Path};
use std::io::{SeekFrom, Seek, Read, Write};
use std::lazy::SyncLazy;

static OPEN_FILES: SyncLazy<Arc<Mutex<HashSet<String>>>> = SyncLazy::new(|| Arc::new(Mutex::new(HashSet::new())));

pub fn listfiles() -> Vec<String> {
    let paths = fs::read_dir(&Path::new(
        &env::current_dir().unwrap())).unwrap();
      
    let names =
    paths.filter_map(|entry| {
      entry.ok().and_then(|e|
        e.path().file_name()
        .and_then(|n| n.to_str().map(|s| String::from(s)))
      )
    }).collect::<Vec<String>>();

    return names;
}

pub fn removefile(filename: String) -> std::io::Result<()> {
    let openfiles = OPEN_FILES.lock().unwrap();

    if openfiles.contains(&filename) {
        panic!("FileInUse");
    }

    let path: PathBuf = [".".to_string(), filename].iter().collect();

    let absolute_filename = fs::canonicalize(&path).unwrap();

    if !absolute_filename.exists() {
        panic!("FileNotFoundError");
    }

    fs::remove_file(absolute_filename)?;

    Ok(())
}

// Checker for illegal filenames
fn assert_is_allowed_filename(filename: &String) {

    const MAX_FILENAME_LENGTH: usize = 120;

    if filename.len() > MAX_FILENAME_LENGTH {
        panic!("ArgumentError: Filename exceeds maximum length.")
    }

    if !filename.chars().all(char::is_alphanumeric) {
        panic!("ArgumentError: Filename has disallowed charachters.")
    }

    match filename.as_str() {
        "" | "." | ".." => panic!("ArgumentError: Illegal filename."),
        _ => {}
    }

    if filename.starts_with(".") {
        panic!("ArgumentError: Filename cannot start with a period.")
    }
}

pub fn emulated_open(filename: String, create: bool) -> std::io::Result<EmulatedFile> {
    EmulatedFile::new(filename, create)
}

pub struct EmulatedFile {
    filename: String,
    abs_filename: PathBuf,
    fobj: Option<Arc<Mutex<File>>>,
    filesize: usize,
}

impl EmulatedFile {

    fn new(filename: String, create: bool) -> std::io::Result<EmulatedFile> {
        assert_is_allowed_filename(&filename);

        let mut openfiles = OPEN_FILES.lock().unwrap();

        if openfiles.contains(&filename) {
            panic!("FileInUse");
        }

        let path: PathBuf = [".".to_string(), filename.clone()].iter().collect();

        let f = if !path.exists() {
            if !create {
              panic!("Cannot open non-existent file {}", filename);
            }

            OpenOptions::new().read(true).write(true).create(true).open(filename.clone())
        } else {
            OpenOptions::new().read(true).write(true).open(filename.clone())
        }?;

        let absolute_filename = fs::canonicalize(&path)?;

        openfiles.insert(filename.clone());
        f.sync_all()?;
        let filesize = f.metadata()?.len();

        Ok(EmulatedFile {filename: filename, abs_filename: absolute_filename, fobj: Some(Arc::new(Mutex::new(f))), filesize: filesize as usize})

    }

    pub fn close(&self) -> std::io::Result<()> {
        let mut openfiles = OPEN_FILES.lock().unwrap();

        openfiles.remove(&self.filename);
        Ok(())
    }

    // Read from file into provided C-buffer
    pub unsafe fn readat(&self, ptr: *mut u8, length: usize, offset: usize) -> std::io::Result<usize> {

        let mut buf = unsafe {
            assert!(!ptr.is_null());
            slice::from_raw_parts_mut(ptr, length)
        };

        match &self.fobj {
            None => panic!("{} is already closed.", self.filename),
            Some(f) => { 
                let mut fobj = f.lock().unwrap();
                if offset > self.filesize {
                  panic!("Seek offset extends past the EOF!");
                }
                fobj.seek(SeekFrom::Start(offset as u64))?;
                let bytes_read = fobj.read(buf)?;
                fobj.sync_data()?;
                Ok(bytes_read)
            }
        }
    }

    // Write to file from provided C-buffer
    pub unsafe fn writeat(&mut self, ptr: *const u8, length: usize, offset: usize) -> std::io::Result<usize> {

        let mut bytes_written;

        let buf = unsafe {
            assert!(!ptr.is_null());
            slice::from_raw_parts(ptr, length)
        };

        match &self.fobj {
            None => panic!("{} is already closed.", self.filename),
            Some(f) => { 
                let mut fobj = f.lock().unwrap();
                if offset > self.filesize {
                    panic!("Seek offset extends past the EOF!");
                }
                fobj.seek(SeekFrom::Start(offset as u64))?;
                bytes_written = fobj.write(buf)?;
                fobj.sync_data()?;
            }
        }

        if offset + length > self.filesize {
            self.filesize = offset + length;
        }

        Ok(bytes_written)
    }
}

#[cfg(test)]
mod tests {
    extern crate libc;
    use std::mem;
    use super::*;
    #[test]
    pub fn filewritetest() {
      println!("{:?}", listfiles());
      let mut f = emulated_open("foobar".to_string(), true).expect("?!");
      println!("{:?}", listfiles());
      let mut q = unsafe{libc::malloc(mem::size_of::<u8>() * 9) as *mut u8};
      unsafe{std::ptr::copy_nonoverlapping("fizzbuzz!".as_bytes().as_ptr() , q as *mut u8, 9)};
      println!("{:?}", unsafe{f.writeat(q, 9, 0)});
      let mut b = unsafe{libc::malloc(mem::size_of::<u8>() * 9)} as *mut u8;
      println!("{:?}", String::from_utf8(unsafe{std::slice::from_raw_parts(b, 9)}.to_vec()));
      println!("{:?}", unsafe{f.readat(b, 9, 0)});
      println!("{:?}", String::from_utf8(unsafe{std::slice::from_raw_parts(b, 9)}.to_vec()));
      println!("{:?}", f.close());
      unsafe {
        libc::free(q as *mut libc::c_void);
        libc::free(b as *mut libc::c_void);
      }
      println!("{:?}", removefile("foobar".to_string()));
    }
}
