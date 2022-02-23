#![allow(dead_code)]

extern crate encoding;
extern crate num;

use core::fmt;
use std::cmp::Ordering;
use std::ffi::c_void;
use std::ffi::CStr;
use std::os::raw::c_char;

use bitflags::bitflags;
use num_derive::FromPrimitive;
use std::slice;
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, WIN32_ERROR};
use windows::Win32::System::Memory;
use windows::Win32::System::Threading;

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
    InvalidEnumValue(i32),
    Win32(WIN32_ERROR),
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
            None => Err(Error::InvalidEnumValue(v)),
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
            None => Err(Error::InvalidEnumValue(v)),
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
            None => Err(Error::InvalidEnumValue(v)),
        }
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, FromPrimitive)]
pub enum CarLeftRight {
    Off,
    Clear,        // no cars around us.
    CarLeft,      // there is a car to our left.
    CarRight,     // there is a car to our right.
    CarLeftRight, // there are cars on each side.
    TwoCarsLeft,  // there are two cars to our left.
    TwoCarsRight, // there are two cars to our right.
}
impl FromValue for CarLeftRight {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        match num::FromPrimitive::from_i32(v) {
            Some(t) => Ok(t),
            None => Err(Error::InvalidEnumValue(v)),
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
    None = 0,
    InProgress,
    Complete,

