#![allow(dead_code)]

extern crate encoding;
extern crate num;

use std::ffi::c_void;
use std::ffi::CStr;
use std::os::raw::c_char;

use bitflags::bitflags;
use num_derive::FromPrimitive;
use std::ptr;
use std::slice;
use windows::Win32::System::Memory;
use windows::Win32::System::Threading;
use windows::Win32::System::Threading::WaitForSingleObject;

use encoding::all::WINDOWS_1252;
use encoding::{DecoderTrap, Encoding};

const IRSDK_MAX_BUFS: usize = 4;
const IRSDK_MAX_STRING: usize = 32;
// descriptions can be longer than max_string!
const IRSDK_MAX_DESC: usize = 64;

// define markers for unlimited session lap and time
pub const IRSDK_UNLIMITED_LAPS: i32 = 32767;
pub const IRSDK_UNLIMITED_TIME: f64 = 604800.0;

#[derive(Debug)]
pub enum Error {
    InvalidType,
    InvalidEnumValue,
}

pub trait FromValue: Sized {
    /// Converts an iracing Value into Rust value.
    fn var_result(value: &Value) -> Result<Self, Error>;
}

bitflags! {
    pub struct StatusField:i32 {
        const CONNECTED = 1;
    }
}

bitflags! {
    pub struct EngineWarnings:i32 {
        const WATER_TEMP_WARNING    = 0x01;
        const FUEL_PRESSURE_WARNING = 0x02;
        const OIL_PRESSURE_WARNING  = 0x04;
        const ENGINE_STALLED        = 0x08;
        const PIT_SPEED_LIMITER     = 0x10;
        const REV_LIMITER_ACTIVE    = 0x20;
        const OIL_TEMP_WARNING      = 0x40;
    }
}
impl FromValue for EngineWarnings {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        Ok(Self::from_bits_truncate(v))
    }
}

bitflags! {
    pub struct Flags:u32 {
        // global flags
        const CHECKERED = 0x00000001;
        const WHITE     = 0x00000002;
        const GREEN     = 0x00000004;
        const YELLOW    = 0x00000008;
        const RED       = 0x00000010;
        const BLUE      = 0x00000020;
        const DEBRIS    = 0x00000040;
        const CROSSED   = 0x00000080;
        const YELLOW_WAVING = 0x00000100;
        const ONE_TO_GREEN  = 0x00000200;
        const GREEN_HELD    = 0x00000400;
        const LAPS_10_TO_GO = 0x00000800;
        const LAPS_5_TO_GO  = 0x00001000;
        const RANDOM_WAVING = 0x00002000;
        const CAUTION       = 0x00004000;
        const CAUTION_WAVING= 0x00008000;

        // driver black flags
        const BLACK         = 0x00010000;
        const DISQUALIFY    = 0x00020000;
        const SERVICABLE    = 0x00040000;   // aka can pit
        const FURLED        = 0x00080000;
        const REPAIR        = 0x00100000;

        // start lights
        const START_HIDDEN  = 0x10000000;
        const START_READY   = 0x20000000;
        const START_SET     = 0x40000000;
        const START_GO      = 0x80000000;
    }
}
impl FromValue for Flags {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        Ok(Self::from_bits_truncate(v as u32))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, FromPrimitive)]
pub enum TrackLocation {
    NotInWorld = -1,
    OffTrack,
    InPitStall,
    ApproachingPits,
    OnTrack,
}
impl FromValue for TrackLocation {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        match num::FromPrimitive::from_i32(v) {
            Some(t) => Ok(t),
            None => Err(Error::InvalidEnumValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, FromPrimitive)]
pub enum TrackSurface {
    SurfaceNotInWorld = -1,
    UndefinedMaterial = 0,

    Asphalt1Material,
    Asphalt2Material,
    Asphalt3Material,
    Asphalt4Material,
    Concrete1Material,
    Concrete2Material,
    RacingDirt1Material,
    RacingDirt2Material,
    Paint1Material,
    Paint2Material,
    Rumble1Material,
    Rumble2Material,
    Rumble3Material,
    Rumble4Material,

