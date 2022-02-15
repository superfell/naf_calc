use std::ffi::c_void;
use std::ffi::CStr;
use std::os::raw::c_char;

use bitflags::bitflags;
use std::ptr;
use windows::Win32::System::Memory;
use windows::Win32::System::Threading;

const IRSDK_MAX_BUFS: usize = 4;
const IRSDK_MAX_STRING: usize = 32;
// descriptions can be longer than max_string!
const IRSDK_MAX_DESC: usize = 64;

bitflags! {
    pub struct StatusField:i32 {
        const CONNECTED = 1;
    }
}

bitflags! {
    pub struct VarType:i32 {
        // 1 byte
        const CHAR = 0;
        const BOOL = 1;

        // 4 bytes
        const INT=2;
        const BITFIELD=3;
        const FLOAT=4;

        // 8 bytes
        const DOUBLE=5;

        //index, don't use
        const ETCOUNT=6;
    }
}

#[repr(C)]
struct IrsdkBuf {
    tick_count: i32, // used to detect changes in data
    buf_offset: i32, // offset from header
    pad: [i32; 2],   // (16 byte align)
}

#[repr(C)]
struct IrsdkHeader {
    ver: i32,            // this api header version, see IRSDK_VER
    status: StatusField, // bitfield using irsdk_StatusField
    tick_rate: i32,      // ticks per second (60 or 360 etc)

    // session information, updated periodicaly
    session_info_update: i32, // Incremented when session info changes
    session_info_len: i32,    // Length in bytes of session info string
    session_info_offset: i32, // Session info, encoded in YAML format

    // State data, output at tickRate
    num_vars: i32,          // length of array pointed to by varHeaderOffset
    var_header_offset: i32, // offset to irsdk_varHeader[numVars] array, Describes the variables received in varBuf

    num_buf: i32,                        // <= IRSDK_MAX_BUFS (3 for now)
    buf_len: i32,                        // length in bytes for one line
    pad1: [i32; 2],                      // (16 byte align)
    var_buf: [IrsdkBuf; IRSDK_MAX_BUFS], // buffers of data being written to
}

#[repr(C)]
struct IrsdkVarHeader {
    var_type: VarType, // irsdk_VarType
    offset: i32,       // offset fron start of buffer row
    count: i32,        // number of entrys (array)
    // so length in bytes would be irsdk_VarTypeBytes[type] * count
    count_as_time: u8,
    pad: [i8; 3], // (16 byte align)

    name: [u8; IRSDK_MAX_STRING],
    desc: [u8; IRSDK_MAX_DESC],
    unit: [u8; IRSDK_MAX_STRING], // something like "kg/m^2"
}
impl IrsdkVarHeader {
    fn name(&self) -> Result<&str, std::str::Utf8Error> {
        unsafe { CStr::from_ptr(self.name.as_ptr() as *const c_char).to_str() }
    }
    fn desc(&self) -> Result<&str, std::str::Utf8Error> {
        unsafe { CStr::from_ptr(self.desc.as_ptr() as *const c_char).to_str() }
    }
    fn unit(&self) -> Result<&str, std::str::Utf8Error> {
        unsafe { CStr::from_ptr(self.unit.as_ptr() as *const c_char).to_str() }
    }
    fn has_name(&self, n: &str) -> bool {
        if n.len() > IRSDK_MAX_STRING {
            return false;
        }
        let b = n.as_bytes();
        for i in 0..b.len() {
            if self.name[i] != b[i] {
                return false;
            }
        }
        for i in b.len()..IRSDK_MAX_STRING {
            if self.name[i] != 0 {
                return false;
            }
        }
        true
    }
}

pub struct Var<'a> {
    hdr: &'a IrsdkVarHeader,
    data: &'a [u8],
}
impl<'a> Var<'a> {
    pub fn var_type(&self) -> VarType {
        return self.hdr.var_type;
    }
    pub fn name(&self) -> &'a str {
        self.hdr.name().unwrap()
    }
    pub fn desc(&self) -> &'a str {
        self.hdr.desc().unwrap()
    }
    pub fn unit(&self) -> &'a str {
        self.hdr.unit().unwrap()
    }
    pub fn int(&self) -> Option<i32> {
        if self.hdr.var_type == VarType::INT {
            unsafe {
                let x = self.data.as_ptr().add(self.hdr.offset as usize);
                let v = x as *const i32;
                Some(*v)
            }
        } else {
            None
        }
    }
    pub fn float(&self) -> Option<f32> {
        if self.hdr.var_type == VarType::FLOAT {
            unsafe {
                let x = self.data.as_ptr().add(self.hdr.offset as usize);
                let v = x as *const f32;
                Some(*v)
            }
        } else {
            None
        }
    }
}

