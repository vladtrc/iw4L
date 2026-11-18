# Transport budgets

Protocol 147 adds the authoritative ToggleNoclip debug action (tag 29).
Its movement mode travels in the existing playerstate pm_type field; authority
and prediction both run the same flight movement.

Protocol 146 added script-selected player DObjs to the DObj metadata section.
Each row carries a real client slot, reserved entity generation, life sequence,
and optional composed model state. An absent row uses the ordinary player kit;
a row with no model explicitly clears the body. Rows must be sorted, unique and
within the reserved player slots. Consumers must match generation and life to the
current player. DObj semantic poses also carry optional player controller inputs
(pitch, crouch, prone and lean), which must be finite. Authority and client must
both use protocol 147.

Per-viewer entity visibility was introduced in protocol 145. The entities
section carries a sorted list of `(u16 entity number, u64 hidden-viewer mask)`
after the EQ bypass list. Masks must be nonzero and numbers unique and below 2048.
This affects drawing for the addressed viewer, preserving authoritative payloads
and collision. Client and authority must use matching protocol versions.

The projectile owner layout introduced in protocol 144 remains unchanged. The owner
tag is one byte: `0` is a player (`u32` client and `u32` life sequence), `1` is a
script entity (`u32` model identity and `i32` entity number), and `2` is the world.
The payload uses little endian integers. Entity numbers must precede the reserved
world entity number. Unknown tags and incomplete owners are rejected.
Client and authority must use the same protocol version; older projectile
snapshots cannot be decoded with this layout. AI projectiles retain their entity
identity without requiring a player slot.

Both client and relay validate these environment settings before connecting or accepting work.
Durations are monotonic milliseconds, limited to 1–600000.

| Setting | Default | Owner/outcome |
| --- | ---: | --- |
| `IW4L_NET_RESPONSE_MS` | 20000 | Correlated request; timeout has unknown remote outcome |
| `IW4L_NET_PROGRESS_MS` | 5000 | Useful dedicated-stream progress/acknowledgement |
| `IW4L_NET_TRANSFER_MS` | 90000 | Total dedicated transfer occupation |
| `IW4L_NET_QUEUE_AGE_MS` | 20000 | Queued command/control/bootstrap/reliable data |
| `IW4L_NET_CONNECT_MS` | 60000 | Connection attempt including retries |
| `IW4L_NET_SHUTDOWN_MS` | 500 | Shutdown drain |
| `IW4L_NET_LOADING_MS` | 120000 | Authority loading milestone |
| `IW4L_NET_SILENCE_MS` | 60000 | Authority progress silence |
| `IW4L_NET_ADMISSION_MS` | 60000 | Peer admission milestone |
| `IW4L_NET_QUEUE_ITEMS` | 32 | Control/pending requests; range 1–1024 |
| `IW4L_NET_QUEUE_BYTES` | 8388608 | Purpose queue byte budget; range 262144–67108864 |

Progress must not exceed the total transfer budget. Invalid settings refuse transport startup.
Keepalive and unrelated traffic never renew a request deadline.
Shared reliable framing belongs to the connection reader; dedicated streams stop/reset on failure.
Replaceable gameplay slots retain their oldest unsent age across replacement.
Reliable queues refuse overload and fail the connection visibly on expiry.
Bootstrap streams also retain the protocol payload and concurrency limits.
Both endpoints use Quinn’s default congestion controller (CUBIC).
The application never disables its congestion window after a stall.

Accepted control requests reserve bounded storage for a correlated terminal receipt until the bridge consumes it.
Connection closure resolves every remaining waiter with an explicit unknown remote outcome.
Local commands can expire before connection establishment; those commands have not been sent.
