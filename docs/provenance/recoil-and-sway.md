# Recoil and look sway

Camera and gun recoil use independent scalar responses satisfying
x'' + 2*w*x' + w*w*x = 0, with w equal to inverse peak time. A shot adds
velocity e*w*amplitude. The analytic transition advances physical deviation and
velocity once per frame. Output is limit*tanh(x/limit); physical state is never
clipped or snapped. Bursts add, cancellation is linear, and settling is exponential.

IW4L policy uses camera peak time 80 ms and model peak time 60 ms. Authored camera
ranges become peak amplitudes at scale 0.020; authored gun ranges at 0.015.
Camera output limit is 8 degrees; gun limits use the weapon-authored pitch/yaw
caps. Hip and ADS ranges and reduced-shot percentages blend continuously.
The gameplay recoil percentage multiplies amplitudes; a double-barrel event uses
two amplitudes. Camera roll impulse is zero. These choices are independent policy,
not measured retail response. Former center acceleration/decay parameters are
retained in asset decoding but do not drive the response. The existing weapon
reduced-shot timer and input random samples are outside this state-evolution core.

Look sway filters pitch/yaw angular velocity with an exact exponential response.
Its half life is 50 ms, and normalized drive is tanh(rate/180 degrees per second).
Authored sway limits and positional/angular gains scale that drive at 0.25.
Mode blending changes output gains without changing history or filtered rate.
Suppressed scope sway consumes look history and decays with zero rate, so release
does not replay hidden movement. Caller resets at weapon/life boundaries.
Landing/shellshock gains are bounded in [0,4] and separate from filter timing.

Camera output changes the displayed camera without rewriting user aim. For
composed iron-ADS aim, model and sway placement join the camera in the command's
two gun-angle offsets; hip and scope aim use the camera axis. Authority and
prediction consume the encoded offsets, including during input replay. Demo
files retain authoritative shot directions in snapshot events, while visual
response is reconstructed with the current policy. These responses therefore
affect gameplay and visuals. Command and demo formats are unchanged.

The analytic core was authored by a separate implementer receiving only a
source-free differential-equation contract and mathematical fixtures. Isolation
was procedural on a shared filesystem. The integrator inspected callers and
adapted the core to no_std using the existing libm component. Local artifacts
retain inputs, derivations and disposable probes. This is a development-boundary
record, not a legal conclusion, retail-feel guarantee or cross-version visual claim.
