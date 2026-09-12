package org.tarwyn;

import static org.junit.jupiter.api.Assertions.assertTrue;

import java.lang.reflect.Method;
import java.util.Arrays;
import java.util.Set;
import java.util.stream.Collectors;
import org.junit.jupiter.api.Test;

final class WrapperSurfaceTest {
    @Test
    void the_client_exposes_every_call_the_generated_one_does() {
        Set<String> generated =
            Arrays.stream(org.tarwyn.ffi.TarwynClientInterface.class.getMethods())
                .map(Method::getName)
                .collect(Collectors.toUnmodifiableSet());
        Set<String> exposed = Arrays.stream(TarwynClient.class.getMethods())
            .map(Method::getName)
            .collect(Collectors.toUnmodifiableSet());

        assertTrue(generated.size() >= 60, "the generated interface lost most of its methods");
        for (String name : generated) {
            assertTrue(
                exposed.contains(name),
                name + " is generated but the client does not expose it, so the wrapper "
                    + "drifted from the interface it delegates to");
        }
    }
}
