// Author: Nicholas Renner
//
// Misc functions for interface
// Random, locks, etc.
#![allow(dead_code)]

use std::fs::File;
use std::io::Read;
pub use std::collections::{HashMap as RustHashMap, HashSet as RustHashSet};
pub use std::cmp::{max as rust_max, min as rust_min};
use std::str::Utf8Error;

pub use std::sync::{RwLock as RustLock, Arc as RustRfc};
use std::sync::{Mutex, Condvar};

use libc::mmap;
use std::ffi::c_void;

pub use serde::{Serialize as SerdeSerialize, Deserialize as SerdeDeserialize};

pub use serde_json::{to_string as serde_serialize_to_string, from_str as serde_deserialize_from_string};

pub fn log_from_ptr(buf: *const u8) {
    if let Ok(s) = unsafe{std::ffi::CStr::from_ptr(buf as *const i8).to_str()} {
      log_to_stdout(s);
    }
}
// Print text to stdout
pub fn log_to_stdout(s: &str) {
    print!("{}", s);
}

// Print text to stderr
pub fn log_to_stderr(s: &str) {
    eprintln!("{}", s);
}

pub fn fillrandom(bufptr: *mut u8, count: usize) -> i32 {
    let slice = unsafe{std::slice::from_raw_parts_mut(bufptr, count)};
    let mut f = std::fs::OpenOptions::new().read(true).write(false).open("/dev/urandom").unwrap();
    f.read(slice).unwrap() as i32
}
pub fn fillzero(bufptr: *mut u8, count: usize) -> i32 {
    let slice = unsafe{std::slice::from_raw_parts_mut(bufptr, count)};
    for i in 0..count {slice[i] = 0u8;}
    count as i32
}

pub fn copy_fromvec_sized(bufptr: *mut u8, count: usize, vec: &Vec<u8>) {
    unsafe {std::ptr::copy(vec.as_ptr(), bufptr, count);}
}
pub fn extend_fromptr_sized(bufptr: *const u8, count: usize, vec: &mut Vec<u8>) {
    let byteslice = unsafe {std::slice::from_raw_parts(bufptr, count)};
    vec.extend_from_slice(byteslice);
}

// Wrapper to return a dictionary (hashmap)
pub fn new_hashmap<K, V>() -> RustHashMap<K, V> {
    RustHashMap::new()
}

pub unsafe fn charstar_to_ruststr<'a>(cstr: *const i8) -> Result<&'a str, Utf8Error> {
    return std::ffi::CStr::from_ptr(cstr).to_str();         //returns a result to be unwrapped later
}

pub fn libc_mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fildes: i32, off: i64) -> i32 {
    return ((unsafe{mmap(addr as *mut c_void, len, prot, flags, fildes, off)} as i64) & 0xffffffff) as i32;
}

#[derive(Debug)]
pub struct AdvisoryLock {
    //0 signifies unlocked, -1 signifies locked exclusively, positive number signifies that many shared lock holders
    advisory_lock: RustRfc<Mutex<i32>>,
    advisory_condvar: Condvar
}
impl AdvisoryLock {
    pub fn new() -> Self {
        Self {advisory_lock: RustRfc::new(Mutex::new(0)), advisory_condvar: Condvar::new()}
    }

    pub fn lock_ex(&self) {
        let mut waitedguard = self.advisory_condvar.wait_while(self.advisory_lock.lock().unwrap(), 
                                                               |guard| {*guard != 0}).unwrap();
        *waitedguard = -1;
    }

    pub fn lock_sh(&self) {
        let mut waitedguard = self.advisory_condvar.wait_while(self.advisory_lock.lock().unwrap(), 
                                                               |guard| {*guard < 0}).unwrap();
        *waitedguard += 1;
    }
    pub fn try_lock_ex(&self) -> bool {
        if let Ok(mut guard) = self.advisory_lock.try_lock() {
            if *guard == 0 {
              *guard = -1;
              return true
            }
        }
        false
    }
    pub fn try_lock_sh(&self) -> bool {
        if let Ok(mut guard) = self.advisory_lock.try_lock() {
            if *guard >= 0 {
              *guard += 1;
              return true
            }
        }
        false
    }

    pub fn unlock(&self) -> bool {
        let mut guard = self.advisory_lock.lock().unwrap();

        if *guard < 0 {
            *guard -= 1;
  
            //only a writer could be waiting at this point
            if *guard == 0 {self.advisory_condvar.notify_one();}
            true
        } else if *guard == -1 {
            if *guard != -1 {return false;}
            *guard = 0;
  
            self.advisory_condvar.notify_all(); //in case readers are waiting
            true
        } else {false}
    }
}
