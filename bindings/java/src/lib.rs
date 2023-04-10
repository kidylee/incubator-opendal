// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::str::FromStr;

use opendal::BlockingOperator;
use opendal::Scheme;


#[repr(C)]
#[allow(missing_copy_implementations)]
pub struct Stat(*const opendal::Metadata);


impl Drop for Stat {
    fn drop(&mut self) {
        println!("Dropping Stat");
    }
}
#[no_mangle]
#[allow(non_snake_case)]
pub extern fn dropStat(_: Box<Stat>) {

}

#[no_mangle]
#[allow(non_snake_case)]
pub extern fn stat(ptr: *mut BlockingOperator, fileName: *const c_char) -> Stat {
    let op = unsafe{&mut *ptr};
    let file_name = to_string(fileName);
    Stat(&op.stat(&file_name).unwrap())

}


#[no_mangle]
#[allow(non_snake_case)]
pub extern fn getOperator(scheme: *const c_char, params: *const *const c_char, size: c_int, result: *mut c_int) -> *const i32 {
    let map = string_array_to_hashmap(params, size);
    let scheme = to_string(scheme);
    match Scheme::from_str(&scheme) {
        Ok(scheme) => {
            match build_operator(scheme, map) {
                Ok(operator) => {
                    Box::into_raw(Box::new(operator)) as *const i32
                }
                Err(_) => {
                    unsafe {
                        *result = 1;
                    }
                    // return null box
                    std::ptr::null()
                }
            }
        }
        Err(_) => {
            unsafe {
                *result = 1;
            }
            // return null box
            std::ptr::null()
        }
    }
}

/// # Safety
///
/// This function should not be called before the Operator are ready.
#[no_mangle]
#[allow(non_snake_case)]
pub extern fn write(ptr: *mut BlockingOperator,
                    file_name: *const c_char, content: *const c_char) {
    let op = unsafe{&mut *ptr};
    let file_name = to_string(file_name);
    let content = to_string(content);
    op.write(&file_name, content).unwrap();
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern fn read(ptr: *mut BlockingOperator,
                   file_name: *const c_char) -> *const c_char {
    let op = unsafe{&mut *ptr};
    let file_name = to_string(file_name);
    op.read(&file_name)
        .map(|content| CString::new(content).unwrap())
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw()
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern fn delete(ptr: *mut BlockingOperator,
                   file_name: *const c_char) {
    let op = unsafe{&mut *ptr};
    let file_name = to_string(file_name);
    op.delete(&file_name).unwrap();
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern fn dropOperator(_: Box<BlockingOperator>) {

}


fn string_array_to_hashmap(strings: *const *const c_char, len: c_int) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();

    if strings.is_null() || len <= 0 {
        return map;
    }

    let len = len as usize;
    let strings = unsafe { std::slice::from_raw_parts(strings, len) };

    for i in (0..len).step_by(2) {
        if i + 1 < len {
            let key_cstr = unsafe { CStr::from_ptr(strings[i]) };
            let key = key_cstr.to_str().expect("Invalid UTF-8 string").to_owned();

            let value_cstr = unsafe { CStr::from_ptr(strings[i + 1]) };
            let value = value_cstr.to_str().expect("Invalid UTF-8 string").to_owned();

            map.insert(key, value);
        }
    }

    map
}



fn build_operator(
    scheme: opendal::Scheme,
    map: HashMap<String, String>,
) -> Result<opendal::Operator, opendal::Error> {
    use opendal::services::*;

    let op = match scheme {
        opendal::Scheme::Azblob => opendal::Operator::from_map::<Azblob>(map).unwrap().finish(),
        opendal::Scheme::Azdfs => opendal::Operator::from_map::<Azdfs>(map).unwrap().finish(),
        opendal::Scheme::Fs => opendal::Operator::from_map::<Fs>(map).unwrap().finish(),
        opendal::Scheme::Gcs => opendal::Operator::from_map::<Gcs>(map).unwrap().finish(),
        opendal::Scheme::Ghac => opendal::Operator::from_map::<Ghac>(map).unwrap().finish(),
        opendal::Scheme::Http => opendal::Operator::from_map::<Http>(map).unwrap().finish(),
        opendal::Scheme::Ipmfs => opendal::Operator::from_map::<Ipmfs>(map).unwrap().finish(),
        opendal::Scheme::Memory => opendal::Operator::from_map::<Memory>(map).unwrap().finish(),
        opendal::Scheme::Obs => opendal::Operator::from_map::<Obs>(map).unwrap().finish(),
        opendal::Scheme::Oss => opendal::Operator::from_map::<Oss>(map).unwrap().finish(),
        opendal::Scheme::S3 => opendal::Operator::from_map::<S3>(map).unwrap().finish(),
        opendal::Scheme::Webdav => opendal::Operator::from_map::<Webdav>(map).unwrap().finish(),
        opendal::Scheme::Webhdfs => opendal::Operator::from_map::<Webhdfs>(map)
            .unwrap()
            .finish(),

        _ => {
            return Err(opendal::Error::new(
                opendal::ErrorKind::Unexpected,
                "Scheme not supported",
            ));
        }
    };

    Ok(op)
}

fn to_string(pointer: *const c_char) -> String {
    let slice = unsafe { CStr::from_ptr(pointer).to_bytes() };
    std::str::from_utf8(slice).unwrap().to_string()
}