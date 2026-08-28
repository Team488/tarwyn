/// Covers what the C++ client promises when no server is listening, and the ring
/// contract that neither language can check alone.
///
/// A coprocessor starts before the server it talks to, so every path here runs on
/// a real robot. None of them may block, throw, or invent a value.
///
/// Built with the vendored boost/ut.hpp, so the whole suite is one g++ line.

#include <boost/ut.hpp>

#include <chrono>
#include <cstdint>
#include <cstring>
#include <string>
#include <thread>
#include <vector>

#include <tarwyn.hpp>

namespace {

constexpr std::size_t kRecords = 8;
constexpr std::size_t kRecordBytes = 64;

/// Points at ports nothing is listening on, with a short timeout so a read that
/// will never be answered fails quickly.
tarwyn::Client Offline() {
    tarwyn::Config config;
    config.host = "127.0.0.1";
    config.push_port = 47971;
    config.req_port = 47972;
    config.sub_port = 47973;
    config.request_timeout = std::chrono::milliseconds(50);
    return tarwyn::Client(config);
}

std::uint64_t AsUint64(const std::vector<std::uint8_t>& payload) {
    std::uint64_t value = 0;
    std::memcpy(&value, payload.data(), sizeof(value));
    return value;
}

void Push(const tarwyn::Client& client, std::uint64_t id, const void* data, std::size_t length) {
    const int code =
        xt_ring_push(client.raw(), id, static_cast<const std::uint8_t*>(data), length);
    boost::ut::expect(code == XT_OK) << "ring push failed";
}

}  // namespace

