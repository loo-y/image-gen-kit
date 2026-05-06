use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::os::raw::{c_char, c_int, c_uchar, c_void};
use std::path::Path;
use std::ptr;

#[repr(C)]
struct Sqlite3 {
    _private: [u8; 0],
}

#[repr(C)]
struct Sqlite3Stmt {
    _private: [u8; 0],
}

type ExecCallback =
    Option<unsafe extern "C" fn(*mut c_void, c_int, *mut *mut c_char, *mut *mut c_char) -> c_int>;

#[link(name = "sqlite3")]
extern "C" {
    fn sqlite3_open_v2(
        filename: *const c_char,
        pp_db: *mut *mut Sqlite3,
        flags: c_int,
        z_vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_close(db: *mut Sqlite3) -> c_int;
    fn sqlite3_errmsg(db: *mut Sqlite3) -> *const c_char;
    fn sqlite3_exec(
        db: *mut Sqlite3,
        sql: *const c_char,
        callback: ExecCallback,
        arg: *mut c_void,
        errmsg: *mut *mut c_char,
    ) -> c_int;
    fn sqlite3_free(ptr: *mut c_void);
    fn sqlite3_prepare_v2(
        db: *mut Sqlite3,
        sql: *const c_char,
        n_byte: c_int,
        pp_stmt: *mut *mut Sqlite3Stmt,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_step(stmt: *mut Sqlite3Stmt) -> c_int;
    fn sqlite3_finalize(stmt: *mut Sqlite3Stmt) -> c_int;
    fn sqlite3_bind_text(
        stmt: *mut Sqlite3Stmt,
        index: c_int,
        value: *const c_char,
        n: c_int,
        destructor: unsafe extern "C" fn(*mut c_void),
    ) -> c_int;
    fn sqlite3_bind_int64(stmt: *mut Sqlite3Stmt, index: c_int, value: i64) -> c_int;
    fn sqlite3_bind_null(stmt: *mut Sqlite3Stmt, index: c_int) -> c_int;
    fn sqlite3_column_text(stmt: *mut Sqlite3Stmt, i_col: c_int) -> *const c_uchar;
    fn sqlite3_column_bytes(stmt: *mut Sqlite3Stmt, i_col: c_int) -> c_int;
    fn sqlite3_column_int64(stmt: *mut Sqlite3Stmt, i_col: c_int) -> i64;
}

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_DONE: c_int = 101;
const SQLITE_OPEN_READWRITE: c_int = 0x00000002;
const SQLITE_OPEN_CREATE: c_int = 0x00000004;
const SQLITE_OPEN_FULLMUTEX: c_int = 0x00010000;

pub struct Connection {
    raw: *mut Sqlite3,
}

pub struct Statement<'a> {
    raw: *mut Sqlite3Stmt,
    connection: PhantomData<&'a Connection>,
}

impl Connection {
    pub fn open(path: &Path) -> Result<Self, String> {
        let path_string = path.to_string_lossy().to_string();
        let c_path = CString::new(path_string).map_err(|err| err.to_string())?;
        let mut raw = ptr::null_mut();
        let rc = unsafe {
            sqlite3_open_v2(
                c_path.as_ptr(),
                &mut raw,
                SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX,
                ptr::null(),
            )
        };
        if rc != SQLITE_OK {
            let message = if raw.is_null() {
                "Unable to open SQLite database".to_string()
            } else {
                error_message(raw)
            };
            if !raw.is_null() {
                unsafe {
                    sqlite3_close(raw);
                }
            }
            return Err(message);
        }
        Ok(Self { raw })
    }

    pub fn execute(&self, sql: &str) -> Result<(), String> {
        let c_sql = CString::new(sql).map_err(|err| err.to_string())?;
        let mut err_msg: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            sqlite3_exec(
                self.raw,
                c_sql.as_ptr(),
                None,
                ptr::null_mut(),
                &mut err_msg,
            )
        };
        if rc != SQLITE_OK {
            if err_msg.is_null() {
                return Err(error_message(self.raw));
            }
            let message = unsafe { CStr::from_ptr(err_msg).to_string_lossy().to_string() };
            unsafe {
                sqlite3_free(err_msg.cast());
            }
            return Err(message);
        }
        Ok(())
    }

    pub fn prepare(&self, sql: &str) -> Result<Statement<'_>, String> {
        let c_sql = CString::new(sql).map_err(|err| err.to_string())?;
        let mut raw = ptr::null_mut();
        let rc =
            unsafe { sqlite3_prepare_v2(self.raw, c_sql.as_ptr(), -1, &mut raw, ptr::null_mut()) };
        if rc != SQLITE_OK {
            return Err(error_message(self.raw));
        }
        Ok(Statement {
            raw,
            connection: PhantomData,
        })
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                sqlite3_close(self.raw);
            }
        }
    }
}

impl Statement<'_> {
    pub fn bind_text(&mut self, index: c_int, value: &str) -> Result<(), String> {
        let c_value = CString::new(value).map_err(|_| "Text contains a null byte".to_string())?;
        let rc = unsafe {
            sqlite3_bind_text(
                self.raw,
                index,
                c_value.as_ptr(),
                value.len() as c_int,
                sqlite_transient(),
            )
        };
        check_bind(rc)
    }

    pub fn bind_optional_text(&mut self, index: c_int, value: Option<&str>) -> Result<(), String> {
        match value {
            Some(value) => self.bind_text(index, value),
            None => self.bind_null(index),
        }
    }

    pub fn bind_i64(&mut self, index: c_int, value: i64) -> Result<(), String> {
        let rc = unsafe { sqlite3_bind_int64(self.raw, index, value) };
        check_bind(rc)
    }

    pub fn bind_null(&mut self, index: c_int) -> Result<(), String> {
        let rc = unsafe { sqlite3_bind_null(self.raw, index) };
        check_bind(rc)
    }

    pub fn step(&mut self) -> Result<bool, String> {
        match unsafe { sqlite3_step(self.raw) } {
            SQLITE_ROW => Ok(true),
            SQLITE_DONE => Ok(false),
            rc => Err(format!("SQLite step failed with code {rc}")),
        }
    }

    pub fn column_text(&self, index: c_int) -> Option<String> {
        let ptr = unsafe { sqlite3_column_text(self.raw, index) };
        if ptr.is_null() {
            return None;
        }
        let len = unsafe { sqlite3_column_bytes(self.raw, index) };
        if len <= 0 {
            return Some(String::new());
        }
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
        Some(String::from_utf8_lossy(bytes).to_string())
    }

    pub fn column_i64(&self, index: c_int) -> i64 {
        unsafe { sqlite3_column_int64(self.raw, index) }
    }
}

impl Drop for Statement<'_> {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                sqlite3_finalize(self.raw);
            }
        }
    }
}

fn check_bind(rc: c_int) -> Result<(), String> {
    if rc == SQLITE_OK {
        Ok(())
    } else {
        Err(format!("SQLite bind failed with code {rc}"))
    }
}

fn error_message(db: *mut Sqlite3) -> String {
    unsafe {
        CStr::from_ptr(sqlite3_errmsg(db))
            .to_string_lossy()
            .to_string()
    }
}

fn sqlite_transient() -> unsafe extern "C" fn(*mut c_void) {
    unsafe { std::mem::transmute::<isize, unsafe extern "C" fn(*mut c_void)>(-1) }
}
