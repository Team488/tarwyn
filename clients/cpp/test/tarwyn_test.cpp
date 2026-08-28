/// Covers what the C++ client promises when no server is listening, and the ring
/// contract that neither language can check alone.
///
/// A coprocessor starts before the server it talks to, so every path here runs on
/// a real robot. None of them may block, throw, or invent a value.

#include <tarwyn.hpp>

#include <chrono>
#include <cstdio>
#include <cstring>
#include <string>
#include <thread>
#include <vector>

namespace {

int failures = 0;
int checks = 0;

void Check(bool condition, const char* what, int line) {
    ++checks;
    if (!condition) {
        ++failures;
        std::printf("  FAIL line %d: %s\n", line, what);
    }
}

#define CHECK(condition) Check((condition), #condition, __LINE__)

constexpr std::size_t kRecords = 8;
constexpr std::size_t kRecordBytes = 64;

tarwyn::Client OfflineClient() {
    tarwyn::Config config;
    config.host = "127.0.0.1";
    config.push_port = 47971;
    config.req_port = 47972;
    config.sub_port = 47973;
    config.request_timeout = std::chrono::milliseconds(50);
    return tarwyn::Client(config);
}

void ConstructionDoesNotWaitForAServer() {
    const auto started = std::chrono::steady_clock::now();
    auto client = OfflineClient();
    const auto elapsed = std::chrono::steady_clock::now() - started;
    CHECK(std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count() < 2000);
}

void PublishingIntoTheVoidNeitherBlocksNorThrows() {
    auto client = OfflineClient();
    const auto started = std::chrono::steady_clock::now();
    for (int index = 0; index < 200; ++index) {
        client.PutDouble("nobody-is-listening", index);
    }
    const auto elapsed = std::chrono::steady_clock::now() - started;
    CHECK(std::chrono::duration_cast<std::chrono::milliseconds>(elapsed).count() < 2000);
}

void ReadsReportAbsenceRatherThanInventingAValue() {
    auto client = OfflineClient();
    CHECK(!client.GetString("absent").has_value());
    CHECK(!client.GetDouble("absent").has_value());
    CHECK(!client.GetInteger("absent").has_value());
    CHECK(!client.GetLong("absent").has_value());
    CHECK(!client.GetFloat("absent").has_value());
    CHECK(!client.GetBoolean("absent").has_value());
    CHECK(!client.GetBytes("absent").has_value());
    CHECK(!client.GetDoubleList("absent").has_value());
    CHECK(!client.GetStringList("absent").has_value());
    CHECK(!client.GetBooleanList("absent").has_value());
    CHECK(!client.GetCoordinates("absent").has_value());
    CHECK(!client.GetPose2d("absent").has_value());
    CHECK(!client.GetPose3d("absent").has_value());
    CHECK(!client.Ping().has_value());
    CHECK(!client.Statistics().has_value());
    CHECK(client.RawJson() == "{}");
    CHECK(client.Tables().empty());
    CHECK(client.DeleteAll() == 0);
}

void ATypedPayloadIsValidatedBeforeItIsPublished() {
    auto client = OfflineClient();
    CHECK(client.PutTypedBytes("typed", 999, {1}));
    CHECK(!client.PutTypedBytes("typed", 2, {1, 2, 3}));
    CHECK(!client.PutTypedBytes("typed", 3, {1}));
    const std::vector<std::uint8_t> one = {0x3f, 0xf0, 0, 0, 0, 0, 0, 0};
    CHECK(client.PutTypedBytes("typed", 2, one));
}

void AClientIsMovableAndClosesOnce() {
    auto first = OfflineClient();
    first.PutDouble("moved", 1.0);
    auto second = std::move(first);
    second.PutDouble("moved", 2.0);
    CHECK(second.DroppedPublishes() >= 0);
}

void AnErrorCarriesItsCode() {
    try {
        throw tarwyn::TarwynError("Probe", XT_ERR_NO_VALUE);
    } catch (const tarwyn::TarwynError& error) {
        CHECK(error.code() == XT_ERR_NO_VALUE);
        CHECK(std::strstr(error.what(), "Probe") != nullptr);
        return;
    }
    CHECK(false);
}

void ARecordCrossesTheBoundaryByteForByte() {
    auto client = OfflineClient();
    auto ring = client.Subscribe("layout", kRecords, kRecordBytes);

    std::vector<std::uint8_t> written(kRecordBytes - 8);
    for (std::size_t index = 0; index < written.size(); ++index) {
        written[index] = static_cast<std::uint8_t>(index * 7 + 1);
    }
    CHECK(xt_ring_push(client.raw(), ring.id(), written.data(),
                       written.size()) == XT_OK);

    const auto drained = ring.Drain();
    CHECK(drained.size() == 1);
    CHECK(!drained.empty() && drained[0] == written);
}

void ALappedRingKeepsOnlyTheNewest() {
    auto client = OfflineClient();
    auto ring = client.Subscribe("lap", kRecords, kRecordBytes);

    const std::uint64_t total = kRecords * 3;
    for (std::uint64_t value = 0; value < total; ++value) {
        CHECK(xt_ring_push(client.raw(), ring.id(),
                           reinterpret_cast<const std::uint8_t*>(&value), sizeof(value)) == XT_OK);
    }
    CHECK(ring.Lapped());

    const auto drained = ring.Drain();
    CHECK(drained.size() == kRecords);
    for (std::size_t index = 0; index < drained.size(); ++index) {
        std::uint64_t value = 0;
        std::memcpy(&value, drained[index].data(), sizeof(value));
        CHECK(value == total - kRecords + index);
    }
}

void ConcurrentWritesAreNeverTornOrReordered() {
    auto client = OfflineClient();
    auto ring = client.Subscribe("soak", kRecords, kRecordBytes);

    const std::uint64_t total = 50000;
    std::thread writer([&] {
        for (std::uint64_t value = 0; value < total; ++value) {
            xt_ring_push(client.raw(), ring.id(),
                         reinterpret_cast<const std::uint8_t*>(&value), sizeof(value));
        }
    });

    std::vector<std::uint64_t> seen;
    bool running = true;
    while (running) {
        if (!writer.joinable()) {
            running = false;
        }
        for (const auto& payload : ring.Drain()) {
            if (payload.size() != sizeof(std::uint64_t)) {
                CHECK(false);
                continue;
            }
            std::uint64_t value = 0;
            std::memcpy(&value, payload.data(), sizeof(value));
            seen.push_back(value);
        }
        if (seen.size() >= total || !running) {
            break;
        }
        if (ring.WriteIndex() >= total) {
            break;
        }
    }
    writer.join();
    for (const auto& payload : ring.Drain()) {
        std::uint64_t value = 0;
        std::memcpy(&value, payload.data(), sizeof(value));
        seen.push_back(value);
    }

    CHECK(!seen.empty());
    bool increasing = true;
    for (std::size_t index = 1; index < seen.size(); ++index) {
        if (seen[index] <= seen[index - 1]) {
            increasing = false;
        }
    }
    CHECK(increasing);
    bool in_range = true;
    for (const auto value : seen) {
        if (value >= total) {
            in_range = false;
        }
    }
    CHECK(in_range);
}

struct Case {
    const char* name;
    void (*run)();
};

}  // namespace

int main() {
    const Case cases[] = {
        {"construction does not wait for a server", ConstructionDoesNotWaitForAServer},
        {"publishing into the void neither blocks nor throws",
         PublishingIntoTheVoidNeitherBlocksNorThrows},
        {"reads report absence rather than inventing a value",
         ReadsReportAbsenceRatherThanInventingAValue},
        {"a typed payload is validated before it is published",
         ATypedPayloadIsValidatedBeforeItIsPublished},
        {"a client is movable and closes once", AClientIsMovableAndClosesOnce},
        {"an error carries its code", AnErrorCarriesItsCode},
        {"a record crosses the boundary byte for byte", ARecordCrossesTheBoundaryByteForByte},
        {"a lapped ring keeps only the newest", ALappedRingKeepsOnlyTheNewest},
        {"concurrent writes are never torn or reordered", ConcurrentWritesAreNeverTornOrReordered},
    };

    for (const auto& entry : cases) {
        const int before = failures;
        entry.run();
        std::printf("%s %s\n", failures == before ? "PASS" : "FAIL", entry.name);
    }

    std::printf("\n%d checks, %d failed\n", checks, failures);
    return failures == 0 ? 0 : 1;
}
