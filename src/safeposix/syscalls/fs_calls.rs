// File system related system calls

use crate::interface;

use super::fs_constants::*;
use crate::safeposix::cage::{CAGE_TABLE, Cage, FileDescriptor::*, FileDesc};
use crate::safeposix::filesystem::*;
use super::errnos::*;

impl Cage {

    //------------------OPEN SYSCALL------------------

    pub fn open_syscall(&self, path: &str, flags: i32, mode: u32) -> i32 {
        //Check that path is not empty
        if path.len() != 0 {return syscall_error(Errno::ENOENT, "open", "given path was null");}

        let truepath = normpath(convpath(path), self);

        //file descriptor table write lock held for the whole function to prevent TOCTTOU
        let mut fdtable = self.filedescriptortable.write().unwrap();
        //file system metadata table write lock held for the whole function to prevent TOCTTOU
        let mut mutmetadata = FS_METADATA.write().unwrap();

        let thisfd = if let Some(fd) = self.get_next_fd(None, Some(&fdtable)) {
            fd
        } else {
            return syscall_error(Errno::ENFILE, "open", "no available file descriptor number could be found");
        };


        match metawalkandparent(truepath.as_path(), Some(&mutmetadata)) {
            //If neither the file nor parent exists
            (None, None) => {
                if 0 != (flags & O_CREAT) {
                    return syscall_error(Errno::ENOENT, "open", "tried to open a file that did not exist, and O_CREAT was not specified");
                }
                return syscall_error(Errno::ENOENT, "open", "a directory component in pathname does not exist or is a dangling symbolic link");
            }

            //If the file doesn't exist but the parent does
            (None, Some(pardirinode)) => {
                if 0 != (flags & O_CREAT) {
                    return syscall_error(Errno::ENOENT, "open", "tried to open a file that did not exist, and O_CREAT was not specified");
                }

                let filename = truepath.file_name(); //for now we assume this is sane, but maybe this should be checked later

                if 0 != (S_IFCHR & flags) {
                    return syscall_error(Errno::EINVAL, "open", "Invalid value in flags");
                } 

                let effective_mode = S_IFREG as u32 | mode;

                if mode & (S_IRWXA | S_FILETYPEFLAGS as u32) != mode {
                    return syscall_error(Errno::EPERM, "open", "Mode bits were not sane");
                } //assert sane mode bits

                let time = interface::timestamp(); //We do a real timestamp now
                let newinode = Inode::File(GenericInode {
                    size: 0, uid: DEFAULT_UID, gid: DEFAULT_GID,
                    mode: effective_mode, linkcount: 1, refcount: 0,
                    atime: time, ctime: time, mtime: time,
                });

                let newinodenum = mutmetadata.nextinode;
                mutmetadata.nextinode += 1;
                if let Inode::Dir(ind) = mutmetadata.inodetable.get_mut(&pardirinode).unwrap() {
                    ind.filename_to_inode_dict.insert(filename.unwrap().to_owned(), newinodenum);
                } //insert a reference to the file in the parent directory
                mutmetadata.inodetable.insert(newinodenum, newinode).unwrap();
                //persist metadata?
            }

            //If the file exists (we don't need to look at parent here)
            (Some(inodenum), ..) => {
                if (O_CREAT | O_EXCL) == (flags & (O_CREAT | O_EXCL)) {
                    return syscall_error(Errno::EEXIST, "open", "file already exists and O_CREAT and O_EXCL were used");
                }

                if 0 != (flags & O_TRUNC) {
                    //close the file object if another cage has it open
                    if mutmetadata.fileobjecttable.contains_key(&inodenum) {
                        mutmetadata.fileobjecttable.get(&inodenum).unwrap().close().unwrap();
                    }

                    //set size of file to 0
                    match mutmetadata.inodetable.get_mut(&inodenum).unwrap() {
                        Inode::File(g) => {g.size = 0;}
                        _ => {
                            return syscall_error(Errno::EINVAL, "open", "file is not a normal file and thus cannot be truncated");
                        }
                    }

                    //remove the previous file and add a new one of 0 length
                    let sysfilename = format!("{}{}", FILEDATAPREFIX, inodenum);
                    interface::removefile(sysfilename.clone()).unwrap();
                    mutmetadata.fileobjecttable.insert(inodenum, interface::openfile(sysfilename, true).unwrap());
                }
            }
        }

        //We redo our metawalk in case of O_CREAT, but this is somewhat inefficient
        if let Some(inodenum) = metawalk(truepath.as_path(), Some(&mutmetadata)) {
            let inodeobj = mutmetadata.inodetable.get_mut(&inodenum).unwrap();
            let mode;
            let size;

            //increment number of open handles to the file, retrieve other data from inode
            match inodeobj {
                Inode::File(f) => {size = f.size; mode = f.mode; f.refcount += 1},
                Inode::Dir(f) => {size = f.size; mode = f.mode; f.refcount += 1},
                Inode::CharDev(f) => {size = f.size; mode = f.mode; f.refcount += 1},
                _ => {panic!("How did you even manage to open another kind of file like that?");},
            }

            //If the file is a regular file, open the file object
            if is_reg(mode) {
                if mutmetadata.fileobjecttable.contains_key(&inodenum) {
                    let sysfilename = format!("{}{}", FILEDATAPREFIX, inodenum);
                    mutmetadata.fileobjecttable.insert(inodenum, interface::openfile(sysfilename, false).unwrap());
                }
            }

            //insert file descriptor into fdtableable of the cage
            let position = if 0 != flags & O_APPEND {size} else {0};
            let newfd = File(FileDesc {position: position, inode: inodenum, flags: flags & O_RDWRFLAGS});
            let wrappedfd = interface::RustRfc::new(interface::RustLock::new(newfd));
            fdtable.insert(thisfd, wrappedfd);
        } else {panic!("Inode not created for some reason");}
        thisfd //open returns the opened file descriptr
    }

