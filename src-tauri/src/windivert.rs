//! Minimal, dynamically-loaded bindings for WinDivert 2.2.
//!
//! The DLL is loaded at runtime with `libloading` so no import library is
//! needed and the driver can live next to the executable (dev builds) or in
//! the install directory (bundled builds).

use std::ffi::{c_char, c_void, CString};
use std::io;
use std::path::PathBuf;
use std::sync::OnceLock;

pub type Handle = *mut c_void;
pub const INVALID_HANDLE: Handle = usize::MAX as Handle;

pub const LAYER_NETWORK: u32 = 0;
pub const LAYER_NETWORK_FORWARD: u32 = 1;
pub const LAYER_FLOW: u32 = 2;
pub const LAYER_SOCKET: u32 = 3;

pub const EVENT_FLOW_ESTABLISHED: u8 = 1;
pub const EVENT_FLOW_DELETED: u8 = 2;
pub const EVENT_SOCKET_BIND: u8 = 3;
pub const EVENT_SOCKET_CONNECT: u8 = 4;
pub const EVENT_SOCKET_LISTEN: u8 = 5;
pub const EVENT_SOCKET_ACCEPT: u8 = 6;
pub const EVENT_SOCKET_CLOSE: u8 = 7;

pub const FLAG_SNIFF: u64 = 0x0001;
pub const FLAG_RECV_ONLY: u64 = 0x0004;

pub const PARAM_QUEUE_LENGTH: u32 = 0;
pub const PARAM_QUEUE_TIME: u32 = 1;
pub const PARAM_QUEUE_SIZE: u32 = 2;

pub const SHUTDOWN_BOTH: u32 = 3;

/// Maximum packet size WinDivert can hand us (IPv6 header + 64 KiB payload).
pub const MTU_MAX: usize = 40 + 0xFFFF;

/// Mirrors `WINDIVERT_ADDRESS` (80 bytes). The bit-field word is kept raw and
/// decoded through accessors; the union is a raw 64 byte blob.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Address {
    pub timestamp: i64,
    pub flags: u32,
    pub reserved2: u32,
    pub data: [u8; 64],
}

impl Default for Address {
    fn default() -> Self {
        Self { timestamp: 0, flags: 0, reserved2: 0, data: [0; 64] }
    }
}

/// Decoded `WINDIVERT_DATA_FLOW` / `WINDIVERT_DATA_SOCKET` (same layout).
#[derive(Clone, Copy, Debug)]
pub struct FlowData {
    pub process_id: u32,
    /// Addresses in WinDivert's internal (host) representation, IPv4 mapped
    /// as ::ffff:a.b.c.d. Use [`WinDivert::hton_ipv6`] to get wire bytes.
    pub local_addr: [u32; 4],
    pub remote_addr: [u32; 4],
    pub local_port: u16,
    pub remote_port: u16,
    pub protocol: u8,
}

impl Address {
    #[inline] pub fn event(&self) -> u8 { ((self.flags >> 8) & 0xff) as u8 }
    #[inline] pub fn outbound(&self) -> bool { self.flags & (1 << 17) != 0 }

    pub fn flow(&self) -> FlowData {
        let d = &self.data;
        let u32_at = |o: usize| u32::from_ne_bytes(d[o..o + 4].try_into().unwrap());
        let u16_at = |o: usize| u16::from_ne_bytes(d[o..o + 2].try_into().unwrap());
        FlowData {
            process_id: u32_at(16),
            local_addr: [u32_at(20), u32_at(24), u32_at(28), u32_at(32)],
            remote_addr: [u32_at(36), u32_at(40), u32_at(44), u32_at(48)],
            local_port: u16_at(52),
            remote_port: u16_at(54),
            protocol: d[56],
        }
    }
}

type FnOpen = unsafe extern "C" fn(*const c_char, u32, i16, u64) -> Handle;
type FnRecv = unsafe extern "C" fn(Handle, *mut c_void, u32, *mut u32, *mut Address) -> i32;
type FnSend = unsafe extern "C" fn(Handle, *const c_void, u32, *mut u32, *const Address) -> i32;
type FnShutdown = unsafe extern "C" fn(Handle, u32) -> i32;
type FnClose = unsafe extern "C" fn(Handle) -> i32;
type FnSetParam = unsafe extern "C" fn(Handle, u32, u64) -> i32;
type FnHton = unsafe extern "C" fn(*const u32, *mut u32);

pub struct WinDivert {
    _lib: libloading::Library,
    open: FnOpen,
    recv: FnRecv,
    send: FnSend,
    shutdown: FnShutdown,
    close: FnClose,
    set_param: FnSetParam,
    hton_ipv6: FnHton,
    pub path: PathBuf,
}

unsafe impl Send for WinDivert {}
unsafe impl Sync for WinDivert {}

static INSTANCE: OnceLock<Result<WinDivert, String>> = OnceLock::new();

