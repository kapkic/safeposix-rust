// // Authors: Nicholas Renner and Jonathan Singer
// //
// //

use std::mem::size_of;
extern crate libc;

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum GenSockaddr {
    V4(SockaddrV4),
    V6(SockaddrV6)
}
impl GenSockaddr {
    pub fn port(&self) -> u16 {
        match self {
            GenSockaddr::V4(v4addr) => v4addr.sin_port,
            GenSockaddr::V6(v6addr) => v6addr.sin6_port
        }
    }
    pub fn set_port(&mut self, port: u16) {
        match self {
            GenSockaddr::V4(v4addr) => v4addr.sin_port = port,
            GenSockaddr::V6(v6addr) => v6addr.sin6_port = port
        };
    }

    pub fn addr(&self) -> GenIpaddr {
        match self {
            GenSockaddr::V4(v4addr) => GenIpaddr::V4(v4addr.sin_addr),
            GenSockaddr::V6(v6addr) => GenIpaddr::V6(v6addr.sin6_addr),
        }
    }
    pub fn set_addr(&mut self, ip: GenIpaddr){
        match self {
            GenSockaddr::V4(v4addr) => v4addr.sin_addr = if let GenIpaddr::V4(v4ip) = ip {v4ip} else {unreachable!()},
            GenSockaddr::V6(v6addr) => v6addr.sin6_addr = if let GenIpaddr::V6(v6ip) = ip {v6ip} else {unreachable!()},
        };
    }
}

#[derive(Debug)]
pub enum GenIpaddr {
    V4(V4Addr),
    V6(V6Addr)
}

impl GenIpaddr {
    pub fn is_unspecified(&self) -> bool {
        match self {
            GenIpaddr::V4(v4ip) => v4ip.s_addr == 0,
            GenIpaddr::V6(v6ip) => v6ip.s6_addr == [0; 16],
        }
    }
}

#[repr(C)]
pub union SockaddrAll {
    pub sockaddr_in: *mut SockaddrV4,
    pub sockaddr_in6: *mut SockaddrV6
}

#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct V4Addr {
    pub s_addr: u32
}
#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct SockaddrV4 {
    sin_family: u16,
    sin_port: u16,
    sin_addr: V4Addr
}

#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct V6Addr {
    pub s6_addr: [u8; 16]
}
#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct SockaddrV6 {
    sin6_family: u16,
    sin6_port: u16,
    sin6_flowinfo: u32,
    sin6_addr: V6Addr,
    sin6_scope_id: u32
}

pub struct Socket {
    raw_sys_fd: i32//make private right after
}

impl Socket {
    pub fn new(domain: i32, socktype: i32, protocol: i32) -> Socket {
        let fd = unsafe {libc::socket(domain, socktype, protocol)};
        if fd < 0 {panic!("Socket creation failed when it should never fail");}
        Socket {raw_sys_fd: fd}
    }
    pub fn bind(&self, addr: &GenSockaddr) -> i32 {
        let (finalsockaddr, addrlen) = match addr {
            GenSockaddr::V6(addrref6) => {((addrref6 as *const SockaddrV6).cast::<libc::sockaddr>(), size_of::<SockaddrV6>())}
            GenSockaddr::V4(addrref) => {((addrref as *const SockaddrV4).cast::<libc::sockaddr>(), size_of::<SockaddrV4>())}
        };
        unsafe {libc::bind(self.raw_sys_fd, finalsockaddr, addrlen as u32)}
    }
    pub fn connect(&self, addr: &GenSockaddr) -> i32 {
        let (finalsockaddr, addrlen) = match addr {
            GenSockaddr::V6(addrref6) => {((addrref6 as *const SockaddrV6).cast::<libc::sockaddr>(), size_of::<SockaddrV6>())}
            GenSockaddr::V4(addrref) => {((addrref as *const SockaddrV4).cast::<libc::sockaddr>(), size_of::<SockaddrV4>())}
        };
        unsafe {libc::connect(self.raw_sys_fd, finalsockaddr, addrlen as u32)}
    }
    pub fn sendto(&self, buf: *mut u8, len: usize, flags: i32, addr: &GenSockaddr) -> i32 {
        let (finalsockaddr, addrlen) = match addr {
            GenSockaddr::V6(addrref6) => {((addrref6 as *const SockaddrV6).cast::<libc::sockaddr>(), size_of::<SockaddrV6>())}
            GenSockaddr::V4(addrref) => {((addrref as *const SockaddrV4).cast::<libc::sockaddr>(), size_of::<SockaddrV4>())}
        };
        unsafe {libc::sendto(self.raw_sys_fd, buf as *const libc::c_void, len, flags, finalsockaddr, addrlen as u32) as i32}
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        unsafe { libc::close(self.raw_sys_fd); }
    }
}