    //------------------CREAT SYSCALL------------------
    
    pub fn creat_syscall(&self, path: &str, mode: u32) -> i32 {
        self.open_syscall(path, O_CREAT | O_TRUNC | O_WRONLY, mode)
    }

    //------------------STAT SYSCALL------------------

    pub fn stat_syscall(&self, path: &str, statbuf: &mut StatData) -> i32 {
        let truepath = normpath(convpath(path), self);
        let metadata = FS_METADATA.read().unwrap();

        //Walk the file tree to get inode from path
        if let Some(inodenum) = metawalk(truepath.as_path(), Some(&metadata)) {
            let inodeobj = metadata.inodetable.get(&inodenum).unwrap();
            
            //populate those fields in statbuf which depend on things other than the inode object
            statbuf.st_dev = metadata.dev_id;
            statbuf.st_ino = inodenum;

            //delegate the rest of populating statbuf to the relevant helper
            match inodeobj {
                Inode::File(f) => {
                    Self::_istat_helper(f, statbuf);
                },
                Inode::CharDev(f) => {
                    Self::_istat_helper_chr_file(f, statbuf);
                },
                Inode::Dir(f) => {
                    Self::_istat_helper_dir(f, statbuf);
                },
                Inode::Pipe(_) => {
                    panic!("How did you even manage to refer to a pipe using a path?");
                },
                Inode::Socket(_) => {
                    panic!("How did you even manage to refer to a socket using a path?");
                },
            }
            0 //stat has succeeded!
        } else {
            syscall_error(Errno::ENOENT, "stat", "path refers to an invalid file")
        }

    }

    fn _istat_helper(inodeobj: &GenericInode, statbuf: &mut StatData) {
        statbuf.st_mode = inodeobj.mode;
        statbuf.st_nlink = inodeobj.linkcount;
        statbuf.st_uid = inodeobj.uid;
        statbuf.st_gid = inodeobj.gid;
        statbuf.st_rdev = 0;
        statbuf.st_size = inodeobj.size;
        statbuf.st_blksize = 0;
        statbuf.st_blocks = 0;
    }

    fn _istat_helper_dir(inodeobj: &DirectoryInode, statbuf: &mut StatData) {
        statbuf.st_mode = inodeobj.mode;
        statbuf.st_nlink = inodeobj.linkcount;
        statbuf.st_uid = inodeobj.uid;
        statbuf.st_gid = inodeobj.gid;
        statbuf.st_rdev = 0;
        statbuf.st_size = inodeobj.size;
        statbuf.st_blksize = 0;
        statbuf.st_blocks = 0;
    }

