# Open Device Helper: reference

The developer side of [Open Device Helper](helper.md): every feature with its kernel interface, the D-Bus API, the
design of kernel and helper updates, packaging and tests. What the helper is and how to use it:
[helper.md](helper.md). Code and build instructions: [`helper/`](../helper/README.md). The layer numbers below
(1 platform files, 2 helper daemon, 3 front-ends) are those of [helper.md → Layers](helper.md#layers).

## Feature inventory

<details>
<summary>Every feature with its kernel interface, privilege, layer and hazards</summary>

| Feature | Kernel / sysfs interface | Privilege | Layer | Notes and hazards |
|---|---|---|---|---|
| Charge limit | `/sys/class/power_supply/qcom-battmgr-bat/charge_control_end_threshold` | root | 2 | start threshold = limit − recharge gap (3..20 %, default 10). "Full by": the daemon raises the limit to 100 % when the estimated charging time before the set time begins, and puts it back two hours after the time, or at once when unplugged after it; not while Bypass is on.[^full-by] **The helper is the only owner.** The `CHARGE_LIMIT` udev hint is dropped so UPower/GNOME's "preserve battery health" switch no longer writes the threshold (two writers would overwrite each other) |
| Switch to Android | writes the Android boot image kept in `boot_b` back into `boot_a` and reboots; refuses unless `boot_b` matches the recorded SHA-256 | root; authentication optional (`android.require_auth`) | 2 (tool in 1) | `back-to-android` stays a layer-1 tool (also used by the emergency chord); the daemon method checks the hash file exists, emits a signal and runs it. Irreversible until the user switches back from Android |
| Emergency chord (volume up+down 10 s → Android) | evdev on `pmic_resin` and `gpio-keys` | root | **1** | must work with no desktop and no helper; stays a plain service |
| Torch | `/sys/class/leds/white:flash/brightness` (torch level ~96; never the flash strobe) | root (daemon); `video` group via the optional udev `74-…-leds.rules` without the helper | 2 | on/off/level with a safe maximum; front-ends never write sysfs |
| LED ring (charge indicator / effects) | `/sys/class/leds/aw22127:rgb:indicator/{brightness,multi_intensity,trigger,hw_pattern}` | root (daemon); `video` group via the optional udev `74-…-leds.rules` without the helper | 2 | the charge indicator (amber charging, green full or held, red at or below the low percentage) runs in the daemon. One daemon thread owns every write: solid color, breathing and `Pulse` (notification blinks). Breathing runs on the LED chip (`hw_pattern`, kernels from t37); on older kernels the thread draws it. The vendor effect scripts are not used (they would race the driver) |
| Idle refresh rate | `/sys/module/msm/parameters/idle_refresh_{policy,hz,ms60,ms30,min_hz,input,selfflush,state}` | root (parameters are 0644 root) | 2 | the kernel restores the mode's full rate before suspend or display-off, so the daemon needs no sleep handling. `selfflush` is a debug knob, not exposed |
| Panel modes (90 / 120 / 144 / 164 Hz) | DRM modes from the panel driver | user (compositor) | 3 (no daemon) | handled by each desktop's display settings; the helper does not switch modes |
| Performance profile (power-saver / balanced / performance) | GPU `/sys/class/devfreq/3d00000.gpu/{min,max}_freq`, CPU `/sys/devices/system/cpu/cpufreq/policy{0,6}/scaling_{min,max}_freq` | root | 2 | one profile sets GPU and CPU limits (and, when chosen, the thermal profile and Wi-Fi power saving); follows power-profiles-daemon when present (it offers only power-saver and balanced on this kernel: `placeholder` driver), otherwise the daemon's own choice. Widen before narrowing (min ≤ max at every step). CPU values are rounded to the nearest available frequency; a maximum at the top of the range writes `cpuinfo_max_freq`, so boost frequencies stay reachable; floors are capped at 1996 MHz (little) and 2880 MHz (big), the first thermal step.[^cpu-floor] Changing boost resets `scaling_max_freq`, so the limits are written again after it. Governor and `energy_performance_preference` (absent on scmi) are not exposed |
| Speaker level (protected mode) | ALSA controls `aw_dev_0_volume`, `aw_dev_1_volume` | `audio` group | 1 | user unit `tb323fu-speaker-gain`: amplifier attenuation 21 while the PipeWire protection filter runs, 159 otherwise (`/etc/tb323fu/audio.conf`). **Safety-relevant**: the loud value is only safe behind the filter chain. Do not move into the daemon; do not expose a free slider. Never restart PipeWire during playback (it stalls the audio DSP) |
| Microphone gain | ALSA `ADC1/ADC3 Volume` | `audio` | 1 | boot default only (`MIC_GAIN`, applied by `tb323fu-audio-defaults`) |
| Auto brightness | desktop setting (GNOME: `org.gnome.settings-daemon.plugins.power ambient-enabled`) | user | 3 | desktop-owned; a separate GNOME tile binds the GSettings key |
| Vibration strength and test | evdev `EV_FF` on the two AW86937 inputs (`aw86927-haptics`): `FF_GAIN` (0..0xffff, kept by ff-memless for every application), a 0.3 s `FF_RUMBLE` for the test | root (daemon) | 2 | the gain applies to everything that vibrates (on-screen keyboard, games); motors named by I2C address (0x5b = left, as in the device tree). Re-applied when the motors reappear (driver reload); on-screen-keyboard feedback itself stays the desktop's |
| Wake on charger plug | `/sys/class/power_supply/*/power/wakeup` (battery manager, USB and UCSI supplies) | root | 2 | on in the kernel; "off" stops a charger plugged in or out (and battery-manager events) from waking the tablet. Left as the kernel has it unless set. USB-C roles are shown read-only (`typec/port*`) |
| Board-temperature (thermal) profile | `quiet-thermal` zone, `trip_point_{1..8}_temp` (passive, 43–50 °C in the device tree; mainline marks every DT trip writable) | root | 2 | quiet −3 °C, default (DT), performance +8 °C. **Allow-list and hard bounds in code:** only that zone, only `passive` trips (never `hot`, never a chip, PMIC or NSP zone, never `mode`/`policy`/`emul_temp`), the highest trip never above 58 °C. The DT values are recorded once per boot (`/var/lib/tb323fu/thermal-base`, with the boot id) so a restarted daemon never takes an offset as the base; a base outside 35–55 °C or out of order is refused. Raising writes the highest trip first, lowering the lowest first. performance ends by itself (persisted as default, signal `ProfileFallback`) when `batt`/`batt2` reach 45 °C or the board passes its top step by 2 °C, and switches Bypass on while on a charger.[^thermal-perf] A reboot restores the DT steps until the daemon applies the profile again |
| Panel heat limit | `lcm-thermal` temperature, `/sys/class/backlight/aw99706-backlight/brightness` | root | 2 | Android's `lcm-thermal` policy: from 55 °C the backlight is held at 178/255 (2858/4095), released below 52 °C; the previous brightness comes back unless someone changed it meanwhile. On by default; switching it off asks for authentication |
| Compressed swap in RAM (zram) | `/etc/tb323fu/zram.conf` (`ENABLED`, `SIZE`, `ALGORITHM`), read by the platform's `tb323fu-zram.service`; state from `/proc/swaps`, `/sys/block/zramN/{disksize,comp_algorithm,mm_stat}`, `vm.swappiness`, `vm.page-cluster` | root | 1 (the unit), 2 (the switch) | on by default: zstd, as large as the RAM, priority 100 (before a swap file), `vm.swappiness=100`, `vm.page-cluster=0`; an existing zram swap (zram-generator) is left alone. Turning it off moves the swapped pages back, which fails when they do not fit in RAM. NixOS: `services.tb323fu.zram` |
| Wi-Fi power saving in the performance profile | `iw dev IFACE set power_save` (nl80211) | root | 2 | set with `iw`, only on interfaces whose applied connection does not set `powersave` itself; re-checked once a minute; switched back on when the profile ends.[^wifi-ps] Connection profiles are never modified. Off by default |

</details>

Not implemented: see [Not exposed](helper.md#not-exposed) in the user guide. Chip protection trips (95/105 °C) are never exposed, and
speaker limits stay in layer 1.

## D-Bus API

System bus name **`io.github.joonhoekim.OpenDeviceHelper1`**, one object per feature under `/io/github/joonhoekim/OpenDeviceHelper1/…`.
Standard `org.freedesktop.DBus.Properties` for properties (with `PropertiesChanged`), plus the methods below.

<details>
<summary>Objects, properties, methods and polkit actions</summary>

**When the helper asks for a password:** only where a mistake is hard to undo or opens the device to
others: writing a kernel to `boot_a` (install, rollback, install from a file every time), replacing the helper itself (update, rollback), the USB developer mode
(a root console for anyone with a cable), and the Android-switch authentication setting itself. Everything else
is allowed for the active local user, including the performance thermal profile (its ceiling is fixed in code and the app warns first),
turning off the panel protection or the emergency key, the default system and the update channel.
Administrators can tighten this with polkit rules. Inactive or remote sessions get `no` for everything except reading properties.

| Object / interface | Properties | Methods | Signals | polkit action (default for the active local user) |
|---|---|---|---|---|
| `/…/Battery` · `io.github.joonhoekim.OpenDeviceHelper1.Battery` | `ChargeLimit` (u, %), `Bypass` (b: the battery held on external power; see `BypassMethod`),[^bypass] `BypassMethod` (s: `auto` / `current` / `limit`; `auto` = `current` when the kernel has `constant_charge_current`, series 0132), `BypassMethodActive` (s: what `auto` means here), `BypassCurrentMa` (u, 50..1000, default 100), `ChargeCurrentMa` (i: the charge current limit now, -1 without the knob; never written 0, the firmware then stops charging until the limit crosses the charge), `StallRescue` (b, default on: after 3 min on external power, below the start threshold and "Not charging", the limit is taken below the charge for one poll and back; at most every 30 min, only above 55 %), `Status` (s, raw kernel), `State` (s: `charging` / `discharging` / `bypass` / `full` / `not-charging`, derived from status, current, limit and the Bypass switch),[^battery-state] `Capacity` (u), `CurrentMa` (i), `VoltageMv` (u), `TemperatureC` (d), `Health` (s), `CycleCount` (i, −1 unknown), `DesignCapacityMah` (i, −1 unknown), `ChargerType` (s: UCSI `usb_type`, e.g. `C`, `PD`; `USB` without UCSI, `none`), `ChargerContract` (s: the UCSI contract, e.g. `PD 9.0 V 3.00 A`; for PD/PPS chargers the firmware reports 0, then the measured input marked `in`: `PPS · 9.2 V in · ~40 W`; `unknown` / `none`), `ChargerAdapter` (s: adapter the battery manager detected: `SDP`, `DCP`, `CDP`, `PD`, `PD_PPS`, …; `""` unknown), `InputVoltageMv` (u), `InputCurrentMa` (u: measured charger input, 0 unplugged), `RechargeGap` (u, %: charging resumes this far below the limit; default 10), `FullBy` (s: `HH:MM` local time to be full by, "" off), `FullByDays` (as: `mon`..`sun`, empty = every day), `FullByActive` (b: the limit is at 100 % for FullBy now; `ChargeLimit` keeps showing the configured limit), `StateOfHealth` (i, % of design capacity, −1 unknown), `OcvMv` (u: open-circuit voltage estimate) | `SetChargeLimit(u)`, `SetBypass(b)`, `SetBypassMethod(s)` (a running Bypass switches over), `SetBypassCurrent(u mA)` (50..1000), `SetStallRescue(b)`, `SetRechargeGap(u)` (3..20), `SetFullBy(s time, as days)` ("" switches it off) | `Changed()` | `…charge-limit`: allow (`yes`) |
| `/…/Android` · `…Android` | `Available` (b: hash file present and `boot_b` recorded), `ImageSha256` (s), `RequireAuth` (b) | `SwitchToAndroid()`, `SetRequireAuth(b)` (`…admin`) | `SwitchingToAndroid()` | `…android-switch`: allow (`yes`); `…android-switch-auth`: `auth_admin_keep`. The daemon checks the second one only when `android.require_auth = true` in the config (default `false`) |
| `/…/Torch` · `…Torch` | `On` (b), `Level` (u), `MaxLevel` (u) | `Set(b)`, `SetLevel(u)` | — | `…torch`: allow |
| `/…/LedRing` · `…LedRing` | `Mode` (s: `charge` / `solid` / `breathe` / `chase` / `rainbow` / `off`; chase and rainbow need kernel patch 0154), `Brightness` (u), `LowPercent` (u), `Color` (s: `#rrggbb` of solid, breathe and chase), `Speed` (u: breathing cycle or one turn of chase and rainbow, ms), `ChargeOverride` (b: in solid/breathe/chase/rainbow the charge colors take over while charging, full or held, or low), `NotifyPulse` (b: front-ends pulse the ring for desktop notifications; the GNOME extension does), `OffWithScreen` (b: dark while the screen is off, except the charge colors) | `SetMode(s)`, `SetBrightness(u)`, `SetLowPercent(u)`, `SetColor(s)`, `SetSpeed(u)` (1000..20000), `SetChargeOverride(b)`, `SetNotifyPulse(b)`, `SetOffWithScreen(b)`, `Pulse(s color, u count)` (blink 1..10 times, "" = the ring color) | — | `…led-ring`: allow |
| `/…/Refresh` · `…Refresh` | `Policy` (s: `off` / `auto` / `manual`), `Rate` (u, manual Hz), `IdleMs60` (u), `IdleMs30` (u), `MinHz` (u), `InputWakes` (b), `MaxHz` (u: full rate of the current display mode, e.g. 90, 120, 144 or 164; 120 when it cannot be read), `LiveRate` (u, read-only, from `state`) | `SetPolicy(s)`, `SetRate(u)`, `SetIdle(u ms60, u ms30)`, `ApplyPreset(s)` (`power-saver` 0.5 s/2 s, `balanced` 1 s/5 s, `smooth` 3 s/15 s) | — | `…refresh`: allow |
| `/…/Gpu` · `…Gpu` (the performance profile; the name stays for compatibility) | `Profile` (s: `power-saver` / `balanced` / `performance`), `FollowPowerProfiles` (b), `Floors` (a{s(uu)}: GPU MHz per profile), `CpuLimits` (a{s(uuuu)}: profile → little min, little max, big min, big max MHz; the whole range without an entry), `CpuRange` (a{s(uu)}: `little` / `big` → hardware MHz, boost not counted), `CpuBoost` (b: cpufreq boost, the fast cores' 4.512/4.608 GHz; on by default, `[cpu] boost`), `WifiLowLatency` (b: Wi-Fi power saving off in `performance`), `WifiAvailable` (b: a wireless interface and `iw`) | `SetProfile(s)` (also stops following), `SetFollowPowerProfiles(b)`, `SetLimits(s profile, u min_mhz, u max_mhz)`, `SetCpuLimits(s profile, u little_min, u little_max, u big_min, u big_max)`, `SetCpuBoost(b)`, `SetWifiLowLatency(b)` | — | `…gpu`: allow (`yes`); `SetProfile("performance")` also checks `…thermal-performance` when the thermal profile follows |
| `/…/Usb` · `…Usb` | `WakeEnabled` (b: USB host/port wakeup from suspend), `DevMode` (b: USB gadget network + serial console for developers), `ChargerWake` (b: plugging or unplugging a charger wakes the tablet; kernel default on, `[usb] charger_wake`), `Ports` (a(sssb): USB-C port, data role, power role, partner attached; read-only) | `SetWake(b)`, `SetDevMode(b)`, `SetChargerWake(b)` | — | `…usb-wake`: allow; `…dev-mode`: `auth_admin_keep` |
| `/…/EmergencyKey` · `…EmergencyKey` | `Enabled` (b), `HoldSeconds` (u): writes `/etc/tb323fu/emergency-key.conf` (`ENABLED`, `HOLD_SECONDS`, 3..30 s) that the layer-1 service reads | `SetEnabled(b)`, `SetHoldSeconds(u)` | — | `…emergency-key`: allow; `…emergency-key-disable`: allow |
| `/…/Diagnostics` · `…Diagnostics` | `CrashRecords` (u: pstore archive entries), `LastBootClean` (b: no panic record archived this boot and the previous boot's journal ends with a clean shutdown; a dump-mode crash leaves no pstore record) | `Export() → s` (path of a tarball with pstore, previous-boot journal tail, dmesg head, versions, the `tb323fu-check` output; user names/addresses/serials stripped) | — | `…diagnostics`: allow |
| `/…/Boot` · `…Boot` | `Roots` (a(ssbs): GPT partition name, os-release PRETTY_NAME, present, init kind `systemd`/`nixos`/`none`, or `unknown` when it could not be mounted read-only to look, e.g. after an unclean shutdown), `Default` (s), `Next` (s, one-shot), `Current` (s: the partition `/` came from), `RootHealth` (a{sas}: root → problems for the running kernel: no audio DSP or Wi-Fi firmware; without shared modules or in `own` mode also no `lib/modules/$(uname -r)/modules.dep`, no `extra/` (aw882xx amplifier driver), or "own modules"; roots without problems and NixOS roots are left out) | `SetNext(s)`, `ClearNext()`, `SetDefault(s)`, `RebootInto(s)`, `Rescan()` | — | `…boot-next`, `…reboot-into`, `…boot-default`: allow |
| `/…/Thermal` · `…Thermal` | `Surface` (d, °C: skin NTC, else quiet), `CpuMax` (d: hottest `cpu-*`/`cpullc-*` tsens zone), `GpuMax` (d: hottest `gpuss-*`), `Throttling` (b: a `cpufreq-*`/`devfreq-*` cooling device above state 0), `Zones` (a{sd}: board sensors by zone type without `-thermal`: skin, quiet, batt, batt2, usb, usb2-conn, lcm, wlan, ddr, ufs, xo, rear-cam, fcam, wls; NaN = absent), `Profile` (s: `quiet` / `default` / `performance` as the trips are now, `custom` when they match none, "" without the zone), `Profiles` (as, empty when the kernel has no writable quiet-thermal steps), `TripOffset` (i, °C), `Trips` (ad: the board steps now, °C), `FollowPerformance` (b: power-saver → quiet, balanced → default, performance → performance), `PerformanceBypass` (b), `PanelLimit` (b), `PanelLimited` (b: the backlight is held now) | `SetProfile(s)` (also stops following; refused while too hot), `SetFollowPerformance(b)`, `SetPerformanceBypass(b)`, `SetPanelLimit(b)` | `ProfileFallback(s reason)` | `…thermal`: allow (quiet, default, following, bypass, panel protection on); `…thermal-performance`: allow (performance, following while the performance profile is in effect, panel protection off; the app warns first, the 58 °C ceiling is fixed in code) |
| `/…/Haptics` · `…Haptics` | `Strength` (u, 0..100 %), `Motors` (as: `left`, `right`) | `SetStrength(u)`, `Test(s motor)` (`left` / `right` / `both`: a 0.3 s buzz at the current strength) | — | `…haptics`: allow |
| `/…/Memory` · `…Memory` (when the platform unit is installed, on NixOS, or with an active zram swap) | `ZramEnabled` (b: `zram.conf` `ENABLED`; on NixOS whether a zram swap is active), `ZramSizeMib` (u, 0 = the RAM size), `ZramConfigurable` (b: not NixOS, unit installed), `ZramActive` (b), `ZramDevice` (s), `ZramSource` (s: `tb323fu`, `other`, ""), `ZramAlgorithm` (s), `ZramDiskSize`, `ZramOrigData`, `ZramComprData`, `ZramMemUsed` (t, bytes), `Swappiness`, `PageCluster` (u) | `SetZram(b)`, `SetZramSize(u)` (0 or 256..65536 MiB): write `zram.conf` and restart `tb323fu-zram.service` | — | `…memory`: allow |
| `/…/Kernel` · `…Kernel` (when `boot_a` and `boot_b` exist) | `Running` (s: `uname -r`), `RunningBuild` (s: `/proc/version`), `SharedModules` (b: `/lib/modules/<release>` is the boot image's squashfs), `Channel` (s), `Source` (s: `github:OWNER/REPO`), `Available` (a(sssu): tag, release title, release page URL, serial; the channel's release when newer than the running kernel), `Downloaded` (as: verified, ready to install), `State` (s: `idle` / `checking` / `downloading` / `verifying` / `ready` / `installing` / `pending-reboot` / `trial` / `rolled-back`, also `rolling-back` / `keeping` / `dismissing` while those run), `Progress` (u, % of a download), `Trial` (s), `TrialChannel` (s: `stable` / `testing` / `local`), `TrialLabel` (s), `Tries` (u), `MaxTries` (u), `KeepPending` (b: the running kernel is a trial that waits for Keep), `Good` (s: release of `linux-good.img`), `GoodLabel` (s), `LastFailed` (s), `LastCheck` (t, unix time), `IndexExpired` (b: always false; kept for older clients), `AutoCheck` (b), `RequireSignature` (b), `HelperLatest` (s: a newer `helper-vX.Y.Z` release, "" none), `HelperUpdateCommand` (s), `Message` (s: the last operation's result) | `Check() → s`, `Download(s tag) → s`, `Install(s tag, b reboot) → s`, `InspectLocal(h file) → (s release, s banner, s format, b shared_modules, as warnings)`, `InstallLocal(h file, s name, b auto_confirm, b reboot) → s` (a file descriptor the caller opened), `Rollback(b reboot) → s`, `Keep() → s`, `Dismiss() → s`, `Notes(s tag) → s`, `SetChannel(s)`, `SetAutoCheck(b)`, `SetHelperNotify(b)`, `Refresh()` (re-read `kernel-state`; no authorization). The long methods answer when done | `Finished(s operation, b ok, s message)` | `…kernel-check` (also Dismiss, InspectLocal), `…kernel-download`, `…kernel-keep`: allow; `…kernel-channel` (also SetAutoCheck, SetHelperNotify): allow; `…kernel-install`, `…kernel-rollback`: `auth_admin_keep`; `…kernel-install-local`: `auth_admin` (every time) |
| `/…/Mesa` · `…Mesa` (always; since 0.4.0) | `Installed` (s: version `current` points at), `Previous` (s), `Available` (s: the newest `mesa-*` release of the kernel channel newer than `Installed`), `Downloaded` (s: verified and unpacked, not current), `Enabled` (b: `env.conf` selects the build), `Trial` (s), `Tries` (u), `MaxTries` (u), `Failed` (s: the version switched off after a trial), `FailedReason` (s), `Drivers` (as: `vulkan`, `opencl` from the MANIFEST), `Base` (s: upstream Mesa commit), `State` (s: `idle` / `ready` / `trial`, or the running operation), `Progress` (u), `Message` (s) | `Check() → s`, `Download(s version) → s` ("" the newest), `Install(s version) → s`, `SetEnabled(b) → s`, `Keep() → s`, `Rollback() → s`, `Notes(s version) → s` | `Finished(s operation, b ok, s message)` | `…kernel-check` (Check, Notes), `…kernel-download`, `…kernel-keep`: allow; `…mesa-install` (Install, SetEnabled, Rollback): `auth_admin_keep` |
| `/…/HelperUpdate` · `…HelperUpdate` (always; since 0.3.0) | `Version` (s), `Available` (s: the newest `helper-vX.Y.Z` release newer than this helper, "" none or `kernel.helper_notify` off), `Installable` (b: it carries the asset set and this helper may replace itself), `Method` (s: `self` / `dpkg` / `pacman` / `rpm` / `nix`), `UpdateCommand` (s), `Reason` (s: why not `self`, ""), `Downloaded` (s: a newer version downloaded and checked), `Previous` (s: what Rollback puts back), `State` (s: `idle` / `checking` / `downloading` / `verifying` / `ready` / `installing` / `interrupted`, also `rolling-back`), `Progress` (u, %), `LastUpdate` (a{ss}: `result` (`installing` / `ok` / `rolled-back` / `failed`), `kind`, `from`, `to`, `time`, `reason`), `Message` (s), `LastCheck` (t) | `Check() → s`, `Download(s version) → s`, `Install(s version) → s` (returns once the transient unit runs; the helper restarts), `Rollback() → s`, `Notes(s version) → s` ("" = the newest release) | `Finished(s operation, b ok, s message)` | `…kernel-check` (Check), `…kernel-download` (Download): allow; `…helper-update` (Install, Rollback): `auth_admin_keep` |
| `/…` · `…Helper` | `Version` (s), `Features` (as), `Kernel` (s), `SeriesTag` (s: `/etc/tb323fu/series`, else the tag of a `-tb323fu-TAG` kernel release, else unknown), `Firmware` (a{ss}: file → sha256 match state vs the manifest) | `Reload()` | — | `…admin`: `auth_admin` |

</details>

Implementation notes: property getters read the device live. A 5 s poller runs the LED-ring charge indicator and the GPU profile follow, re-asserts the USB wakeup setting, and emits `PropertiesChanged` (changed properties invalidated) when values change underneath. `Battery.Changed()` fires on state-like battery changes (not on every current sample), `Thermal` on a change of a whole degree.

Each object is only exported when its sysfs interface exists (for example no `Refresh` object on a kernel without the idle-refresh patch).

## Kernel updates

How kernel updates work in detail. The user's view (channels, Keep, going back): [helper.md → Kernel
updates](helper.md#kernel-updates). Design and the reasons behind it:
[kernel-updates-design.md](notes/kernel-updates-design.md).

**What a release is.** A GitHub Release (tag `kernel-tNN`) of the configured repository with a kernel `Image`,
never a boot image: a project-built boot image would carry Lenovo's header, GKI signature and vbmeta blobs,
which must come from your own tablet. So the helper packs the kernel into **your own stock boot image** (the copy in
`boot_b`, checked against the recorded Android hash first), exactly as `tools/boot-repack-kernel.py` does.[^repack] The assets:

| Asset | |
|---|---|
| `Image-tb323fu-tNN` | the raw arm64 `Image`; its release string ends in `-tb323fu-tNN` (the helper refuses anything else) |
| `Image-tb323fu-tNN.gz` | the same, gzip (optional; when present the helper downloads and installs this one, the bootloader decompresses gzip) |
| `SHA256SUMS` | `sha256sum` of every file of the release |
| `SHA256SUMS.minisig` | optional, only for helpers with `require_signature` (below) |
| the release body | the release notes (Markdown), shown in the app; a line `<!-- tb323fu: min_helper=X min_platform=Y -->` states the helper and platform versions the kernel needs |

`tNN` is the release's **serial**: a release is offered when its serial is higher than the running kernel's. Channels:
[helper.md](helper.md#kernel-updates). Releases are prepared with [`tools/kernel-release.py`](../tools/kernel-release.py) and
published with `gh release create`.

**Source.** `[kernel] source = "github:OWNER/REPO"` in `/etc/tb323fu/helper.toml` (default
`github:joonhoekim/tb323fu-linux`). A fork publishes the same asset set and its users point `source` at it. The helper
reads the release list through the GitHub REST API (`/repos/OWNER/REPO/releases`) without a token. GitHub allows 60 such
requests an hour per address; the daily check needs one, and an unchanged list is answered from the cached copy (`ETag`; a
"not modified" answer does not count). When the limit is reached the check says until when. A token is needed only for a
private repository or many tablets behind one address: put a fine-grained read-only token into `/etc/credstore/tb323fu-github-token` (mode 600). Only the download unit reads it (as a systemd
credential), and it is sent only to `api.github.com`.

**Trust.** Releases are published from the maintainer's GitHub account (two-factor authentication;
[design 3.8](notes/kernel-updates-design.md#38-channel-github-releases-2026-10-03)). The helper checks the
file's size, its SHA-256 against `SHA256SUMS` and against the digest GitHub computed at upload, a bootable format, and the
kernel's own version banner (release `…-tb323fu-tNN` of the asset's name). `SHA256SUMS` comes from the same place as the
kernel, so it catches a damaged download, **not** a changed release: whoever can publish a release in that repository
decides what you install. Optional: `require_signature = true` with `public_keys = ["RW…"]` (or
`/etc/tb323fu/keys/kernel-*.pub`) makes the helper also require `SHA256SUMS.minisig` from one of those minisign keys
(off by default; no key is built in), for whoever runs their own channel. Build attestations (a release built by
GitHub Actions, verifiable without any key) are planned: [design 3.9](notes/kernel-updates-design.md#39-plan-2-releases-built-by-github-actions-with-artifact-attestations).

**The flow.**

1. **Check** (daily when `kernel.auto_check` is on, or on request): the daemon writes the URLs it wants to
   `/run/tb323fu/kernel-fetch.list` and starts `tb323fu-kernel-fetch.service`, a dynamic user with network access
   and its cache directory `/var/cache/tb323fu-kernel` as the only writable place; the daemon itself never opens a
   network connection. It parses the release list (size-limited) and picks the channel's release.
2. **Download**: the same unit fetches `SHA256SUMS` and the kernel file; the daemon checks them as above and keeps
   them in `/var/lib/tb323fu/kernel/staged/<tag>/`.
3. **Install** (admin): the staged file is checked again; battery ≥ 30 % or a charger; `boot_b` must be the
   recorded Android image; the running kernel must not be on trial itself. If `boot_a` holds the running kernel, it
   is saved first as `/var/lib/tb323fu/linux-good.img` on the state root (tmp file, hash check, rename); otherwise an
   existing, verified `linux-good.img` is required. Then the trial record, then the repacked image goes to `boot_a`,
   is read back and compared; on a mismatch `linux-good.img` is written back.
4. **Trial**: on each start the initramfs compares the record with `/proc/version` and counts the start in
   `kernel-state` (volume-up held: not counted). The **third** start of a kernel that never got confirmed writes
   `linux-good.img` back into `boot_a` (checked against its record), records `failed=` and restarts.
5. **Confirm**: stable channel: `tb323fu-kernel-confirm.service` (platform files, layer 1, so it works without
   the helper) waits until the system reached `graphical.target` (or `multi-user.target`) and 90 s more, on any
   root, then copies `boot_a` to `linux-good.img` and records the kernel as good. Testing channel, pre-releases and
   kernels from a file: only **Keep** (settings app, `tb323fu-ctl kernel keep`) confirms (`trial_keep=1`); without
   it the kernel goes back after two more starts. The same unit records a hand-flashed kernel as good once it ran
   90 s, so a later trial always has something to go back to.

A kernel that dies **before** the initramfs runs cannot be caught (nothing of ours runs; the way back is
[recovery](recovery.md#linux-does-not-boot)). The bootloader's A/B retry bits are not used (slot `_b` is not a working
slot here). Android's Switch to Linux prefers `linux-good.img` to a kernel on trial ([android/README.md](../android/README.md)).

**A kernel from a file.** `tb323fu-ctl kernel install-local PATH` or **Install Kernel from File…** in the app
goes through the same flow as a release (user view: [helper.md](helper.md#kernel-updates)). The inspection looks for the
shared modules image (`/lib/modules/<release>.sqfs`; without it every root needs its own modules for that release, `own`
mode, or it starts without them). With `--keep` the confirm unit keeps the kernel after 90 s like a stable release.
`--name` gives it a label that the status shows. The file is opened by the caller and handed to the daemon as a file descriptor, so the daemon reads
only files the caller can read. **Authentication:** a local install asks for the administrator's password **every
time** (polkit `auth_admin`, not remembered), while a release install may reuse a recent authentication
(`auth_admin_keep`): a release passed the project's publishing and the helper's checks, a file passed nothing but the
choice of the person installing it, and it decides what runs as the kernel.

State on the state root (`/var/lib/tb323fu/`): `kernel-state` (`good=`, `good_sha256=`, `good_version=`,
`good_serial=`, `good_label=`, `trial=`, `trial_sha256=`, `trial_version=`, `trial_serial=`, `trial_channel=`
(`stable`, `testing`, `local`), `trial_keep=`, `trial_label=`, `tries=`, `max=`, `failed=`, `failed_sha256=`;
`*_version` is the `/proc/version` line, which tells two builds with the same release apart) and `linux-good.img`
(+ `.sha256`).

Settings (`[kernel]` in `helper.toml`): `channel` (`stable`), `source` (`github:joonhoekim/tb323fu-linux`),
`api_url` (`https://api.github.com`; another URL, `http://` or `file://`, only for a local test stand-in made with
`kernel-release.py fake-api`), `auto_check` (`true`; checks only, never downloads), `helper_notify` (`true`: a
`helper-vX.Y.Z` release newer than this helper shows in About and `tb323fu-ctl kernel`, with what updates the helper
on this system; see [Helper updates](#helper-updates)), `require_signature` (`false`; kernel and helper releases),
`public_keys` (`[]`).

## Helper updates

How the helper updates itself in detail. The user's view: [helper.md → Helper updates](helper.md#helper-updates).

**What a release is.**

| Asset | |
|---|---|
| `tb323fu-helper-X.Y.Z-aarch64.tar.gz` | `MANIFEST` and `root/<path>`: exactly what `helper/install.sh`, `crates/tb323fu-settings/install.sh` and the GNOME extension install with `PREFIX=/usr/local` |
| `SHA256SUMS` | `sha256sum` of every file of the release |
| `SHA256SUMS.minisig` | optional, only for helpers with `require_signature` |
| the release body | the notes (Markdown); a line `<!-- tb323fu: min_kernel=tNN min_platform=X -->` states what the helper needs |
| anything else | ignored (for example the Debian packages of the same version) |

`MANIFEST` (`format=1`) names the version, `arch=aarch64`, `prefix=/usr/local`, `min_glibc` (the newest
`GLIBC_x.y` symbol the binaries use), the commit, and one `file SHA256 MODE SIZE PATH` line per file. The helper
accepts only these paths, the files `install.sh` installs and nothing else (`ALLOWED_FILES`/`ALLOWED_DIRS` in
`helperupdate.rs`): `/usr/local/libexec/tb323fu-{helperd,kernel-fetch}`, `/usr/local/bin/tb323fu-{ctl,settings}`,
`/etc/systemd/system/tb323fu-{helperd,kernel-fetch}.service`, the D-Bus policy and activation file, the polkit
policy, the settings app's `.desktop`/icon/metainfo, `/usr/local/share/doc/tb323fu-helper/` and the extension's
directory under `/usr/local/share/gnome-shell/extensions/`. Layer-1 platform files are never part of it. Releases are
made with [`tools/helper-release.py`](../tools/helper-release.py) (`assets`, `check`) from a release build **on the
oldest supported system** (the Debian 13 trixie, glibc 2.41, root of the tablet), because binaries built against
a newer glibc do not start on an older one; the helper also refuses a release whose `min_glibc` is newer than the
system's (`getconf GNU_LIBC_VERSION`).

**Who updates the helper.** Decided by who owns the running daemon binary (and the polkit and D-Bus files in
`/usr/share`), not by the distribution's name:

| Owner | `Method` | What happens |
|---|---|---|
| none | `self` | self-update |
| `dpkg-query -S` names a package | `dpkg` | refused |
| `pacman -Qo` | `pacman` | refused |
| `rpm -qf` | `rpm` | refused |
| `/etc/NIXOS` or a `/nix/store` path | `nix` | refused |

What the user is told for each: [helper.md → Helper updates](helper.md#helper-updates).

A self-update over an install at `/usr` that nobody owns puts the new version in `/usr/local` (and the unit in
`/etc/systemd/system`, which overrides the one in `/usr/lib`); the `/usr` copy stays underneath.

**The flow.**

1. **Check**: the same release list as kernel updates (one request through `tb323fu-kernel-fetch.service`; the
   daily check covers both). `Available` is the newest `helper-v` release (not a pre-release) newer than this
   helper, shown while `kernel.helper_notify` is on.
2. **Download** (allowed without a password): `SHA256SUMS` and the tarball through the fetch unit; the daemon
   checks size, SHA-256 against `SHA256SUMS` and GitHub's digest, the signature when required, then unpacks it
   (plain ustar, regular files only, safe names) into `/var/lib/tb323fu/helper/staged/<version>/tree` and checks
   `MANIFEST`: version = tag, arch, prefix, allowed paths, the daemon, CLI and unit present, every file's size
   and hash, nothing unlisted.
3. **Install** (polkit `helper-update`, `auth_admin_keep` like a kernel install): refused when a package manager
   or NixOS owns the helper, when the release is not newer, when `min_kernel` (the running kernel's `tNN`; a
   development kernel passes), `min_platform` (`platform-version`) or `min_glibc` is not met. The tree is checked
   again, then the daemon starts **this binary** as `tb323fu-helperd --apply …` in a transient unit
   (`systemd-run --unit=tb323fu-helper-update`), outside the daemon's sandbox and outliving its restart:
   - a snapshot of every path the release names (and every file the previous self-update installed) into
     `/var/lib/tb323fu/helper/backup/`; missing paths are recorded as absent; a directory or symlink in the way
     stops the update before anything changes;
   - each new file is written next to its target (`*.tb323fu-new`), then all are renamed over their targets
     (each file changes atomically); files of the previous self-update that the new release no longer has are
     removed;
   - `systemctl daemon-reload`, D-Bus `ReloadConfig`, `systemctl restart tb323fu-helperd`;
   - **health check**: within 30 s the restarted daemon answers on the system bus with the new `Version`, and
     polkit knows the helper's actions (a malformed policy blocks every call);
   - passed: the snapshot becomes `/var/lib/tb323fu/helper/prev/`; failed: the snapshot goes back the same way,
     the old daemon is restarted, `update-state` says `rolled-back` and why.
4. **Rollback** (`helper-update`): the same process with `prev/` as the source, so a rollback can be rolled back.
   An update cut off half-way (power loss) shows `State` `interrupted`; Rollback then uses the snapshot of that update.

The settings app needs a restart to show its new version, and the GNOME extension's new code loads at the next
login (Wayland). State: `/var/lib/tb323fu/helper/` (`staged/`, `current.manifest`, `prev/`, `update-state`,
`update.log`); the transient unit's log is `journalctl -u tb323fu-helper-update`.

**Trust.** The same as for kernel releases: whoever can publish releases in `kernel.source` decides what runs as
root on the tablets that update. `SHA256SUMS` and GitHub's digest catch damaged downloads, not a changed release;
`require_signature` (minisign, optional) applies to helper releases too. The channel rests on two-factor
authentication on the publishing account and a tag ruleset for `helper-v*` and `kernel-*` (only the maintainer
creates, nobody updates or deletes). GitHub's immutable releases are not turned on
([design 3.8](notes/kernel-updates-design.md#38-channel-github-releases-2026-10-03)). Build attestations
verified by the helper are plan 2 ([design 3.9](notes/kernel-updates-design.md#39-plan-2-releases-built-by-github-actions-with-artifact-attestations));
a helper that updates itself changes its own verifier, so it should require them before the kernel channel does.

**Compatibility.** Front-ends of one version talk to daemons of another. Rule for the D-Bus API: **within
`io.github.joonhoekim.OpenDeviceHelper1` changes are additive only**: new objects, properties, methods and signals;
existing names, argument types and meanings stay. A breaking change gets a new name (`OpenDeviceHelper2`), served next
to the old one while front-ends move. Front-ends treat a missing object or property as "not supported" (the app hides
the row; the CLI says the helper is older).

## Packaging

Files (paths for a normal FHS distribution):

| File | Path |
|---|---|
| daemon | `/usr/libexec/tb323fu/tb323fu-helperd` (packages; `/usr/local/libexec/` for a manual install) |
| CLI | `/usr/bin/tb323fu-ctl` |
| systemd unit | `/usr/lib/systemd/system/tb323fu-helperd.service` (`Type=dbus`, `BusName=io.github.joonhoekim.OpenDeviceHelper1`, `WantedBy=multi-user.target`, hardening: `ProtectSystem=strict`, `ReadWritePaths=/etc/tb323fu /sys`) |
| D-Bus policy | `/usr/share/dbus-1/system.d/io.github.joonhoekim.OpenDeviceHelper1.conf` (own: root; send: everyone; polkit decides) |
| D-Bus activation | `/usr/share/dbus-1/system-services/io.github.joonhoekim.OpenDeviceHelper1.service` (`SystemdService=`) |
| polkit | `/usr/share/polkit-1/actions/io.github.joonhoekim.opendevicehelper.policy` |
| kernel download | `/usr/libexec/tb323fu/tb323fu-kernel-fetch` + `/usr/lib/systemd/system/tb323fu-kernel-fetch.service` (oneshot, started by the daemon only: `DynamicUser=yes`, `CacheDirectory=tb323fu-kernel`, network, nothing else writable, optional credential `tb323fu-github-token`; needs `curl`) |
| helper self-update | no files of its own: the daemon binary itself (`--apply`) in a transient unit `tb323fu-helper-update.service`; state in `/var/lib/tb323fu/helper/` |
| kernel signing keys | none shipped (signatures are optional: `require_signature`); administrators who want them put keys in `/etc/tb323fu/keys/kernel-*.pub` or `public_keys`. `install.sh` removes the key of the earlier signed-index channel |
| compressed swap | `tb323fu-zram.service` + `/usr/libexec/tb323fu/tb323fu-zram`: **platform** package, enabled by default (`WantedBy=swap.target`); the helper only writes `/etc/tb323fu/zram.conf` and restarts it |
| kernel confirmation | `tb323fu-kernel-confirm.service` + `/usr/libexec/tb323fu/tb323fu-kernel-confirm`: **platform** package (layer 1), enabled with the other platform units |
| GNOME extension | `/usr/share/gnome-shell/extensions/tb323fu@joonhoekim.github.io/` (separate package) |
| settings app | `/usr/bin/tb323fu-settings`, `.desktop`, icons (separate package) |

Recipes (in `packaging/`, plus `flake.nix` at the repository root). All build the same four packages:
`tb323fu-platform`, `tb323fu-helper`, `tb323fu-settings`, `tb323fu-helper-gnome`.

| Distribution | Recipe | Status |
|---|---|---|
| Debian / Ubuntu | `packaging/debian/build-debs.sh` (dpkg-deb, no debhelper) | verified: built on the device (arm64), contents checked with `dpkg -c`; `tb323fu-settings` install and remove tried |
| Arch Linux ARM | `packaging/arch/PKGBUILD` (split package, aarch64) | untested: syntax only (`bash -n`) |
| NixOS | `flake.nix` → `packaging/nix/*.nix`, modules `nixosModules.default` (`services.tb323fu`) and `nixosModules.rootfs` ([`rootfs/nixos`](../rootfs/nixos/configuration.nix)) | verified: built on the device (aarch64) and installed with `rootfs/nixos/build-rootfs.sh`; boots to GNOME with the helper ([distros.md](distros.md)) |

- Debian: `tb323fu-helper` `Depends: dbus, polkitd | policykit-1, systemd`, enables `tb323fu-helperd`; `tb323fu-platform` `Depends: systemd, udev, bluez`, `Recommends:` PipeWire/WirePlumber, alsa-ucm-conf, iio-sensor-proxy, hexagonrpcd, qrtr-tools, rmtfs, tqftpserv; its postinst enables the platform units and masks `bootmac-bluetooth` when present. `/etc/tb323fu/*` are conffiles.
- Arch installs the daemon under `/usr/lib/tb323fu/` (Arch has no libexec); `.install` files print the enable commands.
- NixOS: see the module details below.
- `back-to-android` ships with the platform package (`/usr/sbin`); the helper looks it up in `PATH` first, then `/usr/local/sbin`, `/usr/sbin`, `/usr/libexec/tb323fu`. The Android image hash is read from `/etc/tb323fu/android-boot.sha256` (older `/etc/android-boot.sha256` as fallback).

<details>
<summary>NixOS module details</summary>

The platform package is installed with `PREFIX=$out` (install.sh rewrites `/usr/libexec/tb323fu` to the store path); `/etc/tb323fu` stays mutable and gets its defaults once through `systemd.tmpfiles` `C` rules. The module wires udev rules, systemd units (with `path` for the tools the scripts call), D-Bus, polkit, PipeWire/WirePlumber `configPackages`, a merged `ALSA_CONFIG_UCM2`, `LIBCAMERA_IPA_CONFIG_PATH`, and options `helper.enable`, `settingsApp.enable`, `gnome.enable`, `android.requireAuth` (initial value), `androidBootSha256`.

Also: `hexagonrpcd` (nixpkgs `hexagonrpc`) + iio-sensor-proxy when `sensors.dataDir` points at the device's sensor files; the swh LADSPA plugins through `services.pipewire.extraLadspaPackages`; `hardware.wirelessRegulatoryDatabase`; `RuntimeWatchdogSec=30s`; the units' `/lib/firmware` is rewritten to `/run/current-system/firmware` (NixOS links `hardware.firmware` there; the module keeps it uncompressed). The `[Install]` sections of packaged units do not apply on NixOS, so the module sets the same `wantedBy` targets.

</details>

## Tests

<details>
<summary>Test plan (without and on the device)</summary>

Without the device
- Daemon against a **fake sysfs root** (`TB323FU_SYSFS_ROOT=/tmp/fake-sys`) on a private D-Bus (`dbus-run-session`): property reads, setters, capability detection, persistence, polkit denials (with a test policy). CLI golden-output tests against the same fake daemon.
- `tests/kernel-update-test.sh` (needs python3, curl, minisign): releases from `tools/kernel-release.py assets` on a local GitHub API stand-in (`kernel-release.py fake-api`, `file://`). Covers channel choice, check, notes, download, install (repacked `boot_a` byte-identical to `tools/boot-repack-kernel.py`, `linux-good.img` = the old image), trial, Keep, rollback; refusals (low battery, wrong `boot_b`, install before download, changed staged file, reinstall of the running release, damaged download); `require_signature` cases; kernel from a file (inspect, `--trial`, `--keep`, no terminal without `--yes`, refused while on trial). `cargo test` covers the same on files; `cargo test -- --ignored real_images` compares the Rust repack with the Python tool on real images.
- `tests/helper-update-test.sh` (needs python3, curl): `install.sh` into a fake root, releases from `tools/helper-release.py` on the stand-in. Covers check, notes, refusal when dpkg or NixOS owns the helper, damaged download, `require_signature`, `min_platform`, update (files = release tree, `prev/` kept), rollback, a release whose daemon does not start (automatic rollback, reason in `LastUpdate`), a path outside the allow-list. `cargo test` covers the manifest, tar reader, unpack checks, snapshot/apply/back and owner detection.
- `tests/mesa-update-test.sh` (needs python3, curl): releases from `tools/mesa-release.py` on the stand-in. Covers check, `on` refused before install, damaged tarball, download, install, on (`env.conf`, trial), the `tb323fu-mesa` wrapper, the boot count (third start without Keep switches off and records `Failed`), Keep, a second version on trial, rollback, off.
- Also offline: the initramfs side (`kernel/initramfs/test-root-selection.sh`), the confirm script (`userspace/platform/test-kernel-confirm.sh`) and Android's choice (`android/test-state-root.sh`).

On the device
- Charge limit: set 60/80 with a PD charger attached; threshold file and charging state follow; the UPower hint is absent (GNOME's switch gone).
- Android switch: `Available` false without the hash file; with it, the auth prompt appears and the switch works (then return to Linux).
- Refresh: policy auto/off/manual, idle presets; the live rate follows (frame-counter check), overview stays open, suspend while stretched resumes at the mode's full rate; in the 164 Hz mode `MaxHz` is 164 and `SetRate` takes up to 164.
- Torch and LED ring: visual check (a person watches).
- Daemon killed or not installed: boot, audio, sensors and the emergency chord still work.

</details>

[^full-by]: Estimated charging time = charge left (from `charge_counter` and the capacity) at 85 % of the measured input power (at least 10 W) into 4.2 V, times 1.5 for the constant-voltage taper, plus 30 min.
[^cpu-floor]: Higher floors only add heat.
[^thermal-perf]: Linux has no temperature-based charge-current reduction like Android's thermal-engine, hence the Bypass switch. Android's game mode uses 58–62 °C; the code ceiling is 58 °C.
[^wifi-ps]: NetworkManager cannot change power saving on an active connection (Reapply refuses `802-11-wireless.powersave`), and a connection with `default`/`ignore` leaves the interface alone. So the helper uses `iw` and skips interfaces whose applied connection sets `powersave` itself (2/3: NetworkManager's).
[^bypass]: `BypassMethod` `current`: limit 100 % at the full charge current until the charger charges (the firmware does not start under a small limit; if it does not within 6 s the limit crosses the charge), then charge current `BypassCurrentMa`. The charger keeps its full PD contract, and a charger that stops (replugged) is started again after 20 s. `limit`: limit = capacity; the firmware drops PD to 5 V.
[^battery-state]: The firmware keeps saying "Charging" while the battery is held. With Bypass on, external power and \|current\| ≤ 300 mA, the state is `bypass` (185 mA was seen in bypass on a 65 W charger); while the current is larger (just switched on, or the charger cannot carry the load) it stays `charging` / `discharging`. Without the switch, "Charging" at or above the limit with \|current\| ≤ 300 mA is `bypass` too, shown as "Held at Limit".
[^repack]: The helper's Rust port gives byte-identical images; checked on a real stock image with a raw and a gzip kernel.
