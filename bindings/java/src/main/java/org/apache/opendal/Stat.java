package org.apache.opendal;

import com.sun.jna.Structure;

import java.io.Closeable;
import java.io.IOException;

public class Stat extends Structure implements Closeable {
    @Override
    public void close() throws IOException {
        setAutoRead(false);
        Operators.INSTANCE.dropStat(this);

    }
}
