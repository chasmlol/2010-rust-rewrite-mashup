# Client audio

Bevy publishes cue intentions, listener snapshots and desired source scenes.
`AudioRuntime` control resolves cues and media; `render_core` mixes prepared
PCM into CPAL, null or offline output. The callback reads fixed slots and
atomic gains; retirement threads free payloads and join media workers.

The internal rate is 48 kHz, the quantum 128 stereo frames and the physical
pool 128 slots. Control caps logical instances at 2048, processes 64 intakes
and 64 pending steps per pass, virtualizes saturated loops and rejects excess
one-shots. Cancellation and epoch invalidation bypass queues. Weapon attacks
preserve their first 20 ms during replacement; explicit Stop cancels them.

Family compilers publish per-variant looping, spatial, voice, gain, pitch, layer
and media policies in their catalog context. Bound sound handles and clip keys
retain its revision; foreign owners and media/source mismatches refuse before playback.
T5 layers activate at resolution; others wait for primary preparation. Layers share
deadlines, cancellation and lifetime, with independent pitch/failure. Unknown looping
uses named one-shot compatibility. [CUES.md](CUES.md) details binding and decode contracts.
Killcam worlds retain the live sound registry for round-result commands.

Weapon publication compiles thermal scopes, cue namespaces, melee precedence,
knife substitution and breath aliases. Prediction identifies fire occurrences;
authority verdicts suppress refused shots and duplicate effects. Entity and
animation identities stay distinct. `EventJournal` deduplicates 8192 identities
within 100 ticks; local life invalidates animation cues.

Media caps 4096 keys and 256 queued jobs, including at most 192 prewarm jobs.
Urgent requests promote queued work; two to four workers publish clips as
completed, with one reserved for urgent work. PCM pins at most 256 MiB;
decode scratch reserves 64 MiB. Geometry and partial frames are validated.
Native T5 WMA2 emits budget-owned cached s16 chunks for mono 44.1 kHz and
stereo 48 kHz profiles. T6 capture retains SAB locators; workers reserve output,
input and FLAC scratch before reading and validate channel/rate/frame metadata.
Descriptor-only SAB entries bypass cross-bank caching. T6 world spatial policy,
IW5 pointer-based speaker maps, cross-channel routes and multichannel output
remain unsupported; WMA seek/tail semantics need corpus validation.

Desired loops use scope/epoch/object/slot versions and retain virtual cursors.
Control ranks eight map voices with a 0.002 gain floor. Device recovery uses
bounded backoff; one-shots preserve PCM during short outages until their start
deadlines expire. Master volume is applied once. `OfflineRenderer` shares the
mixing kernel. Streaming, DSP buses/tails and acoustic propagation are unfinished.

`IW4L_AUDIO_DIAG=1` enables cue identities/decisions, request/preparation timings,
slow control stages, affinity and device/null frame counts. Set
`IW4L_AUDIO_DIAG_PATH` for a buffered file; the bounded queue counts dropped
records. `dump` and `clip` include readiness and recent decisions. Device frames
measure mixing into callbacks, before device latency or hardware playback.
