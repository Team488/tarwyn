#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "tarwyn.h"

static int failures = 0;

#define CHECK(condition, what) check(!!(condition), what)

static void check(int condition, const char *what) {
  if (!condition) {
    (void)fprintf(stderr, "FAIL: %s\n", what);
    ++failures;
  }
}

static void on_update(void *ctx, const uint8_t *channel, size_t channel_len, const uint8_t *value,
                      size_t value_len) {
  (void)channel;
  (void)channel_len;
  (void)value;
  (void)value_len;
  ++*(int *)ctx;
}

static void on_drop(void *ctx) {
  *(int *)ctx = -1;
}

int main(void) {
  CHECK(tarwyn_abi_version() == TARWYN_ABI_VERSION, "the library speaks another ABI");

  const char *host = "127.0.0.1";
  TarwynClient *client =
      tarwyn_client_with_ports((const uint8_t *)host, strlen(host), 26483, 26484, 150, 500, 0, 0);
  const char *channel = "pose";
  const uint8_t *name = (const uint8_t *)channel;
  size_t name_len = strlen(channel);

  tarwyn_put_double(client, name, name_len, 1.5);
  tarwyn_put_pose2d(client, name, name_len, 1.5, -2.0, 0.25);

  double fields[3];
  CHECK(!tarwyn_get_pose2d(client, name, name_len, fields), "a pose was invented with no server");

  size_t len = 0;
  uint8_t *json = tarwyn_get_raw_json(client, (const uint8_t *)"", 0, &len);
  CHECK(json != NULL && len == 2 && memcmp(json, "{}", 2) == 0, "raw json was not {} with no server");
  tarwyn_bytes_free(json, len);

  uint8_t *absent = tarwyn_get_string(client, name, name_len, &len);
  CHECK(absent == NULL, "a string was invented with no server");
  tarwyn_bytes_free(absent, len);

  int calls = 0;
  CHECK(tarwyn_subscribe(client, name, name_len, on_update, &calls, on_drop), "the first subscribe should take");
  int refused = 0;
  CHECK(!tarwyn_subscribe(client, name, name_len, on_update, &refused, on_drop),
        "a second subscribe should report the first");
  CHECK(refused == -1, "a refused subscribe should release its context before returning");
  CHECK(tarwyn_unsubscribe(client, name, name_len), "the cancel handle should have been kept");
  CHECK(calls == -1, "an unsubscribe should release its context");

  tarwyn_client_free(client);
  tarwyn_client_free(NULL);

  const char *bogus = "no host here";
  CHECK(tarwyn_client_connect((const uint8_t *)bogus, strlen(bogus)) == NULL,
        "an unresolvable host has to come back as NULL, not abort");
  CHECK(tarwyn_default_predict_micros() > 0, "the library has a prediction default");

  if (failures == 0) {
    (void)puts("ok");
  }
  return failures == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
