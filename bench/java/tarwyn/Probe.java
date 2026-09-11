package tarwyn;

/**
 * One side of one case, over one transport.
 *
 * A probe moves bytes and stamps clocks. It computes no statistics: the
 * samples it collects are reduced to a row by {@code bench row}, through the
 * same histogram every other harness in this benchmark uses, so a row measured
 * here and a row measured through the Rust client are the same arithmetic over
 * different transports.
 */
public interface Probe {
    void publish(String host, int payload, long rateHz, long count) throws Exception;

    void subscribe(String host, int payload, int samples) throws Exception;
}