int main() {
    using namespace boost::ut;
    using namespace std::chrono;

    "construction does not wait for a server"_test = [] {
        const auto started = steady_clock::now();
        auto client = Offline();
        const auto elapsed = duration_cast<milliseconds>(steady_clock::now() - started).count();
        expect(elapsed < 2000_i) << "construction blocked; ZeroMQ should dial in the background";
    };

    "publishing into the void neither blocks nor throws"_test = [] {
        auto client = Offline();
        const auto started = steady_clock::now();
        for (int index = 0; index < 200; ++index) {
            client.PutDouble("nobody-is-listening", index);
        }
        const auto elapsed = duration_cast<milliseconds>(steady_clock::now() - started).count();
        expect(elapsed < 2000_i) << "publishing blocked; it should drop rather than queue";
    };

    // One case per read, so a regression names the reader that broke rather than
    // reporting that something in a list of thirteen did.
    "GetString reports absence"_test = [] {
        expect(not Offline().GetString("absent").has_value());
    };
    "GetInteger reports absence"_test = [] {
        expect(not Offline().GetInteger("absent").has_value());
    };
    "GetLong reports absence"_test = [] {
        expect(not Offline().GetLong("absent").has_value());
    };
    "GetDouble reports absence"_test = [] {
        expect(not Offline().GetDouble("absent").has_value());
    };
    "GetFloat reports absence"_test = [] {
        expect(not Offline().GetFloat("absent").has_value());
    };
    "GetBoolean reports absence"_test = [] {
        expect(not Offline().GetBoolean("absent").has_value());
    };
    "GetBytes reports absence"_test = [] {
        expect(not Offline().GetBytes("absent").has_value());
    };
    "GetStringList reports absence"_test = [] {
        expect(not Offline().GetStringList("absent").has_value());
    };
    "GetDoubleList reports absence"_test = [] {
        expect(not Offline().GetDoubleList("absent").has_value());
    };
    "GetBooleanList reports absence"_test = [] {
        expect(not Offline().GetBooleanList("absent").has_value());
    };
    "GetCoordinates reports absence"_test = [] {
        expect(not Offline().GetCoordinates("absent").has_value());
    };
    "GetPose2d reports absence"_test = [] {
        expect(not Offline().GetPose2d("absent").has_value());
    };
    "GetPose3d reports absence"_test = [] {
        expect(not Offline().GetPose3d("absent").has_value());
    };

    "the control plane reports absence too"_test = [] {
        auto client = Offline();
        expect(not client.Ping().has_value());
        expect(not client.Statistics().has_value());
        expect(client.RawJson() == std::string("{}"));
        expect(client.Tables().empty());
        expect(client.DeleteAll() == 0_u);
    };

    "an unrecognised tag is kept as raw bytes"_test = [] {
        expect(Offline().PutTypedBytes("typed", 999, {1}))
            << "an unrecognised tag should be stored as raw bytes, as TARWYN does";
    };

    // A recognised tag with the wrong number of bytes is not that type.
    for (const auto& [tag, length] :
         std::vector<std::pair<int, std::size_t>>{{2, 3}, {3, 1}, {5, 2}}) {
        test("tag " + std::to_string(tag) + " rejects " + std::to_string(length) + " bytes") =
            [tag = tag, length = length] {
                expect(not Offline().PutTypedBytes("typed", tag,
                                                   std::vector<std::uint8_t>(length, 1)));
            };
    }

    "a well formed typed payload is accepted"_test = [] {
        const std::vector<std::uint8_t> one = {0x3f, 0xf0, 0, 0, 0, 0, 0, 0};
        expect(Offline().PutTypedBytes("typed", 2, one));
    };

    "a client is movable and closes once"_test = [] {
        auto first = Offline();
        first.PutDouble("moved", 1.0);
        auto second = std::move(first);
        second.PutDouble("moved", 2.0);
        expect(second.LoggingHealthy()) << "logging should read healthy before it is started";
    };

    "an error carries its code"_test = [] {
        expect(throws<tarwyn::TarwynError>(
            [] { throw tarwyn::TarwynError("Probe", XT_ERR_NO_VALUE); }));
        try {
            throw tarwyn::TarwynError("Probe", XT_ERR_NO_VALUE);
        } catch (const tarwyn::TarwynError& error) {
            expect(error.code() == XT_ERR_NO_VALUE);
            expect(std::string(error.what()).find("Probe") != std::string::npos);
        }
    };

    "a record crosses the boundary byte for byte"_test = [] {
        auto client = Offline();
        auto ring = client.Subscribe("layout", kRecords, kRecordBytes);

        std::vector<std::uint8_t> written(kRecordBytes - 8);
        for (std::size_t index = 0; index < written.size(); ++index) {
            written[index] = static_cast<std::uint8_t>(index * 7 + 1);
        }
        Push(client, ring.id(), written.data(), written.size());

        const auto drained = ring.Drain();
        expect(drained.size() == 1_ul) << "one record in, one record out";
        expect(not drained.empty() and drained[0] == written)
            << "C++ read back bytes Rust did not write";
    };

    "a lapped ring keeps only the newest"_test = [] {
        auto client = Offline();
        auto ring = client.Subscribe("lap", kRecords, kRecordBytes);

        const std::uint64_t total = kRecords * 3;
        for (std::uint64_t value = 0; value < total; ++value) {
            Push(client, ring.id(), &value, sizeof(value));
        }
        expect(ring.Lapped()) << "the writer lapped the reader but Lapped() denied it";

        const auto drained = ring.Drain();
        expect(drained.size() == kRecords) << "a lapped ring returned more than it holds";
        for (std::size_t index = 0; index < drained.size(); ++index) {
            expect(AsUint64(drained[index]) == total - kRecords + index)
                << "a lapped ring returned something other than the newest records";
        }
    };

    "concurrent writes are never torn or reordered"_test = [] {
        auto client = Offline();
        auto ring = client.Subscribe("soak", kRecords, kRecordBytes);

        const std::uint64_t total = 50000;
        std::thread writer([&] {
            for (std::uint64_t value = 0; value < total; ++value) {
                xt_ring_push(client.raw(), ring.id(),
                             reinterpret_cast<const std::uint8_t*>(&value), sizeof(value));
            }
        });

        std::vector<std::uint64_t> seen;
        while (ring.WriteIndex() < total) {
            for (const auto& payload : ring.Drain()) {
                seen.push_back(AsUint64(payload));
            }
        }
        writer.join();
        for (const auto& payload : ring.Drain()) {
            seen.push_back(AsUint64(payload));
        }

        expect(not seen.empty()) << "nothing was read, so nothing was tested";

        bool increasing = true;
        bool in_range = true;
        for (std::size_t index = 0; index < seen.size(); ++index) {
            if (index > 0 && seen[index] <= seen[index - 1]) {
                increasing = false;
            }
            if (seen[index] >= total) {
                in_range = false;
            }
        }
        expect(increasing) << "the ring handed back values out of order or twice";
        expect(in_range) << "the ring handed back a value the writer never wrote";
    };
}