    Grass1Material,
    Grass2Material,
    Grass3Material,
    Grass4Material,
    Dirt1Material,
    Dirt2Material,
    Dirt3Material,
    Dirt4Material,
    SandMaterial,
    Gravel1Material,
    Gravel2Material,
    GrasscreteMaterial,
    AstroturfMaterial,
}
impl FromValue for TrackSurface {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        match num::FromPrimitive::from_i32(v) {
            Some(t) => Ok(t),
            None => Err(Error::InvalidEnumValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, FromPrimitive)]
pub enum SessionState {
    Invalid,
    GetInCar,
    Warmup,
    ParadeLaps,
    Racing,
    Checkered,
    CoolDown,
}
impl FromValue for SessionState {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        match num::FromPrimitive::from_i32(v) {
            Some(t) => Ok(t),
            None => Err(Error::InvalidEnumValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, FromPrimitive)]
pub enum CarLeftRight {
    LROff,
    LRClear,        // no cars around us.
    LRCarLeft,      // there is a car to our left.
    LRCarRight,     // there is a car to our right.
    LRCarLeftRight, // there are cars on each side.
    LR2CarsLeft,    // there are two cars to our left.
    LR2CarsRight,   // there are two cars to our right.
}
impl FromValue for CarLeftRight {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        match num::FromPrimitive::from_i32(v) {
            Some(t) => Ok(t),
            None => Err(Error::InvalidEnumValue),
        }
    }
}

bitflags! {
    pub struct CameraState:i32 {
        const IS_SESSION_SCREEN     = 0x0001; // the camera tool can only be activated if viewing the session screen (out of car)
        const IS_SCENIC_ACTIVE      = 0x0002; // the scenic camera is active (no focus car)

        //these can be changed with a broadcast message
        const CAM_TOOL_ACTIVE           = 0x0004;
        const UI_HIDDEN                 = 0x0008;
        const USE_AUTO_SHOT_SELECTION   = 0x0010;
        const USE_TEMPORARY_EDITS       = 0x0020;
        const USE_KEY_ACCELERATION      = 0x0040;
        const USE_KEY_10X_ACCELERATION  = 0x0080;
        const USE_MOUSE_AIM_MODE        = 0x0100;
    }
}
impl FromValue for CameraState {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        Ok(Self::from_bits_truncate(v))
    }
}

bitflags! {
    pub struct PitSvcFlags:i32 {
        const LF_TIRE_CHANGE	= 0x0001;
        const RF_TIRE_CHANGE	= 0x0002;
        const LR_TIRE_CHANGE    = 0x0004;
        const RR_TIRE_CHANGE	= 0x0008;

        const FUEL_FILL			= 0x0010;
        const WINDSHIELD_TEAROFF= 0x0020;
        const FAST_REPAIR		= 0x0040;
    }
}
impl FromValue for PitSvcFlags {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        Ok(Self::from_bits_truncate(v))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, FromPrimitive)]
pub enum PitSvcStatus {
    // status
    PitSvNone = 0,
    PitSvInProgress,
    PitSvComplete,

    // errors
    PitSvTooFarLeft = 100,
    PitSvTooFarRight,
    PitSvTooFarForward,
    PitSvTooFarBack,
    PitSvBadAngle,
    PitSvCantFixThat,
}
impl FromValue for PitSvcStatus {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        match num::FromPrimitive::from_i32(v) {
            Some(t) => Ok(t),
            None => Err(Error::InvalidEnumValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, FromPrimitive)]
pub enum PaceMode {
    PaceModeSingleFileStart = 0,
    PaceModeDoubleFileStart,
    PaceModeSingleFileRestart,
    PaceModeDoubleFileRestart,
    PaceModeNotPacing,
}
impl FromValue for PaceMode {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        match num::FromPrimitive::from_i32(v) {
            Some(t) => Ok(t),
            None => Err(Error::InvalidEnumValue),
        }
    }
}