impl WinDivert {
    /// Loads (once) `WinDivert.dll`, searching next to the executable, then in
    /// `<exe>/windivert`, then `<exe>/resources`, then the system search path.
    pub fn get() -> Result<&'static WinDivert, String> {
        INSTANCE.get_or_init(Self::load).as_ref().map_err(|e| e.clone())
    }

    fn candidates() -> Vec<PathBuf> {
        let mut v = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                v.push(dir.join("WinDivert.dll"));
                v.push(dir.join("windivert").join("WinDivert.dll"));
                v.push(dir.join("resources").join("WinDivert.dll"));
            }
        }
        v.push(PathBuf::from("WinDivert.dll"));
        v
    }

    fn load() -> Result<WinDivert, String> {
        let mut last_err = String::from("WinDivert.dll not found");
        for path in Self::candidates() {
            if path.is_absolute() && !path.exists() {
                continue;
            }
            match unsafe { libloading::Library::new(&path) } {
                Ok(lib) => unsafe {
                    macro_rules! sym {
                        ($name:literal, $t:ty) => {
                            *lib.get::<$t>(concat!($name, "\0").as_bytes())
                                .map_err(|e| format!("{}: {e}", $name))?
                        };
                    }
                    let open = sym!("WinDivertOpen", FnOpen);
                    let recv = sym!("WinDivertRecv", FnRecv);
                    let send = sym!("WinDivertSend", FnSend);
                    let shutdown = sym!("WinDivertShutdown", FnShutdown);
                    let close = sym!("WinDivertClose", FnClose);
                    let set_param = sym!("WinDivertSetParam", FnSetParam);
                    let hton_ipv6 = sym!("WinDivertHelperHtonIpv6Address", FnHton);
                    return Ok(WinDivert {
                        _lib: lib, open, recv, send, shutdown, close, set_param, hton_ipv6, path,
                    });
                },
                Err(e) => last_err = format!("{}: {e}", path.display()),
            }
        }
        Err(last_err)
    }

    pub fn open(&self, filter: &str, layer: u32, priority: i16, flags: u64) -> io::Result<Handle> {
        let f = CString::new(filter).unwrap();
        let h = unsafe { (self.open)(f.as_ptr(), layer, priority, flags) };
        if h == INVALID_HANDLE || h.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(h)
        }
    }

    /// Blocks until a packet/event arrives. Returns the packet length.
    pub fn recv(&self, h: Handle, buf: &mut [u8], addr: &mut Address) -> io::Result<usize> {
        let mut len: u32 = 0;
        let ok = unsafe {
            (self.recv)(h, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut len, addr)
        };
        if ok != 0 { Ok(len as usize) } else { Err(io::Error::last_os_error()) }
    }

    pub fn send(&self, h: Handle, pkt: &[u8], addr: &Address) -> io::Result<()> {
        let mut sent: u32 = 0;
        let ok = unsafe { (self.send)(h, pkt.as_ptr() as *const c_void, pkt.len() as u32, &mut sent, addr) };
        if ok != 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
    }

    pub fn set_param(&self, h: Handle, param: u32, value: u64) -> io::Result<()> {
        let ok = unsafe { (self.set_param)(h, param, value) };
        if ok != 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
    }

    pub fn shutdown(&self, h: Handle) {
        unsafe { (self.shutdown)(h, SHUTDOWN_BOTH) };
    }

    pub fn close(&self, h: Handle) {
        unsafe { (self.close)(h) };
    }

    /// Converts a FLOW/SOCKET-layer address into the 16 wire-order bytes as
    /// they appear inside an IPv6 header (IPv4 shows up as ::ffff:a.b.c.d).
    pub fn hton_ipv6(&self, addr: &[u32; 4]) -> [u8; 16] {
        let mut out = [0u32; 4];
        unsafe { (self.hton_ipv6)(addr.as_ptr(), out.as_mut_ptr()) };
        let mut bytes = [0u8; 16];
        for (i, w) in out.iter().enumerate() {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&w.to_ne_bytes());
        }
        bytes
    }
}

/// Human readable explanation for the common `WinDivertOpen` failures.
pub fn explain_open_error(e: &io::Error) -> String {
    match e.raw_os_error() {
        Some(2) => "No se encontró WinDivert64.sys junto a WinDivert.dll".into(),
        Some(5) => "Acceso denegado: la aplicación debe ejecutarse como administrador".into(),
        Some(87) => "Filtro de WinDivert inválido".into(),
        Some(577) => "Windows rechazó la firma del driver WinDivert64.sys".into(),
        Some(654) => "Versión incompatible del driver WinDivert (¿otra app usa una versión distinta?)".into(),
        Some(1275) => "El driver fue bloqueado por el sistema (política de drivers)".into(),
        Some(1753) => "El servicio Base Filtering Engine (BFE) no está en ejecución".into(),
        _ => format!("Error {}: {}", e.raw_os_error().unwrap_or(0), e),
    }
}