pub struct Client {
    file_mapping: windows::Win32::Foundation::HANDLE,
    shared_mem: *mut c_void,
    header: Option<*mut IrsdkHeader>,

    new_data: windows::Win32::Foundation::HANDLE,

    last_tick_count: i32,
    data: bytes::BytesMut,
}
impl Client {
    pub fn new() -> Self {
        return Client {
            file_mapping: windows::Win32::Foundation::INVALID_HANDLE_VALUE,
            shared_mem: std::ptr::null_mut(),
            header: None,
            new_data: windows::Win32::Foundation::INVALID_HANDLE_VALUE,
            last_tick_count: 0,
            data: bytes::BytesMut::with_capacity(1024),
        };
    }
    pub fn startup(&mut self) -> bool {
        if self.file_mapping.is_invalid() {
            self.last_tick_count = 0;
            unsafe {
                self.file_mapping = Memory::OpenFileMappingA(
                    Memory::FILE_MAP_READ.0,
                    false,
                    "Local\\IRSDKMemMapFileName",
                );
                if !self.file_mapping.is_invalid() {
                    self.shared_mem =
                        Memory::MapViewOfFile(self.file_mapping, Memory::FILE_MAP_READ, 0, 0, 0);
                    if !self.shared_mem.is_null() {
                        self.header = Some(self.shared_mem as *mut IrsdkHeader);
                        self.new_data = Threading::OpenEventA(
                            windows::Win32::Storage::FileSystem::SYNCHRONIZE.0,
                            false,
                            "Local\\IRSDKDataValidEvent",
                        );
                    }
                }
            }
        }
        return !self.file_mapping.is_invalid()
            && !self.shared_mem.is_null()
            && !self.new_data.is_invalid();
    }
    pub fn close(&mut self) {
        unsafe {
            if !self.new_data.is_invalid() {
                windows::Win32::Foundation::CloseHandle(self.new_data);
                self.new_data = windows::Win32::Foundation::INVALID_HANDLE_VALUE;
            }
            if !self.shared_mem.is_null() {
                self.header = None;
                Memory::UnmapViewOfFile(self.shared_mem);
                self.shared_mem = std::ptr::null_mut();
            }
            if !self.file_mapping.is_invalid() {
                windows::Win32::Foundation::CloseHandle(self.file_mapping);
                self.file_mapping = windows::Win32::Foundation::INVALID_HANDLE_VALUE;
            }
        }
    }
    pub fn connected(&self) -> bool {
        unsafe {
            match self.header {
                None => false,
                Some(h) => (*h).status & StatusField::CONNECTED == StatusField::CONNECTED,
            }
        }
    }
    pub fn get_new_data(&mut self) -> bool {
        if !self.startup() {
            return false;
        }
        unsafe {
            if let Some(h) = self.header {
                if !(*h).status.intersects(StatusField::CONNECTED) {
                    self.last_tick_count = 0;
                    return false;
                }
                let mut latest: usize = 0;
                for i in 1..((*h).num_buf as usize) {
                    if (*h).var_buf[latest].tick_count < (*h).var_buf[i].tick_count {
                        latest = i;
                    }
                }
                let buf_len = (*h).buf_len as usize;
                let b = &(*h).var_buf[latest];
                if self.last_tick_count < b.tick_count {
                    if self.data.capacity() < buf_len {
                        println!("buf len {}", buf_len);
                        self.data.reserve(buf_len)
                    }
                    for _tries in 0..2 {
                        let curr_tick_count = b.tick_count;
                        let src = self.shared_mem.add(b.buf_offset as usize);
                        ptr::copy_nonoverlapping(src.cast(), self.data.as_mut_ptr(), buf_len);
                        if curr_tick_count == b.tick_count {
                            self.last_tick_count = curr_tick_count;
                            return true;
                        }
                    }
                }
            }
        }
        return false;
    }
    pub fn find_var(&self, name: &str) -> Option<Var> {
        match self.header {
            None => None,
            Some(h) => {
                unsafe {
                    println!("number of vars {}", (*h).num_vars);
                    let vhbase = self.shared_mem.add((*h).var_header_offset as usize)
                        as *const IrsdkVarHeader;
                    for i in 0..(*h).num_vars as usize {
                        let vh = vhbase.add(i);
                        if (*vh).has_name(name) {
                            return Some(Var {
                                hdr: &*vh,
                                data: &self.data,
                            });
                        }
                    }
                }
                None
            }
        }
    }
    pub fn session_info(&self) -> &str {
        match self.header {
            None => "",
            Some(h) => unsafe {
                let p = self.shared_mem.add((*h).session_info_offset as usize);
                CStr::from_ptr(p as *const c_char).to_str().unwrap()
            },
        }
    }
}