    fn _istat_helper_chr_file(inodeobj: &DeviceInode, statbuf: &mut StatData) {
        statbuf.st_dev = 5;
        statbuf.st_mode = inodeobj.mode;
        statbuf.st_nlink = inodeobj.linkcount;
        statbuf.st_uid = inodeobj.uid;
        statbuf.st_gid = inodeobj.gid;
        //compose device number into u64
        statbuf.st_rdev = makedev(&inodeobj.dev);
        statbuf.st_size = inodeobj.size;
    }

    //Streams and pipes don't have associated inodes so we populate them from mostly dummy information
    fn _stat_alt_helper(&self, statbuf: &mut StatData, inodenum: usize, metadata: &FilesystemMetadata) {
        statbuf.st_dev = metadata.dev_id;
        statbuf.st_ino = inodenum;
        statbuf.st_mode = 49590; //r and w priveliged 
        statbuf.st_nlink = 1;
        statbuf.st_uid = DEFAULT_UID;
        statbuf.st_gid = DEFAULT_GID;
        statbuf.st_rdev = 0;
        statbuf.st_size = 0;
        statbuf.st_blksize = 0;
        statbuf.st_blocks = 0;
    }


    //------------------FSTAT SYSCALL------------------

    pub fn fstat_syscall(&self, fd: i32, statbuf: &mut StatData) -> i32 {
        let fdtable = self.filedescriptortable.read().unwrap();
 
        if let Some(wrappedfd) = fdtable.get(&fd) {
            let filedesc_enum = wrappedfd.read().unwrap();
            let metadata = FS_METADATA.read().unwrap();

            //Delegate populating statbuf to the relevant helper depending on the file type.
            //First we check in the file descriptor to handle sockets, streams, and pipes,
            //and if it is a normal file descriptor we handle regular files, dirs, and char 
            //files based on the information in the inode.
            match &*filedesc_enum {
                File(normalfile_filedesc_obj) => {
                    let inode = metadata.inodetable.get(&normalfile_filedesc_obj.inode).unwrap();

                    //populate those fields in statbuf which depend on things other than the inode object
                    statbuf.st_ino = normalfile_filedesc_obj.inode;
                    statbuf.st_dev = metadata.dev_id;

                    match inode {
                        Inode::File(f) => {
                            Self::_istat_helper(&f, statbuf);
                        }
                        Inode::CharDev(f) => {
                            Self::_istat_helper_chr_file(&f, statbuf);
                        }
                        Inode::Dir(f) => {
                            Self::_istat_helper_dir(&f, statbuf);
                        }
                        _ => {panic!("A file fd points to a socket or pipe");}
                    }
                }
                Socket(_) => {
                    return syscall_error(Errno::EOPNOTSUPP, "fstat", "we don't support fstat on sockets yet");
                    }
                Stream(_) => {self._stat_alt_helper(statbuf, STREAMINODE, &metadata);}
                Pipe(_) => {self._stat_alt_helper(statbuf, 0xfeef0000, &metadata);}
            }
            0 //fstat has succeeded!
        } else {
            syscall_error(Errno::ENOENT, "fstat", "invalid file descriptor")
        }
    }

    //------------------READ SYSCALL------------------