bitflags! {
    pub struct PaceFlags:i32 {
        const END_OF_LINE   = 0x01;
        const FREE_PASS     = 0x02;
        const WAVED_AROUND  = 0x04;
    }
}
impl FromValue for PaceFlags {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        Ok(Self::from_bits_truncate(v))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VarType {
    // 1 byte
    CHAR = 0,
    BOOL = 1,

    // 4 bytes
    INT = 2,
    BITFIELD = 3,
    FLOAT = 4,

    // 8 bytes
    DOUBLE = 5,

    //index, don't use
    ETCOUNT = 6,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value<'a> {
    Char(u8),
    Chars(&'a [u8]),
    Bool(bool),
    Bools(&'a [bool]),
    Int(i32),
    Ints(&'a [i32]),
    Bitfield(i32),
    Bitfields(&'a [i32]),
    Float(f32),
    Floats(&'a [f32]),
    Double(f64),
    Doubles(&'a [f64]),
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
#[derive(Clone, Copy, Debug)]
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
#[allow(dead_code)]
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

#[derive(Debug)]
pub struct Var {
    hdr: IrsdkVarHeader,
}
#[allow(dead_code)]
impl Var {
    pub fn var_type(&self) -> VarType {
        return self.hdr.var_type;
    }
    pub fn name(&self) -> &str {
        self.hdr.name().unwrap()
    }
    pub fn desc(&self) -> &str {
        self.hdr.desc().unwrap()
    }
    pub fn unit(&self) -> &str {
        self.hdr.unit().unwrap()
    }
    pub fn count(&self) -> usize {
        self.hdr.count as usize
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
#[allow(dead_code)]
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
    pub fn wait_for_data(&mut self, wait: std::time::Duration) -> bool {
        if self.get_new_data() {
            return true;
        }
        unsafe {
            WaitForSingleObject(self.new_data, wait.as_millis().try_into().unwrap());
        }
        self.get_new_data()
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
    pub fn dump_vars(&self) {
        match self.header {
            None => {}
            Some(h) => unsafe {
                let vhbase =
                    self.shared_mem.add((*h).var_header_offset as usize) as *const IrsdkVarHeader;
                for i in 0..(*h).num_vars {
                    let vh = vhbase.add(i as usize);
                    let var = Var { hdr: *vh };
                    let value = self.var_value(&var);
                    println!(
                        "{:40} {:32}: {:?}: {}: {}: {:?}",
                        var.desc(),
                        var.name(),
                        var.var_type(),
                        var.count(),
                        var.hdr.count_as_time,
                        value,
                    );
                }
            },
        }
    }
    pub fn find_var(&self, name: &str) -> Option<Var> {
        match self.header {
            None => None,
            Some(h) => {
                unsafe {
                    let vhbase = self.shared_mem.add((*h).var_header_offset as usize)
                        as *const IrsdkVarHeader;
                    for i in 0..(*h).num_vars as usize {
                        let vh = vhbase.add(i);
                        if (*vh).has_name(name) {
                            return Some(Var { hdr: *vh });
                        }
                    }
                }
                None
            }
        }
    }
    pub fn var_value(&self, var: &Var) -> Value {
        unsafe {
            let x = self.data.as_ptr().add(var.hdr.offset as usize);
            if var.hdr.count == 1 {
                match var.hdr.var_type {
                    VarType::CHAR => Value::Char(*x),
                    VarType::BOOL => Value::Bool(*(x as *const bool)),
                    VarType::INT => Value::Int(*(x as *const i32)),
                    VarType::BITFIELD => Value::Bitfield(*(x as *const i32)),
                    VarType::FLOAT => Value::Float(*(x as *const f32)),
                    VarType::DOUBLE => Value::Double(*(x as *const f64)),
                    VarType::ETCOUNT => todo!(),
                }
            } else {
                let l = var.count();
                match var.hdr.var_type {
                    VarType::CHAR => Value::Chars(slice::from_raw_parts(x, l)),
                    VarType::BOOL => Value::Bools(slice::from_raw_parts(x as *const bool, l)),
                    VarType::INT => Value::Ints(slice::from_raw_parts(x as *const i32, l)),
                    VarType::BITFIELD => {
                        Value::Bitfields(slice::from_raw_parts(x as *const i32, l))
                    }
                    VarType::FLOAT => Value::Floats(slice::from_raw_parts(x as *const f32, l)),
                    VarType::DOUBLE => Value::Doubles(slice::from_raw_parts(x as *const f64, l)),
                    VarType::ETCOUNT => todo!(),
                }
            }
        }
    }
    pub fn value<T: FromValue>(&self, var: &Var) -> Result<T, Error> {
        let v = self.var_value(var);
        T::var_result(&v)
    }
    pub fn session_info_update(&self) -> Option<i32> {
        unsafe { self.header.map(|h| (*h).session_info_update) }
    }
    pub fn session_info(&self) -> Result<String, std::borrow::Cow<str>> {
        match self.header {
            None => Err(std::borrow::Cow::from("not connected")),
            Some(h) => unsafe {
                let p = self.shared_mem.add((*h).session_info_offset as usize) as *mut u8;
                let mut bytes = std::slice::from_raw_parts(p, (*h).session_info_len as usize);
                // session_info_len is the size of the buffer, not necessarily the size of the string
                // so we have to look for the null terminatior.
                for i in 0..bytes.len() {
                    if bytes[i] == 0 {
                        bytes = &bytes[0..i];
                        break;
                    }
                }
                WINDOWS_1252.decode(bytes, DecoderTrap::Replace)
            },
        }
    }
}

impl<'a> Value<'a> {
    pub fn as_f64(&self) -> Result<f64, Error> {
        match *self {
            Value::Double(f) => Ok(f),
            _ => Err(Error::InvalidType),
        }
    }
    pub fn as_f32(&self) -> Result<f32, Error> {
        match *self {
            Value::Float(f) => Ok(f),
            _ => Err(Error::InvalidType),
        }
    }
    pub fn as_i32(&self) -> Result<i32, Error> {
        match *self {
            Value::Int(f) => Ok(f),
            Value::Bitfield(f) => Ok(f),
            _ => Err(Error::InvalidType),
        }
    }
    pub fn as_bool(&self) -> Result<bool, Error> {
        match *self {
            Value::Bool(f) => Ok(f),
            _ => Err(Error::InvalidType),
        }
    }
    pub fn as_u8(&self) -> Result<u8, Error> {
        match *self {
            Value::Char(f) => Ok(f),
            _ => Err(Error::InvalidType),
        }
    }
    pub fn as_f64s(&self) -> Result<&[f64], Error> {
        match *self {
            Value::Doubles(f) => Ok(f),
            _ => Err(Error::InvalidType),
        }
    }
    pub fn as_f32s(&self) -> Result<&[f32], Error> {
        match *self {
            Value::Floats(f) => Ok(f),
            _ => Err(Error::InvalidType),
        }
    }
    pub fn as_i32s(&self) -> Result<&[i32], Error> {
        match *self {
            Value::Ints(f) => Ok(f),
            _ => Err(Error::InvalidType),
        }
    }
    pub fn as_bools(&self) -> Result<&[bool], Error> {
        match *self {
            Value::Bools(f) => Ok(f),
            _ => Err(Error::InvalidType),
        }
    }
    pub fn as_u8s(&self) -> Result<&[u8], Error> {
        match *self {
            Value::Chars(f) => Ok(f),
            _ => Err(Error::InvalidType),
        }
    }
}

impl FromValue for bool {
    fn var_result(value: &Value) -> Result<Self, Error> {
        value.as_bool()
    }
}
impl FromValue for u8 {
    fn var_result(value: &Value) -> Result<Self, Error> {
        value.as_u8()
    }
}
impl FromValue for i32 {
    fn var_result(value: &Value) -> Result<Self, Error> {
        value.as_i32()
    }
}
impl FromValue for f32 {
    fn var_result(value: &Value) -> Result<Self, Error> {
        value.as_f32()
    }
}
impl FromValue for f64 {
    fn var_result(value: &Value) -> Result<Self, Error> {
        value.as_f64()
    }
}