    // errors
    TooFarLeft = 100,
    TooFarRight,
    TooFarForward,
    TooFarBack,
    BadAngle,
    CantFixThat,
}
impl FromValue for PitSvcStatus {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        match num::FromPrimitive::from_i32(v) {
            Some(t) => Ok(t),
            None => Err(Error::InvalidEnumValue(v)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, FromPrimitive)]
pub enum PaceMode {
    SingleFileStart = 0,
    DoubleFileStart,
    SingleFileRestart,
    DoubleFileRestart,
    NotPacing,
}
impl FromValue for PaceMode {
    fn var_result(value: &Value) -> Result<Self, Error> {
        let v = value.as_i32()?;
        match num::FromPrimitive::from_i32(v) {
            Some(t) => Ok(t),
            None => Err(Error::InvalidEnumValue(v)),
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
    Char = 0,
    Bool = 1,

    // 4 bytes
    Int = 2,
    Bitfield = 3,
    Float = 4,

    // 8 bytes
    Double = 5,

    //index, don't use
    #[deprecated]
    Etcount = 6,
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
    count: i32, // number of entrys (array) so length in bytes would be irsdk_VarTypeBytes[type] * count

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
        for (i, item) in b.iter().enumerate() {
            if *item != self.name[i] {
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

pub struct Var {
    hdr: IrsdkVarHeader,
    session_id: i32,
}
#[allow(dead_code)]
impl Var {
    pub fn var_type(&self) -> VarType {
        self.hdr.var_type
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
impl fmt::Debug for Var {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}) {:?}", self.name(), self.desc(), self.var_type())
    }
}
struct Connection {
    file_mapping: HANDLE,
    shared_mem: *mut c_void,
    header: *mut IrsdkHeader,
    new_data: HANDLE,
}
impl Connection {
    // This will return an error if iRacing is not running.
    // However once you have a Connection it will remain usable even if iracing is exited and started again.
    unsafe fn new() -> Result<Self, Error> {
        let file_mapping =
            Memory::OpenFileMappingA(Memory::FILE_MAP_READ.0, false, "Local\\IRSDKMemMapFileName");
        if file_mapping.is_invalid() {
            return Err(Error::Win32(GetLastError()));
        }
        let shared_mem = Memory::MapViewOfFile(file_mapping, Memory::FILE_MAP_READ, 0, 0, 0);
        if shared_mem.is_null() {
            let e = Err(Error::Win32(GetLastError()));
            CloseHandle(file_mapping);
            return e;
        }
        let new_data = Threading::OpenEventA(
            windows::Win32::Storage::FileSystem::SYNCHRONIZE.0,
            false,
            "Local\\IRSDKDataValidEvent",
        );
        if new_data.is_invalid() {
            let e = Err(Error::Win32(GetLastError()));
            Memory::UnmapViewOfFile(shared_mem);
            CloseHandle(file_mapping);
            return e;
        }
        let header = shared_mem as *mut IrsdkHeader;
        Ok(Connection {
            file_mapping,
            shared_mem,
            header,
            new_data,
        })
    }
    unsafe fn connected(&self) -> bool {
        (*self.header).status.intersects(StatusField::CONNECTED)
    }
    unsafe fn variables(&self) -> &[IrsdkVarHeader] {
        let vhbase = self
            .shared_mem
            .add((*self.header).var_header_offset as usize)
            as *const IrsdkVarHeader;
        slice::from_raw_parts(vhbase, (*self.header).num_vars as usize)
    }
    unsafe fn buffers(&self) -> &[IrsdkBuf] {
        let l = (*self.header).num_buf as usize;
        assert!(l <= IRSDK_MAX_BUFS);
        &(*self.header).var_buf[..l]
    }
    // returns the telemetry buffer with the highest tick count, along with the actual data
    // this is the buffer in the shared mem, so you copy it.
    unsafe fn lastest_row(&self) -> (&IrsdkBuf, &[u8]) {
        let b = self.buffers();
        let mut latest = &b[0];
        for buff in b {
            if buff.tick_count > latest.tick_count {
                latest = buff;
            }
        }
        let buf_len = (*self.header).buf_len as usize;
        let src = self.shared_mem.add(latest.buf_offset as usize);
        return (latest, slice::from_raw_parts(src as *const u8, buf_len));
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.new_data);
            Memory::UnmapViewOfFile(self.shared_mem);
            windows::Win32::Foundation::CloseHandle(self.file_mapping);
        }
    }
}

pub struct Client {
    conn: Option<Connection>,
    last_tick_count: i32,
    session_id: i32, // incremented each time we detect a new session, means anything cached from header is invalid when this changes
    data: bytes::BytesMut,
}
impl Client {
    pub unsafe fn new() -> Client {
        Client {
            conn: Connection::new().ok(),
            last_tick_count: 0,
            session_id: 0,
            data: bytes::BytesMut::new(),
        }
    }
    // attempts to connect to iracing if we're not already. returns true if we're now connected (or was already connected), false otherwise
    unsafe fn connect(&mut self) -> bool {
        match &self.conn {
            Some(c) => c.connected(),
            None => match Connection::new() {
                Ok(c) => {
                    let result = c.connected();
                    self.conn = Some(c);
                    self.session_id += 1;
                    result
                }
                Err(_) => false,
            },
        }
    }
    pub unsafe fn connected(&self) -> bool {
        match &self.conn {
            Some(c) => c.connected(),
            None => false,
        }
    }
    pub unsafe fn wait_for_data(&mut self, wait: std::time::Duration) -> bool {
        if !self.get_new_data() {
            return false;
        }
        match &self.conn {
            Some(c) => {
                Threading::WaitForSingleObject(c.new_data, wait.as_millis().try_into().unwrap());
                self.get_new_data()
            }
            None => unreachable!("you shouldn't be able to get here"),
        }
    }
    pub unsafe fn get_new_data(&mut self) -> bool {
        if !self.connect() {
            self.last_tick_count = 0;
            self.session_id += 1;
            return false;
        }
        match &self.conn {
            None => {
                unreachable!("You shouldn't be able to get here");
            }
            Some(c) => {
                let (buf_hdr, row) = c.lastest_row();
                match buf_hdr.tick_count.cmp(&self.last_tick_count) {
                    Ordering::Greater => {
                        for _tries in 0..2 {
                            let curr_tick_count = buf_hdr.tick_count;
                            self.data.clear();
                            self.data.extend_from_slice(row);
                            if curr_tick_count == buf_hdr.tick_count {
                                self.last_tick_count = curr_tick_count;
                                return true;
                            }
                        }
                    }
                    Ordering::Less => {
                        // if ours is newer than the latest, then the session has reset
                        // any variables created from the previous session need
                        // recreating
                        self.session_id += 1;
                    }
                    Ordering::Equal => {}
                }
            }
        }
        false
    }
    pub unsafe fn dump_vars(&self) {
        match &self.conn {
            None => {}
            Some(c) => {
                for var_header in c.variables() {
                    let mut var = Var {
                        hdr: *var_header,
                        session_id: self.session_id,
                    };
                    let value = self.var_value(&mut var);
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
            }
        }
    }
    pub unsafe fn find_var(&self, name: &str) -> Option<Var> {
        self.conn.as_ref().and_then(|c| {
            for var_header in c.variables() {
                if var_header.has_name(name) {
                    return Some(Var {
                        hdr: *var_header,
                        session_id: self.session_id,
                    });
                }
            }
            None
        })
    }
    pub unsafe fn var_value(&self, var: &mut Var) -> Value {
        if var.session_id != self.session_id {
            // There's an annoying edge case where this variable doesn't exist in the new session
            // (think car specific variables)
            println!("session changed, re-finding var {}", var.name());
            *var = self.find_var(var.name()).unwrap();
        }
        let x = self.data.as_ptr().add(var.hdr.offset as usize);
        if var.hdr.count == 1 {
            match var.hdr.var_type {
                VarType::Char => Value::Char(*x),
                VarType::Bool => Value::Bool(*(x as *const bool)),
                VarType::Int => Value::Int(*(x as *const i32)),
                VarType::Bitfield => Value::Bitfield(*(x as *const i32)),
                VarType::Float => Value::Float(*(x as *const f32)),
                VarType::Double => Value::Double(*(x as *const f64)),
                _ => todo!(), // ETCount
            }
        } else {
            let l = var.count();
            match var.hdr.var_type {
                VarType::Char => Value::Chars(slice::from_raw_parts(x, l)),
                VarType::Bool => Value::Bools(slice::from_raw_parts(x as *const bool, l)),
                VarType::Int => Value::Ints(slice::from_raw_parts(x as *const i32, l)),
                VarType::Bitfield => Value::Bitfields(slice::from_raw_parts(x as *const i32, l)),
                VarType::Float => Value::Floats(slice::from_raw_parts(x as *const f32, l)),
                VarType::Double => Value::Doubles(slice::from_raw_parts(x as *const f64, l)),
                _ => todo!(), // ETCount
            }
        }
    }
    pub unsafe fn value<T: FromValue>(&self, var: &mut Var) -> Result<T, Error> {
        let v = self.var_value(var);
        T::var_result(&v)
    }
    pub unsafe fn session_info_update(&self) -> Option<i32> {
        self.conn.as_ref().map(|c| (*c.header).session_info_update)
    }
    pub unsafe fn session_info(&self) -> Result<String, std::borrow::Cow<str>> {
        match &self.conn {
            None => Ok("".into()),
            Some(c) => {
                let p = c.shared_mem.add((*c.header).session_info_offset as usize) as *mut u8;
                let mut bytes =
                    std::slice::from_raw_parts(p, (*c.header).session_info_len as usize);
                // session_info_len is the size of the buffer, not necessarily the size of the string
                // so we have to look for the null terminatior.
                for i in 0..bytes.len() {
                    if bytes[i] == 0 {
                        bytes = &bytes[0..i];
                        break;
                    }
                }
                WINDOWS_1252.decode(bytes, DecoderTrap::Replace)
            }
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
