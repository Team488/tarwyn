# Security

tarwyn is built for a robot's private network. Nothing is authenticated or
encrypted: anyone who reaches port 5810 can read, write and delete every
channel, and anyone who reaches UDP 5809 can use telemetry. Run it on a network
you control, or bind it to `127.0.0.1`.

Per-peer caps (connections, topics, publishers, subscriptions, 1 MiB messages,
nesting depth) keep a broken client from taking the server down. They do not
keep a hostile one out.

Report a vulnerability as a GitHub issue, or privately to the maintainers.
