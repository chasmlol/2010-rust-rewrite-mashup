# Compiled cue and media policies

`SoundCatalog::publish` selects an internal family cue compiler for each source
variant. The catalog owns the compiled policies for its revision. Explicit IW4/IW5/T5/T6
adapters select semantics; the common compiler uses the catalog-owned channel/group
context. Private sound handles bind actual aliases and media; forged handles,
foreign banks and metadata relabeling cannot authorize playback. Changing
aliases, channels or mixer groups requires publication again; existing audio
work retains its bank and checks scope/epoch and media bank revision.

`AliasPlaybackPolicy` exposes interpreted channel admission, limits, gain,
pitch, layers, routing and spatial requirements. Its constructors are internal.
Looping is `OneShot`, `Loop` or `UnknownOneShotCompatibility`; the last preserves
one-shot playback when capture supplied no looping fact. Cue decisions retain
that policy's name. The executor applies explicit source-lifetime and sound-class
host rules when starting a voice, without interpreting source flags.

A compiled spatial requirement is local playback, a curve, or a typed refusal.
Spatial requests consume that requirement; local requests do not claim spatial
support. Nonfinite authored curve data refuses at compilation. Listener position
and distance attenuation remain dynamic execution inputs. Unavailable channels,
spatial implementations, layers or speaker routes retain explicit refusal data;
invalid authored speaker gains are refused rather than discarded.

A policy is an execution contract with requirements, not a ready PCM buffer.
Composition and mixer failures are checked before requesting media. Spatial
requirements are checked when execution binds a world origin. Layer activation,
lifetime, independent pitch and failure rules remain part of the compiled cue;
media failure in a primary does not change its independent layer's contract.

Streamed media keys carry the compiler's decode policy alongside namespace and
archive path. The media service dispatches by that policy and the byte header,
not by the cue's family. Decode policy is part of key identity. The WMA-container
policy explicitly accepts RIFF/WAVE as compatibility input. Loaded media retains
its existing codec descriptors and binding provenance. Namespace is still the
source/archive identity; decoder availability and budgets do not become cue
semantics. PCM preparation, pinning and readiness remain the media service's job.
