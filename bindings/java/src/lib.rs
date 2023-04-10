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

use jni::objects::JClass;
use jni::objects::JObject;
use jni::objects::JString;
use jni::sys::{jboolean, jlong};
use jni::JNIEnv;

use opendal::services;
use opendal::BlockingOperator;
use opendal::Operator;

#[no_mangle]
pub extern "system" fn Java_org_apache_opendal_Operator_getOperator(
    mut env: JNIEnv,
    _class: JClass,
    scheme: JString,
    _params: JObject,
) -> jlong {
    let scheme: String = env
        .get_string(&scheme)
        .expect("Couldn't get java string!")
        .into();
    if scheme != "Memory" {
        env.exception_clear().unwrap();
        env.throw_new("java/lang/UnsupportedOperationException",
                       "Memory is not supported yet").unwrap();
        return 0 as jlong;
    }

    let builder = services::Memory::default();
    let op = Operator::new(builder).unwrap().finish().blocking();
    Box::into_raw(Box::new(op)) as jlong
}

/// # Safety
///
/// This function should not be called before the Operator are ready.
#[no_mangle]
pub unsafe extern "system" fn Java_org_apache_opendal_Operator_freeOperator(
    mut _env: JNIEnv,
    _class: JClass,
    ptr: *mut Operator,
) {
    // Take ownership of the pointer by wrapping it with a Box
    let _ = Box::from_raw(ptr);
}

/// # Safety
///
/// This function should not be called before the Operator are ready.
#[no_mangle]
pub unsafe extern "system" fn Java_org_apache_opendal_Operator_write(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut BlockingOperator,
    file: JString,
    content: JString,
) {
    let op = &mut *ptr;
    let file: String = env
        .get_string(&file)
        .expect("Couldn't get java string!")
        .into();
    let content: String = env
        .get_string(&content)
        .expect("Couldn't get java string!")
        .into();
    op.write(&file, content).unwrap();
}

/// # Safety
///
/// This function should not be called before the Operator are ready.
#[no_mangle]
pub unsafe extern "system" fn Java_org_apache_opendal_Operator_read<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: *mut BlockingOperator,
    file: JString<'local>,
) -> JString<'local> {
    let op = &mut *ptr;
    let file: String = env
        .get_string(&file)
        .expect("Couldn't get java string!")
        .into();
    let content = String::from_utf8(op.read(&file).unwrap()).expect("Couldn't convert to string");

    let output = env
        .new_string(content)
        .expect("Couldn't create java string!");
    output
}

/// # Safety
///
/// This function should not be called before the Operator are ready.
#[no_mangle]
pub unsafe extern "system" fn Java_org_apache_opendal_Operator_stat(
    mut env: JNIEnv,
    _class: JClass,
    ptr: *mut BlockingOperator,
    file: JString,
) -> jlong {
    let op = &mut *ptr;
    let file: String = env
        .get_string(&file)
        .expect("Couldn't get java string!")
        .into();
    let metadata = op.stat(&file).unwrap();
    Box::into_raw(Box::new(metadata)) as jlong
}

/// # Safety
///
/// This function should not be called before the Stat are ready.
#[no_mangle]
pub unsafe extern "system" fn Java_org_apache_opendal_Metadata_isFile(
    mut _env: JNIEnv,
    _class: JClass,
    ptr: *mut opendal::Metadata,
) -> jboolean {
    let metadata = &mut *ptr;
    metadata.is_file() as jboolean
}

/// # Safety
///
/// This function should not be called before the Stat are ready.
#[no_mangle]
pub unsafe extern "system" fn Java_org_apache_opendal_Metadata_getContentLength(
    mut _env: JNIEnv,
    _class: JClass,
    ptr: *mut opendal::Metadata,
) -> jlong {
    let metadata = &mut *ptr;
    metadata.content_length() as jlong
}

/// # Safety
///
/// This function should not be called before the Stat are ready.
#[no_mangle]
pub unsafe extern "system" fn Java_org_apache_opendal_Metadata_freeMetadata(
    mut _env: JNIEnv,
    _class: JClass,
    ptr: *mut opendal::Metadata,
) {
    // Take ownership of the pointer by wrapping it with a Box
    let _ = Box::from_raw(ptr);
}

/// # Safety
///
/// This function should not be called before the Operator are ready.
#[no_mangle]
pub unsafe extern "system" fn Java_org_apache_opendal_Operator_delete<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: *mut BlockingOperator,
    file: JString<'local>,
) {
    let op = &mut *ptr;
    let file: String = env
        .get_string(&file)
        .expect("Couldn't get java string!")
        .into();
    op.delete(&file).unwrap();
}
