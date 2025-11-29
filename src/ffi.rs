//! FFI bindings for KeraDB
//! 
//! These functions are designed to be called from C/C++ code.
//! All pointer arguments are checked for null before use.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::panic;
use serde_json::Value;

use crate::Database;

// Opaque pointer types
#[repr(C)]
pub struct KeraDB {
    _private: [u8; 0],
}

// Error handling
thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn set_last_error(err: String) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = Some(err);
    });
}

#[no_mangle]
pub extern "C" fn keradb_last_error() -> *const c_char {
    LAST_ERROR.with(|e| {
        match e.borrow().as_ref() {
            Some(err) => {
                match CString::new(err.as_str()) {
                    Ok(s) => s.into_raw(),
                    Err(_) => ptr::null(),
                }
            }
            None => ptr::null(),
        }
    })
}

/// Free a string returned by KeraDB functions
/// 
/// # Safety
/// The pointer must be a valid pointer returned by a KeraDB function,
/// and must not have been freed before.
#[no_mangle]
pub unsafe extern "C" fn keradb_free_string(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

// Database operations
#[no_mangle]
pub extern "C" fn keradb_create(path: *const c_char) -> *mut KeraDB {
    let result = panic::catch_unwind(|| {
        if path.is_null() {
            set_last_error("Path cannot be null".to_string());
            return ptr::null_mut();
        }

        let path_str = unsafe {
            match CStr::from_ptr(path).to_str() {
                Ok(s) => s,
                Err(e) => {
                    set_last_error(format!("Invalid UTF-8 in path: {}", e));
                    return ptr::null_mut();
                }
            }
        };

        match Database::create(path_str) {
            Ok(db) => Box::into_raw(Box::new(db)) as *mut KeraDB,
            Err(e) => {
                set_last_error(format!("Failed to create database: {}", e));
                ptr::null_mut()
            }
        }
    });

    result.unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn keradb_open(path: *const c_char) -> *mut KeraDB {
    let result = panic::catch_unwind(|| {
        if path.is_null() {
            set_last_error("Path cannot be null".to_string());
            return ptr::null_mut();
        }

        let path_str = unsafe {
            match CStr::from_ptr(path).to_str() {
                Ok(s) => s,
                Err(e) => {
                    set_last_error(format!("Invalid UTF-8 in path: {}", e));
                    return ptr::null_mut();
                }
            }
        };

        match Database::open(path_str) {
            Ok(db) => Box::into_raw(Box::new(db)) as *mut KeraDB,
            Err(e) => {
                set_last_error(format!("Failed to open database: {}", e));
                ptr::null_mut()
            }
        }
    });

    result.unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn keradb_close(db: *mut KeraDB) {
    if !db.is_null() {
        let _ = panic::catch_unwind(|| unsafe {
            let _ = Box::from_raw(db as *mut Database);
        });
    }
}

#[no_mangle]
pub extern "C" fn keradb_insert(
    db: *mut KeraDB,
    collection: *const c_char,
    json_data: *const c_char,
) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        if db.is_null() || collection.is_null() || json_data.is_null() {
            set_last_error("Arguments cannot be null".to_string());
            return ptr::null_mut();
        }

        let db = unsafe { &*(db as *const Database) };
        
        let collection_str = unsafe {
            match CStr::from_ptr(collection).to_str() {
                Ok(s) => s,
                Err(e) => {
                    set_last_error(format!("Invalid UTF-8 in collection: {}", e));
                    return ptr::null_mut();
                }
            }
        };

        let json_str = unsafe {
            match CStr::from_ptr(json_data).to_str() {
                Ok(s) => s,
                Err(e) => {
                    set_last_error(format!("Invalid UTF-8 in JSON: {}", e));
                    return ptr::null_mut();
                }
            }
        };

        let data: Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                set_last_error(format!("Invalid JSON: {}", e));
                return ptr::null_mut();
            }
        };

        match db.insert(collection_str, data) {
            Ok(id) => match CString::new(id) {
                Ok(s) => s.into_raw(),
                Err(_) => {
                    set_last_error("Failed to create ID string".to_string());
                    ptr::null_mut()
                }
            },
            Err(e) => {
                set_last_error(format!("Insert failed: {}", e));
                ptr::null_mut()
            }
        }
    });

    result.unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn keradb_find_by_id(
    db: *mut KeraDB,
    collection: *const c_char,
    doc_id: *const c_char,
) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        if db.is_null() || collection.is_null() || doc_id.is_null() {
            set_last_error("Arguments cannot be null".to_string());
            return ptr::null_mut();
        }

        let db = unsafe { &*(db as *const Database) };
        
        let collection_str = unsafe {
            match CStr::from_ptr(collection).to_str() {
                Ok(s) => s,
                Err(e) => {
                    set_last_error(format!("Invalid UTF-8 in collection: {}", e));
                    return ptr::null_mut();
                }
            }
        };

        let id_str = unsafe {
            match CStr::from_ptr(doc_id).to_str() {
                Ok(s) => s,
                Err(e) => {
                    set_last_error(format!("Invalid UTF-8 in ID: {}", e));
                    return ptr::null_mut();
                }
            }
        };

        match db.find_by_id(collection_str, id_str) {
            Ok(doc) => {
                let json = serde_json::to_string(&doc).unwrap();
                match CString::new(json) {
                    Ok(s) => s.into_raw(),
                    Err(_) => {
                        set_last_error("Failed to create JSON string".to_string());
                        ptr::null_mut()
                    }
                }
            }
            Err(e) => {
                set_last_error(format!("Find failed: {}", e));
                ptr::null_mut()
            }
        }
    });

    result.unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn keradb_update(
    db: *mut KeraDB,
    collection: *const c_char,
    doc_id: *const c_char,
    json_data: *const c_char,
) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        if db.is_null() || collection.is_null() || doc_id.is_null() || json_data.is_null() {
            set_last_error("Arguments cannot be null".to_string());
            return ptr::null_mut();
        }

        let db = unsafe { &*(db as *const Database) };
        
        let collection_str = unsafe { CStr::from_ptr(collection).to_str().unwrap() };
        let id_str = unsafe { CStr::from_ptr(doc_id).to_str().unwrap() };
        let json_str = unsafe { CStr::from_ptr(json_data).to_str().unwrap() };

        let data: Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                set_last_error(format!("Invalid JSON: {}", e));
                return ptr::null_mut();
            }
        };

        match db.update(collection_str, id_str, data) {
            Ok(doc) => {
                let json = serde_json::to_string(&doc).unwrap();
                match CString::new(json) {
                    Ok(s) => s.into_raw(),
                    Err(_) => ptr::null_mut()
                }
            }
            Err(e) => {
                set_last_error(format!("Update failed: {}", e));
                ptr::null_mut()
            }
        }
    });

    result.unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn keradb_delete(
    db: *mut KeraDB,
    collection: *const c_char,
    doc_id: *const c_char,
) -> c_int {
    let result = panic::catch_unwind(|| {
        if db.is_null() || collection.is_null() || doc_id.is_null() {
            set_last_error("Arguments cannot be null".to_string());
            return 0;
        }

        let db = unsafe { &*(db as *const Database) };
        
        let collection_str = unsafe { CStr::from_ptr(collection).to_str().unwrap() };
        let id_str = unsafe { CStr::from_ptr(doc_id).to_str().unwrap() };

        match db.delete(collection_str, id_str) {
            Ok(_) => 1,
            Err(e) => {
                set_last_error(format!("Delete failed: {}", e));
                0
            }
        }
    });

    result.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn keradb_find_all(
    db: *mut KeraDB,
    collection: *const c_char,
    limit: c_int,
    skip: c_int,
) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        if db.is_null() || collection.is_null() {
            set_last_error("Arguments cannot be null".to_string());
            return ptr::null_mut();
        }

        let db = unsafe { &*(db as *const Database) };
        let collection_str = unsafe { CStr::from_ptr(collection).to_str().unwrap() };

        let limit_opt = if limit < 0 { None } else { Some(limit as usize) };
        let skip_opt = if skip < 0 { None } else { Some(skip as usize) };

        match db.find_all(collection_str, limit_opt, skip_opt) {
            Ok(docs) => {
                let json = serde_json::to_string(&docs).unwrap();
                match CString::new(json) {
                    Ok(s) => s.into_raw(),
                    Err(_) => ptr::null_mut()
                }
            }
            Err(e) => {
                set_last_error(format!("Find all failed: {}", e));
                ptr::null_mut()
            }
        }
    });

    result.unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn keradb_count(
    db: *mut KeraDB,
    collection: *const c_char,
) -> c_int {
    let result = panic::catch_unwind(|| {
        if db.is_null() || collection.is_null() {
            return -1;
        }

        let db = unsafe { &*(db as *const Database) };
        let collection_str = unsafe { CStr::from_ptr(collection).to_str().unwrap() };

        db.count(collection_str) as c_int
    });

    result.unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn keradb_list_collections(db: *mut KeraDB) -> *mut c_char {
    let result = panic::catch_unwind(|| {
        if db.is_null() {
            set_last_error("Database pointer cannot be null".to_string());
            return ptr::null_mut();
        }

        let db = unsafe { &*(db as *const Database) };
        let collections = db.list_collections();
        
        let json = serde_json::to_string(&collections).unwrap();
        match CString::new(json) {
            Ok(s) => s.into_raw(),
            Err(_) => ptr::null_mut()
        }
    });

    result.unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn keradb_sync(db: *mut KeraDB) -> c_int {
    let result = panic::catch_unwind(|| {
        if db.is_null() {
            return 0;
        }

        let db = unsafe { &*(db as *const Database) };
        match db.sync() {
            Ok(_) => 1,
            Err(e) => {
                set_last_error(format!("Sync failed: {}", e));
                0
            }
        }
    });

    result.unwrap_or(0)
}
