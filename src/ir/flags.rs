extern crate num;

use bitflags::bitflags;
use num::ToPrimitive;
use num_derive::{FromPrimitive, ToPrimitive};

bitflags! {
    pub struct StatusField:i32 {
        const CONNECTED = 1;
    }
}

// BITFIELD & ENUMs that can appear as telemetry values

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

#[derive(Clone, Copy, Debug, PartialEq, FromPrimitive)]
pub enum TrackLocation {
    NotInWorld = -1,
    OffTrack,
    InPitStall,
    ApproachingPits,
    OnTrack,
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

#[derive(Clone, Copy, Debug, PartialEq, FromPrimitive)]
pub enum PaceMode {
    SingleFileStart = 0,
    DoubleFileStart,
    SingleFileRestart,
    DoubleFileRestart,
    NotPacing,
}

bitflags! {
    pub struct PaceFlags:i32 {
        const END_OF_LINE   = 0x01;
        const FREE_PASS     = 0x02;
        const WAVED_AROUND  = 0x04;
    }
}

/// Enums for broadcast msg

//----
// Remote controll the sim by sending these windows messages
// camera and replay commands only work when you are out of your car,
// pit commands only work when in your car
#[derive(Debug)]
pub enum BroadcastMsg {
    CamSwitchPos,               // car position, group, camera
    CamSwitchNum,               // driver #, group, camera
    CamSetState,                // irsdk_CameraState, unused, unused
    ReplaySetPlaySpeed,         // speed, slowMotion, unused
    ReplaySetPlayPosition,      // irsdk_RpyPosMode, Frame Number (high, low)
    ReplaySearch,               // irsdk_RpySrchMode, unused, unused
    ReplaySetState,             // irsdk_RpyStateMode, unused, unused
    ReloadTextures,             // irsdk_ReloadTexturesMode, carIdx, unused
    ChatComand,                 // irsdk_ChatCommandMode, subCommand, unused
    PitCommand(PitCommandMode), // irsdk_PitCommandMode, parameter
    TelemCommand,               // irsdk_TelemCommandMode, unused, unused
    FFBCommand,                 // irsdk_FFBCommandMode, value (float, high, low)
    ReplaySearchSessionTime,    // sessionNum, sessionTimeMS (high, low)
    VideoCapture,               // irsdk_VideoCaptureMode, unused, unused
    Last,                       // unused placeholder
}
impl BroadcastMsg {
    pub fn msg(&self) -> (i16, (i16, isize)) {
        match self {
            BroadcastMsg::CamSwitchPos => todo!(),
            BroadcastMsg::CamSwitchNum => todo!(),
            BroadcastMsg::CamSetState => todo!(),
            BroadcastMsg::ReplaySetPlaySpeed => todo!(),
            BroadcastMsg::ReplaySetPlayPosition => todo!(),
            BroadcastMsg::ReplaySearch => todo!(),
            BroadcastMsg::ReplaySetState => todo!(),
            BroadcastMsg::ReloadTextures => todo!(),
            BroadcastMsg::ChatComand => todo!(),
            BroadcastMsg::PitCommand(c) => (9, c.msg()),
            BroadcastMsg::TelemCommand => todo!(),
            BroadcastMsg::FFBCommand => todo!(),
            BroadcastMsg::ReplaySearchSessionTime => todo!(),
            BroadcastMsg::VideoCapture => todo!(),
            BroadcastMsg::Last => todo!(),
        }
    }
}
// this only works when the driver is in the car
#[derive(Debug)]
pub enum PitCommandMode {
    Clear,      // Clear all pit checkboxes
    TearOff,    // WS: Clean the winshield, using one tear off
    Fuel(i16), // Add fuel, optionally specify the amount to add in liters or pass '0' to use existing amount
    LF(i16), // Change the left front tire, optionally specifying the pressure in KPa or pass '0' to use existing pressure
    RF(i16), // right front
    LR(i16), // left rear
    RR(i16), // right rear
    ClearTires, // Clear tire pit checkboxes
    FastRepair, // FR: Request a fast repair
    ClearWS, // Uncheck Clean the winshield checkbox
    ClearFR, // Uncheck request a fast repair
    ClearFuel, // Uncheck add fuel
}
impl PitCommandMode {
    pub fn msg(&self) -> (i16, isize) {
        match self {
            PitCommandMode::Clear => (0, 0),
            PitCommandMode::TearOff => (1, 0),
            PitCommandMode::Fuel(l) => (2, *l as isize),
            PitCommandMode::LF(p) => (3, *p as isize),
            PitCommandMode::RF(p) => (4, *p as isize),
            PitCommandMode::LR(p) => (5, *p as isize),
            PitCommandMode::RR(p) => (6, *p as isize),
            PitCommandMode::ClearTires => (7, 0),
            PitCommandMode::FastRepair => (8, 0),
            PitCommandMode::ClearWS => (9, 0),
            PitCommandMode::ClearFR => (10, 0),
            PitCommandMode::ClearFuel => (11, 0),
        }
    }
}
