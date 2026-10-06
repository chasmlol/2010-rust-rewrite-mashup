# GSC post effects

GSC owns the presented post effects. Script presets and dvars pass through
snapshot metadata to the GPU; user brightness, DoF, bloom and debug tweak
settings do not suppress explicit script effects. State resets with the match.

* `VisionSetNaked/Night/Pain/Thermal/MissileCam(name, seconds)` and the
  `self ...ForPlayer(name, seconds)` methods select `vision/<name>.vision`.
  An empty name restores the map preset; global changes clear player overrides.
  Night follows the night vision flag. Pain blends with the base vision below
  half health and fades on recovery.
* `self SetBlurForPlayer(radius, seconds)` transitions Gaussian screen blur.
  Zero clears it; a new transition starts from the current blur.
* `self SetDepthOfField(nearStart, nearEnd, farStart, farEnd, nearBlur, farBlur)`
  controls focus and blur; zero distance ranges return to normal autofocus.
* `SetDvar(name, value)` applies globally; `self SetClientDvar(s)` overrides
  that client's values. Film supports enable, brightness, contrast,
  desaturation, dark desaturation, invert, and light/medium/dark RGB tints.
  Both `r_film...` and `r_filmTweak...` spellings are accepted.
* Bloom supports `r_glow`, radius0, bloom intensity0, cutoff and desaturation,
  including the `r_glowTweak...` spellings. DoF supports `r_dof_enable`,
  viewmodel/near/far start/end, near/far blur, and bias.
* IW4L also accepts `r_hue` / `r_filmHue` (degrees), `r_gamma` (>0),
  `r_exposure` (stops), `r_saturation` (1 is neutral), `r_blur`,
  `r_brightness` and `r_contrast`. These are dvars, not new GSC natives.
* `SetExpFog(start, halfway, r, g, b, [opacity,] seconds)` changes map fog.
  The 14-argument sun-fog form is supported. Transitions blend density and
  packed color from the current state; the first call applies immediately.
* `Earthquake(scale, seconds, origin, radius)` adds a timed camera shake.
  Calls overlap, attenuate linearly with distance and fade over their duration.
  Zero radius applies globally; each call clamps scale to 1 for camera angles.
  Shake is presentation only and does not change player aim or collision.
```c
self SetClientDvars("r_filmBrightness", 0.1, "r_hue", 90, "r_gamma", 1.2);
self SetClientDvar("r_filmLightTint", (1, 0.5, 0.25));
self SetBlurForPlayer(6, 0.5);
```

The render chain grades color, blurs the scene, then applies film, DoF and bloom.
HUD remains readable. Bloom preserves the material's authored sRGB writes.
Blurred shellshock accumulates successive scene frames using the profile's
`bg_shock_screenBlurBlendTime` and `bg_shock_screenBlurBlendFadeTime`.
It desaturates the saved image; stop, respawn and map changes reset history.
Thermal selection uses the scoped weapon and view gates.
`SetThermalBodyMaterial(name)` selects the global body camera material. An empty first
token selects `thermalbody_default`; Cold-Blooded bodies keep their material.
`thermalBlurFactorScope` and `thermalBlurFactorNoScope` set accumulation time
in milliseconds (default 250, range 0–10000). `cg_drawShellshock` controls profile
screen effects; thermal accumulation can still run while it is off.
Snapshots use protocol 99; host and client must share that protocol.
