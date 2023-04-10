package org.apache.opendal;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.NativeLibrary;
import com.sun.jna.Pointer;
import com.sun.jna.ptr.IntByReference;

public interface Operators extends Library{
    String JNA_LIBRARY_NAME = "opendal_java";
    NativeLibrary JNA_NATIVE_LIB = NativeLibrary.getInstance(JNA_LIBRARY_NAME);
    Operators INSTANCE = Native.load(JNA_LIBRARY_NAME, Operators.class);

    public Pointer getOperator(String scheme, String[] params, int size, IntByReference result);

    void write(Pointer ptr, String fileName, String content);
    String read(Pointer ptr, String fileName);

    void delete(Pointer ptr, String fileName);

    void dropOperator(Pointer ptr);

    void dropStat(Stat stat);
}
