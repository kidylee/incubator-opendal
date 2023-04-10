/*
 * Licensed to the Apache Software Foundation (ASF) under one
 * or more contributor license agreements.  See the NOTICE file
 * distributed with this work for additional information
 * regarding copyright ownership.  The ASF licenses this file
 * to you under the Apache License, Version 2.0 (the
 * "License"); you may not use this file except in compliance
 * with the License.  You may obtain a copy of the License at
 *
 *   http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */


package org.apache.opendal;

import com.sun.jna.Pointer;
import com.sun.jna.ptr.IntByReference;

import java.util.Map;


public class Operator {

    Pointer ptr;

    public Operator(String scheme, Map<String, String> params) {
        // convert params to a String[]
        String[] paramsArray = new String[params.size() * 2];
        int i = 0;
        for (Map.Entry<String, String> entry : params.entrySet()) {
            paramsArray[i++] = entry.getKey();
            paramsArray[i++] = entry.getValue();
        }
        IntByReference result = new IntByReference();
        ptr = Operators.INSTANCE.getOperator(scheme, paramsArray, params.size(), result);
        if(result.getValue() != 0) {
            throw new IllegalArgumentException("Failed to create operator.");
        }
    }
    public void write(String fileName, String content) {
        Operators.INSTANCE.write(ptr, fileName, content);
    }

    public String read(String s) {
        return Operators.INSTANCE.read(ptr, s);
    }

    public void delete(String s) {
        Operators.INSTANCE.delete(ptr, s);
    }

    @Override
    protected void finalize() throws Throwable {
        super.finalize();
        drop();
    }

    protected void drop() {
        Operators.INSTANCE.dropOperator(ptr);
    }
}
