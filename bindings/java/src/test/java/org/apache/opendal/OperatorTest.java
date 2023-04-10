package org.apache.opendal;



//import static org.junit.jupiter.api.Assertions.*;

import org.junit.jupiter.api.Test;

import java.util.HashMap;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;

public class OperatorTest {

    @Test
    void getOperator() {
        Map<String, String> params = new HashMap<>();
        params.put("root", "./tmp");
        Operator op = new Operator("Memory", params);

    }

    @Test
    void testReadAndWrite() {
        Map<String, String> params = new HashMap<>();
        params.put("root", "./tmp");
        Operator op = new Operator("Memory", params);
        op.write("test", "hello world");
        String content = op.read("test");
        assertEquals(content, "hello world");
        op.delete("test");



    }


}