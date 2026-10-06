# Material preparation contracts

`asset_material::compile_material_catalog` translates captured definitions into a runtime catalog through private family compilers. It owns argument translation, shader/declaration references, pass color and shadow policy, compiled state rows, material sort order and source selection provenance. Catalog compilation is separate from shader-port admission and dynamic binding; a missing shader/declaration remains explicit metadata for admission to refuse. A prepared table contains reusable local banks, sampler bindings, admitted ports and compiled state. It does not by itself guarantee that a draw can execute: the requested technique and vertex layout must have an admitted port, and every dynamic code source must bind successfully.

`PreparedMaterialTable::from_catalog` binds products to the catalog's `MaterialGenerationId`. Material execution, prepared technique queries and stable-shell capture reject a different catalog generation. A catalog replacement requires a rebuilt table. Shader-port identity still includes programs, declaration, arguments, sampler policy and color space. `RuntimeMaterialBuild::publish` mints the generation inside render_material. Published fields are private; `parts()` borrows immutable data. Exact catalog clones retain identity; publishing cloned or edited build parts always assigns a new identity. The generation constructor is private. `TessMaterials::new` checks the catalog/table pair before installation and exposes read-only owners.

Family catalog compilation calls `compile_material_state` with two authored state words and stores `CompiledPassState` rows. Shared table preparation indexes these products; FPV reads their prepared cull mode. `compile_material_state` remains the low-level state compiler for capture/preparation owners, including HUD installation. `CompiledPassState` owns decoded blend, alpha-test, cull, color-write, sRGB-write, fill, depth, stencil and offset rules, plus authored-field and unsupported-field diagnostics. Its fields and raw constructors are private. Authored words are available for provenance and exact-state admission checks. GPU adapters consume the semantic getters and convert them to wgpu values; they do not decode source state or choose an alpha-test family.

IW4 and IW5 share the captured state encoding. T5 alpha testing uses its own source decoder. T6 retains the existing absent alpha-test contract. The remaining state fields retain the current shared encoding and host behavior. This is preservation of the existing supported runtime contract, not a claim that every native source state is implemented.

Both the ordinary blend and the existing multiply-pass adaptation are compiled together. The execution caller selects the already prepared multiply policy. Unknown blend factors and operations remain explicit `UnsupportedState` refusals; authored state words are never repaired. Stencil provenance and the existing host treatment are preserved.

Technique selection keeps `SourceTechniqueSelection`: source namespace, source slot and the named policy (`Exact`, `DfogCompatibility`, `UnshadowedCompatibility`, `LitFallbackCompatibility`, `EmissiveCompatibility` or `DepthToColourCompatibility`). State compilation does not replace or relabel these adaptations. Source technique mapping remains in asset_material capture/preparation. Its private `MaterialCompiler` dispatches to named IW4/IW5/T5/T6 adapters and projects selected techniques and arguments into the runtime catalog; shared SM3/state behavior remains shared. `render_material::prepare_program_abi` owns exact source/stage lookup, SM3/DXBC decoding, declaration routing and ABI bindings. Its prepared product has a private constructor; low-level ABI builders are crate-private. Decoded DXBC stage mismatches refuse before lowering. The frontend lowers those prepared programs, validates WGSL and derives the GPU layout.

ABI preparation success establishes program decoding and register-source/layout bindings, not material literal/texture residency, supported WGSL lowering or frame-specific source readiness. Prepared ABI products borrow source names from their catalog, while admitted ports own validated modules and ABI data. Catalog replacement requires renewed admission and prepared tables under the current generation.

Stable material shells retain catalog generation, material/technique/vertex identity, selection provenance, compiled state and local bindings. Constructors and retained fields are private; capture rejects a mismatched execution identity, and rebinding receives the actual immutable catalog and refuses a stale shell. Rebinding resolves dynamic code constants and textures again and preserves typed failures. The renderer owns current-generation checks for retained frame products and GPU resources; a shell alone does not establish residency in a newer scene.

HUD chrome compiles its state when installing its menu catalog and caches the result with that catalog. Unsupported HUD state is refused during installation; blood composition also reports its material-state failure before creating a batch. Tess batches and pipeline keys carry compiled products. The current chrome namespace remains an explicit HUD ownership rule.

Family compilation also publishes colour camera routing, smodel colour admission, unlit sky routing and the explicit IW4 host-postfx admission rule. Shared draw consumers read these rules; authored camera regions remain available as provenance.

Material execution still uses one shared drawsurf path. Compiled state is independent of shader compilation success, texture residency, GPU pipeline availability and frame-specific dynamic sources. These dependencies must all succeed before drawing.

`compile_material_bindings` prepares a private, generation-bound operation plan
from the actual snapshot's technique arguments. It selects demanded frame and
local-light producers and prepares fixed saturation, SH approximation, water,
wind and host-default rows once. Execution runs those selected operations;
optional sources retain their existing absence/default policy. Unmapped required
sources remain absent and material execution reports `MissingCodeConstant`.

World preparation retains source provenance and supplies semantic lighting,
exposure, sky, fog and sun inputs. Material adapters own shader packing and named
compatibility defaults. The active material generation owns both frame and light
plans; map policy does not select a shader recipe. Stale plans and overlays
refuse before writes. Backend keeps common/light/overlay order and applies
prepared local-light rows after shared light/shadow/texture bindings. Frame
refresh/teardown clears products and catalog replacement invalidates run/shell
caches. GPU/media readiness remains separate.

T6 common and map materials use captured native headers, technique sets, state,
constants and textures. Missing captures or wrong source families refuse before
adding a compiled material. Common weapons do not inherit IW4 material surfaces.
Packed state decoding is shared; family adapters own alpha-test and draw rules.
