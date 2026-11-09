# Evidra on the Raspberry Pi 5

The Pi 5 hosts the Rust assurance software, simulation experiments, Python
orchestration and dashboard. The Pi 4 is reserved for sensor acquisition and
actuator commands with independent feedback. No GPIO libraries belong on the Pi 5.

This delivery packages the existing **simulation software** for a headless Pi 5.
It does not yet connect physical observations or commands to the Rust runtime.
The health endpoint reports `mode: simulation` and `gateway_connected: false`.
Do not interpret experiment playback as a live device connection.

## Prepare and verify on the Pi 5

Use a 64-bit Linux OS, Python 3.9 or later and a Rust toolchain supporting edition
2024. Copy or clone this repository to the Pi. Build on the Pi itself; the Mac
binaries in `target/` cannot run on Linux ARM64. The Rust project has no external
crate dependencies. Internet access is needed only to provision tools/source if
not supplied locally; execution and building this repository can run offline.

From the repository root:

```sh
cargo test --offline
cargo fmt --check
cargo clippy --offline --all-targets -- -D warnings
cargo build --offline --bins
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo build --offline --release --bins
python3 scripts/pi5_preflight.py --require-pi5 --output-dir output --report output/pi5-preflight.json
python3 scripts/run_web_demo.py --profile release --skip-build
```

Use a new report filename on each run. Preflight checks the board/architecture,
Python, executable presence, assets and output writes; it records OS/kernel,
source revision/dirty state, binary hashes and a temperature snapshot. Executable
presence does not prove architecture compatibility: the actual server startup and
experiment smoke test below are required. Record power supply, cooling,
throttling and repeated-run thermal measurements alongside benchmark results.

## Install the boot service

The supplied service has a fixed installation path `/opt/evidra`, a dedicated
unprivileged `evidra` account and writable results at `/var/lib/evidra`. Install
only after the validation above. These commands are run on the Pi with sudo;
this repository does not modify the development laptop's services.

```sh
sudo useradd --system --user-group --home-dir /var/lib/evidra --shell /usr/sbin/nologin evidra
sudo install -d /opt/evidra
sudo cp -R scripts web deploy /opt/evidra/
sudo install -d /opt/evidra/target/release
sudo install -m 0755 target/release/web_bridge target/release/model_demo /opt/evidra/target/release/
sudo chown -R root:root /opt/evidra
sudo chmod -R a+rX /opt/evidra
sudo install -m 0644 deploy/pi5/evidra.service /etc/systemd/system/evidra.service
sudo systemctl daemon-reload
sudo systemctl enable --now evidra.service
sudo systemctl status evidra.service
```

Skip account creation if the dedicated account already exists. The copy includes
prebuilt binaries; no compiler or build writes are needed at service startup.
For an update, stop the service, copy the verified new software, then start it.
Keep source and benchmark archives outside `/opt/evidra` for reproducibility.
The service restarts after failure; experiment playback state is reset after a
restart. Completed experiment files persist. Monitor free space and archive old
results before the microSD fills; automatic deletion is deliberately absent.

## Open and smoke-test

Keep the dashboard bound to loopback. From the administration laptop, replace
`PI5_ADDRESS` and `YOUR_USER` with the Pi's wired address and SSH account:

```sh
ssh -N -L 8765:127.0.0.1:8765 YOUR_USER@PI5_ADDRESS
```

Open `http://127.0.0.1:8765/model` on the laptop. Forward the same port so the
Host/Origin checks match. For a different local port, configure the server with
`--port` and forward that same port. There is no unauthenticated LAN listener.

On the Pi:

```sh
curl --fail http://127.0.0.1:8765/health/ready
curl --fail -H 'Content-Type: application/json' -d '{"scenario":"short-gap","policy":"B6","seed":42}' http://127.0.0.1:8765/api/model/run
sudo journalctl -u evidra.service -n 50 --no-pager
```

The first call checks whether the bridge child remains alive; it does not prove
sensor health or physical-control readiness. The second runs a real Rust
simulation and saves it under `/var/lib/evidra/model-dashboard/`. A failed child
returns HTTP 503; restarting the service or resetting the original demo recovers
it. Dashboard request validation and the two-experiment concurrency limit remain
active. Optional Ollama AI planning requires a separately installed local model;
scripted/model experiments require no AI service.

Run the existing frozen benchmark campaign manually from a writable source
checkout, using `scripts/run_refinement_benchmarks.py` as documented in the
software refinement delivery. It uses debug binaries and writes into that
checkout's output directory. The service uses release binaries: record build
profile and compare only matched profiles when measuring processing costs.
Benchmarks are not launched automatically at boot.

## Remaining work for live Pi 5 operation

The following is required before calling the complete two-board software ready:

1. Specify and implement the Pi 4/Pi 5 wire protocol and authenticated transport:
   gateway identity/session, per-channel versions, sample age/expiry, command IDs,
   deadlines, duplicate handling, acknowledgements and measured outcomes.
2. Add a Rust live adapter using monotonic elapsed time. It must feed observations
   into the assurance core and expire them without waiting for another message.
   Gateway timestamps require a measured clock uncertainty or a conservative age
   budget; receiving an old packet must not renew old evidence.
3. Dispatch only runtime-authorized commands. Carry correlation IDs through
   feedback; an accepted command is not proof of physical completion. Reconnects
   must not replay stale commands or reinterpret old results as new outcomes.
4. Test gateway loss, restart, delayed/duplicate/reordered messages, absent outcome
   feedback and recovery with a gateway emulator before connecting Pi 4 GPIO.
5. Present live evidence, connection status and outcomes separately from recorded
   simulations, then validate the actual wired Pi setup and independent scoring.

Physical sensor thresholds, actuator contracts and recovery behavior must match
the selected hardware and measured plant; the simulated pressure model cannot be
substituted for measured physical pressure. The existing model and simulation
checks remain useful software evidence, not a completed hardware validation.
