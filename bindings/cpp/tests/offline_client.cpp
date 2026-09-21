#include <cstdlib>
#include <memory>
#include <print>
#include <string>
#include <vector>

#include "tarwyn.hpp"

namespace {

int failures = 0;

void Check(bool condition, const char* what) {
  if (!condition) {
    std::println(stderr, "FAIL: {}", what);
    ++failures;
  }
}

tarwyn::Client Offline() {
  return tarwyn::Client::WithPorts("127.0.0.1", 26583, 26584, 150, 500);
}

void a_read_reports_absence_rather_than_inventing_a_value() {
  auto client = Offline();
  Check(!client.GetString("absent"), "GetString invented a value");
  Check(!client.GetInteger("absent"), "GetInteger invented a value");
  Check(!client.GetDouble("absent"), "GetDouble invented a value");
  Check(!client.GetBytes("absent"), "GetBytes invented a value");
  Check(!client.GetStringList("absent"), "GetStringList invented a value");
  Check(!client.GetDoubleList("absent"), "GetDoubleList invented a value");
  Check(!client.GetBooleanList("absent"), "GetBooleanList invented a value");
  Check(!client.GetCoordinates("absent"), "GetCoordinates invented a value");
  Check(!client.GetPose2d("absent"), "GetPose2d invented a value");
  Check(!client.GetPose3d("absent"), "GetPose3d invented a value");
  Check(!client.GetBezierCurve("absent"), "GetBezierCurve invented a value");
  Check(!client.GetPing(), "GetPing invented a value");
  Check(!client.GetServerStatistics(), "GetServerStatistics invented a value");
}

void publishing_into_the_void_neither_blocks_nor_throws() {
  auto client = Offline();
  client.PutDouble("pose", 1.5);
  client.PutString("mode", "auto");
  std::vector<uint8_t> frame{1, 2, 3};
  client.PutBytes("frame", frame);
  std::vector<double> wheels{1.0, 2.0};
  client.PutDoubleList("wheels", wheels);
  std::vector<std::string> names{"a", "b"};
  client.PutStringList("names", names);
  bool flags[]{true, false};
  client.PutBooleanList("flags", flags);
  std::vector<uint8_t> raw{4, 5};
  client.PublishTelemetry("fast", raw);
  client.PutPose2d("pose", wpi::math::Pose2d(wpi::units::meter_t{1.5}, wpi::units::meter_t{-2.0},
                                             wpi::math::Rotation2d(wpi::units::radian_t{0.25})));
  client.PutPose3d("pose3", wpi::math::Pose3d(wpi::units::meter_t{1.0}, wpi::units::meter_t{2.0},
                                              wpi::units::meter_t{3.0},
                                              wpi::math::Rotation3d(
                                                  wpi::math::Quaternion(0.5, 0.5, 0.5, 0.5))));
  std::vector<wpi::math::Translation2d> path{
      wpi::math::Translation2d(wpi::units::meter_t{1.0}, wpi::units::meter_t{2.0})};
  client.PutCoordinates("path", path);
  std::vector<tarwyn::Point> curve{{1.0, 2.0, std::nullopt}, {3.0, 4.0, 90.0}};
  client.PutBezierCurve("curve", curve);
}

void a_listing_is_empty_rather_than_absent_when_the_server_is() {
  auto client = Offline();
  Check(client.GetTables("").empty(), "GetTables invented a table");
  Check(client.GetRawJson("") == "{}", "GetRawJson invented a value");
  Check(client.Delete("absent") == 0, "Delete claimed a removal");
  Check(client.DeleteAll() == 0, "DeleteAll claimed a removal");
}

void a_compare_and_set_fails_rather_than_claiming_it_swapped() {
  auto client = Offline();
  Check(!client.CompareAndSetAbsentString("lock", "agent-a"), "CAS absent string claimed a swap");
  Check(!client.CompareAndSetDouble("counter", 1.0, 2.0), "CAS double claimed a swap");
  Check(!client.CompareAndSetLong("counter", 1, 2), "CAS long claimed a swap");
  Check(!client.CompareAndSetBoolean("flag", false, true), "CAS boolean claimed a swap");
}

void logging_reports_healthy_before_it_is_started() {
  auto client = Offline();
  Check(client.LoggingHealthy(), "logging unhealthy before start");
  Check(client.DroppedLogRecords() == 0, "dropped log records before start");
}

void a_typed_put_rejects_bytes_that_are_not_that_type() {
  auto client = Offline();
  std::vector<uint8_t> bytes{1, 2, 3};
  Check(!client.PutTypedBytes("pose", 2, bytes), "a typed put accepted the wrong bytes");
  Check(client.PutTypedBytes("pose", 9999, bytes), "an unknown type was refused");
}

void cancelling_a_subscription_stops_it_rather_than_leaking_it() {
  auto client = Offline();
  auto discard = [](std::string_view, std::span<const uint8_t>) {};
  Check(client.Subscribe("pose", discard), "the first subscribe should take");
  Check(!client.Subscribe("pose", discard), "a second subscribe should report the first");
  Check(client.Unsubscribe("pose"), "the cancel handle should have been kept");
  Check(!client.Unsubscribe("pose"), "cancelling twice should report the first");
  Check(client.Subscribe("pose", discard), "cancelling frees the channel again");
  Check(client.SubscribeToLogs(discard), "the first log subscribe should take");
  Check(!client.SubscribeToLogs(discard), "a second log subscribe should report the first");
  Check(client.UnsubscribeFromLogs(), "the log cancel handle should have been kept");
}

void a_client_releases_its_callbacks_when_it_goes() {
  int released = 0;
  struct Counter {
    int* released;
    ~Counter() { ++*released; }
  };
  {
    auto client = Offline();
    auto shared = std::make_shared<Counter>(&released);
    client.Subscribe("pose", [shared](std::string_view, std::span<const uint8_t>) {});
  }
  Check(released == 1, "a freed client kept its callback alive");
}

}  // namespace

int main() {
  a_read_reports_absence_rather_than_inventing_a_value();
  publishing_into_the_void_neither_blocks_nor_throws();
  a_listing_is_empty_rather_than_absent_when_the_server_is();
  a_compare_and_set_fails_rather_than_claiming_it_swapped();
  logging_reports_healthy_before_it_is_started();
  a_typed_put_rejects_bytes_that_are_not_that_type();
  cancelling_a_subscription_stops_it_rather_than_leaking_it();
  a_client_releases_its_callbacks_when_it_goes();
  if (failures == 0) {
    std::println("ok");
  }
  return failures == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
