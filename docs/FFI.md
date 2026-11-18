# Native data contracts

Direct C calls use libc. On Unix with `perf/native`, `perfetto-sdk` also calls
the C ABI of a C++ Perfetto implementation. Other platform calls are behind
Rust libraries such as Bevy/wgpu/winit and the Windows allocator.

| Interface | Creates data | Owns data | Valid until | Frees memory | Null | Size / error | Types / calling convention |
|---|---|---|---|---|---|---|---|
| `getenv` | Rust static CStr key; C environment value | Rust key; C value | Key: process; value: environment change | C runtime; Rust only checks presence | Key: no; result: yes (absent) | CStr guarantees NUL termination; result checked | `c_char*`, C ABI |
| `malloc_trim` (Linux GNU) | No transferred buffer | Allocator heap | Call | Allocator releases free pages | No pointer | `size_t` pad, `int` status ignored (best effort) | `usize` / `c_int`, C ABI |
| `sched_getaffinity` | Rust zeroed `cpu_set_t` | Rust stack | Call | Rust stack | No | `sizeof(cpu_set_t)`; failure becomes `None` before mask use | libc target types, C ABI |
| `sched_setaffinity` | Rust populated `cpu_set_t` | Rust stack | Call | Rust stack | No | Same size; bootstrap checks status, load worker ignores it | libc target types, C ABI |
| `atexit` (perf, bench, capture) | Rust function pointer | Program code; state in Rust statics | Process exit | No transferred allocation | Callback: no | No payload/length; registration status currently ignored | `extern "C" fn()` → C `void (*)(void)`; `i32` → `int` on supported targets |
| Perfetto category / track | Rust static categories; SDK descriptor `Vec` | Rust statics / SDK track | Emission; tracks retained in `OnceLock` | Rust / SDK | Required names and descriptors: no | NUL names; descriptor pointer + `size_t`; registration results checked for tracks | SDK `repr(C)` structs, C ABI |
| Perfetto debug string | IW4L display string; SDK `CString`s | Rust / SDK `EventContext` | Synchronous emission returns | Rust drops SDK strings | Name/value: no | NUL terminated; interior NUL rendered as literal `\0` | SDK `const char*`, C ABI |
| Perfetto config / heap buffer | Rust protobuf writer; C heap buffer | Rust writer; SDK owns C handle; Rust output `Vec` | Writer outlives buffer; config borrowed during setup | SDK C destroy; Rust drops `Vec` | Required handles: no | Written size = output length; setup parses during call | SDK pointer + `size_t`, C ABI |
| Perfetto trace read | C chunk; Rust stack callback sink | C chunk; Rust mutex and copied bytes | Chunk: callback only; sink: blocking read returns | C chunk owner; Rust owns output Vec | Empty chunks return before slice construction; null with nonzero size is refused | Pointer + `size_t` + `bool`; IW4L copies nonempty chunks under the sink mutex | Direct SDK C ABI; blocking call waits for the final callback |
| Perfetto flush / session | C session; Rust RecordingSession | Rust NonNull handle owns C session | Until RecordingSession drop; process static retains it | C destroy on Rust drop | Create may return null, checked | Timeout checked to fit `uint32_t`; false flush status returns an error | Direct SDK C ABI and scalar types match |
| Global allocator adapter | System / Windows MiMalloc | Rust allocation consumer | Until deallocation or successful reallocation | Same backing allocator with matching `Layout` | Allocation can fail with null | Layout and new size forwarded unchanged | Rust `GlobalAlloc`; native ABI handled by backing library |

All dynamic debug strings in `crates/perf/src/event.rs` pass through `str_arg`.
Master-protocol map names accept length-bounded UTF-8, including interior NUL.
`observe_master_bridge` → `follow_master_match` → `request_zone` →
`perf::swap` can therefore supply such a string before any map file is opened.
The adapter renders NUL for diagnostics; the original gameplay value is unchanged.
It allocates a temporary display string only when NUL is present. The SDK
copies this string into its own `CString` before that temporary is dropped.
The C consumer serializes it before the SDK drops the event context.

Adapter display examples:

| Rust input | Perfetto display |
|---|---|
| `""`, `"mp_test"`, `"карта"` | unchanged |
| `"a\0b"` | `a\0b` with a visible backslash and digit zero |
| `"\0a\0"`, `"\0\0"` | every NUL rendered, including at the ends |
| `"a\\0b"` | unchanged; display escaping is not a reversible encoding |

The session adapter bypasses the SDK's Rust trace-read trampoline, which forms a
slice from null even for a zero-length chunk, and preserves the C flush status.
Perfetto permits session operations on application threads; the Rust session is
Send and its process owner serializes access through a mutex. Blocking methods
are never invoked from Perfetto service callbacks. Captured bytes survive an I/O
failure for retry, and a flush is marked complete only after both trace and
manifest are written. Capture errors remain errors on subsequent flush attempts.
The callback reserves space before copying each chunk; allocation or capacity
failure becomes a capture error. After the first refused chunk, later chunks
are ignored rather than appended to an incomplete trace.