    pub fn read_syscall(&self, fd: i32, buf: *mut u8, count: usize) -> i32 {
        let fdtable = self.filedescriptortable.write().unwrap();
 
        if let Some(wrappedfd) = fdtable.get(&fd) {
            let mut filedesc_enum = wrappedfd.write().unwrap();

            match &mut *filedesc_enum {
                File(ref mut normalfile_filedesc_obj) => {
                    if is_wronly(normalfile_filedesc_obj.flags) {
                        return syscall_error(Errno::EBADF, "read", "specified file not open for reading");
                    }

                    let metadata = FS_METADATA.read().unwrap();
                    let inodeobj = metadata.inodetable.get(&normalfile_filedesc_obj.inode).unwrap();
                    match inodeobj {
                        Inode::File(_) => {
                            let position = normalfile_filedesc_obj.position;
                            let fileobject = metadata.fileobjecttable.get(&normalfile_filedesc_obj.inode).unwrap();
                            if let Ok(bytesread) = fileobject.readat(buf, count, position) {
                               normalfile_filedesc_obj.position += bytesread;
                               bytesread as i32
                            } else {
                               0 //0 bytes read, but not an error value that can/should be passed to the user
                            }
                        }
                        Inode::CharDev(char_inode_obj) => {
                            self._read_chr_file(char_inode_obj, buf, count)
                        }
                        Inode::Dir(_) => {
                            syscall_error(Errno::EISDIR, "read", "attempted to read from a directory")
                        }
                        _ => {panic!("Wonky file descriptor shenanigains");}
                    }
                }
                Socket(_) => {syscall_error(Errno::EOPNOTSUPP, "read", "recv not implemented yet")}
                Stream(_) => {syscall_error(Errno::EOPNOTSUPP, "read", "reading from stdin not implemented yet")}
                Pipe(pipe_filedesc_obj) => {
                    if is_wronly(pipe_filedesc_obj.flags) {
                        return syscall_error(Errno::EBADF, "read", "specified file not open for reading");
                    }
                    //self._read_from_pipe...
                    syscall_error(Errno::EOPNOTSUPP, "read", "reading from a pipe not implemented yet")
                }
            }
        } else {
            syscall_error(Errno::EBADF, "read", "invalid file descriptor")
        }
    }

    fn _read_chr_file(&self, inodeobj: &DeviceInode, buf: *mut u8, count: usize) -> i32 {
        match inodeobj.dev {
            NULLDEVNO => {0} //reading from /dev/null always reads 0 bytes
            ZERODEVNO => {interface::fillzero(buf, count)}
            RANDOMDEVNO => {interface::fillrandom(buf, count)}
            URANDOMDEVNO => {interface::fillrandom(buf, count)}
            _ => {syscall_error(Errno::EOPNOTSUPP, "read or readat", "read from specified device not implemented")}
        }
    }

    //------------------WRITE SYSCALL------------------

    pub fn write_syscall(&self, fd: i32, buf: *const u8, count: usize) -> i32 {
        let fdtable = self.filedescriptortable.write().unwrap();
 
        if let Some(wrappedfd) = fdtable.get(&fd) {
            let mut filedesc_enum = wrappedfd.write().unwrap();

            match &mut *filedesc_enum {
                //we must borrow the filedesc object as a mutable reference to update the position
                File(ref mut normalfile_filedesc_obj) => {
                    if is_rdonly(normalfile_filedesc_obj.flags) {
                        return syscall_error(Errno::EBADF, "write", "specified file not open for writing");
                    }

                    let mut metadata = FS_METADATA.write().unwrap();
                    let inodeobj = metadata.inodetable.get(&normalfile_filedesc_obj.inode).unwrap();

                    match inodeobj {
                        Inode::File(_) => {
                            let position = normalfile_filedesc_obj.position;
                            let fileobject = metadata.fileobjecttable.get_mut(&normalfile_filedesc_obj.inode).unwrap();

                            if let Ok(byteswritten) = fileobject.writeat(buf, count, position) {
                               normalfile_filedesc_obj.position += byteswritten;
                               byteswritten as i32
                            } else {
                               0 //0 bytes written, but not an error value that can/should be passed to the user
                            }
                        }

                        Inode::CharDev(char_inode_obj) => {
                            self._write_chr_file(char_inode_obj, buf, count)
                        }

                        Inode::Dir(_) => {
                            syscall_error(Errno::EISDIR, "write", "attempted to write to a directory")
                        }
                        _ => {panic!("Wonky file descriptor shenanigains");}
                    }
                }
                Socket(_) => {syscall_error(Errno::EOPNOTSUPP, "write", "send not implemented yet")}
                Stream(stream_filedesc_obj) => {
                    //if it's stdout or stderr, print out and we're done
                    if let 1..=2 = stream_filedesc_obj.stream {
                        interface::log_from_ptr(buf);
                        count as i32
                    } else {
                        0
                    }
                }
                Pipe(pipe_filedesc_obj) => {
                    if is_rdonly(pipe_filedesc_obj.flags) {
                        return syscall_error(Errno::EBADF, "write", "specified pipe not open for writing");
                    }
                    //self._write_to_pipe...
                    syscall_error(Errno::EOPNOTSUPP, "write", "writing from a pipe not implemented yet")
                }
            }
        } else {
            syscall_error(Errno::EBADF, "write", "invalid file descriptor")
        }
    }

