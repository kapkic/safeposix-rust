#[cfg(test)]
mod fs_tests {
    use crate::interface;
    use crate::safeposix::{cage::*, dispatcher::*, filesystem};
    use super::super::*;
    // use std::os::unix::fs::PermissionsExt;
    // use std::fs::OpenOptions;

    #[test]
    pub fn net_tests() {
        ut_lind_net_bind();
        ut_lind_net_bind_multiple();
        // ut_lind_net_bind_on_zero();
        ut_lind_net_connect_basic_udp();
        ut_lind_net_getpeername();
        ut_lind_net_getsockname();
    }



    //not finished
    pub fn ut_lind_net_bind() {
        lindrustinit();
        let cage = {CAGE_TABLE.read().unwrap().get(&1).unwrap().clone()};
        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        //should work...
        let socket = interface::GenSockaddr::V4(interface::SockaddrV4{ sin_family: 0, sin_port: 50102, sin_addr: interface::V4Addr{ s_addr: u32::from_be_bytes([127, 0, 0, 1]) }}); //127.0.0.1

        assert_eq!(cage.bind_syscall(sockfd, &socket, 4096), 0);
        assert_eq!(cage.bind_syscall(sockfd, &socket, 4096), -(Errno::EINVAL as i32)); //already bound so should fail

        //trying to bind another to the same IP/PORT
        let sockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        assert_eq!(cage.bind_syscall(sockfd2, &socket, 4096), -(Errno::EADDRINUSE as i32)); //already bound so should fail

        //UDP should still work...
        let sockfd3 = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        assert_eq!(cage.bind_syscall(sockfd3, &socket, 4096), 0);

        assert_eq!(cage.exit_syscall(), 0);
        lindrustfinalize();
    }

    

    pub fn ut_lind_net_bind_on_zero() {
        lindrustinit();
        let cage = {CAGE_TABLE.read().unwrap().get(&1).unwrap().clone()};

        //both the server and the socket are run from this file
        let serversockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        let clientsockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let clientsockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        let socket = interface::GenSockaddr::V4(interface::SockaddrV4{ sin_family: 0, sin_port: 50103, sin_addr: interface::V4Addr{ s_addr: 0 }}); //127.0.0.1
        assert_eq!(cage.bind_syscall(serversockfd, &socket, 4096), 0);
        assert_eq!(cage.listen_syscall(serversockfd, 1), 0);

        assert_eq!(cage.exit_syscall(), 0);
        lindrustfinalize();
    }



    pub fn ut_lind_net_bind_multiple() {
        lindrustinit();
        let cage = {CAGE_TABLE.read().unwrap().get(&1).unwrap().clone()};

        let mut sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let socket = interface::GenSockaddr::V4(interface::SockaddrV4{ sin_family: 0, sin_port: 50104, sin_addr: interface::V4Addr{ s_addr: u32::from_be_bytes([127, 0, 0, 1]) }}); //127.0.0.1
        assert_eq!(cage.bind_syscall(sockfd, &socket, 4096), 0);

        let sockfd2 = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);

        //allowing port reuse
        assert_eq!(cage.setsockopt_syscall(sockfd, SOL_SOCKET, SO_REUSEPORT, 1), 0);
        assert_eq!(cage.setsockopt_syscall(sockfd2, SOL_SOCKET, SO_REUSEPORT, 1), 0);

        assert_eq!(cage.bind_syscall(sockfd2, &socket, 4096), 0);

        //double listen should be allowed
        assert_eq!(cage.listen_syscall(sockfd, 1), 0);
        assert_eq!(cage.listen_syscall(sockfd2, 1), 0);

        //UDP bind should be allowed
        sockfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        assert_eq!(cage.bind_syscall(sockfd, &socket, 4096), 0);

        assert_eq!(cage.exit_syscall(), 0);
        lindrustfinalize();
    }



    pub fn ut_lind_net_connect_basic_udp() {
        lindrustinit();
        let cage = {CAGE_TABLE.read().unwrap().get(&1).unwrap().clone()};

        //should be okay...
        let sockfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        let mut socket = interface::GenSockaddr::V4(interface::SockaddrV4{ sin_family: 0, sin_port: 50105, sin_addr: interface::V4Addr{ s_addr: u32::from_be_bytes([127, 0, 0, 1]) }}); //127.0.0.1
        assert_eq!(cage.connect_syscall(sockfd, &socket), 0);

        //should be able to retarget the socket
        socket = interface::GenSockaddr::V4(interface::SockaddrV4{ sin_family: 0, sin_port: 50106, sin_addr: interface::V4Addr{ s_addr: u32::from_be_bytes([127, 0, 0, 1]) }}); //127.0.0.1
        assert_eq!(cage.connect_syscall(sockfd, &socket), 0);

        assert_eq!(cage.exit_syscall(), 0);
        lindrustfinalize();
    }



    pub fn ut_lind_net_getpeername() {
        lindrustinit();
        let cage = {CAGE_TABLE.read().unwrap().get(&1).unwrap().clone()};

        //doing a few things with connect -- only UDP right now
        let sockfd = cage.socket_syscall(AF_INET, SOCK_DGRAM, 0);
        let mut socket = interface::GenSockaddr::V4(interface::SockaddrV4{ sin_family: 0, sin_port: 50107, sin_addr: interface::V4Addr{ s_addr: u32::from_be_bytes([127, 0, 0, 1]) }}); //127.0.0.1
        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default()); //127.0.0.1
        
        assert_eq!(cage.connect_syscall(sockfd, &socket), 0);
        assert_eq!(cage.getpeername_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket, socket);

        //should be able to retarget
        socket = interface::GenSockaddr::V4(interface::SockaddrV4{ sin_family: 0, sin_port: 50108, sin_addr: interface::V4Addr{ s_addr: u32::from_be_bytes([127, 0, 0, 1]) }}); //127.0.0.1
        assert_eq!(cage.connect_syscall(sockfd, &socket), 0);
        assert_eq!(cage.getpeername_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket, socket);

        assert_eq!(cage.exit_syscall(), 0);
        lindrustfinalize();
    }



    pub fn ut_lind_net_getsockname() {
        lindrustinit();
        let cage = {CAGE_TABLE.read().unwrap().get(&1).unwrap().clone()};
        
        let sockfd = cage.socket_syscall(AF_INET, SOCK_STREAM, 0);
        let mut retsocket = interface::GenSockaddr::V4(interface::SockaddrV4::default()); 

        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket.port(), 0);
        assert_eq!(retsocket.addr(), interface::GenIpaddr::V4(interface::V4Addr::default()));

        let mut socket = interface::GenSockaddr::V4(interface::SockaddrV4{ sin_family: 0, sin_port: 50109, sin_addr: interface::V4Addr{ s_addr: u32::from_be_bytes([127, 0, 0, 1]) }}); //127.0.0.1
        
        assert_eq!(cage.bind_syscall(sockfd, &socket, 4096), 0);
        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        assert_eq!(retsocket, socket);    

        //checking that we cannot rebind the socket
        socket = interface::GenSockaddr::V4(interface::SockaddrV4{ sin_family: 0, sin_port: 50110, sin_addr: interface::V4Addr{ s_addr: u32::from_be_bytes([127, 0, 0, 1]) }}); //127.0.0.1
        assert_eq!(cage.bind_syscall(sockfd, &socket, 4096), -(Errno::EINVAL as i32)); //already bound so should fail
        assert_eq!(cage.getsockname_syscall(sockfd, &mut retsocket), 0);
        assert_ne!(retsocket, socket);

        assert_eq!(cage.exit_syscall(), 0);
        lindrustfinalize();
    }
}