    fn _write_chr_file(&self, inodeobj: &DeviceInode, _buf: *const u8, count: usize) -> i32 {
        //writes to any of these device files transparently succeed while doing nothing
        match inodeobj.dev {
            NULLDEVNO => {count as i32}
            ZERODEVNO => {count as i32}
            RANDOMDEVNO => {count as i32}
            URANDOMDEVNO => {count as i32}
            _ => {syscall_error(Errno::EOPNOTSUPP, "write or writeat", "write to specified device not implemented")}
        }
    }

    //------------------ACCESS SYSCALL------------------

    pub fn access_syscall(&self, path: &str, amode: u32) -> i32 {
        let truepath = normpath(convpath(path), self);
        let metadata = FS_METADATA.read().unwrap();


        //Walk the file tree to get inode from path
        if let Some(inodenum) = metawalk(truepath.as_path(), Some(&metadata)) {
            let inodeobj = metadata.inodetable.get(&inodenum).unwrap();

            //Get the mode bits if the type of the inode is sane
            let mode = match inodeobj {
                Inode::File(f) => {f.mode},
                Inode::CharDev(f) => {f.mode},
                Inode::Dir(f) => {f.mode},
                Inode::Pipe(_) => {
                    panic!("How did you even manage to refer to a pipe by a path?");
                },
                Inode::Socket(_) => {
                    panic!("How did you even manage to refer to a socket by a path?");
                },
            };

            //We assume that the current user owns the file

            //Construct desired access bits (i.e. 0777) based on the amode parameter
            let mut newmode: u32 = 0;
            if amode & X_OK == X_OK {newmode |= S_IXUSR;}
            if amode & W_OK == W_OK {newmode |= S_IWUSR;}
            if amode & R_OK == R_OK {newmode |= S_IRUSR;}

            //if the desired access bits are compatible with the actual access bits 
            //of the file, return a success result, else return a failure result
            if mode & newmode == newmode {
                0
            } else {
                syscall_error(Errno::EACCES, "access", "the requested access would be denied to the file")
            }
        } else {
            syscall_error(Errno::ENOENT, "access", "path does not refer to an existing file")
        }
    }

    //------------------CHDIR SYSCALL------------------
    
    pub fn chdir_syscall(&self, path: &str) -> i32 {
        let truepath = normpath(convpath(path), self);
        let mutmetadata = FS_METADATA.write().unwrap();

        //Walk the file tree to get inode from path
        if let Some(inodenum) = metawalk(&truepath, Some(&mutmetadata)) {
            if let Inode::Dir(dir) = mutmetadata.inodetable.get(&inodenum).unwrap() {

                //decrement refcount of previous cwd inode, however this is complex because of cage
                //initialization and deinitialization concerns so we leave it unimplemented for now
                //if let Some(oldinodenum) = metawalk(&self.cwd, Some(&mutmetadata)) {
                //    if let Inode::Dir(olddir) = mutmetadata.inodetable.get(&oldinodenum).unwrap() {
                //        olddir.linkcount -= 1;
                //    } else {panic!("We changed from a directory that was not a directory in chdir!");}
                //} else {panic!("We changed from a directory that was not a directory in chdir!");}

                self.changedir(truepath);

                //increment refcount of new cwd inode to ensure that you can't remove a directory,
                //currently unimplmented
                //dir.linkcount += 1;

                0 //chdir has succeeded!;
            } else {
                syscall_error(Errno::ENOTDIR, "chdir", "the last component in path is not a directory")
            }
        } else {
            syscall_error(Errno::ENOENT, "chdir", "the directory referred to in path does not exist")
        }
    }
}
